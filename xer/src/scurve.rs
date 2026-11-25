//! The S-curve export and the cumulative earned hours export: the numbers
//! behind both workbooks, shared by the app's Tools > Reports commands, the
//! command line and MCP, so all three give the same figures.
//!
//! Every spread goes through [`crate::spread`] (the usage views' spreading,
//! resource curves included), every percent complete and earned value
//! through [`crate::ev::Calculator`] (the earned value columns' figures).
//!
//! Rules of the S-curve:
//!
//! - **Labor units**: the budgeted, actual and remaining units of labor
//!   assignments, spread as the Activity Usage Profile spreads them.
//! - **Cost**: the costs of every assignment (labor, nonlabor and material)
//!   plus expenses; an expense spreads evenly over its activity's dates.
//! - **Duration**: each activity weighs its planned (original) duration in
//!   working days on its own calendar, spread evenly over its planned
//!   working days. Actual is that weight times the activity % complete (as
//!   the activity table derives it: physical, duration or units) over its
//!   actual dates, remaining the rest over its remaining dates. Milestones
//!   weigh nothing.
//! - **Budgeted** comes from the earned value baseline (the project
//!   baseline, or the primary baseline when the project chooses it) when one
//!   is assigned and readable, spread over the baseline's own planned
//!   dates; else from the open project's budget over its planned dates.
//! - **Actual** runs to the data date, **remaining** from it (early dates,
//!   or leveled dates when the project levels). **Late dates** is actual
//!   plus remaining spread over the late dates.
//! - **Earned** is one cumulative figure at the data date: the earned value
//!   (baseline budget times performance % complete) of every activity in
//!   scope, in the measure.
use crate::edit::{documents, records::float, records::num, wbs};
use crate::ev::{self, Basis, Calculator, DateBasis, Loaded};
use crate::periods::{self, Interval};
use crate::spread::{self, Item, Plan, Shaping};
use crate::ProjectData;
use openxer_engine::calendar::DAY;
use openxer_engine::model::{Activity, Model, TaskType};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

/// Most periods one curve holds.
pub const MAX_PERIODS: usize = 2000;

/// What the curve measures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Measure {
    Labor,
    Cost,
    Duration,
}

impl Measure {
    pub const ALL: [Measure; 3] = [Measure::Labor, Measure::Cost, Measure::Duration];
    pub fn key(self) -> &'static str {
        match self {
            Measure::Labor => "labor",
            Measure::Cost => "cost",
            Measure::Duration => "duration",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Measure::Labor => "Labor units (hours)",
            Measure::Cost => "Cost",
            Measure::Duration => "Duration (working days)",
        }
    }
    /// The unit a column header names.
    pub fn unit(self) -> &'static str {
        match self {
            Measure::Labor => "Hours",
            Measure::Cost => "Cost",
            Measure::Duration => "Days",
        }
    }
    pub fn from_key(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "labor" | "labor_units" | "units" | "hours" => Some(Measure::Labor),
            "cost" | "costs" => Some(Measure::Cost),
            "duration" | "days" => Some(Measure::Duration),
            _ => None,
        }
    }
}

/// How long a period is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Period {
    Day,
    Week,
    Month,
    /// The project's financial periods.
    Financial,
}

impl Period {
    pub const ALL: [Period; 4] = [Period::Day, Period::Week, Period::Month, Period::Financial];
    pub fn key(self) -> &'static str {
        match self {
            Period::Day => "day",
            Period::Week => "week",
            Period::Month => "month",
            Period::Financial => "financial",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Period::Day => "Day",
            Period::Week => "Week",
            Period::Month => "Month",
            Period::Financial => "Financial period",
        }
    }
    pub fn from_key(s: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|p| p.key() == s.trim().to_ascii_lowercase())
    }
}

/// The rows of the breakdown sheets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Breakdown {
    Total,
    Resource,
    Wbs1,
    Wbs2,
}

impl Breakdown {
    pub const ALL: [Breakdown; 4] = [
        Breakdown::Total,
        Breakdown::Resource,
        Breakdown::Wbs1,
        Breakdown::Wbs2,
    ];
    pub fn key(self) -> &'static str {
        match self {
            Breakdown::Total => "total",
            Breakdown::Resource => "resource",
            Breakdown::Wbs1 => "wbs1",
            Breakdown::Wbs2 => "wbs2",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Breakdown::Total => "Total only",
            Breakdown::Resource => "Resource",
            Breakdown::Wbs1 => "WBS level 1",
            Breakdown::Wbs2 => "WBS level 2",
        }
    }
    pub fn from_key(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "total" | "none" => Some(Breakdown::Total),
            "resource" | "resources" => Some(Breakdown::Resource),
            "wbs" | "wbs1" | "wbs_1" => Some(Breakdown::Wbs1),
            "wbs2" | "wbs_2" => Some(Breakdown::Wbs2),
            _ => None,
        }
    }
}

/// One project the exports read, with what earned value reads besides it.
#[derive(Clone)]
pub struct Source<'a> {
    pub project: &'a ProjectData,
    /// The assigned project and primary user baselines (`None`: the project
    /// stands in for itself), as `ev::Calculator` takes them.
    pub project_baseline: Option<Arc<Loaded>>,
    pub primary_baseline: Option<Arc<Loaded>>,
    pub basis: DateBasis,
    /// Only these activities (model indexes); `None` for all.
    pub only: Option<&'a BTreeSet<usize>>,
}

impl<'a> Source<'a> {
    /// The project on its own, as its own baseline.
    pub fn new(project: &'a ProjectData) -> Self {
        Self {
            project,
            project_baseline: None,
            primary_baseline: None,
            basis: DateBasis::Planned,
            only: None,
        }
    }
    pub fn calculator(&self) -> Calculator<'a> {
        Calculator::new(
            self.project,
            self.project_baseline.clone(),
            self.primary_baseline.clone(),
            self.basis,
        )
    }
    /// The baseline earned value reads (and the budget comes from).
    pub fn baseline(&self) -> Option<&Arc<Loaded>> {
        if ev::uses_primary_baseline(self.project) {
            self.primary_baseline.as_ref()
        } else {
            self.project_baseline.as_ref()
        }
    }
    /// The baseline project the budget spreads from, when one is assigned
    /// and was read in full.
    pub fn budget_project(&self) -> Option<&Arc<ProjectData>> {
        self.baseline()?.project.as_ref()
    }
    pub fn includes(&self, i: usize) -> bool {
        self.only.is_none_or(|o| o.contains(&i))
    }
    /// Where the budget comes from, for the Info sheet.
    pub fn budget_source(&self) -> String {
        match self.baseline() {
            Some(b) if b.project.is_some() => format!("Baseline {:?}", b.name),
            Some(b) => format!(
                "Current project (baseline {:?} could not be read in full)",
                b.name
            ),
            None => "Current project (no baseline assigned)".into(),
        }
    }
}

fn is_milestone(a: &Activity) -> bool {
    matches!(a.task_type, TaskType::Mile | TaskType::FinMile)
}

/// An activity's weight in the duration measure: its planned (original)
/// duration in working days on its own calendar; milestones weigh nothing.
pub fn duration_weight(p: &ProjectData, a: &Activity) -> f64 {
    if is_milestone(a) {
        return 0.;
    }
    let day = ev::day_minutes(p, a.cal).max(1);
    ev::original(p, a).max(0) as f64 / day as f64
}

/// An expense (PROJCOST row) of an activity: budgeted, actual and remaining
/// cost.
struct Expense {
    task: usize,
    q: [f64; 3],
}

fn expenses(p: &ProjectData) -> Vec<Expense> {
    let Some(t) = p.document.table("PROJCOST") else {
        return vec![];
    };
    let pid = p.model.project.proj_id;
    let index: HashMap<i64, usize> = p
        .model
        .activities
        .iter()
        .enumerate()
        .map(|(i, a)| (a.task_id, i))
        .collect();
    let has_project = t.field("proj_id").is_some();
    t.rows
        .iter()
        .filter(|r| !has_project || num(t.value(r, "proj_id")) == pid)
        .filter_map(|r| {
            let v = |k: &str| float(t.value(r, k));
            Some(Expense {
                task: *index.get(&num(t.value(r, "task_id")))?,
                q: [v("target_cost"), v("act_cost"), v("remain_cost")],
            })
        })
        .collect()
}

/// Resource short names and names by id.
fn resource_names(p: &ProjectData) -> HashMap<i64, (String, String)> {
    p.document.table("RSRC").map_or_else(HashMap::new, |t| {
        t.rows
            .iter()
            .map(|r| {
                (
                    num(t.value(r, "rsrc_id")),
                    (
                        t.value(r, "rsrc_short_name").trim().to_string(),
                        t.value(r, "rsrc_name").trim().to_string(),
                    ),
                )
            })
            .collect()
    })
}

/// A resource's row key (its short name) and name.
fn resource_key(names: &HashMap<i64, (String, String)>, rsrc: i64) -> (String, String) {
    match names.get(&rsrc) {
        Some((code, name)) if !code.is_empty() => (code.clone(), name.clone()),
        Some((_, name)) => (format!("#{rsrc}"), name.clone()),
        None => (format!("#{rsrc}"), String::new()),
    }
}

/// The key and name of WBS element `id`'s ancestor at `level` (1: the
/// element under the project node); an element above that level keys
/// itself, an activity on the project node `(Project)`.
pub struct WbsLevels<'p> {
    p: &'p ProjectData,
    by_id: HashMap<i64, (Option<i64>, &'p str)>,
    project: Option<i64>,
    paths: HashMap<i64, String>,
}

impl<'p> WbsLevels<'p> {
    pub fn new(p: &'p ProjectData) -> Self {
        Self {
            p,
            by_id: p
                .wbs
                .iter()
                .map(|w| (w.id, (w.parent, w.name.as_str())))
                .collect(),
            project: wbs::project_node(p),
            paths: HashMap::new(),
        }
    }

    /// The element's code path and name at `level` (0: the element itself).
    pub fn at(&mut self, id: i64, level: usize) -> (String, String) {
        let mut chain = vec![];
        let mut at = Some(id);
        while let Some(w) = at {
            if Some(w) == self.project || chain.len() > self.by_id.len() {
                break;
            }
            let Some((parent, _)) = self.by_id.get(&w) else {
                break;
            };
            chain.push(w);
            at = *parent;
        }
        chain.reverse();
        let pick = if level == 0 {
            chain.last()
        } else {
            chain.get(level - 1).or(chain.last())
        };
        let Some(&w) = pick else {
            return ("(Project)".into(), self.p.model.project.name.clone());
        };
        let p = self.p;
        let path = self
            .paths
            .entry(w)
            .or_insert_with(|| documents::wbs_path(p, w))
            .clone();
        let name = self.by_id.get(&w).map_or("", |x| x.1).to_string();
        (path, name)
    }

    /// The level of activity `a`.
    pub fn of_activity(&mut self, a: &Activity, level: usize) -> (String, String) {
        let id = self.p.details.get(&a.task_id).map_or(0, |d| d.wbs);
        self.at(id, level)
    }
}

/// What a project holds, for the dialog's choices and its one-line note.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Detection {
    /// Activities in scope.
    pub activities: usize,
    /// Activities with a labor assignment that has budgeted units.
    pub labor: usize,
    /// Activities with an assignment or expense that has a budgeted cost.
    pub cost: usize,
    /// Activities (not milestones) with a planned duration.
    pub timed: usize,
    /// Activities with any assignment.
    pub assigned: usize,
    /// The first project has financial periods.
    pub financial: bool,
}

impl Detection {
    pub fn of(sources: &[Source]) -> Self {
        let mut d = Detection::default();
        for src in sources {
            let p = src.project;
            let labor = spread::labor_resources(&p.document);
            let (mut labor_tasks, mut cost_tasks, mut assigned) =
                (BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
            for x in spread::assignment_rows(p) {
                if !src.includes(x.task) {
                    continue;
                }
                assigned.insert(x.task);
                if labor.contains(&x.rsrc) && x.units[0] > 0. {
                    labor_tasks.insert(x.task);
                }
                if x.costs[0] > 0. {
                    cost_tasks.insert(x.task);
                }
            }
            for e in expenses(p) {
                if src.includes(e.task) && e.q[0] > 0. {
                    cost_tasks.insert(e.task);
                }
            }
            for (i, a) in p.model.activities.iter().enumerate() {
                if !src.includes(i) {
                    continue;
                }
                d.activities += 1;
                if duration_weight(p, a) > 0. {
                    d.timed += 1;
                }
            }
            d.labor += labor_tasks.len();
            d.cost += cost_tasks.len();
            d.assigned += assigned.len();
        }
        if let Some(src) = sources.first() {
            let p = src.project;
            d.financial =
                !periods::financial(&p.document, p.model.project.proj_id, p.model.origin_serial)
                    .is_empty();
        }
        d
    }

    /// The measures the project can give, in order of preference.
    pub fn measures(&self) -> Vec<Measure> {
        let mut out = vec![];
        if self.labor > 0 {
            out.push(Measure::Labor);
        }
        if self.cost > 0 {
            out.push(Measure::Cost);
        }
        out.push(Measure::Duration);
        out
    }

    /// The measure chosen when nothing else is asked for: labor units when
    /// labor assignments carry budgeted units, else cost when assignments or
    /// expenses carry one, else duration.
    pub fn measure(&self) -> Measure {
        self.measures()[0]
    }

    pub fn periods(&self) -> Vec<Period> {
        Period::ALL
            .into_iter()
            .filter(|p| *p != Period::Financial || self.financial)
            .collect()
    }

    pub fn breakdowns(&self, m: Measure) -> Vec<Breakdown> {
        Breakdown::ALL
            .into_iter()
            .filter(|b| *b != Breakdown::Resource || (m != Measure::Duration && self.assigned > 0))
            .collect()
    }

    /// Resource when the measure has resources to break down by, else the
    /// total only.
    pub fn breakdown(&self, m: Measure) -> Breakdown {
        if self.breakdowns(m).contains(&Breakdown::Resource) {
            Breakdown::Resource
        } else {
            Breakdown::Total
        }
    }

    /// One plain line saying what was found for measure `m`.
    pub fn line(&self, m: Measure) -> String {
        let n = self.activities;
        match m {
            Measure::Labor => format!("Labor hours found on {} of {n} activities", self.labor),
            Measure::Cost => format!("Costs found on {} of {n} activities", self.cost),
            Measure::Duration if self.labor == 0 && self.cost == 0 => format!(
                "No labor hours or costs found; {} of {n} activities weighed by working days",
                self.timed
            ),
            Measure::Duration => format!(
                "{} of {n} activities weighed by their planned working days",
                self.timed
            ),
        }
    }
}

/// What the S-curve is asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Options {
    pub measure: Measure,
    pub period: Period,
    pub breakdown: Breakdown,
    /// Add the late dates curve (actual plus remaining on late dates).
    pub late: bool,
    /// Weeks start on Sunday (else Monday).
    pub week_starts_sunday: bool,
}

impl Options {
    /// The defaults for what `d` found: its measure, weeks from Sunday, its
    /// breakdown, no late curve.
    pub fn detected(d: &Detection) -> Self {
        let measure = d.measure();
        Self {
            measure,
            period: Period::Week,
            breakdown: d.breakdown(measure),
            late: false,
            week_starts_sunday: true,
        }
    }
}

/// Per-period values of one row or of the total.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Series {
    pub budget: Vec<f64>,
    pub actual: Vec<f64>,
    pub remaining: Vec<f64>,
    /// Actual plus remaining over the late dates (empty without the late
    /// curve).
    pub late: Vec<f64>,
}

impl Series {
    fn zero(n: usize, late: bool) -> Self {
        Self {
            budget: vec![0.; n],
            actual: vec![0.; n],
            remaining: vec![0.; n],
            late: if late { vec![0.; n] } else { vec![] },
        }
    }
}

/// One row of a breakdown: its key (resource short name or WBS code path),
/// name and values.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub key: String,
    pub name: String,
    pub series: Series,
}

/// The S-curve: periods, the total and the breakdown rows.
#[derive(Clone, Debug, PartialEq)]
pub struct Curve {
    pub options: Options,
    /// The first project's origin day serial; every minute here is in its
    /// model.
    pub origin: i64,
    pub periods: Vec<(i64, i64)>,
    pub data_date: Option<i64>,
    pub total: Series,
    /// Rows in key order (empty for the total only).
    pub lines: Vec<Line>,
    /// Budgeted, actual and remaining as stored, summed over everything
    /// the curve read (the periods add up to them unless something falls
    /// outside the periods).
    pub stored: [f64; 3],
    /// Earned to the data date (cumulative) and the period holding it.
    pub earned: Option<f64>,
    pub earned_period: Option<usize>,
    /// The projects' names.
    pub projects: Vec<String>,
    /// Where the budget comes from.
    pub budget_source: String,
    /// The one-line note of what was found.
    pub detection: String,
    pub warnings: Vec<String>,
}

/// One quantity source spread on one model.
struct Piece {
    /// Index into `models`.
    model: usize,
    task: usize,
    original: i64,
    shaping: Option<Shaping>,
    /// Budgeted, actual, remaining.
    q: [f64; 3],
    line: usize,
}

/// Row keys in first-seen order, sorted at the end.
#[derive(Default)]
struct Keys {
    index: HashMap<String, usize>,
    names: Vec<(String, String)>,
}

impl Keys {
    fn of(&mut self, (key, name): (String, String)) -> usize {
        if let Some(&k) = self.index.get(&key) {
            return k;
        }
        self.names.push((key.clone(), name));
        self.index.insert(key, self.names.len() - 1);
        self.names.len() - 1
    }
}

/// The S-curve of `sources` (the first one's minutes and data date).
pub fn curve(sources: &[Source], options: Options) -> Result<Curve, String> {
    let first = sources.first().ok_or("No project is open.")?;
    let origin = first.project.model.origin_serial;
    let detection = Detection::of(sources);
    let measure = options.measure;
    if options.breakdown == Breakdown::Resource && measure == Measure::Duration {
        return Err("The duration measure has no resources to break down by.".into());
    }
    // The models the pieces spread on and each one's shift: its minutes are
    // the first project's plus the shift.
    let mut models: Vec<(&ProjectData, i64)> = vec![];
    let mut pieces: Vec<Piece> = vec![];
    let mut keys = Keys::default();
    let mut warnings: Vec<String> = vec![];
    let mut sources_text: Vec<String> = vec![];
    let mut unassigned = 0;
    let mut no_dates = 0;
    let mut curved = 0;
    let line_key = |keys: &mut Keys,
                    breakdown: Breakdown,
                    resource: Option<(String, String)>,
                    wbs: &mut dyn FnMut(usize) -> (String, String)| {
        match breakdown {
            Breakdown::Total => 0,
            Breakdown::Resource => keys.of(resource.unwrap_or_default()),
            Breakdown::Wbs1 => keys.of(wbs(1)),
            Breakdown::Wbs2 => keys.of(wbs(2)),
        }
    };
    for src in sources {
        let p = src.project;
        let current = models.len();
        models.push((p, (origin - p.model.origin_serial) * DAY));
        let baseline = src.budget_project();
        sources_text.push(src.budget_source());
        if let Some(b) = src.baseline() {
            if b.project.is_none() {
                warnings.push(format!(
                    "Baseline {:?} was not read in full; the budget is the current project's.",
                    b.name
                ));
            }
        }
        let budget_here = baseline.is_none();
        let names = resource_names(p);
        let labor = spread::labor_resources(&p.document);
        let mut levels = WbsLevels::new(p);
        let codes: HashMap<&str, usize> = p
            .model
            .activities
            .iter()
            .enumerate()
            .map(|(i, a)| (a.task_code.as_str(), i))
            .collect();
        let calc = src.calculator();
        // The current project's actual and remaining (and the budget when
        // no baseline is read).
        let keep = |q: [f64; 3]| {
            if budget_here {
                q
            } else {
                [0., q[1], q[2]]
            }
        };
        match measure {
            Measure::Labor | Measure::Cost => {
                let mut labored = BTreeSet::new();
                for x in spread::assignment_rows(p) {
                    if !src.includes(x.task) {
                        continue;
                    }
                    let is_labor = labor.contains(&x.rsrc);
                    if measure == Measure::Labor && !is_labor {
                        continue;
                    }
                    if is_labor && x.units.iter().any(|v| *v != 0.) {
                        labored.insert(x.task);
                    }
                    let q = if measure == Measure::Labor {
                        x.units
                    } else {
                        x.costs
                    };
                    let a = &p.model.activities[x.task];
                    let line = line_key(
                        &mut keys,
                        options.breakdown,
                        Some(resource_key(&names, x.rsrc)),
                        &mut |level| levels.of_activity(a, level),
                    );
                    if x.shaping.as_ref().is_some_and(|s| s.shape.is_some()) {
                        curved += 1;
                    }
                    pieces.push(Piece {
                        model: current,
                        task: x.task,
                        original: x.original,
                        shaping: x.shaping,
                        q: keep(q),
                        line,
                    });
                }
                if measure == Measure::Cost {
                    for e in expenses(p) {
                        if !src.includes(e.task) {
                            continue;
                        }
                        let a = &p.model.activities[e.task];
                        let line = line_key(
                            &mut keys,
                            options.breakdown,
                            Some(("(Expenses)".into(), "Expenses".into())),
                            &mut |level| levels.of_activity(a, level),
                        );
                        pieces.push(Piece {
                            model: current,
                            task: e.task,
                            original: ev::original(p, a),
                            shaping: None,
                            q: keep(e.q),
                            line,
                        });
                    }
                }
                if measure == Measure::Labor {
                    unassigned += p
                        .model
                        .activities
                        .iter()
                        .enumerate()
                        .filter(|(i, a)| {
                            src.includes(*i) && !is_milestone(a) && !labored.contains(i)
                        })
                        .count();
                }
            }
            Measure::Duration => {
                for (i, a) in p.model.activities.iter().enumerate() {
                    if !src.includes(i) {
                        continue;
                    }
                    let w = duration_weight(p, a);
                    if w <= 0. {
                        continue;
                    }
                    let pct = (calc.activity_percent(a) / 100.).clamp(0., 1.);
                    let line = line_key(&mut keys, options.breakdown, None, &mut |level| {
                        levels.of_activity(a, level)
                    });
                    pieces.push(Piece {
                        model: current,
                        task: i,
                        original: ev::original(p, a),
                        shaping: None,
                        q: keep([w, w * pct, w * (1. - pct)]),
                        line,
                    });
                }
            }
        }
        // The budget from the baseline project, over its own planned dates.
        if let Some(b) = baseline {
            let bm = models.len();
            models.push((b, (origin - b.model.origin_serial) * DAY));
            let names = resource_names(b);
            let labor = spread::labor_resources(&b.document);
            let mut own_levels = WbsLevels::new(b);
            // A baseline activity counts when its activity is in scope in
            // the open project (by Activity ID), or, without a filter, also
            // when the open project no longer has it.
            let scope = |a: &Activity| match codes.get(a.task_code.as_str()) {
                Some(&i) => src.includes(i),
                None => src.only.is_none(),
            };
            let mut wbs_of = |a: &Activity, level: usize| match codes.get(a.task_code.as_str()) {
                Some(&i) => levels.of_activity(&p.model.activities[i], level),
                None => own_levels.of_activity(a, level),
            };
            match measure {
                Measure::Labor | Measure::Cost => {
                    for x in spread::assignment_rows(b) {
                        let a = &b.model.activities[x.task];
                        if !scope(a) {
                            continue;
                        }
                        if measure == Measure::Labor && !labor.contains(&x.rsrc) {
                            continue;
                        }
                        let budget = if measure == Measure::Labor {
                            x.units[0]
                        } else {
                            x.costs[0]
                        };
                        let line = line_key(
                            &mut keys,
                            options.breakdown,
                            Some(resource_key(&names, x.rsrc)),
                            &mut |level| wbs_of(a, level),
                        );
                        pieces.push(Piece {
                            model: bm,
                            task: x.task,
                            original: x.original,
                            shaping: x.shaping,
                            q: [budget, 0., 0.],
                            line,
                        });
                    }
                    if measure == Measure::Cost {
                        for e in expenses(b) {
                            let a = &b.model.activities[e.task];
                            if !scope(a) {
                                continue;
                            }
                            let line = line_key(
                                &mut keys,
                                options.breakdown,
                                Some(("(Expenses)".into(), "Expenses".into())),
                                &mut |level| wbs_of(a, level),
                            );
                            pieces.push(Piece {
                                model: bm,
                                task: e.task,
                                original: ev::original(b, a),
                                shaping: None,
                                q: [e.q[0], 0., 0.],
                                line,
                            });
                        }
                    }
                }
                Measure::Duration => {
                    for (i, a) in b.model.activities.iter().enumerate() {
                        if !scope(a) {
                            continue;
                        }
                        let w = duration_weight(b, a);
                        if w <= 0. {
                            continue;
                        }
                        let line = line_key(&mut keys, options.breakdown, None, &mut |level| {
                            wbs_of(a, level)
                        });
                        pieces.push(Piece {
                            model: bm,
                            task: i,
                            original: ev::original(b, a),
                            shaping: None,
                            q: [w, 0., 0.],
                            line,
                        });
                    }
                }
            }
        }
    }
    // Each piece's windows, once.
    let item = |x: &'_ Piece| -> (&Model, i64) {
        let (p, shift) = models[x.model];
        (&p.model, shift)
    };
    let plans: Vec<Plan> = pieces
        .iter()
        .map(|x| {
            let (m, _) = item(x);
            Plan::of(
                m,
                &Item {
                    activity: &m.activities[x.task],
                    original: x.original,
                    shaping: x.shaping.as_ref(),
                },
            )
        })
        .collect();
    // The range: every window that carries a quantity, and the data date.
    let (mut lo, mut hi) = (i64::MAX, i64::MIN);
    let mut reach = |w: Option<(i64, i64)>, shift: i64| {
        if let Some((s, f)) = w {
            lo = lo.min(s - shift);
            hi = hi.max(f.max(s + 1) - shift);
        }
    };
    for (x, plan) in pieces.iter().zip(&plans) {
        let (_, shift) = item(x);
        let w = &plan.windows;
        if x.q[0] != 0. {
            if w.budget.is_none() {
                no_dates += 1;
            }
            reach(w.budget, shift);
        }
        if x.q[1] != 0. {
            reach(w.actual, shift);
        }
        if x.q[2] != 0. {
            reach(w.remaining, shift);
            if options.late {
                reach(w.remaining_late, shift);
            }
        }
    }
    let data_date = first.project.model.project.data_date;
    if lo == i64::MAX {
        return Err(match measure {
            Measure::Labor => "No labor hours to spread: no labor assignment has budgeted, actual or remaining units with dates.".into(),
            Measure::Cost => "No costs to spread: no assignment or expense has a cost with dates.".into(),
            Measure::Duration => "No activity has a planned duration with dates to spread.".into(),
        });
    }
    if let Some(dd) = data_date {
        lo = lo.min(dd);
        hi = hi.max(dd + 1);
    }
    let too_many = |_| {
        format!(
            "More than {MAX_PERIODS} periods; choose a longer period (week or month) for this schedule."
        )
    };
    let periods: Vec<(i64, i64)> = match options.period {
        Period::Day => {
            periods::sweep(origin, Interval::Day, lo, hi, MAX_PERIODS).map_err(too_many)?
        }
        Period::Week => periods::sweep(
            origin,
            Interval::Week {
                sunday: options.week_starts_sunday,
            },
            lo,
            hi,
            MAX_PERIODS,
        )
        .map_err(too_many)?,
        Period::Month => {
            periods::sweep(origin, Interval::Month, lo, hi, MAX_PERIODS).map_err(too_many)?
        }
        Period::Financial => {
            let p = first.project;
            let all = periods::financial(&p.document, p.model.project.proj_id, origin);
            let kept: Vec<(i64, i64)> =
                all.into_iter().filter(|&(s, e)| e > lo && s < hi).collect();
            if kept.is_empty() {
                return Err("The project's financial periods do not cover its dates.".into());
            }
            kept
        }
    };
    let n = periods.len();
    let lines_n = keys.names.len();
    let mut rows: Vec<Series> = (0..lines_n)
        .map(|_| Series::zero(n, options.late))
        .collect();
    let mut total = Series::zero(n, options.late);
    let mut stored = [0.; 3];
    let mut shifted: HashMap<i64, Vec<(i64, i64)>> = HashMap::new();
    let mut buffer = Series::zero(n, options.late);
    for (x, plan) in pieces.iter().zip(&plans) {
        let (m, shift) = item(x);
        let here = shifted.entry(shift).or_insert_with(|| {
            periods
                .iter()
                .map(|(a, b)| (a + shift, b + shift))
                .collect()
        });
        let it = Item {
            activity: &m.activities[x.task],
            original: x.original,
            shaping: x.shaping.as_ref(),
        };
        for v in [
            &mut buffer.budget,
            &mut buffer.actual,
            &mut buffer.remaining,
            &mut buffer.late,
        ] {
            v.iter_mut().for_each(|c| *c = 0.);
        }
        spread::budget(m, &it, plan, x.q[0], here, &mut buffer.budget);
        spread::actual(m, &it, plan, x.q[1], here, &mut buffer.actual);
        spread::remaining(m, &it, plan, x.q[2], here, &mut buffer.remaining);
        if options.late {
            spread::actual(m, &it, plan, x.q[1], here, &mut buffer.late);
            spread::remaining_late(m, &it, plan, x.q[2], here, &mut buffer.late);
        }
        for (s, q) in stored.iter_mut().zip(x.q) {
            *s += q;
        }
        let add = |to: &mut Series| {
            for (to, from) in [
                (&mut to.budget, &buffer.budget),
                (&mut to.actual, &buffer.actual),
                (&mut to.remaining, &buffer.remaining),
                (&mut to.late, &buffer.late),
            ] {
                for (a, b) in to.iter_mut().zip(from) {
                    *a += b;
                }
            }
        };
        add(&mut total);
        if options.breakdown != Breakdown::Total {
            add(&mut rows[x.line]);
        }
    }
    // What no period holds.
    let what = match measure {
        Measure::Labor => "hours",
        Measure::Cost => "of cost",
        Measure::Duration => "days",
    };
    for (name, values, s) in [
        ("budgeted", &total.budget, stored[0]),
        ("actual", &total.actual, stored[1]),
        ("remaining", &total.remaining, stored[2]),
    ] {
        let placed: f64 = values.iter().sum();
        if (s - placed).abs() > 0.005 {
            warnings.push(format!(
                "{:.2} {what} of the {name} total are not in any period (no dates to spread over, or outside the periods).",
                s - placed
            ));
        }
    }
    if unassigned > 0 {
        warnings.push(format!(
            "{unassigned} activities (not milestones) have no labor assignment; they add nothing to the labor curve."
        ));
    }
    if no_dates > 0 {
        warnings.push(format!(
            "{no_dates} assignments or activities have no planned dates; their budget is not on the curve."
        ));
    }
    if curved > 0 {
        warnings.push(format!(
            "{curved} assignments follow a resource curve (Fixed Duration activities); their units are spread along it."
        ));
    }
    // Earned to the data date.
    let (mut earned, mut any) = (0., false);
    if data_date.is_some() {
        for src in sources {
            let p = src.project;
            let calc = src.calculator();
            let bl = src.budget_project();
            let bl_codes: HashMap<&str, &Activity> = bl.map_or_else(HashMap::new, |b| {
                b.model
                    .activities
                    .iter()
                    .map(|a| (a.task_code.as_str(), a))
                    .collect()
            });
            for (i, a) in p.model.activities.iter().enumerate() {
                if !src.includes(i) {
                    continue;
                }
                any = true;
                earned += match measure {
                    Measure::Labor => calc.parts(i, Basis::LaborUnits).ev,
                    Measure::Cost => calc.parts(i, Basis::Cost).ev,
                    Measure::Duration => {
                        let weight = match bl {
                            Some(b) => bl_codes
                                .get(a.task_code.as_str())
                                .map_or(0., |x| duration_weight(b, x)),
                            None => duration_weight(p, a),
                        };
                        weight * calc.performance_percent(a) / 100.
                    }
                };
            }
        }
    }
    let earned_period = data_date.and_then(|dd| {
        periods
            .iter()
            .position(|&(s, e)| dd >= s && dd < e)
            .or_else(|| (dd >= periods.last()?.1).then(|| periods.len() - 1))
    });
    // Rows in key order.
    let mut order: Vec<usize> = (0..lines_n).collect();
    order.sort_by(|a, b| keys.names[*a].0.cmp(&keys.names[*b].0));
    let mut rows: Vec<Option<Series>> = rows.into_iter().map(Some).collect();
    let lines = if options.breakdown == Breakdown::Total {
        vec![]
    } else {
        order
            .into_iter()
            .map(|k| Line {
                key: keys.names[k].0.clone(),
                name: keys.names[k].1.clone(),
                series: rows[k].take().unwrap_or_default(),
            })
            .collect()
    };
    sources_text.dedup();
    Ok(Curve {
        options,
        origin,
        periods,
        data_date,
        total,
        lines,
        stored,
        earned: any.then_some(earned),
        earned_period,
        projects: sources
            .iter()
            .map(|s| s.project.model.project.name.clone())
            .collect(),
        budget_source: sources_text.join("; "),
        detection: detection.line(measure),
        warnings,
    })
}

// ------------------------------------------------------------ earned hours

/// The rows of the Earned to date sheet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Resource,
    Wbs,
    Activity,
}

impl Group {
    pub const ALL: [Group; 3] = [Group::Resource, Group::Wbs, Group::Activity];
    pub fn key(self) -> &'static str {
        match self {
            Group::Resource => "resource",
            Group::Wbs => "wbs",
            Group::Activity => "activity",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Group::Resource => "Resource",
            Group::Wbs => "WBS",
            Group::Activity => "Activity",
        }
    }
    pub fn from_key(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "resource" | "resources" => Some(Group::Resource),
            "wbs" => Some(Group::Wbs),
            "activity" | "activities" => Some(Group::Activity),
            _ => None,
        }
    }
}

/// What the message says when a project has no labor units.
pub const NO_LABOR: &str = "The project has no labor units: no labor resource is assigned with budgeted, actual or remaining hours, so there are no earned hours to export.";

/// One labor assignment (activity and resource) of the By activity sheet.
#[derive(Clone, Debug, PartialEq)]
pub struct EarnedDetail {
    pub activity_id: String,
    pub activity_name: String,
    /// WBS code path and name.
    pub wbs: String,
    pub wbs_name: String,
    pub resource_id: String,
    pub resource_name: String,
    /// The baseline's budgeted units (the current budget without a
    /// baseline).
    pub budgeted: f64,
    /// Performance % complete, 0 to 100.
    pub percent: f64,
    pub earned: f64,
    pub actual: f64,
    pub remaining: f64,
}

/// One row of the Earned to date sheet.
#[derive(Clone, Debug, PartialEq)]
pub struct EarnedRow {
    pub key: String,
    pub name: String,
    pub budgeted: f64,
    pub earned: f64,
    pub actual: f64,
    pub remaining: f64,
}

/// Cumulative earned hours to the data date.
#[derive(Clone, Debug, PartialEq)]
pub struct EarnedHours {
    pub project: String,
    pub origin: i64,
    pub data_date: Option<i64>,
    pub group: Group,
    /// Rows by key (resource short name, WBS path or Activity ID).
    pub rows: Vec<EarnedRow>,
    /// By Activity ID, then resource short name.
    pub details: Vec<EarnedDetail>,
    pub budget_source: String,
    /// How performance % complete was measured.
    pub percent_rule: String,
    pub warnings: Vec<String>,
}

/// The techniques in use, in words.
fn percent_rule(src: &Source) -> String {
    let p = src.project;
    let (map, node) = ev::technique_table(p);
    let mut used: BTreeSet<&'static str> = BTreeSet::new();
    for a in &p.model.activities {
        let wbs = p.details.get(&a.task_id).map_or(0, |d| d.wbs);
        let t = map.get(&wbs).copied().unwrap_or(node);
        used.insert(match t.performance {
            ev::Performance::ActivityPercent => "Activity % Complete (each activity's percent complete type: physical, duration or units)",
            ev::Performance::WbsMilestones => "WBS milestones",
            ev::Performance::ZeroHundred => "0/100",
            ev::Performance::FiftyFifty => "50/50",
            ev::Performance::Custom(_) => "Custom % complete",
        });
    }
    let list: Vec<&str> = used.into_iter().collect();
    format!(
        "Performance % complete by each WBS element's earned value technique: {}",
        if list.is_empty() {
            "Activity % Complete".to_string()
        } else {
            list.join("; ")
        }
    )
}

/// Earned hours of the project of `src`: every labor assignment's baseline
/// budgeted units (by Activity ID and resource) times its activity's
/// performance % complete, with its actual and remaining units.
pub fn earned_hours(src: &Source, group: Group) -> Result<EarnedHours, String> {
    let p = src.project;
    let calc = src.calculator();
    let labor = spread::labor_resources(&p.document);
    let names = resource_names(p);
    let mut levels = WbsLevels::new(p);
    #[derive(Default)]
    struct Acc {
        budgeted: f64,
        actual: f64,
        remaining: f64,
        current_budget: f64,
    }
    // (Activity ID, resource key) -> amounts, and what names them.
    let mut acc: BTreeMap<(String, String), Acc> = BTreeMap::new();
    let mut resource: HashMap<String, String> = HashMap::new();
    let codes: HashMap<&str, usize> = p
        .model
        .activities
        .iter()
        .enumerate()
        .map(|(i, a)| (a.task_code.as_str(), i))
        .collect();
    for x in spread::assignment_rows(p) {
        if !src.includes(x.task) || !labor.contains(&x.rsrc) {
            continue;
        }
        let (key, name) = resource_key(&names, x.rsrc);
        resource.entry(key.clone()).or_insert(name);
        let e = acc
            .entry((p.model.activities[x.task].task_code.clone(), key))
            .or_default();
        e.actual += x.units[1];
        e.remaining += x.units[2];
        e.current_budget += x.units[0];
    }
    let baseline = src.budget_project();
    match baseline {
        Some(b) => {
            let names = resource_names(b);
            let labor = spread::labor_resources(&b.document);
            for x in spread::assignment_rows(b) {
                if !labor.contains(&x.rsrc) {
                    continue;
                }
                let code = &b.model.activities[x.task].task_code;
                if !codes
                    .get(code.as_str())
                    .map_or(src.only.is_none(), |&i| src.includes(i))
                {
                    continue;
                }
                let (key, name) = resource_key(&names, x.rsrc);
                resource.entry(key.clone()).or_insert(name);
                acc.entry((code.clone(), key)).or_default().budgeted += x.units[0];
            }
        }
        None => acc.values_mut().for_each(|e| e.budgeted = e.current_budget),
    }
    if acc
        .values()
        .all(|e| e.budgeted == 0. && e.actual == 0. && e.remaining == 0.)
    {
        return Err(NO_LABOR.into());
    }
    let bl_names: HashMap<&str, &Activity> = baseline.map_or_else(HashMap::new, |b| {
        b.model
            .activities
            .iter()
            .map(|a| (a.task_code.as_str(), a))
            .collect()
    });
    let mut details = vec![];
    for ((code, rkey), e) in acc {
        let current = codes.get(code.as_str()).map(|&i| &p.model.activities[i]);
        let percent = current.map_or(0., |a| calc.performance_percent(a));
        let (name, (wbs, wbs_name)) = match current {
            Some(a) => (
                p.details
                    .get(&a.task_id)
                    .map_or(String::new(), |d| d.name.clone()),
                levels.of_activity(a, 0),
            ),
            None => {
                let b = baseline.expect("only a baseline adds activities the project lacks");
                let mut own = WbsLevels::new(b);
                let a = bl_names[code.as_str()];
                (
                    b.details
                        .get(&a.task_id)
                        .map_or(String::new(), |d| d.name.clone()),
                    own.of_activity(a, 0),
                )
            }
        };
        details.push(EarnedDetail {
            activity_id: code,
            activity_name: name,
            wbs,
            wbs_name,
            resource_name: resource.get(&rkey).cloned().unwrap_or_default(),
            resource_id: rkey,
            budgeted: e.budgeted,
            percent,
            earned: e.budgeted * percent / 100.,
            actual: e.actual,
            remaining: e.remaining,
        });
    }
    let mut rows: BTreeMap<String, EarnedRow> = BTreeMap::new();
    for d in &details {
        let (key, name) = match group {
            Group::Resource => (d.resource_id.clone(), d.resource_name.clone()),
            Group::Wbs => (d.wbs.clone(), d.wbs_name.clone()),
            Group::Activity => (d.activity_id.clone(), d.activity_name.clone()),
        };
        let r = rows.entry(key.clone()).or_insert(EarnedRow {
            key,
            name,
            budgeted: 0.,
            earned: 0.,
            actual: 0.,
            remaining: 0.,
        });
        r.budgeted += d.budgeted;
        r.earned += d.earned;
        r.actual += d.actual;
        r.remaining += d.remaining;
    }
    let mut warnings = vec![];
    if p.model.project.data_date.is_none() {
        warnings.push("The project has no data date.".into());
    }
    if let Some(b) = src.baseline() {
        if b.project.is_none() {
            warnings.push(format!(
                "Baseline {:?} was not read in full; the budget is the current project's.",
                b.name
            ));
        }
    }
    let missing = details
        .iter()
        .filter(|d| !codes.contains_key(d.activity_id.as_str()))
        .count();
    if missing > 0 {
        warnings.push(format!(
            "{missing} baseline assignments are on activities the project no longer has; they earn nothing."
        ));
    }
    Ok(EarnedHours {
        project: p.model.project.name.clone(),
        origin: p.model.origin_serial,
        data_date: p.model.project.data_date,
        group,
        rows: rows.into_values().collect(),
        details,
        budget_source: src.budget_source(),
        percent_rule: percent_rule(src),
        warnings,
    })
}

#[cfg(test)]
#[path = "scurve_tests.rs"]
mod tests;
