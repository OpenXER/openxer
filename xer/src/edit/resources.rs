//! Deleting and merging resources of the resource dictionary (`RSRC`). The
//! Resources view's Delete and Del. / Merge, the command line and the MCP
//! server all call these functions.
//!
//! A merge moves everything that names a source resource to the target and
//! then removes the sources:
//!
//! - assignments (`TASKRSRC`) in every project of the file, and the primary
//!   resource of activities (`TASK.rsrc_id`). Where the target and a source
//!   (or two sources) are assigned to the same activity, the assignments are
//!   combined into one: the target's own row is kept (else the first in file
//!   order), budgeted, actual, remaining and this-period units and costs are
//!   summed, units per time are summed, the start dates take the earliest
//!   and the finish dates the latest (the actual finish stays blank while
//!   any combined assignment that has started is unfinished). The kept row's
//!   role, cost account, rate type, price per unit, curve and lags stay.
//!   Financial period rows, assignment codes and assignment user defined
//!   field values of a combined row move to the kept row: period values are
//!   summed per period, and a code or field value the kept row already has
//!   wins.
//! - rates (`RSRCRATE`): a source's rate row moves when the target has no
//!   row with the same effective date and shift; otherwise the target's row
//!   wins.
//! - role links (`RSRCROLE`), resource codes (`RSRCRCAT`) and resource user
//!   defined field values: they move unless the target already has that
//!   role, a value of that code type, or a value of that field.
//! - the resource calendar: the target keeps its own; a target without one
//!   takes the first source calendar.
//! - child resources move under the target; a target that sat under a
//!   source takes that source's place in the tree.
//! - any other table with an `rsrc_id` column (leveling lists, feedback,
//!   tables this program does not know) is re-pointed at the target, and a
//!   leveling list keeps the target once.
//!
//! Assignments keep their stored prices and costs; nothing is repriced.
use super::records::{self, float, num};
use crate::{Document, ProjectData};
use std::collections::{BTreeMap, BTreeSet};

type R<T> = Result<T, String>;

/// Tables cleaned of a deleted resource's rows.
const OWNED: [&str; 5] = [
    "RSRCRATE",
    "RSRCROLE",
    "RSRCRCAT",
    "RSRCLEVELLIST",
    "TASKFDBK",
];

/// Tables a merge handles itself; every other `rsrc_id` column is re-pointed.
const HANDLED: [&str; 6] = [
    "RSRC", "TASKRSRC", "RSRCRATE", "RSRCROLE", "RSRCRCAT", "UDFVALUE",
];

/// Assignment columns summed when two assignments are combined.
const SUMMED: [&str; 12] = [
    "target_qty",
    "remain_qty",
    "act_reg_qty",
    "act_ot_qty",
    "act_this_per_qty",
    "target_cost",
    "remain_cost",
    "act_reg_cost",
    "act_ot_cost",
    "act_this_per_cost",
    "target_qty_per_hr",
    "remain_qty_per_hr",
];
/// Start dates take the earliest of the combined assignments.
const EARLIEST: [&str; 4] = [
    "act_start_date",
    "target_start_date",
    "restart_date",
    "rem_late_start_date",
];
/// Finish dates take the latest.
const LATEST: [&str; 3] = ["target_end_date", "reend_date", "rem_late_end_date"];

/// The RSRC row of a resource.
pub fn row(doc: &Document, rsrc: i64) -> Option<usize> {
    let t = doc.table("RSRC")?;
    t.rows
        .iter()
        .position(|r| num(t.value(r, "rsrc_id")) == rsrc)
}

fn rsrc_value(doc: &Document, rsrc: i64, column: &str) -> String {
    row(doc, rsrc)
        .map(|r| records::get(doc, "RSRC", r, column))
        .unwrap_or_default()
}

/// A resource's Resource ID (short name).
pub fn code(doc: &Document, rsrc: i64) -> String {
    rsrc_value(doc, rsrc, "rsrc_short_name")
}

/// A resource's id by Resource ID (short name): exact, else ignoring case.
pub fn find(doc: &Document, short_name: &str) -> R<i64> {
    let t = doc.table("RSRC").ok_or("The file has no resources.")?;
    let ids = |exact: bool| -> Vec<i64> {
        t.rows
            .iter()
            .filter(|r| {
                let s = t.value(r, "rsrc_short_name");
                if exact {
                    s == short_name
                } else {
                    s.eq_ignore_ascii_case(short_name.trim())
                }
            })
            .map(|r| num(t.value(r, "rsrc_id")))
            .collect()
    };
    let mut found = ids(true);
    if found.is_empty() {
        found = ids(false);
    }
    match found.as_slice() {
        [id] => Ok(*id),
        [] => Err(format!("No resource with Resource ID {short_name:?}.")),
        _ => Err(format!("Resource ID {short_name:?} is not unique.")),
    }
}

/// Assignments of the resource in every project of the file.
pub fn assignment_count(doc: &Document, rsrc: i64) -> usize {
    doc.table("TASKRSRC").map_or(0, |t| {
        t.rows
            .iter()
            .filter(|r| num(t.value(r, "rsrc_id")) == rsrc)
            .count()
    })
}

fn children(doc: &Document, rsrc: i64) -> Vec<i64> {
    doc.table("RSRC").map_or(vec![], |t| {
        t.rows
            .iter()
            .filter(|r| num(t.value(r, "parent_rsrc_id")) == rsrc)
            .map(|r| num(t.value(r, "rsrc_id")))
            .collect()
    })
}

/// User defined field types of a table (`UDFTYPE.table_name`).
fn udf_types(doc: &Document, table: &str) -> BTreeSet<String> {
    doc.table("UDFTYPE").map_or(BTreeSet::new(), |t| {
        t.rows
            .iter()
            .filter(|r| t.value(r, "table_name").eq_ignore_ascii_case(table))
            .map(|r| t.value(r, "udf_type_id").to_string())
            .collect()
    })
}

fn retain(doc: &mut Document, table: &str, keep: impl Fn(&crate::Table, &[String]) -> bool) {
    if let Some(i) = doc
        .tables
        .iter()
        .position(|t| t.name.eq_ignore_ascii_case(table))
    {
        let t = &doc.tables[i];
        let flags: Vec<bool> = t.rows.iter().map(|r| keep(t, r)).collect();
        let mut it = flags.into_iter();
        doc.tables[i].rows.retain(|_| it.next().unwrap_or(true));
    }
}

/// Delete a resource with its rates, roles, codes, leveling list entries
/// and user defined field values. A resource that is assigned or has child
/// resources is refused with the reason.
pub fn delete(doc: &mut Document, rsrc: i64) -> R<()> {
    let at = row(doc, rsrc).ok_or("The resource no longer exists")?;
    let code = records::get(doc, "RSRC", at, "rsrc_short_name");
    let count = assignment_count(doc, rsrc);
    if count > 0 {
        return Err(format!(
            "Resource {code} cannot be deleted: it has {count} activity assignment{}. Remove or replace them first, or merge it into another resource (Del. / Merge).",
            if count == 1 { "" } else { "s" }
        ));
    }
    if !children(doc, rsrc).is_empty() {
        return Err(format!(
            "Resource {code} cannot be deleted: it has child resources. Delete or move them first."
        ));
    }
    records::remove_row(doc, "RSRC", at);
    for table in OWNED {
        retain(doc, table, |t, r| num(t.value(r, "rsrc_id")) != rsrc);
    }
    let types = udf_types(doc, "RSRC");
    let id = rsrc.to_string();
    retain(doc, "UDFVALUE", |t, r| {
        !(types.contains(t.value(r, "udf_type_id")) && t.value(r, "fk_id") == id)
    });
    // A primary resource left on an activity row.
    clear_primary(doc, rsrc);
    Ok(())
}

fn clear_primary(doc: &mut Document, rsrc: i64) {
    if let Some(t) = doc
        .tables
        .iter_mut()
        .find(|t| t.name.eq_ignore_ascii_case("TASK"))
    {
        if let Some(f) = t.field("rsrc_id") {
            for r in &mut t.rows {
                if r.get(f).is_some_and(|v| num(v) == rsrc) {
                    r[f].clear();
                }
            }
        }
    }
}

/// Delete a resource by Resource ID, on a project.
pub fn delete_by_code(p: &mut ProjectData, short_name: &str) -> R<()> {
    let id = find(&p.document, short_name)?;
    delete(&mut p.document, id)?;
    p.refresh_resource_inputs();
    Ok(())
}

/// One activity where assignments were combined.
#[derive(Clone, Debug, PartialEq)]
pub struct Combined {
    pub proj_id: i64,
    pub task_id: i64,
    /// Activity ID.
    pub activity: String,
    /// How many assignments became one.
    pub rows: usize,
}

/// What a merge did (or, run on a copy, would do).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MergeReport {
    pub into: i64,
    pub into_code: String,
    /// Source resources: (id, Resource ID).
    pub from: Vec<(i64, String)>,
    /// Assignments moved to the target, per source Resource ID.
    pub assignments: Vec<(String, usize)>,
    /// Activities where assignments were combined.
    pub combined: Vec<Combined>,
    /// Activities whose primary resource became the target.
    pub primary: usize,
    pub rates_moved: usize,
    pub rates_kept: usize,
    pub roles_moved: usize,
    pub roles_kept: usize,
    pub codes_moved: usize,
    pub codes_kept: usize,
    pub udfs_moved: usize,
    pub udfs_kept: usize,
    /// Child resources moved under the target.
    pub children: usize,
    /// The target took a source's calendar (it had none).
    pub calendar: Option<String>,
    /// Rows of other tables re-pointed at the target: (table, rows).
    pub other: Vec<(String, usize)>,
}

impl MergeReport {
    /// Plain lines for a preview: what moves where and what was chosen.
    pub fn lines(&self) -> Vec<String> {
        let sources: Vec<&str> = self.from.iter().map(|(_, c)| c.as_str()).collect();
        let mut out = vec![format!(
            "Merge {} into {}; {} removed afterwards.",
            sources.join(", "),
            self.into_code,
            if sources.len() == 1 {
                "it is"
            } else {
                "they are"
            }
        )];
        let plural = |n: usize, one: &str, many: &str| {
            if n == 1 {
                format!("{n} {one}")
            } else {
                format!("{n} {many}")
            }
        };
        for (code, n) in &self.assignments {
            out.push(format!(
                "{} of {code} move to {}.",
                plural(*n, "assignment", "assignments"),
                self.into_code
            ));
        }
        if !self.combined.is_empty() {
            let list: Vec<&str> = self
                .combined
                .iter()
                .take(8)
                .map(|c| c.activity.as_str())
                .collect();
            let more = if self.combined.len() > 8 {
                format!(" and {} more", self.combined.len() - 8)
            } else {
                String::new()
            };
            out.push(format!(
                "{} already had both: their assignments become one ({}{more}). Units and costs are summed, actuals kept, dates span both; the kept row's role, rate and curve stay.",
                plural(self.combined.len(), "activity", "activities"),
                list.join(", ")
            ));
        }
        if self.primary > 0 {
            out.push(format!(
                "{} take {} as primary resource.",
                plural(self.primary, "activity", "activities"),
                self.into_code
            ));
        }
        let pair = |moved: usize, kept: usize, what: &str, kept_why: &str| {
            (moved + kept > 0).then(|| {
                let verb = if moved == 1 { "moves" } else { "move" };
                let s = format!("{} {verb}", plural(moved, what, &format!("{what}s")));
                if kept > 0 {
                    format!("{s}; {kept} dropped ({kept_why}).")
                } else {
                    format!("{s}.")
                }
            })
        };
        out.extend(pair(
            self.rates_moved,
            self.rates_kept,
            "rate row",
            "the target has a rate from that date",
        ));
        out.extend(pair(
            self.roles_moved,
            self.roles_kept,
            "role link",
            "the target already has that role",
        ));
        out.extend(pair(
            self.codes_moved,
            self.codes_kept,
            "resource code value",
            "the target has a value of that code",
        ));
        out.extend(pair(
            self.udfs_moved,
            self.udfs_kept,
            "user defined field value",
            "the target has a value of that field",
        ));
        if self.children > 0 {
            out.push(format!(
                "{} move under {}.",
                plural(self.children, "child resource", "child resources"),
                self.into_code
            ));
        }
        if let Some(c) = &self.calendar {
            out.push(format!(
                "{} had no calendar and takes calendar {c}.",
                self.into_code
            ));
        }
        for (table, n) in &self.other {
            out.push(format!(
                "{} of {table} now name {}.",
                plural(*n, "row", "rows"),
                self.into_code
            ));
        }
        out
    }
}

/// Check a merge: the sources and the target exist, differ, and are of one
/// resource type.
fn check(doc: &Document, from: &[i64], into: i64) -> R<()> {
    if from.is_empty() {
        return Err("Name at least one resource to merge.".into());
    }
    let into_code = code(doc, into);
    if row(doc, into).is_none() {
        return Err("The target resource no longer exists.".into());
    }
    let mut seen = BTreeSet::new();
    let kind = |id: i64| match rsrc_value(doc, id, "rsrc_type").as_str() {
        "" => "RT_Labor".to_string(),
        s => s.to_string(),
    };
    for &id in from {
        if row(doc, id).is_none() {
            return Err(format!("Resource {id} no longer exists."));
        }
        if id == into {
            return Err(format!(
                "{into_code} cannot be merged into itself; pick another target."
            ));
        }
        if !seen.insert(id) {
            return Err(format!("{} is listed twice.", code(doc, id)));
        }
        if kind(id) != kind(into) {
            return Err(format!(
                "{} and {into_code} are different resource types (labor, nonlabor or material); merge resources of one type.",
                code(doc, id)
            ));
        }
    }
    Ok(())
}

/// Merge the resources `from` into `into` (see the module comment).
pub fn merge(doc: &mut Document, from: &[i64], into: i64) -> R<MergeReport> {
    check(doc, from, into)?;
    let sources: BTreeSet<i64> = from.iter().copied().collect();
    let mut report = MergeReport {
        into,
        into_code: code(doc, into),
        from: from.iter().map(|&id| (id, code(doc, id))).collect(),
        ..Default::default()
    };
    let is_source = |v: &str| sources.contains(&num(v));
    let target = into.to_string();
    // Assignments and primary resources.
    for &id in from {
        let n = assignment_count(doc, id);
        if n > 0 {
            report.assignments.push((code(doc, id), n));
        }
    }
    // The target's own assignment rows, before any source row names it.
    let own: BTreeSet<usize> = doc.table("TASKRSRC").map_or(BTreeSet::new(), |t| {
        t.rows
            .iter()
            .enumerate()
            .filter(|(_, r)| num(t.value(r, "rsrc_id")) == into)
            .map(|(i, _)| i)
            .collect()
    });
    let target_type = rsrc_value(doc, into, "rsrc_type");
    if let Some(t) = doc
        .tables
        .iter_mut()
        .find(|t| t.name.eq_ignore_ascii_case("TASKRSRC"))
    {
        if let Some(f) = t.field("rsrc_id") {
            let kind = t.field("rsrc_type");
            for r in &mut t.rows {
                if r.get(f).is_some_and(|v| is_source(v)) {
                    r[f] = target.clone();
                    if let Some(k) = kind {
                        if !target_type.is_empty() {
                            r[k] = target_type.clone();
                        }
                    }
                }
            }
        }
    }
    if let Some(t) = doc
        .tables
        .iter_mut()
        .find(|t| t.name.eq_ignore_ascii_case("TASK"))
    {
        if let Some(f) = t.field("rsrc_id") {
            for r in &mut t.rows {
                if r.get(f).is_some_and(|v| is_source(v)) {
                    r[f] = target.clone();
                    report.primary += 1;
                }
            }
        }
    }
    combine_assignments(doc, into, &own, &mut report);
    // Rates: a source row moves unless the target has one from that date.
    let rate_key = |t: &crate::Table, r: &[String]| {
        let start = t.value(r, "start_date");
        (
            crate::parse_absolute(start).map_or(start.trim().to_string(), |m| m.to_string()),
            t.value(r, "shift_period_id").to_string(),
        )
    };
    move_rows(
        doc,
        "RSRCRATE",
        &sources,
        into,
        rate_key,
        (&mut report.rates_moved, &mut report.rates_kept),
    );
    move_rows(
        doc,
        "RSRCROLE",
        &sources,
        into,
        |t, r| (t.value(r, "role_id").to_string(), String::new()),
        (&mut report.roles_moved, &mut report.roles_kept),
    );
    move_rows(
        doc,
        "RSRCRCAT",
        &sources,
        into,
        |t, r| (t.value(r, "rsrc_catg_type_id").to_string(), String::new()),
        (&mut report.codes_moved, &mut report.codes_kept),
    );
    // Resource user defined field values (UDFVALUE keyed by fk_id).
    let types = udf_types(doc, "RSRC");
    if let Some(t) = doc
        .tables
        .iter_mut()
        .find(|t| t.name.eq_ignore_ascii_case("UDFVALUE"))
    {
        if let (Some(kind), Some(key)) = (t.field("udf_type_id"), t.field("fk_id")) {
            let mut taken: BTreeSet<String> = t
                .rows
                .iter()
                .filter(|r| types.contains(&r[kind]) && r.get(key).is_some_and(|k| *k == target))
                .map(|r| r[kind].clone())
                .collect();
            let mut drop = vec![];
            for (i, r) in t.rows.iter_mut().enumerate() {
                if !types.contains(&r[kind]) || !r.get(key).is_some_and(|k| is_source(k)) {
                    continue;
                }
                if taken.insert(r[kind].clone()) {
                    r[key] = target.clone();
                    report.udfs_moved += 1;
                } else {
                    drop.push(i);
                    report.udfs_kept += 1;
                }
            }
            for i in drop.into_iter().rev() {
                t.rows.remove(i);
            }
        }
    }
    // Calendar, children and the target's place in the tree.
    if rsrc_value(doc, into, "clndr_id").trim().is_empty() {
        if let Some(cal) = from
            .iter()
            .map(|&id| rsrc_value(doc, id, "clndr_id"))
            .find(|c| !c.trim().is_empty())
        {
            let at = row(doc, into).unwrap_or(0);
            records::set(doc, "RSRC", at, "clndr_id", cal.clone());
            let name = doc
                .table("CALENDAR")
                .and_then(|t| {
                    t.rows
                        .iter()
                        .find(|r| t.value(r, "clndr_id") == cal)
                        .map(|r| t.value(r, "clndr_name").to_string())
                })
                .unwrap_or(cal);
            report.calendar = Some(name);
        }
    }
    // A target under a source takes the place of its nearest ancestor that
    // stays.
    let parent_of = |doc: &Document, id: i64| num(&rsrc_value(doc, id, "parent_rsrc_id"));
    let mut parent = parent_of(doc, into);
    let mut guard = 0;
    while sources.contains(&parent) && guard < 100_000 {
        parent = parent_of(doc, parent);
        guard += 1;
    }
    if let Some(t) = doc
        .tables
        .iter_mut()
        .find(|t| t.name.eq_ignore_ascii_case("RSRC"))
    {
        if let (Some(id_f), Some(parent_f)) = (t.field("rsrc_id"), t.field("parent_rsrc_id")) {
            for r in &mut t.rows {
                let id = num(&r[id_f]);
                if id == into {
                    if sources.contains(&num(&r[parent_f])) {
                        r[parent_f] = if parent == 0 {
                            String::new()
                        } else {
                            parent.to_string()
                        };
                    }
                } else if !sources.contains(&id) && is_source(&r[parent_f]) {
                    r[parent_f] = target.clone();
                    report.children += 1;
                }
            }
        }
    }
    // Every other rsrc_id column.
    for t in doc.tables.iter_mut().filter(|t| {
        !HANDLED.iter().any(|h| t.name.eq_ignore_ascii_case(h))
            && !t.name.eq_ignore_ascii_case("TASK")
    }) {
        let Some(f) = t.field("rsrc_id") else {
            continue;
        };
        let mut n = 0;
        for r in &mut t.rows {
            if r.get(f).is_some_and(|v| is_source(v)) {
                r[f] = target.clone();
                n += 1;
            }
        }
        if n > 0 {
            report.other.push((t.name.clone(), n));
        }
    }
    // A leveling list names the target once.
    if let Some(t) = doc
        .tables
        .iter_mut()
        .find(|t| t.name.eq_ignore_ascii_case("RSRCLEVELLIST"))
    {
        let (opt, rs) = (t.field("schedoptions_id"), t.field("rsrc_id"));
        if let Some(rs) = rs {
            let mut seen = BTreeSet::new();
            t.rows.retain(|r| {
                let key = (opt.map(|o| r[o].clone()).unwrap_or_default(), r[rs].clone());
                num(&key.1) != into || seen.insert(key)
            });
        }
    }
    // Finally the sources themselves.
    retain(doc, "RSRC", |t, r| {
        !sources.contains(&num(t.value(r, "rsrc_id")))
    });
    Ok(report)
}

/// Move the sources' rows of `table` to the target unless the target has
/// a row with the same key; those are dropped. Counts (moved, kept).
fn move_rows(
    doc: &mut Document,
    table: &str,
    sources: &BTreeSet<i64>,
    into: i64,
    key: impl Fn(&crate::Table, &[String]) -> (String, String),
    counts: (&mut usize, &mut usize),
) {
    let Some(i) = doc
        .tables
        .iter()
        .position(|t| t.name.eq_ignore_ascii_case(table))
    else {
        return;
    };
    let t = &doc.tables[i];
    let Some(f) = t.field("rsrc_id") else {
        return;
    };
    let mut taken: BTreeSet<(String, String)> = t
        .rows
        .iter()
        .filter(|r| num(&r[f]) == into)
        .map(|r| key(t, r))
        .collect();
    // Decide per row first, then write.
    let decisions: Vec<Option<bool>> = t
        .rows
        .iter()
        .map(|r| {
            sources
                .contains(&num(&r[f]))
                .then(|| taken.insert(key(t, r)))
        })
        .collect();
    let t = &mut doc.tables[i];
    let mut it = decisions.iter();
    let target = into.to_string();
    let (moved, kept) = counts;
    t.rows.retain_mut(|r| match it.next().copied().flatten() {
        Some(true) => {
            r[f] = target.clone();
            *moved += 1;
            true
        }
        Some(false) => {
            *kept += 1;
            false
        }
        None => true,
    });
}

fn earlier(a: &str, b: &str) -> String {
    match (a.trim().is_empty(), b.trim().is_empty()) {
        (true, _) => b.to_string(),
        (_, true) => a.to_string(),
        _ if crate::parse_absolute(b) < crate::parse_absolute(a) => b.to_string(),
        _ => a.to_string(),
    }
}

fn later(a: &str, b: &str) -> String {
    match (a.trim().is_empty(), b.trim().is_empty()) {
        (true, _) => b.to_string(),
        (_, true) => a.to_string(),
        _ if crate::parse_absolute(b) > crate::parse_absolute(a) => b.to_string(),
        _ => a.to_string(),
    }
}

/// Combine the target's assignments that now share an activity.
fn combine_assignments(
    doc: &mut Document,
    into: i64,
    own: &BTreeSet<usize>,
    report: &mut MergeReport,
) {
    let Some(ti) = doc
        .tables
        .iter()
        .position(|t| t.name.eq_ignore_ascii_case("TASKRSRC"))
    else {
        return;
    };
    // Groups of the target's rows per (project, activity), in file order.
    let t = &doc.tables[ti];
    let mut groups: BTreeMap<(i64, i64), Vec<usize>> = BTreeMap::new();
    for (i, r) in t.rows.iter().enumerate() {
        if num(t.value(r, "rsrc_id")) == into {
            groups
                .entry((num(t.value(r, "proj_id")), num(t.value(r, "task_id"))))
                .or_default()
                .push(i);
        }
    }
    groups.retain(|_, rows| rows.len() > 1);
    if groups.is_empty() {
        return;
    }
    let mut removed: BTreeSet<usize> = BTreeSet::new();
    // taskrsrc_id of removed rows -> kept row's id.
    let mut moved_ids: BTreeMap<String, String> = BTreeMap::new();
    let codes = task_codes(doc);
    for ((proj, task), rows) in &groups {
        let t = &doc.tables[ti];
        // The target's own row is kept, else the first in file order.
        let keep = rows
            .iter()
            .copied()
            .find(|i| own.contains(i))
            .unwrap_or(rows[0]);
        let others: Vec<usize> = rows.iter().copied().filter(|&i| i != keep).collect();
        let started_open = rows.iter().any(|&i| {
            let r = &t.rows[i];
            !t.value(r, "act_start_date").trim().is_empty()
                && t.value(r, "act_end_date").trim().is_empty()
        });
        let mut sets: Vec<(String, String)> = vec![];
        for column in SUMMED {
            if t.field(column).is_none() {
                continue;
            }
            let total: f64 = rows
                .iter()
                .map(|&i| float(t.value(&t.rows[i], column)))
                .sum();
            sets.push((column.into(), records::format_number(total)));
        }
        for column in EARLIEST {
            if t.field(column).is_some() {
                let v = rows.iter().fold(String::new(), |acc, &i| {
                    earlier(&acc, t.value(&t.rows[i], column))
                });
                sets.push((column.into(), v));
            }
        }
        for column in LATEST {
            if t.field(column).is_some() {
                let v = rows.iter().fold(String::new(), |acc, &i| {
                    later(&acc, t.value(&t.rows[i], column))
                });
                sets.push((column.into(), v));
            }
        }
        if t.field("act_end_date").is_some() {
            let v = if started_open {
                String::new()
            } else {
                rows.iter().fold(String::new(), |acc, &i| {
                    later(&acc, t.value(&t.rows[i], "act_end_date"))
                })
            };
            sets.push(("act_end_date".into(), v));
        }
        let kept_id = t.value(&t.rows[keep], "taskrsrc_id").to_string();
        for &i in &others {
            moved_ids.insert(
                t.value(&t.rows[i], "taskrsrc_id").to_string(),
                kept_id.clone(),
            );
            removed.insert(i);
        }
        let table = &mut doc.tables[ti];
        for (column, value) in sets {
            table.set(keep, &column, value);
        }
        report.combined.push(Combined {
            proj_id: *proj,
            task_id: *task,
            activity: codes
                .get(&(*proj, *task))
                .cloned()
                .unwrap_or_else(|| task.to_string()),
            rows: rows.len(),
        });
    }
    let mut index = 0;
    doc.tables[ti].rows.retain(|_| {
        let keep = !removed.contains(&index);
        index += 1;
        keep
    });
    // Rows that hang off a removed assignment follow it to the kept one.
    let types = udf_types(doc, "TASKRSRC");
    for t in doc.tables.iter_mut() {
        match t.name.to_ascii_uppercase().as_str() {
            "TRSRCFIN" => {
                let (Some(k), period) = (t.field("taskrsrc_id"), t.field("fin_dates_id")) else {
                    continue;
                };
                let numeric: Vec<usize> = t
                    .fields
                    .iter()
                    .enumerate()
                    .filter(|(_, f)| f.ends_with("_qty") || f.ends_with("_cost"))
                    .map(|(i, _)| i)
                    .collect();
                let mut at: BTreeMap<(String, String), usize> = BTreeMap::new();
                for (i, r) in t.rows.iter().enumerate() {
                    if !moved_ids.contains_key(&r[k]) {
                        at.entry((
                            r[k].clone(),
                            period.map(|p| r[p].clone()).unwrap_or_default(),
                        ))
                        .or_insert(i);
                    }
                }
                let mut drop = vec![];
                for i in 0..t.rows.len() {
                    let Some(kept) = moved_ids.get(&t.rows[i][k]).cloned() else {
                        continue;
                    };
                    let key = (
                        kept.clone(),
                        period.map(|p| t.rows[i][p].clone()).unwrap_or_default(),
                    );
                    match at.get(&key).copied() {
                        Some(j) => {
                            for &c in &numeric {
                                let sum = float(&t.rows[j][c]) + float(&t.rows[i][c]);
                                t.rows[j][c] = records::format_number(sum);
                            }
                            drop.push(i);
                        }
                        None => {
                            t.rows[i][k] = kept;
                            at.insert(key, i);
                        }
                    }
                }
                for i in drop.into_iter().rev() {
                    t.rows.remove(i);
                }
            }
            "ASGNMNTACAT" => {
                let (Some(k), Some(kind)) =
                    (t.field("taskrsrc_id"), t.field("asgnmnt_catg_type_id"))
                else {
                    continue;
                };
                dedupe_moved(t, k, kind, &moved_ids, |_| true);
            }
            "UDFVALUE" => {
                let (Some(k), Some(kind)) = (t.field("fk_id"), t.field("udf_type_id")) else {
                    continue;
                };
                dedupe_moved(t, k, kind, &moved_ids, |r| types.contains(&r[kind]));
            }
            _ => {}
        }
    }
}

/// Re-key rows of removed assignments to the kept one; a kind the kept
/// assignment already has wins.
fn dedupe_moved(
    t: &mut crate::Table,
    key: usize,
    kind: usize,
    moved: &BTreeMap<String, String>,
    applies: impl Fn(&[String]) -> bool,
) {
    let mut have: BTreeSet<(String, String)> = t
        .rows
        .iter()
        .filter(|r| applies(r) && !moved.contains_key(&r[key]))
        .map(|r| (r[key].clone(), r[kind].clone()))
        .collect();
    t.rows.retain_mut(|r| {
        if !applies(r) {
            return true;
        }
        let Some(kept) = moved.get(&r[key]) else {
            return true;
        };
        if have.insert((kept.clone(), r[kind].clone())) {
            r[key] = kept.clone();
            true
        } else {
            false
        }
    });
}

fn task_codes(doc: &Document) -> BTreeMap<(i64, i64), String> {
    doc.table("TASK").map_or(BTreeMap::new(), |t| {
        t.rows
            .iter()
            .map(|r| {
                (
                    (num(t.value(r, "proj_id")), num(t.value(r, "task_id"))),
                    t.value(r, "task_code").to_string(),
                )
            })
            .collect()
    })
}

/// What merging `from` into `into` would do, without changing anything.
pub fn preview(doc: &Document, from: &[i64], into: i64) -> R<MergeReport> {
    let mut copy = doc.clone();
    merge(&mut copy, from, into)
}

/// Merge on a project: the document edit, then the assignments and the
/// resource inputs are re-read so the next schedule sees them.
pub fn merge_project(p: &mut ProjectData, from: &[i64], into: i64) -> R<MergeReport> {
    p.document = p.updated_document();
    let report = merge(&mut p.document, from, into)?;
    p.rebuild_assignments();
    p.refresh_resource_inputs();
    Ok(report)
}

/// Merge by Resource IDs (short names).
pub fn merge_by_code(p: &mut ProjectData, from: &[String], into: &str) -> R<MergeReport> {
    let target = find(&p.document, into)?;
    let sources = from
        .iter()
        .map(|c| find(&p.document, c))
        .collect::<R<Vec<i64>>>()?;
    merge_project(p, &sources, target)
}
