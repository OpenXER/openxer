//! Cost, earned value and baseline figures of activities, shared by the
//! activity table's columns, Monitor Thresholds and the command line.
//!
//! The definitions, as OpenXER applies them to one activity:
//!
//! - The earned value baseline is the project baseline, or the primary
//!   baseline when the project's settings name the user's primary baseline.
//!   With neither assigned the open project stands in as its own baseline.
//!   Baseline values are its budgeted values and, by the workspace's earned
//!   value date choice, its planned dates or its current dates.
//! - Budget at completion (BAC): the activity's budgeted total cost, or its
//!   budgeted labor units, in that baseline.
//! - Schedule % Complete: how far the data date has moved through the
//!   baseline's planned dates, in working time on the activity's calendar.
//! - Performance % Complete: by the technique of the activity's WBS element
//!   (or of the project's own WBS node).
//! - PV = BAC x Schedule % Complete; for an activity with a resource curve
//!   that shapes an assignment, PV follows the curved planned spread instead
//!   (`curved_planned_value`). EV = BAC x Performance % Complete; AC is the
//!   actual total cost, or the actual labor units.
//! - Estimate to complete (ETC), by the WBS element's technique: the
//!   remaining cost (remaining labor units), or PF x (BAC - EV) with
//!   PF = 1, 1 / CPI, 1 / (CPI x SPI) or a set factor. A factor that needs
//!   an index without a value falls back to PF = 1.
use crate::edit::records::{float, num};
use crate::spread::Shaping;
use crate::{Document, ProjectData};
use openxer_engine::calendar::DAY;
use openxer_engine::model::{Activity, StatusType, TaskType};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

// ---------------------------------------------------------------- figures

/// Budgeted, actual and remaining amounts of one measure.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Figures {
    pub budget: f64,
    pub actual: f64,
    pub remaining: f64,
}
impl Figures {
    pub fn add(&mut self, budget: f64, actual: f64, remaining: f64) {
        self.budget += budget;
        self.actual += actual;
        self.remaining += remaining;
    }
}

/// Units and costs per activity, packed: key `2 x task_id` holds the units
/// and `2 x task_id + 1` the costs, each `[labor, nonlabor, material,
/// expense]`. Assignments give the units and costs of their resource type;
/// an activity without assignments of a type carries its own work and
/// equipment quantities; expense items add to the expense cost.
pub fn figure_table(p: &ProjectData) -> HashMap<i64, [Figures; 4]> {
    let doc = &p.document;
    let pid = p.model.project.proj_id;
    let mut units: HashMap<i64, [Figures; 4]> = HashMap::new();
    let mut costs: HashMap<i64, [Figures; 4]> = HashMap::new();
    let mut labor = HashSet::new();
    let mut nonlabor = HashSet::new();
    if let Some(t) = doc.table("TASKRSRC") {
        let rsrc_types: HashMap<i64, String> = doc.table("RSRC").map_or_else(HashMap::new, |r| {
            r.rows
                .iter()
                .map(|row| {
                    (
                        num(r.value(row, "rsrc_id")),
                        r.value(row, "rsrc_type").to_string(),
                    )
                })
                .collect()
        });
        let has_project = t.field("proj_id").is_some();
        for r in &t.rows {
            if has_project && num(t.value(r, "proj_id")) != pid {
                continue;
            }
            let task = num(t.value(r, "task_id"));
            let v = |k: &str| float(t.value(r, k));
            let mut kind = t.value(r, "rsrc_type").to_string();
            if kind.is_empty() {
                kind = rsrc_types
                    .get(&num(t.value(r, "rsrc_id")))
                    .cloned()
                    .unwrap_or_default();
            }
            let slot = match kind.as_str() {
                "RT_Equip" => 1,
                "RT_Mat" => 2,
                _ => 0,
            };
            match slot {
                0 => labor.insert(task),
                1 => nonlabor.insert(task),
                _ => false,
            };
            units.entry(task).or_default()[slot].add(
                v("target_qty"),
                v("act_reg_qty") + v("act_ot_qty"),
                v("remain_qty"),
            );
            costs.entry(task).or_default()[slot].add(
                v("target_cost"),
                v("act_reg_cost") + v("act_ot_cost"),
                v("remain_cost"),
            );
        }
    }
    if let Some(t) = doc.table("TASK") {
        for r in &t.rows {
            if num(t.value(r, "proj_id")) != pid {
                continue;
            }
            let task = num(t.value(r, "task_id"));
            let v = |k: &str| float(t.value(r, k));
            for (slot, own, columns) in [
                (
                    0,
                    &labor,
                    ["target_work_qty", "act_work_qty", "remain_work_qty"],
                ),
                (
                    1,
                    &nonlabor,
                    ["target_equip_qty", "act_equip_qty", "remain_equip_qty"],
                ),
            ] {
                let (budget, actual, remaining) = (v(columns[0]), v(columns[1]), v(columns[2]));
                if !own.contains(&task) && (budget != 0. || actual != 0. || remaining != 0.) {
                    units.entry(task).or_default()[slot].add(budget, actual, remaining);
                }
            }
        }
    }
    if let Some(t) = doc.table("PROJCOST") {
        let has_project = t.field("proj_id").is_some();
        for r in &t.rows {
            if has_project && num(t.value(r, "proj_id")) != pid {
                continue;
            }
            let v = |k: &str| float(t.value(r, k));
            costs.entry(num(t.value(r, "task_id"))).or_default()[3].add(
                v("target_cost"),
                v("act_cost"),
                v("remain_cost"),
            );
        }
    }
    let mut out = HashMap::new();
    for (task, f) in units {
        out.insert(task * 2, f);
    }
    for (task, f) in costs {
        out.insert(task * 2 + 1, f);
    }
    out
}

/// The sum of the four figures (total cost from the cost slots).
pub fn total(f: &[Figures; 4]) -> Figures {
    let mut total = Figures::default();
    for x in f {
        total.add(x.budget, x.actual, x.remaining);
    }
    total
}

/// Budgeted total cost, labor units, nonlabor units and expense cost of an
/// activity, in that order (what a baseline stores per activity).
pub fn budget_figures(table: &HashMap<i64, [Figures; 4]>, task_id: i64) -> [f64; 4] {
    let units = table.get(&(task_id * 2));
    let costs = table.get(&(task_id * 2 + 1));
    [
        costs.map_or(0., |c| total(c).budget),
        units.map_or(0., |u| u[0].budget),
        units.map_or(0., |u| u[1].budget),
        costs.map_or(0., |c| c[3].budget),
    ]
}

// --------------------------------------------------- durations and percent

/// The activity's original duration in working minutes.
pub fn original(p: &ProjectData, a: &Activity) -> i64 {
    p.details.get(&a.task_id).map_or(0, |d| d.original_duration)
}

/// Working minutes from the actual start to the actual finish (or the data
/// date while in progress).
pub fn elapsed(p: &ProjectData, a: &Activity) -> i64 {
    let finish = a.act_finish.or(p.model.project.data_date);
    match (a.act_start, finish, p.model.calendars.get(a.cal)) {
        (Some(s), Some(f), Some(cal)) if f > s => cal.between(s, f).max(0),
        _ => 0,
    }
}

/// Duration % complete: (original - remaining) over original.
pub fn duration_percent(p: &ProjectData, a: &Activity) -> f64 {
    let original = original(p, a);
    if original > 0 {
        ((original - a.rem) as f64 / original as f64 * 100.).clamp(0., 100.)
    } else if a.is_completed() {
        100.
    } else {
        0.
    }
}

/// Activity % complete by the activity's percent complete type
/// (`complete_pct_type`): physical, units (from its labor units) or
/// duration.
pub fn activity_percent(
    p: &ProjectData,
    a: &Activity,
    percent_type: &str,
    labor_units: Option<Figures>,
) -> f64 {
    match percent_type {
        "CP_Phys" => p.details.get(&a.task_id).map_or(0., |d| d.percent),
        "CP_Units" => {
            let f = labor_units.unwrap_or_default();
            let total = f.actual + f.remaining;
            if total > 0. {
                f.actual / total * 100.
            } else if a.is_completed() {
                100.
            } else {
                0.
            }
        }
        _ => duration_percent(p, a),
    }
}

/// Minutes in a day on calendar `index`: its hours per day, else the
/// project calendar's, else 8 hours.
pub fn day_minutes(p: &ProjectData, index: usize) -> i64 {
    let valid = |hours: f64| {
        (hours.is_finite() && hours > 0. && hours <= 24.).then(|| (hours * 60.).round() as i64)
    };
    let hours = |id: &str| crate::edit::calendars::hours(&p.document, id)[0];
    let project = p
        .model
        .calendars
        .get(p.model.project_calendar)
        .map_or(8 * 60, |c| valid(hours(&c.id)).unwrap_or(8 * 60));
    p.model
        .calendars
        .get(index)
        .map_or(project, |c| valid(hours(&c.id)).unwrap_or(project))
}

// ------------------------------------------------------------- techniques

/// How performance percent complete is measured.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Performance {
    ActivityPercent,
    WbsMilestones,
    ZeroHundred,
    FiftyFifty,
    Custom(f64),
}

/// How the estimate to complete is computed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ToComplete {
    Remaining,
    Pf1,
    PfCpi,
    PfCpiSpi,
    Pf(f64),
}

/// The earned value techniques of one WBS element.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Techniques {
    pub performance: Performance,
    pub to_complete: ToComplete,
}

impl Default for Techniques {
    fn default() -> Self {
        Self {
            performance: Performance::ActivityPercent,
            to_complete: ToComplete::Remaining,
        }
    }
}

/// The stored WBS fields of one element: (performance code, custom
/// percent, ETC code, factor).
type RawTechniques = (String, String, String, String);

fn techniques(raw: &RawTechniques, fallback: &RawTechniques) -> Techniques {
    let get = |own: &String, other: &String| {
        if own.trim().is_empty() {
            other.trim().to_string()
        } else {
            own.trim().to_string()
        }
    };
    let code = get(&raw.0, &fallback.0);
    let pct = get(&raw.1, &fallback.1).parse::<f64>().unwrap_or(0.);
    let etc = get(&raw.2, &fallback.2);
    let factor = get(&raw.3, &fallback.3).parse::<f64>().unwrap_or(1.);
    Techniques {
        performance: match code.as_str() {
            "EC_WBS_pct" => Performance::WbsMilestones,
            "EC_0_100" => Performance::ZeroHundred,
            "EC_50_50" => Performance::FiftyFifty,
            "EC_User_pct" => Performance::Custom(pct.clamp(0., 100.)),
            _ => Performance::ActivityPercent,
        },
        to_complete: match etc.as_str() {
            "EE_PF_1" => ToComplete::Pf1,
            "EE_PF_cpi" => ToComplete::PfCpi,
            "EE_PF_cpi_spi" => ToComplete::PfCpiSpi,
            "EE_PF_usr" => ToComplete::Pf(if factor.is_finite() { factor } else { 1. }),
            _ => ToComplete::Remaining,
        },
    }
}

/// Techniques per WBS id, and the project node's for activities without an
/// element of their own. An element without its own technique takes the
/// project node's.
pub fn technique_table(p: &ProjectData) -> (HashMap<i64, Techniques>, Techniques) {
    let pid = p.model.project.proj_id;
    let mut raw: Vec<(i64, bool, RawTechniques)> = vec![];
    if let Some(t) = p.document.table("PROJWBS") {
        for r in &t.rows {
            if num(t.value(r, "proj_id")) != pid {
                continue;
            }
            raw.push((
                num(t.value(r, "wbs_id")),
                t.value(r, "proj_node_flag") == "Y",
                (
                    t.value(r, "ev_compute_type").into(),
                    t.value(r, "ev_user_pct").into(),
                    t.value(r, "ev_etc_compute_type").into(),
                    t.value(r, "ev_etc_user_value").into(),
                ),
            ));
        }
    }
    let none = RawTechniques::default();
    let node = raw
        .iter()
        .find(|r| r.1)
        .map_or(none.clone(), |r| r.2.clone());
    let map = raw
        .iter()
        .map(|(id, _, own)| (*id, techniques(own, &node)))
        .collect();
    (map, techniques(&node, &none))
}

/// Weighted percent of completed WBS milestones per WBS id.
pub fn milestone_table(doc: &Document) -> HashMap<i64, f64> {
    let mut sums: HashMap<i64, (f64, f64, usize, usize)> = HashMap::new();
    if let Some(t) = doc.table("WBSSTEP") {
        for r in &t.rows {
            let weight = float(t.value(r, "step_wt"));
            let done = t.value(r, "complete_flag") == "Y";
            let e = sums.entry(num(t.value(r, "wbs_id"))).or_default();
            e.0 += weight;
            e.2 += 1;
            if done {
                e.1 += weight;
                e.3 += 1;
            }
        }
    }
    sums.into_iter()
        .map(|(id, (total, done, n, n_done))| {
            let pct = if total > 0. {
                done / total * 100.
            } else {
                n_done as f64 / n.max(1) as f64 * 100.
            };
            (id, pct.clamp(0., 100.))
        })
        .collect()
}

/// Whether earned value reads the user's primary baseline instead of the
/// project baseline (the project's `use_project_baseline_flag` is N).
pub fn uses_primary_baseline(p: &ProjectData) -> bool {
    let pid = p.model.project.proj_id;
    p.document.table("PROJECT").is_some_and(|t| {
        t.rows.iter().any(|r| {
            num(t.value(r, "proj_id")) == pid && t.value(r, "use_project_baseline_flag") == "N"
        })
    })
}

/// Performance % Complete, 0 to 100, by the technique: `activity` is the
/// activity percent complete, `milestones` the element's weighted WBS
/// milestones percent.
pub fn performance_percent(
    technique: Performance,
    a: &Activity,
    activity: impl FnOnce() -> f64,
    milestones: Option<f64>,
) -> f64 {
    let started = a.status != StatusType::NotStarted;
    let done = a.is_completed();
    let pct = match technique {
        Performance::ActivityPercent => activity(),
        Performance::WbsMilestones => milestones.unwrap_or_else(activity),
        Performance::ZeroHundred => {
            if done {
                100.
            } else {
                0.
            }
        }
        Performance::FiftyFifty => {
            if done {
                100.
            } else if started {
                50.
            } else {
                0.
            }
        }
        Performance::Custom(pct) => {
            if done {
                100.
            } else if started {
                pct
            } else {
                0.
            }
        }
    };
    pct.clamp(0., 100.)
}

// -------------------------------------------------------------- baselines

/// Which dates of a baseline earned value and the BL date columns read:
/// the planned dates, or the baseline's current dates (actual when started,
/// else its early dates).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DateBasis {
    #[default]
    Planned,
    Current,
}
impl DateBasis {
    pub fn key(self) -> &'static str {
        match self {
            DateBasis::Planned => "planned",
            DateBasis::Current => "current",
        }
    }
    pub fn from_key(s: &str) -> Self {
        if s.trim() == "current" {
            DateBasis::Current
        } else {
            DateBasis::Planned
        }
    }
    /// The choice as the combo shows it.
    pub fn label(self) -> &'static str {
        match self {
            DateBasis::Planned => "Budgeted values with planned dates",
            DateBasis::Current => "Budgeted values with current dates",
        }
    }
}

/// One activity in a baseline, in the open project's minutes.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BlDates {
    /// Planned dates.
    pub start: Option<i64>,
    pub finish: Option<i64>,
    /// The baseline's current dates.
    pub current_start: Option<i64>,
    pub current_finish: Option<i64>,
    /// Planned duration in working minutes.
    pub duration: i64,
    /// Remaining duration in working minutes.
    pub remaining: i64,
    /// Budgeted total cost, labor units, nonlabor units and expense cost.
    pub budget_cost: f64,
    pub budget_labor_units: f64,
    pub budget_nonlabor_units: f64,
    pub budget_expense_cost: f64,
}

impl BlDates {
    /// The start and finish of the chosen basis in `start` and `finish`.
    pub fn in_basis(mut self, basis: DateBasis) -> Self {
        if basis == DateBasis::Current {
            self.start = self.current_start;
            self.finish = self.current_finish;
        }
        self
    }

    /// Take the budgets of one activity (see [`budget_figures`]).
    pub fn with_budgets(mut self, b: [f64; 4]) -> Self {
        [
            self.budget_cost,
            self.budget_labor_units,
            self.budget_nonlabor_units,
            self.budget_expense_cost,
        ] = b;
        self
    }
}

/// The start the activity table shows: actual, else remaining (leveled) or
/// early, else planned.
pub fn displayed_start(a: &Activity, leveled: bool) -> Option<i64> {
    a.act_start
        .or(if leveled { a.restart } else { a.early_start })
        .or(a.target_start)
}
/// The finish the activity table shows.
pub fn displayed_finish(a: &Activity, leveled: bool) -> Option<i64> {
    a.act_finish
        .or(if leveled { a.reend } else { a.early_finish })
        .or(a.target_finish)
}

fn planned(a: &Activity, original: i64, shift: i64, leveled: bool) -> BlDates {
    // Planned dates; an activity without them falls back to its dates.
    let start = a.target_start.or(a.act_start).or(a.early_start);
    let finish = a.target_finish.or(a.act_finish).or(a.early_finish);
    let mut d = BlDates {
        start: start.map(|m| m + shift),
        finish: finish.map(|m| m + shift),
        current_start: displayed_start(a, leveled).map(|m| m + shift),
        current_finish: displayed_finish(a, leveled).map(|m| m + shift),
        duration: original,
        remaining: a.rem,
        ..Default::default()
    };
    // Milestones have one date.
    match a.task_type {
        TaskType::Mile => {
            d.finish = d.start;
            d.current_finish = d.current_start;
        }
        TaskType::FinMile => {
            d.start = d.finish;
            d.current_start = d.current_finish;
        }
        _ => {}
    }
    d
}

/// The open project's own planned values, used when it is its own baseline.
pub fn current(p: &ProjectData, a: &Activity) -> BlDates {
    planned(a, original(p, a), 0, p.model.options.do_level)
}

/// A stored baseline read for comparison: its activities by Activity ID.
#[derive(Debug, Default)]
pub struct Loaded {
    pub name: String,
    pub by_code: HashMap<String, BlDates>,
    /// The baseline project itself, for what needs more than its activity
    /// values (the time-phased budget of the S-curve export, the budget of
    /// each assignment for earned hours). Its minutes are its own: shift by
    /// `(its origin - the open project's origin) x DAY`.
    pub project: Option<Arc<ProjectData>>,
}

/// Read baseline project `key` of `doc` for a project whose minutes start
/// on day `origin_serial`.
pub fn load(name: &str, doc: Document, key: i64, origin_serial: i64) -> Result<Loaded, String> {
    let p = ProjectData::from_document(doc, key, String::new())?;
    let shift = (p.model.origin_serial - origin_serial) * DAY;
    let table = figure_table(&p);
    let by_code = p
        .model
        .activities
        .iter()
        .map(|a| {
            let d = planned(a, original(&p, a), shift, p.model.options.do_level)
                .with_budgets(budget_figures(&table, a.task_id));
            (a.task_code.clone(), d)
        })
        .collect();
    Ok(Loaded {
        name: name.to_string(),
        by_code,
        project: Some(Arc::new(p)),
    })
}

// ------------------------------------------------------------------ parts

/// Cost or labor units.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Basis {
    Cost,
    LaborUnits,
}

/// The additive parts of earned value for one activity or a band.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Parts {
    pub bac: f64,
    pub pv: f64,
    pub ev: f64,
    pub ac: f64,
    pub etc: f64,
}

impl Parts {
    pub fn add(&mut self, o: Parts) {
        self.bac += o.bac;
        self.pv += o.pv;
        self.ev += o.ev;
        self.ac += o.ac;
        self.etc += o.etc;
    }
    fn ratio(a: f64, b: f64) -> Option<f64> {
        (b.abs() > 1e-9).then(|| a / b)
    }
    pub fn spi(&self) -> Option<f64> {
        Self::ratio(self.ev, self.pv)
    }
    pub fn cpi(&self) -> Option<f64> {
        Self::ratio(self.ev, self.ac)
    }
}

/// Schedule % Complete of an activity against baseline dates, 0 to 100;
/// `None` without a data date or baseline dates.
pub fn schedule_percent(p: &ProjectData, a: &Activity, bl: &BlDates) -> Option<f64> {
    let dd = p.model.project.data_date?;
    let (start, finish) = (bl.start?, bl.finish?);
    if dd <= start {
        return Some(0.);
    }
    if dd >= finish {
        return Some(100.);
    }
    let (done, total) = match p.model.calendars.get(a.cal) {
        Some(cal) => (cal.between(start, dd), cal.between(start, finish)),
        None => (dd - start, finish - start),
    };
    Some(if total > 0 {
        (done as f64 / total as f64 * 100.).clamp(0., 100.)
    } else {
        0.
    })
}

// --------------------------------------------------- curved planned value

/// One assignment of an activity that has at least one assignment shaped by
/// a resource curve: what its planned value share needs.
#[derive(Clone, Debug, PartialEq)]
pub struct CurvedAssignment {
    /// Budgeted units (`target_qty`) and cost (`target_cost`).
    pub budget_units: f64,
    pub budget_cost: f64,
    /// A labor resource (not equipment or material), so its units count in
    /// labor units.
    pub labor: bool,
    /// Its spread shaping (see [`crate::spread::Shaping`]).
    pub shaping: Option<Shaping>,
}

impl CurvedAssignment {
    /// A resource curve shapes this assignment's spread.
    /// A curve or manual budget values shape its planned spread.
    pub fn curved(&self) -> bool {
        self.shaping.as_ref().is_some_and(|s| {
            s.shape.is_some() || s.manual.as_ref().is_some_and(|m| m.budget.is_some())
        })
    }
}

/// The assignments, per task id, of the activities that have at least one
/// assignment shaped by a resource curve; empty when no assignment of the
/// project has one.
pub fn curved_table(p: &ProjectData) -> HashMap<i64, Vec<CurvedAssignment>> {
    let Some(t) = p.document.table("TASKRSRC") else {
        return HashMap::new();
    };
    let pid = p.model.project.proj_id;
    let activities: HashMap<i64, &Activity> =
        p.model.activities.iter().map(|a| (a.task_id, a)).collect();
    let lookup = crate::spread::Lookup::new(p);
    let mut kinds: Option<HashMap<i64, String>> = None;
    let mut out: HashMap<i64, Vec<CurvedAssignment>> = HashMap::new();
    let mut shaped = HashSet::new();
    for (row, r) in t.rows.iter().enumerate() {
        if num(t.value(r, "proj_id")) != pid {
            continue;
        }
        let Some(a) = activities.get(&num(t.value(r, "task_id"))) else {
            continue;
        };
        let mut kind = t.value(r, "rsrc_type").to_string();
        if kind.is_empty() {
            let kinds = kinds.get_or_insert_with(|| {
                p.document.table("RSRC").map_or_else(HashMap::new, |k| {
                    k.rows
                        .iter()
                        .map(|row| {
                            (
                                num(k.value(row, "rsrc_id")),
                                k.value(row, "rsrc_type").to_string(),
                            )
                        })
                        .collect()
                })
            });
            kind = kinds
                .get(&num(t.value(r, "rsrc_id")))
                .cloned()
                .unwrap_or_default();
        }
        let item = CurvedAssignment {
            budget_units: float(t.value(r, "target_qty")),
            budget_cost: float(t.value(r, "target_cost")),
            labor: !matches!(kind.as_str(), "RT_Equip" | "RT_Mat"),
            shaping: lookup.shaping(row, a),
        };
        if item.curved() {
            shaped.insert(a.task_id);
        }
        out.entry(a.task_id).or_default().push(item);
    }
    out.retain(|task, _| shaped.contains(task));
    out
}

/// Planned value of an activity with an assignment that a resource curve
/// shapes (`list`, its assignments from [`curved_table`]); `None` when none
/// does (PV is then BAC x Schedule % Complete). Each assignment's budget in
/// the basis (cost, or labor units of labor resources) weighs its planned
/// share at the data date: along its curve over its own window of the
/// baseline's dates (`crate::spread::planned_share`), or `linear` (Schedule
/// % Complete, 0 to 1) without a curve. The part of BAC no assignment holds
/// (expenses) keeps `linear`. The weighted share is applied to BAC, so a
/// baseline whose budget differs from the open project's scales with it.
/// OpenXER's reading of the documented behavior, unverified until a probe.
pub fn curved_planned_value(
    p: &ProjectData,
    a: &Activity,
    list: &[CurvedAssignment],
    bl: &BlDates,
    bac: f64,
    basis: Basis,
    linear: f64,
) -> Option<f64> {
    if !list.iter().any(CurvedAssignment::curved) {
        return None;
    }
    let dd = p.model.project.data_date?;
    let (start, finish) = (bl.start?, bl.finish?);
    let original = original(p, a);
    let (mut weight, mut planned) = (0., 0.);
    for c in list {
        let w = match basis {
            Basis::Cost => c.budget_cost,
            Basis::LaborUnits if c.labor => c.budget_units,
            Basis::LaborUnits => 0.,
        };
        if w <= 0. {
            continue;
        }
        let share = if c.curved() {
            let item = crate::spread::Item {
                activity: a,
                original,
                shaping: c.shaping.as_ref(),
            };
            crate::spread::planned_share(&p.model, &item, start, finish, dd)
        } else {
            linear
        };
        weight += w;
        planned += w * share;
    }
    let rest = (bac - weight).max(0.);
    let total = weight + rest;
    (weight > 0. && total > 0.).then(|| bac * (planned + rest * linear) / total)
}

/// The parts of one activity from its figures: `bl` its earned value
/// baseline, `own` its figures in the basis, `performance` its Performance %
/// Complete, `to_complete` its WBS element's ETC technique and `curved` its
/// assignments when a resource curve shapes one (see [`curved_table`];
/// empty otherwise), which move planned value off the straight time share.
#[allow(clippy::too_many_arguments)]
pub fn parts(
    p: &ProjectData,
    a: &Activity,
    basis: Basis,
    bl: Option<BlDates>,
    own: Figures,
    performance: f64,
    to_complete: ToComplete,
    curved: &[CurvedAssignment],
) -> Parts {
    let bac = bl.map_or(0., |b| match basis {
        Basis::Cost => b.budget_cost,
        Basis::LaborUnits => b.budget_labor_units,
    });
    let schedule = bl
        .as_ref()
        .and_then(|b| schedule_percent(p, a, b))
        .unwrap_or(0.);
    let pv = bl
        .as_ref()
        .and_then(|b| curved_planned_value(p, a, curved, b, bac, basis, schedule / 100.))
        .unwrap_or(bac * schedule / 100.);
    let mut out = Parts {
        bac,
        pv,
        ev: bac * performance / 100.,
        ac: own.actual,
        etc: 0.,
    };
    let left = out.bac - out.ev;
    out.etc = match to_complete {
        ToComplete::Remaining => own.remaining,
        ToComplete::Pf1 => left,
        ToComplete::PfCpi => out
            .cpi()
            .filter(|c| c.abs() > 1e-9)
            .map_or(left, |c| left / c),
        ToComplete::PfCpiSpi => match (out.cpi(), out.spi()) {
            (Some(c), Some(s)) if (c * s).abs() > 1e-9 => left / (c * s),
            _ => left,
        },
        ToComplete::Pf(factor) => factor * left,
    };
    out
}

/// Earned value of a project in one pass, for front ends without the
/// activity table's lookups (the command line): the same figures, techniques
/// and baselines the table's earned value columns read.
pub struct Calculator<'a> {
    p: &'a ProjectData,
    figures: HashMap<i64, [Figures; 4]>,
    techniques: (HashMap<i64, Techniques>, Techniques),
    milestones: HashMap<i64, f64>,
    /// Assignments of the activities a resource curve shapes, per task id.
    curved: HashMap<i64, Vec<CurvedAssignment>>,
    /// The baseline earned value reads (`None`: the project itself).
    ev_baseline: Option<Arc<Loaded>>,
    /// The project baseline (`None`: the project itself).
    project_baseline: Option<Arc<Loaded>>,
    basis: DateBasis,
}

impl<'a> Calculator<'a> {
    /// `project` and `primary` are the assigned project and primary user
    /// baselines (`None` when the project stands in for itself).
    pub fn new(
        p: &'a ProjectData,
        project: Option<Arc<Loaded>>,
        primary: Option<Arc<Loaded>>,
        basis: DateBasis,
    ) -> Self {
        let ev_baseline = if uses_primary_baseline(p) {
            primary
        } else {
            project.clone()
        };
        Self {
            p,
            figures: figure_table(p),
            techniques: technique_table(p),
            milestones: milestone_table(&p.document),
            curved: curved_table(p),
            ev_baseline,
            project_baseline: project,
            basis,
        }
    }
    fn wbs(&self, a: &Activity) -> i64 {
        self.p.details.get(&a.task_id).map_or(0, |d| d.wbs)
    }
    fn technique(&self, wbs: i64) -> Techniques {
        self.techniques
            .0
            .get(&wbs)
            .copied()
            .unwrap_or(self.techniques.1)
    }
    /// Units (`costs` false) or costs of an activity, `[labor, nonlabor,
    /// material, expense]`.
    pub fn figures(&self, task_id: i64, costs: bool) -> Option<&[Figures; 4]> {
        self.figures.get(&(task_id * 2 + costs as i64))
    }
    /// The activity in the earned value baseline, in the date basis.
    pub fn ev_baseline(&self, a: &Activity) -> Option<BlDates> {
        match &self.ev_baseline {
            Some(b) => b.by_code.get(&a.task_code).map(|d| d.in_basis(self.basis)),
            None => Some(
                current(self.p, a)
                    .with_budgets(budget_figures(&self.figures, a.task_id))
                    .in_basis(self.basis),
            ),
        }
    }
    /// The activity's planned dates in the project baseline (the project's
    /// own when it has none assigned).
    pub fn project_baseline(&self, a: &Activity) -> Option<BlDates> {
        match &self.project_baseline {
            Some(b) => b.by_code.get(&a.task_code).copied(),
            None => Some(current(self.p, a)),
        }
    }
    /// Activity % complete.
    pub fn activity_percent(&self, a: &Activity) -> f64 {
        let pct_type = crate::edit::records::task_value(
            &self.p.document,
            a.task_id,
            self.p.model.project.proj_id,
            "complete_pct_type",
        );
        let labor = self.figures(a.task_id, false).map(|u| u[0]);
        activity_percent(self.p, a, &pct_type, labor)
    }
    /// Performance % complete.
    pub fn performance_percent(&self, a: &Activity) -> f64 {
        let wbs = self.wbs(a);
        performance_percent(
            self.technique(wbs).performance,
            a,
            || self.activity_percent(a),
            self.milestones.get(&wbs).copied(),
        )
    }
    /// Earned value parts of activity `i`.
    pub fn parts(&self, i: usize, basis: Basis) -> Parts {
        let Some(a) = self.p.model.activities.get(i) else {
            return Parts::default();
        };
        let own = match basis {
            Basis::Cost => self.figures(a.task_id, true).map(total),
            Basis::LaborUnits => self.figures(a.task_id, false).map(|u| u[0]),
        }
        .unwrap_or_default();
        parts(
            self.p,
            a,
            basis,
            self.ev_baseline(a),
            own,
            self.performance_percent(a),
            self.technique(self.wbs(a)).to_complete,
            self.curved.get(&a.task_id).map_or(&[], Vec::as_slice),
        )
    }
}
