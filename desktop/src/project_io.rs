use super::*;
impl App {
    /// Save every open project to the workspace; a failure is shown in the
    /// error box and the changes stay open (and unsaved).
    pub(super) fn save(&mut self) {
        if let Err(e) = self.try_save() {
            self.error(e);
        }
    }
    /// What a failed save of project `name` says: the project, the
    /// workspace file and the reason.
    pub(crate) fn save_failure(&self, name: &str, reason: &str) -> String {
        format!(
            "{name} could not be saved to the workspace {}: {reason}",
            self.store.path.display()
        )
    }
    /// [`App::save`], returning the failure instead of showing it.
    pub(crate) fn try_save(&mut self) -> Result<(), String> {
        self.save_view();
        if self.open_set.empty {
            self.dirty = false;
            return Ok(());
        }
        // Other open projects first, so the active one is the workspace's last project.
        self.save_parked()?;
        match self.store.save_stored(&self.project) {
            Ok((id, stored)) => {
                // Rows the merge renumbered or matched: the project now
                // refers to them by the ids the workspace stored.
                if let Some(doc) = stored {
                    self.take_document(doc);
                }
                self.dirty = false;
                self.open_set.active = Some(id);
                self.drop_stale_recovery_copies();
                self.status = "Saved to workspace".into();
                self.saved_projects = self.store.list().unwrap_or_default();
                if self.open_set.multi() {
                    self.save_open_set();
                }
                if !self.any_dirty() {
                    self.clear_recovery_copies();
                }
                Ok(())
            }
            Err(e) => {
                self.dirty = true;
                // The other open projects were saved: their copies go.
                self.drop_stale_recovery_copies();
                Err(self.save_failure(&self.project.model.project.name, &e))
            }
        }
    }
    pub(super) fn load(&mut self, p: ProjectData) {
        self.load_from(p, false);
    }
    /// Open a project read from a file (File > Open, a project picked from a
    /// multi-project file). It may have the source name and project key of
    /// a workspace project: the workspace copy's rows are not its rows.
    pub(crate) fn load_file(&mut self, p: ProjectData) {
        self.load_from(p, true);
    }
    fn load_from(&mut self, p: ProjectData, file: bool) {
        if !self.open_set.empty {
            if self.any_dirty() {
                self.save();
                if self.any_dirty() {
                    return;
                }
            }
            self.keep_closing_history();
            // Loading one project replaces the open set.
            self.close_parked();
        }
        self.open_set.empty = false;
        if file {
            // Only now: the projects saved and closed above still needed it.
            self.store
                .forget_mark(&(p.source_name.clone(), p.model.project.proj_id));
        }
        self.forget_synced(&p);
        self.project = p;
        self.open_set.active = None;
        self.history = history::History::new(&self.project);
        self.open_set.steps_seen = 0;
        self.records_revision = 0;
        self.detail.picker = None;
        self.link_drag = None;
        self.link_editor = None;
        self.edit_cell = None;
        self.selected = (!self.project.model.activities.is_empty()).then_some(0);
        self.query.clear();
        self.wbs_filter = None;
        self.collapsed.clear();
        self.date_edits.clear();
        self.data_date = date_string(
            self.project.model.origin_serial,
            self.project.model.project.data_date,
        );
        self.must_finish = date_string(
            self.project.model.origin_serial,
            self.project.model.project.plan_end,
        );
        self.status = format!("{} activities loaded", self.project.model.activities.len());
        self.fit_project();
        self.save();
        self.restore_closed_history();
        self.open_set.default = self.open_set.active;
        self.save_open_set();
        self.note_loops_on_open();
        self.repair_add_sample_wbs();
    }
    /// Open project `p` read from a file. When the workspace already has a
    /// copy of it (same file and project), nothing is replaced yet: the
    /// user is asked whether to open that copy or replace it with the file.
    pub(crate) fn open_file_project(&mut self, p: ProjectData) {
        match self.store.id_of(&p.source_name, p.model.project.proj_id) {
            Some(id) => self.replace_prompt = Some((p, id)),
            None => {
                self.load_file(p);
                self.adopt_file_baselines();
            }
        }
    }
    /// The answer to [`App::open_file_project`]'s question: 0 opens the
    /// workspace copy, 1 replaces it with the file after copying the
    /// workspace into backups, anything else leaves everything unchanged.
    pub(crate) fn answer_replace_prompt(&mut self, answer: usize) {
        let Some((p, id)) = self.replace_prompt.take() else {
            return;
        };
        if answer > 1 {
            return;
        }
        // Unsaved changes are saved first: the copy opened must hold them,
        // and the backup taken before replacing must too.
        if self.any_dirty() {
            self.save();
            if self.any_dirty() {
                return;
            }
        }
        match answer {
            0 => match self.store.load(id) {
                Ok(copy) => self.load(copy),
                Err(e) => self.error(e),
            },
            _ => match crate::interchange::workspace::backup(&self.store) {
                Ok(path) => {
                    self.load_file(p);
                    self.adopt_file_baselines();
                    let file = path
                        .file_name()
                        .map_or(String::new(), |f| f.to_string_lossy().into_owned());
                    self.status = format!(
                            "The workspace copy was replaced by the file; the workspace as it was is kept in backups\\{file}"
                        );
                }
                Err(e) => self.error(format!("Nothing was replaced: {e}")),
            },
        }
    }
    /// The question [`App::open_file_project`] asks.
    pub(crate) fn replace_prompt_dialog(&mut self, ctx: &egui::Context) {
        let Some((p, id)) = &self.replace_prompt else {
            return;
        };
        let title = self
            .saved_projects
            .iter()
            .find(|(i, _)| i == id)
            .map_or_else(|| p.model.project.name.clone(), |(_, t)| t.clone());
        let text = format!(
            "The workspace already has a copy of {title} from {}, with any changes saved to it since.\n\nOpen Copy opens the workspace copy. Replace puts the file in its place; the workspace is copied into backups first. Cancel leaves everything as it is.",
            p.source_name
        );
        let mut rects = vec![];
        let answer = crate::kit_window::message_box(
            ctx,
            "replace_copy",
            "Open",
            crate::kit_window::MessageIcon::Question,
            &text,
            &["Open Copy", "Replace", "Cancel"],
            crate::kit_window::Question::ReplaceCopy.default_in(&[
                "Open Copy",
                "Replace",
                "Cancel",
            ]),
            &mut rects,
        );
        self.record_dialog_actions(rects);
        if let Some(answer) = answer {
            self.answer_replace_prompt(answer);
        } else if ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.answer_replace_prompt(2);
        }
    }
    pub(super) fn open(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("XER files", &["xer"])
            .pick_file()
        {
            self.open_path(path);
        }
    }
    pub(crate) fn open_path(&mut self, path: PathBuf) {
        match Document::read(&path) {
            Ok(doc) => {
                let projects = openxer_interchange::baseline::schedule_projects(&doc);
                if projects.len() == 1 {
                    match ProjectData::from_document(doc, projects[0].0, path.display().to_string())
                    {
                        Ok(p) => self.open_file_project(p),
                        Err(e) => self.error(e),
                    }
                } else if projects.is_empty() {
                    self.error("No projects found in this XER".into());
                } else {
                    self.pending = Some((doc, path.display().to_string()));
                }
            }
            Err(e) => self.error(e),
        }
    }
    /// Where Add puts a new activity: switches to the Default Project, then
    /// the WBS (a selected band row's, the WBS filter's, the selected
    /// activity's or the first), the selected activity that numbers it and
    /// a selected band row's code values. Add and the New Activity wizard
    /// both start here.
    pub(crate) fn new_activity_place(&mut self) -> crate::activity_wizard::NewActivityPlace {
        // New activities go to the Default Project.
        let before = self.open_set.active;
        self.activate_default();
        if self.open_set.active != before {
            self.wbs_filter = None;
        }
        // A selected band row gives the new activity its WBS and code values.
        let (band_wbs, codes) = self
            .band_assignments()
            .filter(|_| self.open_set.active == before)
            .unwrap_or_default();
        let wbs = band_wbs
            .or(self.wbs_filter)
            .or_else(|| {
                self.selected.and_then(|i| {
                    self.project
                        .details
                        .get(&self.project.model.activities[i].task_id)
                        .map(|d| d.wbs)
                })
            })
            .or_else(|| self.project.wbs.first().map(|w| w.id))
            .unwrap_or(0);
        // The selected activity numbers the new one when the project's
        // Defaults increment from the selection.
        let selected = self.selected.filter(|_| self.open_set.active == before);
        crate::activity_wizard::NewActivityPlace {
            wbs,
            selected,
            codes,
        }
    }

    /// Insert a new activity at `place` with the project's defaults (the
    /// shared `add_activity`), selected and shown. Returns its index.
    pub(crate) fn insert_new_activity(
        &mut self,
        place: &crate::activity_wizard::NewActivityPlace,
    ) -> usize {
        let i = openxer_xer::edit::new_activity::add_activity(
            &mut self.project,
            place.wbs,
            place.selected,
        );
        self.unique_new_activity(i);
        for &(type_id, value) in &place.codes {
            let edit = crate::details::setters::Edit::Code {
                type_id,
                value: Some(value),
            };
            if let Err(e) = self.edit_activity(i, edit) {
                self.error(e);
            }
        }
        self.grid.cells.selected_band = None;
        self.selected = Some(i);
        self.query.clear();
        self.reveal_activity(i);
        self.dirty = true;
        if self.detail.tabs.contains(&DetailTab::General) {
            self.detail.tab = DetailTab::General;
        }
        i
    }

    /// Add without the wizard: the row goes in at once and its name cell
    /// opens for typing.
    pub(crate) fn new_activity(&mut self) {
        if self.open_set.empty {
            self.status = "Open or create a project first.".into();
            return;
        }
        let place = self.new_activity_place();
        let i = self.insert_new_activity(&place);
        // The name cell opens with the new name selected: typing replaces
        // it and Enter keeps it.
        let catalog = grid::columns::catalog(&self.project);
        let name = self
            .visible_columns(&catalog)
            .into_iter()
            .find(|c| c.id == "activity_name");
        if let (Some(column), true) = (name, self.views.main.is_activities()) {
            self.begin_edit(i, &column);
            self.grid.cells.select_all = self.edit_cell.is_some();
        }
    }
    pub(super) fn history_move(&mut self, ctx: &egui::Context, redo: bool) {
        self.history.record(
            &self.project,
            self.records_revision,
            None,
            ctx.input(|i| i.time),
        );
        self.sync_journal();
        if self.open_set.multi() && self.journal_move(ctx, redo) {
            return;
        }
        self.active_history_step(ctx, redo);
    }
    /// Undo or redo one step of the active project; true when it changed.
    pub(super) fn active_history_step(&mut self, _ctx: &egui::Context, redo: bool) -> bool {
        let external = if redo {
            self.history.redo_external()
        } else {
            self.history.undo_external()
        };
        // The selection follows the activity, not its row index, which
        // shifts when the step adds or removes activities before it.
        let selected_task = self
            .selected
            .and_then(|i| self.project.model.activities.get(i))
            .map(|a| a.task_id);
        let changed = if redo {
            self.history.redo(&mut self.project)
        } else {
            self.history.undo(&mut self.project)
        };
        let mut undone = external.filter(|_| changed);
        // A dictionary change runs first; the step's other workspace change
        // (if any) after it.
        if let Some(also) = undone.and_then(|id| self.dictionary_history(id, redo)) {
            undone = also;
        }
        if let Some(id) = undone {
            if !self.enterprise_history(id, redo) {
                self.baseline_history(id, redo);
            }
        }
        if changed {
            self.records_revision = self.history.revision();
            self.dirty = !self.open_set.empty;
            self.status = if redo { "Redo" } else { "Undo" }.into();
            self.date_edits.clear();
            self.edit_cell = None;
            self.link_drag = None;
            self.link_editor = None;
            let activities = &self.project.model.activities;
            self.selected = selected_task
                .and_then(|id| activities.iter().position(|a| a.task_id == id))
                .or(self.selected.filter(|&i| i < activities.len()));
            self.data_date = date_string(
                self.project.model.origin_serial,
                self.project.model.project.data_date,
            );
            self.must_finish = date_string(
                self.project.model.origin_serial,
                self.project.model.project.plan_end,
            );
        }
        if changed {
            self.settle_dictionary();
        }
        match (undone, redo) {
            (Some(id), false) => self.leave_removed_project(id),
            (Some(id), true) => self.enter_recreated_project(id),
            _ => {}
        }
        changed
    }
}
