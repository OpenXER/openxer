//! Activity field setters shared by Activity Details and the activity table's
//! in-place editors (and the commands built on them, such as dropping
//! activities on a band). Each change is an `Edit`. The rules themselves
//! (locks, status follow-ups, Original and Remaining Duration staying equal
//! until the activity starts, resource assignments following duration
//! changes by the duration type) live in `openxer_xer::edit::activity`, which
//! the command line and MCP edit commands call too. `App::edit_activity`
//! adds what only the app does: detail-table writes bump the undo revision,
//! the window is marked dirty, and a progress change that leaves a
//! predecessor out of sequence raises the out-of-sequence prompt.
use super::*;
use crate::grid::columns::{Field, Phase};
use crate::schedule_dialogs::StatusRevert;
use openxer_xer::edit::activity::{self as shared, Lock};

pub use shared::{Edit, UdfValue, STATUSES};

/// The shared lock a table field is checked against (the shared
/// catalog's; the table's own columns are never locked here).
fn lock_of(field: Field) -> Lock {
    field
        .shared()
        .map_or(Lock::Open, openxer_xer::edit::fields::lock_of)
}

/// Durations in messages as the user's preferences show them.
fn duration_text(p: &ProjectData, i: usize, minutes: i64) -> String {
    format::duration(minutes, format::activity_periods(p, i))
}

impl App {
    /// The actual start that checking Started records, and the date its
    /// unchecked box shows: the planned start (early start when there is none).
    pub(crate) fn default_actual_start(&self, i: usize) -> i64 {
        shared::default_actual_start(&self.project, i)
    }

    /// The actual finish that checking Finished records, and the date its
    /// unchecked box shows: the planned finish (early finish when there is
    /// none), never before the actual start.
    pub(crate) fn default_actual_finish(&self, i: usize) -> i64 {
        shared::default_actual_finish(&self.project, i)
    }

    /// Why field `field` of activity `i` cannot change now, if it cannot.
    pub(crate) fn field_lock(&self, i: usize, field: Field) -> Result<(), String> {
        shared::field_lock(&self.project, i, lock_of(field))
    }

    /// Apply one change to activity `i` with the rules Activity Details and
    /// the table share. Returns Ok(true) when something changed; on `Err`
    /// nothing changed.
    pub(crate) fn edit_activity(&mut self, i: usize, edit: Edit) -> Result<bool, String> {
        self.unpin_serial();
        if i >= self.project.model.activities.len() {
            return Err("No such activity.".into());
        }
        // A unit or cost figure typed as it already shows is not a change.
        let figure = match &edit {
            Edit::Figure {
                measure,
                phase,
                value,
            } => Some((*measure, Some(*phase), *value)),
            Edit::ThisPeriod { measure, value } => Some((*measure, None, *value)),
            _ => None,
        };
        if let Some((measure, phase, value)) = figure {
            shared::field_lock(&self.project, i, edit.lock())?;
            if (0. ..1.0e12).contains(&value)
                && (self.figure_now(i, measure, phase) - value).abs() < 1e-9
            {
                return Ok(false);
            }
        }
        let a = &self.project.model.activities[i];
        let before = StatusRevert {
            status: a.status,
            act_start: a.act_start,
            act_finish: a.act_finish,
            rem: a.rem,
        };
        let outcome = shared::edit_activity_with(&mut self.project, i, edit, duration_text)?;
        if outcome.records_changed {
            self.touch_records();
        }
        if let Some(warning) = outcome.warning {
            self.status = warning;
        }
        if outcome.changed {
            self.dirty = true;
            self.date_edits.clear();
        }
        if outcome.progress_changed {
            self.check_status_sequence(i, before);
        }
        Ok(outcome.changed)
    }

    /// The figure as the activity shows it now (hours or currency).
    fn figure_now(
        &self,
        i: usize,
        measure: crate::grid::columns::Measure,
        phase: Option<Phase>,
    ) -> f64 {
        let ix = self.grid_index();
        let task_id = self.project.model.activities[i].task_id;
        match phase {
            None => ix.this_period(&self.project, task_id, measure),
            Some(phase) => {
                ix.figures(&self.project, task_id, measure)
                    .map_or(0., |f| match phase {
                        Phase::Budgeted => f.budget,
                        Phase::Actual => f.actual,
                        Phase::Remaining => f.remaining,
                        Phase::AtCompletion => f.actual + f.remaining,
                    })
            }
        }
    }
}
