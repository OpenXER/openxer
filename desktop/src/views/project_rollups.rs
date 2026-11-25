//! The Projects view columns: what each project and EPS folder shows, with
//! the rollup rules of `openxer_xer::workspace::rollup` (shared with
//! `openxer-cli eps list`), and how the table sorts, groups and filters.
use super::projects::{ProjectSummary, STATUSES};
use crate::format::{self, Periods};
use crate::grid::columns::{default_align, Align, Kind};
use openxer_xer::workspace::rollup::Rollup;
use std::cmp::Ordering;
use std::collections::BTreeMap;

pub const TITLES: [&str; 36] = [
    "Project ID",
    "Project Name",
    "Total Activities",
    "Strategic Priority",
    "Status",
    "Start",
    "Finish",
    "Data Date",
    "Must Finish By",
    "Late Start",
    "Late Finish",
    "Baseline Finish",
    "Finish Variance",
    "Total Float",
    "Schedule % Complete (duration)",
    "Budgeted Labor Units",
    "Actual Labor Units",
    "Remaining Labor Units",
    "Budgeted Total Cost",
    "Actual Total Cost",
    "Remaining Total Cost",
    "Not Started",
    "In Progress",
    "Completed",
    "Project Leveling Priority",
    "Responsible Manager",
    "Planned Start",
    "Actual Start",
    "Actual Finish",
    "Anticipated Start",
    "Anticipated Finish",
    "Original Budget",
    "Current Budget",
    "Proposed Budget",
    "Total Spending Plan",
    "Total Benefit Plan",
];
pub const WIDTHS: [f32; 36] = [
    150., 150., 70., 70., 70., 100., 100., 100., 100., 100., 100., 100., 70., 60., 135., 90., 90.,
    90., 100., 100., 100., 60., 60., 70., 70., 140., 100., 100., 100., 100., 100., 100., 100.,
    100., 100., 100.,
];
/// What each column holds; its alignment follows the activity table's
/// defaults (`columns::default_align`: text and dates left, numbers right).
pub const KINDS: [Kind; 36] = [
    Kind::Text,
    Kind::Text,
    Kind::Number,
    Kind::Number,
    Kind::Text,
    Kind::Date,
    Kind::Date,
    Kind::Date,
    Kind::Date,
    Kind::Date,
    Kind::Date,
    Kind::Date,
    Kind::Duration,
    Kind::Duration,
    Kind::Percent,
    Kind::Units,
    Kind::Units,
    Kind::Units,
    Kind::Cost,
    Kind::Cost,
    Kind::Cost,
    Kind::Number,
    Kind::Number,
    Kind::Number,
    Kind::Number,
    Kind::Text,
    Kind::Date,
    Kind::Date,
    Kind::Date,
    Kind::Date,
    Kind::Date,
    Kind::Cost,
    Kind::Cost,
    Kind::Cost,
    Kind::Cost,
    Kind::Cost,
];

const GENERAL: &str = "General";
const ACTIVITIES: &str = "Number of Activities";
const DATES: &str = "Dates";
const DURATIONS: &str = "Durations";
const PERCENTS: &str = "Percent Completes";
const UNITS: &str = "Units";
const COSTS: &str = "Costs";
const BUDGET: &str = "Budget";
/// The Columns chooser group of the project code columns.
pub const CODES: &str = "Project Codes";

/// The Columns chooser group of each column.
pub const GROUPS: [&str; 36] = [
    GENERAL, GENERAL, ACTIVITIES, GENERAL, GENERAL, DATES, DATES, DATES, DATES, DATES, DATES,
    DATES, DURATIONS, DURATIONS, PERCENTS, UNITS, UNITS, UNITS, COSTS, COSTS, COSTS, ACTIVITIES,
    ACTIVITIES, ACTIVITIES, GENERAL, GENERAL, DATES, DATES, DATES, DATES, DATES, BUDGET, BUDGET,
    BUDGET, BUDGET, BUDGET,
];

/// The cell commands' field id of each column; the open project's fields
/// that are typed in the table have their own names.
pub const IDS: [&str; 36] = [
    "project_id",
    "project_name",
    "total_activities",
    "strategic_priority",
    "project_status",
    "start",
    "finish",
    "data_date",
    "must_finish_by",
    "late_start",
    "late_finish",
    "baseline_finish",
    "finish_variance",
    "total_float",
    "schedule_percent_complete",
    "budgeted_labor_units",
    "actual_labor_units",
    "remaining_labor_units",
    "budgeted_total_cost",
    "actual_total_cost",
    "remaining_total_cost",
    "not_started",
    "in_progress",
    "completed",
    "leveling_priority",
    "responsible_manager",
    "planned_start",
    "actual_start",
    "actual_finish",
    "anticipated_start",
    "anticipated_finish",
    "original_budget",
    "current_budget",
    "proposed_budget",
    "total_spending_plan",
    "total_benefit_plan",
];

/// Extra field id of project code type `type_id` (an extra column of the
/// Projects table, after its own).
pub fn code_id(type_id: i64) -> String {
    format!("pcat_{type_id}")
}

/// The project code type of extra field id `id`.
pub fn code_type_of(id: &str) -> Option<i64> {
    id.strip_prefix("pcat_")?.parse().ok()
}

/// Project code types of `doc`: (type id, name), in table order.
pub fn code_types(doc: &openxer_xer::Document) -> Vec<(i64, String)> {
    doc.table("PCATTYPE").map_or(vec![], |t| {
        t.rows
            .iter()
            .map(|r| {
                (
                    t.value(r, "proj_catg_type_id").trim().parse().unwrap_or(0),
                    t.value(r, "proj_catg_type").to_string(),
                )
            })
            .collect()
    })
}

/// Project `key`'s project code values in `doc`, by code type: the stored
/// assignments (PROJPCAT), shown read-only by their code value. Assigning
/// a value is Project Details, Codes; the table reads what is stored.
pub fn code_values(doc: &openxer_xer::Document, key: i64) -> BTreeMap<i64, String> {
    let values: BTreeMap<String, String> = doc.table("PCATVAL").map_or(BTreeMap::new(), |t| {
        t.rows
            .iter()
            .map(|r| {
                (
                    t.value(r, "proj_catg_id").to_string(),
                    t.value(r, "proj_catg_short_name").to_string(),
                )
            })
            .collect()
    });
    let key = key.to_string();
    doc.table("PROJPCAT").map_or(BTreeMap::new(), |t| {
        t.rows
            .iter()
            .filter(|r| t.value(r, "proj_id").trim() == key)
            .map(|r| {
                (
                    t.value(r, "proj_catg_type_id").trim().parse().unwrap_or(0),
                    values
                        .get(t.value(r, "proj_catg_id"))
                        .cloned()
                        .unwrap_or_default(),
                )
            })
            .collect()
    })
}

/// The right-aligned columns.
pub fn right_columns() -> Vec<usize> {
    (0..KINDS.len())
        .filter(|&i| default_align(KINDS[i]) == Align::Right)
        .collect()
}
/// The columns a new Projects view showed before the reference's
/// defaults (`defaults::PROJECT_COLUMNS`), in order.
pub const LEGACY_ORDER: [usize; 11] = [0, 1, 4, 5, 6, 7, 12, 13, 14, 2, 3];

/// The columns a new Projects view shows, in order.
pub fn default_order() -> Vec<usize> {
    if crate::defaults::reference() {
        crate::defaults::positions(&TITLES, crate::defaults::PROJECT_COLUMNS)
    } else {
        LEGACY_ORDER.to_vec()
    }
}

/// How the Projects table groups its projects (Group and Sort By).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Group {
    /// Under their EPS folders (the default).
    #[default]
    Eps,
    /// One list without bands.
    List,
    /// Under their responsible manager.
    Manager,
    /// Under their project status.
    Status,
    /// Under their project leveling priority.
    Priority,
}

impl Group {
    /// Group and Sort By's choices in menu order: (group, menu key, caption).
    pub const MENU: [(Group, &'static str, &'static str); 5] = [
        (Group::Eps, "group_eps", "EPS"),
        (Group::List, "group_list", "List"),
        (Group::Manager, "group_manager", "Responsible Manager"),
        (Group::Status, "group_status", "Status"),
        (
            Group::Priority,
            "group_priority",
            "Project Leveling Priority",
        ),
    ];

    /// The name a layout stores.
    pub fn key(self) -> &'static str {
        match self {
            Group::Eps => "eps",
            Group::List => "list",
            Group::Manager => "manager",
            Group::Status => "status",
            Group::Priority => "priority",
        }
    }

    pub fn from_key(key: &str) -> Option<Group> {
        [
            Group::Eps,
            Group::List,
            Group::Manager,
            Group::Status,
            Group::Priority,
        ]
        .into_iter()
        .find(|g| g.key() == key)
    }

    /// The menu choice of key `key` (`group_eps`, ...).
    pub fn of_menu(key: &str) -> Option<Group> {
        Group::MENU.iter().find(|m| m.1 == key).map(|m| m.0)
    }
}

/// Bands of the Projects table: (band id, depth, projects under it).
pub type Bands = Vec<(i64, usize, Vec<i64>)>;

/// Projects grouped by a value of theirs (`value`): one band per value in
/// sorted order, the projects without one last under "(none)". Band ids
/// are negative so they never meet an EPS node id; the names come back in
/// band order (band `-(k + 1)` is `names[k]`).
pub fn value_groups(
    projects: &[ProjectSummary],
    keys: &[i64],
    value: impl Fn(&ProjectSummary) -> String,
) -> (Bands, Vec<String>) {
    let mut by: BTreeMap<(bool, String), Vec<i64>> = BTreeMap::new();
    for (s, id) in projects.iter().zip(keys) {
        let v = value(s);
        by.entry((v.is_empty(), v)).or_default().push(*id);
    }
    let mut groups = vec![];
    let mut names = vec![];
    for (k, ((blank, name), ids)) in by.into_iter().enumerate() {
        groups.push((-(k as i64) - 1, 0, ids));
        names.push(if blank { "(none)".to_string() } else { name });
    }
    (groups, names)
}

/// The column the projects are sorted on inside each group.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sort {
    pub field: usize,
    pub descending: bool,
}

/// A header click: ascending, then descending, then back to Project ID order.
pub fn next_sort(now: Option<Sort>, field: usize) -> Option<Sort> {
    match now {
        Some(s) if s.field == field && !s.descending => Some(Sort {
            field,
            descending: true,
        }),
        Some(s) if s.field == field => None,
        _ => Some(Sort {
            field,
            descending: false,
        }),
    }
}

pub fn status_label(code: &str) -> &'static str {
    STATUSES
        .iter()
        .find(|(k, _)| *k == code)
        .map_or("", |(_, v)| v)
}

/// The project status code (the project node's `status_code`).
pub fn status_of(s: &ProjectSummary) -> &str {
    s.node.get("status_code").map_or("", String::as_str)
}

fn periods(day_hours_x100: i64) -> Periods {
    let day = if day_hours_x100 > 0 {
        (day_hours_x100 * 60 / 100).max(1)
    } else {
        Periods::DEFAULT.day
    };
    Periods {
        day,
        ..Periods::DEFAULT
    }
}

fn date(v: Option<i64>) -> String {
    format::date(0, v)
}

/// Schedule % Complete as shown: one decimal.
pub fn percent_text(v: Option<f64>) -> String {
    v.map_or(String::new(), |v| format!("{v:.1}%"))
}

/// The cells of a project row, or of a folder row when `folder`.
pub fn cells(s: &ProjectSummary, folder: bool) -> Vec<String> {
    let r = &s.rollup;
    let p = periods(r.float_day_hours_x100);
    let measure = periods(r.measure().day_hours_x100);
    let count = |n: usize| n.to_string();
    vec![
        s.id.clone(),
        s.name.clone(),
        s.activities.to_string(),
        s.project
            .get("strgy_priority_num")
            .cloned()
            .unwrap_or_default(),
        if folder {
            String::new()
        } else {
            status_label(status_of(s)).to_string()
        },
        date(s.start),
        date(s.finish),
        date(r.data_date),
        date(r.must_finish),
        date(r.late_start),
        date(r.late_finish),
        date(r.baseline_finish),
        r.finish_variance()
            .map_or(String::new(), |v| format::float(Some(v), measure)),
        r.total_float
            .map_or(String::new(), |v| format::float(Some(v), p)),
        percent_text(r.schedule_percent()),
        format::units(r.labor_units.budgeted),
        format::units(r.labor_units.actual),
        format::units(r.labor_units.remaining),
        format::cost(r.costs.budgeted),
        format::cost(r.costs.actual),
        format::cost(r.costs.remaining),
        count(r.not_started),
        count(r.in_progress),
        count(r.completed),
        if folder {
            String::new()
        } else {
            s.project.get("priority_num").cloned().unwrap_or_default()
        },
        s.manager.clone(),
        date(s.planned_start),
        date(s.actual_start),
        date(s.actual_finish),
        date(s.anticipated.0),
        date(s.anticipated.1),
        money(s.budget.map(|b| b[0])),
        money(s.budget.map(|b| b[1])),
        money(s.budget.map(|b| b[2])),
        money(s.plan.map(|p| p[0])),
        money(s.plan.map(|p| p[1])),
    ]
}

fn money(v: Option<f64>) -> String {
    v.map_or(String::new(), format::cost)
}

/// What a column sorts on.
#[derive(Clone, Debug, PartialEq, PartialOrd)]
enum Key {
    Num(f64),
    Text(String),
}

fn key(s: &ProjectSummary, field: usize) -> Option<Key> {
    let r = &s.rollup;
    let n = |v: Option<i64>| v.map(|v| Key::Num(v as f64));
    let f = |v: f64| Some(Key::Num(v));
    match field {
        0 => Some(Key::Text(s.id.to_lowercase())),
        1 => Some(Key::Text(s.name.to_lowercase())),
        2 => f(s.activities as f64),
        3 => s
            .project
            .get("strgy_priority_num")
            .and_then(|v| v.trim().parse().ok())
            .map(Key::Num),
        4 => Some(Key::Text(status_label(status_of(s)).into())),
        5 => n(s.start),
        6 => n(s.finish),
        7 => n(r.data_date),
        8 => n(r.must_finish),
        9 => n(r.late_start),
        10 => n(r.late_finish),
        11 => n(r.baseline_finish),
        12 => n(r.finish_variance()),
        13 => n(r.total_float),
        14 => r.schedule_percent().map(Key::Num),
        15 => f(r.labor_units.budgeted),
        16 => f(r.labor_units.actual),
        17 => f(r.labor_units.remaining),
        18 => f(r.costs.budgeted),
        19 => f(r.costs.actual),
        20 => f(r.costs.remaining),
        21 => f(r.not_started as f64),
        22 => f(r.in_progress as f64),
        23 => f(r.completed as f64),
        24 => s
            .project
            .get("priority_num")
            .and_then(|v| v.trim().parse().ok())
            .map(Key::Num),
        25 => (!s.manager.is_empty()).then(|| Key::Text(s.manager.to_lowercase())),
        26 => n(s.planned_start),
        27 => n(s.actual_start),
        28 => n(s.actual_finish),
        29 => n(s.anticipated.0),
        30 => n(s.anticipated.1),
        31..=33 => s.budget.map(|b| Key::Num(b[field - 31])),
        34 | 35 => s.plan.map(|p| Key::Num(p[field - 34])),
        _ => None,
    }
}

/// Order two projects by `sort`; blanks go last either way, ties by
/// Project ID.
pub fn compare(a: &ProjectSummary, b: &ProjectSummary, sort: Sort) -> Ordering {
    let by_id = || a.id.cmp(&b.id).then(a.source.cmp(&b.source));
    let ordered = match (key(a, sort.field), key(b, sort.field)) {
        (Some(x), Some(y)) => {
            let o = x.partial_cmp(&y).unwrap_or(Ordering::Equal);
            if sort.descending {
                o.reverse()
            } else {
                o
            }
        }
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    };
    ordered.then_with(by_id)
}

/// A folder (or status group) row: the rollup of `projects`.
pub fn folder_summary(id: String, name: String, projects: &[&ProjectSummary]) -> ProjectSummary {
    let mut rollup = Rollup::default();
    for p in projects {
        rollup.add(&p.rollup);
    }
    let mut s = ProjectSummary {
        id,
        name,
        activities: projects.iter().map(|p| p.activities).sum(),
        start: projects.iter().filter_map(|p| p.start).min(),
        finish: projects.iter().filter_map(|p| p.finish).max(),
        data_date: rollup.data_date,
        must_finish: rollup.must_finish,
        actual_start: rollup.actual_start,
        actual_finish: rollup.actual_finish,
        rollup,
        ..Default::default()
    };
    s.project.insert("strgy_priority_num".into(), "500".into());
    s
}

/// Projects grouped by status: one band per status that has projects, in
/// the status list's order, then the projects without one. Band ids are
/// negative so they never meet an EPS node id.
pub fn status_groups(projects: &[ProjectSummary], keys: &[i64]) -> Vec<(i64, usize, Vec<i64>)> {
    let mut out = vec![];
    for k in 0..=STATUSES.len() {
        let code = STATUSES.get(k).map_or("", |(c, _)| *c);
        let ids: Vec<i64> = projects
            .iter()
            .zip(keys)
            .filter(|(s, _)| {
                let own = status_of(s);
                if k == STATUSES.len() {
                    !STATUSES.iter().any(|(c, _)| *c == own)
                } else {
                    own == code
                }
            })
            .map(|(_, id)| *id)
            .collect();
        if !ids.is_empty() {
            out.push((-(k as i64) - 1, 0, ids));
        }
    }
    out
}

/// The caption of status band `id` (from [`status_groups`]).
pub fn status_group_name(id: i64) -> &'static str {
    let k = (-id - 1) as usize;
    STATUSES.get(k).map_or("No Status", |(_, label)| label)
}
