//! More activity columns for the registry: earned value, percent completes
//! from units, costs, the baseline schedule and performance, activity counts,
//! activity steps, feedback, and General, Dates and Durations additions.
//! Values the file stores (labor units, dates, priorities) are read as
//! stored; the rest is derived from them (see `columns_ev` for earned value).
use super::columns::{self, Column, Editor, Field, Index, Kind, Measure, Source, Value};
use super::columns_ev::{self as ev, Basis, EvMeasure};
use super::*;
use crate::records::{self, float, num};
use openxer_xer::ActivityDetails;
use std::cell::OnceCell;
use std::collections::HashMap;

pub const EARNED_VALUE: &str = "Earned Value";
pub const NUMBER_OF_ACTIVITIES: &str = "Number of Activities";
pub const STEPS: &str = "Activity Steps";
pub const FEEDBACK: &str = "Feedback";

/// A column defined here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Ext {
    ProjectId,
    ProjectName,
    LevelingPriority,
    ResourceIds,
    Resources,
    Roles,
    AddedBy,
    AddedDate,
    LastModifiedBy,
    LastModifiedDate,
    ExternalEarlyStart,
    ExternalLateFinish,
    RemainingFloat,
    UnitsPercent,
    LaborUnitsPercent,
    NonlaborUnitsPercent,
    CostPercent,
    SchedulePercent,
    PerformancePercent,
    ActivityCount,
    CompletedCount,
    InProgressCount,
    NotStartedCount,
    StepCount,
    CompletedSteps,
    StepsPercent,
    Feedback,
    NewFeedback,
    Ev(EvMeasure, Basis),
}

/// (field, id, title, group, width, kind) of the fixed columns.
const FIXED: [(Ext, &str, &str, &str, f32, Kind); 28] = [
    (
        Ext::ProjectId,
        "project_id",
        "Project ID",
        columns::GENERAL,
        90.,
        Kind::Text,
    ),
    (
        Ext::ProjectName,
        "project_name",
        "Project Name",
        columns::GENERAL,
        160.,
        Kind::Text,
    ),
    (
        Ext::LevelingPriority,
        "leveling_priority",
        "Leveling Priority",
        columns::GENERAL,
        70.,
        Kind::Text,
    ),
    (
        Ext::ResourceIds,
        "resource_ids",
        "Resource IDs",
        columns::GENERAL,
        120.,
        Kind::Text,
    ),
    (
        Ext::Resources,
        "resources",
        "Resources",
        columns::GENERAL,
        150.,
        Kind::Text,
    ),
    (
        Ext::Roles,
        "roles",
        "Roles",
        columns::GENERAL,
        120.,
        Kind::Text,
    ),
    (
        Ext::AddedBy,
        "added_by",
        "Added By",
        columns::GENERAL,
        90.,
        Kind::Text,
    ),
    (
        Ext::AddedDate,
        "added_date",
        "Added Date",
        columns::GENERAL,
        95.,
        Kind::Date,
    ),
    (
        Ext::LastModifiedBy,
        "last_modified_by",
        "Last Modified By",
        columns::GENERAL,
        90.,
        Kind::Text,
    ),
    (
        Ext::LastModifiedDate,
        "last_modified_date",
        "Last Modified Date",
        columns::GENERAL,
        95.,
        Kind::Date,
    ),
    (
        Ext::ExternalEarlyStart,
        "external_early_start",
        "External Early Start",
        columns::DATES,
        95.,
        Kind::Date,
    ),
    (
        Ext::ExternalLateFinish,
        "external_late_finish",
        "External Late Finish",
        columns::DATES,
        95.,
        Kind::Date,
    ),
    (
        Ext::RemainingFloat,
        "remaining_float",
        "Remaining Float",
        columns::DURATIONS,
        65.,
        Kind::Duration,
    ),
    (
        Ext::UnitsPercent,
        "units_percent_complete",
        "Units % Complete",
        columns::PERCENTS,
        70.,
        Kind::Percent,
    ),
    (
        Ext::LaborUnitsPercent,
        "labor_units_percent_complete",
        "Labor Units % Complete",
        columns::PERCENTS,
        70.,
        Kind::Percent,
    ),
    (
        Ext::NonlaborUnitsPercent,
        "nonlabor_units_percent_complete",
        "Nonlabor Units % Complete",
        columns::PERCENTS,
        70.,
        Kind::Percent,
    ),
    (
        Ext::CostPercent,
        "cost_percent_complete",
        "Cost % Complete",
        columns::PERCENTS,
        70.,
        Kind::Percent,
    ),
    (
        Ext::SchedulePercent,
        "schedule_percent_complete",
        "Schedule % Complete",
        columns::PERCENTS,
        70.,
        Kind::Percent,
    ),
    (
        Ext::PerformancePercent,
        "performance_percent_complete",
        "Performance % Complete",
        columns::PERCENTS,
        70.,
        Kind::Percent,
    ),
    (
        Ext::ActivityCount,
        "number_of_activities",
        "Number of Activities",
        NUMBER_OF_ACTIVITIES,
        70.,
        Kind::Number,
    ),
    (
        Ext::CompletedCount,
        "number_of_completed_activities",
        "Number of Completed Activities",
        NUMBER_OF_ACTIVITIES,
        70.,
        Kind::Number,
    ),
    (
        Ext::InProgressCount,
        "number_of_in_progress_activities",
        "Number of In Progress Activities",
        NUMBER_OF_ACTIVITIES,
        70.,
        Kind::Number,
    ),
    (
        Ext::NotStartedCount,
        "number_of_not_started_activities",
        "Number of Not Started Activities",
        NUMBER_OF_ACTIVITIES,
        70.,
        Kind::Number,
    ),
    (
        Ext::StepCount,
        "number_of_steps",
        "Number of Steps",
        STEPS,
        60.,
        Kind::Number,
    ),
    (
        Ext::CompletedSteps,
        "completed_steps",
        "Completed Steps",
        STEPS,
        60.,
        Kind::Number,
    ),
    (
        Ext::StepsPercent,
        "steps_percent_complete",
        "Steps % Complete",
        STEPS,
        70.,
        Kind::Percent,
    ),
    (
        Ext::Feedback,
        "feedback",
        "Feedback",
        FEEDBACK,
        160.,
        Kind::Text,
    ),
    (
        Ext::NewFeedback,
        "new_feedback",
        "New Feedback",
        FEEDBACK,
        60.,
        Kind::Flag,
    ),
];

fn column(field: Ext, id: &str, title: &str, group: &str, width: f32, kind: Kind) -> Column {
    Column {
        id: id.into(),
        title: title.into(),
        group: group.into(),
        width,
        align: super::columns::default_align(kind),
        kind,
        editor: Editor::ReadOnly,
        source: Source::Field(Field::Ext(field)),
    }
}

/// The columns defined here, for the registry.
pub fn columns() -> Vec<Column> {
    let mut out: Vec<Column> = FIXED
        .iter()
        .map(|&(f, id, title, group, width, kind)| column(f, id, title, group, width, kind))
        .collect();
    for basis in [Basis::Cost, Basis::LaborUnits] {
        for m in EvMeasure::ALL {
            let Some((id, title)) = m.caption(basis) else {
                continue;
            };
            let kind = if m.is_index() {
                Kind::Number
            } else if basis == Basis::Cost {
                Kind::Cost
            } else {
                Kind::Units
            };
            out.push(column(
                Ext::Ev(m, basis),
                id,
                title,
                EARNED_VALUE,
                80.,
                kind,
            ));
        }
    }
    out
}

/// One activity's resource assignments as the list columns show them.
#[derive(Clone, Debug, Default)]
struct Assigned {
    ids: Vec<String>,
    names: Vec<String>,
    roles: Vec<String>,
}

/// Steps of one activity: (count, completed, weighted percent).
type Steps = (usize, usize, Option<f64>);

/// Lookups for these columns, built when a column first needs them.
#[derive(Default)]
pub struct Cache {
    /// `TASK` row per task id (any project of the file).
    tasks: OnceCell<HashMap<i64, usize>>,
    /// Project ID and name per project id.
    projects: OnceCell<HashMap<i64, (String, String)>>,
    assigned: OnceCell<HashMap<i64, Assigned>>,
    steps: OnceCell<HashMap<i64, Steps>>,
    feedback: OnceCell<HashMap<i64, String>>,
    pub(crate) ev: ev::EvCache,
}

impl Cache {
    fn task(&self, p: &ProjectData, task_id: i64, column: &str) -> String {
        let rows = self.tasks.get_or_init(|| {
            let pid = p.model.project.proj_id;
            let mut map = HashMap::new();
            if let Some(t) = p.document.table("TASK") {
                for (n, r) in t.rows.iter().enumerate() {
                    let id = num(t.value(r, "task_id"));
                    // The open project's own row wins over another project's.
                    if num(t.value(r, "proj_id")) == pid || !map.contains_key(&id) {
                        map.insert(id, n);
                    }
                }
            }
            map
        });
        rows.get(&task_id)
            .map(|&n| records::get(&p.document, "TASK", n, column))
            .unwrap_or_default()
    }

    fn project(&self, p: &ProjectData, proj_id: i64) -> (String, String) {
        self.projects
            .get_or_init(|| {
                let doc = &p.document;
                let mut map: HashMap<i64, (String, String)> = HashMap::new();
                if let Some(t) = doc.table("PROJECT") {
                    for r in &t.rows {
                        map.entry(num(t.value(r, "proj_id"))).or_default().0 =
                            t.value(r, "proj_short_name").to_string();
                    }
                }
                if let Some(t) = doc.table("PROJWBS") {
                    for r in &t.rows {
                        if t.value(r, "proj_node_flag") == "Y" {
                            map.entry(num(t.value(r, "proj_id"))).or_default().1 =
                                t.value(r, "wbs_name").to_string();
                        }
                    }
                }
                map
            })
            .get(&proj_id)
            .cloned()
            .unwrap_or_default()
    }

    fn assigned(&self, p: &ProjectData, task_id: i64) -> Assigned {
        self.assigned
            .get_or_init(|| {
                let doc = &p.document;
                let names = |table: &str, key: &str, short: &str, long: &str| {
                    doc.table(table).map_or_else(HashMap::new, |t| {
                        t.rows
                            .iter()
                            .map(|r| {
                                (
                                    num(t.value(r, key)),
                                    (t.value(r, short).to_string(), t.value(r, long).to_string()),
                                )
                            })
                            .collect::<HashMap<i64, (String, String)>>()
                    })
                };
                let resources = names("RSRC", "rsrc_id", "rsrc_short_name", "rsrc_name");
                let mut roles = names("ROLES", "role_id", "role_short_name", "role_name");
                if roles.is_empty() {
                    roles = names("ROLE", "role_id", "role_short_name", "role_name");
                }
                let mut map: HashMap<i64, Assigned> = HashMap::new();
                if let Some(t) = doc.table("TASKRSRC") {
                    for r in &t.rows {
                        let e = map.entry(num(t.value(r, "task_id"))).or_default();
                        let rsrc = num(t.value(r, "rsrc_id"));
                        if rsrc != 0 {
                            let (id, name) = resources
                                .get(&rsrc)
                                .cloned()
                                .unwrap_or_else(|| (rsrc.to_string(), rsrc.to_string()));
                            let id = if id.is_empty() { name.clone() } else { id };
                            if !e.ids.contains(&id) {
                                e.ids.push(id);
                            }
                            if !e.names.contains(&name) {
                                e.names.push(name);
                            }
                        }
                        let role = num(t.value(r, "role_id"));
                        if role != 0 {
                            let (short, name) = roles
                                .get(&role)
                                .cloned()
                                .unwrap_or_else(|| (role.to_string(), role.to_string()));
                            let name = if name.is_empty() { short } else { name };
                            if !e.roles.contains(&name) {
                                e.roles.push(name);
                            }
                        }
                    }
                }
                map
            })
            .get(&task_id)
            .cloned()
            .unwrap_or_default()
    }

    fn steps(&self, p: &ProjectData, task_id: i64) -> Steps {
        self.steps
            .get_or_init(|| {
                // (count, completed, weight, weighted percent, unweighted percent)
                let mut sums: HashMap<i64, (usize, usize, f64, f64, f64)> = HashMap::new();
                if let Some(t) = p.document.table("TASKPROC") {
                    let has_pct = t.field("complete_pct").is_some();
                    for r in &t.rows {
                        let done = t.value(r, "complete_flag") == "Y";
                        let pct = if done {
                            100.
                        } else if has_pct {
                            float(t.value(r, "complete_pct")).clamp(0., 100.)
                        } else {
                            0.
                        };
                        let weight = float(t.value(r, "proc_wt")).max(0.);
                        let e = sums.entry(num(t.value(r, "task_id"))).or_default();
                        e.0 += 1;
                        if done {
                            e.1 += 1;
                        }
                        e.2 += weight;
                        e.3 += weight * pct;
                        e.4 += pct;
                    }
                }
                sums.into_iter()
                    .map(|(id, (n, done, weight, weighted, plain))| {
                        let pct = if weight > 0. {
                            weighted / weight
                        } else {
                            plain / n.max(1) as f64
                        };
                        (id, (n, done, Some(pct.clamp(0., 100.))))
                    })
                    .collect()
            })
            .get(&task_id)
            .copied()
            .unwrap_or((0, 0, None))
    }

    fn feedback(&self, p: &ProjectData, task_id: i64) -> String {
        self.feedback
            .get_or_init(|| {
                let Some(t) = p.document.table("TASKFDBK") else {
                    return HashMap::new();
                };
                let text_field = if t.field("task_fdbk").is_some() {
                    "task_fdbk".to_string()
                } else {
                    t.fields
                        .iter()
                        .find(|f| f.ends_with("fdbk"))
                        .cloned()
                        .unwrap_or_default()
                };
                t.rows
                    .iter()
                    .map(|r| {
                        let text = records::memo_to_text(t.value(r, &text_field));
                        let line = text
                            .lines()
                            .map(str::trim)
                            .find(|l| !l.is_empty())
                            .unwrap_or("")
                            .to_string();
                        (num(t.value(r, "task_id")), line)
                    })
                    .collect()
            })
            .get(&task_id)
            .cloned()
            .unwrap_or_default()
    }
}

fn percent(actual: f64, at_completion: f64) -> f64 {
    if at_completion > 1e-9 {
        (actual / at_completion * 100.).clamp(0., 100.)
    } else {
        0.
    }
}

/// (actual, at completion) of a measure.
fn progress(p: &ProjectData, ix: &Index, task_id: i64, measures: &[Measure]) -> (f64, f64) {
    measures.iter().fold((0., 0.), |(a, t), m| {
        let f = ix.figures(p, task_id, *m).unwrap_or_default();
        (a + f.actual, t + f.actual + f.remaining)
    })
}

fn measures(e: Ext) -> &'static [Measure] {
    match e {
        Ext::UnitsPercent => &[Measure::LaborUnits, Measure::NonlaborUnits],
        Ext::LaborUnitsPercent => &[Measure::LaborUnits],
        Ext::NonlaborUnitsPercent => &[Measure::NonlaborUnits],
        Ext::CostPercent => &[Measure::TotalCost],
        _ => &[],
    }
}

/// Working minutes from `from` to `to` on the activity's calendar,
/// negative when `to` is earlier.
fn signed_work(p: &ProjectData, a: &Activity, from: i64, to: i64) -> i64 {
    let Some(cal) = p.model.calendars.get(a.cal) else {
        return to - from;
    };
    if to >= from {
        cal.between(from, to).max(0)
    } else {
        -cal.between(to, from).max(0)
    }
}

fn stored_date(p: &ProjectData, text: &str) -> Value {
    openxer_xer::parse_relative(p.model.origin_serial, text)
        .ok()
        .flatten()
        .map_or(Value::Empty, |minute| Value::Date {
            minute,
            mark: crate::format::DateMark::Plain,
        })
}

fn text(s: impl Into<String>) -> Value {
    let s = s.into();
    if s.is_empty() {
        Value::Empty
    } else {
        Value::Text(s)
    }
}

fn count(on: bool) -> Value {
    Value::Number(if on { 1. } else { 0. })
}

/// The value of column `e` for activity `i`.
pub fn value(
    p: &ProjectData,
    ix: &Index,
    a: &Activity,
    _d: Option<&ActivityDetails>,
    i: usize,
    e: Ext,
) -> Value {
    let c = &ix.ext;
    let task = |column: &str| c.task(p, a.task_id, column);
    match e {
        Ext::ProjectId | Ext::ProjectName => {
            let stored = num(&task("proj_id"));
            let pid = if stored == 0 {
                p.model.project.proj_id
            } else {
                stored
            };
            let (id, name) = c.project(p, pid);
            if e == Ext::ProjectId {
                text(id)
            } else if name.is_empty() && pid == p.model.project.proj_id {
                text(p.model.project.name.clone())
            } else {
                text(name)
            }
        }
        Ext::LevelingPriority => text(match task("priority_type").as_str() {
            "PT_Top" => "Top",
            "PT_High" => "High",
            "PT_Normal" => "Normal",
            "PT_Low" => "Low",
            "PT_Lowest" => "Lowest",
            _ => "",
        }),
        Ext::ResourceIds => text(c.assigned(p, a.task_id).ids.join(", ")),
        Ext::Resources => text(c.assigned(p, a.task_id).names.join(", ")),
        Ext::Roles => text(c.assigned(p, a.task_id).roles.join(", ")),
        Ext::AddedBy => text(task("create_user")),
        Ext::AddedDate => stored_date(p, &task("create_date")),
        Ext::LastModifiedBy => text(task("update_user")),
        Ext::LastModifiedDate => stored_date(p, &task("update_date")),
        Ext::ExternalEarlyStart => a
            .ext_early_start
            .map_or(Value::Empty, |minute| Value::Date {
                minute,
                mark: crate::format::DateMark::Plain,
            }),
        Ext::ExternalLateFinish => a.ext_late_end.map_or(Value::Empty, |minute| Value::Date {
            minute,
            mark: crate::format::DateMark::Plain,
        }),
        Ext::RemainingFloat => {
            if a.is_completed() {
                return Value::Empty;
            }
            match (
                a.reend.or(a.early_finish),
                a.rem_late_finish.or(a.late_finish),
            ) {
                (Some(early), Some(late)) => Value::Duration(signed_work(p, a, early, late)),
                _ => Value::Empty,
            }
        }
        Ext::UnitsPercent
        | Ext::LaborUnitsPercent
        | Ext::NonlaborUnitsPercent
        | Ext::CostPercent => {
            let (actual, total) = progress(p, ix, a.task_id, measures(e));
            Value::Percent(percent(actual, total))
        }
        Ext::SchedulePercent => ev::schedule_percent(p, ix, i)
            .map_or(Value::Empty, |v| Value::Percent(v.clamp(0., 100.))),
        Ext::PerformancePercent => Value::Percent(ev::performance_percent(p, ix, i)),
        Ext::ActivityCount => count(true),
        Ext::CompletedCount => count(a.status == StatusType::Completed),
        Ext::InProgressCount => count(a.status == StatusType::InProgress),
        Ext::NotStartedCount => count(a.status == StatusType::NotStarted),
        Ext::StepCount => Value::Number(c.steps(p, a.task_id).0 as f64),
        Ext::CompletedSteps => Value::Number(c.steps(p, a.task_id).1 as f64),
        Ext::StepsPercent => c.steps(p, a.task_id).2.map_or(Value::Empty, Value::Percent),
        Ext::Feedback => text(c.feedback(p, a.task_id)),
        Ext::NewFeedback => Value::Flag(task("rev_fdbk_flag") == "Y"),
        Ext::Ev(m, basis) => ev::value(p, ix, i, m, basis),
    }
}

/// A band row's total for column `e`, when it has one: sums of counts and
/// earned value, percents from the summed parts, the smallest remaining
/// float and the outer external dates.
pub fn band_total(p: &ProjectData, ix: &Index, e: Ext, members: &[usize]) -> Option<Value> {
    let values = || {
        members.iter().filter_map(|&i| {
            let a = p.model.activities.get(i)?;
            Some(value(p, ix, a, p.details.get(&a.task_id), i, e))
        })
    };
    let minutes = || {
        values().filter_map(|v| match v {
            Value::Date { minute, .. } | Value::Duration(minute) => Some(minute),
            _ => None,
        })
    };
    let date = |m: Option<i64>| {
        m.map(|minute| Value::Date {
            minute,
            mark: crate::format::DateMark::Plain,
        })
    };
    match e {
        Ext::Ev(m, basis) => Some(ev::band_total(p, ix, members, m, basis)),
        Ext::UnitsPercent
        | Ext::LaborUnitsPercent
        | Ext::NonlaborUnitsPercent
        | Ext::CostPercent => {
            let (actual, total) = members
                .iter()
                .filter_map(|&i| p.model.activities.get(i))
                .map(|a| progress(p, ix, a.task_id, measures(e)))
                .fold((0., 0.), |s, x| (s.0 + x.0, s.1 + x.1));
            Some(Value::Percent(percent(actual, total)))
        }
        Ext::SchedulePercent | Ext::PerformancePercent => {
            // Weighted by the budget at completion, in cost when the band
            // has a cost budget and in labor units otherwise.
            let sum = |basis| {
                members.iter().fold(ev::Parts::default(), |mut s, &i| {
                    let x = ev::parts(p, ix, i, basis);
                    s.bac += x.bac;
                    s.pv += x.pv;
                    s.ev += x.ev;
                    s
                })
            };
            let mut parts = sum(Basis::Cost);
            if parts.bac <= 1e-9 {
                parts = sum(Basis::LaborUnits);
            }
            let earned = if e == Ext::SchedulePercent {
                parts.pv
            } else {
                parts.ev
            };
            (parts.bac > 1e-9).then(|| Value::Percent(percent(earned, parts.bac)))
        }
        Ext::ActivityCount
        | Ext::CompletedCount
        | Ext::InProgressCount
        | Ext::NotStartedCount
        | Ext::StepCount
        | Ext::CompletedSteps => Some(Value::Number(
            values().filter_map(|v| v.number()).sum::<f64>(),
        )),
        Ext::RemainingFloat => minutes().min().map(Value::Duration),
        Ext::ExternalEarlyStart => date(minutes().min()),
        Ext::ExternalLateFinish => date(minutes().max()),
        _ => None,
    }
}
