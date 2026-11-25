//! The project registers of the Project menu, shared by every front end:
//! expense items (`PROJCOST`), issues (`PROJISSU`), risks (`PROJRISK`, or a
//! `RISK` table when a file carries one, with their activities in
//! `TASKRISK`) and thresholds (`PROJTHRS`, see [`super::thresholds`]).
//!
//! The desktop's record views, the Activity Details tabs and the command
//! line edits call these same functions. Two layers:
//!
//! - column edits by record id (`add_expense`, `set_expense`, `assign_risk`,
//!   ...), as the views make them;
//! - named fields ([`fields`], [`set_field`], [`record_values`]) that take
//!   the values the way an agent names them (an Activity ID, a WBS path, a
//!   resource short name, a dictionary name, a choice by code or label) and
//!   write the column the view would write.
use super::records::{self, float, format_number, num};
use super::{thresholds, wbs};
use crate::{Document, ProjectData};

type R<T> = Result<T, String>;

/// Priority codes of issues and thresholds.
pub const PRIORITY: [(&str, &str); 5] = [
    ("PT_Top", "Top"),
    ("PT_High", "High"),
    ("PT_Normal", "Normal"),
    ("PT_Low", "Low"),
    ("PT_Lowest", "Lowest"),
];
/// Accrual types of expense items.
pub const ACCRUAL: [(&str, &str); 3] = [
    ("CL_Start", "Start of Activity"),
    ("CL_End", "End of Activity"),
    ("CL_Uniform", "Uniform"),
];
pub const ISSUE_STATUS: [(&str, &str); 3] = [
    ("IS_Open", "Open"),
    ("IS_On_Hold", "On Hold"),
    ("IS_Closed", "Closed"),
];
pub const RISK_STATUS: [(&str, &str); 6] = [
    ("RS_Propose", "Proposed"),
    ("RS_Open", "Open"),
    ("RS_Active", "Active"),
    ("RS_Reject", "Rejected (Closed)"),
    ("RS_Impact", "Impacted (Closed)"),
    ("RS_Manage", "Managed (Closed)"),
];
pub const RISK_KIND: [(&str, &str); 2] = [("RT_Threat", "Threat"), ("RT_Opport", "Opportunity")];
pub const RISK_RESPONSE: [(&str, &str); 8] = [
    ("RP_Accept", "Accept"),
    ("RP_Avoid", "Avoid"),
    ("RP_Trans", "Transfer"),
    ("RP_Reduce", "Reduce"),
    ("RP_Exploit", "Exploit"),
    ("RP_Facil", "Facilitate"),
    ("RP_Enhance", "Enhance"),
    ("RP_Reject", "Reject"),
];

// ------------------------------------------------------------ keyed rows

/// Index of the row of `table` whose `key` column holds `id`.
pub fn row_of(doc: &Document, table: &str, key: &str, id: i64) -> Option<usize> {
    let t = doc.table(table)?;
    t.rows.iter().position(|r| num(t.value(r, key)) == id)
}

/// A column of the row keyed `id`, or an empty string.
pub fn value(doc: &Document, table: &str, key: &str, id: i64, column: &str) -> String {
    row_of(doc, table, key, id)
        .map(|i| records::get(doc, table, i, column))
        .unwrap_or_default()
}

/// Set a column of the row keyed `id`; true when the value changed.
pub fn set_value(
    doc: &mut Document,
    table: &str,
    key: &str,
    id: i64,
    column: &str,
    text: String,
) -> bool {
    let Some(i) = row_of(doc, table, key, id) else {
        return false;
    };
    if records::get(doc, table, i, column) == text {
        return false;
    }
    records::set(doc, table, i, column, text);
    true
}

/// Remove every row of `table` whose `key` column holds `id`.
pub fn remove_where(doc: &mut Document, table: &str, key: &str, id: i64) {
    if doc.table(table).is_none() {
        return;
    }
    let t = doc.table_mut(table);
    let col = t.field(key);
    t.rows
        .retain(|r| col.and_then(|c| r.get(c)).map_or(0, |v| num(v)) != id);
}

/// Rows of `table` for project `pid` (all rows when the table has no
/// `proj_id` column).
pub fn project_rows(doc: &Document, table: &str, pid: i64) -> Vec<usize> {
    let Some(t) = doc.table(table) else {
        return vec![];
    };
    let scoped = t.field("proj_id").is_some();
    (0..t.rows.len())
        .filter(|&r| !scoped || num(t.value(&t.rows[r], "proj_id")) == pid)
        .collect()
}

/// `(id, name)` pairs of a dictionary table, in file order.
pub fn dictionary(doc: &Document, table: &str, key: &str, name: &str) -> Vec<(String, String)> {
    doc.table(table).map_or(vec![], |t| {
        t.rows
            .iter()
            .map(|r| (t.value(r, key).to_string(), t.value(r, name).to_string()))
            .collect()
    })
}

/// The name for `id` in a dictionary list.
pub fn name_of(list: &[(String, String)], id: &str) -> String {
    if id.trim().is_empty() {
        return String::new();
    }
    list.iter()
        .find(|(k, _)| num(k) == num(id))
        .map_or(String::new(), |(_, n)| n.clone())
}

/// The label of a code in a `(code, label)` list; unknown codes show as is.
pub fn label_of(list: &[(&str, &str)], code: &str) -> String {
    list.iter()
        .find(|(k, _)| *k == code)
        .map_or(code.to_string(), |(_, l)| l.to_string())
}

/// The activity with `task_id` as `(Activity ID, Activity Name)`.
pub fn activity_label(p: &ProjectData, task_id: i64) -> (String, String) {
    p.model
        .activities
        .iter()
        .find(|a| a.task_id == task_id)
        .map_or((String::new(), String::new()), |a| {
            (
                a.task_code.clone(),
                p.details
                    .get(&task_id)
                    .map_or(String::new(), |d| d.name.clone()),
            )
        })
}

/// A WBS element's code path as the edits name it (short codes below the
/// project node joined by `.`); empty for no element.
pub fn wbs_code_path(p: &ProjectData, id: i64) -> String {
    if id == 0 {
        return String::new();
    }
    let project = wbs::project_node(p);
    let mut parts = vec![];
    let mut at = Some(id);
    let mut guard = 0;
    while let Some(w) = at.filter(|w| Some(*w) != project) {
        let Some(e) = p.wbs.iter().find(|e| e.id == w) else {
            break;
        };
        parts.push(e.code.clone());
        at = e.parent;
        guard += 1;
        if guard > 10_000 {
            break;
        }
    }
    parts.reverse();
    parts.join(".")
}

// --------------------------------------------------------------- expenses

/// Add an expense item to activity `task_id`; returns its id.
pub fn add_expense(p: &mut ProjectData, task_id: i64) -> i64 {
    let pid = p.model.project.proj_id;
    let doc = &mut p.document;
    let id = records::next_id(doc, "PROJCOST", "cost_item_id");
    records::add_row(
        doc,
        "PROJCOST",
        &[
            ("cost_item_id", id.to_string()),
            ("proj_id", pid.to_string()),
            ("task_id", task_id.to_string()),
            ("cost_name", "(New Expense Item)".into()),
            ("act_cost", "0".into()),
            ("cost_per_qty", "0".into()),
            ("remain_cost", "0".into()),
            ("target_cost", "0".into()),
            ("cost_load_type", "CL_Uniform".into()),
            ("auto_compute_act_flag", "N".into()),
            ("target_qty", "0".into()),
        ],
    );
    id
}

pub fn delete_expense(p: &mut ProjectData, id: i64) -> bool {
    let Some(i) = row_of(&p.document, "PROJCOST", "cost_item_id", id) else {
        return false;
    };
    records::remove_row(&mut p.document, "PROJCOST", i);
    true
}

/// Move an expense item to another activity.
pub fn move_expense(p: &mut ProjectData, id: i64, task_id: i64) -> bool {
    set_value(
        &mut p.document,
        "PROJCOST",
        "cost_item_id",
        id,
        "task_id",
        task_id.to_string(),
    )
}

/// Set one column of an expense item, keeping the costs consistent:
/// budgeted units times price is the budgeted cost, and an item without
/// actuals has its budget remaining.
pub fn set_expense(p: &mut ProjectData, id: i64, column: &str, text: String) -> bool {
    let doc = &mut p.document;
    let get = |doc: &Document, k: &str| float(&value(doc, "PROJCOST", "cost_item_id", id, k));
    let mut changed = set_value(doc, "PROJCOST", "cost_item_id", id, column, text.clone());
    if !changed {
        return false;
    }
    let mut set = |doc: &mut Document, k: &str, v: f64| {
        changed |= set_value(doc, "PROJCOST", "cost_item_id", id, k, format_number(v));
    };
    match column {
        "target_cost" => {
            let (qty, cost) = (get(doc, "target_qty"), get(doc, "target_cost"));
            if qty > 0. {
                set(doc, "cost_per_qty", cost / qty);
            }
            if get(doc, "act_cost") == 0. {
                set(doc, "remain_cost", cost);
            }
        }
        "target_qty" | "cost_per_qty" => {
            let cost = get(doc, "target_qty") * get(doc, "cost_per_qty");
            set(doc, "target_cost", cost);
            if get(doc, "act_cost") == 0. {
                set(doc, "remain_cost", cost);
            }
        }
        _ => {}
    }
    changed
}

// ----------------------------------------------------------------- issues

/// Add an open issue identified `today` (an XER date); returns its id.
pub fn add_issue(p: &mut ProjectData, today: &str) -> i64 {
    let pid = p.model.project.proj_id;
    let doc = &mut p.document;
    let id = records::next_id(doc, "PROJISSU", "issue_id");
    records::add_row(
        doc,
        "PROJISSU",
        &[
            ("issue_id", id.to_string()),
            ("proj_id", pid.to_string()),
            ("issue_name", "(New Issue)".into()),
            ("add_date", today.to_string()),
            ("priority_type", "PT_Normal".into()),
            ("status_code", "IS_Open".into()),
        ],
    );
    id
}

/// Delete an issue with its history.
pub fn delete_issue(p: &mut ProjectData, id: i64) -> bool {
    let doc = &mut p.document;
    let Some(i) = row_of(doc, "PROJISSU", "issue_id", id) else {
        return false;
    };
    records::remove_row(doc, "PROJISSU", i);
    remove_where(doc, "ISSUHIST", "issue_id", id);
    true
}

/// Tie an issue to an activity (it no longer applies to a WBS element).
pub fn set_issue_activity(p: &mut ProjectData, issue: i64, task_id: i64) -> bool {
    let doc = &mut p.document;
    set_value(
        doc,
        "PROJISSU",
        "issue_id",
        issue,
        "task_id",
        task_id.to_string(),
    ) | set_value(doc, "PROJISSU", "issue_id", issue, "wbs_id", String::new())
}

/// Tie an issue to a WBS element (it no longer applies to an activity).
pub fn set_issue_wbs(p: &mut ProjectData, issue: i64, wbs_id: i64) -> bool {
    let doc = &mut p.document;
    set_value(
        doc,
        "PROJISSU",
        "issue_id",
        issue,
        "wbs_id",
        wbs_id.to_string(),
    ) | set_value(doc, "PROJISSU", "issue_id", issue, "task_id", String::new())
}

// ------------------------------------------------------------------ risks

/// Activities (task ids) a risk is assigned to.
pub fn risk_activities(doc: &Document, pid: i64, risk_id: i64) -> Vec<i64> {
    project_rows(doc, "TASKRISK", pid)
        .into_iter()
        .filter(|&r| num(&records::get(doc, "TASKRISK", r, "risk_id")) == risk_id)
        .map(|r| num(&records::get(doc, "TASKRISK", r, "task_id")))
        .collect()
}

/// Assign a risk to an activity.
pub fn assign_risk(doc: &mut Document, pid: i64, risk_id: i64, task_id: i64) -> bool {
    if risk_activities(doc, pid, risk_id).contains(&task_id) {
        return false;
    }
    records::add_row(
        doc,
        "TASKRISK",
        &[
            ("task_id", task_id.to_string()),
            ("risk_id", risk_id.to_string()),
            ("proj_id", pid.to_string()),
        ],
    );
    true
}

/// Remove a risk's assignment to an activity.
pub fn unassign_risk(doc: &mut Document, pid: i64, risk_id: i64, task_id: i64) -> bool {
    let Some(r) = project_rows(doc, "TASKRISK", pid).into_iter().find(|&r| {
        num(&records::get(doc, "TASKRISK", r, "risk_id")) == risk_id
            && num(&records::get(doc, "TASKRISK", r, "task_id")) == task_id
    }) else {
        return false;
    };
    records::remove_row(doc, "TASKRISK", r);
    true
}

/// Add a proposed threat identified `today`; returns its id.
pub fn add_risk(p: &mut ProjectData, today: &str) -> i64 {
    let pid = p.model.project.proj_id;
    let doc = &mut p.document;
    let table = records::risk_table(doc);
    let id = records::next_id(doc, table, "risk_id");
    let codes: Vec<String> = doc.table(table).map_or(vec![], |t| {
        t.rows
            .iter()
            .map(|r| t.value(r, "risk_code").to_string())
            .collect()
    });
    let mut n = codes.len() + 1;
    while codes.contains(&format!("RISK-{n}")) {
        n += 1;
    }
    records::add_row(
        doc,
        table,
        &[
            ("risk_id", id.to_string()),
            ("risk_code", format!("RISK-{n}")),
            ("proj_id", pid.to_string()),
            ("add_date", today.to_string()),
            ("status_code", "RS_Propose".into()),
            ("risk_name", "(New Risk)".into()),
            ("risk_to_type", "RT_Threat".into()),
        ],
    );
    id
}

/// Delete a risk and its activity assignments.
pub fn delete_risk(p: &mut ProjectData, id: i64) -> bool {
    let doc = &mut p.document;
    let table = records::risk_table(doc);
    let Some(i) = row_of(doc, table, "risk_id", id) else {
        return false;
    };
    records::remove_row(doc, table, i);
    remove_where(doc, "TASKRISK", "risk_id", id);
    true
}

// ------------------------------------------------------- named field layer

/// A kind of project record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Expense,
    Issue,
    Risk,
    Threshold,
}

impl Kind {
    pub const ALL: [Kind; 4] = [Kind::Expense, Kind::Issue, Kind::Risk, Kind::Threshold];
    pub fn parse(name: &str) -> R<Kind> {
        match name.trim().to_ascii_lowercase().trim_end_matches('s') {
            "expense" | "expense item" | "expense_item" => Ok(Kind::Expense),
            "issue" => Ok(Kind::Issue),
            "risk" => Ok(Kind::Risk),
            "threshold" => Ok(Kind::Threshold),
            _ => Err(format!(
                "Unknown record kind {name:?}; use expenses, issues, risks or thresholds."
            )),
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Kind::Expense => "expense",
            Kind::Issue => "issue",
            Kind::Risk => "risk",
            Kind::Threshold => "threshold",
        }
    }
    pub fn table(self, doc: &Document) -> &'static str {
        match self {
            Kind::Expense => "PROJCOST",
            Kind::Issue => "PROJISSU",
            Kind::Risk => records::risk_table(doc),
            Kind::Threshold => "PROJTHRS",
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Kind::Expense => "cost_item_id",
            Kind::Issue => "issue_id",
            Kind::Risk => "risk_id",
            Kind::Threshold => "thresh_id",
        }
    }
    /// The column a record is also found by (besides its id).
    fn name_column(self) -> Option<&'static str> {
        match self {
            Kind::Expense => Some("cost_name"),
            Kind::Issue => Some("issue_name"),
            Kind::Risk => Some("risk_name"),
            Kind::Threshold => None,
        }
    }
}

/// How a field's value is given and stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldKind {
    /// One line of text.
    Text,
    /// Free text kept as written (line breaks too).
    Memo,
    /// A memo stored as HTML: plain text is wrapped as the app wraps it, a
    /// value starting with `<` is kept as HTML.
    Html,
    /// A number; `blank` allows clearing it.
    Number { blank: bool },
    /// A project-local date.
    Date,
    /// A code from a fixed list, by code or label.
    Choice(&'static [(&'static str, &'static str)]),
    /// Y or N.
    Flag,
    /// An id from a dictionary table, by name or id.
    Dictionary {
        table: &'static str,
        key: &'static str,
        name: &'static str,
    },
    /// An activity, by Activity ID.
    Activity,
    /// A WBS element, by code path.
    Wbs,
    /// A resource, by short name.
    Resource,
    /// A threshold parameter, by name or measured field.
    Parameter,
    /// Shown, never written.
    ReadOnly,
}

/// A record field: its name in the edits, its column and its kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Field {
    pub name: &'static str,
    pub column: &'static str,
    pub kind: FieldKind,
}

const fn f(name: &'static str, column: &'static str, kind: FieldKind) -> Field {
    Field { name, column, kind }
}

const OBS: FieldKind = FieldKind::Dictionary {
    table: "OBS",
    key: "obs_id",
    name: "obs_name",
};

const EXPENSE_FIELDS: &[Field] = &[
    f("name", "cost_name", FieldKind::Text),
    f("activity", "task_id", FieldKind::Activity),
    f(
        "category",
        "cost_type_id",
        FieldKind::Dictionary {
            table: "COSTTYPE",
            key: "cost_type_id",
            name: "cost_type",
        },
    ),
    f(
        "cost_account",
        "acct_id",
        FieldKind::Dictionary {
            table: "ACCOUNT",
            key: "acct_id",
            name: "acct_name",
        },
    ),
    f("accrual", "cost_load_type", FieldKind::Choice(&ACCRUAL)),
    f("unit_of_measure", "qty_name", FieldKind::Text),
    f("vendor", "vendor_name", FieldKind::Text),
    f("po_number", "po_number", FieldKind::Text),
    f(
        "auto_compute_actuals",
        "auto_compute_act_flag",
        FieldKind::Flag,
    ),
    f(
        "budgeted_units",
        "target_qty",
        FieldKind::Number { blank: false },
    ),
    f(
        "price_per_unit",
        "cost_per_qty",
        FieldKind::Number { blank: false },
    ),
    f(
        "budgeted_cost",
        "target_cost",
        FieldKind::Number { blank: false },
    ),
    f(
        "actual_cost",
        "act_cost",
        FieldKind::Number { blank: false },
    ),
    f(
        "remaining_cost",
        "remain_cost",
        FieldKind::Number { blank: false },
    ),
    f("description", "cost_descr", FieldKind::Html),
];

const ISSUE_FIELDS: &[Field] = &[
    f("name", "issue_name", FieldKind::Text),
    f("responsible_manager", "obs_id", OBS),
    f("priority", "priority_type", FieldKind::Choice(&PRIORITY)),
    f("status", "status_code", FieldKind::Choice(&ISSUE_STATUS)),
    f("identified_on", "add_date", FieldKind::Date),
    f("identified_by", "add_by_name", FieldKind::Text),
    f("resolution_date", "resolv_date", FieldKind::Date),
    f("activity", "task_id", FieldKind::Activity),
    f("wbs", "wbs_id", FieldKind::Wbs),
    f("resource", "rsrc_id", FieldKind::Resource),
    f("notes", "issue_notes", FieldKind::Html),
    f("threshold_parameter", "thresh_parm_id", FieldKind::ReadOnly),
    f("actual_value", "issue_value", FieldKind::ReadOnly),
    f("lower_threshold", "lo_parm_value", FieldKind::ReadOnly),
    f("upper_threshold", "hi_parm_value", FieldKind::ReadOnly),
];

const RISK_FIELDS: &[Field] = &[
    f("risk_id", "risk_code", FieldKind::Text),
    f("name", "risk_name", FieldKind::Text),
    f(
        "category",
        "risk_type_id",
        FieldKind::Dictionary {
            table: "RISKTYPE",
            key: "risk_type_id",
            name: "risk_type",
        },
    ),
    f("type", "risk_to_type", FieldKind::Choice(&RISK_KIND)),
    f("status", "status_code", FieldKind::Choice(&RISK_STATUS)),
    f("owner", "rsrc_id", FieldKind::Resource),
    f("identified_by", "identified_by_id", FieldKind::Resource),
    f("identified_on", "add_date", FieldKind::Date),
    f(
        "pre_response_probability",
        "pre_rsp_prblty",
        FieldKind::Text,
    ),
    f(
        "pre_response_schedule",
        "pre_rsp_schd_prblty",
        FieldKind::Text,
    ),
    f("pre_response_cost", "pre_rsp_cost_prblty", FieldKind::Text),
    f(
        "post_response_probability",
        "post_rsp_prblty",
        FieldKind::Text,
    ),
    f(
        "post_response_schedule",
        "post_rsp_schd_prblty",
        FieldKind::Text,
    ),
    f(
        "post_response_cost",
        "post_rsp_cost_prblty",
        FieldKind::Text,
    ),
    f(
        "response_type",
        "response_type",
        FieldKind::Choice(&RISK_RESPONSE),
    ),
    f("response_description", "response_text", FieldKind::Text),
    f("description", "risk_descr", FieldKind::Html),
    f("cause", "risk_cause", FieldKind::Memo),
    f("effect", "risk_effect", FieldKind::Memo),
    f("notes", "notes", FieldKind::Memo),
];

const THRESHOLD_FIELDS: &[Field] = &[
    f("parameter", "thresh_parm_id", FieldKind::Parameter),
    f(
        "lower_threshold",
        "lo_parm_value",
        FieldKind::Number { blank: true },
    ),
    f(
        "upper_threshold",
        "hi_parm_value",
        FieldKind::Number { blank: true },
    ),
    f(
        "detail_to_monitor",
        "thresh_type",
        FieldKind::Choice(&thresholds::DETAIL),
    ),
    f("wbs", "wbs_id", FieldKind::Wbs),
    f("responsible_manager", "obs_id", OBS),
    f("priority", "priority_type", FieldKind::Choice(&PRIORITY)),
    f(
        "status",
        "status_code",
        FieldKind::Choice(&thresholds::STATUS),
    ),
    f("from_date", "window_start", FieldKind::Date),
    f("to_date", "window_end", FieldKind::Date),
];

/// The fields of a kind of record, in the order the view shows them.
pub fn fields(kind: Kind) -> &'static [Field] {
    match kind {
        Kind::Expense => EXPENSE_FIELDS,
        Kind::Issue => ISSUE_FIELDS,
        Kind::Risk => RISK_FIELDS,
        Kind::Threshold => THRESHOLD_FIELDS,
    }
}

fn field(kind: Kind, name: &str) -> R<Field> {
    let wanted = name.trim().to_ascii_lowercase().replace([' ', '-'], "_");
    fields(kind)
        .iter()
        .find(|f| f.name == wanted || f.column == wanted)
        .copied()
        .ok_or_else(|| {
            let names: Vec<&str> = fields(kind)
                .iter()
                .filter(|f| f.kind != FieldKind::ReadOnly)
                .map(|f| f.name)
                .collect();
            format!(
                "Unknown {} field {name:?}; use {}.",
                kind.name(),
                names.join(", ")
            )
        })
}

/// The ids of the project's records of a kind, in file order.
pub fn ids(p: &ProjectData, kind: Kind) -> Vec<i64> {
    let doc = &p.document;
    let table = kind.table(doc);
    project_rows(doc, table, p.model.project.proj_id)
        .into_iter()
        .map(|r| num(&records::get(doc, table, r, kind.key())))
        .collect()
}

/// A record of the project by id, or by name (an issue or expense item's
/// name, a risk's name or Risk ID) when that names exactly one.
pub fn find(p: &ProjectData, kind: Kind, name: &str) -> R<i64> {
    let doc = &p.document;
    let table = kind.table(doc);
    let rows = project_rows(doc, table, p.model.project.proj_id);
    let get = |r: usize, c: &str| records::get(doc, table, r, c);
    let wanted = name.trim();
    if let Ok(id) = wanted.parse::<i64>() {
        if rows.iter().any(|&r| num(&get(r, kind.key())) == id) {
            return Ok(id);
        }
    }
    let mut columns: Vec<&str> = kind.name_column().into_iter().collect();
    if kind == Kind::Risk {
        columns.insert(0, "risk_code");
    }
    for column in columns {
        let found: Vec<i64> = rows
            .iter()
            .filter(|&&r| get(r, column).trim().eq_ignore_ascii_case(wanted))
            .map(|&r| num(&get(r, kind.key())))
            .collect();
        match found.as_slice() {
            [id] => return Ok(*id),
            [] => {}
            _ => {
                return Err(format!(
                    "{} {wanted:?} is not unique; name it by id.",
                    kind.name()
                ))
            }
        }
    }
    Err(format!("No {} {wanted:?}.", kind.name()))
}

/// Add a record. An expense item needs its activity (`task_id`); records
/// added by hand are identified `today` (an XER date).
pub fn add(p: &mut ProjectData, kind: Kind, task_id: Option<i64>, today: &str) -> R<i64> {
    Ok(match kind {
        Kind::Expense => add_expense(p, task_id.ok_or("An expense item needs an activity.")?),
        Kind::Issue => add_issue(p, today),
        Kind::Risk => add_risk(p, today),
        Kind::Threshold => thresholds::add_threshold(p),
    })
}

/// Delete a record (an issue with its history, a risk with its activity
/// assignments; issues a threshold raised stay).
pub fn delete(p: &mut ProjectData, kind: Kind, id: i64) -> R<()> {
    let done = match kind {
        Kind::Expense => delete_expense(p, id),
        Kind::Issue => delete_issue(p, id),
        Kind::Risk => delete_risk(p, id),
        Kind::Threshold => thresholds::delete_threshold(p, id),
    };
    done.then_some(())
        .ok_or_else(|| format!("No {} {id}.", kind.name()))
}

fn choice(list: &[(&str, &str)], text: &str) -> Option<String> {
    let t = text.trim();
    list.iter()
        .find(|(code, label)| code.eq_ignore_ascii_case(t) || label.eq_ignore_ascii_case(t))
        .or_else(|| {
            // "Rejected" for "Rejected (Closed)".
            list.iter().find(|(_, label)| {
                label
                    .split(" (")
                    .next()
                    .is_some_and(|l| l.eq_ignore_ascii_case(t))
            })
        })
        .map(|(code, _)| code.to_string())
}

fn number_text(text: &str) -> Option<String> {
    let t = text.trim().replace(',', "");
    t.trim_start_matches(['$', '€', '£'])
        .trim_end_matches('%')
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .map(format_number)
}

/// The stored text for a field's value (`None` clears it).
fn stored(p: &ProjectData, kind: Kind, field: &Field, value: Option<&str>) -> R<String> {
    let label = format!("{} {}", kind.name(), field.name);
    let text = value.map(str::trim).unwrap_or_default();
    let doc = &p.document;
    let clear = text.is_empty();
    Ok(match field.kind {
        FieldKind::ReadOnly => {
            return Err(format!("{label} is set by Monitor Thresholds, not edited."))
        }
        FieldKind::Text => records::clean(value.unwrap_or_default()),
        FieldKind::Memo => value.unwrap_or_default().to_string(),
        FieldKind::Html => {
            let v = value.unwrap_or_default();
            if records::looks_like_html(v) || v.is_empty() {
                v.to_string()
            } else {
                records::text_to_memo(v)
            }
        }
        FieldKind::Number { blank } => {
            if clear {
                if blank {
                    String::new()
                } else {
                    return Err(format!("{label} needs a number."));
                }
            } else {
                number_text(text).ok_or_else(|| format!("{label} must be a number."))?
            }
        }
        FieldKind::Date => {
            let origin = p.model.origin_serial;
            crate::date_string(origin, crate::parse_relative(origin, text)?)
        }
        FieldKind::Choice(list) => {
            if clear {
                String::new()
            } else {
                choice(list, text).ok_or_else(|| {
                    let names: Vec<&str> = list.iter().map(|(_, l)| *l).collect();
                    format!("{text:?} is not a {label}; use {}.", names.join(", "))
                })?
            }
        }
        FieldKind::Flag => match text.to_ascii_lowercase().as_str() {
            "y" | "yes" | "true" | "1" => "Y".into(),
            "n" | "no" | "false" | "0" | "" => "N".into(),
            _ => return Err(format!("{label} is true or false.")),
        },
        FieldKind::Dictionary { table, key, name } => {
            if clear {
                String::new()
            } else {
                let list = dictionary(doc, table, key, name);
                let by_name: Vec<&String> = list
                    .iter()
                    .filter(|(_, n)| n.trim().eq_ignore_ascii_case(text))
                    .map(|(k, _)| k)
                    .collect();
                match by_name.as_slice() {
                    [k] => (*k).clone(),
                    [] => list
                        .iter()
                        .find(|(k, _)| k.trim() == text)
                        .map(|(k, _)| k.clone())
                        .ok_or_else(|| format!("No {label} {text:?}."))?,
                    _ => return Err(format!("{label} {text:?} is not unique; name it by id.")),
                }
            }
        }
        FieldKind::Activity => {
            if clear {
                String::new()
            } else {
                let found: Vec<i64> = p
                    .model
                    .activities
                    .iter()
                    .filter(|a| a.task_code == text)
                    .map(|a| a.task_id)
                    .collect();
                match found.as_slice() {
                    [id] => id.to_string(),
                    [] => return Err(format!("No activity {text:?}.")),
                    _ => return Err(format!("Activity ID {text:?} is not unique.")),
                }
            }
        }
        FieldKind::Wbs => {
            if clear {
                String::new()
            } else {
                wbs::find_path(p, text)
                    .ok_or_else(|| format!("No WBS element {text:?}."))?
                    .to_string()
            }
        }
        FieldKind::Resource => {
            if clear {
                String::new()
            } else {
                super::tables::resource_id(p, text)?.to_string()
            }
        }
        FieldKind::Parameter => {
            let list = thresholds::parameters(doc);
            let t = text.to_ascii_lowercase();
            list.iter()
                .find(|q| {
                    q.name.eq_ignore_ascii_case(text)
                        || q.field.eq_ignore_ascii_case(text)
                        || q.name.split(" (").next().unwrap_or("").to_ascii_lowercase() == t
                })
                .map(|q| q.id.to_string())
                .ok_or_else(|| {
                    let names: Vec<&str> = list.iter().map(|q| q.name.as_str()).collect();
                    format!("No threshold parameter {text:?}; use {}.", names.join(", "))
                })?
        }
    })
}

/// Set one named field of a record; true when the record changed.
pub fn set_field(
    p: &mut ProjectData,
    kind: Kind,
    id: i64,
    name: &str,
    value: Option<&str>,
) -> R<bool> {
    let field = field(kind, name)?;
    let table = kind.table(&p.document);
    if row_of(&p.document, table, kind.key(), id).is_none() {
        return Err(format!("No {} {id}.", kind.name()));
    }
    let text = stored(p, kind, &field, value)?;
    let pid = p.model.project.proj_id;
    Ok(match (kind, field.kind) {
        (Kind::Expense, _) => {
            if field.kind == FieldKind::Activity && text.is_empty() {
                return Err("An expense item needs an activity.".into());
            }
            set_expense(p, id, field.column, text)
        }
        (Kind::Issue, FieldKind::Activity) if !text.is_empty() => {
            set_issue_activity(p, id, num(&text))
        }
        (Kind::Issue, FieldKind::Wbs) if !text.is_empty() => set_issue_wbs(p, id, num(&text)),
        (Kind::Threshold, FieldKind::Parameter) => {
            thresholds::ensure_parameters(&mut p.document);
            set_value(&mut p.document, table, kind.key(), id, field.column, text)
        }
        _ => {
            let _ = pid;
            set_value(&mut p.document, table, kind.key(), id, field.column, text)
        }
    })
}

/// Assign (or with `assign` false remove) a risk's activity.
pub fn set_risk_activity(p: &mut ProjectData, risk: i64, task_id: i64, assign: bool) -> bool {
    let pid = p.model.project.proj_id;
    if assign {
        assign_risk(&mut p.document, pid, risk, task_id)
    } else {
        unassign_risk(&mut p.document, pid, risk, task_id)
    }
}

/// A record's fields as an agent names them: the value shown for each field
/// (choices by label, dictionary entries by name, activities by Activity
/// ID, WBS elements by path, resources by short name, dates as stored).
pub fn record_values(p: &ProjectData, kind: Kind, id: i64) -> Vec<(&'static str, String)> {
    let doc = &p.document;
    let table = kind.table(doc);
    let parms = thresholds::parameters(doc);
    fields(kind)
        .iter()
        .map(|f| {
            let raw = value(doc, table, kind.key(), id, f.column);
            let shown = match f.kind {
                FieldKind::Choice(list) => {
                    if raw.is_empty() {
                        raw
                    } else {
                        label_of(list, &raw)
                    }
                }
                FieldKind::Flag => if raw == "Y" { "true" } else { "false" }.into(),
                FieldKind::Dictionary { table, key, name } => {
                    name_of(&dictionary(doc, table, key, name), &raw)
                }
                FieldKind::Activity => {
                    if num(&raw) == 0 {
                        String::new()
                    } else {
                        activity_label(p, num(&raw)).0
                    }
                }
                FieldKind::Wbs => wbs_code_path(p, num(&raw)),
                FieldKind::Resource => doc
                    .table("RSRC")
                    .and_then(|t| {
                        t.rows
                            .iter()
                            .find(|r| num(t.value(r, "rsrc_id")) == num(&raw) && !raw.is_empty())
                            .map(|r| t.value(r, "rsrc_short_name").to_string())
                    })
                    .unwrap_or_default(),
                FieldKind::Parameter => parms
                    .iter()
                    .find(|q| q.id == num(&raw))
                    .map_or(String::new(), |q| q.name.clone()),
                FieldKind::ReadOnly if f.column == "thresh_parm_id" => parms
                    .iter()
                    .find(|q| q.id == num(&raw) && !raw.is_empty())
                    .map_or(String::new(), |q| q.name.clone()),
                FieldKind::Number { .. } | FieldKind::ReadOnly => {
                    if raw.trim().is_empty() {
                        raw
                    } else if f.column.ends_with("_value") || f.kind != FieldKind::ReadOnly {
                        format_number(float(&raw))
                    } else {
                        raw
                    }
                }
                FieldKind::Html => records::memo_to_text(&raw),
                FieldKind::Text | FieldKind::Memo | FieldKind::Date => raw,
            };
            (f.name, shown)
        })
        .collect()
}

/// Today as an XER date (midnight), the stamp of records added by hand.
pub fn today() -> String {
    chrono::Local::now()
        .date_naive()
        .format("%Y-%m-%d 00:00")
        .to_string()
}

// ------------------------------------------------------- activity feedback

/// Set an activity's notes to resources (`TASKNOTE`, HTML as stored).
pub fn set_notes_to_resources(doc: &mut Document, task_id: i64, pid: i64, html: String) {
    match records::rows_for(doc, "TASKNOTE", task_id, pid).first() {
        Some(&r) => records::set(doc, "TASKNOTE", r, "task_notes", html),
        None => {
            records::add_row(
                doc,
                "TASKNOTE",
                &[
                    ("task_id", task_id.to_string()),
                    ("proj_id", pid.to_string()),
                    ("task_notes", html),
                ],
            );
        }
    }
}

/// Set whether an activity's feedback needs review (`rev_fdbk_flag`).
pub fn set_review_required(doc: &mut Document, task_id: i64, pid: i64, on: bool) {
    let flag = if on { "Y" } else { "N" };
    records::set_task_value(doc, task_id, pid, "rev_fdbk_flag", flag.into());
}
