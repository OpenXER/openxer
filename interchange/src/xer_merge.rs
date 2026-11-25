//! Update Existing Project: merge one project of an imported XER into a project of
//! an existing document, per data type, matching records by names and codes.
//!
//! The merge is computed on a copy of the target document. Nothing is written
//! anywhere; callers show the report, then store the result in one transaction.
use crate::util::{copy_row, next_id, num, remove_rows, rows_where, same_text, update_row, Col};
use openxer_xer::{Document, ProjectData, Table};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataType {
    Activities,
    Relationships,
    Wbs,
    Calendars,
    Resources,
    ActivityCodes,
    Udfs,
    Notebooks,
    Assignments,
}
impl DataType {
    pub const ALL: [DataType; 9] = [
        DataType::Activities,
        DataType::Relationships,
        DataType::Wbs,
        DataType::Assignments,
        DataType::Calendars,
        DataType::Resources,
        DataType::ActivityCodes,
        DataType::Udfs,
        DataType::Notebooks,
    ];
    pub fn label(self) -> &'static str {
        match self {
            DataType::Activities => "Activities",
            DataType::Relationships => "Activity Relationships",
            DataType::Wbs => "WBS",
            DataType::Calendars => "Calendars",
            DataType::Resources => "Resources",
            DataType::ActivityCodes => "Activity Codes",
            DataType::Udfs => "User Defined Fields",
            DataType::Notebooks => "Notebook Topics",
            DataType::Assignments => "Activity Resource Assignments",
        }
    }
    /// Global data lives outside the project and is shared with other projects.
    pub fn is_global(self) -> bool {
        matches!(
            self,
            DataType::Calendars
                | DataType::Resources
                | DataType::ActivityCodes
                | DataType::Udfs
                | DataType::Notebooks
        )
    }
    pub fn actions(self) -> &'static [Action] {
        if self.is_global() {
            &[
                Action::InsertNew,
                Action::KeepExisting,
                Action::UpdateExisting,
                Action::DoNotImport,
            ]
        } else {
            &[
                Action::KeepExisting,
                Action::UpdateExisting,
                Action::DoNotImport,
            ]
        }
    }
    pub fn supports_delete(self) -> bool {
        matches!(
            self,
            DataType::Activities | DataType::Relationships | DataType::Assignments
        )
    }
    pub fn key(self) -> &'static str {
        match self {
            DataType::Activities => "activities",
            DataType::Relationships => "relationships",
            DataType::Wbs => "wbs",
            DataType::Calendars => "calendars",
            DataType::Resources => "resources",
            DataType::ActivityCodes => "activity_codes",
            DataType::Udfs => "udfs",
            DataType::Notebooks => "notebooks",
            DataType::Assignments => "assignments",
        }
    }
    pub fn from_key(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.key() == s.trim())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    InsertNew,
    KeepExisting,
    UpdateExisting,
    DoNotImport,
}
impl Action {
    pub fn label(self) -> &'static str {
        match self {
            Action::InsertNew => "Insert New",
            Action::KeepExisting => "Keep Existing",
            Action::UpdateExisting => "Update Existing",
            Action::DoNotImport => "Do Not Import",
        }
    }
    pub fn from_key(s: &str) -> Option<Self> {
        match s
            .trim()
            .to_ascii_lowercase()
            .replace([' ', '-'], "_")
            .as_str()
        {
            "insert" | "insert_new" => Some(Action::InsertNew),
            "keep" | "keep_existing" => Some(Action::KeepExisting),
            "update" | "update_existing" => Some(Action::UpdateExisting),
            "skip" | "none" | "do_not_import" => Some(Action::DoNotImport),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypeConfig {
    pub action: Action,
    pub delete: bool,
}

/// Per data type import actions. Defaults are the cautious ones: shared (global)
/// data keeps what the workspace has; project data is updated from the file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImportConfig(pub BTreeMap<DataType, TypeConfig>);
impl Default for ImportConfig {
    fn default() -> Self {
        Self(
            DataType::ALL
                .into_iter()
                .map(|t| {
                    let action = if t.is_global() && t != DataType::Notebooks {
                        Action::KeepExisting
                    } else {
                        Action::UpdateExisting
                    };
                    (
                        t,
                        TypeConfig {
                            action,
                            delete: false,
                        },
                    )
                })
                .collect(),
        )
    }
}
impl ImportConfig {
    pub fn get(&self, t: DataType) -> TypeConfig {
        self.0.get(&t).copied().unwrap_or(TypeConfig {
            action: Action::KeepExisting,
            delete: false,
        })
    }
    pub fn set(&mut self, t: DataType, action: Action, delete: bool) {
        self.0.insert(
            t,
            TypeConfig {
                action,
                delete: delete && t.supports_delete(),
            },
        );
    }
    /// Parse `activities=update,calendars=keep,relationships=update+delete`.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut config = Self::default();
        for part in text.split(',').filter(|p| !p.trim().is_empty()) {
            let (key, value) = part
                .split_once('=')
                .ok_or_else(|| format!("Expected type=action, got {part:?}"))?;
            let t = DataType::from_key(key).ok_or_else(|| {
                format!(
                    "Unknown data type {key:?}; use {}",
                    DataType::ALL.map(|t| t.key()).join(", ")
                )
            })?;
            let (value, delete) = match value.split_once('+') {
                Some((v, flag)) if flag.trim() == "delete" => (v, true),
                Some(_) => return Err(format!("Unknown flag in {part:?}; only +delete")),
                None => (value, false),
            };
            let action = Action::from_key(value)
                .ok_or_else(|| format!("Unknown action {value:?} for {key}"))?;
            if !t.actions().contains(&action) {
                return Err(format!("{} does not support {}", t.label(), action.label()));
            }
            if delete && !t.supports_delete() {
                return Err(format!("{} does not support delete", t.label()));
            }
            config.set(t, action, delete);
        }
        Ok(config)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Counts {
    pub added: usize,
    pub updated: usize,
    pub unchanged: usize,
    pub kept: usize,
    pub deleted: usize,
    pub skipped: usize,
}
impl Counts {
    pub fn total_changes(&self) -> usize {
        self.added + self.updated + self.deleted
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct MergeReport {
    pub counts: BTreeMap<DataType, Counts>,
    pub messages: Vec<String>,
}
impl MergeReport {
    fn c(&mut self, t: DataType) -> &mut Counts {
        self.counts.entry(t).or_default()
    }
    pub fn summary_lines(&self) -> Vec<String> {
        DataType::ALL
            .into_iter()
            .filter_map(|t| {
                let c = self.counts.get(&t)?;
                Some(format!(
                    "{}: {} added, {} updated, {} unchanged, {} kept, {} deleted, {} not imported",
                    t.label(),
                    c.added,
                    c.updated,
                    c.unchanged,
                    c.kept,
                    c.deleted,
                    c.skipped
                ))
            })
            .collect()
    }
}

/// Tables whose rows belong to one activity and are copied along with it.
const ACTIVITY_CHILDREN: [(&str, Option<&str>); 3] = [
    ("TASKPROC", Some("proc_id")),
    ("PROJCOST", Some("cost_item_id")),
    ("TASKNOTE", None),
];

/// Extra column values for a copied or updated dictionary row.
type Overrides<'a> = dyn Fn(&Table, &[String]) -> Vec<(&'static str, String)> + 'a;

struct Merge<'a> {
    src: &'a Document,
    out: Document,
    s: i64,
    t: i64,
    cfg: &'a ImportConfig,
    report: MergeReport,
    cal: HashMap<String, String>,
    rsrc: HashMap<String, String>,
    wbs: HashMap<String, String>,
    task: HashMap<String, String>,
    code_type: HashMap<String, String>,
    code: HashMap<String, String>,
    udf: HashMap<String, String>,
    memo: HashMap<String, String>,
    /// Source activity ids present in the file (for deletes).
    file_tasks: HashSet<String>,
    /// Target ids of referenced dictionary rows, by (table, source id).
    refs: HashMap<(&'static str, String), String>,
    /// Target ids of shift periods, by source id.
    shift_periods: HashMap<String, String>,
    /// Dictionary rows brought from the file, and references cleared
    /// because the file does not hold the row they name, by table.
    brought: BTreeMap<&'static str, usize>,
    cleared: BTreeMap<&'static str, usize>,
    /// The next free id of each (table, id column), found by one scan of the
    /// table on first use and then counted up. Every row the merge adds takes
    /// its id from here, so the count stays above every id in the table.
    next_ids: HashMap<(String, String), i64>,
    /// Activity child rows ([`ACTIVITY_CHILDREN`]) by table, indexed once
    /// per activities pass.
    children: HashMap<&'static str, Children>,
}

/// One activity child table during the activities pass: the file's rows by
/// source activity, the target project's rows by target activity (rows
/// added since included), and the target rows replaced so far. Replaced rows
/// are removed once when the pass ends, so row positions stay put during it.
#[derive(Default)]
struct Children {
    src: HashMap<String, Vec<usize>>,
    out: HashMap<String, Vec<usize>>,
    doomed: Vec<usize>,
}

/// Dictionaries that merged rows refer to and that the merge does not match
/// by itself: (table, id column, matching column, parent column, label). A
/// reference is matched to the target row of the same name, or the row is
/// brought from the file (with its parent), or, when the file does not hold
/// it, cleared. Users are matched but never brought across.
const REFERENCED: [(&str, &str, &str, Option<&str>, &str); 6] = [
    (
        "ROLES",
        "role_id",
        "role_short_name",
        Some("parent_role_id"),
        "roles",
    ),
    (
        "ACCOUNT",
        "acct_id",
        "acct_short_name",
        Some("parent_acct_id"),
        "cost accounts",
    ),
    (
        "RSRCCURVDATA",
        "curv_id",
        "curv_name",
        None,
        "resource curves",
    ),
    (
        "COSTTYPE",
        "cost_type_id",
        "cost_type",
        None,
        "expense categories",
    ),
    ("SHIFT", "shift_id", "shift_name", None, "shifts"),
    ("USERS", "user_id", "user_name", None, "users"),
];

fn table<'a>(doc: &'a Document, name: &str) -> Option<&'a Table> {
    doc.table(name)
}

/// Merge `source_pid` from `source` into `target_pid` of `target`.
pub fn merge_project(
    target: &Document,
    target_pid: i64,
    source: &Document,
    source_pid: i64,
    config: &ImportConfig,
) -> Result<(Document, MergeReport), String> {
    if !target.projects().iter().any(|(id, _)| *id == target_pid) {
        return Err(format!(
            "Project {target_pid} is not in the workspace document"
        ));
    }
    if !source.projects().iter().any(|(id, _)| *id == source_pid) {
        return Err(format!("Project {source_pid} is not in the imported file"));
    }
    // Write the file's project the way OpenXER stores projects, so differences in
    // number or date formatting are not reported as updates.
    let normalized = ProjectData::from_document(source.clone(), source_pid, "import".into())
        .map_err(|e| format!("The imported project cannot be read: {e}"))?
        .updated_document();
    let mut out = target.clone();
    out.keep_lone_quotes_of(&normalized);
    let mut m = Merge {
        src: &normalized,
        out,
        s: source_pid,
        t: target_pid,
        cfg: config,
        report: MergeReport::default(),
        cal: HashMap::new(),
        rsrc: HashMap::new(),
        wbs: HashMap::new(),
        task: HashMap::new(),
        code_type: HashMap::new(),
        code: HashMap::new(),
        udf: HashMap::new(),
        memo: HashMap::new(),
        file_tasks: HashSet::new(),
        refs: HashMap::new(),
        shift_periods: HashMap::new(),
        brought: BTreeMap::new(),
        cleared: BTreeMap::new(),
        next_ids: HashMap::new(),
        children: HashMap::new(),
    };
    m.calendars();
    m.resources();
    m.activity_codes();
    m.udf_types();
    m.memo_types();
    m.wbs();
    m.activities();
    m.relationships();
    m.assignments();
    m.code_assignments();
    m.udf_values();
    m.notes();
    m.project_dates();
    m.reference_messages();
    let Merge {
        out, mut report, ..
    } = m;
    // The result must load as a schedulable project before anyone stores it.
    let check = ProjectData::from_document(out.clone(), target_pid, "merge".into())
        .map_err(|e| format!("The merged project would not load: {e}"))?;
    check
        .model
        .validate()
        .map_err(|e| format!("The merged project is not valid: {e}"))?;
    report.messages.extend(check.warnings);
    Ok((out, report))
}

impl Merge<'_> {
    /// A new id for a row added to `table` (see `next_ids`).
    fn new_id(&mut self, table: &str, key: &str) -> String {
        let slot = (table.to_string(), key.to_string());
        let id = match self.next_ids.get(&slot) {
            Some(&id) => id,
            None => next_id(&self.out, table, key),
        };
        self.next_ids.insert(slot, id + 1);
        id.to_string()
    }

    fn project_default_calendar(&self) -> String {
        let p = table(&self.out, "PROJECT").expect("checked");
        p.rows
            .iter()
            .find(|r| num(p.value(r, "proj_id")) == self.t)
            .map(|r| p.value(r, "clndr_id").to_string())
            .unwrap_or_default()
    }
    /// Generic dictionary pass: `relevant` selects source rows, `key` builds the
    /// match key, `candidates` selects target rows that may match.
    #[allow(clippy::too_many_arguments)]
    fn dictionary(
        &mut self,
        kind: DataType,
        name: &str,
        id: &str,
        name_field: &str,
        relevant: &dyn Fn(&Table, &[String]) -> bool,
        candidate: &dyn Fn(&Table, &[String]) -> bool,
        key: &dyn Fn(&Table, &[String]) -> String,
        keep: &[&str],
        overrides: &Overrides<'_>,
    ) -> HashMap<String, String> {
        let mut map = HashMap::new();
        let Some(src) = table(self.src, name) else {
            return map;
        };
        let action = self.cfg.get(kind).action;
        let rows: Vec<Vec<String>> = src
            .rows
            .iter()
            .filter(|r| relevant(src, r))
            .cloned()
            .collect();
        for row in rows {
            let src_id = src.value(&row, id).to_string();
            let k = key(src, &row);
            let found = table(&self.out, name).and_then(|t| {
                t.rows
                    .iter()
                    .position(|r| candidate(t, r) && key(t, r) == k)
            });
            let extra = overrides(src, &row);
            match (found, action) {
                (Some(i), Action::UpdateExisting) => {
                    let t = self.out.table_mut(name);
                    let mut keep_cols = vec![id];
                    keep_cols.extend_from_slice(keep);
                    let changed = update_row(t, i, src, &row, &keep_cols, &extra);
                    map.insert(src_id, t.value(&t.rows[i], id).to_string());
                    let c = self.report.c(kind);
                    if changed {
                        c.updated += 1
                    } else {
                        c.unchanged += 1
                    }
                }
                (Some(i), Action::KeepExisting) => {
                    let t = self.out.table(name).expect("found");
                    map.insert(src_id, t.value(&t.rows[i], id).to_string());
                    self.report.c(kind).kept += 1;
                }
                (Some(i), Action::DoNotImport) => {
                    let t = self.out.table(name).expect("found");
                    map.insert(src_id, t.value(&t.rows[i], id).to_string());
                    self.report.c(kind).skipped += 1;
                }
                (None, Action::DoNotImport) => {
                    self.report.c(kind).skipped += 1;
                }
                (found, _) => {
                    // Insert, renaming a duplicate when Insert New meets an existing record.
                    let new_id = self.new_id(name, id);
                    let mut values: Vec<(&str, String)> = extra;
                    values.push((id, new_id.clone()));
                    if found.is_some() {
                        let base = src.value(&row, name_field).to_string();
                        let mut n = 1;
                        let unique = loop {
                            let candidate_name = format!("{base}_{n}");
                            let taken = table(&self.out, name).is_some_and(|t| {
                                t.rows
                                    .iter()
                                    .any(|r| same_text(t.value(r, name_field), &candidate_name))
                            });
                            if !taken {
                                break candidate_name;
                            }
                            n += 1;
                        };
                        values.push((name_field, unique));
                        values.push(("guid", String::new()));
                    }
                    copy_row(self.out.table_mut(name), src, &row, &values);
                    map.insert(src_id, new_id);
                    self.report.c(kind).added += 1;
                }
            }
        }
        map
    }

    /// The target id for source id `src_id` of a [`REFERENCED`] table:
    /// the target row of the same name, else the file's row brought across,
    /// else blank.
    fn reference(&mut self, name: &'static str, src_id: &str) -> String {
        let src_id = src_id.trim();
        if src_id.is_empty() {
            return String::new();
        }
        let key = (name, src_id.to_string());
        if let Some(v) = self.refs.get(&key) {
            return v.clone();
        }
        // A parent loop in the file ends at the first repeat.
        self.refs.insert(key.clone(), String::new());
        let mapped = self.resolve_reference(name, src_id);
        self.refs.insert(key, mapped.clone());
        mapped
    }

    fn resolve_reference(&mut self, name: &'static str, src_id: &str) -> String {
        let (_, id, matching, parent, _) = *REFERENCED
            .iter()
            .find(|r| r.0 == name)
            .expect("listed table");
        let doc: &Document = self.src;
        let found = table(doc, name).and_then(|src| {
            src.rows
                .iter()
                .find(|r| src.value(r, id).trim() == src_id)
                .map(|r| (src, r))
        });
        let Some((src, row)) = found else {
            *self.cleared.entry(name).or_default() += 1;
            return String::new();
        };
        let text = src.value(row, matching).trim().to_lowercase();
        if let Some(t) = table(&self.out, name) {
            let same = |r: &Vec<String>| t.value(r, matching).trim().to_lowercase() == text;
            // The same id with the same name first (a file from this
            // workspace), then any row of that name.
            if let Some(r) = t
                .rows
                .iter()
                .find(|r| t.value(r, id).trim() == src_id && same(r))
                .or_else(|| t.rows.iter().find(|r| !text.is_empty() && same(r)))
            {
                return t.value(r, id).to_string();
            }
        }
        if name == "USERS" || text.is_empty() {
            *self.cleared.entry(name).or_default() += 1;
            return String::new();
        }
        let mut values = vec![(id, self.new_id(name, id))];
        if let Some(parent) = parent {
            let p = src.value(row, parent).to_string();
            values.push((parent, self.reference(name, &p)));
        }
        let new_id = values[0].1.clone();
        copy_row(self.out.table_mut(name), src, row, &values);
        *self.brought.entry(name).or_default() += 1;
        if name == "SHIFT" {
            // A shift comes with its periods.
            if let Some(periods) = table(doc, "SHIFTPER") {
                for r in periods
                    .rows
                    .iter()
                    .filter(|r| periods.value(r, "shift_id").trim() == src_id)
                {
                    let pid = self.new_id("SHIFTPER", "shift_period_id");
                    self.shift_periods.insert(
                        periods.value(r, "shift_period_id").trim().to_string(),
                        pid.clone(),
                    );
                    copy_row(
                        self.out.table_mut("SHIFTPER"),
                        periods,
                        r,
                        &[("shift_period_id", pid), ("shift_id", new_id.clone())],
                    );
                }
            }
        }
        new_id
    }

    /// The target shift period for a source one: a period of the matched or
    /// brought shift that starts at the same hour, else blank.
    fn shift_period(&mut self, src_id: &str) -> String {
        let src_id = src_id.trim();
        if src_id.is_empty() {
            return String::new();
        }
        let doc: &Document = self.src;
        let Some((shift, start)) = table(doc, "SHIFTPER").and_then(|t| {
            t.rows
                .iter()
                .find(|r| t.value(r, "shift_period_id").trim() == src_id)
                .map(|r| {
                    (
                        t.value(r, "shift_id").to_string(),
                        t.value(r, "shift_start_hr_num").trim().to_string(),
                    )
                })
        }) else {
            return String::new();
        };
        let shift = self.reference("SHIFT", &shift);
        if let Some(p) = self.shift_periods.get(src_id) {
            return p.clone();
        }
        table(&self.out, "SHIFTPER")
            .and_then(|t| {
                t.rows
                    .iter()
                    .find(|r| {
                        t.value(r, "shift_id") == shift
                            && num(t.value(r, "shift_start_hr_num")) == num(&start)
                    })
                    .map(|r| t.value(r, "shift_period_id").to_string())
            })
            .unwrap_or_default()
    }

    /// Overrides for the [`REFERENCED`] columns `columns` of a source row
    /// (only the columns the file's table has).
    fn reference_overrides(
        &mut self,
        src: &Table,
        row: &[String],
        columns: &[(&'static str, &'static str)],
    ) -> Vec<(&'static str, String)> {
        columns
            .iter()
            .filter(|(column, _)| src.field(column).is_some())
            .map(|&(column, name)| (column, self.reference(name, src.value(row, column))))
            .collect()
    }

    fn reference_messages(&mut self) {
        let label = |name: &str| REFERENCED.iter().find(|r| r.0 == name).map_or("", |r| r.4);
        for (name, n) in &self.brought {
            self.report.messages.push(format!(
                "{n} {} the target did not have were added from the file.",
                label(name)
            ));
        }
        for (name, n) in &self.cleared {
            self.report.messages.push(format!(
                "{n} references to {} that the target does not have were cleared.",
                label(name)
            ));
        }
    }

    fn calendars(&mut self) {
        let (s, t) = (self.s, self.t);
        let project_cal = |tab: &Table, r: &[String]| !tab.value(r, "proj_id").trim().is_empty();
        let map = self.dictionary(
            DataType::Calendars,
            "CALENDAR",
            "clndr_id",
            "clndr_name",
            &|tab, r| !project_cal(tab, r) || num(tab.value(r, "proj_id")) == s,
            &|tab, r| !project_cal(tab, r) || num(tab.value(r, "proj_id")) == t,
            &|tab, r| {
                format!(
                    "{}|{}|{}",
                    tab.value(r, "clndr_name").trim().to_lowercase(),
                    tab.value(r, "clndr_type"),
                    project_cal(tab, r)
                )
            },
            &["proj_id", "base_clndr_id"],
            &|tab, r| {
                if project_cal(tab, r) {
                    vec![("proj_id", t.to_string())]
                } else {
                    vec![]
                }
            },
        );
        self.cal = map;
        // Derived calendars point at their base calendar's new id.
        let cal = self.cal.clone();
        if let Some(src) = table(self.src, "CALENDAR") {
            for (src_id, new_id) in &cal {
                let base = src
                    .rows
                    .iter()
                    .find(|r| src.value(r, "clndr_id") == src_id)
                    .map(|r| src.value(r, "base_clndr_id").to_string())
                    .unwrap_or_default();
                if base.is_empty() {
                    continue;
                }
                let out = self.out.table_mut("CALENDAR");
                if let Some(i) = out
                    .rows
                    .iter()
                    .position(|r| out.value(r, "clndr_id") == new_id)
                {
                    let mapped = cal.get(&base).cloned().unwrap_or_default();
                    if out.value(&out.rows[i], "base_clndr_id") != mapped && !mapped.is_empty() {
                        out.set(i, "base_clndr_id", mapped);
                    }
                }
            }
        }
    }

    fn resources(&mut self) {
        let before: HashSet<String> = table(&self.out, "RSRC").map_or_else(HashSet::new, |t| {
            t.rows
                .iter()
                .map(|r| t.value(r, "rsrc_id").to_string())
                .collect()
        });
        let fallback = self.project_default_calendar();
        let cal = self.cal.clone();
        let map = self.dictionary(
            DataType::Resources,
            "RSRC",
            "rsrc_id",
            "rsrc_short_name",
            &|_, _| true,
            &|_, _| true,
            &|tab, r| {
                let short = tab.value(r, "rsrc_short_name").trim();
                if short.is_empty() {
                    tab.value(r, "rsrc_name").trim().to_lowercase()
                } else {
                    short.to_lowercase()
                }
            },
            &[
                "parent_rsrc_id",
                "clndr_id",
                "user_id",
                "pobs_id",
                "role_id",
                "shift_id",
            ],
            &|tab, r| {
                let c = tab.value(r, "clndr_id");
                vec![(
                    "clndr_id",
                    cal.get(c).cloned().unwrap_or_else(|| fallback.clone()),
                )]
            },
        );
        // Resources brought from the file name the target's roles, shifts
        // and users, and bring their rates.
        let inserted: Vec<(String, String)> = map
            .iter()
            .filter(|(_, new_id)| !before.contains(*new_id))
            .map(|(s, n)| (s.clone(), n.clone()))
            .collect();
        let doc: &Document = self.src;
        if let Some(src) = table(doc, "RSRC") {
            let rows: HashMap<&str, &Vec<String>> = src
                .rows
                .iter()
                .map(|r| (src.value(r, "rsrc_id"), r))
                .collect();
            for (src_id, new_id) in &inserted {
                let Some(row) = rows.get(src_id.as_str()) else {
                    continue;
                };
                let values = self.reference_overrides(
                    src,
                    row,
                    &[
                        ("role_id", "ROLES"),
                        ("shift_id", "SHIFT"),
                        ("user_id", "USERS"),
                    ],
                );
                let out = self.out.table_mut("RSRC");
                if let Some(i) = out
                    .rows
                    .iter()
                    .position(|r| out.value(r, "rsrc_id") == new_id)
                {
                    for (column, value) in values {
                        out.set(i, column, value);
                    }
                }
            }
        }
        if let Some(rates) = table(doc, "RSRCRATE") {
            let inserted: HashMap<&str, &str> = inserted
                .iter()
                .map(|(s, n)| (s.as_str(), n.as_str()))
                .collect();
            for r in &rates.rows {
                let Some(new_rsrc) = inserted.get(rates.value(r, "rsrc_id")) else {
                    continue;
                };
                let mut values = vec![
                    ("rsrc_rate_id", self.new_id("RSRCRATE", "rsrc_rate_id")),
                    ("rsrc_id", new_rsrc.to_string()),
                ];
                if rates.field("shift_period_id").is_some() {
                    values.push((
                        "shift_period_id",
                        self.shift_period(rates.value(r, "shift_period_id")),
                    ));
                }
                copy_row(self.out.table_mut("RSRCRATE"), rates, r, &values);
            }
        }
        // Parents point at mapped resources; unknown parents become top level.
        if let Some(src) = table(self.src, "RSRC") {
            for (src_id, new_id) in &map {
                let parent = src
                    .rows
                    .iter()
                    .find(|r| src.value(r, "rsrc_id") == src_id)
                    .map(|r| src.value(r, "parent_rsrc_id").to_string())
                    .unwrap_or_default();
                let out = self.out.table_mut("RSRC");
                if let Some(i) = out
                    .rows
                    .iter()
                    .position(|r| out.value(r, "rsrc_id") == new_id)
                {
                    let mapped = map.get(&parent).cloned().unwrap_or_default();
                    let current = out.value(&out.rows[i], "parent_rsrc_id").to_string();
                    if !current.is_empty() && mapped.is_empty() {
                        // Keep an existing parent in the workspace.
                        if out.rows.iter().any(|r| out.value(r, "rsrc_id") == current) {
                            continue;
                        }
                    }
                    if current != mapped {
                        out.set(i, "parent_rsrc_id", mapped);
                    }
                }
            }
        }
        self.rsrc = map;
    }

    fn activity_codes(&mut self) {
        let (s, t) = (self.s, self.t);
        let project_type = |tab: &Table, r: &[String]| !tab.value(r, "proj_id").trim().is_empty();
        self.code_type = self.dictionary(
            DataType::ActivityCodes,
            "ACTVTYPE",
            "actv_code_type_id",
            "actv_code_type",
            &|tab, r| !project_type(tab, r) || num(tab.value(r, "proj_id")) == s,
            &|tab, r| !project_type(tab, r) || num(tab.value(r, "proj_id")) == t,
            &|tab, r| {
                format!(
                    "{}|{}",
                    tab.value(r, "actv_code_type").trim().to_lowercase(),
                    project_type(tab, r)
                )
            },
            &["proj_id", "wbs_id"],
            &|tab, r| {
                if project_type(tab, r) {
                    vec![("proj_id", t.to_string())]
                } else {
                    vec![]
                }
            },
        );
        // Code values: matched within their (mapped) type by the path of short names.
        let Some(src) = table(self.src, "ACTVCODE") else {
            return;
        };
        // The path of short names of every code value of a table, by id.
        let paths = |tab: &Table| -> HashMap<String, String> {
            let (id, name, parent) = (
                Col::of(tab, "actv_code_id"),
                Col::of(tab, "short_name"),
                Col::of(tab, "parent_actv_code_id"),
            );
            let mut by_id: HashMap<&str, &[String]> = HashMap::new();
            for r in &tab.rows {
                by_id.entry(id.get(r)).or_insert(r);
            }
            let mut out = HashMap::new();
            for r in &tab.rows {
                let mut parts = vec![];
                let mut current = id.get(r);
                for _ in 0..64 {
                    let Some(r) = by_id.get(current) else {
                        break;
                    };
                    parts.push(name.get(r).trim().to_lowercase());
                    current = parent.get(r);
                    if current.is_empty() {
                        break;
                    }
                }
                parts.reverse();
                out.entry(id.get(r).to_string())
                    .or_insert_with(|| parts.join("."));
            }
            out
        };
        let src_paths = paths(src);
        let path = |id: &str| src_paths.get(id).cloned().unwrap_or_default();
        // Target code values by (type, path), the first row of each as a
        // scan from the top found; added values join as they are added.
        let mut by_path: HashMap<(String, String), usize> = HashMap::new();
        if let Some(t) = table(&self.out, "ACTVCODE") {
            let dst_paths = paths(t);
            let (id, kind) = (Col::of(t, "actv_code_id"), Col::of(t, "actv_code_type_id"));
            for (i, r) in t.rows.iter().enumerate() {
                let p = dst_paths.get(id.get(r)).cloned().unwrap_or_default();
                by_path.entry((kind.get(r).to_string(), p)).or_insert(i);
            }
        }
        let action = self.cfg.get(DataType::ActivityCodes).action;
        // Parents first so their new ids exist when children are inserted.
        let mut rows: Vec<Vec<String>> = src
            .rows
            .iter()
            .filter(|r| {
                self.code_type
                    .contains_key(src.value(r, "actv_code_type_id"))
            })
            .cloned()
            .collect();
        rows.sort_by_key(|r| path(src.value(r, "actv_code_id")).matches('.').count());
        for row in rows {
            let src_id = src.value(&row, "actv_code_id").to_string();
            let new_type = self.code_type[src.value(&row, "actv_code_type_id")].clone();
            let key = path(&src_id);
            let found = by_path.get(&(new_type.clone(), key.clone())).copied();
            let parent = src.value(&row, "parent_actv_code_id").to_string();
            let parent = self.code.get(&parent).cloned().unwrap_or_default();
            let overrides = vec![
                ("actv_code_type_id", new_type.clone()),
                ("parent_actv_code_id", parent),
            ];
            match (found, action) {
                (Some(i), Action::UpdateExisting) => {
                    let t = self.out.table_mut("ACTVCODE");
                    update_row(
                        t,
                        i,
                        src,
                        &row,
                        &["actv_code_id", "total_assignments"],
                        &overrides,
                    );
                    self.code
                        .insert(src_id, t.value(&t.rows[i], "actv_code_id").to_string());
                }
                (Some(i), _) => {
                    let t = self.out.table("ACTVCODE").expect("found");
                    self.code
                        .insert(src_id, t.value(&t.rows[i], "actv_code_id").to_string());
                }
                (None, Action::DoNotImport) => {}
                (None, _) => {
                    let new_id = self.new_id("ACTVCODE", "actv_code_id");
                    let mut values = overrides;
                    values.push(("actv_code_id", new_id.clone()));
                    let i = copy_row(self.out.table_mut("ACTVCODE"), src, &row, &values);
                    by_path.entry((new_type, key)).or_insert(i);
                    self.code.insert(src_id, new_id);
                }
            }
        }
    }

    fn udf_types(&mut self) {
        self.udf = self.dictionary(
            DataType::Udfs,
            "UDFTYPE",
            "udf_type_id",
            "udf_type_label",
            &|_, _| true,
            &|_, _| true,
            &|tab, r| {
                format!(
                    "{}|{}",
                    tab.value(r, "table_name").to_lowercase(),
                    tab.value(r, "udf_type_label").trim().to_lowercase()
                )
            },
            &[],
            &|_, _| vec![],
        );
    }

    fn memo_types(&mut self) {
        self.memo = self.dictionary(
            DataType::Notebooks,
            "MEMOTYPE",
            "memo_type_id",
            "memo_type",
            &|_, _| true,
            &|_, _| true,
            &|tab, r| tab.value(r, "memo_type").trim().to_lowercase(),
            &["seq_num"],
            &|_, _| vec![],
        );
    }

    fn wbs_paths(doc: &Document, pid: i64) -> (Option<String>, HashMap<String, String>) {
        let mut root = None;
        let mut paths = HashMap::new();
        let Some(t) = table(doc, "PROJWBS") else {
            return (root, paths);
        };
        let rows: Vec<_> = t
            .rows
            .iter()
            .filter(|r| num(t.value(r, "proj_id")) == pid)
            .collect();
        let (wbs_id, flag, short, parent) = (
            Col::of(t, "wbs_id"),
            Col::of(t, "proj_node_flag"),
            Col::of(t, "wbs_short_name"),
            Col::of(t, "parent_wbs_id"),
        );
        let mut by_id: HashMap<&str, &Vec<String>> = HashMap::new();
        for r in &rows {
            if flag.get(r) == "Y" {
                root = Some(wbs_id.get(r).to_string());
            }
            by_id.entry(wbs_id.get(r)).or_insert(r);
        }
        for r in &rows {
            let id = wbs_id.get(r).to_string();
            if Some(&id) == root.as_ref() {
                continue;
            }
            let mut parts = vec![];
            let mut current = id.as_str();
            for _ in 0..256 {
                let Some(node) = by_id.get(current) else {
                    break;
                };
                if flag.get(node) == "Y" {
                    break;
                }
                parts.push(short.get(node).trim().to_lowercase());
                current = parent.get(node);
            }
            parts.reverse();
            paths.insert(id, parts.join("."));
        }
        (root, paths)
    }

    fn wbs(&mut self) {
        let (src_root, src_paths) = Self::wbs_paths(self.src, self.s);
        let (dst_root, dst_paths) = Self::wbs_paths(&self.out, self.t);
        let dst_root = dst_root.unwrap_or_default();
        if let Some(r) = &src_root {
            self.wbs.insert(r.clone(), dst_root.clone());
        }
        let Some(src) = table(self.src, "PROJWBS") else {
            return;
        };
        let action = self.cfg.get(DataType::Wbs).action;
        let mut ordered: Vec<(String, String)> = src_paths
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        ordered.sort_by(|a, b| {
            a.1.matches('.')
                .count()
                .cmp(&b.1.matches('.').count())
                .then(a.1.cmp(&b.1))
        });
        let root_obs = table(&self.out, "PROJWBS")
            .and_then(|t| {
                t.rows
                    .iter()
                    .find(|r| t.value(r, "wbs_id") == dst_root)
                    .map(|r| t.value(r, "obs_id").to_string())
            })
            .unwrap_or_default();
        // The file's WBS rows and the target's WBS ids by path, and the
        // target's rows by id, looked up instead of searched for each element.
        let mut src_rows: HashMap<&str, &Vec<String>> = HashMap::new();
        let src_wbs = Col::of(src, "wbs_id");
        for r in &src.rows {
            src_rows.entry(src_wbs.get(r)).or_insert(r);
        }
        let mut dst_by_path: HashMap<&str, &str> = HashMap::new();
        for (id, p) in &dst_paths {
            dst_by_path.entry(p.as_str()).or_insert(id.as_str());
        }
        let mut dst_rows: HashMap<String, usize> = HashMap::new();
        if let Some(t) = table(&self.out, "PROJWBS") {
            let id = Col::of(t, "wbs_id");
            for (i, r) in t.rows.iter().enumerate() {
                dst_rows.entry(id.get(r).to_string()).or_insert(i);
            }
        }
        for (src_id, path) in ordered {
            let Some(row) = src_rows.get(src_id.as_str()).map(|r| (*r).clone()) else {
                continue;
            };
            let found = dst_by_path.get(path.as_str()).map(|id| id.to_string());
            let parent = src.value(&row, "parent_wbs_id").to_string();
            let parent = self
                .wbs
                .get(&parent)
                .cloned()
                .unwrap_or_else(|| dst_root.clone());
            match (found, action) {
                (Some(id), Action::UpdateExisting) => {
                    let t = self.out.table_mut("PROJWBS");
                    let i = dst_rows[&id];
                    let changed = update_row(
                        t,
                        i,
                        src,
                        &row,
                        &[
                            "wbs_id",
                            "proj_id",
                            "parent_wbs_id",
                            "obs_id",
                            "guid",
                            "proj_node_flag",
                        ],
                        &[],
                    );
                    let c = self.report.c(DataType::Wbs);
                    if changed {
                        c.updated += 1
                    } else {
                        c.unchanged += 1
                    }
                    self.wbs.insert(src_id, id);
                }
                (Some(id), action) => {
                    let c = self.report.c(DataType::Wbs);
                    if action == Action::DoNotImport {
                        c.skipped += 1
                    } else {
                        c.kept += 1
                    }
                    self.wbs.insert(src_id, id);
                }
                (None, Action::DoNotImport) => {
                    // Activities under a new WBS land on the nearest existing ancestor.
                    self.report.c(DataType::Wbs).skipped += 1;
                    self.wbs.insert(src_id, parent);
                }
                (None, _) => {
                    let new_id = self.new_id("PROJWBS", "wbs_id");
                    copy_row(
                        self.out.table_mut("PROJWBS"),
                        src,
                        &row,
                        &[
                            ("wbs_id", new_id.clone()),
                            ("proj_id", self.t.to_string()),
                            ("parent_wbs_id", parent),
                            ("obs_id", root_obs.clone()),
                            ("proj_node_flag", "N".into()),
                            ("guid", String::new()),
                        ],
                    );
                    self.report.c(DataType::Wbs).added += 1;
                    self.wbs.insert(src_id, new_id);
                }
            }
        }
    }

    fn activities(&mut self) {
        let Some(src) = table(self.src, "TASK") else {
            return;
        };
        let config = self.cfg.get(DataType::Activities);
        let fallback_cal = self.project_default_calendar();
        let root = Self::wbs_paths(&self.out, self.t).0.unwrap_or_default();
        let rows: Vec<Vec<String>> = src
            .rows
            .iter()
            .filter(|r| num(src.value(r, "proj_id")) == self.s)
            .cloned()
            .collect();
        let mut missing_cal = 0;
        let mut missing_rsrc = 0;
        let mut file_codes = HashSet::new();
        // The target project's activities by Activity ID (the first row of
        // each, as a scan from the top found), and its child rows by activity.
        let mut by_code: HashMap<String, usize> = HashMap::new();
        if let Some(t) = table(&self.out, "TASK") {
            let (proj, code) = (Col::of(t, "proj_id"), Col::of(t, "task_code"));
            for (i, r) in t.rows.iter().enumerate() {
                if num(proj.get(r)) == self.t {
                    by_code.entry(code.get(r).to_string()).or_insert(i);
                }
            }
        }
        self.index_children();
        for row in rows {
            let src_id = src.value(&row, "task_id").to_string();
            self.file_tasks.insert(src_id.clone());
            let code = src.value(&row, "task_code").to_string();
            file_codes.insert(code.clone());
            let found = by_code.get(&code).copied();
            let cal = src.value(&row, "clndr_id");
            let cal = self.cal.get(cal).cloned().unwrap_or_else(|| {
                missing_cal += 1;
                fallback_cal.clone()
            });
            let rsrc = src.value(&row, "rsrc_id");
            let rsrc = if rsrc.is_empty() {
                String::new()
            } else {
                self.rsrc.get(rsrc).cloned().unwrap_or_else(|| {
                    missing_rsrc += 1;
                    String::new()
                })
            };
            let wbs = self
                .wbs
                .get(src.value(&row, "wbs_id"))
                .cloned()
                .unwrap_or_else(|| root.clone());
            let overrides = vec![
                ("proj_id", self.t.to_string()),
                ("wbs_id", wbs),
                ("clndr_id", cal),
                ("rsrc_id", rsrc),
            ];
            match (found, config.action) {
                (Some(i), Action::UpdateExisting) => {
                    let t = self.out.table_mut("TASK");
                    let changed = update_row(t, i, src, &row, &["task_id", "guid"], &overrides);
                    let id = t.value(&t.rows[i], "task_id").to_string();
                    self.replace_children(&src_id, &id);
                    let c = self.report.c(DataType::Activities);
                    if changed {
                        c.updated += 1
                    } else {
                        c.unchanged += 1
                    }
                    self.task.insert(src_id, id);
                }
                (Some(i), action) => {
                    let t = self.out.table("TASK").expect("found");
                    let id = t.value(&t.rows[i], "task_id").to_string();
                    let c = self.report.c(DataType::Activities);
                    if action == Action::DoNotImport {
                        c.skipped += 1
                    } else {
                        c.kept += 1
                    }
                    self.task.insert(src_id, id);
                }
                (None, Action::DoNotImport) => self.report.c(DataType::Activities).skipped += 1,
                (None, _) => {
                    let id = self.new_id("TASK", "task_id");
                    let mut values = overrides;
                    values.push(("task_id", id.clone()));
                    values.push(("guid", String::new()));
                    let i = copy_row(self.out.table_mut("TASK"), src, &row, &values);
                    by_code.entry(code).or_insert(i);
                    self.replace_children(&src_id, &id);
                    self.report.c(DataType::Activities).added += 1;
                    self.task.insert(src_id, id);
                }
            }
        }
        for (name, children) in std::mem::take(&mut self.children) {
            remove_rows(&mut self.out, name, children.doomed);
        }
        if missing_cal > 0 {
            self.report.messages.push(format!(
                "{missing_cal} activities use a calendar that was not imported; they use the project calendar."
            ));
        }
        if missing_rsrc > 0 {
            self.report.messages.push(format!(
                "{missing_rsrc} activities have a primary resource that was not imported; it was cleared."
            ));
        }
        if config.delete && config.action != Action::DoNotImport {
            let doomed: Vec<String> = table(&self.out, "TASK")
                .map(|t| {
                    t.rows
                        .iter()
                        .filter(|r| {
                            num(t.value(r, "proj_id")) == self.t
                                && !file_codes.contains(t.value(r, "task_code"))
                        })
                        .map(|r| t.value(r, "task_id").to_string())
                        .collect()
                })
                .unwrap_or_default();
            self.report.c(DataType::Activities).deleted += doomed.len();
            delete_activities(&mut self.out, self.t, &doomed);
        }
    }

    /// Steps, expenses and notes follow their activity: the file's rows replace the
    /// workspace rows of an updated or inserted activity.
    fn replace_children(&mut self, src_task: &str, task: &str) {
        for (name, key) in ACTIVITY_CHILDREN {
            let Some(src) = table(self.src, name) else {
                continue;
            };
            let children = self.children.entry(name).or_default();
            let rows: Vec<Vec<String>> = children
                .src
                .get(src_task)
                .map(|rows| rows.iter().map(|&i| src.rows[i].clone()).collect())
                .unwrap_or_default();
            let old = children.out.remove(task).unwrap_or_default();
            children.doomed.extend(old);
            for row in rows {
                let mut values = vec![
                    ("task_id", task.to_string()),
                    ("proj_id", self.t.to_string()),
                ];
                if let Some(key) = key {
                    values.push((key, self.new_id(name, key)));
                }
                if src.field("wbs_id").is_some() {
                    let w = src.value(&row, "wbs_id");
                    values.push(("wbs_id", self.wbs.get(w).cloned().unwrap_or_default()));
                }
                if name == "PROJCOST" {
                    values.extend(self.reference_overrides(
                        src,
                        &row,
                        &[("acct_id", "ACCOUNT"), ("cost_type_id", "COSTTYPE")],
                    ));
                }
                let i = copy_row(self.out.table_mut(name), src, &row, &values);
                let children = self.children.entry(name).or_default();
                children.out.entry(task.to_string()).or_default().push(i);
            }
        }
    }

    /// Index the activity child tables for [`Self::replace_children`].
    fn index_children(&mut self) {
        self.children.clear();
        for (name, _) in ACTIVITY_CHILDREN {
            let mut children = Children::default();
            if let Some(src) = table(self.src, name) {
                let task = Col::of(src, "task_id");
                for (i, r) in src.rows.iter().enumerate() {
                    children
                        .src
                        .entry(task.get(r).to_string())
                        .or_default()
                        .push(i);
                }
            }
            if let Some(t) = table(&self.out, name) {
                let (task, proj) = (Col::of(t, "task_id"), Col::of(t, "proj_id"));
                for (i, r) in t.rows.iter().enumerate() {
                    if num(proj.get(r)) == self.t {
                        children
                            .out
                            .entry(task.get(r).to_string())
                            .or_default()
                            .push(i);
                    }
                }
            }
            self.children.insert(name, children);
        }
    }

    fn relationships(&mut self) {
        let config = self.cfg.get(DataType::Relationships);
        let Some(src) = table(self.src, "TASKPRED") else {
            return;
        };
        if config.action == Action::DoNotImport {
            self.report.c(DataType::Relationships).skipped += src
                .rows
                .iter()
                .filter(|r| num(src.value(r, "proj_id")) == self.s)
                .count();
            return;
        }
        let mut external = 0;
        let mut matched = HashSet::new();
        let rows: Vec<Vec<String>> = src
            .rows
            .iter()
            .filter(|r| {
                num(src.value(r, "proj_id")) == self.s
                    || num(src.value(r, "pred_proj_id")) == self.s
            })
            .cloned()
            .collect();
        // Target relationships by (predecessor, successor, type), in table
        // order. Updates keep those three, and added rows are matched at once.
        let mut existing: HashMap<(String, String, String), Vec<usize>> = HashMap::new();
        if let Some(t) = table(&self.out, "TASKPRED") {
            let (p, s, k) = (
                Col::of(t, "pred_task_id"),
                Col::of(t, "task_id"),
                Col::of(t, "pred_type"),
            );
            for (i, r) in t.rows.iter().enumerate() {
                existing
                    .entry((p.get(r).into(), s.get(r).into(), k.get(r).into()))
                    .or_default()
                    .push(i);
            }
        }
        for row in rows {
            let local = num(src.value(&row, "proj_id")) == self.s
                && num(src.value(&row, "pred_proj_id")) == self.s;
            let pred = self.task.get(src.value(&row, "pred_task_id")).cloned();
            let succ = self.task.get(src.value(&row, "task_id")).cloned();
            let (Some(pred), Some(succ), true) = (pred, succ, local) else {
                external += 1;
                continue;
            };
            let kind = src.value(&row, "pred_type").to_string();
            let found = table(&self.out, "TASKPRED").and_then(|t| {
                let id = Col::of(t, "task_pred_id");
                existing
                    .get(&(pred.clone(), succ.clone(), kind))
                    .and_then(|rows| {
                        rows.iter()
                            .copied()
                            .find(|&i| !matched.contains(id.get(&t.rows[i])))
                    })
            });
            let overrides = vec![
                ("pred_task_id", pred.clone()),
                ("task_id", succ.clone()),
                ("proj_id", self.t.to_string()),
                ("pred_proj_id", self.t.to_string()),
            ];
            match (found, config.action) {
                (Some(i), Action::UpdateExisting) => {
                    let t = self.out.table_mut("TASKPRED");
                    let changed =
                        update_row(t, i, src, &row, &["task_pred_id", "guid"], &overrides);
                    matched.insert(t.value(&t.rows[i], "task_pred_id").to_string());
                    let c = self.report.c(DataType::Relationships);
                    if changed {
                        c.updated += 1
                    } else {
                        c.unchanged += 1
                    }
                }
                (Some(i), _) => {
                    let t = self.out.table("TASKPRED").expect("found");
                    matched.insert(t.value(&t.rows[i], "task_pred_id").to_string());
                    self.report.c(DataType::Relationships).kept += 1;
                }
                (None, _) => {
                    let id = self.new_id("TASKPRED", "task_pred_id");
                    let mut values = overrides;
                    values.push(("task_pred_id", id.clone()));
                    copy_row(self.out.table_mut("TASKPRED"), src, &row, &values);
                    matched.insert(id);
                    self.report.c(DataType::Relationships).added += 1;
                }
            }
        }
        if external > 0 {
            self.report.messages.push(format!(
                "{external} relationships to activities outside the imported project were not imported."
            ));
        }
        if config.delete {
            // Only links between activities that are both in the file are candidates.
            let in_file: HashSet<String> = self
                .file_tasks
                .iter()
                .filter_map(|s| self.task.get(s).cloned())
                .collect();
            let doomed = rows_where(&self.out, "TASKPRED", |t, r| {
                num(t.value(r, "proj_id")) == self.t
                    && num(t.value(r, "pred_proj_id")) == self.t
                    && in_file.contains(t.value(r, "pred_task_id"))
                    && in_file.contains(t.value(r, "task_id"))
                    && !matched.contains(t.value(r, "task_pred_id"))
            });
            self.report.c(DataType::Relationships).deleted += doomed.len();
            remove_rows(&mut self.out, "TASKPRED", doomed);
        }
    }

    fn assignments(&mut self) {
        let config = self.cfg.get(DataType::Assignments);
        let Some(src) = table(self.src, "TASKRSRC") else {
            return;
        };
        let rows: Vec<Vec<String>> = src
            .rows
            .iter()
            .filter(|r| num(src.value(r, "proj_id")) == self.s)
            .cloned()
            .collect();
        if config.action == Action::DoNotImport {
            self.report.c(DataType::Assignments).skipped += rows.len();
            return;
        }
        let mut matched = HashSet::new();
        let mut unknown = 0;
        // The target project's assignments by (activity, resource), in table
        // order. Updates keep those two, and added rows are matched at once.
        let mut existing: HashMap<(String, String), Vec<usize>> = HashMap::new();
        if let Some(t) = table(&self.out, "TASKRSRC") {
            let (task, rsrc, proj) = (
                Col::of(t, "task_id"),
                Col::of(t, "rsrc_id"),
                Col::of(t, "proj_id"),
            );
            for (i, r) in t.rows.iter().enumerate() {
                if num(proj.get(r)) == self.t {
                    existing
                        .entry((task.get(r).into(), rsrc.get(r).into()))
                        .or_default()
                        .push(i);
                }
            }
        }
        for row in rows {
            let Some(task) = self.task.get(src.value(&row, "task_id")).cloned() else {
                self.report.c(DataType::Assignments).skipped += 1;
                continue;
            };
            let Some(rsrc) = self.rsrc.get(src.value(&row, "rsrc_id")).cloned() else {
                unknown += 1;
                self.report.c(DataType::Assignments).skipped += 1;
                continue;
            };
            let found = table(&self.out, "TASKRSRC").and_then(|t| {
                let id = Col::of(t, "taskrsrc_id");
                existing
                    .get(&(task.clone(), rsrc.clone()))
                    .and_then(|rows| {
                        rows.iter()
                            .copied()
                            .find(|&i| !matched.contains(id.get(&t.rows[i])))
                    })
            });
            let mut overrides = vec![
                ("task_id", task.clone()),
                ("rsrc_id", rsrc.clone()),
                ("proj_id", self.t.to_string()),
            ];
            if found.is_none() || config.action == Action::UpdateExisting {
                overrides.extend(self.reference_overrides(
                    src,
                    &row,
                    &[
                        ("role_id", "ROLES"),
                        ("acct_id", "ACCOUNT"),
                        ("curv_id", "RSRCCURVDATA"),
                    ],
                ));
            }
            match (found, config.action) {
                (Some(i), Action::UpdateExisting) => {
                    let t = self.out.table_mut("TASKRSRC");
                    let changed = update_row(t, i, src, &row, &["taskrsrc_id", "guid"], &overrides);
                    matched.insert(t.value(&t.rows[i], "taskrsrc_id").to_string());
                    let c = self.report.c(DataType::Assignments);
                    if changed {
                        c.updated += 1
                    } else {
                        c.unchanged += 1
                    }
                }
                (Some(i), _) => {
                    let t = self.out.table("TASKRSRC").expect("found");
                    matched.insert(t.value(&t.rows[i], "taskrsrc_id").to_string());
                    self.report.c(DataType::Assignments).kept += 1;
                }
                (None, _) => {
                    let id = self.new_id("TASKRSRC", "taskrsrc_id");
                    let mut values = overrides;
                    values.push(("taskrsrc_id", id.clone()));
                    values.push(("guid", String::new()));
                    copy_row(self.out.table_mut("TASKRSRC"), src, &row, &values);
                    matched.insert(id);
                    self.report.c(DataType::Assignments).added += 1;
                }
            }
        }
        if unknown > 0 {
            self.report.messages.push(format!(
                "{unknown} resource assignments name a resource that was not imported; they were skipped."
            ));
        }
        if config.delete {
            let in_file: HashSet<String> = self
                .file_tasks
                .iter()
                .filter_map(|s| self.task.get(s).cloned())
                .collect();
            let doomed = rows_where(&self.out, "TASKRSRC", |t, r| {
                num(t.value(r, "proj_id")) == self.t
                    && in_file.contains(t.value(r, "task_id"))
                    && !matched.contains(t.value(r, "taskrsrc_id"))
            });
            self.report.c(DataType::Assignments).deleted += doomed.len();
            remove_rows(&mut self.out, "TASKRSRC", doomed);
        }
    }

    fn code_assignments(&mut self) {
        let action = self.cfg.get(DataType::ActivityCodes).action;
        let Some(src) = table(self.src, "TASKACTV") else {
            return;
        };
        let mut count = 0;
        let rows: Vec<Vec<String>> = src
            .rows
            .iter()
            .filter(|r| self.file_tasks.contains(src.value(r, "task_id")))
            .cloned()
            .collect();
        // Target code assignments by (activity, code type), kept current as
        // rows are replaced and added; replaced rows go when the pass ends.
        let mut existing: HashMap<(String, String), Vec<usize>> = HashMap::new();
        if let Some(t) = table(&self.out, "TASKACTV") {
            let (task, kind) = (Col::of(t, "task_id"), Col::of(t, "actv_code_type_id"));
            for (i, r) in t.rows.iter().enumerate() {
                existing
                    .entry((task.get(r).into(), kind.get(r).into()))
                    .or_default()
                    .push(i);
            }
        }
        let mut doomed = vec![];
        for row in rows {
            let (Some(task), Some(kind), Some(code)) = (
                self.task.get(src.value(&row, "task_id")).cloned(),
                self.code_type
                    .get(src.value(&row, "actv_code_type_id"))
                    .cloned(),
                self.code.get(src.value(&row, "actv_code_id")).cloned(),
            ) else {
                continue;
            };
            let key = (task.clone(), kind.clone());
            let slot = existing.entry(key.clone()).or_default();
            if !slot.is_empty() {
                let t = self.out.table("TASKACTV").expect("rows exist");
                let column = Col::of(t, "actv_code_id");
                let same = slot.iter().any(|&i| column.get(&t.rows[i]) == code);
                if same || action != Action::UpdateExisting {
                    continue;
                }
                doomed.append(slot);
            }
            let i = copy_row(
                self.out.table_mut("TASKACTV"),
                src,
                &row,
                &[
                    ("task_id", task),
                    ("actv_code_type_id", kind),
                    ("actv_code_id", code),
                    ("proj_id", self.t.to_string()),
                ],
            );
            existing.entry(key).or_default().push(i);
            count += 1;
        }
        remove_rows(&mut self.out, "TASKACTV", doomed);
        if count > 0 {
            self.report
                .messages
                .push(format!("{count} activity code assignments imported."));
        }
    }

    fn udf_values(&mut self) {
        let action = self.cfg.get(DataType::Udfs).action;
        let (Some(src), Some(types)) = (table(self.src, "UDFVALUE"), table(self.src, "UDFTYPE"))
        else {
            return;
        };
        // The table of each field, first row of an id winning as a scan did.
        let mut tables: HashMap<String, String> = HashMap::new();
        {
            let (id, name) = (Col::of(types, "udf_type_id"), Col::of(types, "table_name"));
            for r in &types.rows {
                tables
                    .entry(id.get(r).to_string())
                    .or_insert_with(|| name.get(r).to_ascii_uppercase());
            }
        }
        let table_of = |id: &str| tables.get(id).cloned().unwrap_or_default();
        // Target values by (field, record), kept current as values are
        // replaced and added; replaced rows go when the pass ends.
        let mut existing: HashMap<(String, String), Vec<usize>> = HashMap::new();
        if let Some(t) = table(&self.out, "UDFVALUE") {
            let (kind, fk) = (Col::of(t, "udf_type_id"), Col::of(t, "fk_id"));
            for (i, r) in t.rows.iter().enumerate() {
                existing
                    .entry((kind.get(r).into(), fk.get(r).into()))
                    .or_default()
                    .push(i);
            }
        }
        let mut doomed = vec![];
        let mut count = 0;
        for row in src.rows.clone() {
            let Some(kind) = self.udf.get(src.value(&row, "udf_type_id")).cloned() else {
                continue;
            };
            let fk = src.value(&row, "fk_id");
            let mapped = match table_of(src.value(&row, "udf_type_id")).as_str() {
                "TASK" if self.file_tasks.contains(fk) => self.task.get(fk).cloned(),
                "PROJWBS" => self
                    .wbs
                    .get(fk)
                    .cloned()
                    .filter(|_| num(src.value(&row, "proj_id")) == self.s),
                "PROJECT" if num(fk) == self.s => Some(self.t.to_string()),
                _ => None,
            };
            let Some(fk) = mapped else {
                continue;
            };
            let key = (kind.clone(), fk.clone());
            let slot = existing.entry(key.clone()).or_default();
            if !slot.is_empty() {
                if action != Action::UpdateExisting {
                    continue;
                }
                let t = self.out.table("UDFVALUE").expect("rows exist");
                let same = slot.len() == 1 && {
                    let r = &t.rows[slot[0]];
                    ["udf_date", "udf_number", "udf_text", "udf_code_id"]
                        .iter()
                        .all(|f| t.value(r, f) == src.value(&row, f))
                };
                if same {
                    continue;
                }
                doomed.append(slot);
            }
            let i = copy_row(
                self.out.table_mut("UDFVALUE"),
                src,
                &row,
                &[
                    ("udf_type_id", kind),
                    ("fk_id", fk),
                    ("proj_id", self.t.to_string()),
                ],
            );
            existing.entry(key).or_default().push(i);
            count += 1;
        }
        remove_rows(&mut self.out, "UDFVALUE", doomed);
        if count > 0 {
            self.report
                .messages
                .push(format!("{count} user defined field values imported."));
        }
    }

    fn notes(&mut self) {
        let action = self.cfg.get(DataType::Notebooks).action;
        let mut count = 0;
        for (name, id_field, owner, text_field) in [
            ("TASKMEMO", "memo_id", "task_id", "task_memo"),
            ("WBSMEMO", "wbs_memo_id", "wbs_id", "wbs_memo"),
        ] {
            let Some(src) = table(self.src, name) else {
                continue;
            };
            // Target entries by (owner, notebook topic), kept current as
            // entries are replaced and added; replaced rows go at the end.
            let mut existing: HashMap<(String, String), Vec<usize>> = HashMap::new();
            if let Some(t) = table(&self.out, name) {
                let (fk, kind) = (Col::of(t, owner), Col::of(t, "memo_type_id"));
                for (i, r) in t.rows.iter().enumerate() {
                    existing
                        .entry((fk.get(r).into(), kind.get(r).into()))
                        .or_default()
                        .push(i);
                }
            }
            let mut doomed = vec![];
            for row in src.rows.clone() {
                if num(src.value(&row, "proj_id")) != self.s {
                    continue;
                }
                let Some(kind) = self.memo.get(src.value(&row, "memo_type_id")).cloned() else {
                    continue;
                };
                let fk = src.value(&row, owner);
                let mapped = if owner == "task_id" {
                    self.task.get(fk).cloned()
                } else {
                    self.wbs.get(fk).cloned()
                };
                let Some(fk) = mapped else {
                    continue;
                };
                let key = (fk.clone(), kind.clone());
                let slot = existing.entry(key.clone()).or_default();
                if !slot.is_empty() {
                    if action != Action::UpdateExisting {
                        continue;
                    }
                    let t = self.out.table(name).expect("rows exist");
                    if slot.len() == 1
                        && t.value(&t.rows[slot[0]], text_field) == src.value(&row, text_field)
                    {
                        continue;
                    }
                    doomed.append(slot);
                }
                let new_id = self.new_id(name, id_field);
                let i = copy_row(
                    self.out.table_mut(name),
                    src,
                    &row,
                    &[
                        (id_field, new_id),
                        (owner, fk),
                        ("memo_type_id", kind),
                        ("proj_id", self.t.to_string()),
                    ],
                );
                existing.entry(key).or_default().push(i);
                count += 1;
            }
            remove_rows(&mut self.out, name, doomed);
        }
        if count > 0 {
            self.report
                .messages
                .push(format!("{count} notebook entries imported."));
        }
    }

    /// An activity update carries the file's data date and schedule dates.
    fn project_dates(&mut self) {
        if self.cfg.get(DataType::Activities).action != Action::UpdateExisting {
            return;
        }
        let Some(src) = table(self.src, "PROJECT") else {
            return;
        };
        let Some(row) = src
            .rows
            .iter()
            .find(|r| num(src.value(r, "proj_id")) == self.s)
            .cloned()
        else {
            return;
        };
        let out = self.out.table_mut("PROJECT");
        let Some(i) = out
            .rows
            .iter()
            .position(|r| num(out.value(r, "proj_id")) == self.t)
        else {
            return;
        };
        for field in ["last_recalc_date", "scd_end_date", "last_schedule_date"] {
            let value = src.value(&row, field).to_string();
            if !value.is_empty() && out.value(&out.rows[i], field) != value {
                out.set(i, field, value.clone());
                if field == "last_recalc_date" {
                    self.report
                        .messages
                        .push(format!("Data date updated to {value}."));
                }
            }
        }
    }
}

/// Add Into Existing Project: copy every activity of `source_pid` into
/// `target_pid` beside the activities already there. An activity whose ID
/// the target project already uses gets the next free `ID-n`; nothing in the
/// target project is updated or deleted, and its dates and settings stay.
/// WBS elements are matched by code path (new ones are added), and shared
/// data follows `config` as for Update Existing Project.
pub fn add_into_project(
    target: &Document,
    target_pid: i64,
    source: &Document,
    source_pid: i64,
    config: &ImportConfig,
) -> Result<(Document, MergeReport), String> {
    let taken: HashSet<String> = target.table("TASK").map_or_else(HashSet::new, |t| {
        t.rows
            .iter()
            .filter(|r| num(t.value(r, "proj_id")) == target_pid)
            .map(|r| t.value(r, "task_code").to_string())
            .collect()
    });
    let mut src = source.clone();
    let mut renamed = 0;
    if let Some(t) = src.tables.iter_mut().find(|t| t.name == "TASK") {
        let mut used: HashSet<String> = taken.clone();
        used.extend(
            t.rows
                .iter()
                .filter(|r| num(t.value(r, "proj_id")) == source_pid)
                .map(|r| t.value(r, "task_code").to_string()),
        );
        for i in 0..t.rows.len() {
            if num(t.value(&t.rows[i], "proj_id")) != source_pid {
                continue;
            }
            let code = t.value(&t.rows[i], "task_code").to_string();
            if !taken.contains(&code) {
                continue;
            }
            let fresh = (1..)
                .map(|n| format!("{code}-{n}"))
                .find(|c| !used.contains(c))
                .expect("a free activity ID");
            used.insert(fresh.clone());
            t.set(i, "task_code", fresh);
            renamed += 1;
        }
    }
    let mut cfg = config.clone();
    for t in [
        DataType::Activities,
        DataType::Relationships,
        DataType::Assignments,
    ] {
        if cfg.get(t).action != Action::DoNotImport {
            cfg.set(t, Action::KeepExisting, false);
        }
    }
    if cfg.get(DataType::Wbs).action == Action::UpdateExisting {
        cfg.set(DataType::Wbs, Action::KeepExisting, false);
    }
    let (out, mut report) = merge_project(target, target_pid, &src, source_pid, &cfg)?;
    if renamed > 0 {
        report.messages.push(format!(
            "{renamed} activities took a new activity ID because the project already used theirs."
        ));
    }
    Ok((out, report))
}

/// Delete activities and every row that refers to them.
pub fn delete_activities(doc: &mut Document, pid: i64, task_ids: &[String]) {
    let ids: Vec<i64> = task_ids.iter().map(|s| num(s)).collect();
    openxer_xer::delete::delete_activity_rows(doc, pid, &ids);
}
