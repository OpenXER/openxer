//! Update Baseline: carry data from the open project into one of its
//! baselines, by the options of the Update Baseline dialog, and describe
//! every change it makes.
//!
//! [`update_baseline`] works on a copy of the baseline and returns the copy
//! with the [`Preview`] of what changed: the Preview window and a dry run show
//! that list, and Update stores the copy. Both come from the same call, so a
//! preview is exactly what an update with the same options does.
//!
//! Matching: a project activity and a baseline activity are the same when
//! they carry the same GUID (a baseline copy keeps its activities' GUIDs, so
//! an activity renumbered after the baseline was taken still matches); the
//! rest are matched by Activity ID. Assignments within a matched activity are
//! matched the same way (GUID, then resource and role in order), WBS elements
//! by GUID, then by their code path.
//!
//! The options, in the dialog's order:
//! - General: project details, project UDFs, risks, issues and thresholds
//!   with their UDFs, work products and documents with their UDFs, WBS UDFs.
//!   "Compare all records" and "Run optimized" are shown but have no
//!   effect: every update compares all data in full.
//! - Activities: all activities or those passing one filter; add new
//!   activities (with their data, without actuals); delete baseline
//!   activities no longer in the project; update the activities both have.
//! - Activity: what an update of a matched activity carries.
//! - Resource Assignment: update or add assignments of matched activities,
//!   each with or without budgeted and actual units and costs; their UDFs and
//!   codes.
//!
//! WBS elements and project calendars and activity codes that carried data
//! needs are added to the baseline; the shared dictionaries (calendars,
//! resources, codes, UDF types) get the rows they lack. Nothing else of the
//! baseline changes.
use crate::util::{copy_row, equivalent, next_id, num, remove_rows, KeySpec};
use crate::xer_merge::delete_activities;
use openxer_xer::{Document, Table};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

/// Every option of the Update Baseline dialog. The defaults are the
/// dialog's first-opening defaults.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UpdateOptions {
    // General
    pub project_details: bool,
    pub project_udfs: bool,
    pub risks_issues: bool,
    pub risk_udfs: bool,
    pub issue_udfs: bool,
    pub documents: bool,
    pub document_udfs: bool,
    pub wbs_udfs: bool,
    /// Shown, never in effect: OpenXER does not keep a change date per
    /// record, so every update compares in full.
    pub ignore_last_update: bool,
    /// Shown, never in effect (a database tuning switch elsewhere).
    pub run_optimized: bool,
    pub log_to_file: bool,
    /// The log file; blank leaves it to the caller's default.
    pub log_path: String,
    // Activities
    /// "Only activities in filter:" chosen.
    pub use_filter: bool,
    /// The filters ticked in the picker: keys (`std:` or `user:` in the
    /// app), or names.
    pub filters: Vec<String>,
    /// An activity must pass every ticked filter (true) or any of them.
    pub filter_match_all: bool,
    pub add_new: bool,
    pub delete_missing: bool,
    pub update_existing: bool,
    // Activity
    pub general: bool,
    pub relationships: bool,
    pub constraints: bool,
    pub dates: bool,
    pub wbs: bool,
    pub risks: bool,
    pub activity_udfs: bool,
    pub codes: bool,
    pub notebooks: bool,
    pub steps: bool,
    pub step_udfs: bool,
    pub expenses: bool,
    pub expense_udfs: bool,
    /// Activities without resource assignments: budgeted units and cost.
    pub budget_no_assignments: bool,
    /// Activities without resource assignments: actual units and cost.
    pub actual_no_assignments: bool,
    // Resource Assignment
    pub update_assignments: bool,
    pub update_assignment_budget: bool,
    pub update_assignment_actual: bool,
    pub add_assignments: bool,
    pub add_assignment_budget: bool,
    pub add_assignment_actual: bool,
    pub assignment_udfs: bool,
    pub assignment_codes: bool,
}

impl Default for UpdateOptions {
    fn default() -> Self {
        UpdateOptions {
            project_details: true,
            project_udfs: false,
            risks_issues: false,
            risk_udfs: false,
            issue_udfs: false,
            documents: false,
            document_udfs: false,
            wbs_udfs: false,
            ignore_last_update: false,
            run_optimized: false,
            log_to_file: true,
            log_path: String::new(),
            use_filter: false,
            filters: vec![],
            filter_match_all: true,
            add_new: false,
            delete_missing: false,
            update_existing: true,
            general: true,
            relationships: false,
            constraints: false,
            dates: false,
            wbs: true,
            risks: false,
            activity_udfs: false,
            codes: true,
            notebooks: true,
            steps: true,
            step_udfs: false,
            expenses: false,
            expense_udfs: false,
            budget_no_assignments: false,
            actual_no_assignments: false,
            update_assignments: false,
            update_assignment_budget: false,
            update_assignment_actual: false,
            add_assignments: true,
            add_assignment_budget: true,
            add_assignment_actual: true,
            assignment_udfs: false,
            assignment_codes: false,
        }
    }
}

impl UpdateOptions {
    /// The options as they act: a child whose parent is off is off, and the
    /// two options OpenXER cannot honour are off.
    pub fn effective(&self) -> UpdateOptions {
        let mut o = self.clone();
        o.project_udfs &= o.project_details;
        o.risk_udfs &= o.risks_issues;
        o.issue_udfs &= o.risks_issues;
        o.document_udfs &= o.documents;
        o.step_udfs &= o.steps;
        o.expense_udfs &= o.expenses;
        o.update_assignment_budget &= o.update_assignments;
        o.update_assignment_actual &= o.update_assignments;
        o.add_assignment_budget &= o.add_assignments;
        o.add_assignment_actual &= o.add_assignments;
        o.ignore_last_update = false;
        o.run_optimized = false;
        o
    }
}

/// What happened to one object.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum ChangeKind {
    Added,
    Deleted,
    Updated,
}

impl ChangeKind {
    pub fn label(self) -> &'static str {
        match self {
            ChangeKind::Added => "Added",
            ChangeKind::Deleted => "Deleted",
            ChangeKind::Updated => "Updated",
        }
    }
}

/// One field of a changed object: its value in the project (what the
/// baseline holds after the update) and in the baseline before it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FieldChange {
    pub field: String,
    pub project: String,
    pub baseline: String,
}

/// One changed object: the project, a WBS element or an activity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ObjectChange {
    /// Project ID, WBS code or Activity ID.
    pub object: String,
    pub kind: ChangeKind,
    /// In field name order.
    pub fields: Vec<FieldChange>,
}

/// Every change an update makes, grouped as the Preview window shows them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Preview {
    pub project: Vec<ObjectChange>,
    pub wbs: Vec<ObjectChange>,
    /// In Activity ID order.
    pub activities: Vec<ObjectChange>,
}

impl Preview {
    pub fn is_empty(&self) -> bool {
        self.project.is_empty() && self.wbs.is_empty() && self.activities.is_empty()
    }
    /// How many activities were added, deleted and updated.
    pub fn counts(&self) -> (usize, usize, usize) {
        let n = |k| self.activities.iter().filter(|c| c.kind == k).count();
        (
            n(ChangeKind::Added),
            n(ChangeKind::Deleted),
            n(ChangeKind::Updated),
        )
    }
    /// The changes as plain text lines (the log file and the CLI).
    pub fn lines(&self) -> Vec<String> {
        let mut out = vec![];
        for (group, list) in [
            ("Project", &self.project),
            ("WBS", &self.wbs),
            ("Activities", &self.activities),
        ] {
            out.push(format!("{group}: {}", list.len()));
            for c in list {
                out.push(format!("  {} {}", c.object, c.kind.label()));
                for f in &c.fields {
                    out.push(format!(
                        "    {}: project {:?}, baseline {:?}",
                        f.field, f.project, f.baseline
                    ));
                }
            }
        }
        out
    }
}

// ------------------------------------------------------------ field groups

/// General activity information.
const GENERAL: [&str; 13] = [
    "task_code",
    "task_name",
    "task_type",
    "duration_type",
    "complete_pct_type",
    "priority_type",
    "clndr_id",
    "rsrc_id",
    "location_id",
    "est_wt",
    "lock_plan_flag",
    "auto_compute_act_flag",
    "rev_fdbk_flag",
];

const CONSTRAINTS: [&str; 4] = ["cstr_type", "cstr_date", "cstr_type2", "cstr_date2"];

/// Dates, durations, data date (with the schedule results that go with
/// the dates).
const DATES: [&str; 27] = [
    "target_drtn_hr_cnt",
    "remain_drtn_hr_cnt",
    "act_start_date",
    "act_end_date",
    "target_start_date",
    "target_end_date",
    "early_start_date",
    "early_end_date",
    "late_start_date",
    "late_end_date",
    "restart_date",
    "reend_date",
    "rem_late_start_date",
    "rem_late_end_date",
    "expect_end_date",
    "suspend_date",
    "resume_date",
    "total_float_hr_cnt",
    "free_float_hr_cnt",
    "status_code",
    "phys_complete_pct",
    "float_path",
    "float_path_order",
    "driving_path_flag",
    "crt_path_num",
    "external_early_start_date",
    "external_late_end_date",
];

/// Budgeted units of an activity without assignments (it carries no cost
/// of its own in the file).
const BUDGET_UNITS: [&str; 2] = ["target_work_qty", "target_equip_qty"];
const ACTUAL_UNITS: [&str; 4] = [
    "act_work_qty",
    "act_equip_qty",
    "act_this_per_work_qty",
    "act_this_per_equip_qty",
];

/// Assignment data an update of an existing assignment carries.
const ASSIGNMENT_GENERAL: [&str; 16] = [
    "role_id",
    "acct_id",
    "rate_type",
    "cost_per_qty",
    "cost_per_qty_source_type",
    "cost_qty_link_flag",
    "curv_id",
    "ot_factor",
    "skill_level",
    "target_lag_drtn_hr_cnt",
    "target_start_date",
    "target_end_date",
    "restart_date",
    "reend_date",
    "rem_late_start_date",
    "rem_late_end_date",
];
const ASSIGNMENT_BUDGET: [&str; 3] = ["target_qty", "target_cost", "target_qty_per_hr"];
const ASSIGNMENT_ACTUAL: [&str; 8] = [
    "act_reg_qty",
    "act_ot_qty",
    "act_reg_cost",
    "act_ot_cost",
    "act_this_per_qty",
    "act_this_per_cost",
    "act_start_date",
    "act_end_date",
];

/// The PROJECT fields of "Project details": the project's own description
/// and dates; its defaults, settings and the data date are left out (the
/// data date goes with "Dates, durations, data date").
const PROJECT_DETAILS: [&str; 8] = [
    "plan_start_date",
    "plan_end_date",
    "fcst_start_date",
    "priority_num",
    "strgy_priority_num",
    "proj_url",
    "location_id",
    "web_local_root_path",
];

/// Columns never compared or copied as data.
const SKIP: [&str; 9] = [
    "guid",
    "tmpl_guid",
    "create_date",
    "update_date",
    "create_user",
    "update_user",
    "proj_id",
    "pred_proj_id",
    "last_chng_date",
];

/// Reference columns and the key they point at.
fn family(column: &str) -> String {
    let c = column.to_ascii_lowercase();
    match c.as_str() {
        "pred_task_id" => "task_id".into(),
        "parent_wbs_id" => "wbs_id".into(),
        "parent_actv_code_id" => "actv_code_id".into(),
        _ => c,
    }
}

fn skip(column: &str) -> bool {
    SKIP.iter().any(|s| s.eq_ignore_ascii_case(column))
}

/// A value in one canonical text, so equal numbers and dates compare equal.
fn canon(v: &str) -> String {
    let v = v.trim();
    if let Ok(x) = v.parse::<f64>() {
        return format!("{x}");
    }
    if let Some(m) = openxer_xer::parse_absolute(v) {
        return format!("@{m}");
    }
    v.to_string()
}

fn owned(t: &Table, r: &[String], pid: i64) -> bool {
    let v = t.value(r, "proj_id").trim();
    !v.is_empty() && num(v) == pid
}

fn rows_of(doc: &Document, table: &str, pid: i64) -> Vec<Vec<String>> {
    doc.table(table).map_or_else(Vec::new, |t| {
        t.rows
            .iter()
            .filter(|r| owned(t, r, pid))
            .cloned()
            .collect()
    })
}

/// The UDF types defined for records of `table` (TASK, PROJWBS, ...).
fn udf_types(doc: &Document, table: &str) -> HashSet<String> {
    doc.table("UDFTYPE").map_or_else(HashSet::new, |t| {
        t.rows
            .iter()
            .filter(|r| t.value(r, "table_name").eq_ignore_ascii_case(table))
            .map(|r| t.value(r, "udf_type_id").trim().to_string())
            .collect()
    })
}

type Overrides = Vec<(String, String)>;

fn as_refs(o: &Overrides) -> Vec<(&str, String)> {
    o.iter().map(|(k, v)| (k.as_str(), v.clone())).collect()
}

/// One update run: the project (`src`, project `pid`) into the baseline
/// copy `out` (project `key`).
struct Run<'a> {
    src: &'a Document,
    pid: i64,
    key: i64,
    out: Document,
    o: UpdateOptions,
    /// Source id to baseline id, by key family.
    maps: HashMap<String, HashMap<String, String>>,
    next: HashMap<(String, String), i64>,
}

impl Run<'_> {
    fn new_id(&mut self, table: &str, key: &str) -> String {
        let slot = (table.to_ascii_uppercase(), key.to_string());
        let id = match self.next.get(&slot) {
            Some(&id) => id,
            None => next_id(&self.out, table, key),
        };
        self.next.insert(slot, id + 1);
        id.to_string()
    }

    fn mapped(&self, fam: &str, v: &str) -> Option<String> {
        self.maps.get(fam)?.get(v.trim()).cloned()
    }

    fn remember(&mut self, fam: &str, src: &str, bl: &str) {
        self.maps
            .entry(fam.to_string())
            .or_default()
            .insert(src.trim().to_string(), bl.trim().to_string());
    }

    fn src_rows(&self, table: &str) -> Vec<Vec<String>> {
        rows_of(self.src, table, self.pid)
    }

    /// The baseline value for source value `v` of column `column`: mapped
    /// ids, the project id, calendars and codes the baseline may lack
    /// (added on the way); anything else as it is.
    fn value_for(&mut self, column: &str, v: &str) -> String {
        let fam = family(column);
        if v.trim().is_empty() {
            return v.to_string();
        }
        match fam.as_str() {
            "proj_id" | "pred_proj_id" => {
                if num(v) == self.pid {
                    self.key.to_string()
                } else {
                    v.to_string()
                }
            }
            "wbs_id" => self.ensure_wbs(v),
            "clndr_id" => self.ensure_calendar(v),
            "actv_code_type_id" => self.ensure_code_type(v),
            "actv_code_id" => self.ensure_code(v),
            "risk_id" => self.ensure_named("PROJRISK", "risk_id", "risk_name", v),
            "doc_id" => self.ensure_named("DOCUMENT", "doc_id", "doc_name", v),
            "issue_id" => self.mapped(&fam, v).unwrap_or_default(),
            "task_id" | "taskrsrc_id" | "proc_id" | "cost_item_id" | "thresh_id" => {
                self.mapped(&fam, v).unwrap_or_default()
            }
            _ => v.to_string(),
        }
    }

    /// Overrides for copying source row `row` of `t` into the baseline: every
    /// reference column mapped, `own` set to new values first.
    fn overrides(&mut self, t: &Table, row: &[String], own: &[(&str, String)]) -> Overrides {
        let mut out: Overrides = own
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect();
        for (k, field) in t.fields.iter().enumerate() {
            if out.iter().any(|(o, _)| o.eq_ignore_ascii_case(field)) {
                continue;
            }
            let fam = family(field);
            let reference = matches!(
                fam.as_str(),
                "proj_id"
                    | "wbs_id"
                    | "clndr_id"
                    | "actv_code_type_id"
                    | "actv_code_id"
                    | "risk_id"
                    | "doc_id"
                    | "issue_id"
                    | "task_id"
                    | "taskrsrc_id"
                    | "proc_id"
                    | "cost_item_id"
                    | "thresh_id"
            ) || field.eq_ignore_ascii_case("pred_proj_id");
            if reference {
                let v = row.get(k).cloned().unwrap_or_default();
                let mapped = self.value_for(field, &v);
                out.push((field.clone(), mapped));
            }
        }
        // A copy gets a GUID only from its own source row.
        out
    }

    /// Append source row `row` of table `name` to the baseline.
    fn copy_in(&mut self, name: &str, row: &[String], own: &[(&str, String)]) -> usize {
        let t = self.src.table(name).expect("source table").clone();
        let o = self.overrides(&t, row, own);
        copy_row(self.out.table_mut(name), &t, row, &as_refs(&o))
    }

    // ---------------------------------------------------- dictionaries

    /// Shared dictionary rows (no project) the baseline lacks.
    fn sync_dictionary(&mut self) {
        let project_types: HashSet<String> = self
            .src_rows("ACTVTYPE")
            .iter()
            .filter_map(|r| {
                let t = self.src.table("ACTVTYPE")?;
                Some(t.value(r, "actv_code_type_id").trim().to_string())
            })
            .collect();
        let names: Vec<String> = self.src.tables.iter().map(|t| t.name.clone()).collect();
        for name in names {
            let upper = name.to_ascii_uppercase();
            if matches!(
                upper.as_str(),
                "PROJECT"
                    | "TASK"
                    | "PROJWBS"
                    | "UDFVALUE"
                    | "TASKPRED"
                    | "TASKRSRC"
                    | "RSRCLEVELLIST"
                    | "TASKNOTE"
                    | "TASKFDBK"
            ) {
                continue;
            }
            let src = self.src.table(&name).expect("listed").clone();
            let shared: Vec<&Vec<String>> = src
                .rows
                .iter()
                .filter(|r| src.value(r, "proj_id").trim().is_empty())
                .filter(|r| {
                    !(upper == "ACTVCODE"
                        && project_types.contains(src.value(r, "actv_code_type_id").trim()))
                })
                .collect();
            if shared.is_empty() {
                continue;
            }
            let spec = KeySpec::of(&upper, src.fields.first().map(String::as_str));
            let have: HashSet<Vec<String>> = self.out.table(&name).map_or_else(HashSet::new, |t| {
                let reader = spec.reader(t);
                t.rows.iter().map(|r| reader.value(r)).collect()
            });
            let reader = spec.reader(&src);
            for row in shared {
                if !have.contains(&reader.value(row)) {
                    copy_row(self.out.table_mut(&name), &src, row, &[]);
                }
            }
        }
    }

    /// A project calendar of the source as the baseline's calendar of the
    /// same name (added when missing); a shared calendar keeps its id.
    fn ensure_calendar(&mut self, v: &str) -> String {
        if let Some(m) = self.mapped("clndr_id", v) {
            return m;
        }
        let Some(t) = self.src.table("CALENDAR") else {
            return v.to_string();
        };
        let Some(row) = t
            .rows
            .iter()
            .find(|r| t.value(r, "clndr_id").trim() == v.trim())
            .cloned()
        else {
            return v.to_string();
        };
        if !owned(t, &row, self.pid) {
            return v.to_string();
        }
        let name = t.value(&row, "clndr_name").to_string();
        let key = self.key;
        let found = self.out.table("CALENDAR").and_then(|o| {
            o.rows
                .iter()
                .find(|r| owned(o, r, key) && o.value(r, "clndr_name") == name)
                .map(|r| o.value(r, "clndr_id").to_string())
        });
        let id = match found {
            Some(id) => id,
            None => {
                let id = self.new_id("CALENDAR", "clndr_id");
                let t = t.clone();
                copy_row(
                    self.out.table_mut("CALENDAR"),
                    &t,
                    &row,
                    &[("clndr_id", id.clone()), ("proj_id", key.to_string())],
                );
                id
            }
        };
        self.remember("clndr_id", v, &id);
        id
    }

    fn ensure_code_type(&mut self, v: &str) -> String {
        if let Some(m) = self.mapped("actv_code_type_id", v) {
            return m;
        }
        let Some(t) = self.src.table("ACTVTYPE") else {
            return v.to_string();
        };
        let Some(row) = t
            .rows
            .iter()
            .find(|r| t.value(r, "actv_code_type_id").trim() == v.trim())
            .cloned()
        else {
            return v.to_string();
        };
        if !owned(t, &row, self.pid) {
            return v.to_string();
        }
        let name = t.value(&row, "actv_code_type").to_string();
        let key = self.key;
        let found = self.out.table("ACTVTYPE").and_then(|o| {
            o.rows
                .iter()
                .find(|r| owned(o, r, key) && o.value(r, "actv_code_type") == name)
                .map(|r| o.value(r, "actv_code_type_id").to_string())
        });
        let id = match found {
            Some(id) => id,
            None => {
                let id = self.new_id("ACTVTYPE", "actv_code_type_id");
                let t = t.clone();
                copy_row(
                    self.out.table_mut("ACTVTYPE"),
                    &t,
                    &row,
                    &[
                        ("actv_code_type_id", id.clone()),
                        ("proj_id", key.to_string()),
                        ("wbs_id", String::new()),
                    ],
                );
                id
            }
        };
        self.remember("actv_code_type_id", v, &id);
        id
    }

    /// A code value: one of a project code type maps to the baseline's
    /// value of the same short name under the mapped type.
    fn ensure_code(&mut self, v: &str) -> String {
        if let Some(m) = self.mapped("actv_code_id", v) {
            return m;
        }
        let Some(t) = self.src.table("ACTVCODE") else {
            return v.to_string();
        };
        let Some(row) = t
            .rows
            .iter()
            .find(|r| t.value(r, "actv_code_id").trim() == v.trim())
            .cloned()
        else {
            return v.to_string();
        };
        let src_type = t.value(&row, "actv_code_type_id").trim().to_string();
        let ty = self.ensure_code_type(&src_type);
        if ty == src_type {
            return v.to_string();
        }
        let short = t.value(&row, "short_name").to_string();
        let parent = t.value(&row, "parent_actv_code_id").trim().to_string();
        let found = self.out.table("ACTVCODE").and_then(|o| {
            o.rows
                .iter()
                .find(|r| {
                    o.value(r, "actv_code_type_id").trim() == ty
                        && o.value(r, "short_name") == short
                })
                .map(|r| o.value(r, "actv_code_id").to_string())
        });
        let id = match found {
            Some(id) => id,
            None => {
                let parent = if parent.is_empty() {
                    String::new()
                } else {
                    self.ensure_code(&parent)
                };
                let id = self.new_id("ACTVCODE", "actv_code_id");
                let t = t.clone();
                copy_row(
                    self.out.table_mut("ACTVCODE"),
                    &t,
                    &row,
                    &[
                        ("actv_code_id", id.clone()),
                        ("actv_code_type_id", ty),
                        ("parent_actv_code_id", parent),
                    ],
                );
                id
            }
        };
        self.remember("actv_code_id", v, &id);
        id
    }

    /// A project record named by `name_col` (a risk, a document): the
    /// baseline's record of the same name, added when missing.
    fn ensure_named(&mut self, table: &str, id_col: &str, name_col: &str, v: &str) -> String {
        if let Some(m) = self.mapped(id_col, v) {
            return m;
        }
        let Some(t) = self.src.table(table).cloned() else {
            return String::new();
        };
        let Some(row) = t
            .rows
            .iter()
            .find(|r| owned(&t, r, self.pid) && t.value(r, id_col).trim() == v.trim())
            .cloned()
        else {
            return String::new();
        };
        let name = t.value(&row, name_col).to_string();
        let key = self.key;
        let found = self.out.table(table).and_then(|o| {
            o.rows
                .iter()
                .find(|r| owned(o, r, key) && o.value(r, name_col) == name)
                .map(|r| o.value(r, id_col).to_string())
        });
        let id = match found {
            Some(id) => id,
            None => {
                let id = self.new_id(table, id_col);
                self.remember(id_col, v, &id);
                self.copy_in(table, &row, &[(id_col, id.clone())]);
                id
            }
        };
        self.remember(id_col, v, &id);
        id
    }

    // ---------------------------------------------------- WBS

    /// Code path of every WBS element of project `pid` in `doc` (the
    /// project's own node is the empty path).
    fn wbs_paths(doc: &Document, pid: i64) -> HashMap<String, String> {
        let Some(t) = doc.table("PROJWBS") else {
            return HashMap::new();
        };
        let rows: HashMap<String, (String, String, bool)> = t
            .rows
            .iter()
            .filter(|r| owned(t, r, pid))
            .map(|r| {
                (
                    t.value(r, "wbs_id").trim().to_string(),
                    (
                        t.value(r, "parent_wbs_id").trim().to_string(),
                        t.value(r, "wbs_short_name").to_string(),
                        t.value(r, "proj_node_flag") == "Y",
                    ),
                )
            })
            .collect();
        let mut out = HashMap::new();
        for id in rows.keys() {
            let mut parts = vec![];
            let mut at = id.clone();
            let mut seen = HashSet::new();
            while let Some((parent, short, root)) = rows.get(&at) {
                if *root || !seen.insert(at.clone()) {
                    break;
                }
                parts.push(short.clone());
                at = parent.clone();
            }
            parts.reverse();
            out.insert(id.clone(), parts.join("."));
        }
        out
    }

    fn match_wbs(&mut self) {
        let guid = |doc: &Document, pid: i64| -> HashMap<String, String> {
            let mut count: HashMap<String, usize> = HashMap::new();
            let mut by: HashMap<String, String> = HashMap::new();
            if let Some(t) = doc.table("PROJWBS") {
                for r in t.rows.iter().filter(|r| owned(t, r, pid)) {
                    let g = t.value(r, "guid").trim().to_string();
                    if !g.is_empty() {
                        *count.entry(g.clone()).or_default() += 1;
                        by.insert(g, t.value(r, "wbs_id").trim().to_string());
                    }
                }
            }
            by.retain(|g, _| count[g] == 1);
            by
        };
        let (sg, bg) = (guid(self.src, self.pid), guid(&self.out, self.key));
        for (g, s) in &sg {
            if let Some(b) = bg.get(g) {
                self.remember("wbs_id", s, b);
            }
        }
        let sp = Self::wbs_paths(self.src, self.pid);
        let bp = Self::wbs_paths(&self.out, self.key);
        let mut by_path: HashMap<&String, &String> = HashMap::new();
        for (id, path) in &bp {
            by_path.insert(path, id);
        }
        let taken: HashSet<String> = self
            .maps
            .get("wbs_id")
            .map(|m| m.values().cloned().collect())
            .unwrap_or_default();
        for (s, path) in &sp {
            if self.mapped("wbs_id", s).is_some() {
                continue;
            }
            if let Some(b) = by_path.get(path) {
                if !taken.contains(*b) {
                    let b = (*b).clone();
                    self.remember("wbs_id", s, &b);
                }
            }
        }
    }

    /// The baseline WBS element for source element `v`, added (with its
    /// parents) when the baseline lacks it.
    fn ensure_wbs(&mut self, v: &str) -> String {
        if let Some(m) = self.mapped("wbs_id", v) {
            return m;
        }
        let Some(t) = self.src.table("PROJWBS").cloned() else {
            return v.to_string();
        };
        let Some(row) = t
            .rows
            .iter()
            .find(|r| owned(&t, r, self.pid) && t.value(r, "wbs_id").trim() == v.trim())
            .cloned()
        else {
            return if num(v) == self.pid {
                self.key.to_string()
            } else {
                v.to_string()
            };
        };
        let id = self.new_id("PROJWBS", "wbs_id");
        self.remember("wbs_id", v, &id);
        let parent = t.value(&row, "parent_wbs_id").trim().to_string();
        let parent = self.value_for("parent_wbs_id", &parent);
        copy_row(
            self.out.table_mut("PROJWBS"),
            &t,
            &row,
            &[
                ("wbs_id", id.clone()),
                ("proj_id", self.key.to_string()),
                ("parent_wbs_id", parent),
            ],
        );
        id
    }

    // ---------------------------------------------------- matching

    /// (task id, Activity ID, GUID) of the activities of `pid` in `doc`.
    fn tasks(doc: &Document, pid: i64) -> Vec<(String, String, String)> {
        doc.table("TASK").map_or_else(Vec::new, |t| {
            t.rows
                .iter()
                .filter(|r| owned(t, r, pid))
                .map(|r| {
                    (
                        t.value(r, "task_id").trim().to_string(),
                        t.value(r, "task_code").to_string(),
                        t.value(r, "guid").trim().to_string(),
                    )
                })
                .collect()
        })
    }

    /// Pair the two sides' items by GUID (unique on both sides), then by a
    /// second key in order. Items are (id, key, guid); returns (source id,
    /// baseline id).
    fn pair(
        src: &[(String, String, String)],
        bl: &[(String, String, String)],
    ) -> Vec<(String, String)> {
        let unique = |items: &[(String, String, String)]| -> HashMap<String, String> {
            let mut count: HashMap<&str, usize> = HashMap::new();
            for i in items.iter().filter(|i| !i.2.is_empty()) {
                *count.entry(&i.2).or_default() += 1;
            }
            items
                .iter()
                .filter(|i| !i.2.is_empty() && count[i.2.as_str()] == 1)
                .map(|i| (i.2.clone(), i.0.clone()))
                .collect()
        };
        let (sg, bg) = (unique(src), unique(bl));
        let mut out = vec![];
        let mut used_s = HashSet::new();
        let mut used_b = HashSet::new();
        for s in src {
            if let (Some(_), Some(b)) = (sg.get(&s.2), bg.get(&s.2)) {
                out.push((s.0.clone(), b.clone()));
                used_s.insert(s.0.clone());
                used_b.insert(b.clone());
            }
        }
        for s in src.iter().filter(|s| !used_s.contains(&s.0)) {
            if let Some(b) = bl.iter().find(|b| !used_b.contains(&b.0) && b.1 == s.1) {
                out.push((s.0.clone(), b.0.clone()));
                used_b.insert(b.0.clone());
            }
        }
        out
    }

    // ---------------------------------------------------- copying

    /// Set `columns` of baseline row `bi` of `table` from source row `row`
    /// (references mapped).
    fn set_columns(&mut self, table: &str, bi: usize, row: &[String], columns: &[&str]) {
        let Some(src) = self.src.table(table).cloned() else {
            return;
        };
        for c in columns {
            let Some(k) = src.field(c) else {
                continue;
            };
            let raw = row.get(k).cloned().unwrap_or_default();
            let value = self.value_for(c, &raw);
            let t = self.out.table_mut(table);
            if equivalent(t.value(&t.rows[bi], c), &value) {
                continue;
            }
            if value.is_empty() && t.field(c).is_none() {
                continue;
            }
            t.set(bi, c, value);
        }
    }

    fn row_index(&self, table: &str, id_col: &str, id: &str) -> Option<usize> {
        let t = self.out.table(table)?;
        t.rows
            .iter()
            .position(|r| owned(t, r, self.key) && t.value(r, id_col).trim() == id)
    }

    /// A comparable text of rows, without their own ids, as they would
    /// read in the baseline.
    fn signature(t: &Table, rows: &[Vec<String>], own: &str) -> Vec<String> {
        let mut out: Vec<String> = rows
            .iter()
            .map(|r| {
                let mut cells: Vec<String> = t
                    .fields
                    .iter()
                    .enumerate()
                    .filter(|(_, f)| !skip(f) && !f.eq_ignore_ascii_case(own))
                    .map(|(k, f)| {
                        format!(
                            "{}={}",
                            f.to_ascii_lowercase(),
                            canon(r.get(k).map_or("", String::as_str))
                        )
                    })
                    .filter(|c| !c.ends_with('='))
                    .collect();
                cells.sort();
                cells.join("|")
            })
            .collect();
        out.sort();
        out
    }

    /// Replace the baseline rows of `table` that `belongs` selects by the
    /// source rows `src_rows` (references mapped, `own` given new ids),
    /// unless they already read the same. Returns (old own ids, new own
    /// ids) in order when replaced.
    fn replace(
        &mut self,
        table: &str,
        own: &str,
        src_rows: Vec<Vec<String>>,
        belongs: &dyn Fn(&Table, &[String]) -> bool,
        fixed: &[(&str, String)],
    ) -> Option<(Vec<String>, Vec<String>)> {
        let key = self.key;
        let old: Vec<usize> = self.out.table(table).map_or_else(Vec::new, |t| {
            t.rows
                .iter()
                .enumerate()
                .filter(|(_, r)| owned(t, r, key) && belongs(t, r))
                .map(|(i, _)| i)
                .collect()
        });
        if src_rows.is_empty() && old.is_empty() {
            return None;
        }
        // The source rows as the baseline would hold them, in a scratch table.
        let mut scratch = Table {
            name: table.to_string(),
            fields: self
                .out
                .table(table)
                .map(|t| t.fields.clone())
                .unwrap_or_default(),
            rows: vec![],
        };
        // References the baseline lacks (a calendar, a code) are added here
        // already; a row that needs one differs from the baseline's anyway.
        let src_table = self.src.table(table).cloned();
        if let Some(st) = &src_table {
            for row in &src_rows {
                let mut o = fixed.to_vec();
                if !own.is_empty() {
                    o.push((own, String::new()));
                }
                let over = self.overrides(st, row, &o);
                copy_row(&mut scratch, st, row, &as_refs(&over));
            }
        }
        let old_rows: Vec<Vec<String>> = old
            .iter()
            .map(|&i| self.out.table(table).expect("rows").rows[i].clone())
            .collect();
        let same = match self.out.table(table) {
            Some(t) => {
                let mine = Table {
                    name: t.name.clone(),
                    fields: t.fields.clone(),
                    rows: old_rows.clone(),
                };
                Self::signature(&mine, &mine.rows, own)
                    == Self::signature(&scratch, &scratch.rows, own)
            }
            None => scratch.rows.is_empty(),
        };
        if same {
            return None;
        }
        let old_ids: Vec<String> = match self.out.table(table) {
            Some(t) => old_rows
                .iter()
                .map(|r| t.value(r, own).trim().to_string())
                .collect(),
            None => vec![],
        };
        remove_rows(&mut self.out, table, old);
        let mut new_ids = vec![];
        if let Some(st) = &src_table {
            for row in &src_rows {
                let mut o = fixed.to_vec();
                if !own.is_empty() && st.field(own).is_some() {
                    let id = self.new_id(table, own);
                    self.remember(own, st.value(row, own), &id);
                    new_ids.push(id.clone());
                    o.push((own, id));
                }
                self.copy_in(table, row, &o);
            }
        }
        Some((old_ids, new_ids))
    }

    /// UDF values of records of `udf_table`: for each (source id, baseline
    /// id), the baseline's values become the source's.
    fn copy_udfs(&mut self, udf_table: &str, pairs: &[(String, String)]) {
        if pairs.is_empty() {
            return;
        }
        let types = udf_types(self.src, udf_table);
        if types.is_empty() && udf_types(&self.out, udf_table).is_empty() {
            return;
        }
        let bl_types = udf_types(&self.out, udf_table);
        let src_rows = self.src_rows("UDFVALUE");
        let Some(st) = self.src.table("UDFVALUE").cloned() else {
            // The project has no values: the baseline's go.
            let fks: HashSet<String> = pairs.iter().map(|p| p.1.clone()).collect();
            let key = self.key;
            let doomed: Vec<usize> = self.out.table("UDFVALUE").map_or_else(Vec::new, |t| {
                t.rows
                    .iter()
                    .enumerate()
                    .filter(|(_, r)| {
                        owned(t, r, key)
                            && bl_types.contains(t.value(r, "udf_type_id").trim())
                            && fks.contains(t.value(r, "fk_id").trim())
                    })
                    .map(|(i, _)| i)
                    .collect()
            });
            remove_rows(&mut self.out, "UDFVALUE", doomed);
            return;
        };
        for (s, b) in pairs {
            let mine: Vec<Vec<String>> = src_rows
                .iter()
                .filter(|r| {
                    types.contains(st.value(r, "udf_type_id").trim())
                        && st.value(r, "fk_id").trim() == s.trim()
                })
                .cloned()
                .collect();
            let (b2, bl_types2) = (b.clone(), bl_types.clone());
            self.replace(
                "UDFVALUE",
                "",
                mine,
                &move |t, r| {
                    bl_types2.contains(t.value(r, "udf_type_id").trim())
                        && t.value(r, "fk_id").trim() == b2
                },
                &[("fk_id", b.clone())],
            );
        }
    }

    /// Move UDF values of records of `udf_table` from old ids to new ids
    /// (children rebuilt with new ids keep their values); values of old ids
    /// without a new one go.
    fn carry_udfs(&mut self, udf_table: &str, old: &[String], new: &[String]) {
        let types = udf_types(&self.out, udf_table);
        if types.is_empty() || old.is_empty() {
            return;
        }
        let moved: HashMap<&String, Option<&String>> = old
            .iter()
            .enumerate()
            .map(|(i, o)| (o, new.get(i)))
            .collect();
        let key = self.key;
        let Some(t) = self.out.tables.iter_mut().find(|t| t.name == "UDFVALUE") else {
            return;
        };
        let (Some(tc), Some(fc)) = (t.field("udf_type_id"), t.field("fk_id")) else {
            return;
        };
        let pc = t.field("proj_id");
        let mut doomed = vec![];
        for (i, r) in t.rows.iter_mut().enumerate() {
            let mine = pc.is_some_and(|p| num(&r[p]) == key) && types.contains(r[tc].trim());
            if !mine {
                continue;
            }
            if let Some(to) = moved.get(&r[fc].trim().to_string()) {
                match to {
                    Some(n) => r[fc] = (*n).clone(),
                    None => doomed.push(i),
                }
            }
        }
        remove_rows(&mut self.out, "UDFVALUE", doomed);
    }

    // ---------------------------------------------------- the passes

    /// Project-level records matched by name: risks, issues, documents.
    /// Source records update or join the baseline's; baseline records the
    /// project no longer has go. Returns (source id, baseline id) pairs.
    fn sync_named(&mut self, table: &str, id_col: &str, name_col: &str) -> Vec<(String, String)> {
        let Some(st) = self.src.table(table).cloned() else {
            return vec![];
        };
        let src = self.src_rows(table);
        let key = self.key;
        let mut pairs = vec![];
        let mut kept = HashSet::new();
        for row in &src {
            let s = st.value(row, id_col).trim().to_string();
            let name = st.value(row, name_col).to_string();
            let found = self.out.table(table).and_then(|o| {
                o.rows
                    .iter()
                    .position(|r| owned(o, r, key) && o.value(r, name_col) == name)
            });
            let b = match found {
                Some(i) => {
                    let o = self.out.table(table).expect("found");
                    let b = o.value(&o.rows[i], id_col).trim().to_string();
                    self.remember(id_col, &s, &b);
                    let cols: Vec<String> = st
                        .fields
                        .iter()
                        .filter(|f| !skip(f) && !f.eq_ignore_ascii_case(id_col))
                        .cloned()
                        .collect();
                    let cols: Vec<&str> = cols.iter().map(String::as_str).collect();
                    self.set_columns(table, i, row, &cols);
                    b
                }
                None => {
                    let b = self.new_id(table, id_col);
                    self.remember(id_col, &s, &b);
                    self.copy_in(table, row, &[(id_col, b.clone())]);
                    b
                }
            };
            kept.insert(b.clone());
            pairs.push((s, b));
        }
        let doomed: Vec<usize> = self.out.table(table).map_or_else(Vec::new, |t| {
            t.rows
                .iter()
                .enumerate()
                .filter(|(_, r)| owned(t, r, key) && !kept.contains(t.value(r, id_col).trim()))
                .map(|(i, _)| i)
                .collect()
        });
        if !doomed.is_empty() {
            let gone: HashSet<String> = doomed
                .iter()
                .map(|&i| {
                    let t = self.out.table(table).expect("rows");
                    t.value(&t.rows[i], id_col).trim().to_string()
                })
                .collect();
            remove_rows(&mut self.out, table, doomed);
            // Links to a removed record go with it.
            for link in ["TASKRISK", "TASKDOC"] {
                let rows: Vec<usize> = self.out.table(link).map_or_else(Vec::new, |t| {
                    t.rows
                        .iter()
                        .enumerate()
                        .filter(|(_, r)| {
                            owned(t, r, key) && gone.contains(t.value(r, id_col).trim())
                        })
                        .map(|(i, _)| i)
                        .collect()
                });
                remove_rows(&mut self.out, link, rows);
            }
        }
        pairs
    }

    fn project_level(&mut self) {
        let o = self.o.clone();
        if o.project_details {
            if let Some(t) = self.src.table("PROJECT") {
                if let Some(row) = t
                    .rows
                    .iter()
                    .find(|r| num(t.value(r, "proj_id")) == self.pid)
                {
                    let row = row.clone();
                    if let Some(bi) = self.row_index("PROJECT", "proj_id", &self.key.to_string()) {
                        self.set_columns("PROJECT", bi, &row, &PROJECT_DETAILS);
                    }
                }
            }
        }
        if o.project_udfs {
            self.copy_udfs("PROJECT", &[(self.pid.to_string(), self.key.to_string())]);
        }
        if o.risks_issues {
            let risks = self.sync_named("PROJRISK", "risk_id", "risk_name");
            let issues = self.sync_named("PROJISSU", "issue_id", "issue_name");
            let rows = self.src_rows("PROJTHRS");
            self.replace("PROJTHRS", "thresh_id", rows, &|_, _| true, &[]);
            if o.risk_udfs {
                self.copy_udfs("PROJRISK", &risks);
            }
            if o.issue_udfs {
                self.copy_udfs("PROJISSU", &issues);
            }
        }
        if o.documents {
            let docs = self.sync_named("DOCUMENT", "doc_id", "doc_name");
            if o.document_udfs {
                self.copy_udfs("DOCUMENT", &docs);
            }
        }
    }

    /// The source rows of `table` that belong to source activity `s`.
    fn children_of(&self, table: &str, s: &str) -> Vec<Vec<String>> {
        let Some(t) = self.src.table(table) else {
            return vec![];
        };
        self.src_rows(table)
            .into_iter()
            .filter(|r| t.value(r, "task_id").trim() == s)
            .collect()
    }

    /// Update one matched activity (source `s`, baseline `b`).
    fn update_activity(&mut self, s: &str, b: &str) {
        let o = self.o.clone();
        let Some(st) = self.src.table("TASK") else {
            return;
        };
        let Some(row) = st
            .rows
            .iter()
            .find(|r| owned(st, r, self.pid) && st.value(r, "task_id").trim() == s)
            .cloned()
        else {
            return;
        };
        let Some(bi) = self.row_index("TASK", "task_id", b) else {
            return;
        };
        let mut columns: Vec<&str> = vec![];
        if o.general {
            columns.extend(GENERAL);
        }
        if o.constraints {
            columns.extend(CONSTRAINTS);
        }
        if o.dates {
            columns.extend(DATES);
        }
        if o.wbs {
            columns.push("wbs_id");
        }
        let assignments = self.children_of("TASKRSRC", s);
        if assignments.is_empty() {
            if o.budget_no_assignments {
                columns.extend(BUDGET_UNITS);
            }
            if o.actual_no_assignments {
                columns.extend(ACTUAL_UNITS);
            }
        }
        self.set_columns("TASK", bi, &row, &columns);
        let b_owned = b.to_string();
        let mine = move |t: &Table, r: &[String]| t.value(r, "task_id").trim() == b_owned;
        let fixed = [("task_id", b.to_string())];
        if o.codes {
            let rows = self.children_of("TASKACTV", s);
            self.replace("TASKACTV", "", rows, &mine, &fixed);
        }
        if o.notebooks {
            let rows = self.children_of("TASKMEMO", s);
            self.replace("TASKMEMO", "memo_id", rows, &mine, &fixed);
        }
        if o.risks {
            let rows = self.children_of("TASKRISK", s);
            self.replace("TASKRISK", "", rows, &mine, &fixed);
        }
        if o.documents {
            let rows = self.children_of("TASKDOC", s);
            self.replace("TASKDOC", "taskdoc_id", rows, &mine, &fixed);
        }
        if o.activity_udfs {
            self.copy_udfs("TASK", &[(s.to_string(), b.to_string())]);
        }
        for (on, udfs, table, own) in [
            (o.steps, o.step_udfs, "TASKPROC", "proc_id"),
            (o.expenses, o.expense_udfs, "PROJCOST", "cost_item_id"),
        ] {
            if !on {
                continue;
            }
            let rows = self.children_of(table, s);
            let src_ids: Vec<String> = self
                .src
                .table(table)
                .map(|t| {
                    rows.iter()
                        .map(|r| t.value(r, own).trim().to_string())
                        .collect()
                })
                .unwrap_or_default();
            let wbs = self.out.table("TASK").map_or(String::new(), |t| {
                t.value(&t.rows[bi], "wbs_id").to_string()
            });
            let mut fixed = fixed.to_vec();
            if self
                .src
                .table(table)
                .is_some_and(|t| t.field("wbs_id").is_some())
            {
                // A step or expense sits in its activity's WBS element.
                fixed.push(("wbs_id", wbs));
            }
            let udf_table = table;
            match self.replace(table, own, rows, &mine, &fixed) {
                Some((old, new)) => {
                    if udfs {
                        let pairs: Vec<(String, String)> =
                            src_ids.into_iter().zip(new.iter().cloned()).collect();
                        self.carry_udfs(udf_table, &old, &[]);
                        self.copy_udfs(udf_table, &pairs);
                    } else {
                        self.carry_udfs(udf_table, &old, &new);
                    }
                }
                None if udfs => {
                    // Unchanged rows: pair them in order for their UDFs.
                    let bl_ids = self.ids_of(table, own, b);
                    let pairs: Vec<(String, String)> = src_ids.into_iter().zip(bl_ids).collect();
                    self.copy_udfs(udf_table, &pairs);
                }
                None => {}
            }
        }
        self.update_assignments(b, &assignments);
    }

    /// Own ids of the baseline rows of `table` for baseline activity `b`.
    fn ids_of(&self, table: &str, own: &str, b: &str) -> Vec<String> {
        self.out.table(table).map_or_else(Vec::new, |t| {
            t.rows
                .iter()
                .filter(|r| owned(t, r, self.key) && t.value(r, "task_id").trim() == b)
                .map(|r| t.value(r, own).trim().to_string())
                .collect()
        })
    }

    fn assignment_items(t: &Table, rows: &[Vec<String>]) -> Vec<(String, String, String)> {
        rows.iter()
            .map(|r| {
                (
                    t.value(r, "taskrsrc_id").trim().to_string(),
                    format!(
                        "{}/{}",
                        t.value(r, "rsrc_id").trim(),
                        t.value(r, "role_id").trim()
                    ),
                    t.value(r, "guid").trim().to_string(),
                )
            })
            .collect()
    }

    /// Resource Assignment options for matched baseline activity `b`, whose\n    /// project assignments are `src_rows`.
    fn update_assignments(&mut self, b: &str, src_rows: &[Vec<String>]) {
        let o = self.o.clone();
        let Some(st) = self.src.table("TASKRSRC").cloned() else {
            return;
        };
        let bl_rows: Vec<Vec<String>> = self.out.table("TASKRSRC").map_or_else(Vec::new, |t| {
            t.rows
                .iter()
                .filter(|r| owned(t, r, self.key) && t.value(r, "task_id").trim() == b)
                .cloned()
                .collect()
        });
        let bt = self.out.table("TASKRSRC").cloned().unwrap_or(Table {
            name: "TASKRSRC".into(),
            fields: vec![],
            rows: vec![],
        });
        let pairs = Self::pair(
            &Self::assignment_items(&st, src_rows),
            &Self::assignment_items(&bt, &bl_rows),
        );
        let mut done: Vec<(String, String)> = vec![];
        for row in src_rows {
            let sid = st.value(row, "taskrsrc_id").trim().to_string();
            match pairs.iter().find(|p| p.0 == sid) {
                Some((_, bid)) => {
                    self.remember("taskrsrc_id", &sid, bid);
                    if !o.update_assignments {
                        continue;
                    }
                    let mut cols: Vec<&str> = ASSIGNMENT_GENERAL.to_vec();
                    if o.update_assignment_budget {
                        cols.extend(ASSIGNMENT_BUDGET);
                    }
                    if o.update_assignment_actual {
                        cols.extend(ASSIGNMENT_ACTUAL);
                    }
                    if let Some(bi) = self.row_index("TASKRSRC", "taskrsrc_id", bid) {
                        self.set_columns("TASKRSRC", bi, row, &cols);
                    }
                    done.push((sid, bid.clone()));
                }
                None => {
                    if !o.add_assignments {
                        continue;
                    }
                    let bid = self.add_assignment(
                        row,
                        b,
                        o.add_assignment_budget,
                        o.add_assignment_actual,
                    );
                    done.push((sid, bid));
                }
            }
        }
        if o.assignment_udfs {
            self.copy_udfs("TASKRSRC", &done);
        }
        if o.assignment_codes {
            for (sid, bid) in &done {
                let rows: Vec<Vec<String>> = match self.src.table("ASGNMNTACAT") {
                    Some(t) => self
                        .src_rows("ASGNMNTACAT")
                        .into_iter()
                        .filter(|r| t.value(r, "taskrsrc_id").trim() == sid)
                        .collect(),
                    None => vec![],
                };
                let bid2 = bid.clone();
                self.replace(
                    "ASGNMNTACAT",
                    "",
                    rows,
                    &move |t, r| t.value(r, "taskrsrc_id").trim() == bid2,
                    &[("taskrsrc_id", bid.clone())],
                );
            }
        }
    }

    /// Add source assignment `row` to baseline activity `b`; budgeted and
    /// actual values come along only when asked for.
    fn add_assignment(&mut self, row: &[String], b: &str, budget: bool, actual: bool) -> String {
        let st = self.src.table("TASKRSRC").expect("source assignments");
        let sid = st.value(row, "taskrsrc_id").trim().to_string();
        let id = self.new_id("TASKRSRC", "taskrsrc_id");
        self.remember("taskrsrc_id", &sid, &id);
        let mut own: Vec<(&str, String)> = vec![("taskrsrc_id", id.clone()), ("task_id", b.into())];
        if !budget {
            own.extend([
                ("target_qty", "0".to_string()),
                ("target_cost", "0".to_string()),
            ]);
        }
        if !actual {
            for c in ASSIGNMENT_ACTUAL {
                let v = if c.ends_with("_date") { "" } else { "0" };
                own.push((c, v.to_string()));
            }
        }
        let wbs = self.out.table("TASK").and_then(|t| {
            t.rows
                .iter()
                .find(|r| t.value(r, "task_id").trim() == b)
                .map(|r| t.value(r, "wbs_id").to_string())
        });
        if let (Some(w), true) = (wbs, st.field("wbs_id").is_some()) {
            own.push(("wbs_id", w));
        }
        self.copy_in("TASKRSRC", row, &own);
        id
    }

    /// Add source activity `s` (in the task map already) with its data,
    /// leaving out actuals.
    fn add_activity(&mut self, s: &str, b: &str) {
        let st = self.src.table("TASK").expect("source activities").clone();
        let Some(row) = st
            .rows
            .iter()
            .find(|r| owned(&st, r, self.pid) && st.value(r, "task_id").trim() == s)
            .cloned()
        else {
            return;
        };
        let mut own: Vec<(&str, String)> = vec![("task_id", b.to_string())];
        let started = !matches!(st.value(&row, "status_code"), "" | "TK_NotStart");
        if started {
            own.extend([
                ("act_start_date", String::new()),
                ("act_end_date", String::new()),
                ("act_work_qty", "0".into()),
                ("act_equip_qty", "0".into()),
                ("act_this_per_work_qty", "0".into()),
                ("act_this_per_equip_qty", "0".into()),
                ("status_code", "TK_NotStart".into()),
                ("phys_complete_pct", "0".into()),
                (
                    "remain_drtn_hr_cnt",
                    st.value(&row, "target_drtn_hr_cnt").to_string(),
                ),
                (
                    "remain_work_qty",
                    st.value(&row, "target_work_qty").to_string(),
                ),
                (
                    "remain_equip_qty",
                    st.value(&row, "target_equip_qty").to_string(),
                ),
            ]);
        }
        self.copy_in("TASK", &row, &own);
        let fixed = [("task_id", b.to_string())];
        let wbs = self.out.table("TASK").map_or(String::new(), |t| {
            t.rows
                .iter()
                .find(|r| t.value(r, "task_id").trim() == b)
                .map(|r| t.value(r, "wbs_id").to_string())
                .unwrap_or_default()
        });
        for (table, own) in [
            ("TASKACTV", ""),
            ("TASKMEMO", "memo_id"),
            ("TASKPROC", "proc_id"),
            ("PROJCOST", "cost_item_id"),
            ("TASKRISK", ""),
            ("TASKDOC", "taskdoc_id"),
        ] {
            let rows = self.children_of(table, s);
            let mut fixed = fixed.to_vec();
            if (table == "PROJCOST" || table == "TASKPROC")
                && self
                    .src
                    .table(table)
                    .is_some_and(|t| t.field("wbs_id").is_some())
            {
                fixed.push(("wbs_id", wbs.clone()));
            }
            let b2 = b.to_string();
            self.replace(
                table,
                own,
                rows,
                &move |t, r| t.value(r, "task_id").trim() == b2,
                &fixed,
            );
        }
        if self.o.add_assignments {
            for row in self.children_of("TASKRSRC", s) {
                self.add_assignment(&row, b, self.o.add_assignment_budget, false);
            }
        }
        let pairs = [(s.to_string(), b.to_string())];
        self.copy_udfs("TASK", &pairs);
        for (table, own) in [
            ("TASKPROC", "proc_id"),
            ("PROJCOST", "cost_item_id"),
            ("TASKRSRC", "taskrsrc_id"),
        ] {
            let pairs: Vec<(String, String)> = match self.src.table(table) {
                Some(t) => self
                    .children_of(table, s)
                    .iter()
                    .filter_map(|r| {
                        let sid = t.value(r, own).trim().to_string();
                        self.mapped(own, &sid).map(|bid| (sid, bid))
                    })
                    .collect(),
                None => vec![],
            };
            self.copy_udfs(table, &pairs);
        }
    }

    /// Relationships of the activities in `scope` (baseline ids) become the
    /// project's, as far as both ends are in the baseline. `added` are the
    /// activities just added, whose relationships always come along.
    fn relationships(&mut self, scope: &HashSet<String>) {
        if scope.is_empty() {
            return;
        }
        let Some(st) = self.src.table("TASKPRED").cloned() else {
            let key = self.key;
            let doomed: Vec<usize> = self.out.table("TASKPRED").map_or_else(Vec::new, |t| {
                t.rows
                    .iter()
                    .enumerate()
                    .filter(|(_, r)| {
                        owned(t, r, key)
                            && (scope.contains(t.value(r, "task_id").trim())
                                || scope.contains(t.value(r, "pred_task_id").trim()))
                    })
                    .map(|(i, _)| i)
                    .collect()
            });
            remove_rows(&mut self.out, "TASKPRED", doomed);
            return;
        };
        let wanted: Vec<Vec<String>> = self
            .src_rows("TASKPRED")
            .into_iter()
            .filter(|r| {
                let succ = self.mapped("task_id", st.value(r, "task_id"));
                let pred = self.mapped("task_id", st.value(r, "pred_task_id"));
                match (succ, pred) {
                    (Some(a), Some(b)) => scope.contains(&a) || scope.contains(&b),
                    _ => false,
                }
            })
            .collect();
        let scope = scope.clone();
        self.replace(
            "TASKPRED",
            "task_pred_id",
            wanted,
            &move |t, r| {
                scope.contains(t.value(r, "task_id").trim())
                    || scope.contains(t.value(r, "pred_task_id").trim())
            },
            &[],
        );
    }

    fn data_date(&mut self) {
        let Some(t) = self.src.table("PROJECT") else {
            return;
        };
        let Some(row) = t
            .rows
            .iter()
            .find(|r| num(t.value(r, "proj_id")) == self.pid)
        else {
            return;
        };
        let row = row.clone();
        if let Some(bi) = self.row_index("PROJECT", "proj_id", &self.key.to_string()) {
            self.set_columns("PROJECT", bi, &row, &["last_recalc_date"]);
        }
    }
}

/// Update baseline `key` of `baseline` from project `pid` of `source` by
/// `options`. `only` limits the activities that are added or updated to
/// those Activity IDs (the filter); deleting baseline activities that are
/// gone from the project is not limited by it. Returns the updated copy and
/// every change it holds.
pub fn update_baseline(
    baseline: &Document,
    key: i64,
    source: &Document,
    pid: i64,
    options: &UpdateOptions,
    only: Option<&BTreeSet<String>>,
) -> Result<(Document, Preview), String> {
    if !baseline.projects().iter().any(|(id, _)| *id == key) {
        return Err("The baseline holds no project".into());
    }
    if !source.projects().iter().any(|(id, _)| *id == pid) {
        return Err("The project is not in the document".into());
    }
    let o = options.effective();
    let mut out = baseline.clone();
    out.keep_lone_quotes_of(source);
    let mut run = Run {
        src: source,
        pid,
        key,
        out,
        o: o.clone(),
        maps: HashMap::new(),
        next: HashMap::new(),
    };
    run.sync_dictionary();
    run.match_wbs();
    let src_tasks = Run::tasks(source, pid);
    let bl_tasks = Run::tasks(&run.out, key);
    let pairs = Run::pair(&src_tasks, &bl_tasks);
    for (s, b) in &pairs {
        run.remember("task_id", s, b);
    }
    let in_scope = |code: &str| only.is_none_or(|set| set.contains(code));
    let code_of: HashMap<&str, &str> = src_tasks
        .iter()
        .map(|t| (t.0.as_str(), t.1.as_str()))
        .collect();
    run.project_level();
    let mut scope: HashSet<String> = HashSet::new();
    if o.update_existing {
        for (s, b) in &pairs {
            if in_scope(code_of.get(s.as_str()).copied().unwrap_or("")) {
                run.update_activity(s, b);
                scope.insert(b.clone());
            }
        }
    }
    let matched: HashSet<&String> = pairs.iter().map(|p| &p.0).collect();
    let mut added = vec![];
    if o.add_new {
        for (s, code, _) in &src_tasks {
            if matched.contains(s) || !in_scope(code) {
                continue;
            }
            let b = run.new_id("TASK", "task_id");
            run.remember("task_id", s, &b);
            added.push((s.clone(), b));
        }
        for (s, b) in &added {
            run.add_activity(s, b);
        }
    }
    // Relationships: those of updated activities when asked for, and
    // always those of added ones.
    let mut links: HashSet<String> = added.iter().map(|a| a.1.clone()).collect();
    if o.update_existing && o.relationships {
        links.extend(scope.iter().cloned());
    }
    run.relationships(&links);
    if o.delete_missing {
        let kept: HashSet<&String> = pairs.iter().map(|p| &p.1).collect();
        let gone: Vec<String> = bl_tasks
            .iter()
            .filter(|t| !kept.contains(&t.0))
            .map(|t| t.0.clone())
            .collect();
        delete_activities(&mut run.out, key, &gone);
    }
    if o.update_existing && o.dates && only.is_none() {
        run.data_date();
    }
    if o.wbs_udfs {
        let pairs: Vec<(String, String)> = run
            .maps
            .get("wbs_id")
            .map(|m| m.iter().map(|(s, b)| (s.clone(), b.clone())).collect())
            .unwrap_or_default();
        run.copy_udfs("PROJWBS", &pairs);
    }
    let out = run.out;
    let preview = describe(baseline, &out, key);
    Ok((out, preview))
}

// ---------------------------------------------------------------- preview

/// The caption of a TASK column, as the activity table names it.
fn task_label(column: &str) -> String {
    let label = match column {
        "task_code" => "Activity ID",
        "task_name" => "Activity Name",
        "task_type" => "Activity Type",
        "duration_type" => "Duration Type",
        "complete_pct_type" => "Percent Complete Type",
        "priority_type" => "Activity Priority",
        "clndr_id" => "Calendar",
        "wbs_id" => "WBS",
        "rsrc_id" => "Primary Resource",
        "location_id" => "Location",
        "est_wt" => "Estimated Weight",
        "lock_plan_flag" => "Lock Remaining",
        "auto_compute_act_flag" => "Auto Compute Actuals",
        "rev_fdbk_flag" => "Review Required",
        "status_code" => "Activity Status",
        "phys_complete_pct" => "Physical % Complete",
        "target_drtn_hr_cnt" => "Original Duration",
        "remain_drtn_hr_cnt" => "Remaining Duration",
        "act_start_date" => "Actual Start",
        "act_end_date" => "Actual Finish",
        "target_start_date" => "Planned Start",
        "target_end_date" => "Planned Finish",
        "early_start_date" => "Early Start",
        "early_end_date" => "Early Finish",
        "late_start_date" => "Late Start",
        "late_end_date" => "Late Finish",
        "restart_date" => "Remaining Early Start",
        "reend_date" => "Remaining Early Finish",
        "rem_late_start_date" => "Remaining Late Start",
        "rem_late_end_date" => "Remaining Late Finish",
        "expect_end_date" => "Expected Finish",
        "suspend_date" => "Suspend Date",
        "resume_date" => "Resume Date",
        "cstr_type" => "Primary Constraint",
        "cstr_date" => "Primary Constraint Date",
        "cstr_type2" => "Secondary Constraint",
        "cstr_date2" => "Secondary Constraint Date",
        "total_float_hr_cnt" => "Total Float",
        "free_float_hr_cnt" => "Free Float",
        "target_work_qty" => "Budgeted Labor Units",
        "act_work_qty" => "Actual Labor Units",
        "remain_work_qty" => "Remaining Labor Units",
        "target_equip_qty" => "Budgeted Nonlabor Units",
        "act_equip_qty" => "Actual Nonlabor Units",
        "remain_equip_qty" => "Remaining Nonlabor Units",
        "act_this_per_work_qty" => "Labor Units This Period",
        "act_this_per_equip_qty" => "Nonlabor Units This Period",
        "float_path" => "Float Path",
        "float_path_order" => "Float Path Order",
        "driving_path_flag" => "Longest Path",
        "crt_path_num" => "Critical Path Number",
        "external_early_start_date" => "External Early Start",
        "external_late_end_date" => "External Late Finish",
        other => return other.to_string(),
    };
    label.to_string()
}

fn project_label(column: &str) -> String {
    let label = match column {
        "plan_start_date" => "Project Planned Start",
        "plan_end_date" => "Must Finish By",
        "fcst_start_date" => "Anticipated Start",
        "priority_num" => "Project Leveling Priority",
        "strgy_priority_num" => "Strategic Priority",
        "proj_url" => "Project URL",
        "location_id" => "Location",
        "web_local_root_path" => "Project Folder",
        "last_recalc_date" => "Data Date",
        other => return other.to_string(),
    };
    label.to_string()
}

fn number(v: &str) -> String {
    match v.trim().parse::<f64>() {
        Ok(x) => {
            let r = (x * 1000.).round() / 1000.;
            format!("{r}")
        }
        Err(_) => v.trim().to_string(),
    }
}

fn sum(t: &Table, r: &[String], a: &str, b: &str) -> String {
    let n = |c: &str| t.value(r, c).trim().parse::<f64>().unwrap_or(0.);
    number(&format!("{}", n(a) + n(b)))
}

/// One document's baseline project, indexed once for the preview: names
/// of the records rows refer to, and child rows by activity.
struct Index<'d> {
    doc: &'d Document,
    key: i64,
    /// (table, id) to the row's display name.
    names: HashMap<(&'static str, String), String>,
    hours: HashMap<String, f64>,
    paths: HashMap<String, String>,
    /// (table, task id) to the activity's rows of that table.
    children: HashMap<(&'static str, String), Vec<&'d Vec<String>>>,
    /// (UDF type id, fk) to the values.
    udfs: HashMap<String, Vec<&'d Vec<String>>>,
    /// Activities in the baseline both before and after the update: only
    /// links between them are listed under Predecessors, so a link that
    /// goes with an added or deleted activity shows on that activity alone.
    stay: HashSet<String>,
}

const NAMED: [(&str, &str, &str); 11] = [
    ("TASK", "task_id", "task_code"),
    ("CALENDAR", "clndr_id", "clndr_name"),
    ("RSRC", "rsrc_id", "rsrc_short_name"),
    ("ROLES", "role_id", "role_short_name"),
    ("ACTVTYPE", "actv_code_type_id", "actv_code_type"),
    ("ACTVCODE", "actv_code_id", "short_name"),
    ("MEMOTYPE", "memo_type_id", "memo_type"),
    ("PROJRISK", "risk_id", "risk_name"),
    ("DOCUMENT", "doc_id", "doc_name"),
    ("UDFTYPE", "udf_type_id", "udf_type_label"),
    ("PROJISSU", "issue_id", "issue_name"),
];

const CHILDREN: [&str; 8] = [
    "TASKPRED", "TASKRSRC", "TASKACTV", "TASKMEMO", "TASKPROC", "PROJCOST", "TASKRISK", "TASKDOC",
];

impl<'d> Index<'d> {
    fn new(doc: &'d Document, key: i64) -> Self {
        let mut names = HashMap::new();
        for (table, id, name) in NAMED {
            if let Some(t) = doc.table(table) {
                for r in &t.rows {
                    let mut n = t.value(r, name).to_string();
                    if table == "UDFTYPE" && n.trim().is_empty() {
                        n = t.value(r, "udf_type_name").to_string();
                    }
                    names.insert((table, t.value(r, id).trim().to_string()), n);
                }
            }
        }
        let hours = doc.table("CALENDAR").map_or_else(HashMap::new, |t| {
            t.rows
                .iter()
                .filter_map(|r| {
                    let h = t.value(r, "day_hr_cnt").trim().parse::<f64>().ok()?;
                    (h > 0.).then(|| (t.value(r, "clndr_id").trim().to_string(), h))
                })
                .collect()
        });
        let mut children: HashMap<(&'static str, String), Vec<&'d Vec<String>>> = HashMap::new();
        for table in CHILDREN {
            if let Some(t) = doc.table(table) {
                for r in t.rows.iter().filter(|r| owned(t, r, key)) {
                    children
                        .entry((table, t.value(r, "task_id").trim().to_string()))
                        .or_default()
                        .push(r);
                }
            }
        }
        let mut udfs: HashMap<String, Vec<&'d Vec<String>>> = HashMap::new();
        if let Some(t) = doc.table("UDFVALUE") {
            for r in t.rows.iter().filter(|r| owned(t, r, key)) {
                udfs.entry(t.value(r, "fk_id").trim().to_string())
                    .or_default()
                    .push(r);
            }
        }
        Index {
            doc,
            key,
            names,
            hours,
            paths: Run::wbs_paths(doc, key),
            children,
            udfs,
            stay: HashSet::new(),
        }
    }

    fn name(&self, table: &'static str, id: &str) -> String {
        self.names
            .get(&(table, id.trim().to_string()))
            .cloned()
            .unwrap_or_else(|| id.trim().to_string())
    }

    fn rows(&self, table: &'static str, task: &str) -> &[&'d Vec<String>] {
        self.children
            .get(&(table, task.to_string()))
            .map_or(&[], Vec::as_slice)
    }

    /// UDF values of record `fk` of `udf_table`, by UDF title.
    fn udf_values(&self, udf_table: &str, fk: &str) -> BTreeMap<String, String> {
        let Some(t) = self.doc.table("UDFVALUE") else {
            return BTreeMap::new();
        };
        let types = udf_types(self.doc, udf_table);
        let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for r in self.udfs.get(fk.trim()).map_or(&[][..], Vec::as_slice) {
            let ty = t.value(r, "udf_type_id").trim();
            if !types.contains(ty) {
                continue;
            }
            let value = ["udf_text", "udf_number", "udf_date", "udf_code_id"]
                .iter()
                .map(|c| t.value(r, c).trim())
                .find(|v| !v.is_empty())
                .unwrap_or("")
                .to_string();
            out.entry(self.name("UDFTYPE", ty)).or_default().push(value);
        }
        out.into_iter()
            .map(|(k, mut v)| {
                v.sort();
                (k, v.join("; "))
            })
            .collect()
    }

    /// A TASK value as the preview shows it.
    fn task_value(&self, t: &Table, row: &[String], column: &str) -> String {
        let v = t.value(row, column).trim();
        if v.is_empty() {
            return String::new();
        }
        match column {
            "clndr_id" => self.name("CALENDAR", v),
            "rsrc_id" => self.name("RSRC", v),
            "wbs_id" => self.paths.get(v).cloned().unwrap_or_else(|| v.to_string()),
            c if c.ends_with("_hr_cnt") => match v.parse::<f64>() {
                Ok(h) => {
                    let per_day = self
                        .hours
                        .get(t.value(row, "clndr_id").trim())
                        .copied()
                        .unwrap_or(8.);
                    format!("{}d", number(&(h / per_day).to_string()))
                }
                Err(_) => v.to_string(),
            },
            c if c.ends_with("_qty") || c.ends_with("_pct") || c == "est_wt" => number(v),
            _ => v.to_string(),
        }
    }

    /// One activity's fields (TASK columns and its child rows) by caption.
    fn activity(&self, t: &Table, row: &[String]) -> BTreeMap<String, String> {
        let mut out = BTreeMap::new();
        for f in &t.fields {
            if skip(f) || f.eq_ignore_ascii_case("task_id") {
                continue;
            }
            let v = self.task_value(t, row, f);
            if !v.is_empty() {
                out.insert(task_label(f), v);
            }
        }
        let task = t.value(row, "task_id").trim().to_string();
        let doc = self.doc;
        let mut put = |label: &str, mut items: Vec<String>| {
            if !items.is_empty() {
                items.sort();
                out.insert(label.to_string(), items.join("; "));
            }
        };
        let list = |table: &'static str, f: &dyn Fn(&Table, &[String]) -> String| -> Vec<String> {
            match doc.table(table) {
                Some(t) => self.rows(table, &task).iter().map(|r| f(t, r)).collect(),
                None => vec![],
            }
        };
        let links: Vec<String> = match doc.table("TASKPRED") {
            Some(t) => self
                .rows("TASKPRED", &task)
                .iter()
                .filter(|r| self.stay.contains(t.value(r, "pred_task_id").trim()))
                .map(|r| {
                    format!(
                        "{} {} {}",
                        self.name("TASK", t.value(r, "pred_task_id")),
                        t.value(r, "pred_type").trim_start_matches("PR_"),
                        self.task_value(t, r, "lag_hr_cnt")
                    )
                })
                .collect(),
            None => vec![],
        };
        put("Predecessors", links);

        put(
            "Resource Assignments",
            list("TASKRSRC", &|t, r| {
                let who = if t.value(r, "rsrc_id").trim().is_empty() {
                    self.name("ROLES", t.value(r, "role_id"))
                } else {
                    self.name("RSRC", t.value(r, "rsrc_id"))
                };
                format!(
                    "{who}: budgeted {} units {} cost, actual {} units {} cost",
                    number(t.value(r, "target_qty")),
                    number(t.value(r, "target_cost")),
                    sum(t, r, "act_reg_qty", "act_ot_qty"),
                    sum(t, r, "act_reg_cost", "act_ot_cost"),
                )
            }),
        );
        put(
            "Activity Codes",
            list("TASKACTV", &|t, r| {
                format!(
                    "{}: {}",
                    self.name("ACTVTYPE", t.value(r, "actv_code_type_id")),
                    self.name("ACTVCODE", t.value(r, "actv_code_id"))
                )
            }),
        );
        put(
            "Notebook Topics",
            list("TASKMEMO", &|t, r| {
                format!(
                    "{}: {}",
                    self.name("MEMOTYPE", t.value(r, "memo_type_id")),
                    t.value(r, "task_memo").chars().take(60).collect::<String>()
                )
            }),
        );
        put(
            "Steps",
            list("TASKPROC", &|t, r| {
                format!(
                    "{} {} ({}%)",
                    t.value(r, "seq_num"),
                    t.value(r, "proc_name"),
                    number(t.value(r, "complete_pct"))
                )
            }),
        );
        put(
            "Expenses",
            list("PROJCOST", &|t, r| {
                format!(
                    "{}: budgeted {}, actual {}",
                    t.value(r, "cost_name"),
                    number(t.value(r, "target_cost")),
                    number(t.value(r, "act_cost"))
                )
            }),
        );
        put(
            "Risks",
            list("TASKRISK", &|t, r| {
                self.name("PROJRISK", t.value(r, "risk_id"))
            }),
        );
        put(
            "Documents",
            list("TASKDOC", &|t, r| {
                self.name("DOCUMENT", t.value(r, "doc_id"))
            }),
        );
        out.extend(self.udf_values("TASK", &task));
        for (table, label, col) in [
            ("TASKPROC", "Step", "proc_id"),
            ("PROJCOST", "Expense", "cost_item_id"),
            ("TASKRSRC", "Assignment", "taskrsrc_id"),
        ] {
            if let Some(t) = doc.table(table) {
                let mut values: BTreeMap<String, Vec<String>> = BTreeMap::new();
                for r in self.rows(table, &task) {
                    for (k, v) in self.udf_values(table, t.value(r, col)) {
                        values.entry(format!("{label} {k}")).or_default().push(v);
                    }
                }
                for (k, mut v) in values {
                    v.sort();
                    out.insert(k, v.join("; "));
                }
            }
        }
        out
    }

    /// The project's fields by caption.
    fn project(&self) -> BTreeMap<String, String> {
        let (doc, key) = (self.doc, self.key);
        let mut f = BTreeMap::new();
        if let Some(t) = doc.table("PROJECT") {
            if let Some(r) = t.rows.iter().find(|r| num(t.value(r, "proj_id")) == key) {
                for col in PROJECT_DETAILS.iter().chain(&["last_recalc_date"]) {
                    let v = t.value(r, col).trim().to_string();
                    if !v.is_empty() {
                        f.insert(project_label(col), v);
                    }
                }
            }
        }
        for (table, label, name, id_col) in [
            ("PROJRISK", "Risks", "risk_name", "risk_id"),
            ("PROJISSU", "Issues", "issue_name", "issue_id"),
            ("DOCUMENT", "Documents", "doc_name", "doc_id"),
        ] {
            if let Some(t) = doc.table(table) {
                let mut items = vec![];
                for r in t.rows.iter().filter(|r| owned(t, r, key)) {
                    items.push(t.value(r, name).to_string());
                    for (k, v) in self.udf_values(table, t.value(r, id_col)) {
                        f.insert(format!("{} {k}", t.value(r, name)), v);
                    }
                }
                items.sort();
                if !items.is_empty() {
                    f.insert(label.into(), items.join("; "));
                }
            }
        }
        if let Some(t) = doc.table("PROJTHRS") {
            let rows: Vec<Vec<String>> = t
                .rows
                .iter()
                .filter(|r| owned(t, r, key))
                .cloned()
                .collect();
            if !rows.is_empty() {
                f.insert(
                    "Thresholds".into(),
                    Run::signature(t, &rows, "thresh_id").join("; "),
                );
            }
        }
        f.extend(self.udf_values("PROJECT", &key.to_string()));
        f
    }

    /// The WBS elements (not the project's own node) by id, with fields.
    fn wbs(&self) -> BTreeMap<String, BTreeMap<String, String>> {
        let Some(t) = self.doc.table("PROJWBS") else {
            return BTreeMap::new();
        };
        t.rows
            .iter()
            .filter(|r| owned(t, r, self.key) && t.value(r, "proj_node_flag") != "Y")
            .map(|r| {
                let id = t.value(r, "wbs_id").trim().to_string();
                let mut f = BTreeMap::new();
                for (col, label) in [
                    ("wbs_short_name", "WBS Code"),
                    ("wbs_name", "WBS Name"),
                    ("status_code", "WBS Status"),
                ] {
                    let v = t.value(r, col).trim().to_string();
                    if !v.is_empty() {
                        f.insert(label.to_string(), v);
                    }
                }
                if let Some(p) = self.paths.get(&id) {
                    f.insert("Path".into(), p.clone());
                }
                f.extend(self.udf_values("PROJWBS", &id));
                (id, f)
            })
            .collect()
    }
}

fn diff_fields(
    before: &BTreeMap<String, String>,
    after: &BTreeMap<String, String>,
) -> Vec<FieldChange> {
    let keys: BTreeSet<&String> = before.keys().chain(after.keys()).collect();
    keys.into_iter()
        .filter_map(|k| {
            let (b, a) = (
                before.get(k).cloned().unwrap_or_default(),
                after.get(k).cloned().unwrap_or_default(),
            );
            (canon(&a) != canon(&b)).then(|| FieldChange {
                field: k.clone(),
                project: a,
                baseline: b,
            })
        })
        .collect()
}

/// Every difference between the baseline before (`old`) and after (`new`).
fn describe(old: &Document, new: &Document, key: i64) -> Preview {
    let mut preview = Preview::default();
    let (mut io, mut inew) = (Index::new(old, key), Index::new(new, key));
    // Linked activities are named by their Activity ID after the update on
    // both sides, so a renumber shows on the renumbered activity only.
    for ((table, id), name) in &inew.names {
        if *table == "TASK" {
            io.names.insert((table, id.clone()), name.clone());
        }
    }
    let ids = |doc: &Document| -> HashSet<String> {
        rows_of(doc, "TASK", key)
            .iter()
            .filter_map(|r| Some(doc.table("TASK")?.value(r, "task_id").trim().to_string()))
            .collect()
    };
    let stay: HashSet<String> = ids(old).intersection(&ids(new)).cloned().collect();
    io.stay = stay.clone();
    inew.stay = stay;
    let empty = Table {
        name: "TASK".into(),
        fields: vec![],
        rows: vec![],
    };
    let (ot, nt) = (
        old.table("TASK").unwrap_or(&empty),
        new.table("TASK").unwrap_or(&empty),
    );
    let index = |t: &Table| -> BTreeMap<String, usize> {
        t.rows
            .iter()
            .enumerate()
            .filter(|(_, r)| owned(t, r, key))
            .map(|(i, r)| (t.value(r, "task_id").trim().to_string(), i))
            .collect()
    };
    let (before, after) = (index(ot), index(nt));
    let ids: BTreeSet<&String> = before.keys().chain(after.keys()).collect();
    let none = BTreeMap::new();
    for id in ids {
        let b = before.get(id).map(|&i| io.activity(ot, &ot.rows[i]));
        let a = after.get(id).map(|&i| inew.activity(nt, &nt.rows[i]));
        let object = after
            .get(id)
            .map(|&i| nt.value(&nt.rows[i], "task_code").to_string())
            .or_else(|| {
                before
                    .get(id)
                    .map(|&i| ot.value(&ot.rows[i], "task_code").to_string())
            })
            .unwrap_or_default();
        let (kind, fields) = match (&b, &a) {
            (None, Some(a)) => (ChangeKind::Added, diff_fields(&none, a)),
            (Some(b), None) => (ChangeKind::Deleted, diff_fields(b, &none)),
            (Some(b), Some(a)) => (ChangeKind::Updated, diff_fields(b, a)),
            (None, None) => continue,
        };
        if kind == ChangeKind::Updated && fields.is_empty() {
            continue;
        }
        preview.activities.push(ObjectChange {
            object,
            kind,
            fields,
        });
    }
    preview.activities.sort_by(|a, b| a.object.cmp(&b.object));
    let (wb, wa) = (io.wbs(), inew.wbs());
    let path = |f: &BTreeMap<String, String>, id: &str| {
        f.get("Path").cloned().unwrap_or_else(|| id.to_string())
    };
    for (id, a) in &wa {
        let (kind, fields) = match wb.get(id) {
            None => (ChangeKind::Added, diff_fields(&none, a)),
            Some(b) => (ChangeKind::Updated, diff_fields(b, a)),
        };
        if !fields.is_empty() {
            preview.wbs.push(ObjectChange {
                object: path(a, id),
                kind,
                fields,
            });
        }
    }
    for (id, b) in &wb {
        if !wa.contains_key(id) {
            preview.wbs.push(ObjectChange {
                object: path(b, id),
                kind: ChangeKind::Deleted,
                fields: diff_fields(b, &none),
            });
        }
    }
    let fields = diff_fields(&io.project(), &inew.project());
    if !fields.is_empty() {
        preview.project.push(ObjectChange {
            object: crate::baseline::project_value(new, key, "proj_short_name"),
            kind: ChangeKind::Updated,
            fields,
        });
    }
    preview
}

#[cfg(test)]
mod tests;
