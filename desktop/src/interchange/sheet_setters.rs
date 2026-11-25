//! Spreadsheet import through the activity setters.
//!
//! The import plans typed field edits per row; this adapter hands each one to
//! `App::edit_activity`, the same function the activity table and Activity
//! Details use, so locks, status follow-ups, assignment recalculation and the
//! out-of-sequence check are the ones a typed edit gets. The import works on a
//! copy of the project: each call swaps the copy into the application for the
//! length of one edit.
use super::*;
use crate::details::record_tabs::{udf_kind, UdfKind};
use crate::details::setters::{Edit, UdfValue};
use crate::grid::columns::{Measure, Phase};
use crate::schedule_dialogs::SequencePrompt;
use openxer_interchange::columns::Value as CellValue;
use openxer_interchange::field_edits::{FieldEdit, Setters};

/// An activity the import left out of sequence: it has progress while a
/// predecessor has not made the progress its relationship needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OutOfSequence {
    pub activity: String,
    pub predecessor: String,
}

impl OutOfSequence {
    pub(crate) fn line(&self) -> String {
        format!(
            "{} has progress ahead of its predecessor {}.",
            self.activity, self.predecessor
        )
    }
}

/// Application state an import preview must not leave changed.
pub(crate) struct Saved {
    serial: u64,
    revision: u64,
    dirty: bool,
    status: String,
    date_edits: BTreeMap<String, String>,
    sequence: Option<SequencePrompt>,
    quiet_draft: bool,
}

impl App {
    pub(crate) fn save_edit_state(&mut self) -> Saved {
        Saved {
            serial: self.records_serial,
            revision: self.records_revision,
            dirty: self.dirty,
            status: self.status.clone(),
            date_edits: self.date_edits.clone(),
            sequence: self.sched.sequence.take(),
            quiet_draft: self.sched.sequence_quiet_draft,
        }
    }

    /// Put back the state `save_edit_state` took.
    pub(crate) fn restore_edit_state(&mut self, saved: Saved) {
        self.records_serial = saved.serial;
        self.records_revision = saved.revision;
        self.dirty = saved.dirty;
        self.status = saved.status;
        self.date_edits = saved.date_edits;
        self.sched.sequence = saved.sequence;
        self.sched.sequence_quiet_draft = saved.quiet_draft;
    }

    /// The setter edit for a planned field edit, read against the project
    /// currently in the application.
    fn setter_edit(&self, edit: &FieldEdit) -> Result<Edit, String> {
        Ok(match edit {
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
                let logical = self
                    .project
                    .document
                    .table("UDFTYPE")
                    .and_then(|t| {
                        t.rows
                            .iter()
                            .find(|r| records::num(t.value(r, "udf_type_id")) == *type_id)
                            .map(|r| t.value(r, "logical_data_type").to_string())
                    })
                    .ok_or("No such user defined field.")?;
                let origin = self.project.model.origin_serial * DAY;
                let value = match (udf_kind(&logical), value) {
                    (UdfKind::Date, CellValue::Date(m)) => UdfValue::Date(Some(m - origin)),
                    (UdfKind::Date, CellValue::Empty) => UdfValue::Date(None),
                    (UdfKind::Number, CellValue::Number(n)) => UdfValue::Number(Some(*n)),
                    (UdfKind::Number, CellValue::Empty) => UdfValue::Number(None),
                    (UdfKind::Text | UdfKind::Indicator, CellValue::Text(t)) => {
                        UdfValue::Text(t.clone())
                    }
                    (UdfKind::Text | UdfKind::Indicator, CellValue::Empty) => {
                        UdfValue::Text(String::new())
                    }
                    (UdfKind::ReadOnly, _) => {
                        return Err("This user defined field is not editable here.".into())
                    }
                    _ => return Err("The value does not match the field's type.".into()),
                };
                Edit::Udf {
                    type_id: *type_id,
                    value,
                }
            }
        })
    }
}

/// The activity setters as the import's [`Setters`], collecting the
/// out-of-sequence prompts the edits raise.
pub(crate) struct AppSetters<'a> {
    pub app: &'a mut App,
    pub out_of_sequence: Vec<OutOfSequence>,
}

impl<'a> AppSetters<'a> {
    pub(crate) fn new(app: &'a mut App) -> Self {
        Self {
            app,
            out_of_sequence: vec![],
        }
    }

    fn note_prompt(&mut self) {
        let Some(prompt) = self.app.sched.sequence.take() else {
            return;
        };
        let m = &self.app.project.model;
        let (Some(a), Some(link)) = (m.activities.get(prompt.activity), m.links.get(prompt.link))
        else {
            return;
        };
        let entry = OutOfSequence {
            activity: a.task_code.clone(),
            predecessor: m
                .activities
                .get(link.pred)
                .map(|p| p.task_code.clone())
                .unwrap_or_default(),
        };
        if !self.out_of_sequence.contains(&entry) {
            self.out_of_sequence.push(entry);
        }
    }
}

impl Setters for AppSetters<'_> {
    fn apply(
        &mut self,
        project: &mut ProjectData,
        activity: usize,
        edit: &FieldEdit,
    ) -> Result<bool, String> {
        std::mem::swap(&mut self.app.project, project);
        let result = self
            .app
            .setter_edit(edit)
            .and_then(|e| self.app.edit_activity(activity, e));
        self.note_prompt();
        std::mem::swap(&mut self.app.project, project);
        result
    }
}
