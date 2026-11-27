//! The Assign dialogs of WBS and Project Details, on the shared assign kit:
//! Assign Notebook Topic (the Notebook tab's Add, for a WBS element or the
//! project) and Assign Project Codes (the Project Details Codes tab's
//! Assign, with Remove and New as on Assign Activity Codes).
use super::assign_kit::{AssignButton, AssignWindow};
use super::*;
use openxer_xer::edit::notebooks::{self, Owner};
use openxer_xer::edit::project_codes;

/// Window ids (also their help names).
pub const TOPIC_DIALOG: &str = "assign_notebook_topic";
pub const CODE_DIALOG: &str = "assign_project_codes";

/// Assign Notebook Topic for `owner` (a WBS element or the project).
#[derive(Clone, Debug, PartialEq)]
pub struct TopicPicker {
    pub owner: Owner,
    pub selected: Option<i64>,
}

/// Assign Project Codes: the code type shown and the value selected.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CodePicker {
    pub type_id: Option<i64>,
    pub selected: Option<i64>,
}

impl App {
    /// Open Assign Notebook Topic for `owner`.
    pub(crate) fn open_topic_picker(&mut self, owner: Owner) {
        self.detail.notes.topic_picker = Some(TopicPicker {
            owner,
            selected: None,
        });
    }

    /// Open Assign Project Codes for the open project.
    pub(crate) fn open_project_code_picker(&mut self) {
        self.detail.notes.code_picker = Some(CodePicker::default());
    }

    /// Ask before taking project code value `value_id` off the project.
    pub(crate) fn ask_remove_project_code(&mut self, value_id: i64) {
        self.detail.removal = Some(super::removals::Removal::ProjectCode { value_id });
    }

    /// The WBS and Project Details pickers, when open.
    pub(super) fn owner_pickers(&mut self, ctx: &egui::Context) {
        self.topic_picker_dialog(ctx);
        self.project_code_dialog(ctx);
    }

    fn topic_picker_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut picker) = self.detail.notes.topic_picker.take() else {
            return;
        };
        let pid = self.project.model.project.proj_id;
        let owner = picker.owner;
        let options = notebooks::unused_topics(&self.project.document, pid, owner);
        let keys: Vec<Option<i64>> = options.iter().map(|(id, _)| Some(*id)).collect();
        let window = AssignWindow {
            id: TOPIC_DIALOG,
            title: "Assign Notebook Topic",
            buttons: vec![AssignButton {
                key: "topic_pick_assign",
                caption: "Assign",
                enabled: picker.selected.is_some(),
            }],
            close: "topic_pick_close",
            help: "topic_pick_help",
        };
        let mut double = None;
        let (open, clicked) = self.assign_window(ctx, &window, |app, ui| {
            let out = app.assign_list(
                ui,
                TOPIC_DIALOG,
                "topic_pick_grid",
                &[("Notebook Topic", 300.)],
                &keys,
                &mut picker.selected,
                Vec2::new(360., 200.),
                &mut |ui, r, _, _| cell(ui, &options[r].1),
            );
            double = out.double;
        });
        let mut ids = match clicked {
            Some("topic_pick_assign") => self.assign_picked(TOPIC_DIALOG, picker.selected),
            _ => vec![],
        };
        if let Some(id) = double {
            ids = vec![id];
        }
        if !ids.is_empty() {
            let doc = &mut self.project.document;
            let mut added = None;
            for id in ids {
                match notebooks::add(doc, pid, owner, id) {
                    Ok(row) => added = Some(row),
                    Err(e) => self.status = e,
                }
            }
            if let Some(row) = added {
                self.notes_step();
                let memos = notebooks::memos(&self.project.document, pid, owner);
                let at = memos.iter().position(|m| m.id == row).unwrap_or(0);
                // The WBS view's project row shows the project's notebook.
                self.views.wbs.topic = at;
                if let Owner::Project(_) = owner {
                    self.views.projects.topic = at;
                }
            }
            // As for an activity, the picker closes once a topic is added.
            self.detail.assign_rows.remove(TOPIC_DIALOG);
            return;
        }
        if open {
            self.detail.notes.topic_picker = Some(picker);
        }
    }

    fn project_code_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut picker) = self.detail.notes.code_picker.take() else {
            return;
        };
        let pid = self.project.model.project.proj_id;
        let doc = &self.project.document;
        let types = project_codes::code_types(doc);
        let known = |t: i64| types.iter().any(|(id, _)| *id == t);
        if !picker.type_id.is_some_and(known) {
            picker.type_id = types.first().map(|(id, _)| *id);
            picker.selected = None;
        }
        let values = |t| project_codes::values(doc, t);
        let options = picker.type_id.map_or(vec![], values);
        let keys: Vec<Option<i64>> = options.iter().map(|(id, _, _)| Some(*id)).collect();
        let assigned: Vec<i64> = project_codes::assigned(doc, pid)
            .iter()
            .map(|a| a.value_id)
            .collect();
        let current = picker.selected;
        // Remove takes the selected value off the project when it has it.
        let remove = match self.assign_picked(CODE_DIALOG, current).as_slice() {
            [one] if assigned.contains(one) => Some(*one),
            _ => None,
        };
        let window = AssignWindow {
            id: CODE_DIALOG,
            title: "Assign Project Codes",
            buttons: vec![
                AssignButton {
                    key: "project_code_pick_assign",
                    caption: "Assign",
                    enabled: current.is_some(),
                },
                AssignButton {
                    key: "project_code_pick_remove",
                    caption: "Remove",
                    enabled: remove.is_some(),
                },
                AssignButton {
                    key: "project_code_pick_new",
                    caption: "New...",
                    enabled: picker.type_id.is_some() && (current.is_some() || options.is_empty()),
                },
            ],
            close: "project_code_pick_close",
            help: "project_code_pick_help",
        };
        let mut double = None;
        let (open, clicked) = self.assign_window(ctx, &window, |app, ui| {
            ui.horizontal(|ui| {
                ui.label("Project Code");
                let shown = types
                    .iter()
                    .find(|(id, _)| Some(*id) == picker.type_id)
                    .map_or("", |(_, n)| n.as_str());
                crate::dialog_kit::flow_combo_with(
                    ui,
                    "project_code_pick_type",
                    220.,
                    shown,
                    true,
                    |ui| {
                        for (id, name) in &types {
                            let on = Some(*id) == picker.type_id;
                            if crate::dialog_kit::flow_selectable(ui, on, name, 216.).clicked() {
                                picker.type_id = Some(*id);
                                picker.selected = None;
                                ui.close();
                            }
                        }
                    },
                );
            });
            let out = app.assign_list(
                ui,
                CODE_DIALOG,
                "project_code_pick_grid",
                &[("Code Value", 110.), ("Description", 220.)],
                &keys,
                &mut picker.selected,
                Vec2::new(360., 200.),
                &mut |ui, r, c, _| {
                    let (_, value, description) = &options[r];
                    cell(ui, if c == 0 { value } else { description })
                },
            );
            double = out.double;
        });
        let mut assign = match clicked {
            Some("project_code_pick_assign") => self.assign_picked(CODE_DIALOG, picker.selected),
            _ => vec![],
        };
        if let Some(id) = double {
            assign = vec![id];
        }
        // One value per code type: the first chosen, as for activity codes.
        if let Some(&id) = assign.first() {
            match project_codes::assign(&mut self.project.document, pid, id) {
                Ok(true) => self.notes_step(),
                Ok(false) => {}
                Err(e) => self.error(e),
            }
        }
        if let (Some("project_code_pick_remove"), Some(id)) = (clicked, remove) {
            self.ask_remove_project_code(id);
        }
        if let (Some("project_code_pick_new"), Some(t)) = (clicked, picker.type_id) {
            self.detail.new_code = Some(super::assign_kit::NewCode {
                type_id: t,
                project: true,
                ..Default::default()
            });
        }
        if open {
            self.detail.notes.code_picker = Some(picker);
            // The activity pickers draw Add Code Value while they are open.
            if self.detail.picker.is_none() {
                self.new_code_dialog(ctx);
            }
        } else if self.detail.new_code.as_ref().is_some_and(|d| d.project) {
            self.detail.new_code = None;
        }
    }
}
