//! Data-safety checks of a shared-dictionary workspace in the app: files
//! opened into it keep their own calendars and resources, opening a file
//! never rewrites or deletes dictionary rows other projects hold, undo never
//! reverts someone else's change, and a crash between an edit and a save
//! leaves the stored projects consistent.
use super::*;
use crate::qa_sweep::Driver;
use openxer_xer::edit::{calendars, tables};

fn named(source: &str, id: &str) -> ProjectData {
    let mut p = openxer_xer::starter();
    p.source_name = source.into();
    p.model.project.name = id.into();
    p
}

fn temp_file(name: &str) -> PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "openxer-safety-{name}-{}-{unique}.xer",
        std::process::id()
    ))
}

/// The calendar name of every activity, through the document as it would
/// be saved.
fn task_calendars(p: &ProjectData) -> Vec<String> {
    let doc = p.updated_document();
    let tasks = doc.table("TASK").unwrap();
    tasks
        .rows
        .iter()
        .map(|r| calendars::name(&doc, tasks.value(r, "clndr_id")))
        .collect()
}

fn calendar_named(doc: &Document, name: &str) -> Option<String> {
    let t = doc.table("CALENDAR")?;
    t.rows
        .iter()
        .find(|r| t.value(r, "clndr_name") == name)
        .map(|r| t.value(r, "clndr_id").to_string())
}

fn rsrc_named(doc: &Document, short: &str) -> bool {
    doc.table("RSRC").is_some_and(|t| {
        t.rows
            .iter()
            .any(|r| t.value(r, "rsrc_short_name") == short)
    })
}

/// A starter project whose every activity runs on a global calendar named
/// `name`, written to an XER file. Its calendar id is the starter's next
/// id, the same one a calendar added to a fresh workspace project gets.
fn file_on_calendar(name: &str) -> (PathBuf, ProjectData) {
    let path = temp_file("open");
    let mut f = named(&path.display().to_string(), "FOXTROT");
    let id = calendars::add(&mut f, calendars::Kind::Global, None, "2026-11-08 09:00").unwrap();
    calendars::rename(&mut f, &id, name).unwrap();
    let mut doc = f.updated_document();
    let tasks = doc.table_mut("TASK");
    let c = tasks.field("clndr_id").unwrap();
    for r in &mut tasks.rows {
        r[c] = id.clone();
    }
    doc.write(&path).unwrap();
    let f = ProjectData::from_document(
        Document::read(&path).unwrap(),
        1,
        path.display().to_string(),
    )
    .unwrap();
    (path, f)
}

/// File > Open of a path the workspace already holds asks first; this
/// answers that the file replaces the workspace copy.
fn replace_with_file(d: &mut Driver) {
    d.frames(2);
    assert!(d.app.replace_prompt.is_some(), "File > Open asks first");
    d.click("replace_copy_replace");
    d.frames(2);
    assert!(d.app.replace_prompt.is_none());
}

/// A workspace project that came from file `path` (written to it), with an
/// activity renamed and saved in the workspace afterwards.
fn edited_copy(name: &str) -> (Driver, PathBuf, i64) {
    let path = temp_file(name);
    let original = named(&path.display().to_string(), "ALPHA");
    original.updated_document().write(&path).unwrap();
    let mut d = Driver::new(name, original);
    let id = d.app.open_set.active.unwrap();
    d.app.project.details.get_mut(&1010).unwrap().name = "Edited in the workspace".into();
    d.app.dirty = true;
    d.app.save();
    (d, path, id)
}

fn stored_name(d: &Driver, id: i64) -> String {
    d.app.store.load(id).unwrap().details[&1010].name.clone()
}

fn backups(d: &Driver) -> usize {
    std::fs::read_dir(d.app.store.path.parent().unwrap().join("backups")).map_or(0, |r| r.count())
}

/// B6: File > Open of a file the workspace already has a copy of asks
/// first. Cancel and Escape leave everything as it was; Open Copy opens the
/// workspace copy with its edits.
#[test]
fn opening_a_file_the_workspace_has_asks_and_cancel_changes_nothing() {
    let (mut d, path, id) = edited_copy("safety-b6-cancel");
    let before = backups(&d);
    for answer in ["replace_copy_cancel", "escape"] {
        d.app.open_path(path.clone());
        d.frames(2);
        assert!(d.app.replace_prompt.is_some(), "the user is asked");
        assert!(d.has("replace_copy_open copy") && d.has("replace_copy_replace"));
        if answer == "escape" {
            d.key(Key::Escape);
        } else {
            d.click(answer);
        }
        d.frames(2);
        assert!(d.app.replace_prompt.is_none(), "{answer} closes it");
        assert_eq!(stored_name(&d, id), "Edited in the workspace", "{answer}");
        assert_eq!(
            d.app.project.details[&1010].name, "Edited in the workspace",
            "{answer} keeps the open project"
        );
        assert_eq!(backups(&d), before, "{answer} takes no backup");
    }
    // Open Copy, with an unsaved edit: the copy opened holds it.
    d.app.project.details.get_mut(&1000).unwrap().name = "Unsaved edit".into();
    d.app.dirty = true;
    d.app.open_path(path.clone());
    d.frames(2);
    d.click("replace_copy_open copy");
    d.frames(2);
    d.assert_clean("open the workspace copy");
    assert_eq!(d.app.open_set.active, Some(id));
    assert_eq!(d.app.project.details[&1010].name, "Edited in the workspace");
    assert_eq!(d.app.project.details[&1000].name, "Unsaved edit");
    let _ = std::fs::remove_file(path);
}

/// B6: Enter answers Open Copy, the button framed as the default: it never
/// replaces the workspace copy and takes no backup.
#[test]
fn enter_in_the_replace_question_opens_the_workspace_copy() {
    let (mut d, path, id) = edited_copy("safety-b6-enter");
    let before = backups(&d);
    d.app.open_path(path.clone());
    d.frames(2);
    assert!(d.app.replace_prompt.is_some(), "the user is asked");
    assert_eq!(
        crate::dialog_keys::top_default_label(&d.ctx).as_deref(),
        Some("Open Copy")
    );
    d.key(Key::Enter);
    d.frames(2);
    assert!(d.app.replace_prompt.is_none(), "Enter answers");
    assert_eq!(d.app.open_set.active, Some(id));
    assert_eq!(
        stored_name(&d, id),
        "Edited in the workspace",
        "not replaced"
    );
    assert_eq!(d.app.project.details[&1010].name, "Edited in the workspace");
    assert_eq!(backups(&d), before, "no backup: nothing was replaced");
    let _ = std::fs::remove_file(path);
}

/// B6: Replace puts the file in place of the workspace copy, after copying
/// the workspace into backups, and the backup holds the edits.
#[test]
fn replacing_the_workspace_copy_with_the_file_takes_a_backup_first() {
    let (mut d, path, id) = edited_copy("safety-b6-replace");
    let before = backups(&d);
    d.app.open_path(path.clone());
    replace_with_file(&mut d);
    d.assert_clean("replace the workspace copy");
    assert_eq!(d.app.open_set.active, Some(id), "the same row");
    assert_ne!(stored_name(&d, id), "Edited in the workspace");
    assert_eq!(backups(&d), before + 1, "one backup");
    let dir = d.app.store.path.parent().unwrap().join("backups");
    let backup = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .max_by_key(|p| std::fs::metadata(p).unwrap().modified().unwrap())
        .unwrap();
    let old = Store::open(backup).unwrap();
    assert_eq!(
        old.load(id).unwrap().details[&1010].name,
        "Edited in the workspace",
        "the backup keeps the workspace copy"
    );
    let _ = std::fs::remove_file(path);
}

/// B1: a file whose global calendar id is taken by another dictionary
/// calendar keeps its activities on its own calendar after File > Open, in
/// memory and after reload, and its dates do not move.
#[test]
fn a_file_opened_into_the_workspace_keeps_its_own_calendar_when_ids_collide() {
    let mut d = Driver::new("safety-b1", named("A.xer", "ALPHA"));
    assert!(d.app.store.shared().unwrap());
    // The workspace holds a global calendar "Night shift" at the id the
    // file's own calendar has.
    let night = calendars::add(
        &mut d.app.project,
        calendars::Kind::Global,
        None,
        "2026-11-08 09:00",
    )
    .unwrap();
    calendars::rename(&mut d.app.project, &night, "Night shift").unwrap();
    d.app.touch_records();
    d.frames(2);
    let (path, f) = file_on_calendar("Seven day");
    assert_eq!(calendar_named(&f.document, "Seven day").unwrap(), night);
    let dates: Vec<_> = f
        .model
        .activities
        .iter()
        .map(|a| (a.early_start, a.early_finish))
        .collect();
    d.app.open_path(path.clone());
    d.frames(3);
    d.assert_clean("open the file");
    let names = task_calendars(&d.app.project);
    assert!(
        names.iter().all(|n| n == "Seven day"),
        "every activity stays on the file's calendar: {names:?}"
    );
    let now: Vec<_> = d
        .app
        .project
        .model
        .activities
        .iter()
        .map(|a| (a.early_start, a.early_finish))
        .collect();
    assert_eq!(now, dates, "dates do not move");
    // Saved and loaded again it is the same.
    d.app.save();
    let id = d.app.open_set.active.unwrap();
    let back = d.app.store.load(id).unwrap();
    let names = task_calendars(&back);
    assert!(names.iter().all(|n| n == "Seven day"), "{names:?}");
    let _ = std::fs::remove_file(path);
}

/// B2: opening the file a workspace project came from again neither
/// rewrites nor deletes dictionary rows: a calendar another project uses
/// keeps its edits, an unused resource stays.
#[test]
fn opening_a_file_again_leaves_the_dictionary_rows_of_other_projects_alone() {
    let path = temp_file("again");
    let source = path.display().to_string();
    let original = named(&source, "ALPHA");
    original.updated_document().write(&path).unwrap();
    let mut d = Driver::new("safety-b2", original);
    let a = d.app.open_set.active.unwrap();
    // B uses the standard calendar; A renames it and adds a crane nobody uses.
    let b = d.app.store.save(&named("B.xer", "BRAVO")).unwrap();
    let standard = calendar_named(&d.app.project.document, openxer_xer::STANDARD_CALENDAR_NAME)
        .expect("the starter's calendar");
    calendars::rename(&mut d.app.project, &standard, "Day shift").unwrap();
    tables::add_resource(&mut d.app.project, "CRN").unwrap();
    d.app.touch_records();
    d.frames(2);
    d.app.save();
    let (dict, _) = d.app.store.dictionary().unwrap();
    assert!(rsrc_named(&dict, "CRN"));
    assert_eq!(calendar_named(&dict, "Day shift"), Some(standard.clone()));
    // The file on disk still has the calendar under its old name and no crane.
    d.app.open_path(path.clone());
    replace_with_file(&mut d);
    d.frames(3);
    d.assert_clean("open the file again");
    let (dict, _) = d.app.store.dictionary().unwrap();
    assert!(
        rsrc_named(&dict, "CRN"),
        "the crane stays in the dictionary"
    );
    assert_eq!(
        calendar_named(&dict, "Day shift"),
        Some(standard.clone()),
        "the calendar keeps its name for every project"
    );
    let bp = d.app.store.load(b).unwrap();
    assert!(task_calendars(&bp).iter().all(|n| n == "Day shift"));
    assert_eq!(
        d.app.open_set.active,
        Some(a),
        "the file replaced the same row"
    );
    let _ = std::fs::remove_file(path);
}

/// An activity that can take resources.
fn task_of(p: &ProjectData) -> i64 {
    use openxer_engine::model::TaskType;
    p.model
        .activities
        .iter()
        .find(|a| !matches!(a.task_type, TaskType::Mile | TaskType::FinMile))
        .unwrap()
        .task_id
}

fn rsrc_id(p: &ProjectData, short: &str) -> i64 {
    let t = p.document.table("RSRC").unwrap();
    t.rows
        .iter()
        .find(|r| t.value(r, "rsrc_short_name") == short)
        .map(|r| t.value(r, "rsrc_id").parse().unwrap())
        .unwrap()
}

/// Every assignment of `doc` names a resource `doc` has.
fn dangling_assignments(doc: &Document) -> Vec<String> {
    let Some(t) = doc.table("TASKRSRC") else {
        return vec![];
    };
    let have: Vec<String> = doc.table("RSRC").map_or(vec![], |r| {
        r.rows
            .iter()
            .map(|x| r.value(x, "rsrc_id").to_string())
            .collect()
    });
    t.rows
        .iter()
        .map(|r| t.value(r, "rsrc_id").to_string())
        .filter(|id| !id.is_empty() && !have.contains(id))
        .collect()
}

/// B4: the welder's last assignment is removed and the welder deleted, and
/// the app stops without a save. The workspace holds no assignment to a
/// resource it no longer has.
#[test]
fn a_deleted_resource_never_leaves_the_saved_project_pointing_at_nothing() {
    let mut d = Driver::new("safety-b4", named("A.xer", "ALPHA"));
    let a = d.app.open_set.active.unwrap();
    tables::add_resource(&mut d.app.project, "WLD").unwrap();
    d.app.touch_records();
    d.frames(2);
    let task = task_of(&d.app.project);
    let wld = rsrc_id(&d.app.project, "WLD");
    d.app.project.add_assignment(task, Some(wld), None).unwrap();
    d.app.touch_records();
    d.frames(2);
    d.app.save();
    assert!(d.app.store.source(a).unwrap().2.contains("TASKRSRC"));
    // The assignment goes, then the welder, without a save.
    let t = d.app.project.document.table("TASKRSRC").unwrap();
    let row = t
        .rows
        .iter()
        .position(|r| t.value(r, "rsrc_id") == wld.to_string())
        .unwrap();
    d.app.project.remove_assignment(row).unwrap();
    d.app.touch_records();
    d.frames(2);
    openxer_xer::edit::resources::delete(&mut d.app.project.document, wld).unwrap();
    d.app.project.refresh_resource_inputs();
    d.app.touch_records();
    d.frames(2);
    d.assert_clean("delete the welder");
    // A crash here: what the workspace holds must agree with itself.
    let stored = d.app.store.load(a).unwrap();
    assert_eq!(
        dangling_assignments(&stored.updated_document()),
        Vec::<String>::new(),
        "assignments to a resource the workspace no longer has"
    );
}

fn rsrc_name(p: &ProjectData, short: &str) -> Option<String> {
    let t = p.document.table("RSRC")?;
    t.rows
        .iter()
        .find(|r| t.value(r, "rsrc_short_name") == short)
        .map(|r| t.value(r, "rsrc_name").to_string())
}

fn set_rsrc_name(d: &mut Driver, short: &str, name: &str) {
    tables::set_resource(
        &mut d.app.project,
        short,
        "rsrc_name",
        &tables::serde_like::Value::Text(name.into()),
    )
    .unwrap();
    d.app.touch_records();
    d.frames(2);
}

fn ctrl() -> egui::Modifiers {
    egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    }
}

/// B8: A renames the welder; B, later, renames it again. Undoing A's rename
/// would silently revert B's: it is refused with a message and B's name
/// stays everywhere.
#[test]
fn undo_never_reverts_a_later_change_another_project_made_to_the_same_row() {
    let mut d = Driver::new("safety-b8", named("A.xer", "ALPHA"));
    let a = d.app.open_set.active.unwrap();
    let b = d.app.store.save(&named("B.xer", "BRAVO")).unwrap();
    tables::add_resource(&mut d.app.project, "WLD").unwrap();
    d.app.touch_records();
    d.frames(2);
    set_rsrc_name(&mut d, "WLD", "Welder, day");
    d.app.open_project_set(&[b]);
    d.frames(1);
    set_rsrc_name(&mut d, "WLD", "Welder, night");
    d.app.open_project_set(&[a]);
    d.frames(1);
    assert_eq!(d.app.open_set.active, Some(a));
    d.key_with(Key::Z, ctrl());
    d.frames(2);
    let error = d
        .app
        .error
        .take()
        .expect("the undo is refused with a message");
    assert!(
        error.contains("Resource \"WLD\"") && error.contains("changed"),
        "{error}"
    );
    assert_eq!(
        rsrc_name(&d.app.project, "WLD").as_deref(),
        Some("Welder, night")
    );
    let stored_b = d.app.store.load(b).unwrap();
    assert_eq!(
        rsrc_name(&stored_b, "WLD").as_deref(),
        Some("Welder, night")
    );
}

/// B8, in the store: a delta whose row changed since is refused whole.
#[test]
fn a_dictionary_step_is_not_undone_over_a_later_change() {
    let mut d = Driver::new("safety-b8-store", named("A.xer", "ALPHA"));
    let (before, _) = d.app.store.dictionary().unwrap();
    tables::add_resource(&mut d.app.project, "WLD").unwrap();
    d.app.touch_records();
    d.frames(2);
    let (after, _) = d.app.store.dictionary().unwrap();
    let step = openxer_xer::workspace_scope::delta::diff(&before, &after);
    set_rsrc_name(&mut d, "WLD", "Welder, night");
    let a = d.app.open_set.active;
    let error = d
        .app
        .store
        .apply_dictionary_delta(&step, false, a, &[])
        .unwrap_err();
    assert!(error.contains("changed"), "{error}");
    let (now, _) = d.app.store.dictionary().unwrap();
    assert!(rsrc_named(&now, "WLD"), "the welder stays");
}

/// B7: A adds the welder; B, open and not saved, assigns it. Undoing A's
/// step must not take the welder out from under B.
#[test]
fn undo_counts_the_uses_of_open_unsaved_projects() {
    let mut d = Driver::new("safety-b7", named("A.xer", "ALPHA"));
    let b = d.app.store.save(&named("B.xer", "BRAVO")).unwrap();
    let (before, _) = d.app.store.dictionary().unwrap();
    tables::add_resource(&mut d.app.project, "WLD").unwrap();
    d.app.touch_records();
    d.frames(2);
    let (after, _) = d.app.store.dictionary().unwrap();
    let step = openxer_xer::workspace_scope::delta::diff(&before, &after);
    // B, open with unsaved changes, assigns the welder.
    let mut bp = d.app.store.load(b).unwrap();
    let task = task_of(&bp);
    let wld = rsrc_id(&bp, "WLD");
    bp.add_assignment(task, Some(wld), None).unwrap();
    let open = [(Some(b), &bp.document)];
    let a = d.app.open_set.active;
    let error = d
        .app
        .store
        .apply_dictionary_delta(&step, false, a, &open)
        .unwrap_err();
    assert!(error.contains("BRAVO"), "{error}");
    let (now, _) = d.app.store.dictionary().unwrap();
    assert!(rsrc_named(&now, "WLD"), "the welder stays for B");
}
