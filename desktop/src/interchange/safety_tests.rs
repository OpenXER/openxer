//! Import and Undo Last Import never lose work: an import into an open
//! project merges its open copy, unsaved changes and all, and the open copy
//! then holds what the import wrote; Undo Last Import asks first, says which
//! projects lose changes, saves and backs up the workspace before it undoes,
//! and the open copies take the rows it put back.
use super::*;
use crate::qa_sweep::Driver;

fn scheduled_starter() -> ProjectData {
    let mut p = openxer_xer::starter();
    schedule_model(&mut p.model, &p.resources).unwrap();
    p
}

/// Activity names of `p`, by task id.
fn name(p: &ProjectData, task: i64) -> String {
    p.details[&task].name.clone()
}

fn has_activity_named(p: &ProjectData, wanted: &str) -> bool {
    p.details.values().any(|d| d.name == wanted)
}

/// A second workspace project, BRAVO, from its own file name.
fn bravo(d: &mut Driver) -> (ProjectData, i64) {
    let mut b = scheduled_starter();
    b.source_name = d.dir.join("bravo.xer").display().to_string();
    b.model.project.name = "BRAVO".into();
    let id = d.app.store.save(&b).unwrap();
    (b, id)
}

/// Write `p` with activity `task` renamed to `to` as an XER file.
fn update_file(d: &Driver, p: &ProjectData, task: i64, to: &str) -> PathBuf {
    let path = d.dir.join(format!("update-{task}.xer"));
    let mut edited = p.clone();
    edited.details.get_mut(&task).unwrap().name = to.into();
    edited.updated_document().write(&path).unwrap();
    path
}

/// File > Import of `file` with Update Existing into workspace project
/// `target`, through the wizard's own steps.
fn import_update(app: &mut App, file: &Path, target: i64, open_after: bool) {
    app.open_import_wizard();
    app.interchange.xer.as_mut().unwrap().path_text = file.display().to_string();
    app.xer_load_files();
    {
        let w = app.interchange.xer.as_mut().unwrap();
        w.rows[0].action = ImportAction::UpdateExisting;
        w.rows[0].target = Some(target);
        w.open_after = open_after;
        assert!(App::xer_validate_rows(w).is_none());
    }
    app.xer_prepare();
    let w = app.interchange.xer.as_ref().unwrap();
    assert!(
        w.prepared.iter().all(|p| p.error.is_none()),
        "{:?}",
        w.prepared.iter().map(|p| &p.error).collect::<Vec<_>>()
    );
    app.xer_finish();
    assert!(app.interchange.xer.is_none(), "{:?}", app.error);
    app.interchange.message = None;
}

fn backups_named(d: &Driver, prefix: &str) -> Vec<PathBuf> {
    std::fs::read_dir(d.dir.join("backups")).map_or(vec![], |r| {
        r.flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with(prefix))
            })
            .collect()
    })
}

/// Finding 1.3: BRAVO is open beside ALPHA with an unsaved new activity.
/// Importing an update into BRAVO keeps that activity (the merge read the
/// open copy), the open copy holds the update, and a later save of BRAVO
/// writes both.
#[test]
fn an_import_into_an_open_project_that_is_not_active_keeps_its_unsaved_changes() {
    let mut d = Driver::new("import-parked", scheduled_starter());
    let a = d.app.open_set.active.unwrap();
    let (b, b_id) = bravo(&mut d);
    d.app.open_project_set(&[a, b_id]);
    assert_eq!(d.app.open_set.active, Some(a));
    let k = d
        .app
        .open_set
        .parked
        .iter()
        .position(|p| p.id == b_id)
        .unwrap();
    {
        let p = &mut d.app.open_set.parked[k];
        let wbs = p.project.wbs[0].id;
        let i = p.project.add_activity(wbs);
        let task = p.project.model.activities[i].task_id;
        p.project.details.get_mut(&task).unwrap().name = "Added in BRAVO, not saved".into();
        p.dirty = true;
    }
    let file = update_file(&d, &b, 1020, "From the contractor update");
    import_update(&mut d.app, &file, b_id, false);
    assert_eq!(d.app.open_set.active, Some(a), "ALPHA stays active");
    let stored = d.app.store.load(b_id).unwrap();
    assert!(
        has_activity_named(&stored, "Added in BRAVO, not saved"),
        "the unsaved change is in what the import wrote"
    );
    assert_eq!(name(&stored, 1020), "From the contractor update");
    let open = d.app.open_or_stored(b_id).unwrap();
    assert!(has_activity_named(&open, "Added in BRAVO, not saved"));
    assert_eq!(
        name(&open, 1020),
        "From the contractor update",
        "the open copy holds what the import wrote"
    );
    // Saving BRAVO again (it is changed later) keeps the update.
    d.app.open_set.parked[k].dirty = true;
    d.app.save();
    assert!(!d.app.any_dirty(), "{:?}", d.app.error);
    let stored = d.app.store.load(b_id).unwrap();
    assert_eq!(name(&stored, 1020), "From the contractor update");
    assert!(has_activity_named(&stored, "Added in BRAVO, not saved"));
}

/// Finding 1.2: Undo Last Import asks first. Cancel and Escape change
/// nothing and take no backup; the question names the project and says when
/// it was changed after the import; Undo Import backs the workspace up (the
/// backup holds the change made since) and then undoes.
#[test]
fn undo_last_import_asks_first_and_backs_up_what_it_takes_back() {
    let mut d = Driver::new("undo-import-asks", scheduled_starter());
    let a = d.app.open_set.active.unwrap();
    let before = name(&d.app.project, 1020);
    let file = update_file(&d, &d.app.project.clone(), 1020, "Imported name");
    import_update(&mut d.app, &file, a, true);
    assert_eq!(name(&d.app.store.load(a).unwrap(), 1020), "Imported name");
    // A save of the unchanged project is no change since the import.
    d.app.save();
    d.app.open_import_wizard();
    d.frames(3);
    d.click("undo_last_import");
    d.frames(2);
    let prompt = d.app.interchange.undo_prompt.as_ref().expect("it asks");
    assert!(
        prompt.lines.iter().any(|l| l.contains("goes back")),
        "{:?}",
        prompt.lines
    );
    assert!(
        !prompt.lines.iter().any(|l| l.contains("changed after")),
        "{:?}",
        prompt.lines
    );
    assert!(d.has("undo_import_undo import") && d.has("undo_import_cancel"));
    for answer in ["undo_import_cancel", "escape"] {
        if answer == "escape" {
            d.click("undo_last_import");
            d.frames(2);
            assert!(d.app.interchange.undo_prompt.is_some());
            d.key(Key::Escape);
        } else {
            d.click(answer);
        }
        d.frames(2);
        assert!(
            d.app.interchange.undo_prompt.is_none(),
            "{answer} closes it"
        );
        assert!(d.app.interchange.xer.is_some(), "{answer} keeps the wizard");
        assert_eq!(name(&d.app.store.load(a).unwrap(), 1020), "Imported name");
        assert!(backups_named(&d, workspace::UNDO_BACKUP).is_empty());
    }
    // A change made after the import, saved, and one not saved yet.
    d.app.project.details.get_mut(&1030).unwrap().name = "Changed after the import".into();
    d.app.dirty = true;
    d.app.save();
    d.app.project.details.get_mut(&1040).unwrap().name = "Not saved yet".into();
    d.app.dirty = true;
    d.click("undo_last_import");
    d.frames(2);
    let prompt = d.app.interchange.undo_prompt.as_ref().expect("it asks");
    assert!(
        prompt.lines.iter().any(|l| l.contains("unsaved changes")),
        "{:?}",
        prompt.lines
    );
    assert!(prompt.text.contains("lost"), "{}", prompt.text);
    d.click("undo_import_undo import");
    d.frames(2);
    assert!(d.app.interchange.undo_prompt.is_none());
    assert_eq!(name(&d.app.store.load(a).unwrap(), 1020), before);
    assert_eq!(name(&d.app.project, 1020), before, "the open copy too");
    assert!(!d.app.any_dirty());
    let taken = backups_named(&d, workspace::UNDO_BACKUP);
    assert_eq!(taken.len(), 1, "one backup before the undo");
    let old = Store::open(taken[0].clone()).unwrap();
    let kept = old.load(a).unwrap();
    assert_eq!(name(&kept, 1020), "Imported name");
    assert_eq!(name(&kept, 1030), "Changed after the import");
    assert_eq!(
        name(&kept, 1040),
        "Not saved yet",
        "unsaved changes are saved before the backup"
    );
}

/// Enter in the Undo Last Import question answers Cancel, the button framed
/// as the default: the import stays and no backup is taken.
#[test]
fn enter_in_the_undo_import_question_cancels() {
    let mut d = Driver::new("undo-import-enter", scheduled_starter());
    let a = d.app.open_set.active.unwrap();
    let file = update_file(&d, &d.app.project.clone(), 1020, "Imported name");
    import_update(&mut d.app, &file, a, true);
    d.app.save();
    d.app.open_import_wizard();
    d.frames(3);
    d.click("undo_last_import");
    d.frames(2);
    assert!(d.app.interchange.undo_prompt.is_some(), "it asks");
    assert_eq!(
        crate::dialog_keys::top_default_label(&d.ctx).as_deref(),
        Some("Cancel")
    );
    d.key(Key::Enter);
    d.frames(2);
    assert!(d.app.interchange.undo_prompt.is_none(), "Enter answers");
    assert!(d.app.interchange.xer.is_some(), "the wizard stays");
    assert_eq!(name(&d.app.store.load(a).unwrap(), 1020), "Imported name");
    assert!(backups_named(&d, workspace::UNDO_BACKUP).is_empty());
}

/// Finding 1.2 with an open project that is not active: Undo Last Import
/// puts BRAVO's open copy back too, so saving it later never writes the
/// imported rows back.
#[test]
fn undo_last_import_puts_back_an_open_project_that_is_not_active() {
    let mut d = Driver::new("undo-import-parked", scheduled_starter());
    let a = d.app.open_set.active.unwrap();
    let (b, b_id) = bravo(&mut d);
    d.app.open_project_set(&[a, b_id]);
    let before = name(&b, 1020);
    let file = update_file(&d, &b, 1020, "Imported into BRAVO");
    import_update(&mut d.app, &file, b_id, false);
    assert_eq!(
        name(&d.app.open_or_stored(b_id).unwrap(), 1020),
        "Imported into BRAVO"
    );
    d.app.undo_last_import();
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
    assert_eq!(name(&d.app.store.load(b_id).unwrap(), 1020), before);
    assert_eq!(
        name(&d.app.open_or_stored(b_id).unwrap(), 1020),
        before,
        "the open copy is put back"
    );
    // BRAVO is changed and saved later: the import stays undone.
    let k = d
        .app
        .open_set
        .parked
        .iter()
        .position(|p| p.id == b_id)
        .unwrap();
    d.app.open_set.parked[k].dirty = true;
    d.app.save();
    assert_eq!(name(&d.app.store.load(b_id).unwrap(), 1020), before);
}

/// Undo Last Import does nothing when the open projects cannot be saved
/// first: the workspace holds the import still, and the unsaved change is
/// still open.
#[test]
fn undo_last_import_waits_for_a_save_that_fails() {
    let mut d = Driver::new("undo-import-save-fails", scheduled_starter());
    let a = d.app.open_set.active.unwrap();
    let file = update_file(&d, &d.app.project.clone(), 1020, "Imported name");
    import_update(&mut d.app, &file, a, true);
    d.app.project.details.get_mut(&1030).unwrap().name = "Not saved yet".into();
    d.app.dirty = true;
    // Another program holds the workspace's write lock.
    let path = d.dir.join("workspace.db");
    let blocker = rusqlite::Connection::open(&path).unwrap();
    blocker.execute_batch("BEGIN EXCLUSIVE").unwrap();
    d.app
        .store
        .connection()
        .busy_timeout(std::time::Duration::ZERO)
        .unwrap();
    d.app.undo_last_import();
    let error = d.app.error.clone().expect("explained");
    assert!(error.contains("not undone"), "{error}");
    assert!(d.app.dirty, "the change is still open");
    assert_eq!(name(&d.app.project, 1030), "Not saved yet");
    blocker.execute_batch("ROLLBACK").unwrap();
    assert_eq!(name(&d.app.store.load(a).unwrap(), 1020), "Imported name");
    assert!(backups_named(&d, workspace::UNDO_BACKUP).is_empty());
}
