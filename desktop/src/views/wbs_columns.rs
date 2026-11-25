//! The WBS table's column catalog: every column the WBS window can show,
//! its Columns chooser group, and where its value comes from. A column is
//! either a field of the element itself (a PROJWBS column, its budget log
//! or spending plan) or an activity column rolled up over the element's
//! activities and those of the elements below it. Rolled-up values come
//! from the same band totals the Activities table draws on its grouping
//! bands (`grid::group::band_total`), so a WBS row and the matching WBS
//! band of the Activities table always agree.
//!
//! The groups and columns follow the reference's WBS Columns chooser as
//! captured; columns it offers that OpenXER does not compute for activity
//! bands yet (durations other than Total Float, baseline dates, Forecast
//! dates, the variance dates and durations, tallies, Est Weight shares,
//! WBS Category) are left out.
use crate::grid::columns::{self as acols, Column, Kind, Value};
use crate::grid::columns_ext::{EARNED_VALUE, NUMBER_OF_ACTIVITIES};
use openxer_xer::ProjectData;

pub const GENERAL: &str = acols::GENERAL;
pub const BUDGET: &str = "Budget";

/// A field of the WBS element itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Own {
    /// The full code path (`wbs_short_name` of each level).
    Code,
    Name,
    /// `status_code`.
    Status,
    /// The responsible manager (`obs_id`, shown by OBS name).
    Manager,
    AnticipatedStart,
    AnticipatedFinish,
    /// Estimated weight (`est_wt`).
    EstWeight,
    /// `orig_cost`.
    OriginalBudget,
    /// The original budget with the approved changes of the budget log.
    CurrentBudget,
    /// The current budget with the pending changes.
    ProposedBudget,
    /// Sum of the spending plan's spending and benefit.
    SpendingPlan,
    BenefitPlan,
    /// `indep_remain_work_qty`, `indep_remain_total_cost`.
    IndependentUnits,
    IndependentCost,
}

impl Own {
    /// The PROJWBS column a typed value is written to, for the fields that
    /// edit in the table (the code and the name go through the structure).
    pub fn column(self) -> Option<&'static str> {
        Some(match self {
            Own::Code => "wbs_short_name",
            Own::Name => "wbs_name",
            Own::Status => "status_code",
            Own::Manager => "obs_id",
            Own::AnticipatedStart => "anticip_start_date",
            Own::AnticipatedFinish => "anticip_end_date",
            Own::EstWeight => "est_wt",
            Own::OriginalBudget => "orig_cost",
            Own::IndependentUnits => "indep_remain_work_qty",
            Own::IndependentCost => "indep_remain_total_cost",
            Own::CurrentBudget | Own::ProposedBudget | Own::SpendingPlan | Own::BenefitPlan => {
                return None
            }
        })
    }
}

/// Where a WBS column's value comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Own(Own),
    /// An activity column (by id) totalled over the element's activities.
    Rolled(&'static str),
}

/// One column the WBS table can show.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WbsColumn {
    /// The cell commands' field id (Find, Replace, the cell editor).
    pub id: &'static str,
    pub title: &'static str,
    pub group: &'static str,
    pub width: f32,
    pub kind: Kind,
    pub source: Source,
}

const fn own(
    id: &'static str,
    title: &'static str,
    group: &'static str,
    width: f32,
    kind: Kind,
    field: Own,
) -> WbsColumn {
    WbsColumn {
        id,
        title,
        group,
        width,
        kind,
        source: Source::Own(field),
    }
}

/// An activity column rolled up; its title, group and kind are the
/// activity column's own (read from the activity catalog).
const fn rolled(id: &'static str) -> WbsColumn {
    WbsColumn {
        id,
        title: "",
        group: "",
        width: 90.,
        kind: Kind::Text,
        source: Source::Rolled(id),
    }
}

/// The catalog in field order. The first three are the fields the WBS
/// table always had, so stored column orders keep their meaning; new
/// columns are only ever added at the end.
const CATALOG: &[WbsColumn] = &[
    own("wbs_code", "WBS Code", GENERAL, 242., Kind::Text, Own::Code),
    own("wbs_name", "WBS Name", GENERAL, 200., Kind::Text, Own::Name),
    WbsColumn {
        id: "total_activities",
        title: "Total Activities",
        group: NUMBER_OF_ACTIVITIES,
        width: 73.,
        kind: Kind::Number,
        source: Source::Rolled("number_of_activities"),
    },
    own(
        "wbs_status",
        "Status",
        GENERAL,
        80.,
        Kind::Text,
        Own::Status,
    ),
    own(
        "responsible_manager",
        "Responsible Manager",
        GENERAL,
        140.,
        Kind::Text,
        Own::Manager,
    ),
    own(
        "est_weight",
        "Est Weight",
        GENERAL,
        70.,
        Kind::Number,
        Own::EstWeight,
    ),
    // Dates.
    rolled("start"),
    rolled("finish"),
    rolled("actual_start"),
    rolled("actual_finish"),
    own(
        "anticipated_start",
        "Anticipated Start",
        acols::DATES,
        100.,
        Kind::Date,
        Own::AnticipatedStart,
    ),
    own(
        "anticipated_finish",
        "Anticipated Finish",
        acols::DATES,
        100.,
        Kind::Date,
        Own::AnticipatedFinish,
    ),
    rolled("remaining_early_start"),
    rolled("remaining_early_finish"),
    // Durations.
    rolled("total_float"),
    // Budget.
    own(
        "original_budget",
        "Original Budget",
        BUDGET,
        100.,
        Kind::Cost,
        Own::OriginalBudget,
    ),
    own(
        "current_budget",
        "Current Budget",
        BUDGET,
        100.,
        Kind::Cost,
        Own::CurrentBudget,
    ),
    own(
        "proposed_budget",
        "Proposed Budget",
        BUDGET,
        100.,
        Kind::Cost,
        Own::ProposedBudget,
    ),
    own(
        "total_spending_plan",
        "Total Spending Plan",
        BUDGET,
        100.,
        Kind::Cost,
        Own::SpendingPlan,
    ),
    own(
        "total_benefit_plan",
        "Total Benefit Plan",
        BUDGET,
        100.,
        Kind::Cost,
        Own::BenefitPlan,
    ),
    own(
        "independent_etc_units",
        "Independent ETC Units",
        BUDGET,
        100.,
        Kind::Units,
        Own::IndependentUnits,
    ),
    own(
        "independent_etc_cost",
        "Independent ETC Cost",
        BUDGET,
        100.,
        Kind::Cost,
        Own::IndependentCost,
    ),
    // Number of Activities.
    rolled("number_of_completed_activities"),
    rolled("number_of_in_progress_activities"),
    rolled("number_of_not_started_activities"),
    // Percent Completes.
    rolled("units_percent_complete"),
    rolled("labor_units_percent_complete"),
    rolled("nonlabor_units_percent_complete"),
    rolled("cost_percent_complete"),
    rolled("schedule_percent_complete"),
    rolled("performance_percent_complete"),
    // Units.
    rolled("actual_labor_units"),
    rolled("actual_nonlabor_units"),
    rolled("actual_this_period_labor_units"),
    rolled("actual_this_period_nonlabor_units"),
    rolled("at_completion_labor_units"),
    rolled("at_completion_nonlabor_units"),
    rolled("remaining_labor_units"),
    rolled("remaining_nonlabor_units"),
    rolled("bl_project_budgeted_labor_units"),
    rolled("bl_project_budgeted_nonlabor_units"),
    rolled("variance_bl_project_labor_units"),
    // Costs.
    rolled("actual_expense_cost"),
    rolled("actual_labor_cost"),
    rolled("actual_material_cost"),
    rolled("actual_nonlabor_cost"),
    rolled("actual_this_period_labor_cost"),
    rolled("actual_this_period_material_cost"),
    rolled("actual_this_period_nonlabor_cost"),
    rolled("actual_total_cost"),
    rolled("at_completion_expense_cost"),
    rolled("at_completion_labor_cost"),
    rolled("at_completion_material_cost"),
    rolled("at_completion_nonlabor_cost"),
    rolled("at_completion_total_cost"),
    rolled("remaining_expense_cost"),
    rolled("remaining_labor_cost"),
    rolled("remaining_material_cost"),
    rolled("remaining_nonlabor_cost"),
    rolled("remaining_total_cost"),
    rolled("bl_project_budgeted_total_cost"),
    rolled("bl_project_budgeted_expense_cost"),
    rolled("variance_bl_project_total_cost"),
    // Earned Value.
    rolled("budget_at_completion"),
    rolled("budget_at_completion_labor_units"),
    rolled("planned_value_cost"),
    rolled("planned_value_labor_units"),
    rolled("earned_value_cost"),
    rolled("earned_value_labor_units"),
    rolled("actual_cost"),
    rolled("schedule_variance"),
    rolled("schedule_variance_labor_units"),
    rolled("cost_variance"),
    rolled("cost_variance_labor_units"),
    rolled("schedule_performance_index"),
    rolled("cost_performance_index"),
    rolled("estimate_to_complete"),
    rolled("estimate_at_completion_cost"),
    rolled("variance_at_completion"),
];

/// The WBS table's columns, with each rolled-up column's title, group,
/// width and kind taken from the activity column it totals.
pub fn catalog() -> &'static [WbsColumn] {
    static CELL: std::sync::OnceLock<Vec<WbsColumn>> = std::sync::OnceLock::new();
    CELL.get_or_init(|| {
        CATALOG
            .iter()
            .map(|c| match (c.source, c.title) {
                (Source::Rolled(id), "") => {
                    let a = activity_column(id);
                    WbsColumn {
                        title: a.map_or(id, |a| leak(&a.title)),
                        group: a.map_or(GENERAL, |a| group_of(&a.group)),
                        width: a.map_or(90., |a| a.width),
                        kind: a.map_or(Kind::Text, |a| a.kind),
                        ..*c
                    }
                }
                _ => *c,
            })
            .collect()
    })
}

/// The activity column `id` of the activity catalog.
pub fn activity_column(id: &str) -> Option<&'static Column> {
    acols::find(acols::builtin(), id)
}

/// A title kept for the life of the program (the catalog is built once).
fn leak(s: &str) -> &'static str {
    Box::leak(s.to_string().into_boxed_str())
}

/// The chooser group of an activity column group.
fn group_of(group: &str) -> &'static str {
    [
        acols::DATES,
        acols::DURATIONS,
        acols::PERCENTS,
        acols::UNITS,
        acols::COSTS,
        EARNED_VALUE,
        NUMBER_OF_ACTIVITIES,
    ]
    .into_iter()
    .find(|g| *g == group)
    .unwrap_or(GENERAL)
}

/// The column titles in field order.
pub fn titles() -> Vec<&'static str> {
    catalog().iter().map(|c| c.title).collect()
}

/// The column widths in field order.
pub fn widths() -> Vec<f32> {
    catalog().iter().map(|c| c.width).collect()
}

/// The cell commands' field ids in field order.
pub fn ids() -> Vec<&'static str> {
    catalog().iter().map(|c| c.id).collect()
}

/// The chooser group of each column, in field order.
pub fn groups() -> Vec<&'static str> {
    catalog().iter().map(|c| c.group).collect()
}

/// The right-aligned fields (numbers, as the activity table aligns them).
pub fn right() -> Vec<usize> {
    catalog()
        .iter()
        .enumerate()
        .filter(|(_, c)| acols::default_align(c.kind) == acols::Align::Right)
        .map(|(i, _)| i)
        .collect()
}

/// The field of column id `id`.
pub fn field_of(id: &str) -> Option<usize> {
    catalog().iter().position(|c| c.id == id)
}

/// The rolled-up value of column `c` over the activities `members` (rows
/// of `p.model.activities`), with the Activities table's band totals.
pub fn rolled_value(
    p: &ProjectData,
    ix: &acols::Index,
    c: &WbsColumn,
    members: &[usize],
) -> Option<Value> {
    let Source::Rolled(id) = c.source else {
        return None;
    };
    let column = activity_column(id)?;
    crate::grid::group::band_total(p, ix, column, members)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_rolled_column_is_an_activity_column_with_a_band_total() {
        let p = super::super::wbs::tests::with_project_node();
        let ix = acols::Index::new((false, 0.));
        for c in catalog() {
            if let Source::Rolled(id) = c.source {
                let Some(column) = activity_column(id) else {
                    panic!("{id} is not an activity column");
                };
                let total = crate::grid::group::band_total(&p, &ix, column, &[]);
                let open = matches!(column.kind, Kind::Date | Kind::Duration | Kind::Percent);
                assert!(total.is_some() || open, "{id} has no band total");
                assert!(!c.title.is_empty() && c.title != id, "{id} has no title");
            }
        }
    }

    #[test]
    fn ids_and_titles_are_unique_and_the_first_fields_keep_their_place() {
        let ids = ids();
        let mut seen = std::collections::BTreeSet::new();
        assert!(ids.iter().all(|id| seen.insert(*id)));
        let titles = titles();
        let mut seen = std::collections::BTreeSet::new();
        assert!(titles.iter().all(|t| seen.insert(*t)), "{titles:?}");
        assert_eq!(&titles[..3], &["WBS Code", "WBS Name", "Total Activities"]);
    }
}
