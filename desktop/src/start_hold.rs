//! A Start typed for an activity that has not started, in the activity
//! table's Start cell or on the Status tab, or a bar of one dropped in the
//! Gantt chart (at the time of day it was dropped), asks whether to hold the
//! activity on that date with a Start On or After constraint. Yes moves the
//! Start and Planned Start there with the constraint (`Edit::Start`); No
//! moves them without one (`Edit::StartUnheld`), so the next schedule may
//! move the activity back. Either way the finish follows by the duration,
//! in one undo step. Ticking "Always use this answer" keeps the answer and
//! stops the question; User Preferences, Application, brings it back.
//!
//! A typed Finish never asks: the shared setter keeps the start and fits
//! the durations. Fill Down, pasted cells, Global Change, the command line
//! and MCP set the date as given, without asking.
use super::*;
use crate::details::setters::Edit;
use crate::dialog_kit::{Entry, FACE};

/// Workspace preference holding [`Prefs`].
pub const PREFS_KEY: &str = "start_hold_v1";

/// The saved answer.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Prefs {
    /// Ask each time a Start is typed.
    pub ask: bool,
    /// The answer used while not asking: true adds the constraint.
    pub hold: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            ask: true,
            hold: true,
        }
    }
}

/// A typed Start waiting for its answer.
#[derive(Clone, Debug, PartialEq)]
pub struct Prompt {
    pub task_id: i64,
    /// The typed date (minutes from the project's origin).
    pub date: i64,
    /// "Always use this answer" is ticked.
    pub keep: bool,
}

impl App {
    pub(crate) fn restore_start_hold(&mut self) {
        if let Ok(Some(text)) = self.store.preference(PREFS_KEY) {
            match serde_json::from_str::<Prefs>(&text) {
                Ok(p) => self.sched.start_hold_prefs = p,
                Err(e) => self.status = format!("Saved start date answer ignored: {e}"),
            }
        }
    }

    /// Ask again (User Preferences), or stop asking and use `hold`.
    pub(crate) fn set_start_hold(&mut self, prefs: Prefs) {
        if self.sched.start_hold_prefs == prefs {
            return;
        }
        self.sched.start_hold_prefs = prefs;
        let text = serde_json::to_string(&prefs).unwrap_or_default();
        if let Err(e) = self.store.set_preference(PREFS_KEY, &text) {
            self.error(format!("Cannot save the start date answer: {e}"));
        }
    }

    /// A Start typed for activity `i`, from the table or the Status tab, or
    /// the drop of its dragged bar.
    /// Started: the actual start. Not started: the question, or the saved
    /// answer when it is not asked; a date equal to the Start changes
    /// nothing. Ok(false) while the question is open.
    pub(crate) fn type_start(&mut self, i: usize, date: i64) -> Result<bool, String> {
        let Some(a) = self.project.model.activities.get(i) else {
            return Err("No such activity.".into());
        };
        if a.status != StatusType::NotStarted {
            return self.edit_activity(i, Edit::Start(date));
        }
        if a.early_start == Some(date) {
            return Ok(false);
        }
        let prefs = self.sched.start_hold_prefs;
        if prefs.ask {
            self.sched.start_hold = Some(Prompt {
                task_id: a.task_id,
                date,
                keep: false,
            });
            return Ok(false);
        }
        self.edit_activity(i, held(prefs.hold, date))
    }

    /// Yes (`Some(true)`), No (`Some(false)`), or Escape (`None`: the typed
    /// date is dropped and nothing changes).
    fn answer_start_hold(&mut self, answer: Option<bool>) {
        let Some(prompt) = self.sched.start_hold.take() else {
            return;
        };
        let Some(hold) = answer else {
            self.status = "Start not changed.".into();
            return;
        };
        if prompt.keep {
            self.set_start_hold(Prefs { ask: false, hold });
        }
        let Some(i) = self
            .project
            .model
            .activities
            .iter()
            .position(|a| a.task_id == prompt.task_id)
        else {
            return;
        };
        if let Err(e) = self.edit_activity(i, held(hold, prompt.date)) {
            self.error(e);
        }
    }

    /// The question, while a typed Start waits for it.
    pub(crate) fn start_hold_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut prompt) = self.sched.start_hold.clone() else {
            return;
        };
        let date = format::date(self.project.model.origin_serial, Some(prompt.date));
        let message = format!(
            "Keep this activity on {date} when the project is scheduled, with a \
             Start On or After constraint on that date?"
        );
        let size = Vec2::new(496., 104.);
        let ((yes, no), closed, actions) =
            crate::dialog_kit::modal(ctx, "start_hold", "Confirmation", size, |c| {
                c.fill(0., 0., size.x, size.y, FACE);
                let icon = Rect::from_center_size(c.origin + Vec2::new(33., 32.), Vec2::splat(32.));
                crate::kit_window::paint_message_icon(
                    c.ui.painter(),
                    icon,
                    crate::kit_window::MessageIcon::Question,
                );
                let galley = c.ui.painter().layout(
                    message,
                    crate::dialog_kit::dialog_font(),
                    chrome::INK,
                    300.,
                );
                c.ui.painter()
                    .galley(c.origin + Vec2::new(68., 12.), galley, chrome::INK);
                c.checkbox(
                    "start_hold_keep",
                    21.,
                    76.,
                    318.,
                    22.,
                    "Always use this answer",
                    &mut prompt.keep,
                    true,
                );
                crate::dialog_kit::strip(c, size.x, size.y);
                let column = crate::dialog_kit::button_column(
                    c,
                    size.x,
                    &[
                        Entry::Button("start_hold_yes", "Yes", true, false),
                        Entry::Button("start_hold_no", "No", true, false),
                    ],
                );
                for (key, caption) in [("start_hold_yes", "Yes"), ("start_hold_no", "No")] {
                    if crate::kit_window::Question::StartHold.is_default(caption) {
                        crate::dialog_kit::mark_default(c, key);
                    }
                }
                let clicked = |key: &str| column.clicked.as_deref() == Some(key);
                (clicked("start_hold_yes"), clicked("start_hold_no"))
            });
        self.record_dialog_actions(actions);
        if let Some(open) = self.sched.start_hold.as_mut() {
            open.keep = prompt.keep;
        }
        if yes {
            self.answer_start_hold(Some(true));
        } else if no {
            self.answer_start_hold(Some(false));
        } else if closed {
            self.answer_start_hold(None);
        }
    }
}

/// The shared edit for an answer.
fn held(hold: bool, date: i64) -> Edit {
    if hold {
        Edit::Start(date)
    } else {
        Edit::StartUnheld(date)
    }
}
