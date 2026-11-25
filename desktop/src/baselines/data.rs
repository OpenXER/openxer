//! Baseline values per activity. Activities are matched to the baseline by
//! Activity ID. A baseline's dates are its planned dates and its duration is
//! its planned (original) duration; the open project standing in as its own
//! baseline gives its own planned dates. Dates are converted to the open
//! project's minutes so they sit on the same timescale.
use openxer_engine::model::Activity;
use openxer_xer::ProjectData;
use std::sync::Arc;

/// The four baselines a layout can show.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum Slot {
    #[default]
    Project,
    Primary,
    Secondary,
    Tertiary,
}
impl Slot {
    pub const ALL: [Slot; 4] = [
        Slot::Project,
        Slot::Primary,
        Slot::Secondary,
        Slot::Tertiary,
    ];
    pub fn index(self) -> usize {
        self as usize
    }
    pub fn label(self) -> &'static str {
        match self {
            Slot::Project => "Project Baseline",
            Slot::Primary => "Primary Baseline",
            Slot::Secondary => "Secondary Baseline",
            Slot::Tertiary => "Tertiary Baseline",
        }
    }
}

pub use openxer_xer::ev::{current, load, BlDates, DateBasis, Loaded};

/// Where one slot's values come from.
#[derive(Clone, Debug, Default)]
pub enum Source {
    #[default]
    None,
    Current,
    Baseline(Arc<Loaded>),
}

/// The open project's four baselines, resolved.
#[derive(Clone, Debug, Default)]
pub struct Compare {
    pub slots: [Source; 4],
    /// The dates the BL date columns and earned value read.
    pub basis: DateBasis,
}
impl Compare {
    pub fn source(&self, slot: Slot) -> &Source {
        &self.slots[slot.index()]
    }
    /// Values of activity `a` in the slot; `None` when the slot is empty or
    /// the baseline has no activity with this Activity ID. `start` and
    /// `finish` are the planned dates (bars and checks); the BL date columns
    /// and earned value read `dates_in_basis`.
    pub fn dates(&self, p: &ProjectData, a: &Activity, slot: Slot) -> Option<BlDates> {
        match self.source(slot) {
            Source::None => None,
            Source::Current => Some(current(p, a)),
            Source::Baseline(b) => b.by_code.get(&a.task_code).copied(),
        }
    }
    /// `dates_in_basis` with the budgets filled in for the open project
    /// standing in as its own baseline (a stored baseline carries its own).
    pub fn values(
        &self,
        p: &ProjectData,
        ix: &crate::grid::columns::Index,
        a: &Activity,
        slot: Slot,
    ) -> Option<BlDates> {
        let d = match self.source(slot) {
            Source::Current => current(p, a)
                .with_budgets(crate::grid::columns_ev::budget_figures(p, ix, a.task_id)),
            _ => self.dates(p, a, slot)?,
        };
        Some(d.in_basis(self.basis))
    }
    /// The slot's name for the status bar and dialogs.
    pub fn name(&self, slot: Slot) -> String {
        match self.source(slot) {
            Source::None => String::new(),
            Source::Current => "Current Project".into(),
            Source::Baseline(b) => b.name.clone(),
        }
    }
    /// Spans for the four Gantt baseline bars of one activity.
    pub fn spans(&self, p: &ProjectData, a: &Activity) -> [Option<(i64, i64)>; 4] {
        Slot::ALL.map(|slot| {
            let d = self.dates(p, a, slot)?;
            Some((d.start?, d.finish?))
        })
    }
    /// Project baseline finish per activity, falling back to the planned
    /// finish for an activity the baseline does not have (Check Schedule).
    pub fn project_finishes(&self, p: &ProjectData) -> Vec<Option<i64>> {
        p.model
            .activities
            .iter()
            .map(|a| {
                self.dates(p, a, Slot::Project)
                    .and_then(|d| d.finish)
                    .or_else(|| current(p, a).finish)
            })
            .collect()
    }
}

/// What a baseline column shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BlMeasure {
    Start,
    Finish,
    Duration,
    /// The baseline's remaining duration.
    RemainingDuration,
    /// Baseline start minus start, in working time.
    StartVariance,
    /// Baseline finish minus finish, in working time.
    FinishVariance,
    /// Baseline duration minus at completion duration.
    DurationVariance,
    /// The baseline's budgeted labor units, nonlabor units, total cost and
    /// expense cost.
    LaborUnits,
    NonlaborUnits,
    TotalCost,
    ExpenseCost,
    /// At completion labor units (total cost) minus the baseline's budget.
    LaborUnitsVariance,
    TotalCostVariance,
}

/// A baseline column: which baseline and what it shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BlField {
    pub slot: Slot,
    pub measure: BlMeasure,
}
