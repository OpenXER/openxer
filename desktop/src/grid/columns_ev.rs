//! Earned value columns of the activity table, in cost and in labor units.
//!
//! The definitions, as this program applies them to one activity:
//!
//! - The earned value baseline is the project baseline, or the primary
//!   baseline when the project's settings name the user's primary baseline.
//!   With neither assigned the open project stands in as its own baseline.
//!   Baseline values are its budgeted values and, by the workspace's earned
//!   value date choice, its planned dates or its current dates.
//! - Budget at completion (BAC): the activity's budgeted total cost, or its
//!   budgeted labor units, in that baseline.
//! - Schedule % Complete: how far the data date has moved through the
//!   baseline's planned dates. 0 up to the planned start, 100 from the
//!   planned finish, and in between the working time from the planned start
//!   to the data date over the working time from planned start to planned
//!   finish, on the activity's calendar.
//! - Performance % Complete: by the technique of the activity's WBS element
//!   (or of the project's own WBS node): activity percent complete, 0/100
//!   (nothing until finished), 50/50 (half once started), a custom percent
//!   once started, or the weighted share of completed WBS milestones.
//! - Planned value (PV) = BAC x Schedule % Complete; for an activity with a
//!   resource curve that shapes an assignment, PV follows the planned
//!   (curved) spread instead: each assignment's budget weighs the share its
//!   curve plans by the data date over the baseline's planned dates, and the
//!   rest of BAC (expenses, uncurved assignments) keeps Schedule % Complete
//!   (`openxer_xer::ev::curved_planned_value`, shared with the command line;
//!   OpenXER's reading, unverified). Earned value (EV) =
//!   BAC x Performance % Complete; actual cost (AC) is the actual total
//!   cost, or the actual labor units.
//! - Schedule variance = EV - PV and cost variance = EV - AC; the schedule
//!   performance index is EV / PV and the cost performance index EV / AC,
//!   blank when the divisor is zero.
//! - Estimate to complete (ETC), by the WBS element's technique: the
//!   remaining cost (remaining labor units), or PF x (BAC - EV) with
//!   PF = 1, 1 / CPI, 1 / (CPI x SPI) or a set factor. A factor that needs
//!   an index without a value falls back to PF = 1.
//! - Estimate at completion = AC + ETC; variance at completion = BAC - EAC.
use super::columns::{Field, Index, Measure, Value};
use super::*;
use crate::baselines::data::{self, BlDates, Source};
use crate::baselines::Slot;
use std::cell::OnceCell;
use std::collections::HashMap;

/// What an earned value column shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EvMeasure {
    BudgetAtCompletion,
    PlannedValue,
    EarnedValue,
    ActualCost,
    ScheduleVariance,
    CostVariance,
    SchedulePerformanceIndex,
    CostPerformanceIndex,
    EstimateToComplete,
    EstimateAtCompletion,
    VarianceAtCompletion,
}

use openxer_xer::ev::CurvedAssignment;
pub use openxer_xer::ev::{Basis, Parts, Techniques};

impl EvMeasure {
    pub const ALL: [EvMeasure; 11] = [
        EvMeasure::BudgetAtCompletion,
        EvMeasure::PlannedValue,
        EvMeasure::EarnedValue,
        EvMeasure::ActualCost,
        EvMeasure::ScheduleVariance,
        EvMeasure::CostVariance,
        EvMeasure::SchedulePerformanceIndex,
        EvMeasure::CostPerformanceIndex,
        EvMeasure::EstimateToComplete,
        EvMeasure::EstimateAtCompletion,
        EvMeasure::VarianceAtCompletion,
    ];
    /// (column id, title) for a basis; `None` where the basis has no such
    /// column (actual labor units are a Units column already).
    pub fn caption(self, basis: Basis) -> Option<(&'static str, &'static str)> {
        use Basis::*;
        use EvMeasure::*;
        Some(match (self, basis) {
            (BudgetAtCompletion, Cost) => ("budget_at_completion", "Budget at Completion"),
            (BudgetAtCompletion, LaborUnits) => (
                "budget_at_completion_labor_units",
                "Budget at Completion Labor Units",
            ),
            (PlannedValue, Cost) => ("planned_value_cost", "Planned Value Cost"),
            (PlannedValue, LaborUnits) => {
                ("planned_value_labor_units", "Planned Value Labor Units")
            }
            (EarnedValue, Cost) => ("earned_value_cost", "Earned Value Cost"),
            (EarnedValue, LaborUnits) => ("earned_value_labor_units", "Earned Value Labor Units"),
            (ActualCost, Cost) => ("actual_cost", "Actual Cost"),
            (ActualCost, LaborUnits) => return None,
            (ScheduleVariance, Cost) => ("schedule_variance", "Schedule Variance"),
            (ScheduleVariance, LaborUnits) => (
                "schedule_variance_labor_units",
                "Schedule Variance Labor Units",
            ),
            (CostVariance, Cost) => ("cost_variance", "Cost Variance"),
            (CostVariance, LaborUnits) => {
                ("cost_variance_labor_units", "Cost Variance Labor Units")
            }
            (SchedulePerformanceIndex, Cost) => {
                ("schedule_performance_index", "Schedule Performance Index")
            }
            (SchedulePerformanceIndex, LaborUnits) => (
                "schedule_performance_index_labor_units",
                "Schedule Performance Index Labor Units",
            ),
            (CostPerformanceIndex, Cost) => ("cost_performance_index", "Cost Performance Index"),
            (CostPerformanceIndex, LaborUnits) => (
                "cost_performance_index_labor_units",
                "Cost Performance Index Labor Units",
            ),
            (EstimateToComplete, Cost) => ("estimate_to_complete", "Estimate to Complete"),
            (EstimateToComplete, LaborUnits) => (
                "estimate_to_complete_labor_units",
                "Estimate to Complete Labor Units",
            ),
            (EstimateAtCompletion, Cost) => {
                ("estimate_at_completion_cost", "Estimate at Completion Cost")
            }
            (EstimateAtCompletion, LaborUnits) => (
                "estimate_at_completion_labor_units",
                "Estimate at Completion Labor Units",
            ),
            (VarianceAtCompletion, Cost) => ("variance_at_completion", "Variance at Completion"),
            (VarianceAtCompletion, LaborUnits) => (
                "variance_at_completion_labor_units",
                "Variance at Completion Labor Units",
            ),
        })
    }
    /// Performance indexes are ratios; everything else is cost or units.
    pub fn is_index(self) -> bool {
        matches!(
            self,
            EvMeasure::SchedulePerformanceIndex | EvMeasure::CostPerformanceIndex
        )
    }
}

/// Earned value lookups, built once per pass.
#[derive(Default)]
pub struct EvCache {
    /// Techniques per WBS id, and the project node's for activities
    /// without an element of their own.
    techniques: OnceCell<(HashMap<i64, Techniques>, Techniques)>,
    /// Weighted percent of completed WBS milestones per WBS id.
    milestones: OnceCell<HashMap<i64, f64>>,
    /// The baseline slot earned value reads.
    slot: OnceCell<Slot>,
    /// Parts already computed, per activity and basis.
    parts: std::cell::RefCell<HashMap<(usize, Basis), Parts>>,
    /// Assignments of the activities that have at least one curve-shaped
    /// assignment, per task id (`openxer_xer::ev::curved_table`).
    curved: OnceCell<HashMap<i64, Vec<CurvedAssignment>>>,
}

impl EvCache {
    fn techniques(&self, p: &ProjectData, wbs: i64) -> Techniques {
        let (map, project) = self
            .techniques
            .get_or_init(|| openxer_xer::ev::technique_table(p));
        map.get(&wbs).copied().unwrap_or(*project)
    }

    fn milestones(&self, p: &ProjectData, wbs: i64) -> Option<f64> {
        self.milestones
            .get_or_init(|| openxer_xer::ev::milestone_table(&p.document))
            .get(&wbs)
            .copied()
    }

    fn curved(&self, p: &ProjectData, task_id: i64) -> &[CurvedAssignment] {
        self.curved
            .get_or_init(|| openxer_xer::ev::curved_table(p))
            .get(&task_id)
            .map_or(&[], Vec::as_slice)
    }

    /// The baseline earned value reads: the project baseline unless the
    /// project chooses the user's primary baseline.
    fn slot(&self, p: &ProjectData) -> Slot {
        *self.slot.get_or_init(|| {
            if openxer_xer::ev::uses_primary_baseline(p) {
                Slot::Primary
            } else {
                Slot::Project
            }
        })
    }
}

/// Budgeted total cost, labor units, nonlabor units and expense cost of an
/// activity, in that order (what a baseline stores per activity).
pub fn budget_figures(p: &ProjectData, ix: &Index, task_id: i64) -> [f64; 4] {
    let budget = |m: Measure| ix.figures(p, task_id, m).map_or(0., |f| f.budget);
    [
        budget(Measure::TotalCost),
        budget(Measure::LaborUnits),
        budget(Measure::NonlaborUnits),
        budget(Measure::ExpenseCost),
    ]
}

/// The activity in the earned value baseline, with the start and finish the
/// project's earned value date choice names; the open project's own values
/// and budgets when no baseline is assigned to the slot.
fn baseline(p: &ProjectData, ix: &Index, a: &Activity) -> Option<BlDates> {
    let slot = ix.ext.ev.slot(p);
    let basis = ix
        .baselines
        .as_deref()
        .map_or(data::DateBasis::Planned, |c| c.basis);
    let current = || {
        Some(
            data::current(p, a)
                .with_budgets(budget_figures(p, ix, a.task_id))
                .in_basis(basis),
        )
    };
    match ix.baselines.as_deref().map(|c| c.source(slot)) {
        Some(Source::Baseline(b)) => b.by_code.get(&a.task_code).map(|d| d.in_basis(basis)),
        _ => current(),
    }
}

/// Schedule % Complete of activity `i`, 0 to 100; `None` without a data
/// date or baseline dates.
pub fn schedule_percent(p: &ProjectData, ix: &Index, i: usize) -> Option<f64> {
    let a = p.model.activities.get(i)?;
    let bl = baseline(p, ix, a)?;
    schedule_percent_of(p, a, &bl)
}

fn schedule_percent_of(p: &ProjectData, a: &Activity, bl: &BlDates) -> Option<f64> {
    openxer_xer::ev::schedule_percent(p, a, bl)
}

/// Performance % Complete of activity `i`, 0 to 100.
pub fn performance_percent(p: &ProjectData, ix: &Index, i: usize) -> f64 {
    let Some(a) = p.model.activities.get(i) else {
        return 0.;
    };
    let wbs = p.details.get(&a.task_id).map_or(0, |d| d.wbs);
    let activity = || {
        columns::field(p, ix, i, Field::ActivityPercent)
            .number()
            .unwrap_or(0.)
    };
    openxer_xer::ev::performance_percent(
        ix.ext.ev.techniques(p, wbs).performance,
        a,
        activity,
        ix.ext.ev.milestones(p, wbs),
    )
}

/// Earned value parts of activity `i` in a basis, computed once per set of
/// lookups (every earned value column and band total reads them).
pub fn parts(p: &ProjectData, ix: &Index, i: usize, basis: Basis) -> Parts {
    if let Some(v) = ix.ext.ev.parts.borrow().get(&(i, basis)) {
        return *v;
    }
    let v = parts_of(p, ix, i, basis);
    ix.ext.ev.parts.borrow_mut().insert((i, basis), v);
    v
}

fn parts_of(p: &ProjectData, ix: &Index, i: usize, basis: Basis) -> Parts {
    let Some(a) = p.model.activities.get(i) else {
        return Parts::default();
    };
    let measure = match basis {
        Basis::Cost => Measure::TotalCost,
        Basis::LaborUnits => Measure::LaborUnits,
    };
    let own = ix.figures(p, a.task_id, measure).unwrap_or_default();
    let wbs = p.details.get(&a.task_id).map_or(0, |d| d.wbs);
    openxer_xer::ev::parts(
        p,
        a,
        basis,
        baseline(p, ix, a),
        own,
        performance_percent(p, ix, i),
        ix.ext.ev.techniques(p, wbs).to_complete,
        ix.ext.ev.curved(p, a.task_id),
    )
}

/// The value one earned value column shows for summed parts.
pub fn show(parts: Parts, measure: EvMeasure, basis: Basis) -> Value {
    let amount = |v: f64| match basis {
        Basis::Cost => Value::Cost(v),
        Basis::LaborUnits => Value::Units(v),
    };
    let index =
        |v: Option<f64>| v.map_or(Value::Empty, |v| Value::Number((v * 100.).round() / 100.));
    let eac = parts.ac + parts.etc;
    match measure {
        EvMeasure::BudgetAtCompletion => amount(parts.bac),
        EvMeasure::PlannedValue => amount(parts.pv),
        EvMeasure::EarnedValue => amount(parts.ev),
        EvMeasure::ActualCost => amount(parts.ac),
        EvMeasure::ScheduleVariance => amount(parts.ev - parts.pv),
        EvMeasure::CostVariance => amount(parts.ev - parts.ac),
        EvMeasure::SchedulePerformanceIndex => index(parts.spi()),
        EvMeasure::CostPerformanceIndex => index(parts.cpi()),
        EvMeasure::EstimateToComplete => amount(parts.etc),
        EvMeasure::EstimateAtCompletion => amount(eac),
        EvMeasure::VarianceAtCompletion => amount(parts.bac - eac),
    }
}

/// The column value for activity `i`.
pub fn value(p: &ProjectData, ix: &Index, i: usize, measure: EvMeasure, basis: Basis) -> Value {
    show(parts(p, ix, i, basis), measure, basis)
}

/// A band's total: the members' parts summed, the indexes from the sums.
pub fn band_total(
    p: &ProjectData,
    ix: &Index,
    members: &[usize],
    measure: EvMeasure,
    basis: Basis,
) -> Value {
    let mut sum = Parts::default();
    for &i in members {
        sum.add(parts(p, ix, i, basis));
    }
    show(sum, measure, basis)
}
