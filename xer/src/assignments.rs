//! Resource assignments (TASKRSRC rows).
//!
//! The retained document's TASKRSRC rows are the only copy of an assignment.
//! The scheduling model (`Activity.assignments`), the resource-driven duration
//! flags and the leveling inputs are rebuilt from those rows by the same code
//! the importer uses, so an edit made here schedules exactly as a reimport of
//! the exported file would.
//!
//! This module holds:
//! - the duration type rules (which of units, units/time and duration is
//!   recalculated when another one is edited), as pure functions;
//! - assignment edits (add, remove, change units, rates and durations);
//! - the dates and remaining units written back after scheduling.
//!
//! Units are stored per hour in XER: `remain_qty` hours of work at
//! `remain_qty_per_hr` units per hour last `remain_qty / remain_qty_per_hr`
//! working hours. The file has no assignment duration column.
use super::*;

mod progress;
pub use progress::{reset_this_period, ResourceSlot, UnitsPhase};

/// An activity's duration type (`TASK.duration_type`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DurationType {
    /// `DT_FixedDrtn`.
    FixedDurationAndUnitsPerTime,
    /// `DT_FixedDUR2`, the default for new activities. Units never rewrite
    /// the planned duration under this type; a started activity's remaining
    /// duration follows its driving units at each schedule (probe 1301b).
    FixedDurationAndUnits,
    /// `DT_FixedRate`.
    FixedUnitsPerTime,
    /// `DT_FixedQty`.
    FixedUnits,
}
impl DurationType {
    pub const ALL: [Self; 4] = [
        Self::FixedDurationAndUnitsPerTime,
        Self::FixedDurationAndUnits,
        Self::FixedUnitsPerTime,
        Self::FixedUnits,
    ];
    /// Unknown or blank codes read as `DT_FixedDUR2`, matching how scheduling
    /// treats them (units do not drive the duration).
    pub fn from_code(code: &str) -> Self {
        match code {
            "DT_FixedDrtn" => Self::FixedDurationAndUnitsPerTime,
            "DT_FixedRate" => Self::FixedUnitsPerTime,
            "DT_FixedQty" => Self::FixedUnits,
            _ => Self::FixedDurationAndUnits,
        }
    }
    pub fn code(self) -> &'static str {
        match self {
            Self::FixedDurationAndUnitsPerTime => "DT_FixedDrtn",
            Self::FixedDurationAndUnits => "DT_FixedDUR2",
            Self::FixedUnitsPerTime => "DT_FixedRate",
            Self::FixedUnits => "DT_FixedQty",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::FixedDurationAndUnitsPerTime => "Fixed Duration and Units/Time",
            Self::FixedDurationAndUnits => "Fixed Duration and Units",
            Self::FixedUnitsPerTime => "Fixed Units/Time",
            Self::FixedUnits => "Fixed Units",
        }
    }
    /// The value that is recalculated when `changed` is edited. The type's
    /// fixed values are held; when the edited value is itself a fixed one,
    /// the duration types that fix the duration recalculate units (or units/time
    /// when the units are fixed too) and the others recalculate the duration.
    pub fn recalculates(self, changed: Changed) -> Changed {
        use Changed::*;
        use DurationType::*;
        match (self, changed) {
            (FixedDurationAndUnitsPerTime | FixedUnitsPerTime, Duration) => Units,
            (FixedDurationAndUnits | FixedUnits, Duration) => UnitsPerTime,
            (FixedDurationAndUnitsPerTime | FixedDurationAndUnits, Units) => UnitsPerTime,
            (FixedUnitsPerTime | FixedUnits, Units) => Duration,
            (FixedDurationAndUnitsPerTime | FixedDurationAndUnits, UnitsPerTime) => Units,
            (FixedUnitsPerTime | FixedUnits, UnitsPerTime) => Duration,
        }
    }
}

/// One of the three related assignment values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Changed {
    Units,
    UnitsPerTime,
    Duration,
}

/// Units = units per hour x working hours.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Work {
    pub units: f64,
    pub per_hour: f64,
    pub hours: f64,
}

/// Recalculate `work` after `changed` was set to its new value, following the
/// duration type. A duration cannot be derived from zero units/time, so in
/// that case the units/time is derived instead; a zero duration keeps the
/// units/time.
pub fn recalculate(kind: DurationType, changed: Changed, work: Work) -> Work {
    let mut w = work;
    // Units/time cannot come from a zero duration; the reference keeps it (an
    // in-progress activity whose remaining duration and units both go to
    // zero keeps its remaining units/time).
    let per_hour = |w: &Work| {
        if w.hours > 0. {
            w.units / w.hours
        } else {
            w.per_hour
        }
    };
    match kind.recalculates(changed) {
        Changed::Units => w.units = w.per_hour * w.hours,
        Changed::UnitsPerTime => w.per_hour = per_hour(&w),
        Changed::Duration if w.per_hour > 0. => w.hours = w.units / w.per_hour,
        Changed::Duration => w.per_hour = per_hour(&w),
    }
    w
}

/// Rate types and their RSRCRATE/ROLERATE price columns.
pub const RATE_TYPES: [(&str, &str, &str); 5] = [
    ("COST_PER_QTY", "Price / Unit", "cost_per_qty"),
    ("COST_PER_QTY2", "Price / Unit2", "cost_per_qty2"),
    ("COST_PER_QTY3", "Price / Unit3", "cost_per_qty3"),
    ("COST_PER_QTY4", "Price / Unit4", "cost_per_qty4"),
    ("COST_PER_QTY5", "Price / Unit5", "cost_per_qty5"),
];
/// Where an assignment's price comes from (`cost_per_qty_source_type`).
pub const RATE_SOURCES: [(&str, &str); 3] = [
    ("ST_Rsrc", "Resource"),
    ("ST_Role", "Role"),
    ("ST_Custom", "Override"),
];

/// An edit of one assignment row.
#[derive(Clone, Debug, PartialEq)]
pub enum AssignmentEdit {
    BudgetedUnits(f64),
    RemainingUnits(f64),
    BudgetedUnitsPerTime(f64),
    RemainingUnitsPerTime(f64),
    /// Minutes.
    OriginalDuration(i64),
    /// Minutes.
    RemainingDuration(i64),
    DriveActivityDates(bool),
    RateType(String),
    PricePerUnit(f64),
    CostAccount(Option<i64>),
    Resource(i64),
    Role(Option<i64>),
    /// Remaining lag in minutes: the assignment starts this much work time
    /// after the activity (not started activities only). With budget and
    /// remaining linked the planned lag follows.
    Lag(i64),
    /// Planned (budgeted) lag in minutes.
    PlannedLag(i64),
    /// Overtime factor (`ot_factor`); actual overtime cost follows.
    OvertimeFactor(f64),
    /// Actual regular units; the activity must have started. Costs follow
    /// units when linked, and with "Subtract Actual from At Completion" the
    /// remaining units drop by the change.
    ActualRegularUnits(f64),
    /// Actual overtime units, as actual regular units.
    ActualOvertimeUnits(f64),
    /// Actual regular cost; with "Update units when costs change" the actual
    /// units follow at the assignment's price.
    ActualRegularCost(f64),
    /// Actual overtime cost, as actual regular cost (price x overtime factor).
    ActualOvertimeCost(f64),
    /// Budgeted cost: the units follow at the assignment's price (an
    /// assignment without a price takes units instead).
    BudgetedCost(f64),
    /// Remaining cost: with cost and units linked the remaining units follow
    /// at the price, otherwise the cost is kept on its own.
    RemainingCost(f64),
    /// Actual units this period. With the project's "link actual to date and
    /// actual this period" option the actual units move by the change; without
    /// it only the period figure is set. The activity must have started.
    ThisPeriodUnits(f64),
    /// Actual cost this period, as the units.
    ThisPeriodCost(f64),
    /// At completion units (actual plus remaining): the remaining units
    /// become the value less the actual units, so the budget follows only
    /// while budget and remaining are linked (a not started activity). Not
    /// below the actual units, and not on a completed activity.
    AtCompletionUnits(f64),
    /// At completion cost (actual plus remaining): the remaining cost
    /// becomes the value less the actual cost, by the same rule as
    /// [`AssignmentEdit::RemainingCost`] (while cost and units are linked
    /// the remaining units follow at the price, so the budget follows only
    /// before the start). Not below the actual cost, and not on a completed
    /// activity.
    AtCompletionCost(f64),
    /// Where the price comes from (`cost_per_qty_source_type`): `ST_Rsrc`
    /// and `ST_Role` take the resource's or role's rate for the rate type
    /// at the data date; `ST_Custom` keeps the price as it is, to be typed.
    RateSource(String),
    /// Actual cost, regular and overtime together: the regular cost takes
    /// the value less the overtime cost.
    ActualCost(f64),
    /// Whether the assignment's costs follow its units at its price
    /// (`cost_qty_link_flag`). While on, the remaining cost is units x price.
    CostUnitsLinked(bool),
    /// Actual start (minutes relative to the model origin) of an assignment of
    /// a started activity: not after the data date, the assignment's actual
    /// finish or the activity's actual finish, and not before the activity's
    /// actual start. The schedule keeps it.
    ActualStart(i64),
    /// Actual finish of an assignment of a started activity: not before the
    /// assignment's actual start, not after the data date or the activity's
    /// actual finish. The schedule keeps it.
    ActualFinish(i64),
    /// Remaining start of an assignment of a not started activity, not before
    /// the data date or the activity's remaining start. It is stored as the
    /// remaining lag (work time after the activity's remaining start), so the
    /// schedule reproduces it. A started activity's assignments follow the
    /// activity's remaining dates and refuse it.
    RemainingStart(i64),
    /// Remaining finish of an assignment of an activity that has not
    /// finished, not before the data date or the assignment's remaining
    /// start. It is stored as the remaining duration (work time from the
    /// remaining start), recalculated with the duration type.
    RemainingFinish(i64),
}

/// The dates of one assignment, minutes relative to the model origin. The
/// schedule writes them into the row; actual dates entered on the assignment
/// are kept (see `ProjectData::write_assignment_schedule`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AssignmentDates {
    pub act_start: Option<i64>,
    pub act_finish: Option<i64>,
    pub rem_start: Option<i64>,
    pub rem_finish: Option<i64>,
    pub rem_late_start: Option<i64>,
    pub rem_late_finish: Option<i64>,
    pub target_start: Option<i64>,
    pub target_finish: Option<i64>,
}

/// The values the Resources tab shows for one assignment row.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AssignmentView {
    pub row: usize,
    pub rsrc_id: Option<i64>,
    pub role_id: Option<i64>,
    pub acct_id: Option<i64>,
    pub budgeted: Work,
    pub remaining: Work,
    pub actual_units: f64,
    /// Minutes.
    pub original_duration: i64,
    /// Minutes.
    pub remaining_duration: i64,
    pub drives_dates: bool,
    pub rate_type: String,
    pub rate_source: String,
    pub price: f64,
    pub budgeted_cost: f64,
    pub actual_cost: f64,
    pub remaining_cost: f64,
    pub primary: bool,
    /// Remaining and planned lag, minutes.
    pub lag: i64,
    pub planned_lag: i64,
    /// Overtime factor; blank in the file reads as 0.
    pub ot_factor: f64,
    pub actual_regular_units: f64,
    pub actual_overtime_units: f64,
    pub actual_regular_cost: f64,
    pub actual_overtime_cost: f64,
    /// Actual units and cost this period (regular and overtime together).
    pub this_period_units: f64,
    pub this_period_cost: f64,
    /// Costs follow units at the price (`cost_qty_link_flag`, on unless N).
    pub cost_linked: bool,
    pub dates: AssignmentDates,
}

/// Units and prices as the reference writes them: 15 significant digits, no trailing zeros.
pub fn quantity_text(v: f64) -> String {
    if !v.is_finite() || v == 0. {
        return "0".into();
    }
    let digits = (14 - v.abs().log10().floor() as i32).clamp(0, 15) as usize;
    let text = format!("{v:.digits$}");
    let text = if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.')
    } else {
        &text
    };
    if text == "-0" {
        "0".into()
    } else {
        text.into()
    }
}
/// Costs as the reference writes them: four decimals.
pub fn cost_text(v: f64) -> String {
    format!("{:.4}", if v == 0. { 0. } else { v })
}

/// A cost with `places` decimals: the value times 10^places rounded half
/// away from zero, as the reference rounds (`934.555` gives `934.56` because the
/// product is exactly 93455.5; `2361.345` gives `2361.34`).
pub fn cost_in(places: usize, v: f64) -> String {
    if places >= 4 {
        return cost_text(v);
    }
    let scale = 10_f64.powi(places as i32);
    let x = (v * scale).round() / scale;
    format!("{:.*}", places, if x == 0. { 0. } else { x })
}

/// The decimals a file stores costs with. Older exports round every cost to
/// the cent (two decimals on every cost cell); a file with no costs, or any
/// cost with other decimals, keeps four.
pub fn cost_places(doc: &Document) -> usize {
    let Some(t) = doc.table("TASKRSRC") else {
        return 4;
    };
    let columns: Vec<usize> = ["target_cost", "act_reg_cost", "act_ot_cost", "remain_cost"]
        .iter()
        .filter_map(|c| t.field(c))
        .collect();
    let mut seen = false;
    for r in &t.rows {
        for &c in &columns {
            let Some(v) = r.get(c).filter(|v| !v.is_empty()) else {
                continue;
            };
            match v.split_once('.') {
                Some((_, d)) if d.len() == 2 => seen = true,
                _ => return 4,
            }
        }
    }
    if seen {
        2
    } else {
        4
    }
}
pub(crate) fn float(s: &str) -> f64 {
    s.trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .unwrap_or(0.)
}
fn optional_id(s: &str) -> Option<i64> {
    s.trim().parse::<i64>().ok().filter(|v| *v != 0)
}

/// Rebuild every activity's model assignments and resource-driven flag from
/// the TASKRSRC rows of `doc`, in file order (shared with the importer).
pub(crate) fn apply_rows(
    doc: &Document,
    pid: i64,
    activities: &mut [Activity],
    calendars: &[CalendarAdapter],
    details: &mut BTreeMap<i64, ActivityDetails>,
) {
    let index: HashMap<i64, usize> = activities
        .iter()
        .enumerate()
        .map(|(i, a)| (a.task_id, i))
        .collect();
    // A Resource Dependent activity works on its resources' calendars, so
    // its assignments with units carry them (saved reference probes 814, 816).
    let cal_pos: HashMap<&str, usize> = calendars
        .iter()
        .enumerate()
        .filter(|(_, c)| has_recurring_work(&c.cal))
        .map(|(i, c)| (c.id.as_str(), i))
        .collect();
    let resource_calendar: HashMap<i64, usize> = doc.table("RSRC").map_or_else(HashMap::new, |t| {
        let cols = t.field_index();
        let c_clndr = cols.col("clndr_id");
        let c_rsrc = cols.col("rsrc_id");
        t.rows
            .iter()
            .filter_map(|r| {
                let id = FieldIndex::get(r, c_clndr);
                cal_pos
                    .get(id)
                    .map(|&c| (num(FieldIndex::get(r, c_rsrc)), c))
            })
            .collect()
    });
    for a in activities.iter_mut() {
        a.assignments.clear();
    }
    let mut driving: HashMap<i64, bool> = HashMap::new();
    if let Some(t) = doc.table("TASKRSRC") {
        let cols = t.field_index();
        let c_proj = cols.col("proj_id");
        let c_task = cols.col("task_id");
        let c_flag = cols.col("rollup_dates_flag");
        let c_rsrc = cols.col("rsrc_id");
        let c_rate = cols.col("remain_qty_per_hr");
        let c_qty = cols.col("remain_qty");
        let c_lag = cols.col("relag_drtn_hr_cnt");
        for r in &t.rows {
            if num(FieldIndex::get(r, c_proj)) != pid {
                continue;
            }
            let task = num(FieldIndex::get(r, c_task));
            let flag = FieldIndex::get(r, c_flag);
            if flag == "Y" {
                driving.insert(task, true);
            }
            if let Some(&i) = index.get(&task) {
                let rem = activities[i].rem;
                let rate_s = FieldIndex::get(r, c_rate);
                let qty_s = FieldIndex::get(r, c_qty);
                let rate = rate_s.parse::<f64>().unwrap_or(0.0);
                let qty = qty_s.parse::<f64>().unwrap_or(0.0);
                let lag = hours(FieldIndex::get(r, c_lag));
                let mut assignment = Assignment {
                    rsrc_id: num(FieldIndex::get(r, c_rsrc)),
                    rollup: flag == "Y",
                    rollup_present: !flag.is_empty(),
                    rem: if rate != 0.0 {
                        (qty / rate * 60.0).round_ties_even() as i64
                    } else {
                        rem
                    },
                    remain_lag: lag,
                    relag: lag,
                    rem_hours_bits: (rate != 0.0 && qty != 0.0).then(|| (qty / rate).to_bits()),
                    ..Assignment::default()
                };
                if activities[i].task_type == TaskType::Resource && rate > 0.0 && qty > 0.0 {
                    assignment.cal = resource_calendar.get(&assignment.rsrc_id).copied();
                }
                activities[i].assignments.push(assignment);
            }
        }
    }
    let mut kinds: HashMap<i64, &str> = HashMap::new();
    if let Some(t) = doc.table("TASK") {
        let cols = t.field_index();
        let c_proj = cols.col("proj_id");
        let c_task = cols.col("task_id");
        let c_dur = cols.col("duration_type");
        for r in &t.rows {
            if num(FieldIndex::get(r, c_proj)) == pid {
                kinds
                    .entry(num(FieldIndex::get(r, c_task)))
                    .or_insert(FieldIndex::get(r, c_dur));
            }
        }
    }
    for a in activities.iter() {
        if let Some(d) = details.get_mut(&a.task_id) {
            d.resource_driven_duration = matches!(
                kinds.get(&a.task_id).copied().unwrap_or(""),
                "DT_FixedDrtn" | "DT_FixedRate" | "DT_FixedQty"
            ) && driving.contains_key(&a.task_id);
        }
    }
}

fn project_flag(doc: &Document, pid: i64, column: &str) -> String {
    doc.table("PROJECT")
        .and_then(|t| {
            t.rows
                .iter()
                .find(|r| num(t.value(r, "proj_id")) == pid)
                .map(|r| t.value(r, column).to_string())
        })
        .unwrap_or_default()
}
fn lookup<'a>(
    doc: &'a Document,
    table: &str,
    key: &str,
    id: i64,
) -> Option<(&'a Table, &'a [String])> {
    let t = doc.table(table)?;
    t.rows
        .iter()
        .find(|r| num(t.value(r, key)) == id)
        .map(|r| (t, r.as_slice()))
}
pub(crate) fn task_column(doc: &Document, pid: i64, task_id: i64, column: &str) -> String {
    doc.table("TASK")
        .and_then(|t| {
            t.rows
                .iter()
                .find(|r| {
                    num(t.value(r, "task_id")) == task_id && num(t.value(r, "proj_id")) == pid
                })
                .map(|r| t.value(r, column).to_string())
        })
        .unwrap_or_default()
}
pub(crate) fn set_task_column(
    doc: &mut Document,
    pid: i64,
    task_id: i64,
    column: &str,
    value: String,
) {
    let t = doc.table_mut("TASK");
    let row = t
        .rows
        .iter()
        .position(|r| num(t.value(r, "task_id")) == task_id && num(t.value(r, "proj_id")) == pid);
    let row = row.unwrap_or_else(|| {
        t.add(&[
            ("task_id", task_id.to_string()),
            ("proj_id", pid.to_string()),
        ]);
        t.rows.len() - 1
    });
    t.set(row, column, value);
}

/// The price in effect at `at` (absolute minutes) for a resource or role rate
/// table, for the given rate type.
fn rate_price(doc: &Document, table: &str, key: &str, id: i64, rate_type: &str, at: i64) -> f64 {
    let column = RATE_TYPES
        .iter()
        .find(|(code, _, _)| *code == rate_type)
        .map_or("cost_per_qty", |(_, _, c)| *c);
    let Some(t) = doc.table(table) else {
        return 0.;
    };
    let mut best: Option<(i64, f64)> = None;
    let mut first: Option<(i64, f64)> = None;
    for r in t.rows.iter().filter(|r| num(t.value(r, key)) == id) {
        let start = parse_absolute(t.value(r, "start_date")).unwrap_or(i64::MIN);
        let price = float(t.value(r, column));
        if first.is_none_or(|(s, _)| start < s) {
            first = Some((start, price));
        }
        if start <= at && best.is_none_or(|(s, _)| start >= s) {
            best = Some((start, price));
        }
    }
    best.or(first).map_or(0., |(_, p)| p)
}

/// The file has no assignment duration; units / units-per-hour reproduce it
/// only as precisely as the stored units. A duration within 1% (and at least
/// two minutes) of the activity's is taken to be the activity's own.
fn same_length(assignment: i64, activity: i64) -> bool {
    (assignment - activity).abs() <= (activity.abs() / 100).max(2)
}

impl ProjectData {
    /// Rebuild `Activity.assignments`, the resource-driven durations and the
    /// leveling inputs from the current TASKRSRC rows. Call it after any
    /// assignment edit and before scheduling. For rows that came from the file
    /// unchanged it reproduces the import exactly.
    pub fn rebuild_assignments(&mut self) {
        let current = self.updated_document();
        let pid = self.model.project.proj_id;
        apply_rows(
            &current,
            pid,
            &mut self.model.activities,
            &self.model.calendars,
            &mut self.details,
        );
        preprocess_units(&current, &mut self.model, &mut self.details, false);
        self.resources = leveling_inputs(&current, &self.model, &self.wbs);
    }

    /// After a resource dictionary edit (RSRC, RSRCRATE): refresh the
    /// resource names and the leveling inputs (calendars, limits) from the
    /// rows without touching the scheduling model. Scheduling rebuilds the
    /// same inputs again before it runs.
    pub fn refresh_resource_inputs(&mut self) {
        self.resource_names = self.document.table("RSRC").map_or(BTreeMap::new(), |t| {
            t.rows
                .iter()
                .map(|r| (num(t.value(r, "rsrc_id")), t.value(r, "rsrc_name").into()))
                .collect()
        });
        let current = self.updated_document();
        self.resources = leveling_inputs(&current, &self.model, &self.wbs);
    }

    /// TASKRSRC row indices of an activity, in file order.
    pub fn assignment_rows(&self, task_id: i64) -> Vec<usize> {
        let pid = self.model.project.proj_id;
        self.document.table("TASKRSRC").map_or(vec![], |t| {
            t.rows
                .iter()
                .enumerate()
                .filter(|(_, r)| {
                    num(t.value(r, "task_id")) == task_id && num(t.value(r, "proj_id")) == pid
                })
                .map(|(i, _)| i)
                .collect()
        })
    }

    fn activity_index(&self, task_id: i64) -> Result<usize, String> {
        self.model
            .activities
            .iter()
            .position(|a| a.task_id == task_id)
            .ok_or_else(|| format!("Activity {task_id} is not in this project"))
    }
    pub(crate) fn assignment_task(&self, row: usize) -> Result<(usize, i64), String> {
        let t = self
            .document
            .table("TASKRSRC")
            .filter(|t| row < t.rows.len())
            .ok_or("The resource assignment no longer exists")?;
        let r = &t.rows[row];
        if num(t.value(r, "proj_id")) != self.model.project.proj_id {
            return Err("The resource assignment belongs to another project".into());
        }
        let task_id = num(t.value(r, "task_id"));
        Ok((self.activity_index(task_id)?, task_id))
    }

    /// Whether budgeted values follow remaining values for this activity:
    /// a not-started activity in a project that links budget and remaining.
    pub(crate) fn linked(&self, i: usize) -> bool {
        self.model.activities[i].status == StatusType::NotStarted
            && project_flag(
                &self.document,
                self.model.project.proj_id,
                "rem_target_link_flag",
            ) != "N"
    }

    /// The values shown for an assignment row.
    pub fn assignment_view(&self, row: usize) -> Option<AssignmentView> {
        let t = self.document.table("TASKRSRC")?;
        let r = t.rows.get(row)?;
        let v = |k: &str| float(t.value(r, k));
        let pid = self.model.project.proj_id;
        let task_id = num(t.value(r, "task_id"));
        let i = self.activity_index(task_id).ok()?;
        let a = &self.model.activities[i];
        let original = self
            .details
            .get(&task_id)
            .map_or(0, |d| d.original_duration);
        let minutes = |qty: f64, rate: f64, fallback: i64| {
            if rate > 0. {
                units_minutes(qty, rate, 0.).unwrap_or(fallback)
            } else {
                fallback
            }
        };
        let fallback_remaining = if a.is_completed() { 0 } else { a.rem };
        let original_duration = minutes(v("target_qty"), v("target_qty_per_hr"), original);
        let remaining_duration =
            minutes(v("remain_qty"), v("remain_qty_per_hr"), fallback_remaining);
        let rsrc_id = optional_id(t.value(r, "rsrc_id"));
        Some(AssignmentView {
            row,
            rsrc_id,
            role_id: optional_id(t.value(r, "role_id")),
            acct_id: optional_id(t.value(r, "acct_id")),
            budgeted: Work {
                units: v("target_qty"),
                per_hour: v("target_qty_per_hr"),
                hours: original_duration as f64 / 60.,
            },
            remaining: Work {
                units: v("remain_qty"),
                per_hour: v("remain_qty_per_hr"),
                hours: remaining_duration as f64 / 60.,
            },
            actual_units: v("act_reg_qty") + v("act_ot_qty"),
            original_duration,
            remaining_duration,
            drives_dates: t.value(r, "rollup_dates_flag") == "Y",
            rate_type: match t.value(r, "rate_type") {
                "" => "COST_PER_QTY".into(),
                s => s.into(),
            },
            rate_source: match t.value(r, "cost_per_qty_source_type") {
                "" => "ST_Rsrc".into(),
                s => s.into(),
            },
            price: v("cost_per_qty"),
            budgeted_cost: v("target_cost"),
            actual_cost: v("act_reg_cost") + v("act_ot_cost"),
            remaining_cost: v("remain_cost"),
            primary: rsrc_id.is_some()
                && optional_id(&task_column(&self.document, pid, task_id, "rsrc_id")) == rsrc_id,
            lag: hours(t.value(r, "relag_drtn_hr_cnt")),
            planned_lag: hours(t.value(r, "target_lag_drtn_hr_cnt")),
            ot_factor: v("ot_factor"),
            actual_regular_units: v("act_reg_qty"),
            actual_overtime_units: v("act_ot_qty"),
            actual_regular_cost: v("act_reg_cost"),
            actual_overtime_cost: v("act_ot_cost"),
            this_period_units: v("act_this_per_qty"),
            this_period_cost: v("act_this_per_cost"),
            cost_linked: t.value(r, "cost_qty_link_flag") != "N",
            dates: self.assignment_dates_at(row, i),
        })
    }

    /// Assign a resource or a role to an activity. The new assignment lasts the
    /// activity's durations at the resource's default units/time (the role's
    /// when no resource is given) and drives the activity's dates. The first
    /// resource assigned becomes the primary resource. Returns the new row.
    pub fn add_assignment(
        &mut self,
        task_id: i64,
        rsrc_id: Option<i64>,
        role_id: Option<i64>,
    ) -> Result<usize, String> {
        let places = cost_places(&self.document);
        let i = self.activity_index(task_id)?;
        let a = &self.model.activities[i];
        if is_milestone(a.task_type) {
            return Err(milestone_refusal(&a.task_code));
        }
        let pid = self.model.project.proj_id;
        let doc = &self.document;
        let rsrc = match rsrc_id {
            Some(id) => Some(
                lookup(doc, "RSRC", "rsrc_id", id)
                    .ok_or_else(|| format!("Resource {id} is not in this file"))?,
            ),
            None => None,
        };
        let role = match role_id {
            Some(id) => Some(
                lookup(doc, "ROLES", "role_id", id)
                    .ok_or_else(|| format!("Role {id} is not in this file"))?,
            ),
            None => None,
        };
        if rsrc.is_none() && role.is_none() {
            return Err("Choose a resource or a role to assign".into());
        }
        let per_hour = rsrc
            .map(|(t, r)| float(t.value(r, "def_qty_per_hr")))
            .filter(|v| *v > 0.)
            .unwrap_or(1.);
        let original = self
            .details
            .get(&task_id)
            .map_or(0, |d| d.original_duration);
        let remaining = if a.is_completed() { 0 } else { a.rem };
        let budget = per_hour * original as f64 / 60.;
        let left = if self.linked(i) {
            budget
        } else {
            per_hour * remaining as f64 / 60.
        };
        let at = (self.model.origin_serial * DAY) + self.model.project.data_date.unwrap_or(0);
        let (price, source) = match (rsrc_id, role_id) {
            (Some(id), _) => (
                rate_price(doc, "RSRCRATE", "rsrc_id", id, "COST_PER_QTY", at),
                "ST_Rsrc",
            ),
            (None, Some(id)) => (
                rate_price(doc, "ROLERATE", "role_id", id, "COST_PER_QTY", at),
                "ST_Role",
            ),
            (None, None) => (0., "ST_Rsrc"),
        };
        let link = rsrc
            .map(|(t, r)| t.value(r, "def_cost_qty_link_flag").to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "Y".into());
        let kind = rsrc.map_or("RT_Labor".to_string(), |(t, r)| {
            match t.value(r, "rsrc_type") {
                "" => "RT_Labor".into(),
                s => s.into(),
            }
        });
        let account = project_flag(doc, pid, "acct_id");
        let started = (a.status != StatusType::NotStarted).then_some(());
        let dates = [
            ("act_start_date", started.and(a.act_start)),
            ("act_end_date", a.act_finish),
            (
                "restart_date",
                (!a.is_completed()).then_some(a.restart).flatten(),
            ),
            (
                "reend_date",
                (!a.is_completed()).then_some(a.reend).flatten(),
            ),
            ("target_start_date", a.target_start.or(a.early_start)),
            ("target_end_date", a.target_finish.or(a.early_finish)),
            (
                "rem_late_start_date",
                (!a.is_completed()).then_some(a.rem_late_start).flatten(),
            ),
            (
                "rem_late_end_date",
                (!a.is_completed()).then_some(a.rem_late_finish).flatten(),
            ),
        ];
        let origin = self.model.origin_serial;
        let id = crate::edit::records::next_id(&self.document, "TASKRSRC", "taskrsrc_id");
        let guid = new_guid(&format!("taskrsrc/{pid}/{task_id}/{id}"));
        let mut values: Vec<(&str, String)> = vec![
            ("taskrsrc_id", id.to_string()),
            ("task_id", task_id.to_string()),
            ("proj_id", pid.to_string()),
            ("cost_qty_link_flag", link.clone()),
            ("role_id", role_id.map_or(String::new(), |r| r.to_string())),
            ("acct_id", account),
            ("rsrc_id", rsrc_id.map_or(String::new(), |r| r.to_string())),
            ("remain_qty", quantity_text(left)),
            ("target_qty", quantity_text(budget)),
            ("remain_qty_per_hr", quantity_text(per_hour)),
            ("target_lag_drtn_hr_cnt", "0".into()),
            ("target_qty_per_hr", quantity_text(per_hour)),
            ("act_ot_qty", "0".into()),
            ("act_reg_qty", "0".into()),
            ("relag_drtn_hr_cnt", "0".into()),
            ("cost_per_qty", cost_in(places, price)),
            ("target_cost", cost_in(places, budget * price)),
            ("act_reg_cost", cost_in(places, 0.)),
            ("act_ot_cost", cost_in(places, 0.)),
            (
                "remain_cost",
                cost_in(places, if link == "N" { 0. } else { left * price }),
            ),
            ("rollup_dates_flag", "Y".into()),
            ("ts_pend_act_end_flag", "N".into()),
            ("guid", guid),
            ("rate_type", "COST_PER_QTY".into()),
            ("act_this_per_cost", cost_in(places, 0.)),
            ("act_this_per_qty", "0".into()),
            ("rsrc_type", kind),
            ("cost_per_qty_source_type", source.into()),
            ("create_date", now_stamp()),
        ];
        for (k, v) in dates {
            values.push((k, date_string(origin, v)));
        }
        let table = self.document.table_mut("TASKRSRC");
        table.add(&values);
        let row = table.rows.len() - 1;
        if let Some(id) = rsrc_id {
            if optional_id(&task_column(&self.document, pid, task_id, "rsrc_id")).is_none() {
                set_task_column(&mut self.document, pid, task_id, "rsrc_id", id.to_string());
            }
        }
        self.sync_activity_durations(i);
        self.rebuild_assignments();
        Ok(row)
    }

    /// Remove an assignment. Removing the primary resource's assignment clears
    /// the primary resource. The activity keeps its durations.
    pub fn remove_assignment(&mut self, row: usize) -> Result<(), String> {
        let (_, task_id) = self.assignment_task(row)?;
        let pid = self.model.project.proj_id;
        let t = self.document.table_mut("TASKRSRC");
        let rsrc = optional_id(t.value(&t.rows[row], "rsrc_id"));
        let gone = num(t.value(&t.rows[row], "taskrsrc_id").trim());
        t.rows.remove(row);
        // Its codes, period actuals and user field values go with it, so a
        // later assignment that reuses the ID does not inherit them.
        crate::delete::delete_assignment_children(
            &mut self.document,
            &std::collections::HashSet::from([gone]),
        );
        let primary = optional_id(&task_column(&self.document, pid, task_id, "rsrc_id"));
        if rsrc.is_some() && primary == rsrc {
            let still = self.assignment_rows(task_id).into_iter().any(|r| {
                let t = self.document.table("TASKRSRC").expect("rows exist");
                optional_id(t.value(&t.rows[r], "rsrc_id")) == rsrc
            });
            if !still {
                set_task_column(&mut self.document, pid, task_id, "rsrc_id", String::new());
            }
        }
        self.rebuild_assignments();
        Ok(())
    }

    /// Units held by the activity's driving assignments with units/time:
    /// (budgeted, remaining).
    fn driving_units(&self, task_id: i64) -> (f64, f64) {
        let mut total = (0., 0.);
        for row in self.assignment_rows(task_id) {
            if let Some(v) = self.assignment_view(row).filter(|v| v.drives_dates) {
                if v.budgeted.per_hour > 0. {
                    total.0 += v.budgeted.units;
                }
                if v.remaining.per_hour > 0. {
                    total.1 += v.remaining.units;
                }
            }
        }
        total
    }

    /// Recalculate the activity's driving assignments after one was added or
    /// removed, holding the units in `held` (budgeted, remaining) by the
    /// duration type: Fixed Duration and Units shares the units by
    /// units/time over the same durations (units/time changes); Fixed Units
    /// and Fixed Units/Time share them at each assignment's units/time, so
    /// the durations change; Fixed Duration and Units/Time keeps every
    /// assignment as it is. Budgeted values change only before the activity
    /// starts.
    fn share_units(&mut self, task_id: i64, held: (f64, f64)) -> Result<(), String> {
        let i = self.activity_index(task_id)?;
        let pid = self.model.project.proj_id;
        let kind =
            DurationType::from_code(&task_column(&self.document, pid, task_id, "duration_type"));
        let a = &self.model.activities[i];
        if kind == DurationType::FixedDurationAndUnitsPerTime
            || a.is_completed()
            || !matches!(a.task_type, TaskType::Task | TaskType::Resource)
        {
            return Ok(());
        }
        let started = a.status != StatusType::NotStarted;
        let views: Vec<AssignmentView> = self
            .assignment_rows(task_id)
            .into_iter()
            .filter_map(|r| self.assignment_view(r))
            .filter(|v| v.drives_dates)
            .collect();
        for planned in [true, false] {
            if planned && started {
                continue;
            }
            let side = |v: &AssignmentView| if planned { v.budgeted } else { v.remaining };
            let total = if planned { held.0 } else { held.1 };
            let rate: f64 = views.iter().map(|v| side(v).per_hour).sum();
            if rate <= 0. {
                continue;
            }
            for v in &views {
                let w = side(v);
                if w.per_hour <= 0. {
                    continue;
                }
                let units = total * w.per_hour / rate;
                let per_hour = if kind == DurationType::FixedDurationAndUnits && w.hours > 0. {
                    units / w.hours
                } else {
                    w.per_hour
                };
                let (qty, per) = if planned {
                    ("target_qty", "target_qty_per_hr")
                } else {
                    ("remain_qty", "remain_qty_per_hr")
                };
                let t = self.document.table_mut("TASKRSRC");
                for (k, new, old) in [(qty, units, w.units), (per, per_hour, w.per_hour)] {
                    if (new - old).abs() > 1e-9 * new.abs().max(1.) {
                        t.set(v.row, k, quantity_text(new));
                    }
                }
                self.update_costs(v.row);
            }
        }
        self.sync_activity_durations(i);
        self.rebuild_assignments();
        Ok(())
    }

    /// Add an assignment as [`Self::add_assignment`]; with `recalculate`
    /// (User Preferences, Calculations) the activity's existing driving
    /// assignments share their units with the new one by the duration type.
    pub fn add_assignment_with(
        &mut self,
        task_id: i64,
        rsrc_id: Option<i64>,
        role_id: Option<i64>,
        recalculate: bool,
    ) -> Result<usize, String> {
        let held = self.driving_units(task_id);
        let had = self
            .assignment_rows(task_id)
            .into_iter()
            .filter_map(|r| self.assignment_view(r))
            .any(|v| v.drives_dates && v.remaining.per_hour > 0.);
        let row = self.add_assignment(task_id, rsrc_id, role_id)?;
        if recalculate && had {
            self.share_units(task_id, held)?;
        }
        Ok(row)
    }

    /// Remove an assignment as [`Self::remove_assignment`]; with
    /// `recalculate` the activity's other driving assignments take over its
    /// units by the duration type.
    pub fn remove_assignment_with(&mut self, row: usize, recalculate: bool) -> Result<(), String> {
        let (_, task_id) = self.assignment_task(row)?;
        let held = self.driving_units(task_id);
        self.remove_assignment(row)?;
        if recalculate && !self.assignment_rows(task_id).is_empty() {
            self.share_units(task_id, held)?;
        }
        Ok(())
    }

    /// Replace the assignment's resource. With `new_rate` the assignment
    /// takes the new resource's default units/time (recalculated with the
    /// duration type) and overtime factor; otherwise it keeps its own. With
    /// `role_price` an assignment that also names a role is priced from the
    /// role's rate instead of the resource's.
    pub fn replace_resource(
        &mut self,
        row: usize,
        rsrc_id: i64,
        new_rate: bool,
        role_price: bool,
    ) -> Result<(), String> {
        let places = cost_places(&self.document);
        self.edit_assignment(row, AssignmentEdit::Resource(rsrc_id))?;
        let (t, r) = lookup(&self.document, "RSRC", "rsrc_id", rsrc_id)
            .ok_or_else(|| format!("Resource {rsrc_id} is not in this file"))?;
        let per_hour = float(t.value(r, "def_qty_per_hr"));
        let factor = t.value(r, "ot_factor").to_string();
        if new_rate {
            let view = self
                .assignment_view(row)
                .ok_or("The resource assignment no longer exists")?;
            let (i, _) = self.assignment_task(row)?;
            if per_hour > 0. && (per_hour - view.remaining.per_hour).abs() > 1e-9 {
                self.edit_assignment(row, AssignmentEdit::RemainingUnitsPerTime(per_hour))?;
                if !self.linked(i) && self.model.activities[i].status == StatusType::NotStarted {
                    self.edit_assignment(row, AssignmentEdit::BudgetedUnitsPerTime(per_hour))?;
                }
            }
            if !factor.trim().is_empty() {
                self.edit_assignment(row, AssignmentEdit::OvertimeFactor(float(&factor)))?;
            }
        }
        let view = self
            .assignment_view(row)
            .ok_or("The resource assignment no longer exists")?;
        if role_price && view.rate_source != "ST_Custom" {
            if let Some(role) = view.role_id {
                let at =
                    (self.model.origin_serial * DAY) + self.model.project.data_date.unwrap_or(0);
                let price = rate_price(
                    &self.document,
                    "ROLERATE",
                    "role_id",
                    role,
                    &view.rate_type,
                    at,
                );
                let t = self.document.table_mut("TASKRSRC");
                t.set(row, "cost_per_qty", cost_in(places, price));
                t.set(row, "cost_per_qty_source_type", "ST_Role".into());
                self.update_costs(row);
            }
        }
        Ok(())
    }

    /// Make the row's resource the activity's primary resource.
    pub fn set_primary_resource(&mut self, row: usize, primary: bool) -> Result<(), String> {
        let (_, task_id) = self.assignment_task(row)?;
        let pid = self.model.project.proj_id;
        let t = self.document.table("TASKRSRC").expect("checked");
        let rsrc = optional_id(t.value(&t.rows[row], "rsrc_id"))
            .ok_or("A role assignment cannot be the primary resource")?;
        let current = optional_id(&task_column(&self.document, pid, task_id, "rsrc_id"));
        let value = if primary {
            rsrc.to_string()
        } else if current == Some(rsrc) {
            String::new()
        } else {
            return Ok(());
        };
        set_task_column(&mut self.document, pid, task_id, "rsrc_id", value);
        Ok(())
    }

    /// Apply an edit to an assignment row with the activity's duration type,
    /// then update the activity's durations from its driving assignments.
    pub fn edit_assignment(&mut self, row: usize, edit: AssignmentEdit) -> Result<(), String> {
        let places = cost_places(&self.document);
        let (i, task_id) = self.assignment_task(row)?;
        let view = self
            .assignment_view(row)
            .ok_or("The resource assignment no longer exists")?;
        // Remaining dates are stored as a lag or a duration.
        match edit {
            AssignmentEdit::RemainingStart(t) => {
                let lag = self.remaining_start_lag(i, t)?;
                return self.edit_assignment(row, AssignmentEdit::Lag(lag));
            }
            AssignmentEdit::RemainingFinish(t) => {
                let minutes = self.remaining_finish_duration(i, &view, t)?;
                return self.edit_assignment(row, AssignmentEdit::RemainingDuration(minutes));
            }
            // At completion is actual plus remaining: the remaining takes
            // the change.
            AssignmentEdit::AtCompletionUnits(v) => {
                if !(0. ..1.0e12).contains(&v) {
                    return Err("At completion units must be zero or more".into());
                }
                if self.model.activities[i].is_completed() {
                    return Err("A completed activity's at completion units are fixed".into());
                }
                if v + 1e-9 < view.actual_units {
                    return Err(format!(
                        "At completion units cannot be less than the actual units ({})",
                        quantity_text(view.actual_units)
                    ));
                }
                let left = (v - view.actual_units).max(0.);
                return self.edit_assignment(row, AssignmentEdit::RemainingUnits(left));
            }
            AssignmentEdit::AtCompletionCost(v) => {
                if !(0. ..1.0e12).contains(&v) {
                    return Err("At completion cost must be zero or more".into());
                }
                if self.model.activities[i].is_completed() {
                    return Err("A completed activity's at completion cost is fixed".into());
                }
                if v + 1e-9 < view.actual_cost {
                    return Err(format!(
                        "At completion cost cannot be less than the actual cost ({})",
                        cost_text(view.actual_cost)
                    ));
                }
                let left = (v - view.actual_cost).max(0.);
                return self.edit_assignment(row, AssignmentEdit::RemainingCost(left));
            }
            AssignmentEdit::ActualCost(v) => {
                if !(0. ..1.0e12).contains(&v) {
                    return Err("Actual cost must be zero or more".into());
                }
                if v + 1e-9 < view.actual_overtime_cost {
                    return Err(format!(
                        "The actual cost cannot be less than the overtime cost ({})",
                        cost_text(view.actual_overtime_cost)
                    ));
                }
                let regular = (v - view.actual_overtime_cost).max(0.);
                return self.edit_assignment(row, AssignmentEdit::ActualRegularCost(regular));
            }
            _ => {}
        }
        let pid = self.model.project.proj_id;
        let kind =
            DurationType::from_code(&task_column(&self.document, pid, task_id, "duration_type"));
        let linked = self.linked(i);
        let completed = self.model.activities[i].is_completed();
        let started = self.model.activities[i].status != StatusType::NotStarted;
        // "Subtract Actual from At Completion" (the flag is N) keeps the
        // at-completion units, so remaining units take an actual change.
        let subtract = project_flag(&self.document, pid, "add_act_remain_flag") == "N";
        let cost_to_units = project_flag(&self.document, pid, "cost_qty_recalc_flag") == "Y";
        let cost_linked = {
            let t = self.document.table("TASKRSRC").expect("checked");
            t.value(&t.rows[row], "cost_qty_link_flag") != "N"
        };
        let this_period_linked = project_flag(&self.document, pid, "act_this_per_link_flag") == "Y";
        let at = (self.model.origin_serial * DAY) + self.model.project.data_date.unwrap_or(0);
        let finite = |v: f64, what: &str| {
            if (0. ..1.0e12).contains(&v) {
                Ok(v)
            } else {
                Err(format!("{what} must be zero or more"))
            }
        };
        let mut budgeted = view.budgeted;
        let mut remaining = view.remaining;
        // A driving assignment lasts the activity's remaining duration. Once
        // that is zero (finished work) the units/time stays as it is, since
        // it cannot come from units over no time.
        let driving = self
            .assignment_rows(task_id)
            .into_iter()
            .filter_map(|r| self.assignment_view(r))
            .filter(|v| v.drives_dates)
            .count();
        if view.drives_dates && (self.model.activities[i].rem == 0 || driving == 1) {
            // The only driving assignment lasts the activity's remaining
            // duration, whatever its units and units/time last.
            remaining.hours = self.model.activities[i].rem as f64 / 60.;
        }
        let mut work_changed = false;
        // Apply a change to one side; when budget and remaining are linked both
        // sides take the result.
        let mut change = |planned: bool, changed: Changed, set: &dyn Fn(&mut Work)| {
            let side = if planned {
                &mut budgeted
            } else {
                &mut remaining
            };
            set(side);
            *side = recalculate(kind, changed, *side);
            if linked {
                if planned {
                    remaining = budgeted;
                } else {
                    budgeted = remaining;
                }
            }
            work_changed = true;
        };
        let mut sets: Vec<(&str, String)> = vec![];
        // Actual units to set: (overtime, units).
        let mut actual: Option<(bool, f64)> = None;
        match edit.clone() {
            AssignmentEdit::BudgetedUnits(v) => {
                let v = finite(v, "Budgeted units")?;
                change(true, Changed::Units, &|w| w.units = v);
            }
            AssignmentEdit::RemainingUnits(v) => {
                // The reference keeps remaining units entered on a completed activity's
                // assignment (its activity shows none).
                let v = finite(v, "Remaining units")?;
                change(false, Changed::Units, &|w| w.units = v);
            }
            AssignmentEdit::BudgetedUnitsPerTime(v) => {
                let v = finite(v, "Units / time")?;
                change(true, Changed::UnitsPerTime, &|w| w.per_hour = v);
            }
            AssignmentEdit::RemainingUnitsPerTime(v) => {
                let v = finite(v, "Remaining units / time")?;
                change(false, Changed::UnitsPerTime, &|w| w.per_hour = v);
            }
            AssignmentEdit::OriginalDuration(m) => {
                if m < 0 {
                    return Err("Original duration must be zero or more".into());
                }
                change(true, Changed::Duration, &|w| w.hours = m as f64 / 60.);
            }
            AssignmentEdit::RemainingDuration(m) => {
                if m < 0 {
                    return Err("Remaining duration must be zero or more".into());
                }
                if completed && m > 0 {
                    return Err("A completed activity has no remaining duration".into());
                }
                change(false, Changed::Duration, &|w| w.hours = m as f64 / 60.);
            }
            AssignmentEdit::DriveActivityDates(on) => {
                sets.push(("rollup_dates_flag", if on { "Y" } else { "N" }.into()));
            }
            AssignmentEdit::RateType(code) => {
                if !RATE_TYPES.iter().any(|(c, _, _)| *c == code) {
                    return Err(format!("Unknown rate type {code}"));
                }
                let price = match (view.rate_source.as_str(), view.rsrc_id, view.role_id) {
                    ("ST_Rsrc", Some(id), _) => Some(rate_price(
                        &self.document,
                        "RSRCRATE",
                        "rsrc_id",
                        id,
                        &code,
                        at,
                    )),
                    ("ST_Role", _, Some(id)) => Some(rate_price(
                        &self.document,
                        "ROLERATE",
                        "role_id",
                        id,
                        &code,
                        at,
                    )),
                    _ => None,
                };
                sets.push(("rate_type", code));
                if let Some(p) = price {
                    sets.push(("cost_per_qty", cost_in(places, p)));
                }
            }
            AssignmentEdit::RateSource(code) => {
                let price = match (code.as_str(), view.rsrc_id, view.role_id) {
                    ("ST_Rsrc", Some(id), _) => rate_price(
                        &self.document,
                        "RSRCRATE",
                        "rsrc_id",
                        id,
                        &view.rate_type,
                        at,
                    ),
                    ("ST_Rsrc", None, _) => {
                        return Err("A role assignment has no resource to price from".into())
                    }
                    ("ST_Role", _, Some(id)) => rate_price(
                        &self.document,
                        "ROLERATE",
                        "role_id",
                        id,
                        &view.rate_type,
                        at,
                    ),
                    ("ST_Role", _, None) => {
                        return Err("The assignment has no role to price from".into())
                    }
                    ("ST_Custom", _, _) => view.price,
                    (other, _, _) => return Err(format!("Unknown rate source {other}")),
                };
                sets.push(("cost_per_qty", cost_in(places, price)));
                sets.push(("cost_per_qty_source_type", code));
            }
            AssignmentEdit::PricePerUnit(v) => {
                let v = finite(v, "Price / unit")?;
                sets.push(("cost_per_qty", cost_in(places, v)));
                sets.push(("cost_per_qty_source_type", "ST_Custom".into()));
            }
            AssignmentEdit::CostAccount(id) => {
                if let Some(id) = id {
                    lookup(&self.document, "ACCOUNT", "acct_id", id)
                        .ok_or_else(|| format!("Cost account {id} is not in this file"))?;
                }
                sets.push(("acct_id", id.map_or(String::new(), |v| v.to_string())));
            }
            AssignmentEdit::Resource(id) => {
                let (t, r) = lookup(&self.document, "RSRC", "rsrc_id", id)
                    .ok_or_else(|| format!("Resource {id} is not in this file"))?;
                let kind = match t.value(r, "rsrc_type") {
                    "" => "RT_Labor",
                    s => s,
                };
                sets.push(("rsrc_id", id.to_string()));
                sets.push(("rsrc_type", kind.into()));
                if view.rate_source != "ST_Custom" {
                    let price = rate_price(
                        &self.document,
                        "RSRCRATE",
                        "rsrc_id",
                        id,
                        &view.rate_type,
                        at,
                    );
                    sets.push(("cost_per_qty", cost_in(places, price)));
                    sets.push(("cost_per_qty_source_type", "ST_Rsrc".into()));
                }
            }
            AssignmentEdit::Role(id) => {
                if let Some(id) = id {
                    lookup(&self.document, "ROLES", "role_id", id)
                        .ok_or_else(|| format!("Role {id} is not in this file"))?;
                } else if view.rsrc_id.is_none() {
                    return Err(
                        "A role assignment needs its role; remove the assignment instead".into(),
                    );
                }
                sets.push(("role_id", id.map_or(String::new(), |v| v.to_string())));
            }
            AssignmentEdit::Lag(m) | AssignmentEdit::PlannedLag(m) => {
                if !(0..=100_000 * 60).contains(&m) {
                    return Err("Lag must be zero or more".into());
                }
                let text = quantity_text(m as f64 / 60.);
                let planned = matches!(edit, AssignmentEdit::PlannedLag(_));
                if !planned {
                    sets.push(("relag_drtn_hr_cnt", text.clone()));
                }
                if planned || linked {
                    sets.push(("target_lag_drtn_hr_cnt", text));
                }
            }
            AssignmentEdit::OvertimeFactor(v) => {
                let v = finite(v, "Overtime factor")?;
                sets.push(("ot_factor", quantity_text(v)));
                if cost_linked {
                    let factor = if v > 0. { v } else { 1. };
                    sets.push((
                        "act_ot_cost",
                        cost_in(places, view.actual_overtime_units * view.price * factor),
                    ));
                }
            }
            AssignmentEdit::ActualRegularUnits(v) => {
                actual = Some((false, finite(v, "Actual units")?));
            }
            AssignmentEdit::ActualOvertimeUnits(v) => {
                actual = Some((true, finite(v, "Actual overtime units")?));
            }
            AssignmentEdit::ActualStart(t) => {
                if !started {
                    return Err("Actual dates need a started activity".into());
                }
                let a = &self.model.activities[i];
                let dd = self.model.project.data_date;
                if dd.is_some_and(|d| t > d) {
                    return Err("An actual start cannot be after the data date".into());
                }
                if a.act_start.is_some_and(|x| t < x) {
                    return Err(
                        "An assignment cannot start before its activity's actual start".into(),
                    );
                }
                if view
                    .dates
                    .act_finish
                    .or(a.act_finish)
                    .is_some_and(|f| t > f)
                {
                    return Err("An actual start cannot be after the actual finish".into());
                }
                sets.push((
                    "act_start_date",
                    date_string(self.model.origin_serial, Some(t)),
                ));
            }
            AssignmentEdit::ActualFinish(t) => {
                if !started {
                    return Err("Actual dates need a started activity".into());
                }
                let a = &self.model.activities[i];
                let dd = self.model.project.data_date;
                if dd.is_some_and(|d| t > d) {
                    return Err("An actual finish cannot be after the data date".into());
                }
                if view.dates.act_start.is_some_and(|s| t < s) {
                    return Err("An actual finish cannot be before the actual start".into());
                }
                if a.act_finish.is_some_and(|f| t > f) {
                    return Err(
                        "An assignment cannot finish after its activity's actual finish".into(),
                    );
                }
                sets.push((
                    "act_end_date",
                    date_string(self.model.origin_serial, Some(t)),
                ));
            }
            AssignmentEdit::RemainingStart(_)
            | AssignmentEdit::RemainingFinish(_)
            | AssignmentEdit::AtCompletionUnits(_)
            | AssignmentEdit::AtCompletionCost(_)
            | AssignmentEdit::ActualCost(_) => {
                unreachable!("handled above")
            }
            AssignmentEdit::CostUnitsLinked(on) => {
                sets.push(("cost_qty_link_flag", if on { "Y" } else { "N" }.into()));
            }
            AssignmentEdit::ActualRegularCost(v) | AssignmentEdit::ActualOvertimeCost(v) => {
                let v = finite(v, "Actual cost")?;
                let overtime = matches!(edit, AssignmentEdit::ActualOvertimeCost(_));
                if !started {
                    return Err("Actual costs need a started activity".into());
                }
                let factor = if !overtime {
                    1.
                } else if view.ot_factor > 0. {
                    view.ot_factor
                } else {
                    1.
                };
                if cost_to_units && view.price > 0. {
                    actual = Some((overtime, v / (view.price * factor)));
                } else {
                    let key = if overtime {
                        "act_ot_cost"
                    } else {
                        "act_reg_cost"
                    };
                    sets.push((key, cost_in(places, v)));
                }
            }
            AssignmentEdit::BudgetedCost(v) => {
                let v = finite(v, "Budgeted cost")?;
                if view.price <= 0. {
                    return Err("The assignment has no price per unit; enter units instead".into());
                }
                return self.edit_assignment(row, AssignmentEdit::BudgetedUnits(v / view.price));
            }
            AssignmentEdit::RemainingCost(v) => {
                let v = finite(v, "Remaining cost")?;
                if completed && v > 0. {
                    return Err("A completed activity has no remaining cost".into());
                }
                if cost_linked {
                    if view.price <= 0. {
                        return Err(
                            "The assignment has no price per unit; enter units instead".into()
                        );
                    }
                    return self
                        .edit_assignment(row, AssignmentEdit::RemainingUnits(v / view.price));
                }
                sets.push(("remain_cost", cost_in(places, v)));
            }
            AssignmentEdit::ThisPeriodUnits(v) => {
                let v = finite(v, "Actual units this period")?;
                if !started {
                    return Err("Actual units need a started activity".into());
                }
                if this_period_linked {
                    let regular = view.actual_regular_units + (v - view.this_period_units);
                    if regular < -1e-9 {
                        return Err("The actual units cannot go below zero".into());
                    }
                    return self
                        .edit_assignment(row, AssignmentEdit::ActualRegularUnits(regular.max(0.)));
                }
                sets.push(("act_this_per_qty", quantity_text(v)));
            }
            AssignmentEdit::ThisPeriodCost(v) => {
                let v = finite(v, "Actual cost this period")?;
                if !started {
                    return Err("Actual costs need a started activity".into());
                }
                if this_period_linked {
                    let regular = view.actual_regular_cost + (v - view.this_period_cost);
                    if regular < -1e-9 {
                        return Err("The actual cost cannot go below zero".into());
                    }
                    return self
                        .edit_assignment(row, AssignmentEdit::ActualRegularCost(regular.max(0.)));
                }
                sets.push(("act_this_per_cost", cost_in(places, v)));
            }
        }
        if let Some((overtime, v)) = actual {
            // Zero actual units are allowed on any activity (clearing them
            // after the activity went back to not started).
            if !started && v > 0. {
                return Err("Actual units need a started activity".into());
            }
            let (old, units_key, cost_key, factor) = if overtime {
                let factor = if view.ot_factor > 0. {
                    view.ot_factor
                } else {
                    1.
                };
                (
                    view.actual_overtime_units,
                    "act_ot_qty",
                    "act_ot_cost",
                    factor,
                )
            } else {
                (view.actual_regular_units, "act_reg_qty", "act_reg_cost", 1.)
            };
            sets.push((units_key, quantity_text(v)));
            if cost_linked {
                sets.push((cost_key, cost_in(places, v * view.price * factor)));
            }
            let delta = v - old;
            if subtract && !completed && delta.abs() > 1e-9 {
                let left = (view.remaining.units - delta).max(0.);
                change(false, Changed::Units, &|w| w.units = left);
            }
        }
        if this_period_linked {
            // Actual this period moves with the actual to date.
            let newest = |key: &str, now: f64| {
                sets.iter()
                    .rev()
                    .find(|(k, _)| *k == key)
                    .map_or(now, |(_, text)| float(text))
            };
            let units = newest("act_reg_qty", view.actual_regular_units)
                + newest("act_ot_qty", view.actual_overtime_units)
                - (view.actual_regular_units + view.actual_overtime_units);
            let cost = newest("act_reg_cost", view.actual_regular_cost)
                + newest("act_ot_cost", view.actual_overtime_cost)
                - (view.actual_regular_cost + view.actual_overtime_cost);
            if units.abs() > 1e-9 {
                let now = (view.this_period_units + units).max(0.);
                sets.push(("act_this_per_qty", quantity_text(now)));
            }
            if cost.abs() > 1e-9 {
                let now = (view.this_period_cost + cost).max(0.);
                sets.push(("act_this_per_cost", cost_in(places, now)));
            }
        }
        if work_changed {
            sets.push(("target_qty", quantity_text(budgeted.units)));
            sets.push(("target_qty_per_hr", quantity_text(budgeted.per_hour)));
            sets.push(("remain_qty", quantity_text(remaining.units)));
            sets.push(("remain_qty_per_hr", quantity_text(remaining.per_hour)));
        }
        let table = self.document.table_mut("TASKRSRC");
        let old: Vec<(String, String)> = sets
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect();
        for (k, v) in old {
            // Keep the original text when it already reads as the same number.
            let current = table.value(&table.rows[row], &k).to_string();
            let same_number = matches!(
                k.as_str(),
                "target_qty"
                    | "target_qty_per_hr"
                    | "remain_qty"
                    | "remain_qty_per_hr"
                    | "cost_per_qty"
            ) && current.parse::<f64>().ok().is_some_and(|c| {
                v.parse::<f64>()
                    .is_ok_and(|n| (c - n).abs() <= 1e-9 * n.abs().max(1.))
            });
            if current != v && !same_number {
                table.set(row, &k, v);
            }
        }
        self.update_costs(row);
        self.sync_activity_durations(i);
        self.rebuild_assignments();
        self.sync_task_rollups(task_id);
        Ok(())
    }

    /// The remaining lag (minutes) that puts the remaining start of an
    /// assignment of activity `i` at `t`.
    fn remaining_start_lag(&self, i: usize, t: i64) -> Result<i64, String> {
        let a = &self.model.activities[i];
        if a.is_completed() {
            return Err("A completed activity has no remaining dates".into());
        }
        if a.status != StatusType::NotStarted {
            return Err(
                "A started activity's assignments follow its remaining dates; \
                 change the remaining duration instead"
                    .into(),
            );
        }
        if self.model.project.data_date.is_some_and(|d| t < d) {
            return Err("A remaining start cannot be before the data date".into());
        }
        let start = a.restart.ok_or("The activity has no remaining start yet")?;
        if t < start {
            return Err("An assignment cannot start before its activity's remaining start".into());
        }
        Ok(self
            .assignment_calendar(a)
            .map_or(t - start, |cal| cal.between(start, t)))
    }

    /// The remaining duration (minutes) that puts the remaining finish of the
    /// assignment `view` at `t`.
    fn remaining_finish_duration(
        &self,
        i: usize,
        view: &AssignmentView,
        t: i64,
    ) -> Result<i64, String> {
        let a = &self.model.activities[i];
        if a.is_completed() {
            return Err("A completed activity has no remaining dates".into());
        }
        if self.model.project.data_date.is_some_and(|d| t < d) {
            return Err("A remaining finish cannot be before the data date".into());
        }
        let start = view
            .dates
            .rem_start
            .or(a.restart)
            .ok_or("The activity has no remaining start yet")?;
        if t < start {
            return Err("A remaining finish cannot be before the remaining start".into());
        }
        Ok(self
            .assignment_calendar(a)
            .map_or(t - start, |cal| cal.between(start, t)))
    }

    /// The calendar activity `a`'s assignments work on (the last one when its
    /// index is out of range), or `None` when the project has no calendar:
    /// callers then use elapsed time, as for a missing calendar elsewhere.
    fn assignment_calendar(&self, a: &Activity) -> Option<&CalendarAdapter> {
        let cals = &self.model.calendars;
        cals.get(a.cal.min(cals.len().saturating_sub(1)))
    }

    /// Budgeted and remaining cost follow units x price (remaining only when
    /// the assignment links cost and units).
    pub(crate) fn update_costs(&mut self, row: usize) {
        let places = cost_places(&self.document);
        let t = self.document.table_mut("TASKRSRC");
        let r = &t.rows[row];
        let price = float(t.value(r, "cost_per_qty"));
        let link = t.value(r, "cost_qty_link_flag") != "N";
        let budget = float(t.value(r, "target_qty")) * price;
        let left = float(t.value(r, "remain_qty")) * price;
        let close = |old: &str, new: f64| (float(old) - new).abs() < 0.00005;
        if !close(t.value(r, "target_cost"), budget) {
            t.set(row, "target_cost", cost_in(places, budget));
        }
        let r = &t.rows[row];
        if link && !close(t.value(r, "remain_cost"), left) {
            t.set(row, "remain_cost", cost_in(places, left));
        }
    }

    /// Activity durations follow the driving assignments: the remaining
    /// duration is the longest driving assignment (with its lag before the
    /// activity starts), and a not-started activity's original duration the
    /// longest budgeted one. Assignments without units/time have no duration
    /// of their own and are skipped; with none left the durations are kept.
    pub(crate) fn sync_activity_durations(&mut self, i: usize) {
        let a = &self.model.activities[i];
        if !matches!(a.task_type, TaskType::Task | TaskType::Resource) || a.is_completed() {
            return;
        }
        let task_id = a.task_id;
        let started = a.status != StatusType::NotStarted;
        let Some(t) = self.document.table("TASKRSRC") else {
            return;
        };
        let mut remaining = None::<i64>;
        let mut original = None::<i64>;
        for row in self.assignment_rows(task_id) {
            let r = &t.rows[row];
            if t.value(r, "rollup_dates_flag") != "Y" {
                continue;
            }
            let lag = |k: &str| if started { 0. } else { float(t.value(r, k)) };
            let rate = float(t.value(r, "remain_qty_per_hr"));
            if rate > 0. {
                if let Some(m) = units_minutes(
                    float(t.value(r, "remain_qty")),
                    rate,
                    lag("relag_drtn_hr_cnt"),
                ) {
                    remaining = Some(remaining.map_or(m, |x| x.max(m)));
                }
            }
            let rate = float(t.value(r, "target_qty_per_hr"));
            if rate > 0. {
                if let Some(m) = units_minutes(
                    float(t.value(r, "target_qty")),
                    rate,
                    float(t.value(r, "target_lag_drtn_hr_cnt")),
                ) {
                    original = Some(original.map_or(m, |x| x.max(m)));
                }
            }
        }
        if let Some(m) = remaining {
            self.model.activities[i].rem = m;
        }
        if !started {
            if let (Some(m), Some(d)) = (original, self.details.get_mut(&task_id)) {
                d.original_duration = m;
            }
        }
    }

    /// After an activity's own duration was edited, recalculate its driving
    /// assignments with the duration type, as an edit of each assignment's
    /// duration would. `original` and `remaining` are the activity's new
    /// durations in minutes; `None` leaves that side alone.
    pub fn activity_duration_changed(
        &mut self,
        task_id: i64,
        original: Option<i64>,
        remaining: Option<i64>,
    ) -> Result<(), String> {
        let i = self.activity_index(task_id)?;
        let pid = self.model.project.proj_id;
        let kind =
            DurationType::from_code(&task_column(&self.document, pid, task_id, "duration_type"));
        let linked = self.linked(i);
        let completed = self.model.activities[i].is_completed();
        let mut changed = false;
        for row in self.assignment_rows(task_id) {
            let Some(view) = self.assignment_view(row) else {
                continue;
            };
            if !view.drives_dates {
                continue;
            }
            let mut budgeted = view.budgeted;
            let mut left = view.remaining;
            if let Some(m) = original {
                budgeted.hours = m as f64 / 60.;
                budgeted = recalculate(kind, Changed::Duration, budgeted);
            }
            if let Some(m) = remaining.filter(|_| !completed) {
                left.hours = m as f64 / 60.;
                left = recalculate(kind, Changed::Duration, left);
            } else if linked && original.is_some() {
                left = budgeted;
            }
            let t = self.document.table_mut("TASKRSRC");
            for (k, v, old) in [
                ("target_qty", budgeted.units, view.budgeted.units),
                (
                    "target_qty_per_hr",
                    budgeted.per_hour,
                    view.budgeted.per_hour,
                ),
                ("remain_qty", left.units, view.remaining.units),
                ("remain_qty_per_hr", left.per_hour, view.remaining.per_hour),
            ] {
                if (v - old).abs() > 1e-9 * v.abs().max(1.) {
                    t.set(row, k, quantity_text(v));
                    changed = true;
                }
            }
            self.update_costs(row);
        }
        if changed {
            self.rebuild_assignments();
        }
        Ok(())
    }

    /// Write the scheduled dates (and remaining units the schedule changed)
    /// into this project's TASKRSRC rows, so an export carries current
    /// assignment dates. Call it after a successful schedule. Cells keep their
    /// original text when they already hold the value. Returns the number of
    /// rows that changed.
    ///
    /// Rules, per assignment of an activity in the model:
    /// - `act_start_date` / `act_end_date`: the activity's actual dates. An
    ///   actual date entered on the assignment is kept while it is consistent
    ///   with the activity (a start not before the activity's actual start,
    ///   not after its actual finish; a finish not before the assignment's
    ///   start and not after the activity's actual finish; neither after the
    ///   data date); otherwise it follows the activity.
    /// - `restart_date` / `reend_date`: blank for a completed activity. An
    ///   assignment that lasts as long as the activity (within 1%, at least two
    ///   minutes, see `same_length`) and has no lag takes the activity's
    ///   remaining dates; otherwise it starts its lag after the
    ///   activity's remaining start (no lag once the activity has started) and
    ///   finishes its own duration later, on the activity calendar.
    /// - `rem_late_start_date` / `rem_late_end_date`: the same rule applied to
    ///   the activity's remaining late dates; blank when completed.
    /// - `target_start_date` / `target_end_date`: the same rule applied to the
    ///   activity's planned dates with the budgeted lag and duration.
    /// - `remain_qty`, `remain_qty_per_hr`, `remain_cost`: only when the
    ///   schedule itself changed the activity's remaining duration (an expected
    ///   finish in use). Driving assignments whose duration no longer matches
    ///   are recalculated as a duration edit with the activity's duration type.
    ///
    /// A not-started LOE has no work of its own, so its original duration
    /// follows its remaining duration, which the schedule sets from its dates.
    pub fn write_assignment_schedule(&mut self) -> usize {
        for a in &self.model.activities {
            if a.task_type == TaskType::Loe && a.status == StatusType::NotStarted {
                if let Some(d) = self.details.get_mut(&a.task_id) {
                    d.original_duration = a.rem;
                }
            }
        }
        let pid = self.model.project.proj_id;
        let origin = self.model.origin_serial;
        let index: HashMap<i64, usize> = self
            .model
            .activities
            .iter()
            .enumerate()
            .map(|(i, a)| (a.task_id, i))
            .collect();
        let mut rsrc_cols = match self.document.table("TASKRSRC") {
            Some(t) => t.field_index(),
            None => return 0,
        };
        let rows = self.document.table("TASKRSRC").expect("present").rows.len();
        let leveling = self.model.options.do_level;
        // Remaining units first: they decide the assignment durations below.
        let mut retimed = vec![];
        {
            let t = self.document.table("TASKRSRC").expect("present");
            let c_proj = rsrc_cols.col("proj_id");
            let c_flag = rsrc_cols.col("rollup_dates_flag");
            let c_task = rsrc_cols.col("task_id");
            let c_rate = rsrc_cols.col("remain_qty_per_hr");
            let c_qty = rsrc_cols.col("remain_qty");
            for (row, r) in t.rows.iter().enumerate() {
                if num(FieldIndex::get(r, c_proj)) != pid || FieldIndex::get(r, c_flag) != "Y" {
                    continue;
                }
                let Some(&i) = index.get(&num(FieldIndex::get(r, c_task))) else {
                    continue;
                };
                let a = &self.model.activities[i];
                if a.is_completed()
                    || !a.uses_expected_finish(&self.model.options, leveling)
                    || !matches!(a.task_type, TaskType::Task | TaskType::Resource)
                {
                    continue;
                }
                let rate = float(FieldIndex::get(r, c_rate));
                let qty = float(FieldIndex::get(r, c_qty));
                let current = if rate > 0. {
                    units_minutes(qty, rate, 0.)
                } else {
                    None
                };
                if current.is_some_and(|m| m != a.rem) || (rate <= 0. && qty > 0.) {
                    retimed.push((row, a.task_id, a.rem));
                }
            }
        }
        if !retimed.is_empty() {
            let mut task_kinds: HashMap<i64, DurationType> = HashMap::new();
            if let Some(tasks) = self.document.table("TASK") {
                let tcols = tasks.field_index();
                let c_proj = tcols.col("proj_id");
                let c_task = tcols.col("task_id");
                let c_dur = tcols.col("duration_type");
                for r in &tasks.rows {
                    if num(FieldIndex::get(r, c_proj)) == pid {
                        task_kinds
                            .entry(num(FieldIndex::get(r, c_task)))
                            .or_insert_with(|| DurationType::from_code(FieldIndex::get(r, c_dur)));
                    }
                }
            }
            let mut by_task: HashMap<i64, Vec<usize>> = HashMap::new();
            {
                let t = self.document.table("TASKRSRC").expect("present");
                let c_proj = rsrc_cols.col("proj_id");
                let c_task = rsrc_cols.col("task_id");
                for (row_idx, r) in t.rows.iter().enumerate() {
                    if num(FieldIndex::get(r, c_proj)) == pid {
                        by_task
                            .entry(num(FieldIndex::get(r, c_task)))
                            .or_default()
                            .push(row_idx);
                    }
                }
            }
            let c_qty = rsrc_cols.col("remain_qty");
            let c_rate = rsrc_cols.col("remain_qty_per_hr");
            for (row, task_id, rem) in retimed {
                let kind = task_kinds
                    .get(&task_id)
                    .copied()
                    .unwrap_or(DurationType::FixedDurationAndUnits);
                let t = self.document.table_mut("TASKRSRC");
                let r = &t.rows[row];
                let qty = float(FieldIndex::get(r, c_qty));
                let rate = float(FieldIndex::get(r, c_rate));
                let work = Work {
                    units: qty,
                    per_hour: rate,
                    hours: rem as f64 / 60.,
                };
                let w = if rate > 0. {
                    recalculate(kind, Changed::Duration, work)
                } else {
                    // Units without a units/time have no duration to scale: the
                    // units stay and the units/time spreads them over the new
                    // remaining duration, whatever the duration type.
                    recalculate(DurationType::FixedUnits, Changed::Duration, work)
                };
                if (w.units - qty).abs() > 1e-9 * qty.abs().max(1.) {
                    rsrc_cols.set(t, row, "remain_qty", quantity_text(w.units));
                }
                if (w.per_hour - rate).abs() > 1e-9 * rate.abs().max(1.) {
                    rsrc_cols.set(t, row, "remain_qty_per_hr", quantity_text(w.per_hour));
                }
                self.update_costs(row);
                // The model assignment takes the new duration too, so the
                // assignment the next schedule sees matches the rewritten row
                // without a rebuild. Model assignments are the activity's rows in
                // file order.
                if let Some(i) = index.get(&task_id).copied() {
                    let k = by_task
                        .get(&task_id)
                        .and_then(|v| v.iter().position(|&r| r == row));
                    if let Some(a) = k.and_then(|k| self.model.activities[i].assignments.get_mut(k))
                    {
                        a.rem = rem;
                    }
                }
            }
        }
        let _ = rows;
        let relative = |s: &str| parse_absolute(s).map(|v| v - origin * DAY);
        let mut plan: Vec<(usize, AssignmentDates)> = vec![];
        if let Some(table) = self.document.table("TASKRSRC") {
            let cols = TaskRsrcCols {
                remain_qty: rsrc_cols.col("remain_qty"),
                remain_rate: rsrc_cols.col("remain_qty_per_hr"),
                target_qty: rsrc_cols.col("target_qty"),
                target_rate: rsrc_cols.col("target_qty_per_hr"),
                relag: rsrc_cols.col("relag_drtn_hr_cnt"),
                target_lag: rsrc_cols.col("target_lag_drtn_hr_cnt"),
                act_start: rsrc_cols.col("act_start_date"),
                act_end: rsrc_cols.col("act_end_date"),
            };
            let c_proj = rsrc_cols.col("proj_id");
            let c_task = rsrc_cols.col("task_id");
            plan.reserve(table.rows.len());
            for (row, r) in table.rows.iter().enumerate() {
                if num(FieldIndex::get(r, c_proj)) != pid {
                    continue;
                }
                if let Some(&i) = index.get(&num(FieldIndex::get(r, c_task))) {
                    plan.push((row, self.assignment_dates_at_cols(table, &cols, row, i)));
                }
            }
        }
        let mut changed = 0;
        let table = self.document.table_mut("TASKRSRC");
        for (row, d) in plan {
            let mut w = RowWriter {
                table: &mut *table,
                cols: &mut rsrc_cols,
                row,
                fresh: false,
            };
            let mut row_changed = false;
            for (k, v) in [
                ("act_start_date", d.act_start),
                ("act_end_date", d.act_finish),
                ("restart_date", d.rem_start),
                ("reend_date", d.rem_finish),
                ("rem_late_start_date", d.rem_late_start),
                ("rem_late_end_date", d.rem_late_finish),
                ("target_start_date", d.target_start),
                ("target_end_date", d.target_finish),
            ] {
                row_changed |= w.put(
                    k,
                    |old| {
                        if old.is_empty() {
                            v.is_none()
                        } else {
                            relative(old) == v
                        }
                    },
                    || date_string(origin, v),
                );
            }
            if row_changed {
                changed += 1;
            }
        }
        changed
    }

    /// The dates of assignment row `row` of the activity at model index `i`,
    /// as [`Self::write_assignment_schedule`] writes them.
    fn assignment_dates_at(&self, row: usize, i: usize) -> AssignmentDates {
        let table = self.document.table("TASKRSRC").expect("a row exists");
        let rsrc_cols = table.field_index();
        let cols = TaskRsrcCols {
            remain_qty: rsrc_cols.col("remain_qty"),
            remain_rate: rsrc_cols.col("remain_qty_per_hr"),
            target_qty: rsrc_cols.col("target_qty"),
            target_rate: rsrc_cols.col("target_qty_per_hr"),
            relag: rsrc_cols.col("relag_drtn_hr_cnt"),
            target_lag: rsrc_cols.col("target_lag_drtn_hr_cnt"),
            act_start: rsrc_cols.col("act_start_date"),
            act_end: rsrc_cols.col("act_end_date"),
        };
        self.assignment_dates_at_cols(table, &cols, row, i)
    }
    fn assignment_dates_at_cols(
        &self,
        table: &Table,
        cols: &TaskRsrcCols,
        row: usize,
        i: usize,
    ) -> AssignmentDates {
        let origin = self.model.origin_serial;
        let r = &table.rows[row];
        let a = &self.model.activities[i];
        // A calendar with no recurring work could never place the work (the
        // walk would not end), and a project may have no calendar at all:
        // elapsed time instead.
        let cal = self
            .assignment_calendar(a)
            .filter(|cal| has_recurring_work(&cal.cal));
        let own = matches!(a.task_type, TaskType::Task | TaskType::Resource);
        let started = a.status != StatusType::NotStarted;
        let duration = |qty: Option<usize>, rate: Option<usize>, fallback: i64| {
            let rate = float(FieldIndex::get(r, rate));
            if own && rate > 0. {
                units_minutes(float(FieldIndex::get(r, qty)), rate, 0.).unwrap_or(fallback)
            } else {
                fallback
            }
        };
        let lag = |k: Option<usize>| {
            if own && !started {
                hours(FieldIndex::get(r, k))
            } else {
                0
            }
        };
        let remaining = duration(cols.remain_qty, cols.remain_rate, a.rem);
        let original = self
            .details
            .get(&a.task_id)
            .map_or(0, |d| d.original_duration);
        let budgeted = duration(cols.target_qty, cols.target_rate, original);
        // The activity's own span, or one offset by the assignment's lag
        // and duration.
        let span = |start: Option<i64>, finish: Option<i64>, lag: i64, length: i64, whole: i64| {
            if lag == 0 && same_length(length, whole) {
                return (start, finish);
            }
            let Some(s) = start else {
                return (None, None);
            };
            let s = match (lag > 0, cal) {
                (false, _) => s,
                (true, Some(cal)) => cal.add_start(s, lag),
                (true, None) => s + lag,
            };
            (
                Some(s),
                Some(match (length > 0, cal) {
                    (false, _) => s,
                    (true, Some(cal)) => cal.add(s, length),
                    (true, None) => s + length,
                }),
            )
        };
        let completed = a.is_completed();
        let (rs, re) = if completed {
            (None, None)
        } else {
            span(a.restart, a.reend, lag(cols.relag), remaining, a.rem)
        };
        let (ls, le) = if completed {
            (None, None)
        } else {
            span(
                a.rem_late_start,
                a.rem_late_finish,
                lag(cols.relag),
                remaining,
                a.rem,
            )
        };
        let (ts, te) = span(
            a.target_start,
            a.target_finish,
            if own {
                hours(FieldIndex::get(r, cols.target_lag))
            } else {
                0
            },
            budgeted,
            original,
        );
        // Actual dates entered on the assignment are kept while they are
        // consistent with the activity and the data date (see
        // `actual_dates_valid`); otherwise they follow the activity.
        let stored =
            |k: Option<usize>| parse_absolute(FieldIndex::get(r, k)).map(|v| v - origin * DAY);
        let data_date = self.model.project.data_date;
        let own_start =
            stored(cols.act_start).filter(|&s| started && actual_start_valid(a, data_date, s));
        let act_start = own_start.or(a.act_start);
        let own_finish = stored(cols.act_end)
            .filter(|&f| started && actual_finish_valid(a, data_date, act_start, f));
        AssignmentDates {
            act_start,
            act_finish: own_finish.or(a.act_finish),
            rem_start: rs,
            rem_finish: re,
            rem_late_start: ls,
            rem_late_finish: le,
            target_start: ts,
            target_finish: te,
        }
    }
}

struct TaskRsrcCols {
    remain_qty: Option<usize>,
    remain_rate: Option<usize>,
    target_qty: Option<usize>,
    target_rate: Option<usize>,
    relag: Option<usize>,
    target_lag: Option<usize>,
    act_start: Option<usize>,
    act_end: Option<usize>,
}

/// An assignment actual start is kept when it is not after the data date, the
/// activity's actual finish, and not before the activity's actual start.
fn actual_start_valid(a: &Activity, data_date: Option<i64>, s: i64) -> bool {
    data_date.is_none_or(|d| s <= d)
        && a.act_start.is_none_or(|x| s >= x)
        && a.act_finish.is_none_or(|x| s <= x)
}

/// An assignment actual finish is kept when it is not before the assignment's
/// actual start, not after the data date and not after the activity's actual
/// finish.
fn actual_finish_valid(a: &Activity, data_date: Option<i64>, start: Option<i64>, f: i64) -> bool {
    data_date.is_none_or(|d| f <= d)
        && start.is_none_or(|s| f >= s)
        && a.act_finish.is_none_or(|x| f <= x)
}

// ------------------------------------------------- milestones carry none

/// The text every refusal of an assignment on a milestone contains (front
/// ends map it to the error code `milestone_assignment`).
pub const MILESTONE_RULE: &str = "milestones cannot have resource or role assignments";

/// Whether an activity type is a milestone (start or finish).
pub fn is_milestone(t: TaskType) -> bool {
    matches!(t, TaskType::Mile | TaskType::FinMile)
}

/// The refusal for adding an assignment to milestone `code`.
pub fn milestone_refusal(code: &str) -> String {
    format!(
        "{code} is a milestone: {MILESTONE_RULE} (a milestone may have a primary resource, expenses, documents and work products)."
    )
}

/// The milestone rule for changing activity `task_id` (Activity ID `code`)
/// from type `from` to type `to`, as every edit path applies it. A task
/// with assignments does not become a milestone: the change is refused and
/// the message says to remove the assignments first. A milestone that
/// becomes a task keeps its primary resource only when that resource is
/// assigned (a task's primary resource must be); `Ok(true)` means the
/// caller clears it.
pub fn milestone_type_change(
    p: &ProjectData,
    task_id: i64,
    code: &str,
    from: TaskType,
    to: TaskType,
) -> Result<bool, String> {
    if is_milestone(to) && !is_milestone(from) {
        let labels = assignment_labels(p, task_id);
        if !labels.is_empty() {
            return Err(format!(
                "{code} cannot become a milestone because {MILESTONE_RULE}. Remove its assignments first ({}), then change the activity type.",
                labels.join(", ")
            ));
        }
    }
    Ok(is_milestone(from) && !is_milestone(to) && unassigned_primary(p, task_id))
}

/// Whether activity `task_id` names a primary resource that is not one of
/// its assignments.
pub fn unassigned_primary(p: &ProjectData, task_id: i64) -> bool {
    use crate::edit::records;
    let pid = p.model.project.proj_id;
    let doc = &p.document;
    let primary = records::num(&records::task_value(doc, task_id, pid, "rsrc_id"));
    if primary == 0 {
        return false;
    }
    !records::rows_for(doc, "TASKRSRC", task_id, pid)
        .into_iter()
        .any(|r| records::num(&records::get(doc, "TASKRSRC", r, "rsrc_id")) == primary)
}

/// What each assignment of an activity names: the resource short name, else
/// the role short name, else the assignment id.
pub fn assignment_labels(p: &ProjectData, task_id: i64) -> Vec<String> {
    let pid = p.model.project.proj_id;
    let doc = &p.document;
    let Some(t) = doc.table("TASKRSRC") else {
        return vec![];
    };
    let name = |table: &str, key: &str, short: &str, id: &str| -> String {
        doc.table(table)
            .and_then(|d| {
                d.rows
                    .iter()
                    .find(|r| d.value(r, key) == id)
                    .map(|r| d.value(r, short).to_string())
            })
            .unwrap_or_default()
    };
    crate::edit::records::rows_for(doc, "TASKRSRC", task_id, pid)
        .into_iter()
        .map(|r| {
            let row = &t.rows[r];
            let rsrc = t.value(row, "rsrc_id");
            let role = t.value(row, "role_id");
            let by_rsrc = (!rsrc.trim().is_empty())
                .then(|| name("RSRC", "rsrc_id", "rsrc_short_name", rsrc))
                .filter(|s| !s.is_empty());
            let by_role = (!role.trim().is_empty())
                .then(|| name("ROLES", "role_id", "role_short_name", role))
                .filter(|s| !s.is_empty())
                .map(|s| format!("role {s}"));
            by_rsrc
                .or(by_role)
                .unwrap_or_else(|| format!("assignment {}", t.value(row, "taskrsrc_id")))
        })
        .collect()
}

/// Milestones that carry assignments (an imported file may hold them; they
/// load and save untouched), with what each assignment names.
pub fn milestones_with_assignments(p: &ProjectData) -> Vec<(usize, Vec<String>)> {
    p.model
        .activities
        .iter()
        .enumerate()
        .filter(|(_, a)| is_milestone(a.task_type))
        .filter_map(|(i, a)| {
            let labels = assignment_labels(p, a.task_id);
            (!labels.is_empty()).then_some((i, labels))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use Changed::*;
    use DurationType::*;

    fn w(units: f64, per_hour: f64, hours: f64) -> Work {
        Work {
            units,
            per_hour,
            hours,
        }
    }

    /// Every duration type and edited value: which value is recalculated, and
    /// to what. Start: 80 units at 2 units/hour over 40 hours.
    #[test]
    fn duration_type_matrix() {
        let cases = [
            // (type, changed, edited work, expected work)
            (
                FixedDurationAndUnitsPerTime,
                Duration,
                w(80., 2., 20.),
                w(40., 2., 20.),
            ),
            (
                FixedDurationAndUnitsPerTime,
                Units,
                w(40., 2., 40.),
                w(40., 1., 40.),
            ),
            (
                FixedDurationAndUnitsPerTime,
                UnitsPerTime,
                w(80., 4., 40.),
                w(160., 4., 40.),
            ),
            (
                FixedDurationAndUnits,
                Duration,
                w(80., 2., 20.),
                w(80., 4., 20.),
            ),
            (
                FixedDurationAndUnits,
                Units,
                w(40., 2., 40.),
                w(40., 1., 40.),
            ),
            (
                FixedDurationAndUnits,
                UnitsPerTime,
                w(80., 4., 40.),
                w(160., 4., 40.),
            ),
            (
                FixedUnitsPerTime,
                Duration,
                w(80., 2., 20.),
                w(40., 2., 20.),
            ),
            (FixedUnitsPerTime, Units, w(40., 2., 40.), w(40., 2., 20.)),
            (
                FixedUnitsPerTime,
                UnitsPerTime,
                w(80., 4., 40.),
                w(80., 4., 20.),
            ),
            (FixedUnits, Duration, w(80., 2., 20.), w(80., 4., 20.)),
            (FixedUnits, Units, w(40., 2., 40.), w(40., 2., 20.)),
            (FixedUnits, UnitsPerTime, w(80., 4., 40.), w(80., 4., 20.)),
        ];
        for (kind, changed, edited, expected) in cases {
            assert_eq!(
                recalculate(kind, changed, edited),
                expected,
                "{} after a {changed:?} edit",
                kind.label()
            );
        }
        // The held values never move.
        for kind in DurationType::ALL {
            for changed in [Units, UnitsPerTime, Duration] {
                assert_ne!(kind.recalculates(changed), changed);
            }
        }
    }

    #[test]
    fn zero_rates_and_durations_do_not_divide_by_zero() {
        // A duration cannot come from zero units/time; units/time is derived.
        assert_eq!(
            recalculate(FixedUnits, Units, w(40., 0., 10.)),
            w(40., 4., 10.)
        );
        // Zero duration keeps the units/time.
        assert_eq!(
            recalculate(FixedDurationAndUnits, Duration, w(40., 2., 0.)),
            w(40., 2., 0.)
        );
        assert_eq!(
            recalculate(FixedDurationAndUnits, Units, w(0., 1.4, 0.)),
            w(0., 1.4, 0.)
        );
        assert_eq!(
            recalculate(FixedUnitsPerTime, Units, w(0., 0., 0.)),
            w(0., 0., 0.)
        );
    }

    #[test]
    fn codes_round_trip_and_default() {
        for kind in DurationType::ALL {
            assert_eq!(DurationType::from_code(kind.code()), kind);
        }
        assert_eq!(DurationType::from_code(""), FixedDurationAndUnits);
    }

    #[test]
    fn quantities_and_costs_are_written_as_the_reference_does() {
        assert_eq!(quantity_text(2400. / 350.), "6.85714285714286");
        assert_eq!(quantity_text(34.17), "34.17");
        assert_eq!(quantity_text(420.), "420");
        assert_eq!(quantity_text(0.489977728285078), "0.489977728285078");
        assert_eq!(quantity_text(0.), "0");
        assert_eq!(cost_text(0.), "0.0000");
        assert_eq!(cost_text(12.5), "12.5000");
    }
}
