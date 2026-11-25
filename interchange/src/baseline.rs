//! Baseline projects inside XER documents.
//!
//! A baseline is a frozen copy of a project. In a file it is an ordinary
//! PROJECT row (with its WBS, activities and relationships) whose
//! `orig_proj_id` names the project it belongs to. The functions here pull one
//! project out of a document, give a copy fresh ids so it can sit beside its
//! parent, mark it as a baseline, and append it to an export.
use crate::util::{copy_row, num};
use openxer_xer::{Document, Table};
use std::collections::{BTreeMap, HashMap, HashSet};

/// Tables whose rows belong to one project, with their key column. A copied
/// project gets new values in these columns, and every reference to them in
/// the project's own rows follows. Activity code types count only when they
/// belong to the project (a global type has no `proj_id`); their values are
/// copied with them (see [`renumber_project`]).
const KEYS: [(&str, &str); 22] = [
    ("PROJECT", "proj_id"),
    ("PROJWBS", "wbs_id"),
    ("TASK", "task_id"),
    ("TASKPRED", "task_pred_id"),
    ("TASKRSRC", "taskrsrc_id"),
    ("CALENDAR", "clndr_id"),
    ("TASKMEMO", "memo_id"),
    ("WBSMEMO", "wbs_memo_id"),
    ("PROJCOST", "cost_item_id"),
    ("TASKPROC", "proc_id"),
    ("SCHEDOPTIONS", "schedoptions_id"),
    ("ACTVTYPE", "actv_code_type_id"),
    ("DOCUMENT", "doc_id"),
    ("TASKDOC", "taskdoc_id"),
    ("PROJTHRS", "thresh_id"),
    ("PROJISSU", "issue_id"),
    ("PROJRISK", "risk_id"),
    ("RISK", "risk_id"),
    ("BUDGCHNG", "budg_chng_id"),
    ("WBSBUDG", "wbs_budg_id"),
    ("WBSSTEP", "wbs_step_id"),
    ("PROJFUND", "proj_fund_id"),
];

/// The key family a reference column points at.
fn family(column: &str) -> String {
    let c = column.to_ascii_lowercase();
    match c.as_str() {
        "pred_task_id" => "task_id".into(),
        "parent_wbs_id" => "wbs_id".into(),
        "pred_proj_id" => "proj_id".into(),
        _ => c,
    }
}

fn owner(t: &Table, row: &[String]) -> Option<i64> {
    let v = t.value(row, "proj_id").trim();
    (!v.is_empty()).then(|| num(v))
}

/// Value of a PROJECT column for one project ("" when absent).
pub fn project_value(doc: &Document, pid: i64, field: &str) -> String {
    doc.table("PROJECT")
        .and_then(|t| {
            t.rows
                .iter()
                .find(|r| num(t.value(r, "proj_id")) == pid)
                .map(|r| t.value(r, field).to_string())
        })
        .unwrap_or_default()
}

/// Set a PROJECT column for one project. A missing column is only added when
/// the value is not blank, so an unchanged file keeps its exact columns.
pub fn set_project_value(doc: &mut Document, pid: i64, field: &str, value: &str) {
    let Some(t) = doc.tables.iter_mut().find(|t| t.name == "PROJECT") else {
        return;
    };
    let Some(row) = t
        .rows
        .iter()
        .position(|r| num(t.value(r, "proj_id")) == pid)
    else {
        return;
    };
    if t.field(field).is_none() && value.is_empty() {
        return;
    }
    t.set(row, field, value.to_string());
}

/// The project's name: its root WBS node's name, else its short name.
pub fn project_name(doc: &Document, pid: i64) -> String {
    let root = doc.table("PROJWBS").and_then(|t| {
        t.rows
            .iter()
            .find(|r| num(t.value(r, "proj_id")) == pid && t.value(r, "proj_node_flag") == "Y")
            .map(|r| t.value(r, "wbs_name").to_string())
    });
    root.filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| project_value(doc, pid, "proj_short_name"))
}

/// Rename a project: short name on its PROJECT row and root WBS node, long
/// name on the root WBS node.
pub fn set_project_names(doc: &mut Document, pid: i64, short: &str, name: &str) {
    set_project_value(doc, pid, "proj_short_name", short);
    if let Some(t) = doc.tables.iter_mut().find(|t| t.name == "PROJWBS") {
        if let Some(row) = t
            .rows
            .iter()
            .position(|r| num(t.value(r, "proj_id")) == pid && t.value(r, "proj_node_flag") == "Y")
        {
            t.set(row, "wbs_short_name", short.to_string());
            t.set(row, "wbs_name", name.to_string());
        }
    }
}

/// (baseline project id, parent project id) for every project in the file
/// that is a baseline of another project in the same file.
pub fn baselines_in(doc: &Document) -> Vec<(i64, i64)> {
    let ids: HashSet<i64> = doc.projects().into_iter().map(|(id, _)| id).collect();
    let Some(t) = doc.table("PROJECT") else {
        return vec![];
    };
    t.rows
        .iter()
        .filter_map(|r| {
            let id = num(t.value(r, "proj_id"));
            let parent = t.value(r, "orig_proj_id").trim();
            if parent.is_empty() {
                return None;
            }
            let parent = num(parent);
            (parent != id && ids.contains(&parent)).then_some((id, parent))
        })
        .collect()
}

/// The projects a user picks from: every project in the file except the
/// baselines of another project in it, which travel with their parent.
pub fn schedule_projects(doc: &Document) -> Vec<(i64, String)> {
    let baselines: HashSet<i64> = baselines_in(doc).into_iter().map(|(b, _)| b).collect();
    doc.projects()
        .into_iter()
        .filter(|(id, _)| !baselines.contains(id))
        .collect()
}

/// A document holding one project: the header, every row of that project and
/// every row that belongs to no project (calendars, resources, codes and the
/// other shared dictionaries). Rows of other projects are left out.
pub fn extract_project(doc: &Document, pid: i64) -> Document {
    let mut out = doc.clone();
    for t in &mut out.tables {
        if t.field("proj_id").is_none() {
            continue;
        }
        let col = t.field("proj_id").expect("checked");
        t.rows.retain(|r| {
            let v = r.get(col).map_or("", |v| v.trim());
            v.is_empty() || num(v) == pid
        });
    }
    out.warnings.clear();
    out
}

/// Remove every row of project `pid` from `doc`, and the values of the
/// activity code types that belong to it (they carry no project id).
pub fn remove_project_rows(doc: &mut Document, pid: i64) {
    let own_types: HashSet<String> = doc
        .table("ACTVTYPE")
        .map(|t| {
            t.rows
                .iter()
                .filter(|r| owner(t, r) == Some(pid))
                .map(|r| t.value(r, "actv_code_type_id").trim().to_string())
                .collect()
        })
        .unwrap_or_default();
    for t in &mut doc.tables {
        if let Some(col) = t.field("proj_id") {
            t.rows.retain(|r| {
                r.get(col)
                    .is_none_or(|v| v.trim().is_empty() || num(v) != pid)
            });
        } else if t.name.eq_ignore_ascii_case("ACTVCODE") && !own_types.is_empty() {
            if let Some(col) = t.field("actv_code_type_id") {
                t.rows
                    .retain(|r| r.get(col).is_none_or(|v| !own_types.contains(v.trim())));
            }
        }
    }
}

/// Largest numeric value of a column in a table across documents.
fn max_key(docs: &[&Document], table: &str, key: &str) -> i64 {
    docs.iter()
        .filter_map(|d| d.table(table))
        .flat_map(|t| t.rows.iter().map(move |r| num(t.value(r, key))))
        .max()
        .unwrap_or(0)
}

/// Give project `pid` of `doc` new ids for every key in `KEYS`, above every
/// value in `doc` and in `reserved`, and update every reference in the
/// project's own rows. `new_pid` forces the project id. Returns the new id.
pub fn renumber_project(
    doc: &mut Document,
    pid: i64,
    reserved: &[&Document],
    new_pid: Option<i64>,
) -> i64 {
    let mut maps: HashMap<String, HashMap<String, String>> = HashMap::new();
    for (table, key) in KEYS {
        let mut next = {
            let mut docs: Vec<&Document> = reserved.to_vec();
            docs.push(doc);
            max_key(&docs, table, key) + 1
        };
        let Some(t) = doc.table(table) else {
            continue;
        };
        let map = maps.entry(key.to_string()).or_default();
        for r in &t.rows {
            if owner(t, r) != Some(pid) {
                continue;
            }
            let old = t.value(r, key).trim().to_string();
            if old.is_empty() || map.contains_key(&old) {
                continue;
            }
            let id = if key == "proj_id" {
                new_pid.unwrap_or(next)
            } else {
                next
            };
            next = next.max(id) + 1;
            map.insert(old, id.to_string());
        }
    }
    // The values of the project's own code types are copied with them: they
    // carry no project id, so they get new ids here and the project's code
    // assignments follow through the map.
    let types = maps.get("actv_code_type_id").cloned().unwrap_or_default();
    if !types.is_empty() {
        let mut docs: Vec<&Document> = reserved.to_vec();
        docs.push(doc);
        let mut next = max_key(&docs, "ACTVCODE", "actv_code_id") + 1;
        let mut codes: HashMap<String, String> = HashMap::new();
        if let Some(t) = doc.tables.iter_mut().find(|t| t.name == "ACTVCODE") {
            let cols = (
                t.field("actv_code_id"),
                t.field("actv_code_type_id"),
                t.field("parent_actv_code_id"),
            );
            if let (Some(id), Some(kind)) = (cols.0, cols.1) {
                for r in &t.rows {
                    if types.contains_key(r[kind].trim()) {
                        codes.insert(r[id].trim().to_string(), next.to_string());
                        next += 1;
                    }
                }
                for r in &mut t.rows {
                    let Some(new_kind) = types.get(r[kind].trim()).cloned() else {
                        continue;
                    };
                    r[kind] = new_kind;
                    r[id] = codes[r[id].trim()].clone();
                    if let Some(parent) = cols.2 {
                        if let Some(p) = codes.get(r[parent].trim()) {
                            r[parent] = p.clone();
                        }
                    }
                }
            }
        }
        maps.insert("actv_code_id".into(), codes);
    }
    // UDF values point at the record named by their type's table.
    let udf_tables: HashMap<String, String> = doc
        .table("UDFTYPE")
        .map(|t| {
            t.rows
                .iter()
                .map(|r| {
                    (
                        t.value(r, "udf_type_id").trim().to_string(),
                        t.value(r, "table_name").to_string(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let udf_family = |table: &str| -> Option<&'static str> {
        Some(match table {
            "TASK" => "task_id",
            "PROJWBS" => "wbs_id",
            "PROJECT" => "proj_id",
            "TASKRSRC" => "taskrsrc_id",
            "PROJCOST" => "cost_item_id",
            _ => return None,
        })
    };
    for t in &mut doc.tables {
        let Some(proj_col) = t.field("proj_id") else {
            continue;
        };
        let columns: Vec<(usize, String)> = t
            .fields
            .iter()
            .enumerate()
            .map(|(i, f)| (i, family(f)))
            .filter(|(_, f)| maps.contains_key(f))
            .collect();
        let fk = t
            .name
            .eq_ignore_ascii_case("UDFVALUE")
            .then(|| (t.field("fk_id"), t.field("udf_type_id")));
        for row in &mut t.rows {
            let owned = row
                .get(proj_col)
                .is_some_and(|v| !v.trim().is_empty() && num(v) == pid);
            if !owned {
                continue;
            }
            for (i, fam) in &columns {
                if let Some(cell) = row.get_mut(*i) {
                    if let Some(new) = maps[fam].get(cell.trim()) {
                        *cell = new.clone();
                    }
                }
            }
            if let Some((Some(fk), Some(ty))) = fk {
                let table = udf_tables
                    .get(row.get(ty).map_or("", |v| v.trim()))
                    .cloned()
                    .unwrap_or_default();
                if let Some(fam) = udf_family(&table) {
                    if let Some(cell) = row.get_mut(fk) {
                        if let Some(new) = maps.get(fam).and_then(|m| m.get(cell.trim())) {
                            *cell = new.clone();
                        }
                    }
                }
            }
        }
    }
    maps.get("proj_id")
        .and_then(|m| m.get(&pid.to_string()))
        .map_or(pid, |v| num(v))
}

/// Whether appending project `pid` of `src` to `out` would reuse a key that
/// `out` already holds in one of the project-owned tables.
pub fn collides(out: &Document, src: &Document, pid: i64) -> bool {
    KEYS.iter().any(|(table, key)| {
        let (Some(s), Some(o)) = (src.table(table), out.table(table)) else {
            return false;
        };
        let taken: HashSet<&str> = o.rows.iter().map(|r| o.value(r, key).trim()).collect();
        s.rows
            .iter()
            .filter(|r| owner(s, r) == Some(pid))
            .any(|r| taken.contains(s.value(r, key).trim()))
    })
}

/// Append project `pid` of `src` to `out`: every row of the project (given new
/// ids first when they would collide with `out`) and the dictionary rows `out`
/// lacks. Returns the project's id in `out`.
pub fn append_project(out: &mut Document, src: &Document, pid: i64) -> i64 {
    let mut src = extract_project(src, pid);
    let pid = if collides(out, &src, pid) {
        renumber_project(&mut src, pid, &[out], None)
    } else {
        pid
    };
    for t in &src.tables {
        let has_project = t.field("proj_id").is_some();
        let key = t.fields.first().cloned().unwrap_or_default();
        let target = out.table_mut(&t.name);
        if target.fields.is_empty() {
            target.fields = t.fields.clone();
        }
        let existing: HashSet<String> = target
            .rows
            .iter()
            .map(|r| target.value(r, &key).to_string())
            .collect();
        for r in &t.rows {
            let owned = has_project && owner(t, r) == Some(pid);
            if !owned && existing.contains(t.value(r, &key)) {
                continue;
            }
            copy_row(target, t, r, &[]);
        }
    }
    pid
}

/// What a baseline's PROJECT row says about it, for listing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BaselineInfo {
    pub short_name: String,
    pub name: String,
    pub data_date: String,
    pub added: String,
    pub updated: String,
}

pub fn baseline_info(doc: &Document, pid: i64) -> BaselineInfo {
    let v = |f: &str| project_value(doc, pid, f);
    let data_date = Some(v("last_recalc_date"))
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| v("plan_start_date"));
    let added = v("add_date");
    let updated = Some(v("last_baseline_update_date"))
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| added.clone());
    BaselineInfo {
        short_name: v("proj_short_name"),
        name: project_name(doc, pid),
        data_date,
        added,
        updated,
    }
}

/// Make project `pid` of `doc` a baseline of `parent`: `orig_proj_id` names
/// the parent, the names are set, and the baseline carries no baseline
/// assignment of its own.
pub fn mark_baseline(
    doc: &mut Document,
    pid: i64,
    parent: i64,
    short: &str,
    name: &str,
    stamp: &str,
) {
    set_project_value(doc, pid, "orig_proj_id", &parent.to_string());
    set_project_value(doc, pid, "sum_base_proj_id", "");
    set_project_value(doc, pid, "add_date", stamp);
    set_project_value(doc, pid, "last_baseline_update_date", stamp);
    set_project_names(doc, pid, short, name);
}

/// A copy of project `pid` of `doc` as a baseline of it, with a new project
/// id (`new_pid`) and fresh ids for its other records.
pub fn copy_as_baseline(
    doc: &Document,
    pid: i64,
    new_pid: i64,
    short: &str,
    name: &str,
    stamp: &str,
) -> Document {
    let mut copy = extract_project(doc, pid);
    let id = renumber_project(&mut copy, pid, &[doc], Some(new_pid));
    mark_baseline(&mut copy, id, pid, short, name, stamp);
    copy
}

/// Activity IDs of a project, keyed by internal activity id.
pub fn activity_codes(doc: &Document, pid: i64) -> BTreeMap<i64, String> {
    doc.table("TASK")
        .map(|t| {
            t.rows
                .iter()
                .filter(|r| owner(t, r) == Some(pid))
                .map(|r| {
                    (
                        num(t.value(r, "task_id")),
                        t.value(r, "task_code").to_string(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Rename Activity IDs of project `pid` (old code to new code). Refused, with
/// the clashing codes, when a new code is already used by an activity that
/// keeps its code.
pub fn rename_activities(
    doc: &mut Document,
    pid: i64,
    renames: &BTreeMap<String, String>,
) -> Result<usize, Vec<String>> {
    let codes = activity_codes(doc, pid);
    let kept: HashSet<&String> = codes
        .values()
        .filter(|c| !renames.contains_key(*c))
        .collect();
    let mut clashes: Vec<String> = renames
        .iter()
        .filter(|(old, new)| codes.values().any(|c| c == *old) && kept.contains(new))
        .map(|(_, new)| new.clone())
        .collect();
    clashes.sort();
    clashes.dedup();
    if !clashes.is_empty() {
        return Err(clashes);
    }
    let Some(t) = doc.tables.iter_mut().find(|t| t.name == "TASK") else {
        return Ok(0);
    };
    let (Some(pc), Some(cc)) = (t.field("proj_id"), t.field("task_code")) else {
        return Ok(0);
    };
    let mut changed = 0;
    for r in &mut t.rows {
        if r.get(pc).is_some_and(|v| num(v) == pid) {
            if let Some(new) = r.get(cc).and_then(|c| renames.get(c)).cloned() {
                if r[cc] != new {
                    r[cc] = new;
                    changed += 1;
                }
            }
        }
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn starter_doc() -> (Document, i64) {
        let p = openxer_xer::starter();
        (p.updated_document(), p.model.project.proj_id)
    }

    #[test]
    fn copy_gets_fresh_ids_and_names_its_parent() {
        let (doc, pid) = starter_doc();
        let copy = copy_as_baseline(
            &doc,
            pid,
            77,
            "RB - B1",
            "Riverbend - B1",
            "2026-11-04 10:00",
        );
        assert_eq!(copy.projects(), vec![(77, "RB - B1".to_string())]);
        assert_eq!(project_value(&copy, 77, "orig_proj_id"), pid.to_string());
        // The starter has no root WBS node, so the short name is the name.
        assert_eq!(project_name(&copy, 77), "RB - B1");
        let tasks = |d: &Document, p: i64| activity_codes(d, p);
        let (a, b) = (tasks(&doc, pid), tasks(&copy, 77));
        assert_eq!(a.len(), b.len());
        assert!(a.keys().all(|k| !b.contains_key(k)), "task ids renumbered");
        let mut codes: Vec<_> = a.values().cloned().collect();
        let mut copied: Vec<_> = b.values().cloned().collect();
        codes.sort();
        copied.sort();
        assert_eq!(codes, copied, "activity IDs kept");
        // Relationships point at the copied activities.
        let t = copy.table("TASKPRED").unwrap();
        assert!(t
            .rows
            .iter()
            .all(|r| b.contains_key(&num(t.value(r, "task_id")))
                && b.contains_key(&num(t.value(r, "pred_task_id")))));
        // The copy loads as a project of its own.
        let loaded =
            openxer_xer::ProjectData::from_document(copy.clone(), 77, "bl".into()).unwrap();
        assert_eq!(loaded.model.activities.len(), a.len());
        assert_eq!(baselines_in(&copy), vec![]);
        // Beside its parent it is a baseline and drops out of the pick list.
        let mut both = doc.clone();
        assert_eq!(append_project(&mut both, &copy, 77), 77);
        assert_eq!(baselines_in(&both), vec![(77, pid)]);
        assert_eq!(schedule_projects(&both).len(), 1);
        assert_eq!(
            baseline_info(&both, 77).added,
            "2026-11-04 10:00".to_string()
        );
    }

    #[test]
    fn append_renumbers_only_on_collision_and_keeps_dictionaries_once() {
        let (doc, pid) = starter_doc();
        // A copy with the parent's own ids collides and is renumbered.
        let mut same = extract_project(&doc, pid);
        let original = activity_codes(&same, pid);
        let mut out = doc.clone();
        let calendars = out.table("CALENDAR").map_or(0, |t| t.rows.len());
        let new = append_project(&mut out, &same, pid);
        assert_ne!(new, pid);
        assert_eq!(out.projects().len(), 2);
        assert_eq!(activity_codes(&out, new).len(), original.len());
        let tasks = out.table("TASK").unwrap();
        let ids: HashSet<&str> = tasks
            .rows
            .iter()
            .map(|r| tasks.value(r, "task_id"))
            .collect();
        assert_eq!(ids.len(), tasks.rows.len(), "no duplicate task ids");
        let global = |d: &Document| {
            d.table("CALENDAR").map_or(0, |t| {
                t.rows
                    .iter()
                    .filter(|r| t.value(r, "proj_id").is_empty())
                    .count()
            })
        };
        assert!(global(&doc) > 0);
        assert_eq!(
            global(&out),
            global(&doc),
            "shared calendars are not copied"
        );
        assert_eq!(
            out.table("CALENDAR").map_or(0, |t| t.rows.len()),
            calendars,
            "the starter has no project calendar to copy"
        );
        // Renaming in the copy is refused on a clash and applied otherwise.
        let codes: Vec<String> = original.values().cloned().collect();
        let clash = BTreeMap::from([(codes[0].clone(), codes[1].clone())]);
        assert!(rename_activities(&mut same, pid, &clash).is_err());
        let ok = BTreeMap::from([(codes[0].clone(), "NEW-1".to_string())]);
        assert_eq!(rename_activities(&mut same, pid, &ok), Ok(1));
        assert!(activity_codes(&same, pid).values().any(|c| c == "NEW-1"));
    }

    /// Regression: KEYS left out document links, project activity codes,
    /// thresholds, issues and risks, so a baseline beside its parent in one
    /// file reused their ids.
    #[test]
    fn baseline_beside_its_parent_shares_no_record_ids() {
        let (mut doc, pid) = starter_doc();
        let p = pid.to_string();
        let add = |doc: &mut Document, table: &str, values: &[(&str, &str)]| {
            let values: Vec<(&str, String)> =
                values.iter().map(|(k, v)| (*k, v.to_string())).collect();
            doc.table_mut(table).add(&values);
        };
        add(
            &mut doc,
            "ACTVTYPE",
            &[
                ("actv_code_type_id", "7"),
                ("actv_code_type", "Area"),
                ("actv_code_type_scope", "AS_Project"),
                ("proj_id", &p),
            ],
        );
        add(
            &mut doc,
            "ACTVTYPE",
            &[
                ("actv_code_type_id", "8"),
                ("actv_code_type", "Global"),
                ("actv_code_type_scope", "AS_Global"),
            ],
        );
        add(
            &mut doc,
            "ACTVCODE",
            &[
                ("actv_code_id", "71"),
                ("actv_code_type_id", "7"),
                ("short_name", "NORTH"),
            ],
        );
        add(
            &mut doc,
            "ACTVCODE",
            &[
                ("actv_code_id", "72"),
                ("actv_code_type_id", "7"),
                ("short_name", "SOUTH"),
                ("parent_actv_code_id", "71"),
            ],
        );
        add(
            &mut doc,
            "ACTVCODE",
            &[
                ("actv_code_id", "81"),
                ("actv_code_type_id", "8"),
                ("short_name", "ALL"),
            ],
        );
        add(
            &mut doc,
            "TASKACTV",
            &[
                ("task_id", "1010"),
                ("actv_code_type_id", "7"),
                ("actv_code_id", "72"),
                ("proj_id", &p),
            ],
        );
        add(
            &mut doc,
            "TASKACTV",
            &[
                ("task_id", "1010"),
                ("actv_code_type_id", "8"),
                ("actv_code_id", "81"),
                ("proj_id", &p),
            ],
        );
        add(
            &mut doc,
            "DOCUMENT",
            &[("doc_id", "3"), ("doc_name", "Drawing"), ("proj_id", &p)],
        );
        add(
            &mut doc,
            "TASKDOC",
            &[
                ("taskdoc_id", "5"),
                ("doc_id", "3"),
                ("task_id", "1010"),
                ("proj_id", &p),
            ],
        );
        add(&mut doc, "PROJTHRS", &[("thresh_id", "4"), ("proj_id", &p)]);
        add(
            &mut doc,
            "PROJISSU",
            &[
                ("issue_id", "6"),
                ("proj_id", &p),
                ("issue_name", "Late steel"),
            ],
        );
        add(
            &mut doc,
            "ISSUHIST",
            &[
                ("issue_id", "6"),
                ("proj_id", &p),
                ("issue_history", "Raised"),
            ],
        );
        add(
            &mut doc,
            "PROJRISK",
            &[("risk_id", "9"), ("proj_id", &p), ("risk_name", "Weather")],
        );
        add(
            &mut doc,
            "TASKRISK",
            &[("task_id", "1010"), ("risk_id", "9"), ("proj_id", &p)],
        );
        let copy = copy_as_baseline(&doc, pid, 77, "B1", "Baseline 1", "2026-11-04 10:00");
        let mut both = doc.clone();
        append_project(&mut both, &copy, 77);
        for (table, key) in [
            ("ACTVTYPE", "actv_code_type_id"),
            ("ACTVCODE", "actv_code_id"),
            ("DOCUMENT", "doc_id"),
            ("TASKDOC", "taskdoc_id"),
            ("PROJTHRS", "thresh_id"),
            ("PROJISSU", "issue_id"),
            ("PROJRISK", "risk_id"),
        ] {
            let t = both.table(table).unwrap();
            let ids: Vec<&str> = t.rows.iter().map(|r| t.value(r, key)).collect();
            let unique: HashSet<&&str> = ids.iter().collect();
            assert_eq!(unique.len(), ids.len(), "{table}: {ids:?}");
        }
        assert_eq!(
            both.table("ACTVCODE").unwrap().rows.len(),
            5,
            "the global code is shared"
        );
        // The baseline's rows point at its own copies.
        let one = |table: &str, key: &str| -> String {
            let t = both.table(table).unwrap();
            t.rows
                .iter()
                .find(|r| t.value(r, "proj_id") == "77")
                .map(|r| t.value(r, key).to_string())
                .unwrap_or_default()
        };
        let own_type = one("ACTVTYPE", "actv_code_type_id");
        let codes = both.table("ACTVCODE").unwrap();
        let taskactv = both.table("TASKACTV").unwrap();
        let assigned: Vec<&Vec<String>> = taskactv
            .rows
            .iter()
            .filter(|r| taskactv.value(r, "proj_id") == "77")
            .collect();
        assert_eq!(assigned.len(), 2);
        for r in assigned {
            let code = taskactv.value(r, "actv_code_id");
            let row = codes
                .rows
                .iter()
                .find(|c| codes.value(c, "actv_code_id") == code)
                .unwrap();
            assert_eq!(
                codes.value(row, "actv_code_type_id"),
                taskactv.value(r, "actv_code_type_id")
            );
        }
        let child = codes
            .rows
            .iter()
            .find(|c| {
                codes.value(c, "actv_code_type_id") == own_type
                    && codes.value(c, "short_name") == "SOUTH"
            })
            .unwrap();
        let parent = codes.value(child, "parent_actv_code_id");
        assert!(codes
            .rows
            .iter()
            .any(|c| codes.value(c, "actv_code_id") == parent
                && codes.value(c, "actv_code_type_id") == own_type));
        assert_eq!(one("TASKDOC", "doc_id"), one("DOCUMENT", "doc_id"));
        assert_eq!(one("ISSUHIST", "issue_id"), one("PROJISSU", "issue_id"));
        assert_eq!(one("TASKRISK", "risk_id"), one("PROJRISK", "risk_id"));
        // Removing the copy again takes its code values with it.
        remove_project_rows(&mut both, 77);
        for t in &doc.tables {
            let after = both.table(&t.name).unwrap();
            assert_eq!(after.rows.len(), t.rows.len(), "{}", t.name);
        }
        assert_eq!(
            both.table("ACTVCODE").unwrap().rows,
            doc.table("ACTVCODE").unwrap().rows
        );
    }

    #[test]
    fn project_values_add_columns_only_for_values() {
        let (mut doc, pid) = starter_doc();
        let fields = doc.table("PROJECT").unwrap().fields.len();
        set_project_value(&mut doc, pid, "no_such_column", "");
        assert_eq!(doc.table("PROJECT").unwrap().fields.len(), fields);
        set_project_value(&mut doc, pid, "sum_base_proj_id", "12");
        assert_eq!(project_value(&doc, pid, "sum_base_proj_id"), "12");
    }
}
