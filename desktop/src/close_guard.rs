//! Closing OpenXER when the save fails.
//!
//! Closing the window, File > Exit and a close sent by the system all save
//! every open project first. When that save fails the close is cancelled
//! and a message box says which project could not be saved to which
//! workspace file and why, and whether a recovery copy of the changes could
//! be written instead (it is tried at once). The user can try the save
//! again, export the changed projects to XER files in a folder they choose,
//! close without saving (the recovery copy, when there is one, is offered at
//! the next start), or cancel and keep working. Nothing is lost silently.
use super::*;

/// Picks the folder Export writes to (a folder dialog; tests put their own).
pub type FolderPicker = Box<dyn Fn() -> Option<PathBuf>>;

/// A close that the failed save stopped.
pub struct CloseFailure {
    /// Which project, which workspace file, and why.
    pub reason: String,
    /// When the recovery copy was written, or why it could not be.
    pub recovery: Result<String, String>,
    /// Files Export wrote, or why it could not.
    pub exported: Option<Result<Vec<PathBuf>, String>>,
}

pub struct CloseGuard {
    pub failed: Option<CloseFailure>,
    /// Close Without Saving was chosen: the next close goes ahead.
    pub leave: bool,
    pub pick_folder: FolderPicker,
}

impl Default for CloseGuard {
    fn default() -> Self {
        Self {
            failed: None,
            leave: false,
            pick_folder: Box::new(|| rfd::FileDialog::new().pick_folder()),
        }
    }
}

/// The message box's id.
const ID: &str = "close_failed";

impl App {
    /// A close of the window was asked for (its close box, Alt+F4, the
    /// system). Unsaved changes are saved; when that fails the close is
    /// cancelled and the user is asked what to do.
    pub(crate) fn close_requested(&mut self, ctx: &egui::Context) {
        if self.close_guard.leave || !self.any_dirty() {
            return;
        }
        if let Err(e) = self.try_save() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.close_failed(e);
        }
    }

    /// File > Exit: save, then close; a failed save asks first.
    pub(crate) fn exit(&mut self, ctx: &egui::Context) {
        if !self.close_guard.leave && self.any_dirty() {
            if let Err(e) = self.try_save() {
                self.close_failed(e);
                return;
            }
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    /// The save before a close failed with `reason`: keep a recovery copy
    /// if that can be written, and ask.
    fn close_failed(&mut self, reason: String) {
        self.status = reason.clone();
        let recovery = self.write_recovery_copies_now();
        self.close_guard.failed = Some(CloseFailure {
            reason,
            recovery,
            exported: None,
        });
    }

    /// Export: write every open project with unsaved changes to its own XER
    /// file in `dir` (a new name when the file is there already). Returns
    /// the files written.
    pub(crate) fn export_unsaved_to(&mut self, dir: &Path) -> Result<Vec<PathBuf>, String> {
        let shared = self.dictionary_shared();
        let mut projects: Vec<ProjectData> = vec![];
        if self.dirty {
            projects.push(self.project.clone());
        }
        projects.extend(
            self.open_set
                .parked
                .iter()
                .filter(|p| p.dirty)
                .map(|p| p.project.clone()),
        );
        let mut written = vec![];
        for p in &projects {
            let doc = openxer_interchange::xer_export::export_document(
                &[p],
                openxer_interchange::xer_export::XER_VERSIONS[0],
            )?;
            let doc = if shared {
                openxer_xer::workspace_scope::used_subset_of(&doc, &[p.model.project.proj_id])
            } else {
                doc
            };
            let stem = crate::interchange::safe_file_name(&p.model.project.name);
            let mut path = dir.join(format!("{stem}.xer"));
            let mut n = 2;
            while path.exists() {
                path = dir.join(format!("{stem} ({n}).xer"));
                n += 1;
            }
            doc.write(&path)
                .map_err(|e| format!("{} could not be written: {e}", path.display()))?;
            written.push(path);
        }
        Ok(written)
    }

    /// The question asked when the save before a close failed.
    pub(crate) fn close_failure_dialog(&mut self, ctx: &egui::Context) {
        let Some(f) = self.close_guard.failed.as_ref() else {
            return;
        };
        let recovery = match &f.recovery {
            Ok(at) => format!(
                "A recovery copy of the changes was written to the workspace at {at}. If you close without saving, OpenXER offers it the next time it starts."
            ),
            Err(e) => format!(
                "No recovery copy could be written either ({e}). If you close without saving, the changes are lost unless you export them first."
            ),
        };
        let text = format!(
            "OpenXER could not save your changes, so it is still open.\n\n{}\n\n{recovery}\n\nTry Again saves again. Export writes the changed projects to XER files in a folder you choose. Close Without Saving closes OpenXER. Cancel goes back to the changes.",
            f.reason
        );
        let lines: Vec<String> = match &f.exported {
            None => vec![],
            Some(Ok(files)) => files
                .iter()
                .map(|p| format!("Exported to {}", p.display()))
                .collect(),
            Some(Err(e)) => vec![format!("Export failed: {e}")],
        };
        let mut rects = vec![];
        let answer = crate::kit_window::message_box_with_lines(
            ctx,
            ID,
            "OpenXER",
            crate::kit_window::MessageIcon::Error,
            &text,
            &lines,
            &["Try Again", "Export...", "Close Without Saving", "Cancel"],
            crate::kit_window::Question::SaveFailed.default_in(&[
                "Try Again",
                "Export...",
                "Close Without Saving",
                "Cancel",
            ]),
            &mut rects,
        );
        self.record_dialog_actions(rects);
        let escape = crate::kit_window::escape_goes_to(
            ctx,
            egui::Id::new(("kit_message", ID)),
            egui::Order::Foreground,
        );
        match answer {
            Some(0) => {
                self.close_guard.failed = None;
                self.exit(ctx);
            }
            Some(1) => {
                if let Some(dir) = (self.close_guard.pick_folder)() {
                    let exported = self.export_unsaved_to(&dir);
                    if let Some(f) = self.close_guard.failed.as_mut() {
                        f.exported = Some(exported);
                    }
                }
            }
            Some(2) => {
                self.close_guard.failed = None;
                self.close_guard.leave = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Some(_) => self.close_guard.failed = None,
            None if escape => self.close_guard.failed = None,
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::open_set::recovery::left_behind;
    use crate::qa_sweep::Driver;

    fn scheduled_starter() -> ProjectData {
        let mut p = openxer_xer::starter();
        schedule_model(&mut p.model, &p.resources).unwrap();
        p
    }

    /// One frame in which the window is asked to close (`close`) and
    /// `before` runs first; the window commands the app sent.
    fn frame(
        d: &mut Driver,
        close: bool,
        before: impl FnOnce(&mut App, &egui::Context),
    ) -> Vec<egui::ViewportCommand> {
        d.time += 0.1;
        let mut viewports = egui::ViewportIdMap::default();
        viewports.insert(
            egui::ViewportId::ROOT,
            egui::ViewportInfo {
                events: if close {
                    vec![egui::ViewportEvent::Close]
                } else {
                    vec![]
                },
                ..Default::default()
            },
        );
        let app = &mut d.app;
        let mut before = Some(before);
        let output = d.ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, d.screen)),
                time: Some(d.time),
                viewports,
                focused: true,
                ..Default::default()
            },
            |ctx| {
                if let Some(before) = before.take() {
                    before(app, ctx);
                }
                app.update_ui(ctx);
            },
        );
        output
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .map_or(vec![], |v| v.commands.clone())
    }

    fn cancelled(commands: &[egui::ViewportCommand]) -> bool {
        commands.contains(&egui::ViewportCommand::CancelClose)
    }

    fn closes(commands: &[egui::ViewportCommand]) -> bool {
        commands.contains(&egui::ViewportCommand::Close)
    }

    /// Saves to the workspace fail (a trigger on its projects table stands
    /// for a full disk); the rest of the database still takes writes.
    fn fail_saves(d: &Driver) {
        d.app
            .store
            .connection()
            .execute_batch(
                "CREATE TEMP TRIGGER fail_insert BEFORE INSERT ON projects \
                 BEGIN SELECT RAISE(ABORT, 'database or disk is full (test)'); END; \
                 CREATE TEMP TRIGGER fail_update BEFORE UPDATE ON projects \
                 BEGIN SELECT RAISE(ABORT, 'database or disk is full (test)'); END;",
            )
            .unwrap();
    }

    fn mend_saves(d: &Driver) {
        d.app
            .store
            .connection()
            .execute_batch("DROP TRIGGER fail_insert; DROP TRIGGER fail_update;")
            .unwrap();
    }

    fn edit(d: &mut Driver, name: &str) {
        let task = d.app.project.model.activities[0].task_id;
        d.app.project.details.get_mut(&task).unwrap().name = name.into();
        d.app.dirty = true;
    }

    fn first_name(p: &ProjectData) -> String {
        p.details[&p.model.activities[0].task_id].name.clone()
    }

    /// The close box with a save that fails: the close is cancelled, the
    /// message names the project, the workspace file and the reason, and a
    /// recovery copy holds the change. Export writes it to a folder; Try
    /// Again saves once saving works again, and the window then closes.
    #[test]
    fn a_close_whose_save_fails_is_cancelled_and_nothing_is_lost() {
        let mut d = Driver::new("close-save-fails", scheduled_starter());
        edit(&mut d, "Changed before closing");
        fail_saves(&d);
        let commands = frame(&mut d, true, |_, _| {});
        assert!(cancelled(&commands), "the close is cancelled");
        let f = d
            .app
            .close_guard
            .failed
            .as_ref()
            .expect("the user is asked");
        let workspace = d.app.store.path.display().to_string();
        assert!(
            f.reason.contains(&d.app.project.model.project.name)
                && f.reason.contains(&workspace)
                && f.reason.contains("disk is full"),
            "names the project, the file and the reason: {}",
            f.reason
        );
        assert!(f.recovery.is_ok(), "{:?}", f.recovery);
        let copies = left_behind(d.app.store.connection(), "a later run").unwrap();
        assert_eq!(copies.len(), 1);
        assert!(copies[0].text.contains("Changed before closing"));
        assert!(d.app.dirty, "the change is still open");
        d.frames(2);
        for button in [
            "close_failed_try again",
            "close_failed_export...",
            "close_failed_close without saving",
            "close_failed_cancel",
        ] {
            assert!(d.has(button), "{button}");
        }
        // Export writes the changed project to the folder chosen.
        let out = d.dir.join("exported");
        std::fs::create_dir_all(&out).unwrap();
        let chosen = out.clone();
        d.app.close_guard.pick_folder = Box::new(move || Some(chosen.clone()));
        d.click("close_failed_export...");
        let f = d.app.close_guard.failed.as_ref().expect("still asked");
        let files = f.exported.clone().unwrap().unwrap();
        assert_eq!(files.len(), 1);
        let doc = Document::read(&files[0]).unwrap();
        let pid = doc.projects()[0].0;
        let exported = ProjectData::from_document(doc, pid, "x".into()).unwrap();
        assert_eq!(first_name(&exported), "Changed before closing");
        // Try Again while saving still fails asks again.
        d.click("close_failed_try again");
        assert!(d.app.close_guard.failed.is_some());
        assert!(d.app.dirty);
        // Saving works again: Try Again saves and closes.
        mend_saves(&d);
        d.click("close_failed_try again");
        assert!(d.app.close_guard.failed.is_none());
        assert!(!d.app.any_dirty());
        let id = d.app.open_set.active.unwrap();
        assert_eq!(
            first_name(&d.app.store.load(id).unwrap()),
            "Changed before closing"
        );
        let commands = frame(&mut d, true, |_, _| {});
        assert!(!cancelled(&commands), "saved: the window closes");
        assert!(
            left_behind(d.app.store.connection(), "a later run")
                .unwrap()
                .is_empty(),
            "the recovery copy goes once everything is saved"
        );
    }

    /// Close Without Saving closes, and the recovery copy is offered at the
    /// next start. Cancel and Escape leave OpenXER open with the change.
    #[test]
    fn close_without_saving_keeps_the_recovery_copy_and_cancel_stays() {
        let mut d = Driver::new("close-without-saving", scheduled_starter());
        edit(&mut d, "Kept in the recovery copy");
        fail_saves(&d);
        for answer in ["cancel", "escape"] {
            let commands = frame(&mut d, true, |_, _| {});
            assert!(cancelled(&commands));
            d.frames(2);
            if answer == "escape" {
                d.key(Key::Escape);
            } else {
                d.click("close_failed_cancel");
            }
            assert!(d.app.close_guard.failed.is_none(), "{answer} closes it");
            assert!(!d.app.close_guard.leave);
            assert!(d.app.dirty, "{answer} keeps the change");
        }
        let commands = frame(&mut d, true, |_, _| {});
        assert!(cancelled(&commands));
        d.frames(2);
        d.click("close_failed_close without saving");
        assert!(d.app.close_guard.leave);
        let commands = frame(&mut d, true, |_, _| {});
        assert!(!cancelled(&commands), "the window closes");
        let store = Store::open(d.dir.join("workspace.db")).unwrap();
        let copies = left_behind(store.connection(), "the next run").unwrap();
        assert_eq!(copies.len(), 1, "offered at the next start");
        assert!(copies[0].text.contains("Kept in the recovery copy"));
    }

    /// Enter in the failed-save box answers Try Again, the button framed as
    /// the default, never Close Without Saving.
    #[test]
    fn enter_in_the_failed_save_box_tries_again() {
        let mut d = Driver::new("close-save-enter", scheduled_starter());
        edit(&mut d, "Kept by Enter");
        fail_saves(&d);
        let commands = frame(&mut d, true, |_, _| {});
        assert!(cancelled(&commands));
        d.frames(2);
        assert_eq!(
            crate::dialog_keys::top_default_label(&d.ctx).as_deref(),
            Some("Try Again")
        );
        d.key(Key::Enter);
        assert!(
            !d.app.close_guard.leave,
            "Enter never closes without saving"
        );
        assert!(d.app.close_guard.failed.is_some(), "saving failed again");
        assert!(d.app.dirty);
        mend_saves(&d);
        d.frames(2);
        d.key(Key::Enter);
        assert!(d.app.close_guard.failed.is_none());
        assert!(!d.app.any_dirty(), "Try Again saved");
        let id = d.app.open_set.active.unwrap();
        assert_eq!(first_name(&d.app.store.load(id).unwrap()), "Kept by Enter");
    }

    /// Close All, Close Project and opening another project save first; when
    /// that fails nothing closes, the error names the project, the workspace
    /// file and the reason, and the changes stay open to try again or export.
    #[test]
    fn closing_or_switching_projects_with_a_failing_save_keeps_everything_open() {
        let mut d = Driver::new("close-projects-save-fails", scheduled_starter());
        let a = d.app.open_set.active.unwrap();
        let mut b = scheduled_starter();
        b.source_name = d.dir.join("bravo.xer").display().to_string();
        b.model.project.name = "BRAVO".into();
        let b = d.app.store.save(&b).unwrap();
        d.app.open_project_set(&[a, b]);
        assert_eq!(d.app.open_ids().len(), 2);
        let k = d
            .app
            .open_set
            .parked
            .iter()
            .position(|p| p.id == b)
            .unwrap();
        let task = d.app.open_set.parked[k].project.model.activities[0].task_id;
        d.app.open_set.parked[k]
            .project
            .details
            .get_mut(&task)
            .unwrap()
            .name = "Unsaved in BRAVO".into();
        d.app.open_set.parked[k].dirty = true;
        fail_saves(&d);
        let workspace = d.app.store.path.display().to_string();
        type Attempt = (&'static str, fn(&mut App, i64));
        let attempts: [Attempt; 4] = [
            ("Close All", |app, _| app.close_all_projects()),
            ("Close Project", |app, b| app.close_project_id(b)),
            ("Close the active project", |app, _| {
                app.dirty = true;
                app.close_active_project()
            }),
            ("Open another project", |app, a| app.open_project_set(&[a])),
        ];
        for (what, attempt) in attempts {
            d.app.error = None;
            attempt(
                &mut d.app,
                if what == "Open another project" { a } else { b },
            );
            let error = d.app.error.clone().unwrap_or_default();
            assert!(
                error.contains(&workspace) && error.contains("disk is full"),
                "{what}: {error}"
            );
            assert_eq!(d.app.open_ids().len(), 2, "{what}: nothing closes");
            assert!(
                d.app
                    .open_or_stored(b)
                    .unwrap()
                    .details
                    .values()
                    .any(|x| x.name == "Unsaved in BRAVO"),
                "{what}: the change stays open"
            );
            assert!(d.app.any_dirty(), "{what}");
        }
        mend_saves(&d);
        d.app.close_all_projects();
        assert_eq!(d.app.open_ids().len(), 1);
        assert!(d
            .app
            .store
            .load(b)
            .unwrap()
            .details
            .values()
            .any(|x| x.name == "Unsaved in BRAVO"));
    }

    /// File > Exit with the workspace locked by another program: nothing
    /// closes, and the message says no recovery copy could be written either,
    /// so the changes must be exported or kept open.
    #[test]
    fn exit_with_a_locked_workspace_asks_and_says_no_copy_was_kept() {
        let mut d = Driver::new("exit-locked", scheduled_starter());
        edit(&mut d, "Changed while locked");
        let blocker = rusqlite::Connection::open(d.dir.join("workspace.db")).unwrap();
        blocker.execute_batch("BEGIN EXCLUSIVE").unwrap();
        d.app
            .store
            .connection()
            .busy_timeout(std::time::Duration::ZERO)
            .unwrap();
        let commands = frame(&mut d, false, |app, ctx| app.exit(ctx));
        assert!(!closes(&commands), "Exit does not close");
        let f = d.app.close_guard.failed.as_ref().expect("asked");
        assert!(f.reason.contains("locked"), "{}", f.reason);
        assert!(f.recovery.is_err(), "no copy: {:?}", f.recovery);
        d.frames(2);
        let shown = d.texts();
        assert!(
            shown
                .iter()
                .any(|(t, _)| t.contains("changes are lost unless you export them")),
            "the message says so"
        );
        blocker.execute_batch("ROLLBACK").unwrap();
        d.click("close_failed_try again");
        assert!(d.app.close_guard.failed.is_none());
        assert!(!d.app.any_dirty(), "{:?}", d.app.error);
    }
}
