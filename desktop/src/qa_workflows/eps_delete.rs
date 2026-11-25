//! Workflow: deleting a project folder (EPS node) deletes everything under
//! it, as observed in the scheduler users know: its child folders, the
//! projects filed in any of them and those projects' baselines, after one
//! question that counts the items (folders plus projects). No (also Enter)
//! changes nothing; Yes closes an open project among them (its unsaved
//! changes saved first) and removes it all; one Undo brings it back.
//! Driven through the Projects view's Delete on a folder row, and once
//! through Enterprise > Project Folders (EPS) > Delete.
use super::*;
use crate::baselines::store as bstore;

/// Plant (under the root) with Unit 1 under it, as one workspace step.
/// Adding folders through the EPS dialog is covered by workflow 1.
fn folders(d: &mut Driver) -> (i64, i64, i64) {
    let mut ws = d.app.workspace().clone();
    let top = ws.eps()[0].id;
    let plant = ws.add_folder(Some(top), "Plant", Some("PLANT")).unwrap();
    let unit = ws.add_folder(Some(plant), "Unit 1", Some("UNIT1")).unwrap();
    d.app
        .run_workspace(
            Box::new(move |w| {
                *w = ws;
                Ok(crate::enterprise::Done::Changed(None))
            }),
            0.,
        )
        .unwrap();
    (top, plant, unit)
}

/// Project > Maintain Baselines > Add a copy of the open project, Close.
fn add_baseline(d: &mut Driver) -> i64 {
    open_maintain(d);
    let id = add_baseline_copy(d);
    d.click("baselines_close");
    d.frames(2);
    id
}

/// Click the Projects view row of EPS node `eps`.
fn projects_select_eps(d: &mut Driver, eps: i64) {
    show(d, "projects");
    for k in 0..40 {
        let row = format!("projects_row_{k}");
        if !d.has(&row) {
            break;
        }
        d.click(&row);
        let s = &d.app.views.projects;
        if s.selected.is_none() && s.selected_eps == Some(eps) {
            return;
        }
    }
    panic!("EPS node {eps} is not listed in the Projects view");
}

/// Cut project `id` in the Projects view and paste it on EPS node `eps`.
fn file_under(d: &mut Driver, id: i64, eps: i64) {
    projects_select(d, id);
    d.click("dock_cut");
    projects_select_eps(d, eps);
    d.click("dock_paste");
    d.frames(2);
    assert_eq!(d.app.workspace().node_of(Some(id)), eps, "filed");
}

/// What the workspace holds: EPS node ids, stored project ids, and the
/// baseline ids of `owners`.
fn holdings(d: &mut Driver, owners: &[i64]) -> (Vec<i64>, Vec<i64>, Vec<i64>) {
    let nodes = d.app.workspace().eps().iter().map(|n| n.id).collect();
    let mut projects = d.app.workspace_ids();
    projects.sort();
    let c = d.app.store.connection();
    let baselines = owners
        .iter()
        .flat_map(|o| bstore::list(c, *o).unwrap().into_iter().map(|b| b.id))
        .collect();
    (nodes, projects, baselines)
}

fn drawn(d: &mut Driver, text: &str) -> bool {
    d.texts().iter().any(|(t, _)| t.contains(text))
}

#[test]
fn workflow_eps_delete_takes_child_folders_projects_and_baselines() {
    let mut d = fresh("wf-eps-delete");
    let sample = d.app.open_set.active.expect("the sample has a row");
    let (top, plant, unit) = folders(&mut d);
    // A in Plant, B in Unit 1, each with a baseline.
    let a = new_project_in(
        &mut d,
        Some("    Plant"),
        "PL-100",
        "Plant tie-ins",
        "04-Jan-27",
        "",
    );
    let a_baseline = add_baseline(&mut d);
    let b = new_project_in(
        &mut d,
        Some("    Plant"),
        "PL-200",
        "Unit 1 piping",
        "11-Jan-27",
        "",
    );
    let b_baseline = add_baseline(&mut d);
    file_under(&mut d, b, unit);
    assert_eq!(d.app.workspace().node_of(Some(a)), plant);

    // The sample and A open, A current with a change not saved yet.
    open_projects(&mut d, &[sample, a]);
    d.app.activate_id(a);
    show(&mut d, "activities");
    add_activity(&mut d, "Unsaved demo job", "2d");
    d.key(Key::Escape);
    d.frames(1);
    assert!(d.app.any_dirty(), "A has a change not saved yet");
    let a_open = d.app.project.model.activities.len();
    let before = holdings(&mut d, &[a, b]);
    assert_eq!(before.0, [top, plant, unit]);
    assert_eq!(before.2, [a_baseline, b_baseline]);
    let undo = d.app.history.counts().0;

    // Delete on the Plant row asks once, counting 2 folders + 2 projects.
    projects_select_eps(&mut d, plant);
    d.click("dock_delete");
    d.frames(2);
    assert!(
        d.has("delete_folder_yes") && d.has("delete_folder_no"),
        "Delete on a folder row asks first"
    );
    assert!(drawn(&mut d, "4 items"), "the question counts what goes");
    assert!(drawn(
        &mut d,
        "2 EPS node(s) and 2 project(s) with 2 baseline(s)"
    ));
    // Enter answers No, the default for a delete: nothing changes.
    d.key(Key::Enter);
    d.frames(2);
    assert!(!d.has("delete_folder_yes"), "Enter closes the question");
    assert_eq!(holdings(&mut d, &[a, b]), before, "Enter changes nothing");
    assert!(d.app.is_open(a) && d.app.any_dirty());
    // No changes nothing either.
    projects_select_eps(&mut d, plant);
    d.click("dock_delete");
    d.frames(2);
    d.click("delete_folder_no");
    d.frames(2);
    assert_eq!(holdings(&mut d, &[a, b]), before, "No changes nothing");
    assert!(d.app.is_open(a), "A stays open");
    assert_eq!(d.app.history.counts().0, undo);

    // Yes: both folders, both projects and their baselines go; A is closed
    // (its change saved first) and the sample becomes current.
    projects_select_eps(&mut d, plant);
    d.click("dock_delete");
    d.frames(2);
    d.click("delete_folder_yes");
    d.frames(3);
    d.assert_clean("eps delete");
    let after = holdings(&mut d, &[a, b]);
    assert_eq!(after.0, [top], "the folder and its child folder are gone");
    assert_eq!(after.1, [sample], "both projects are gone");
    assert!(after.2.is_empty(), "their baselines are gone");
    assert!(d.app.store.load(a).is_err() && d.app.store.load(b).is_err());
    assert!(!d.app.is_open(a), "the open project was closed");
    assert_eq!(d.app.open_ids(), [sample]);
    assert_eq!(
        d.app.project.model.project.name,
        stored(&d, sample).model.project.name
    );
    assert!(d.app.error.is_none(), "{:?}", d.app.error);

    // One Undo brings the folders, the projects (closed, A with its change)
    // and the baselines back; Redo deletes them again.
    d.home();
    d.key_with(Key::Z, ctrl());
    d.frames(2);
    assert_eq!(holdings(&mut d, &[a, b]), before, "undo restores it all");
    assert_eq!(d.app.workspace().node_of(Some(a)), plant);
    assert_eq!(d.app.workspace().node_of(Some(b)), unit);
    assert_eq!(
        stored(&d, a).model.activities.len(),
        a_open,
        "A comes back with the change it had when it was closed"
    );
    assert!(!d.app.is_open(a), "the projects come back closed");
    d.key_with(Key::Y, ctrl());
    d.frames(2);
    assert_eq!(holdings(&mut d, &[a, b]), after, "redo deletes again");
    d.key_with(Key::Z, ctrl());
    d.frames(2);
    assert_eq!(holdings(&mut d, &[a, b]), before);
    // The restored projects open as themselves.
    open_projects(&mut d, &[b]);
    assert_eq!(d.app.project.model.project.name, "PL-200");
    d.assert_clean("reopen");
}

/// Enterprise > Project Folders (EPS) > Delete does the same, through the
/// dialog's own question.
#[test]
fn workflow_eps_dialog_delete_takes_the_projects_too() {
    let mut d = fresh("wf-eps-dialog-delete");
    let sample = d.app.open_set.active.expect("the sample has a row");
    let (top, plant, _unit) = folders(&mut d);
    let a = new_project_in(
        &mut d,
        Some("    Plant"),
        "PL-300",
        "Plant cooling water",
        "04-Jan-27",
        "",
    );
    add_baseline(&mut d);
    open_projects(&mut d, &[sample]);
    d.home();
    d.click("menu_bar_enterprise");
    d.click("menu_eps");
    d.frames(2);
    // Rows: the root, Plant, Unit 1.
    d.click("eps_list_row_1");
    assert_eq!(d.app.enterprise.ui.selected, Some(plant));
    d.click("eps_delete");
    d.frames(2);
    let text = d
        .app
        .enterprise
        .ui
        .confirm
        .as_ref()
        .map(|c| c.text.clone())
        .unwrap_or_default();
    assert!(text.contains("3 items"), "folders plus projects: {text:?}");
    d.click("enterprise_msg_no");
    d.frames(2);
    assert!(d.app.store.load(a).is_ok(), "No keeps the project");
    d.click("eps_list_row_1");
    d.click("eps_delete");
    d.frames(2);
    d.click("enterprise_msg_yes");
    d.frames(2);
    d.click("eps_close");
    d.frames(2);
    d.assert_clean("eps dialog delete");
    let after = holdings(&mut d, &[a]);
    assert_eq!(after.0, [top]);
    assert_eq!(after.1, [sample], "the project in the folder is deleted");
    assert!(after.2.is_empty(), "with its baseline");
}
