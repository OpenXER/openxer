//! New projects get a WBS of their own: Projects view Add, File > New and
//! Ctrl+N (with one or two projects open, under an existing project's EPS
//! node) each make one project node and nothing of another project's WBS.
//! Projects made by the old Add, which copied the sample's WBS (RB.1 to
//! RB.4) and no project node, are repaired when they become current, as one
//! undo step, and only when the sample elements are provably untouched.
//! Delete in the WBS view (key and Edit > Delete) deletes the element.
use super::*;
use crate::new_project::Dialog;
use crate::qa_sweep::Driver;
use crate::views::{wbs as view_wbs, MainView};
use openxer_xer::edit::wbs::{repair_add_sample_wbs, ADD_SAMPLE_WBS};

fn sample() -> ProjectData {
    let mut p = openxer_xer::starter();
    crate::scheduling::schedule_model(&mut p.model, &p.resources).unwrap();
    p
}

/// What the Projects view Add built up to 39221ef: the sample with its
/// activities deleted and a new Project ID.
fn legacy_add(name: &str) -> ProjectData {
    let mut p = openxer_xer::starter();
    let all: Vec<i64> = p.model.activities.iter().map(|a| a.task_id).collect();
    p.delete_activities(&all).unwrap();
    p.model.project.name = name.into();
    p
}

fn ctrl() -> egui::Modifiers {
    egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    }
}

fn codes(p: &ProjectData) -> Vec<String> {
    p.wbs.iter().map(|w| w.code.clone()).collect()
}

/// The project's WBS is exactly one project node coded with its Project ID,
/// in `p.wbs` and in the PROJWBS rows the XER saves.
fn only_own_node(p: &ProjectData) {
    let id = &p.model.project.name;
    assert_eq!(codes(p), vec![id.clone()], "{id}: only its own node");
    let node = view_wbs::project_node(p).expect("a project node");
    let t = p.document.table("PROJWBS").unwrap();
    let rows: Vec<_> = t.rows.iter().collect();
    assert_eq!(rows.len(), 1, "{id}: one PROJWBS row");
    let r = rows[0];
    assert_eq!(t.value(r, "wbs_id"), node.to_string());
    assert_eq!(t.value(r, "proj_id"), p.model.project.proj_id.to_string());
    assert_eq!(t.value(r, "parent_wbs_id"), "");
    assert_eq!(t.value(r, "proj_node_flag"), "Y");
}

/// Texts the WBS view draws for WBS codes of the sample (not the status
/// line the repair leaves).
fn sample_codes_on_screen(d: &mut Driver) -> Vec<String> {
    d.app.show_view(MainView::Wbs);
    d.frames(3);
    d.texts()
        .into_iter()
        .map(|(s, _)| s)
        .filter(|s| {
            s.contains("RB.") && !s.starts_with(crate::views::project_ops::SAMPLE_WBS_REPAIRED)
        })
        .collect()
}

fn finish_wizard(d: &mut Driver, eps: i64, id: &str) {
    d.key_with(egui::Key::N, ctrl());
    d.frames(3);
    *d.app.new_project.as_mut().expect("the wizard") = Dialog {
        eps,
        project_id: id.into(),
        name: format!("{id} name"),
        planned_start: "04-Jan-27".into(),
        must_finish: String::new(),
        manager: String::new(),
        rate_type: String::new(),
        error: None,
        ..Default::default()
    };
    d.click("new_project_finish");
    d.frames(3);
    assert!(d.app.new_project.is_none(), "Finish closes the wizard");
    assert_eq!(d.app.project.model.project.name, id);
}

#[test]
fn projects_view_add_makes_its_own_project_node_not_the_samples_wbs() {
    let mut d = Driver::new("addwbs-ui", sample());
    let before = d.app.store.list().unwrap().len();
    d.app.show_view(MainView::Projects);
    d.frames(3);
    // Add opens the New Project dialog; Finish adds the project.
    d.click("add");
    d.frames(3);
    d.click("new_project_finish");
    d.frames(3);
    let rows = d.app.store.list().unwrap();
    assert_eq!(rows.len(), before + 1, "Add adds one project");
    let (id, title) = rows
        .iter()
        .find(|(_, t)| t.starts_with("NEW-"))
        .cloned()
        .expect("the added project");
    let p = d.app.store.load(id).unwrap();
    assert_eq!(p.model.project.name, title);
    assert!(p.model.activities.is_empty());
    only_own_node(&p);
    // Opened, the WBS view shows its node and no sample element; the sample
    // project keeps its own WBS.
    d.app.open_project_set(&[id]);
    assert!(sample_codes_on_screen(&mut d).is_empty());
    let riverbend = d
        .app
        .store
        .list()
        .unwrap()
        .into_iter()
        .find(|(_, t)| t == "RIVERBEND")
        .unwrap()
        .0;
    assert_eq!(
        codes(&d.app.store.load(riverbend).unwrap()),
        ["RB.1", "RB.2", "RB.3", "RB.4"]
    );
    // A second Add with both open is the same.
    d.app.open_project_set(&[id, riverbend]);
    let eps = d.app.workspace().node_of(Some(riverbend));
    d.app.open_new_project_in(eps);
    d.click("new_project_finish");
    d.frames(3);
    let second = d.app.open_set.active.unwrap();
    assert_ne!(second, riverbend);
    only_own_node(&d.app.store.load(second).unwrap());
}

#[test]
fn the_wizard_makes_its_own_project_node_from_every_starting_point() {
    let mut d = Driver::new("wizardwbs", sample());
    let riverbend = d.app.open_set.active.unwrap();
    let eps = d.app.workspace().node_of(Some(riverbend));
    // With the sample open, under the sample's own EPS node.
    finish_wizard(&mut d, eps, "W-1");
    only_own_node(&d.app.project);
    assert!(sample_codes_on_screen(&mut d).is_empty());
    let w1 = d.app.open_set.active.unwrap();
    only_own_node(&d.app.store.load(w1).unwrap());
    // With two projects open (the new one and the sample).
    d.app.open_project_set(&[w1, riverbend]);
    assert_eq!(d.app.open_ids().len(), 2);
    finish_wizard(&mut d, eps, "W-2");
    only_own_node(&d.app.project);
    assert!(sample_codes_on_screen(&mut d).is_empty());
    assert_eq!(d.app.open_ids().len(), 1, "Finish opens the new one alone");
    // With an old Add project (sample WBS) current.
    let old = d
        .app
        .insert_workspace_project(legacy_add("OLD-1"), Some(eps), 3.)
        .unwrap();
    d.app.open_project_set(&[old]);
    finish_wizard(&mut d, eps, "W-3");
    only_own_node(&d.app.project);
    assert!(sample_codes_on_screen(&mut d).is_empty());
    // With several open, the composite gives each project its band and the
    // new project's band holds only its own node.
    let w3 = d.app.open_set.active.unwrap();
    d.app.open_project_set(&[w3, riverbend]);
    let shown = d.app.shown();
    let band = shown
        .wbs
        .iter()
        .find(|w| w.parent.is_none() && w.code == "W-3")
        .expect("a band for W-3")
        .id;
    let under: Vec<String> = shown
        .wbs
        .iter()
        .filter(|w| w.parent == Some(band))
        .map(|w| w.code.clone())
        .collect();
    assert_eq!(under, ["W-3"]);
}

#[test]
fn an_old_add_project_is_repaired_when_opened_and_undo_restores_it() {
    let mut d = Driver::new("repairwbs", sample());
    let riverbend = d.app.open_set.active.unwrap();
    let eps = d.app.workspace().node_of(Some(riverbend));
    let old = d
        .app
        .insert_workspace_project(legacy_add("NEW-1"), Some(eps), 1.)
        .unwrap();
    assert_eq!(
        codes(&d.app.store.load(old).unwrap()),
        ["RB.1", "RB.2", "RB.3", "RB.4"],
        "stored as the old Add made it"
    );
    d.app.open_project_set(&[old]);
    only_own_node(&d.app.project);
    assert!(
        d.app.status.contains("RB.1, RB.2, RB.3, RB.4"),
        "{}",
        d.app.status
    );
    assert!(sample_codes_on_screen(&mut d).is_empty());
    d.app.save();
    only_own_node(&d.app.store.load(old).unwrap());
    // One undo step puts the sample WBS back.
    d.app.history_move(&d.ctx.clone(), false);
    assert_eq!(codes(&d.app.project), ["RB.1", "RB.2", "RB.3", "RB.4"]);
    // The sample project itself is never touched.
    d.app.open_project_set(&[riverbend]);
    assert_eq!(codes(&d.app.project), ["RB.1", "RB.2", "RB.3", "RB.4"]);
    // A parked old project is repaired when it becomes current.
    let old2 = d
        .app
        .insert_workspace_project(legacy_add("NEW-2"), Some(eps), 2.)
        .unwrap();
    d.app.open_project_set(&[riverbend, old2]);
    assert_eq!(d.app.project.model.project.name, "RIVERBEND");
    d.app.activate_id(old2);
    only_own_node(&d.app.project);
}

#[test]
fn the_repair_leaves_alone_what_it_cannot_prove_the_old_add_made() {
    // Sample element in use: one activity kept, on RB.2.
    let mut in_use = openxer_xer::starter();
    let keep = in_use.model.activities[0].task_id;
    let rest: Vec<i64> = in_use.model.activities[1..]
        .iter()
        .map(|a| a.task_id)
        .collect();
    in_use.delete_activities(&rest).unwrap();
    in_use.details.get_mut(&keep).unwrap().wbs = 20;
    in_use.model.project.name = "NEW-1".into();
    let before = codes(&in_use);
    assert_eq!(
        repair_add_sample_wbs(&mut in_use),
        None,
        "an activity on RB.2"
    );
    assert_eq!(codes(&in_use), before);
    // The sample project and its copies.
    for name in ["RIVERBEND", "RIVERBEND-1"] {
        let mut p = legacy_add(name);
        assert_eq!(repair_add_sample_wbs(&mut p), None, "{name}");
    }
    // A renamed sample element is someone's own.
    let mut renamed = legacy_add("NEW-1");
    renamed.wbs[0].name = "Design".into();
    assert_eq!(repair_add_sample_wbs(&mut renamed), None);
    // A child under a sample element.
    let mut child = legacy_add("NEW-1");
    child.wbs.push(openxer_xer::Wbs {
        id: 50,
        parent: Some(10),
        code: "A".into(),
        name: "Area A".into(),
        sequence: 50,
    });
    view_wbs::sync_projwbs(&mut child);
    assert_eq!(repair_add_sample_wbs(&mut child), None);
    // A notebook entry on a sample element.
    let mut memo = legacy_add("NEW-1");
    memo.document.table_mut("WBSMEMO").add(&[
        ("wbs_memo_id", "1".into()),
        ("proj_id", "1".into()),
        ("wbs_id", "30".into()),
        ("wbs_memo", "note".into()),
    ]);
    assert_eq!(repair_add_sample_wbs(&mut memo), None);
    // A project with a project node (any new or imported project).
    let mut wizard = crate::new_project::build(
        &Dialog {
            eps: 1,
            project_id: "W".into(),
            name: "W".into(),
            planned_start: "04-Jan-27".into(),
            ..Default::default()
        }
        .check(&[], &[1])
        .unwrap(),
        None,
        2,
    )
    .unwrap();
    assert_eq!(repair_add_sample_wbs(&mut wizard), None);
}

#[test]
fn the_repair_keeps_elements_added_since_under_the_new_project_node() {
    let mut p = legacy_add("NEW-1");
    let own = view_wbs::add(&mut p, None).unwrap();
    p.wbs.iter_mut().find(|w| w.id == own).unwrap().code = "MINE".into();
    view_wbs::sync_projwbs(&mut p);
    // The user has already deleted two of the four.
    view_wbs::delete_merge(&mut p, 10).unwrap();
    view_wbs::delete_merge(&mut p, 40).unwrap();
    assert_eq!(
        repair_add_sample_wbs(&mut p),
        Some(vec!["RB.2".to_string(), "RB.3".to_string()])
    );
    let node = view_wbs::project_node(&p).expect("a project node");
    let w = |code: &str| p.wbs.iter().find(|w| w.code == code).cloned().unwrap();
    assert_eq!(w("NEW-1").id, node);
    assert_eq!(w("NEW-1").parent, None);
    assert_eq!(w("MINE").parent, Some(node));
    assert_eq!(codes(&p).len(), 2);
    assert!(!ADD_SAMPLE_WBS
        .iter()
        .any(|(_, code, _)| codes(&p).contains(&code.to_string())));
    // It survives the XER round trip.
    let text = p.updated_document().text();
    let back = ProjectData::from_document(Document::parse(&text).unwrap(), 1, "rt".into()).unwrap();
    assert_eq!(view_wbs::project_node(&back), Some(node));
    assert_eq!(codes(&back), codes(&p));
}

#[test]
fn delete_key_and_edit_delete_delete_the_selected_wbs_element() {
    let mut d = Driver::new("wbsdelete", legacy_add("NEW-1"));
    d.app.show_view(MainView::Wbs);
    d.frames(3);
    // The Delete key.
    assert!(d.click_text("NEW-1.RB.1"));
    d.frames(2);
    assert_eq!(d.app.views.wbs.selected, Some(10));
    d.key(egui::Key::Delete);
    d.frames(3);
    assert!(d.has("wbs_confirm_merge"), "Delete asks first");
    d.click("wbs_confirm_merge");
    d.frames(3);
    assert_eq!(codes(&d.app.project), ["RB.2", "RB.3", "RB.4"]);
    // Edit > Delete.
    assert!(d.click_text("NEW-1.RB.2"));
    d.frames(2);
    d.click("menu_bar_edit");
    d.frames(2);
    d.click("edit_delete");
    d.frames(3);
    assert!(d.has("wbs_confirm_merge"));
    d.click("wbs_confirm_merge");
    d.frames(3);
    assert_eq!(codes(&d.app.project), ["RB.3", "RB.4"]);
    // Escape on the confirmation deletes nothing.
    assert!(d.click_text("NEW-1.RB.3"));
    d.frames(2);
    d.key(egui::Key::Delete);
    d.frames(3);
    d.key(egui::Key::Escape);
    d.frames(3);
    assert_eq!(codes(&d.app.project), ["RB.3", "RB.4"]);
    // The project node cannot be deleted.
    let mut d = Driver::new("wbsdelete-node", sample());
    finish_wizard(&mut d, 1, "W-1");
    d.app.show_view(MainView::Wbs);
    d.app.views.wbs.selected = view_wbs::project_node(&d.app.project);
    d.frames(2);
    assert!(!d.app.wbs_can_delete());
    d.key(egui::Key::Delete);
    d.frames(3);
    assert!(!d.has("wbs_confirm_merge"));
    only_own_node(&d.app.project);
}

/// Renders to the ignored `artifacts/fix-new-project-wbs/`: an old Add
/// project before and after the repair, and a project from the fixed Add.
#[test]
#[ignore = "writes review renders to artifacts/fix-new-project-wbs; run on demand"]
fn render_new_project_wbs() {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/fix-new-project-wbs");
    std::fs::create_dir_all(&out).unwrap();
    let screen = egui::Vec2::new(1500., 900.);
    let mut d = Driver::probed("render-wbs", legacy_add("NEW-1"), screen, Some(out.clone()));
    d.home();
    d.app.show_view(MainView::Wbs);
    d.frames(3);
    d.screenshot(&out.join("01-before-old-add-project.png"));
    d.app.repair_add_sample_wbs();
    d.frames(3);
    d.screenshot(&out.join("02-after-repair.png"));
    d.app.show_view(MainView::Projects);
    d.frames(3);
    d.click("add");
    d.frames(3);
    d.click("new_project_finish");
    d.frames(3);
    let id = d
        .app
        .store
        .list()
        .unwrap()
        .into_iter()
        .find(|(_, t)| t == "NEW-2")
        .unwrap()
        .0;
    d.app.open_project_set(&[id]);
    d.app.show_view(MainView::Wbs);
    d.frames(3);
    d.screenshot(&out.join("03-after-fixed-add.png"));
}
