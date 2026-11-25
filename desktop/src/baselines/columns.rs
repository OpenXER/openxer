//! Baseline columns of the activity table: BL Project and BL1 to BL3 dates,
//! baseline durations, budgeted units and costs, and the variances against
//! them, in the user's duration, date, unit and currency formats. Each sits in
//! the group of its kind (Dates, Durations, Units, Costs).
use super::data::{BlField, BlMeasure, Compare, Slot};
use crate::grid::columns::{self, Column, Editor, Index, Kind, Measure, Source, Value};
use openxer_engine::model::Activity;
use openxer_xer::ProjectData;

/// (id, title, group, slot, measure), in Available Options order.
const COLUMNS: [(&str, &str, &str, Slot, BlMeasure); 24] = [
    (
        "bl_project_start",
        "BL Project Start",
        columns::DATES,
        Slot::Project,
        BlMeasure::Start,
    ),
    (
        "bl_project_finish",
        "BL Project Finish",
        columns::DATES,
        Slot::Project,
        BlMeasure::Finish,
    ),
    (
        "bl1_start",
        "BL1 Start",
        columns::DATES,
        Slot::Primary,
        BlMeasure::Start,
    ),
    (
        "bl1_finish",
        "BL1 Finish",
        columns::DATES,
        Slot::Primary,
        BlMeasure::Finish,
    ),
    (
        "bl2_start",
        "BL2 Start",
        columns::DATES,
        Slot::Secondary,
        BlMeasure::Start,
    ),
    (
        "bl2_finish",
        "BL2 Finish",
        columns::DATES,
        Slot::Secondary,
        BlMeasure::Finish,
    ),
    (
        "bl3_start",
        "BL3 Start",
        columns::DATES,
        Slot::Tertiary,
        BlMeasure::Start,
    ),
    (
        "bl3_finish",
        "BL3 Finish",
        columns::DATES,
        Slot::Tertiary,
        BlMeasure::Finish,
    ),
    (
        "variance_bl_project_start",
        "Variance - BL Project Start Date",
        columns::DATES,
        Slot::Project,
        BlMeasure::StartVariance,
    ),
    (
        "variance_bl_project_finish",
        "Variance - BL Project Finish Date",
        columns::DATES,
        Slot::Project,
        BlMeasure::FinishVariance,
    ),
    (
        "variance_bl1_start",
        "Variance - BL1 Start Date",
        columns::DATES,
        Slot::Primary,
        BlMeasure::StartVariance,
    ),
    (
        "variance_bl1_finish",
        "Variance - BL1 Finish Date",
        columns::DATES,
        Slot::Primary,
        BlMeasure::FinishVariance,
    ),
    (
        "bl_project_duration",
        "BL Project Duration",
        columns::DURATIONS,
        Slot::Project,
        BlMeasure::Duration,
    ),
    (
        "bl1_duration",
        "BL1 Duration",
        columns::DURATIONS,
        Slot::Primary,
        BlMeasure::Duration,
    ),
    (
        "variance_bl_project_duration",
        "Variance - BL Project Duration",
        columns::DURATIONS,
        Slot::Project,
        BlMeasure::DurationVariance,
    ),
    (
        "variance_bl1_duration",
        "Variance - BL1 Duration",
        columns::DURATIONS,
        Slot::Primary,
        BlMeasure::DurationVariance,
    ),
    (
        "bl2_duration",
        "BL2 Duration",
        columns::DURATIONS,
        Slot::Secondary,
        BlMeasure::Duration,
    ),
    (
        "bl3_duration",
        "BL3 Duration",
        columns::DURATIONS,
        Slot::Tertiary,
        BlMeasure::Duration,
    ),
    (
        "variance_bl2_duration",
        "Variance - BL2 Duration",
        columns::DURATIONS,
        Slot::Secondary,
        BlMeasure::DurationVariance,
    ),
    (
        "variance_bl3_duration",
        "Variance - BL3 Duration",
        columns::DURATIONS,
        Slot::Tertiary,
        BlMeasure::DurationVariance,
    ),
    (
        "bl_project_remaining_duration",
        "BL Project Remaining Duration",
        columns::DURATIONS,
        Slot::Project,
        BlMeasure::RemainingDuration,
    ),
    (
        "bl1_remaining_duration",
        "BL1 Remaining Duration",
        columns::DURATIONS,
        Slot::Primary,
        BlMeasure::RemainingDuration,
    ),
    (
        "bl2_remaining_duration",
        "BL2 Remaining Duration",
        columns::DURATIONS,
        Slot::Secondary,
        BlMeasure::RemainingDuration,
    ),
    (
        "bl3_remaining_duration",
        "BL3 Remaining Duration",
        columns::DURATIONS,
        Slot::Tertiary,
        BlMeasure::RemainingDuration,
    ),
];

/// The column id and caption prefix of a baseline slot.
fn slot_names(slot: Slot) -> (&'static str, &'static str) {
    match slot {
        Slot::Project => ("bl_project", "BL Project"),
        Slot::Primary => ("bl1", "BL1"),
        Slot::Secondary => ("bl2", "BL2"),
        Slot::Tertiary => ("bl3", "BL3"),
    }
}

fn column(id: String, title: String, group: &str, slot: Slot, measure: BlMeasure) -> Column {
    let (width, kind) = match measure {
        BlMeasure::Start | BlMeasure::Finish => (95., Kind::Date),
        BlMeasure::LaborUnits | BlMeasure::NonlaborUnits | BlMeasure::LaborUnitsVariance => {
            (80., Kind::Units)
        }
        BlMeasure::TotalCost | BlMeasure::ExpenseCost | BlMeasure::TotalCostVariance => {
            (80., Kind::Cost)
        }
        _ => (80., Kind::Duration),
    };
    Column {
        id,
        title,
        group: group.into(),
        width,
        align: columns::default_align(kind),
        kind,
        editor: Editor::ReadOnly,
        source: Source::Field(columns::Field::Baseline(BlField { slot, measure })),
    }
}

/// The baseline columns for the column registry.
pub fn columns() -> Vec<Column> {
    let mut out: Vec<Column> = COLUMNS
        .iter()
        .map(|&(id, title, group, slot, measure)| {
            column(id.into(), title.into(), group, slot, measure)
        })
        .collect();
    // Budgeted units and costs of each baseline.
    for slot in Slot::ALL {
        let (key, name) = slot_names(slot);
        for (suffix, caption, group, measure) in [
            (
                "budgeted_labor_units",
                "Budgeted Labor Units",
                columns::UNITS,
                BlMeasure::LaborUnits,
            ),
            (
                "budgeted_nonlabor_units",
                "Budgeted Nonlabor Units",
                columns::UNITS,
                BlMeasure::NonlaborUnits,
            ),
            (
                "budgeted_total_cost",
                "Budgeted Total Cost",
                columns::COSTS,
                BlMeasure::TotalCost,
            ),
            (
                "budgeted_expense_cost",
                "Budgeted Expense Cost",
                columns::COSTS,
                BlMeasure::ExpenseCost,
            ),
        ] {
            out.push(column(
                format!("{key}_{suffix}"),
                format!("{name} {caption}"),
                group,
                slot,
                measure,
            ));
        }
    }
    // Variances in units and cost against the project and primary baselines.
    for slot in [Slot::Project, Slot::Primary] {
        let (key, name) = slot_names(slot);
        out.push(column(
            format!("variance_{key}_labor_units"),
            format!("Variance - {name} Labor Units"),
            columns::UNITS,
            slot,
            BlMeasure::LaborUnitsVariance,
        ));
        out.push(column(
            format!("variance_{key}_total_cost"),
            format!("Variance - {name} Total Cost"),
            columns::COSTS,
            slot,
            BlMeasure::TotalCostVariance,
        ));
    }
    // Date variances against the secondary and tertiary baselines.
    for slot in [Slot::Secondary, Slot::Tertiary] {
        let (key, name) = slot_names(slot);
        out.push(column(
            format!("variance_{key}_start"),
            format!("Variance - {name} Start Date"),
            columns::DATES,
            slot,
            BlMeasure::StartVariance,
        ));
        out.push(column(
            format!("variance_{key}_finish"),
            format!("Variance - {name} Finish Date"),
            columns::DATES,
            slot,
            BlMeasure::FinishVariance,
        ));
    }
    out
}

/// Working minutes from `from` to `to` on the activity's calendar, negative
/// when `to` is earlier.
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

/// The activity's at completion duration: actual plus remaining.
fn at_completion(p: &ProjectData, a: &Activity) -> i64 {
    let finish = a.act_finish.or(p.model.project.data_date);
    let actual = match (a.act_start, finish, p.model.calendars.get(a.cal)) {
        (Some(s), Some(f), Some(cal)) if f > s => cal.between(s, f).max(0),
        _ => 0,
    };
    actual + a.rem
}

/// At completion units or cost of an activity: actual plus remaining.
fn at_completion_of(p: &ProjectData, ix: &Index, a: &Activity, measure: Measure) -> f64 {
    ix.figures(p, a.task_id, measure)
        .map_or(0., |f| f.actual + f.remaining)
}

/// The value of a baseline column for one activity.
pub fn column_value(
    p: &ProjectData,
    ix: &Index,
    compare: Option<&Compare>,
    a: &Activity,
    f: BlField,
) -> Value {
    let Some(bl) = compare.and_then(|c| c.values(p, ix, a, f.slot)) else {
        return Value::Empty;
    };
    let leveled = p.model.options.do_level;
    let date = |m: Option<i64>| {
        m.map_or(Value::Empty, |minute| Value::Date {
            minute,
            mark: crate::format::DateMark::Plain,
        })
    };
    match f.measure {
        BlMeasure::Start => date(bl.start),
        BlMeasure::Finish => date(bl.finish),
        BlMeasure::Duration => Value::Duration(bl.duration),
        BlMeasure::RemainingDuration => Value::Duration(bl.remaining),
        BlMeasure::StartVariance => match (bl.start, crate::util::displayed_start(a, leveled)) {
            (Some(b), Some(s)) => Value::Duration(signed_work(p, a, s, b)),
            _ => Value::Empty,
        },
        BlMeasure::FinishVariance => match (bl.finish, crate::util::displayed_finish(a, leveled)) {
            (Some(b), Some(s)) => Value::Duration(signed_work(p, a, s, b)),
            _ => Value::Empty,
        },
        BlMeasure::DurationVariance => Value::Duration(bl.duration - at_completion(p, a)),
        BlMeasure::LaborUnits => Value::Units(bl.budget_labor_units),
        BlMeasure::NonlaborUnits => Value::Units(bl.budget_nonlabor_units),
        BlMeasure::TotalCost => Value::Cost(bl.budget_cost),
        BlMeasure::ExpenseCost => Value::Cost(bl.budget_expense_cost),
        // Variances read baseline minus current, like the date and
        // duration variances: positive means under the baseline.
        BlMeasure::LaborUnitsVariance => {
            Value::Units(bl.budget_labor_units - at_completion_of(p, ix, a, Measure::LaborUnits))
        }
        BlMeasure::TotalCostVariance => {
            Value::Cost(bl.budget_cost - at_completion_of(p, ix, a, Measure::TotalCost))
        }
    }
}
