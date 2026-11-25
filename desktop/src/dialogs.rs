use super::*;
/// The Edit Relationship dialog's draft: the relationship, the chosen type
/// and the lag as typed, in the user's duration format.
#[derive(Clone)]
pub(super) struct LinkEditor {
    pub(super) index: usize,
    pub(super) kind: LinkType,
    pub(super) lag_text: String,
    /// Why the last OK was refused, shown under the Lag field.
    pub(super) error: Option<String>,
    /// Delete was clicked: the question is up.
    pub(super) confirm_delete: bool,
}

/// Relationship types in the order the type list shows them.
const LINK_TYPES: [LinkType; 4] = [LinkType::FS, LinkType::SS, LinkType::FF, LinkType::SF];

impl App {
    /// Open Edit Relationship on relationship `index` of the current project.
    pub(super) fn edit_link(&mut self, index: usize) {
        if let Some(link) = self.project.model.links.get(index) {
            let periods = format::lag_periods(&self.project, link.pred, link.succ);
            self.link_editor = Some(LinkEditor {
                index,
                kind: link.link_type,
                lag_text: format::duration(link.lag, periods),
                error: None,
                confirm_delete: false,
            });
        }
    }

    /// `ID - Name` of an activity, as the dialog names the two ends.
    fn link_end_label(&self, i: usize) -> String {
        format!(
            "{} - {}",
            self.project.model.activities[i].task_code,
            self.project.name(i)
        )
    }

    /// Edit Relationship (456 x 146 px inside the frame):
    /// a sunken work panel with the type picture, the predecessor and
    /// successor, the Relationship Type list and the Lag edit, and OK,
    /// Cancel, Delete and Help in the right-hand button column. OK and Delete are
    /// one undo step each; Cancel, the close box and Escape change nothing.
    fn link_editor_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut editor) = self.link_editor.take() else {
            return;
        };
        let Some(link) = self.project.model.links.get(editor.index).cloned() else {
            return;
        };
        let pred = self.link_end_label(link.pred);
        let succ = self.link_end_label(link.succ);
        let mut selected = LINK_TYPES
            .iter()
            .position(|k| *k == editor.kind)
            .unwrap_or(0);
        let ((ok, cancel, delete, help), closed, actions) = schedule_dialogs::modal(
            ctx,
            "edit_relationship",
            "Edit Relationship",
            Vec2::new(456., 146.),
            |c| {
                c.fill(5., 5., 339., 136., dialog_kit::FACE);
                c.lowered(5., 5., 339., 136.);
                let picture = c.r(18., 17., 69., 68.);
                c.sunken(picture, Color32::WHITE);
                gantt::relationship_picture(c.ui.painter(), picture.shrink(2.), editor.kind);
                let clip = c.r(90., 6., 252., 134.);
                let painter = c.ui.painter().with_clip_rect(clip);
                for (y, text) in [(28., &pred), (54., &succ)] {
                    painter.text(
                        c.origin + Vec2::new(94., y),
                        Align2::LEFT_TOP,
                        text,
                        schedule_dialogs::dialog_font(),
                        INK,
                    );
                }
                dialog_kit::text(c, 94., 90., "Relationship Type", true);
                dialog_kit::text(c, 270., 90., "Lag", true);
                let items: Vec<(&str, bool)> = LINK_TYPES
                    .iter()
                    .map(|k| (&k.as_str()[3..], true))
                    .collect();
                c.combo(
                    "link_type",
                    94.,
                    105.,
                    169.,
                    22.,
                    &items,
                    &mut selected,
                    true,
                );
                c.edit(
                    "link_lag",
                    270.,
                    105.,
                    57.,
                    21.,
                    &mut editor.lag_text,
                    true,
                    Align::Min,
                );
                if let Some(error) = &editor.error {
                    dialog_kit::error_text(c, 94., 128., 246., error);
                }
                let out = dialog_kit::button_column(
                    c,
                    456.,
                    &[
                        dialog_kit::Entry::ok("link_ok"),
                        dialog_kit::Entry::cancel("link_cancel"),
                        dialog_kit::Entry::Button("link_delete", "Delete", true, true),
                        dialog_kit::Entry::help("link_help"),
                    ],
                );
                let clicked = out.clicked.as_deref();
                (
                    clicked == Some("link_ok"),
                    clicked == Some("link_cancel"),
                    clicked == Some("link_delete"),
                    clicked == Some("link_help"),
                )
            },
        );
        self.record_dialog_actions(actions);
        editor.kind = LINK_TYPES[selected];
        // Delete asks first (Yes is the default, as for Remove on the
        // Relationships tab); No goes back to the dialog.
        let mut delete = delete;
        if editor.confirm_delete {
            let buttons = ["Yes", "No"];
            let mut rects = vec![];
            let answer = crate::kit_window::message_box(
                ctx,
                "delete_relationship",
                "Confirmation",
                crate::kit_window::MessageIcon::Question,
                "Delete this relationship?",
                &buttons,
                crate::kit_window::Question::DeleteRelationship.default_in(&buttons),
                &mut rects,
            );
            self.record_dialog_actions(rects);
            match answer {
                Some(0) => {
                    editor.confirm_delete = false;
                    delete = true;
                }
                Some(_) => editor.confirm_delete = false,
                None if ctx.input(|i| i.key_pressed(Key::Escape)) => {
                    editor.confirm_delete = false;
                }
                None => {}
            }
            if !delete {
                self.link_editor = Some(editor);
                return;
            }
        } else if delete {
            editor.confirm_delete = true;
            self.link_editor = Some(editor);
            return;
        }

        // Enter reaches OK through `dialog_keys`, and only while this dialog
        // is on top (not while Help or a drop-down list is).
        if help {
            self.open_help_for("edit_relationship");
        }
        if cancel || closed {
            return;
        }
        if !(ok || delete) {
            self.link_editor = Some(editor);
            return;
        }
        let was_out_of_sequence =
            schedule_check::link_out_of_sequence(&self.project.model, editor.index);
        let result = if delete {
            relationships::remove(&mut self.project.model, editor.index)
        } else {
            let periods = format::lag_periods(&self.project, link.pred, link.succ);
            format::parse_duration(&editor.lag_text, periods).and_then(|lag| {
                relationships::edit(
                    &mut self.project.model,
                    editor.index,
                    editor.kind,
                    lag as f64 / 60.,
                )
            })
        };
        match result {
            Ok(()) => {
                let changed = delete
                    || self.project.model.links[editor.index].link_type != link.link_type
                    || self.project.model.links[editor.index].lag != link.lag;
                if changed {
                    self.dirty = true;
                }
                if !delete && changed && !was_out_of_sequence {
                    self.check_relationship_sequence(
                        editor.index,
                        Some((link.link_type, link.lag)),
                    );
                }
                self.status = if delete {
                    "Relationship deleted. Ctrl+Z restores it."
                } else {
                    "Relationship updated."
                }
                .into();
            }
            Err(error) => {
                // The dialog stays open on a bad lag or a duplicate type, and
                // says why under the Lag field.
                editor.error = Some(error);
                self.link_editor = Some(editor);
            }
        }
    }

    /// Escape on the error box closes it and nothing else: the key is taken
    /// before any dialog beneath reads it. Run first in the frame.
    pub(super) fn error_box_keys(&mut self, ctx: &egui::Context) {
        if self.error.is_some()
            && crate::kit_window::escape_goes_to(ctx, error_box_id(), egui::Order::Foreground)
        {
            self.error = None;
            ctx.input_mut(|i| {
                i.events.retain(|e| {
                    !matches!(
                        e,
                        egui::Event::Key {
                            key: Key::Escape,
                            pressed: true,
                            ..
                        }
                    )
                })
            });
        }
    }

    pub(super) fn dialogs(&mut self, ctx: &egui::Context) {
        self.link_editor_dialog(ctx);

        if self.show_about {
            let mut ok = false;
            chrome::dialog_window("About OpenXER").show(ctx, |ui| {
                let buttons = [chrome::DialogButton::ok()];
                let (_, clicked, responses) = chrome::dialog_with_buttons(ui, &buttons, |ui| {
                    ui.set_width(300.);
                    ui.label(
                        RichText::new("OpenXER").font(FontId::new(16., egui::FontFamily::Name("Arial Bold".into()))),
                    );
                    ui.label("Native project scheduling with a Rust engine.");
                    ui.add_space(6.);
                    ui.label("Activity planning, critical path scheduling, calendars, relationship logic and portable XER files.");
                    ui.label("OpenXER is an independent application.");
                    ui.add_space(6.);
                    ui.label(about_version());
                    ui.label(ABOUT_LICENSE);
                    ui.add_space(6.);
                    ui.label(format!("Workspace: {}", self.store.path.display()));
                });
                self.action("about_ok", &responses[0]);
                ok = clicked == Some(0);
            });
            let escape = crate::kit_window::escape_goes_to(
                ctx,
                egui::Id::new("About OpenXER"),
                egui::Order::Middle,
            );
            if ok || escape {
                self.show_about = false;
            }
        }
        if let Some(e) = self.error.clone() {
            let mut rects = vec![];
            let answer = crate::kit_window::message_box(
                ctx,
                "error",
                "OpenXER",
                crate::kit_window::MessageIcon::Error,
                &e,
                &["OK"],
                Some(0),
                &mut rects,
            );
            self.record_dialog_actions(rects);
            if answer.is_some()
                || crate::kit_window::escape_goes_to(ctx, error_box_id(), egui::Order::Foreground)
            {
                // Do not let the dismissing Enter or Escape bleed into a
                // dialog drawn below this error box in the same frame.
                ctx.input_mut(|i| {
                    i.consume_key(egui::Modifiers::NONE, Key::Enter);
                    i.consume_key(egui::Modifiers::NONE, Key::Escape);
                });
                self.error = None;
            }
        }
        let projects = self
            .pending
            .as_ref()
            .map(|(doc, _)| openxer_interchange::baseline::schedule_projects(doc));
        if let Some(projects) = projects {
            let mut chosen = None;
            let mut cancel = false;
            // 420 x 230 px inside the frame: the projects in a list, Open
            // and Cancel in the button column.
            const W: f32 = 420.;
            const H: f32 = 230.;
            let pick_id = egui::Id::new("choose_project_selected");
            let mut selected = ctx
                .data(|d| d.get_temp::<usize>(pick_id))
                .filter(|k| *k < projects.len());
            let ((), closed, actions) = dialog_kit::window(
                ctx,
                "choose_project",
                "Choose a Project",
                Vec2::new(W, H),
                |c| {
                    dialog_kit::strip(c, W, H);
                    dialog_kit::text(c, 8., 8., "This XER contains more than one project.", true);
                    let cols = [
                        dialog_kit::Col {
                            title: "Project ID",
                            width: 100.,
                        },
                        dialog_kit::Col {
                            title: "Project Name",
                            width: 190.,
                        },
                    ];
                    let out = dialog_kit::list(
                        c,
                        "choose_project",
                        (6., 26., W - dialog_kit::STRIP_WIDTH - 12., H - 32.),
                        None,
                        &cols,
                        projects.len(),
                        selected,
                        Some(0),
                        &mut |c, k, col, rect, sel| {
                            let (id, name) = &projects[k];
                            let text = if col == 0 {
                                id.to_string()
                            } else {
                                name.clone()
                            };
                            dialog_kit::cell_text(c, rect, &text, sel, 3.);
                        },
                    );
                    if let Some(k) = out.clicked {
                        selected = Some(k);
                    }
                    if let Some(k) = out.double {
                        chosen = Some(projects[k].0);
                    }
                    let out = dialog_kit::button_column(
                        c,
                        W,
                        &[
                            dialog_kit::Entry::Button(
                                "choose_project_open",
                                "&Open",
                                selected.is_some(),
                                false,
                            ),
                            dialog_kit::Entry::cancel("choose_project_cancel"),
                        ],
                    );
                    match out.clicked.as_deref() {
                        Some("choose_project_open") => {
                            chosen = selected.map(|k| projects[k].0);
                        }
                        Some(_) => cancel = true,
                        None => {}
                    }
                },
            );
            self.record_dialog_actions(actions);
            match selected {
                Some(k) => ctx.data_mut(|d| d.insert_temp(pick_id, k)),
                None => ctx.data_mut(|d| d.remove::<usize>(pick_id)),
            }
            let open = !(cancel || closed);
            if chosen.is_some() || !open {
                ctx.data_mut(|d| d.remove::<usize>(pick_id));
            }
            if let Some((id, (doc, source))) = chosen.zip(self.pending.take()) {
                match ProjectData::from_document(doc, id, source) {
                    Ok(p) => self.open_file_project(p),
                    Err(e) => self.error(e),
                }
            } else if !open {
                self.pending = None;
            }
        }
        self.replace_prompt_dialog(ctx);
        self.schedule_dialogs(ctx);
        self.loops_dialogs(ctx);
    }
}

/// The version line of About, from the crate version.
fn about_version() -> String {
    format!("Version {}", env!("CARGO_PKG_VERSION"))
}

/// The license line of About.
const ABOUT_LICENSE: &str = "Licensed under the MIT License or the Apache License 2.0, at your \
     option. Notices for third-party material (the spell check word list and the fonts \
     of the user interface library) are in THIRD_PARTY.md, which comes with OpenXER and with its source code.";

/// The layer of the error message box.
fn error_box_id() -> egui::Id {
    egui::Id::new(("kit_message", "error"))
}

#[cfg(test)]
#[path = "shell_tests.rs"]
mod shell_tests;
