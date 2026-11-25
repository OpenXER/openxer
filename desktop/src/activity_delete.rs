//! Edit > Delete and the Delete key: remove the selected activity with its
//! relationships and every child row (assignments, codes, notebook, steps,
//! expenses, documents, feedback, risks, user defined fields), after a
//! confirmation. One undo step restores all of it.
use super::*;

impl App {
    /// Ask to delete the selected activities.
    pub(super) fn request_delete(&mut self) {
        let rows = self.selected_in_order();
        if rows.is_empty() {
            self.status = "Select an activity to delete.".into();
            return;
        }
        self.delete_confirm = Some(
            rows.iter()
                .map(|&i| self.project.model.activities[i].task_id)
                .collect(),
        );
    }

    /// The confirmation shown before a delete.
    pub(super) fn delete_dialog(&mut self, ctx: &egui::Context) {
        let Some(ids) = self.delete_confirm.clone() else {
            return;
        };
        let names: Vec<String> = ids
            .iter()
            .filter_map(|id| {
                let i = self
                    .project
                    .model
                    .activities
                    .iter()
                    .position(|a| a.task_id == *id)?;
                Some(format!(
                    "{} - {}",
                    self.project.model.activities[i].task_code,
                    self.project.name(i)
                ))
            })
            .collect();
        if names.is_empty() {
            self.delete_confirm = None;
            return;
        }
        let mut answer = None;
        let mut text = if names.len() == 1 {
            "Delete selected activity?".to_string()
        } else {
            format!("Delete {} selected activities?", names.len())
        };
        for name in names.iter().take(5) {
            text.push('\n');
            text.push_str(name);
        }
        if names.len() > 5 {
            text.push_str(&format!("\nand {} more", names.len() - 5));
        }
        let mut rects = vec![];
        let clicked = crate::kit_window::message_box(
            ctx,
            "delete",
            "Confirmation",
            crate::kit_window::MessageIcon::Question,
            &text,
            &["Yes", "No"],
            crate::kit_window::Question::DeleteActivities.default_in(&["Yes", "No"]),
            &mut rects,
        );
        self.record_dialog_actions(rects);
        match clicked {
            Some(0) => answer = Some(true),
            Some(_) => answer = Some(false),
            None => {}
        }
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            answer = Some(false);
        }
        match answer {
            Some(true) => {
                self.delete_confirm = None;
                let time = ctx.input(|i| i.time);
                self.delete_activities(&ids, time);
            }
            Some(false) => {
                self.delete_confirm = None;
            }
            None => {}
        }
    }

    /// Delete activities as one undo step and select the next table row
    /// (in the table's sort and grouping), scrolled into view.
    pub(super) fn delete_activities(&mut self, ids: &[i64], time: f64) {
        let before = self.project.clone();
        let before_revision = self.records_revision;
        let next = self.next_after_removal(ids);
        match self.project.delete_activities(ids) {
            Ok(_) => {
                self.touch_records();
                self.schedule_bulk_edit(&before.model);
                self.history.record_bulk(
                    &before,
                    before_revision,
                    &self.project,
                    self.records_revision,
                    time,
                );
                self.select_after_removal(next);
                self.edit_cell = None;
                self.link_drag = None;
                self.link_editor = None;
                self.date_edits.clear();
                self.detail.rows = [None, None];
                self.status = if ids.len() == 1 {
                    "Activity deleted".into()
                } else {
                    format!("{} activities deleted", ids.len())
                };
            }
            Err(e) => {
                self.project = before;
                self.error(e);
            }
        }
    }
}
