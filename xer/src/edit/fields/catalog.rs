//! Which activity fields exist: the built-ins, then one per activity code
//! type and one per activity user defined field, each with its stable id,
//! title, kind and how it is edited.
use super::Kind;
use crate::edit::activity::{Measure, Phase};
use crate::edit::records::num;
use crate::edit::udf::{udf_kind, UdfKind};
use crate::ProjectData;
use std::collections::BTreeSet;
use std::sync::OnceLock;

/// How a field is edited in place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Editor {
    ReadOnly,
    Text,
    Choice,
    Date,
    Number,
}

/// Built-in activity fields of the shared catalog.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Field {
    ActivityId,
    ActivityName,
    ActivityType,
    ActivityStatus,
    Wbs,
    WbsName,
    Calendar,
    PrimaryResource,
    DurationType,
    PercentType,
    /// The activity's Auto Compute Actuals flag.
    AutoComputeActuals,
    Critical,
    LongestPath,
    Start,
    Finish,
    EarlyStart,
    EarlyFinish,
    LateStart,
    LateFinish,
    ActualStart,
    ActualFinish,
    PlannedStart,
    PlannedFinish,
    RemainingEarlyStart,
    RemainingEarlyFinish,
    RemainingLateStart,
    RemainingLateFinish,
    ExpectedFinish,
    PrimaryConstraint,
    PrimaryConstraintDate,
    SecondaryConstraint,
    SecondaryConstraintDate,
    SuspendDate,
    ResumeDate,
    OriginalDuration,
    ActualDuration,
    RemainingDuration,
    AtCompletionDuration,
    DurationPercent,
    TotalFloat,
    FreeFloat,
    FloatPath,
    FloatPathOrder,
    ActivityPercent,
    PhysicalPercent,
    Predecessors,
    Successors,
    Figure(Measure, Phase),
    /// Actual units or cost this period (`Measure::editable` ones only).
    ThisPeriod(Measure),
}

/// Where a field's value comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Field(Field),
    /// Activity code type (`actv_code_type_id`).
    Code(i64),
    /// Activity user defined field (`udf_type_id`).
    Udf(i64),
}

/// One activity field.
#[derive(Clone, Debug, PartialEq)]
pub struct FieldDef {
    /// Stable id (`activity_id`, `code:Area`, `udf:Crew`), as layouts and
    /// stored global changes name it.
    pub id: String,
    /// The title as a column header shows it.
    pub title: String,
    pub kind: Kind,
    pub editor: Editor,
    pub source: Source,
}

fn def(field: Field, id: &str, title: &str, kind: Kind, editor: Editor) -> FieldDef {
    FieldDef {
        id: id.into(),
        title: title.into(),
        kind,
        editor,
        source: Source::Field(field),
    }
}

/// The built-in fields, in the order the activity table lists them.
pub fn builtin() -> &'static [FieldDef] {
    static FIELDS: OnceLock<Vec<FieldDef>> = OnceLock::new();
    FIELDS.get_or_init(|| {
        use Editor as E;
        use Field as F;
        use Kind as K;
        let mut c = vec![
            def(
                F::ActivityId,
                "activity_id",
                "Activity ID",
                K::Text,
                E::Text,
            ),
            def(
                F::ActivityName,
                "activity_name",
                "Activity Name",
                K::Text,
                E::Text,
            ),
            def(
                F::ActivityType,
                "activity_type",
                "Activity Type",
                K::Text,
                E::Choice,
            ),
            def(
                F::ActivityStatus,
                "activity_status",
                "Activity Status",
                K::Text,
                E::Choice,
            ),
            def(F::Wbs, "wbs", "WBS", K::Text, E::Choice),
            def(F::WbsName, "wbs_name", "WBS Name", K::Text, E::ReadOnly),
            def(F::Calendar, "calendar", "Calendar", K::Text, E::Choice),
            def(
                F::PrimaryResource,
                "primary_resource",
                "Primary Resource",
                K::Text,
                E::Choice,
            ),
            def(
                F::DurationType,
                "duration_type",
                "Duration Type",
                K::Text,
                E::Choice,
            ),
            def(
                F::PercentType,
                "percent_complete_type",
                "Percent Complete Type",
                K::Text,
                E::Choice,
            ),
            def(
                F::AutoComputeActuals,
                "auto_compute_actuals",
                "Auto Compute Actuals",
                K::Flag,
                E::Choice,
            ),
            def(F::Critical, "critical", "Critical", K::Flag, E::ReadOnly),
            def(
                F::LongestPath,
                "longest_path",
                "Longest Path",
                K::Flag,
                E::ReadOnly,
            ),
            def(F::Start, "start", "Start", K::Date, E::Date),
            def(F::Finish, "finish", "Finish", K::Date, E::Date),
            def(
                F::EarlyStart,
                "early_start",
                "Early Start",
                K::Date,
                E::ReadOnly,
            ),
            def(
                F::EarlyFinish,
                "early_finish",
                "Early Finish",
                K::Date,
                E::ReadOnly,
            ),
            def(
                F::LateStart,
                "late_start",
                "Late Start",
                K::Date,
                E::ReadOnly,
            ),
            def(
                F::LateFinish,
                "late_finish",
                "Late Finish",
                K::Date,
                E::ReadOnly,
            ),
            def(
                F::ActualStart,
                "actual_start",
                "Actual Start",
                K::Date,
                E::Date,
            ),
            def(
                F::ActualFinish,
                "actual_finish",
                "Actual Finish",
                K::Date,
                E::Date,
            ),
            def(
                F::PlannedStart,
                "planned_start",
                "Planned Start",
                K::Date,
                E::Date,
            ),
            def(
                F::PlannedFinish,
                "planned_finish",
                "Planned Finish",
                K::Date,
                E::Date,
            ),
            def(
                F::RemainingEarlyStart,
                "remaining_early_start",
                "Remaining Early Start",
                K::Date,
                E::ReadOnly,
            ),
            def(
                F::RemainingEarlyFinish,
                "remaining_early_finish",
                "Remaining Early Finish",
                K::Date,
                E::ReadOnly,
            ),
            def(
                F::RemainingLateStart,
                "remaining_late_start",
                "Remaining Late Start",
                K::Date,
                E::ReadOnly,
            ),
            def(
                F::RemainingLateFinish,
                "remaining_late_finish",
                "Remaining Late Finish",
                K::Date,
                E::ReadOnly,
            ),
            def(
                F::ExpectedFinish,
                "expected_finish",
                "Expected Finish",
                K::Date,
                E::Date,
            ),
            def(
                F::PrimaryConstraint,
                "primary_constraint",
                "Primary Constraint",
                K::Text,
                E::Choice,
            ),
            def(
                F::PrimaryConstraintDate,
                "primary_constraint_date",
                "Primary Constraint Date",
                K::Date,
                E::Date,
            ),
            def(
                F::SecondaryConstraint,
                "secondary_constraint",
                "Secondary Constraint",
                K::Text,
                E::Choice,
            ),
            def(
                F::SecondaryConstraintDate,
                "secondary_constraint_date",
                "Secondary Constraint Date",
                K::Date,
                E::Date,
            ),
            def(
                F::SuspendDate,
                "suspend_date",
                "Suspend Date",
                K::Date,
                E::Date,
            ),
            def(
                F::ResumeDate,
                "resume_date",
                "Resume Date",
                K::Date,
                E::Date,
            ),
            def(
                F::OriginalDuration,
                "original_duration",
                "Original Duration",
                K::Duration,
                E::Number,
            ),
            def(
                F::ActualDuration,
                "actual_duration",
                "Actual Duration",
                K::Duration,
                E::Number,
            ),
            def(
                F::RemainingDuration,
                "remaining_duration",
                "Remaining Duration",
                K::Duration,
                E::Number,
            ),
            def(
                F::AtCompletionDuration,
                "at_completion_duration",
                "At Completion Duration",
                K::Duration,
                E::Number,
            ),
            def(
                F::TotalFloat,
                "total_float",
                "Total Float",
                K::Duration,
                E::ReadOnly,
            ),
            def(
                F::FreeFloat,
                "free_float",
                "Free Float",
                K::Duration,
                E::ReadOnly,
            ),
            def(
                F::FloatPath,
                "float_path",
                "Float Path",
                K::Number,
                E::ReadOnly,
            ),
            def(
                F::FloatPathOrder,
                "float_path_order",
                "Float Path Order",
                K::Number,
                E::ReadOnly,
            ),
            def(
                F::ActivityPercent,
                "activity_percent_complete",
                "Activity % Complete",
                K::Percent,
                E::Number,
            ),
            def(
                F::DurationPercent,
                "duration_percent_complete",
                "Duration % Complete",
                K::Percent,
                E::Number,
            ),
            def(
                F::PhysicalPercent,
                "physical_percent_complete",
                "Physical % Complete",
                K::Percent,
                E::Number,
            ),
            def(
                F::Predecessors,
                "predecessors",
                "Predecessors",
                K::Text,
                E::ReadOnly,
            ),
            def(
                F::Successors,
                "successors",
                "Successors",
                K::Text,
                E::ReadOnly,
            ),
        ];
        for measure in Measure::ALL {
            for phase in Phase::ALL {
                let kind = if measure.is_cost() { K::Cost } else { K::Units };
                // Budgeted, actual and remaining figures are entered on the
                // activity; at completion is their sum.
                let editor = if measure.editable() && phase != Phase::AtCompletion {
                    E::Number
                } else {
                    E::ReadOnly
                };
                c.push(def(
                    F::Figure(measure, phase),
                    &format!("{}_{}", phase.key(), measure.key()),
                    &format!("{} {}", phase.caption(), measure.caption()),
                    kind,
                    editor,
                ));
            }
        }
        for measure in Measure::ALL.into_iter().filter(|m| m.editable()) {
            let kind = if measure.is_cost() { K::Cost } else { K::Units };
            c.push(def(
                F::ThisPeriod(measure),
                &format!("actual_this_period_{}", measure.key()),
                &format!("Actual This Period {}", measure.caption()),
                kind,
                E::Number,
            ));
        }
        c
    })
}

/// Code types visible to the project: global, EPS and this project's own,
/// by their sequence.
pub fn code_types(p: &ProjectData) -> Vec<(i64, String)> {
    let Some(t) = p.document.table("ACTVTYPE") else {
        return vec![];
    };
    let pid = p.model.project.proj_id;
    let mut types: Vec<(i64, i64, String)> = t
        .rows
        .iter()
        .filter(|r| match t.value(r, "actv_code_type_scope") {
            "AS_Global" | "AS_EPS" => true,
            "AS_Project" => num(t.value(r, "proj_id")) == pid,
            _ => t.value(r, "proj_id").is_empty() || num(t.value(r, "proj_id")) == pid,
        })
        .map(|r| {
            (
                num(t.value(r, "seq_num")),
                num(t.value(r, "actv_code_type_id")),
                t.value(r, "actv_code_type").to_string(),
            )
        })
        .collect();
    types.sort();
    types.into_iter().map(|(_, id, name)| (id, name)).collect()
}

/// One activity user defined field: its id, label (the name when it has
/// none), what it holds and how it is edited (code-list fields are read
/// only, as on the details tab).
#[derive(Clone, Debug, PartialEq)]
pub struct UdfType {
    pub id: i64,
    pub label: String,
    pub kind: Kind,
    pub editor: Editor,
}

/// The project's activity user defined fields.
pub fn udf_types(p: &ProjectData) -> Vec<UdfType> {
    let Some(t) = p.document.table("UDFTYPE") else {
        return vec![];
    };
    t.rows
        .iter()
        .filter(|r| t.value(r, "table_name") == "TASK")
        .map(|r| {
            let label = t.value(r, "udf_type_label");
            let label = if label.is_empty() {
                t.value(r, "udf_type_name")
            } else {
                label
            };
            let logical = t.value(r, "logical_data_type");
            let kind = match logical {
                "FT_INT" | "FT_FLOAT_2_DECIMALS" | "FT_FLOAT" => Kind::Number,
                "FT_MONEY" => Kind::Cost,
                "FT_START_DATE" | "FT_END_DATE" | "FT_DATE" => Kind::Date,
                _ => Kind::Text,
            };
            let editor = match udf_kind(logical) {
                UdfKind::Text => Editor::Text,
                UdfKind::Number => Editor::Number,
                UdfKind::Date => Editor::Date,
                UdfKind::Indicator => Editor::Choice,
                UdfKind::ReadOnly => Editor::ReadOnly,
            };
            UdfType {
                id: num(t.value(r, "udf_type_id")),
                label: label.to_string(),
                kind,
                editor,
            }
        })
        .collect()
}

/// Every field of this project: built-ins, then one per activity code type
/// and one per activity user defined field. An id met twice keeps the first.
pub fn catalog(p: &ProjectData) -> Vec<FieldDef> {
    let mut out = builtin().to_vec();
    let mut seen: BTreeSet<String> = out.iter().map(|c| c.id.clone()).collect();
    let mut push = |c: FieldDef| {
        if seen.insert(c.id.clone()) {
            out.push(c);
        }
    };
    for (id, name) in code_types(p) {
        push(FieldDef {
            id: format!("code:{name}"),
            title: name,
            kind: Kind::Text,
            editor: Editor::Choice,
            source: Source::Code(id),
        });
    }
    for u in udf_types(p) {
        push(FieldDef {
            id: format!("udf:{}", u.label),
            title: u.label,
            kind: u.kind,
            editor: u.editor,
            source: Source::Udf(u.id),
        });
    }
    out
}

/// A field by its id, or by its title ignoring case.
pub fn find<'a>(catalog: &'a [FieldDef], text: &str) -> Option<&'a FieldDef> {
    let t = text.trim();
    catalog
        .iter()
        .find(|c| c.id == t)
        .or_else(|| catalog.iter().find(|c| c.title.eq_ignore_ascii_case(t)))
}
