//! Live sync of the shared workspace dictionary in the app (schema 3):
//! other open projects see a change at once, undo takes back only the
//! step's own rows, a row in use cannot be deleted, closed projects' rollups
//! follow.
use super::*;
use crate::qa_sweep::Driver;
use openxer_xer::edit::{calendars, tables};

fn ctrl() -> egui::Modifiers {
    egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    }
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

fn named(source: &str, id: &str) -> ProjectData {
    let mut p = openxer_xer::starter();
    p.source_name = source.into();
    p.model.project.name = id.into();
    p
}

/// A new workspace (it shares its dictionaries) with projects A and B
/// stored and both open, A active.
fn two_open() -> (Driver, i64, i64) {
    let mut d = Driver::new("ws-sync", named("A.xer", "ALPHA"));
    assert!(
        d.app.store.shared().unwrap(),
        "a new workspace shares its dictionaries"
    );
    let a = d.app.open_set.active.unwrap();
    let b = d.app.store.save(&named("B.xer", "BRAVO")).unwrap();
    d.app.open_project_set(&[a, b]);
    d.app.activate_id(a);
    d.frames(2);
    assert_eq!(d.app.open_set.active, Some(a));
    (d, a, b)
}

fn parked(d: &Driver, id: i64) -> &ProjectData {
    &d.app
        .open_set
        .parked
        .iter()
        .find(|p| p.id == id)
        .expect("open")
        .project
}

fn rsrc_name(p: &ProjectData, short: &str) -> Option<String> {
    let t = p.document.table("RSRC")?;
    t.rows
        .iter()
        .find(|r| t.value(r, "rsrc_short_name") == short)
        .map(|r| t.value(r, "rsrc_name").to_string())
}

fn rsrc_id(p: &ProjectData, short: &str) -> i64 {
    let t = p.document.table("RSRC").unwrap();
    t.rows
        .iter()
        .find(|r| t.value(r, "rsrc_short_name") == short)
        .map(|r| t.value(r, "rsrc_id").parse().unwrap())
        .unwrap()
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

#[test]
fn a_global_calendar_made_in_one_open_project_is_in_the_other_at_once() {
    let (mut d, _a, b) = two_open();
    let id = calendars::add(
        &mut d.app.project,
        calendars::Kind::Global,
        None,
        "2026-11-08 09:00",
    )
    .unwrap();
    calendars::rename(&mut d.app.project, &id, "Night shift").unwrap();
    d.app.touch_records();
    d.frames(2);
    d.assert_clean("add a calendar");
    assert!(
        parked(&d, b)
            .calendar_names
            .contains(&"Night shift".to_string()),
        "B offers it without a save: {:?}",
        parked(&d, b).calendar_names
    );
    assert!(d
        .app
        .store
        .load(b)
        .unwrap()
        .calendar_names
        .contains(&"Night shift".to_string()));
    // Undo takes it out of both; redo brings it back to both.
    d.key_with(Key::Z, ctrl());
    d.frames(2);
    assert!(!d
        .app
        .project
        .calendar_names
        .contains(&"Night shift".to_string()));
    assert!(!parked(&d, b)
        .calendar_names
        .contains(&"Night shift".to_string()));
    assert!(!d
        .app
        .store
        .load(b)
        .unwrap()
        .calendar_names
        .contains(&"Night shift".to_string()));
    d.key_with(Key::Y, ctrl());
    d.frames(2);
    assert!(d
        .app
        .project
        .calendar_names
        .contains(&"Night shift".to_string()));
    assert!(parked(&d, b)
        .calendar_names
        .contains(&"Night shift".to_string()));
    d.assert_clean("redo");
}

#[test]
fn undo_takes_back_only_the_rows_of_its_own_step() {
    let mut d = Driver::new("ws-undo", named("A.xer", "ALPHA"));
    let a = d.app.open_set.active.unwrap();
    let b = d.app.store.save(&named("B.xer", "BRAVO")).unwrap();
    tables::add_resource(&mut d.app.project, "WLD").unwrap();
    tables::add_resource(&mut d.app.project, "CRN").unwrap();
    d.app.touch_records();
    d.frames(2);
    set_rsrc_name(&mut d, "WLD", "Welder, day");
    // A closes; B, opened alone, renames the crane.
    d.app.open_project_set(&[b]);
    d.frames(1);
    assert_eq!(
        rsrc_name(&d.app.project, "WLD").as_deref(),
        Some("Welder, day")
    );
    set_rsrc_name(&mut d, "CRN", "Crane 50t");
    // A again, with its undo history: undo its welder rename. B's crane
    // stays.
    d.app.open_project_set(&[a]);
    d.frames(1);
    assert_eq!(d.app.open_set.active, Some(a));
    assert_eq!(
        rsrc_name(&d.app.project, "CRN").as_deref(),
        Some("Crane 50t")
    );
    d.key_with(Key::Z, ctrl());
    d.frames(2);
    d.assert_clean("undo");
    assert_ne!(
        rsrc_name(&d.app.project, "WLD").as_deref(),
        Some("Welder, day")
    );
    assert_eq!(
        rsrc_name(&d.app.project, "CRN").as_deref(),
        Some("Crane 50t")
    );
    let stored_b = d.app.store.load(b).unwrap();
    assert_eq!(rsrc_name(&stored_b, "CRN").as_deref(), Some("Crane 50t"));
    assert_ne!(
        rsrc_name(&stored_b, "WLD").as_deref(),
        Some("Welder, day"),
        "the undo reaches B"
    );
}

#[test]
fn a_resource_another_project_uses_cannot_be_deleted() {
    let (mut d, _a, b) = two_open();
    tables::add_resource(&mut d.app.project, "WLD").unwrap();
    d.app.touch_records();
    d.frames(2);
    // B assigns the welder and is saved.
    let mut bp = d.app.store.load(b).unwrap();
    let task = task_of(&bp);
    let wld = rsrc_id(&bp, "WLD");
    bp.add_assignment(task, Some(wld), None).unwrap();
    d.app.store.save(&bp).unwrap();
    d.app.open_set.parked.retain(|p| p.id != b);
    let rsrc = rsrc_id(&d.app.project, "WLD");
    assert_eq!(
        d.app
            .store
            .dictionary_usage("RSRC", rsrc, d.app.open_set.active)
            .unwrap(),
        vec![(b, "BRAVO".to_string())]
    );
    openxer_xer::edit::resources::delete(&mut d.app.project.document, rsrc).unwrap();
    d.app.project.refresh_resource_inputs();
    d.app.touch_records();
    d.frames(2);
    let error = d.app.error.take().expect("the delete is refused");
    assert!(
        error.contains("Resource \"WLD\"") && error.contains("BRAVO"),
        "{error}"
    );
    assert!(
        rsrc_name(&d.app.project, "WLD").is_some(),
        "the welder stays"
    );
    assert!(rsrc_name(&d.app.store.load(b).unwrap(), "WLD").is_some());
    assert_eq!(
        d.app.history.counts().1,
        0,
        "the refused step cannot be redone"
    );
}

#[test]
fn closed_project_rollups_follow_a_shared_resource_change() {
    let (mut d, _a, b) = two_open();
    tables::add_resource(&mut d.app.project, "WLD").unwrap();
    d.app.touch_records();
    d.frames(2);
    let mut bp = d.app.store.load(b).unwrap();
    let task = task_of(&bp);
    let wld = rsrc_id(&bp, "WLD");
    bp.add_assignment(task, Some(wld), None).unwrap();
    d.app.store.save(&bp).unwrap();
    d.app.close_project_id(b);
    d.frames(1);
    d.app.workspace_projects();
    assert!(d.app.views.projects.is_cached(b));
    // A change B does not use keeps B's rollup; one it uses drops it.
    tables::add_resource(&mut d.app.project, "CRN").unwrap();
    d.app.touch_records();
    d.frames(2);
    d.app.workspace_projects();
    assert!(d.app.views.projects.is_cached(b));
    d.app.drop_stale_rollups();
    set_rsrc_name(&mut d, "WLD", "Welder, night");
    d.app.drop_stale_rollups();
    assert!(!d.app.views.projects.is_cached(b), "B uses the welder");
}
