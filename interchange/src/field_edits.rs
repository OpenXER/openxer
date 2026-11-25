//! Activity field changes a spreadsheet import makes, and the setters that
//! apply them.
//!
//! The import plans a list of [`FieldEdit`]s per row. Who applies them is up to
//! the caller through [`Setters`]: the desktop passes its own activity setters
//! (the ones the table and Activity Details use), so locks, status follow-ups,
//! assignment recalculation and the out-of-sequence check match a typed edit.
//! [`CrateSetters`] hands the same edits straight to the shared edit layer for
//! callers without those setters (the command line and tests).
use crate::columns::Value;
use openxer_engine::model::CstrType;
use openxer_xer::edit::activity::{self, Edit, Measure, Phase};
use openxer_xer::ProjectData;

/// One change to one activity field. Dates are minutes from the project's
/// origin, durations are working minutes, units are hours.
#[derive(Clone, Debug, PartialEq)]
pub enum FieldEdit {
    Wbs(i64),
    Calendar(usize),
    OriginalDuration(i64),
    RemainingDuration(i64),
    ActualDuration(i64),
    ActualStart(Option<i64>),
    ActualFinish(Option<i64>),
    ExpectedFinish(Option<i64>),
    SuspendDate(Option<i64>),
    ResumeDate(Option<i64>),
    PhysicalPercent(f64),
    ActivityPercent(f64),
    DurationPercent(f64),
    Constraint {
        secondary: bool,
        kind: CstrType,
    },
    ConstraintDate {
        secondary: bool,
        date: Option<i64>,
    },
    /// An activity code value of a code type; `None` removes it.
    Code {
        type_id: i64,
        value: Option<i64>,
    },
    /// A user defined field value. A date is absolute minutes since
    /// 1899-12-30, as the sheet gave it.
    Udf {
        type_id: i64,
        value: Value,
    },
    /// Actual (`actual` true) or remaining labor units in hours.
    LaborUnits {
        actual: bool,
        hours: f64,
    },
}

impl FieldEdit {
    /// Order within a row: later edits see the status the earlier ones left,
    /// so a row that starts and finishes an activity, or sets its remaining
    /// duration along with its percent complete, is accepted in one pass.
    pub(crate) fn rank(&self) -> u8 {
        match self {
            FieldEdit::Calendar(_) | FieldEdit::Wbs(_) => 1,
            FieldEdit::ActualStart(_) => 2,
            FieldEdit::OriginalDuration(_) => 3,
            FieldEdit::RemainingDuration(_) => 4,
            FieldEdit::DurationPercent(_)
            | FieldEdit::ActivityPercent(_)
            | FieldEdit::PhysicalPercent(_) => 5,
            FieldEdit::LaborUnits { .. } => 6,
            FieldEdit::ExpectedFinish(_) | FieldEdit::SuspendDate(_) | FieldEdit::ResumeDate(_) => {
                7
            }
            FieldEdit::ActualFinish(_) => 8,
            FieldEdit::ActualDuration(_) => 9,
            FieldEdit::Constraint {
                secondary: false, ..
            } => 10,
            FieldEdit::Constraint {
                secondary: true, ..
            } => 11,
            FieldEdit::ConstraintDate {
                secondary: false, ..
            } => 12,
            FieldEdit::ConstraintDate {
                secondary: true, ..
            } => 13,
            FieldEdit::Code { .. } | FieldEdit::Udf { .. } => 14,
        }
    }
}

/// Applies one field change to activity `activity` of `project`. `Ok(true)`
/// means something changed; on `Err` nothing did and the message says why.
pub trait Setters {
    fn apply(
        &mut self,
        project: &mut ProjectData,
        activity: usize,
        edit: &FieldEdit,
    ) -> Result<bool, String>;
}

/// The activity setters of the shared edit layer
/// (`openxer_xer::edit::activity`), durations in messages in hours: the same
/// locks, checks and follow-ups the activity table, the command line and MCP
/// apply, so a spreadsheet cannot write a combination they refuse.
#[derive(Default)]
pub struct CrateSetters;

impl Setters for CrateSetters {
    fn apply(&mut self, p: &mut ProjectData, i: usize, edit: &FieldEdit) -> Result<bool, String> {
        if i >= p.model.activities.len() {
            return Err("No such activity.".into());
        }
        let shared = match edit {
            FieldEdit::Wbs(id) => Edit::Wbs(*id),
            FieldEdit::Calendar(c) => Edit::Calendar(*c),
            FieldEdit::OriginalDuration(m) => Edit::OriginalDuration(*m),
            FieldEdit::RemainingDuration(m) => Edit::RemainingDuration(*m),
            FieldEdit::ActualDuration(m) => Edit::ActualDuration(*m),
            FieldEdit::ActualStart(v) => Edit::ActualStart(*v),
            FieldEdit::ActualFinish(v) => Edit::ActualFinish(*v),
            FieldEdit::ExpectedFinish(v) => Edit::ExpectedFinish(*v),
            FieldEdit::SuspendDate(v) => Edit::SuspendDate(*v),
            FieldEdit::ResumeDate(v) => Edit::ResumeDate(*v),
            FieldEdit::PhysicalPercent(v) => Edit::PhysicalPercent(*v),
            FieldEdit::ActivityPercent(v) => Edit::ActivityPercent(*v),
            FieldEdit::DurationPercent(v) => Edit::DurationPercent(*v),
            FieldEdit::Constraint { secondary, kind } => Edit::Constraint {
                secondary: *secondary,
                kind: *kind,
            },
            FieldEdit::ConstraintDate { secondary, date } => Edit::ConstraintDate {
                secondary: *secondary,
                date: *date,
            },
            FieldEdit::Code { type_id, value } => Edit::Code {
                type_id: *type_id,
                value: *value,
            },
            FieldEdit::LaborUnits { actual, hours } => Edit::Figure {
                measure: Measure::LaborUnits,
                phase: if *actual {
                    Phase::Actual
                } else {
                    Phase::Remaining
                },
                value: *hours,
            },
            FieldEdit::Udf { type_id, value } => {
                // The sheet's cell value as the field's type takes it.
                let task_id = p.model.activities[i].task_id;
                let pid = p.model.project.proj_id;
                crate::xlsx_import::set_udf(p, task_id, pid, *type_id, value);
                return Ok(true);
            }
        };
        activity::edit_activity(p, i, shared).map(|out| out.changed)
    }
}
