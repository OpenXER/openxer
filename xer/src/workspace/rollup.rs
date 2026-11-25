//! Project and EPS folder rollups for the Projects view and
//! `openxer-cli eps list`. A project's rollup is read from its stored XER
//! text (or, for an open project, from its live activities), so a closed
//! project is always current: there is no separate summarize step. A
//! folder's rollup combines the rollups of every project under it.
//!
//! The rules:
//! - Start: the earliest start (actual, else early) of any activity; Finish:
//!   the latest finish (actual, else early); Late Start the earliest late
//!   start, Late Finish the latest late finish.
//! - Data Date: the project's own; a folder shows the latest of its projects.
//!   Must Finish By: a folder shows the latest.
//! - Total Float: the lowest total float of any activity not yet completed;
//!   a folder shows the lowest of its projects (the most critical).
//! - Units and costs add up. Labor units count assignments of labor
//!   resources only; costs count every assignment plus the project expenses.
//! - Counts add up.
//! - Schedule % Complete is duration based: (planned span - remaining span)
//!   / planned span x 100, in working time. Planned span: from the earliest
//!   start (the earliest actual start once work has started) to the latest
//!   finish. Remaining span: from the earliest remaining start of an
//!   unfinished activity (the data date when none is stored) to the latest
//!   remaining finish. All activities completed is 100. The working time is
//!   the project's default calendar; a folder uses its projects' default
//!   calendar when they all share one, else the standard calendar new
//!   projects start with.
//! - Finish Variance: the baseline's finish minus the current finish in
//!   working time (negative is later than the baseline). A folder compares
//!   the latest baseline finish with the latest finish of the projects that
//!   have a baseline.
use crate::{parse_absolute, Document, Table};
use openxer_engine::calendar::CalendarAdapter;

/// One activity, in absolute minutes from 1899-12-30.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ActivityFacts {
    pub completed: bool,
    pub started: bool,
    /// Actual start, else early start.
    pub start: Option<i64>,
    /// Actual finish, else early finish.
    pub finish: Option<i64>,
    pub late_start: Option<i64>,
    pub late_finish: Option<i64>,
    pub actual_start: Option<i64>,
    pub actual_finish: Option<i64>,
    /// Remaining early start and finish (unfinished activities).
    pub remaining_start: Option<i64>,
    pub remaining_finish: Option<i64>,
    /// Working minutes; `None` when not stored.
    pub total_float: Option<i64>,
}

/// Budgeted, actual and remaining amounts.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Amounts {
    pub budgeted: f64,
    pub actual: f64,
    pub remaining: f64,
}

impl Amounts {
    fn add(&mut self, o: Amounts) {
        self.budgeted += o.budgeted;
        self.actual += o.actual;
        self.remaining += o.remaining;
    }
}

/// A calendar a rollup measures working time in.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CalendarRef {
    pub name: String,
    /// The `clndr_data` text.
    pub data: String,
    /// Hours a day, for showing durations in days.
    pub day_hours_x100: i64,
}

impl CalendarRef {
    /// The calendar new projects start with (`openxer_xer::starter`).
    pub fn standard() -> Self {
        Self {
            name: crate::STANDARD_CALENDAR_NAME.into(),
            data: crate::standard_calendar_data(),
            day_hours_x100: 800,
        }
    }
    pub fn day_hours(&self) -> f64 {
        self.day_hours_x100 as f64 / 100.
    }
    /// The calendar as the engine's adapter; absolute minutes in and out.
    pub fn adapter(&self) -> Option<CalendarAdapter> {
        crate::calendar_from_data(&self.name, &self.data, 0).ok()
    }
    /// Working minutes from `a` to `b` (negative when `b` is before `a`).
    pub fn between(&self, a: i64, b: i64) -> i64 {
        match self.adapter() {
            Some(c) if b >= a => c.between(a, b),
            Some(c) => -c.between(b, a),
            None => b - a,
        }
    }
}

/// Two default calendars are the same when their work times are: a name a
/// workspace migration made unique does not make them differ.
fn same_calendar(a: &Option<CalendarRef>, b: &Option<CalendarRef>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a.data == b.data && a.day_hours_x100 == b.day_hours_x100,
        (None, None) => true,
        _ => false,
    }
}

/// A project's or folder's rollup. Dates are absolute minutes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rollup {
    pub projects: usize,
    pub start: Option<i64>,
    pub finish: Option<i64>,
    pub late_start: Option<i64>,
    pub late_finish: Option<i64>,
    pub actual_start: Option<i64>,
    /// Set only when every activity is completed.
    pub actual_finish: Option<i64>,
    pub data_date: Option<i64>,
    pub must_finish: Option<i64>,
    pub remaining_start: Option<i64>,
    pub remaining_finish: Option<i64>,
    /// Working minutes.
    pub total_float: Option<i64>,
    /// Hours a day of the calendar of the project holding the lowest float.
    pub float_day_hours_x100: i64,
    pub baseline_start: Option<i64>,
    pub baseline_finish: Option<i64>,
    /// Latest finish of the projects that have a baseline.
    pub finish_with_baseline: Option<i64>,
    pub activities: usize,
    pub not_started: usize,
    pub in_progress: usize,
    pub completed: usize,
    /// Labor units in hours.
    pub labor_units: Amounts,
    pub costs: Amounts,
    /// The default calendar shared by every project; `None` when they differ.
    pub calendar: Option<CalendarRef>,
    /// The projects use different default calendars.
    pub mixed_calendars: bool,
}

fn min_opt(a: Option<i64>, b: Option<i64>) -> Option<i64> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, None) => a,
        (None, b) => b,
    }
}

fn max_opt(a: Option<i64>, b: Option<i64>) -> Option<i64> {
    a.max(b)
}

fn num(s: &str) -> i64 {
    s.trim().parse().unwrap_or(0)
}

fn float(s: &str) -> f64 {
    s.trim().parse().unwrap_or(0.)
}

fn hours(s: &str) -> Option<i64> {
    let t = s.trim();
    (!t.is_empty())
        .then(|| t.parse::<f64>().ok())
        .flatten()
        .map(|h| (h * 60.).round() as i64)
}

impl Rollup {
    /// A project's rollup from its activities. `start` and `finish` fall
    /// back to the project's planned start and scheduled finish.
    #[allow(clippy::too_many_arguments)]
    pub fn of_project(
        facts: &[ActivityFacts],
        data_date: Option<i64>,
        must_finish: Option<i64>,
        planned_start: Option<i64>,
        scheduled_finish: Option<i64>,
        calendar: CalendarRef,
        labor_units: Amounts,
        costs: Amounts,
    ) -> Self {
        let mut r = Rollup {
            projects: 1,
            data_date,
            must_finish,
            labor_units,
            costs,
            float_day_hours_x100: calendar.day_hours_x100,
            ..Default::default()
        };
        let mut all_complete = true;
        for a in facts {
            r.activities += 1;
            if a.completed {
                r.completed += 1;
            } else if a.started || a.actual_start.is_some() {
                r.in_progress += 1;
            } else {
                r.not_started += 1;
            }
            all_complete &= a.completed;
            r.start = min_opt(r.start, a.start);
            r.finish = max_opt(r.finish, a.finish);
            r.late_start = min_opt(r.late_start, a.late_start);
            r.late_finish = max_opt(r.late_finish, a.late_finish);
            r.actual_start = min_opt(r.actual_start, a.actual_start);
            r.actual_finish = max_opt(r.actual_finish, a.actual_finish);
            if !a.completed {
                r.remaining_start = min_opt(r.remaining_start, a.remaining_start.or(a.start));
                r.remaining_finish = max_opt(r.remaining_finish, a.remaining_finish.or(a.finish));
                r.total_float = min_opt(r.total_float, a.total_float);
            }
        }
        if !all_complete || r.activities == 0 {
            r.actual_finish = None;
        }
        r.start = r.start.or(planned_start);
        r.finish = r.finish.or(scheduled_finish);
        r.calendar = Some(calendar);
        r
    }

    /// The rollup of a folder: `self` plus project or folder `o`.
    pub fn add(&mut self, o: &Rollup) {
        let first = self.projects == 0;
        self.projects += o.projects;
        self.start = min_opt(self.start, o.start);
        self.finish = max_opt(self.finish, o.finish);
        self.late_start = min_opt(self.late_start, o.late_start);
        self.late_finish = max_opt(self.late_finish, o.late_finish);
        self.actual_start = min_opt(self.actual_start, o.actual_start);
        let done = |r: &Rollup| r.activities > 0 && r.completed == r.activities;
        self.actual_finish = if (first || done(self)) && done(o) {
            max_opt(self.actual_finish, o.actual_finish)
        } else {
            None
        };
        self.data_date = max_opt(self.data_date, o.data_date);
        self.must_finish = max_opt(self.must_finish, o.must_finish);
        self.remaining_start = min_opt(self.remaining_start, o.remaining_start);
        self.remaining_finish = max_opt(self.remaining_finish, o.remaining_finish);
        if o.total_float.is_some() && self.total_float.is_none_or(|f| o.total_float.unwrap() < f) {
            self.total_float = o.total_float;
            self.float_day_hours_x100 = o.float_day_hours_x100;
        }
        self.baseline_start = min_opt(self.baseline_start, o.baseline_start);
        self.baseline_finish = max_opt(self.baseline_finish, o.baseline_finish);
        self.finish_with_baseline = max_opt(self.finish_with_baseline, o.finish_with_baseline);
        self.activities += o.activities;
        self.not_started += o.not_started;
        self.in_progress += o.in_progress;
        self.completed += o.completed;
        self.labor_units.add(o.labor_units);
        self.costs.add(o.costs);
        if first {
            self.calendar = o.calendar.clone();
            self.mixed_calendars = o.mixed_calendars;
        } else if o.projects > 0
            && (self.mixed_calendars
                || o.mixed_calendars
                || !same_calendar(&self.calendar, &o.calendar))
        {
            self.mixed_calendars = true;
            self.calendar = None;
        }
    }

    /// Record the baseline's start and finish.
    pub fn set_baseline(&mut self, start: Option<i64>, finish: Option<i64>) {
        self.baseline_start = start;
        self.baseline_finish = finish;
        self.finish_with_baseline = finish.and(self.finish);
    }

    /// The calendar working time is measured in: the shared default
    /// calendar, else the standard calendar.
    pub fn measure(&self) -> CalendarRef {
        self.calendar.clone().unwrap_or_else(CalendarRef::standard)
    }

    /// Schedule % Complete (duration based), 0 to 100; `None` without
    /// activities or dates.
    pub fn schedule_percent(&self) -> Option<f64> {
        if self.activities == 0 {
            return None;
        }
        if self.completed == self.activities {
            return Some(100.);
        }
        let cal = self.measure();
        let start = self.actual_start.or(self.start)?;
        let finish = self.finish?;
        let planned = cal.between(start, finish);
        if planned <= 0 {
            return Some(0.);
        }
        let rem_start = self.remaining_start.or(self.data_date)?;
        let rem_finish = self.remaining_finish.unwrap_or(finish);
        let remaining = cal.between(rem_start, rem_finish).max(0);
        Some(((planned - remaining) as f64 / planned as f64 * 100.).clamp(0., 100.))
    }

    /// Finish Variance in working minutes (baseline finish minus finish).
    pub fn finish_variance(&self) -> Option<i64> {
        let (bl, finish) = (self.baseline_finish?, self.finish_with_baseline?);
        Some(self.measure().between(finish, bl))
    }
}

fn find_row<'a>(t: &'a Table, key: &str, id: i64) -> Option<&'a Vec<String>> {
    t.rows.iter().find(|r| num(t.value(r, key)) == id)
}

/// The project's default calendar as stored.
pub fn project_calendar(doc: &Document, pid: i64) -> CalendarRef {
    let clndr = doc
        .table("PROJECT")
        .and_then(|t| find_row(t, "proj_id", pid).map(|r| num(t.value(r, "clndr_id"))));
    let row = doc.table("CALENDAR").and_then(|t| {
        let r = clndr
            .and_then(|c| find_row(t, "clndr_id", c))
            .or_else(|| t.rows.iter().find(|r| t.value(r, "default_flag") == "Y"))
            .or_else(|| t.rows.first())?;
        Some((t, r))
    });
    match row {
        Some((t, r)) => {
            let day = float(t.value(r, "day_hr_cnt"));
            CalendarRef {
                name: t.value(r, "clndr_name").to_string(),
                data: t.value(r, "clndr_data").to_string(),
                day_hours_x100: if day > 0. && day <= 24. {
                    (day * 100.).round() as i64
                } else {
                    800
                },
            }
        }
        None => CalendarRef::standard(),
    }
}

/// Labor units (hours) and costs of project `pid`: its resource assignments
/// (labor resources only for units) and its expenses.
pub fn amounts(doc: &Document, pid: i64) -> (Amounts, Amounts) {
    let mut units = Amounts::default();
    let mut costs = Amounts::default();
    let labor: std::collections::BTreeSet<i64> =
        doc.table("RSRC").map_or_else(Default::default, |t| {
            t.rows
                .iter()
                .filter(|r| {
                    let k = t.value(r, "rsrc_type");
                    k == "RT_Labor" || k.is_empty()
                })
                .map(|r| num(t.value(r, "rsrc_id")))
                .collect()
        });
    if let Some(t) = doc.table("TASKRSRC") {
        for r in t.rows.iter().filter(|r| num(t.value(r, "proj_id")) == pid) {
            let v = |k: &str| float(t.value(r, k));
            if labor.contains(&num(t.value(r, "rsrc_id"))) {
                units.add(Amounts {
                    budgeted: v("target_qty"),
                    actual: v("act_reg_qty") + v("act_ot_qty"),
                    remaining: v("remain_qty"),
                });
            }
            costs.add(Amounts {
                budgeted: v("target_cost"),
                actual: v("act_reg_cost") + v("act_ot_cost"),
                remaining: v("remain_cost"),
            });
        }
    }
    if let Some(t) = doc.table("PROJCOST") {
        for r in t.rows.iter().filter(|r| num(t.value(r, "proj_id")) == pid) {
            let v = |k: &str| float(t.value(r, k));
            costs.add(Amounts {
                budgeted: v("target_cost"),
                actual: v("act_cost"),
                remaining: v("remain_cost"),
            });
        }
    }
    (units, costs)
}

/// The stored activities of project `pid`.
pub fn activity_facts(doc: &Document, pid: i64) -> Vec<ActivityFacts> {
    let Some(t) = doc.table("TASK") else {
        return vec![];
    };
    t.rows
        .iter()
        .filter(|r| num(t.value(r, "proj_id")) == pid)
        .map(|r| {
            let d = |k: &str| parse_absolute(t.value(r, k));
            let status = t.value(r, "status_code");
            let actual_start = d("act_start_date");
            let actual_finish = d("act_end_date");
            let early_start = d("early_start_date").or(d("target_start_date"));
            let early_finish = d("early_end_date").or(d("target_end_date"));
            ActivityFacts {
                completed: status == "TK_Complete",
                started: status == "TK_Active",
                start: actual_start.or(early_start),
                finish: actual_finish.or(early_finish),
                late_start: d("late_start_date"),
                late_finish: d("late_end_date"),
                actual_start,
                actual_finish,
                remaining_start: d("restart_date").or(early_start),
                remaining_finish: d("reend_date").or(early_finish),
                total_float: hours(t.value(r, "total_float_hr_cnt")),
            }
        })
        .collect()
}

/// The rollup of project `pid` from its stored XER text alone.
pub fn of_document(doc: &Document, pid: i64) -> Option<Rollup> {
    let project = doc.table("PROJECT")?;
    let row = find_row(project, "proj_id", pid)?;
    let d = |k: &str| parse_absolute(project.value(row, k));
    let (units, costs) = amounts(doc, pid);
    Some(Rollup::of_project(
        &activity_facts(doc, pid),
        d("last_recalc_date").or(d("plan_start_date")),
        d("plan_end_date"),
        d("plan_start_date"),
        d("scd_end_date"),
        project_calendar(doc, pid),
        units,
        costs,
    ))
}

/// The project baseline a stored project names (PROJECT.sum_base_proj_id).
pub fn assigned_baseline(doc: &Document, pid: i64) -> Option<i64> {
    doc.table("PROJECT")
        .and_then(|t| find_row(t, "proj_id", pid).map(|r| num(t.value(r, "sum_base_proj_id"))))
        .filter(|k| *k != 0)
}

/// The baseline a project's variance compares with: its project baseline
/// (`assigned`, a baseline project id) when that names one of `stored`
/// (row id, baseline project id), else the Primary user baseline `primary`
/// when it is one of them.
pub fn pick_baseline(
    assigned: Option<i64>,
    stored: &[(i64, i64)],
    primary: Option<i64>,
) -> Option<i64> {
    assigned
        .and_then(|k| stored.iter().find(|(_, key)| *key == k).map(|(id, _)| *id))
        .or_else(|| primary.filter(|p| stored.iter().any(|(id, _)| id == p)))
}
