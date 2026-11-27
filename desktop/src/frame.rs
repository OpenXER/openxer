use super::*;
impl App {
    pub(super) fn update_ui(&mut self, ctx: &egui::Context) {
        crate::kit_window::deliver_close_requests(ctx);
        // A long operation queued last frame runs now (usy).
        self.busy_frame(ctx);
        self.grid.frame_verified_serial.set(None);
        let read_only_events = ctx.input(|i| {
            i.events.iter().all(|e| {
                matches!(
                    e,
                    egui::Event::PointerMoved(_) | egui::Event::MouseWheel { .. }
                )
            })
        });
        self.first_run_and_drops(ctx);
        // F1 and Ctrl+F1 work over any dialog; Escape closes Help first.
        self.help_keys(ctx);
        self.error_box_keys(ctx);
        // A close (close box, Alt+F4, the system) saves first; a failed save
        // cancels it and asks (close_guard).
        if ctx.input(|i| i.viewport().close_requested()) {
            self.close_requested(ctx);
        }
        self.frame_number += 1;
        // A page preview under the pointer takes the wheel and the zoom.
        crate::print::viewer::take_wheel(ctx);
        self.refresh_composite();
        format::install(&self.formats);
        #[cfg(debug_assertions)]
        self.capture_frame(ctx);
        self.action_rects.clear();
        self.bar_geometry.clear();
        self.row_geometry.clear();
        self.link_geometry.clear();
        self.poll_context_menu_keys(ctx);
        if self.context_menu_blocked(ctx) {
            self.ctx_menu.clear();
        }
        self.menu_keys(ctx);
        self.chat_keys(ctx);
        self.dialog_keys(ctx);
        self.handle_shortcuts(ctx);
        self.window_title(ctx);
        self.menu(ctx);
        self.status_bar(ctx);
        self.edge_dock(ctx, dockbars::Side::Bottom);
        self.layout_panels(ctx);
        self.bar_drag(ctx);
        // A request no view took (none can show a menu) is dropped.
        self.ctx_menu.clear();
        context_menu::drain_rects(ctx, &mut self.action_rects);
        if std::mem::take(&mut self.grid.cells.tab_used) {
            // Tab moved the table's focus cell; no other control keeps it.
            if let Some(id) = ctx.memory(|m| m.focused()) {
                ctx.memory_mut(|m| m.surrender_focus(id));
            }
        }
        let still_verified = read_only_events
            && !self.busy_active()
            && self.link_drag.is_none()
            && self.gantt_ui.bar_drag.is_none()
            && self.edit_cell.is_none()
            && self.grid.pending_edit.is_none()
            && self.editing.pending.is_none()
            && !self.open_set.multi()
            && self.records_revision == self.history.revision()
            && self.grid.frame_verified_serial.get() == Some(self.history.current_serial());
        if !still_verified && !self.schedule_after_edit() {
            self.history.record(
                &self.project,
                self.records_revision,
                ctx.memory(|m| m.focused()).map(|id| id.value()),
                ctx.input(|i| i.time),
            );
        }
        self.sync_dictionary(ctx.input(|i| i.time));
        self.sync_journal();
        // The Activities view's preferences are written from its own state;
        // the additional view saves its layout under its own key.
        self.with_first_activities_state(|app| {
            app.persist_view_when_idle(ctx);
            app.persist_layout_when_idle(ctx);
            app.persist_panes();
        });
        self.persist_additional_activities();
    }
    /// `OpenXER : <project ID> (<project name>)`, sent only when it changes.
    fn window_title(&mut self, ctx: &egui::Context) {
        let title = if self.open_set.empty {
            "OpenXER".to_string()
        } else {
            format!(
                "OpenXER : {} ({})",
                self.project.model.project.name,
                views::project_title(&self.project)
            )
        };
        let key = egui::Id::new("openxer_window_title");
        if ctx.data(|d| d.get_temp::<String>(key)).as_ref() != Some(&title) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            ctx.data_mut(|d| d.insert_temp(key, title));
        }
    }
    /// `--capture-dir` (debug builds): screenshots of the window with its
    /// state and geometry, on the fifth frame and on F12.
    #[cfg(debug_assertions)]
    fn capture_frame(&mut self, ctx: &egui::Context) {
        if let Some(directory) = &self.capture_dir {
            for event in ctx.input(|i| i.events.clone()) {
                if let egui::Event::Screenshot { image, .. } = event {
                    let _ = std::fs::create_dir_all(directory);
                    let path = directory.join(format!("view-{:03}.png", self.capture_count));
                    let rgba: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
                    if let Err(error) = image::save_buffer(
                        &path,
                        &rgba,
                        image.width() as u32,
                        image.height() as u32,
                        image::ColorType::Rgba8,
                    ) {
                        eprintln!("Screenshot: {error}");
                    }
                    let state = format!("schedule_dialog={}\nactivities={}\nselected={:?}\ndirty={}\nstatus={}\nquery={}\n",self.show_schedule,self.project.model.activities.len(),self.selected,self.dirty,self.status,self.query);
                    let _ = std::fs::write(path.with_extension("state"), state);
                    let bars:Vec<_>=self.bar_geometry.iter().map(|b|serde_json::json!({"task_id":b.task_id,"start":[b.start.x,b.start.y],"finish":[b.finish.x,b.finish.y],"row":[b.row.left(),b.row.top(),b.row.right(),b.row.bottom()]})).collect();
                    let relationships:Vec<_>=self.project.model.links.iter().enumerate().map(|(index,l)|serde_json::json!({"index":index,"pred_task_id":self.project.model.activities[l.pred].task_id,"succ_task_id":self.project.model.activities[l.succ].task_id,"type":l.link_type.as_str(),"lag_minutes":l.lag,"points":self.link_geometry.iter().find(|(i,_)|*i==index).map(|(_,points)|points.iter().map(|p|[p.x,p.y]).collect::<Vec<_>>())})).collect();
                    let (undo_count, redo_count) = self.history.counts();
                    let geometry = serde_json::json!({"actions":self.action_rects,"bars":bars,"relationships":relationships,"selected_task_id":self.selected.and_then(|i|self.project.model.activities.get(i)).map(|a|a.task_id),"links_count":self.project.model.links.len(),"schedule_runs":self.schedule_runs,"options_open":self.show_schedule,"undo_count":undo_count,"redo_count":redo_count,"gantt_left":self.gantt_left,"query":self.query,"status":self.status,"error":self.error,"details_visible":self.show_details,"link_editor_open":self.link_editor.is_some(),"drag_active":self.link_drag.is_some()});
                    let temporary = path.with_extension("json.tmp");
                    if let Ok(bytes) = serde_json::to_vec_pretty(&geometry) {
                        if std::fs::write(&temporary, bytes).is_ok() {
                            let _ = std::fs::rename(temporary, path.with_extension("json"));
                        }
                    }

                    self.capture_count += 1;
                }
            }
            if self.frame_number == 5 || ctx.input(|i| i.key_pressed(Key::F12)) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            }
            if self.capture_count == 0 {
                ctx.request_repaint();
            }
        }
    }
    /// A dialog (or a window that takes the keyboard like one) is up: the
    /// main window's accelerators wait until it closes.
    pub(crate) fn modal_open(&self, ctx: &egui::Context) -> bool {
        self.busy_active()
            || !crate::dialog_kit::open_dialog_layers(ctx).is_empty()
            || self.print.preview.is_some()
            || self.top_schedule_dialog().is_some()
            || self.link_editor.is_some()
            || self.gantt_ui.text.is_some()
            || self.delete_confirm.is_some()
            || self.grid_dialog_open()
            || self.editing_dialog_open()
            || self.tools_modal_open()
            || self.layout_dialog_open()
            || self.enterprise_open()
            || self.dictionary_open()
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.key_pressed(Key::Escape))
            && !self.busy_active()
            && !self.cancel_print_dialog()
            && !self.cancel_top_schedule_dialog()
        {
            if self.link_drag.take().is_some() {
                self.status = "Relationship cancelled.".into();
            }
            self.link_editor = None;
            self.gantt_ui.bar_drag = None;
        }
        // One gate for the main window's accelerators: none runs while a
        // dialog is open. Keys a focused text field uses stay with the field;
        // the commands of `ACCELERATORS` still run, after the field commits.
        let modal = self.modal_open(ctx);
        let typing = ctx.wants_keyboard_input();
        let open = !modal && !typing;
        // The Resource Usage Spreadsheet's selected cells take their keys
        // (F2, typing, Ctrl+E, Ctrl+H, Shift+Ctrl+C, Shift+Ctrl+V) first.
        if open {
            self.sheet_cell_keys(ctx);
        }
        // A report tab has the focus: Print (see `print_command`) and Refresh act
        // on its report.
        let report_tab = self.current_report_tab();
        let deferred = ctx
            .data_mut(|d| d.remove_temp::<Option<Accel>>(deferred_accel_id()))
            .flatten();
        if let Some(accel) = deferred.or_else(|| pressed_accel(ctx)).filter(|_| !modal) {
            if !typing || deferred.is_some() {
                self.run_accel(accel, report_tab);
            } else if let Some((activity, field)) = self.edit_cell.take() {
                // The table's cell editor commits now.
                self.apply_cell(activity, field, &self.edit_text.clone());
                self.run_accel(accel, report_tab);
            } else {
                // Another field commits when it loses the keyboard (this
                // frame); the command runs at the start of the next.
                if let Some(id) = ctx.memory(|m| m.focused()) {
                    ctx.memory_mut(|m| m.surrender_focus(id));
                }
                ctx.data_mut(|d| d.insert_temp(deferred_accel_id(), Some(accel)));
                ctx.request_repaint();
            }
        }
        // Shift+F9 opens Level Resources alone (`progress_dialogs`); a cell
        // being edited is committed first.
        if !modal
            && ctx.input(|i| {
                i.key_pressed(Key::F9) && i.modifiers.matches_exact(egui::Modifiers::SHIFT)
            })
        {
            if let Some((activity, field)) = self.edit_cell.take() {
                self.apply_cell(activity, field, &self.edit_text.clone());
            }
        }
        // A dictionary dialog is modal: the activity table keeps showing but
        // takes no keys.
        let activities = self.views.main.is_activities()
            && !self.dictionary_open()
            && !self.choice_editor_open();
        if open && ctx.input(|i| i.key_pressed(Key::Insert)) && activities {
            self.add_activity_command();
        }
        if open {
            let back = egui::Modifiers::COMMAND | egui::Modifiers::SHIFT;
            let forward = if ctx.input_mut(|i| i.consume_key(back, Key::Tab)) {
                Some(false)
            } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::Tab)) {
                Some(true)
            } else {
                None
            };
            if let Some(forward) = forward {
                // Tab also moves egui's keyboard focus; not for Ctrl+Tab.
                ctx.memory_mut(|m| m.move_focus(egui::FocusDirection::None));
                self.cycle_tabs(forward);
            }
        }
        if open && accel_pressed(ctx, egui::Modifiers::SHIFT, Key::F12) {
            self.toggle_reorganize();
        }
        // Alt+F1 alone toggles Cell Tips; Ctrl+Alt+F1 (once shown for the
        // removed Online Help) does nothing.
        let cell_tips = |i: &egui::InputState| {
            i.modifiers.alt && !i.modifiers.ctrl && !i.modifiers.command && i.key_pressed(Key::F1)
        };
        if open && ctx.input(cell_tips) {
            self.toggle_hint_help();
        }
        // F3 runs Find Next on the last search and never moves the selection
        // on its own. The Find dialog, alone on screen, keeps it too.
        let find_alone = self.top_grid_dialog() == "find"
            && crate::dialog_kit::open_dialog_layers(ctx).len() == 1;
        let findable = activities || self.view_find_ready();
        if findable && ctx.input(|i| i.key_pressed(Key::F3)) && (open || find_alone) {
            self.run_find_next();
        }
        // A page viewer has its own Find (Ctrl+F).
        let viewer_open = self.print.preview.is_some()
            || self.reports.page_preview.is_some()
            || report_tab.is_some();
        if open && !viewer_open && accel_pressed(ctx, egui::Modifiers::COMMAND, Key::F) {
            self.open_find(false);
        }
        if open
            && !viewer_open
            && (accel_pressed(ctx, egui::Modifiers::COMMAND, Key::H)
                || accel_pressed(ctx, egui::Modifiers::COMMAND, Key::R))
        {
            self.open_find(true);
        }
        if open {
            if activities {
                self.editing_shortcuts(ctx);
            }
            if accel_pressed(ctx, egui::Modifiers::COMMAND, Key::Z) {
                self.history_move(ctx, false);
            }
            // Ctrl+Y and Ctrl+Shift+Z redo.
            if accel_pressed(ctx, egui::Modifiers::COMMAND, Key::Y)
                || accel_pressed(
                    ctx,
                    egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                    Key::Z,
                )
            {
                self.history_move(ctx, true);
            }
            if activities && ctx.input(|i| i.key_pressed(Key::Delete)) {
                self.request_delete();
            }
            if activities {
                self.table_keys(ctx);
            }
            // The other view tables: Ctrl+A and the row keys.
            if !activities && self.grid_view_keys_ready() {
                if ctx.input(|i| i.modifiers.command && i.key_pressed(Key::A)) {
                    self.grid_view_select_all();
                }
                self.grid_view_keys(ctx);
            }
            if activities {
                self.row_keys(ctx);
            }
            if activities && ctx.input(|i| i.key_pressed(Key::F2)) {
                self.begin_edit_focused();
            }
            if activities {
                self.type_to_edit(ctx);
            }
        }
    }
    /// Run a main window command reached by its accelerator.
    fn run_accel(
        &mut self,
        accel: Accel,
        report_tab: Option<crate::reports::report_tab::ReportTabId>,
    ) {
        match accel {
            Accel::Open => self.open_project_dialog(),
            Accel::CloseAll => self.close_all_projects(),
            Accel::Save => self.save(),
            Accel::New => self.open_new_project(),
            Accel::Print => self.print_command(),
            Accel::CloseView => {
                let view = self.views.groups.current();
                self.request_close(view);
            }
            Accel::Refresh => match report_tab {
                Some(id) => self.refresh_report_tab(id),
                None => self.refresh_data(),
            },
            Accel::Schedule => {
                if let Some((activity, field)) = self.edit_cell.take() {
                    self.apply_cell(activity, field, &self.edit_text.clone());
                }
                self.open_schedule_dialog();
            }
        }
    }

    /// Status bar: 18 px of sunken panels sized to their text (Modified,
    /// Default Project, Data Date, Baseline, User, Workspace); the last panel
    /// fills the width and carries OpenXER's status messages.
    fn status_bar(&mut self, ctx: &egui::Context) {
        if !self.views.show_status_bar {
            return;
        }
        egui::TopBottomPanel::bottom("status_bar")
            .exact_height(18.)
            .show_separator_line(false)
            .frame(egui::Frame::NONE.fill(chrome::pal::FACE))
            .show(ctx, |ui| {
                let r = ui.max_rect();
                let p = ui.painter();
                let panel = |x: f32, width: f32| {
                    Rect::from_min_max(Pos2::new(x, r.top() + 2.), Pos2::new(x + width, r.bottom()))
                };
                let mut x = r.left();
                for text in self.status_panels() {
                    let width = chrome::status_width(ctx, &text);
                    chrome::status_panel(p, panel(x, width), &text, true);
                    x += width + 2.;
                }
                let last = panel(x, (r.right() - x).max(0.));
                chrome::status_panel(p, last, &self.status, true);
                self.agent_hint(ui, last);
                let p = ui.painter();
                // Size grip: three diagonal rows of dots at the right end.
                for row in 0..3 {
                    for col in 0..=row {
                        let at = Pos2::new(
                            last.right() - 4. - 4. * col as f32,
                            last.bottom() - 4. - 4. * (row - col) as f32,
                        );
                        chrome::fill(
                            p,
                            Rect::from_min_size(at, Vec2::splat(2.)),
                            chrome::pal::SHADOW,
                        );
                    }
                }
            });
    }

    /// Texts of the fixed status panels, in order.
    pub(super) fn status_panels(&self) -> Vec<String> {
        let user = std::env::var("USERNAME")
            .or_else(|_| std::env::var("USER"))
            .unwrap_or_else(|_| "local".into());
        let db = self
            .store
            .path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "workspace".into());
        if self.open_set.empty {
            return vec![
                "No project open".into(),
                format!("User: {user}"),
                format!("Workspace: {db}"),
            ];
        }
        let data_date = format::date(
            self.project.model.origin_serial,
            self.project.model.project.data_date,
        );
        let mut panels = vec![
            format!("Data Date: {data_date}"),
            format!("Baseline: {}", self.baseline_label()),
            format!("User: {user}"),
            format!("Workspace: {db}"),
        ];
        if let Some(default) = self.default_project_name() {
            panels.insert(0, format!("Default Project: {default}"));
        }
        // Any open project with changes; they are written on Save, and on
        // close and project switches without asking.
        if self.any_dirty() {
            panels.insert(0, MODIFIED.to_string());
        }
        panels.extend(self.schedule_needed_text());
        panels
    }

    /// Baseline shown in the status bar: the project baseline's name.
    pub(super) fn baseline_label(&self) -> String {
        self.project_baseline_name()
    }

    fn layout_panels(&mut self, ctx: &egui::Context) {
        self.navigation_bar(ctx);
        self.chat_docked_panel(ctx);
        self.view_area(ctx);
        self.dialogs(ctx);
        self.interchange_dialogs(ctx);
        self.view_dialog(ctx);
        // Before the pickers, which open above it.
        self.activity_wizard_dialog(ctx);
        self.resource_wizard_dialog(ctx);
        self.detail_dialogs(ctx);
        self.delete_dialog(ctx);
        self.calendars_dialog(ctx);
        self.dictionary_dialogs(ctx);
        self.grid_dialogs(ctx);
        self.user_preferences_dialog(ctx);
        self.editing_frame(ctx);
        self.baseline_dialogs(ctx);
        self.pane_dialogs(ctx);
        self.print_dialogs(ctx);
        self.open_set_dialogs(ctx);
        self.record_view_dialogs(ctx);
        self.compare_dialogs(ctx);
        self.checker_dialogs(ctx);
        self.report_dialogs(ctx);
        self.spell_dialog(ctx);
        self.customize_toolbars_dialog(ctx);
        self.top_down_dialog(ctx);
        self.layout_dialogs(ctx);
        self.enterprise_dialogs(ctx);
        self.select_paths_dialog(ctx);
        self.column_customize_dialog(ctx);
        self.external_apps_dialog(ctx);
        self.new_project_dialog(ctx);
        self.share_dictionaries_dialog(ctx);
        self.ai_agents_dialogs(ctx);
        self.chat_window(ctx);
        self.skills_dialogs(ctx);
        self.recovery_frame(ctx);
        self.close_failure_dialog(ctx);
        self.busy_dialog(ctx);
        // Last, so it opens above the dialog that asked for it.
        self.help_window(ctx);
    }
}

/// The status panel while an open project has unsaved changes.
pub(crate) const MODIFIED: &str = "Modified, saved on close";

/// Main window commands with an accelerator that also works while a text
/// field has the keyboard (the field is committed first).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Accel {
    Open,
    CloseAll,
    Save,
    New,
    Print,
    CloseView,
    Refresh,
    Schedule,
}

/// The accelerators of [`Accel`], each with exactly its modifiers: Ctrl+Shift+S
/// is not Ctrl+S, and AltGr (Ctrl+Alt to Windows) is not Ctrl.
const ACCELERATORS: [(egui::Modifiers, Key, Accel); 9] = [
    (egui::Modifiers::COMMAND, Key::O, Accel::Open),
    (egui::Modifiers::COMMAND, Key::W, Accel::CloseAll),
    (egui::Modifiers::COMMAND, Key::S, Accel::Save),
    (egui::Modifiers::NONE, Key::F10, Accel::Save),
    (egui::Modifiers::COMMAND, Key::N, Accel::New),
    (egui::Modifiers::COMMAND, Key::P, Accel::Print),
    (egui::Modifiers::COMMAND, Key::F4, Accel::CloseView),
    (egui::Modifiers::NONE, Key::F5, Accel::Refresh),
    (egui::Modifiers::NONE, Key::F9, Accel::Schedule),
];

fn pressed_accel(ctx: &egui::Context) -> Option<Accel> {
    ACCELERATORS
        .iter()
        .find(|(mods, key, _)| accel_pressed(ctx, *mods, *key))
        .map(|a| a.2)
}

/// `key` was pressed with exactly `mods`.
fn accel_pressed(ctx: &egui::Context, mods: egui::Modifiers, key: Key) -> bool {
    ctx.input(|i| i.key_pressed(key) && i.modifiers.matches_exact(mods))
}

fn deferred_accel_id() -> egui::Id {
    egui::Id::new("openxer_deferred_accelerator")
}
