//! The Projects view's Funding tab through the production UI: Add, in-place
//! edits of Funding Source, Amount and Fund Share, Delete, one undo step per
//! edit, the workspace save and reopen, and the XER round trip of PROJFUND.
use super::*;
use crate::qa_sweep::Driver;
use egui::Key;
use openxer_xer::edit::funding;
use openxer_xer::{Document, ProjectData};

fn ctrl() -> egui::Modifiers {
    egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    }
}

/// The starter project with a project node and two funding sources.
fn funded() -> ProjectData {
    let mut p = wbs::tests::with_project_node();
    for (id, name) in [("7", "Capital"), ("9", "Operating Grant")] {
        crate::records::add_row(
            &mut p.document,
            funding::SOURCES,
            &[("fund_id", id.into()), ("fund_name", name.into())],
        );
    }
    p
}

fn open_funding(d: &mut Driver) {
    d.click("directory_projects");
    let p = &d.app.project;
    d.app.views.projects.selected = Some((p.source_name.clone(), p.model.project.proj_id));
    d.app.views.projects.show_details = true;
    d.app.views.projects.tab = projects::TABS.iter().position(|t| *t == "Funding").unwrap();
    d.frames(3);
}

fn type_into(d: &mut Driver, action: &str, text: &str) {
    d.click(action);
    d.key_with(Key::A, ctrl());
    d.type_text(text);
    d.key(Key::Enter);
    d.frames(2);
}

fn has_text(d: &mut Driver, text: &str) -> bool {
    d.texts().iter().any(|(t, _)| t == text)
}

#[test]
fn the_funding_tab_adds_edits_deletes_and_undoes_one_step_each() {
    let mut d = Driver::new("funding-tab", funded());
    open_funding(&mut d);
    assert!(
        has_text(
            &mut d,
            "No funding sources are assigned to this project. Add assigns one from Enterprise > Funding Sources."
        ),
        "the empty tab tells how to add funding (XER does carry it)"
    );
    assert!(!has_text(&mut d, "XER files do not carry this data."));
    let steps = d.app.history.steps();
    d.click("project_funding_buttons_0");
    d.frames(2);
    let list = funding::list(&d.app.project);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].source, "Capital");
    assert_eq!(d.app.history.steps(), steps + 1, "Add is one undo step");
    type_into(&mut d, "project_funding_cell_1", "250,000");
    type_into(&mut d, "project_funding_cell_2", "40");
    let list = funding::list(&d.app.project);
    assert_eq!((list[0].amount, list[0].share), (250_000., 40.));
    assert_eq!(d.app.history.steps(), steps + 3, "each edit is one step");
    assert!(has_text(&mut d, &format::cost(250_000.)));
    assert!(has_text(&mut d, "40.00%"));
    // A refused value changes nothing and says why.
    type_into(&mut d, "project_funding_cell_2", "140");
    assert_eq!(funding::list(&d.app.project)[0].share, 40.);
    assert!(d.app.status.contains("0 to 100"), "{}", d.app.status);
    // The source combo picks another funding source.
    d.click("project_funding_cell_0");
    d.frames(2);
    assert!(d.click_text("Operating Grant"), "the source list opens");
    d.frames(2);
    assert_eq!(funding::list(&d.app.project)[0].source, "Operating Grant");
    assert_eq!(d.app.history.steps(), steps + 4);
    // A second Add takes the source still free.
    d.click("project_funding_buttons_0");
    d.frames(2);
    let list = funding::list(&d.app.project);
    assert_eq!(list.len(), 2);
    assert_eq!(list[1].source, "Capital");
    // Every source is listed: a third Add says so and adds nothing.
    d.click("project_funding_buttons_0");
    d.frames(2);
    assert_eq!(funding::list(&d.app.project).len(), 2);
    assert!(d.app.status.contains("already"), "{}", d.app.status);
    // Delete the selected (second) row, then undo it and the edits.
    d.click("project_funding_buttons_1");
    d.frames(2);
    assert_eq!(funding::list(&d.app.project).len(), 1);
    d.app.history_move(&d.ctx.clone(), false);
    d.frames(2);
    assert_eq!(
        funding::list(&d.app.project).len(),
        2,
        "undo brings it back"
    );
    for _ in 0..2 {
        d.app.history_move(&d.ctx.clone(), false);
    }
    d.frames(2);
    let list = funding::list(&d.app.project);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].source, "Capital", "undo puts the source back");
    d.app.history_move(&d.ctx.clone(), true);
    d.frames(2);
    assert_eq!(funding::list(&d.app.project)[0].source, "Operating Grant");
    assert_eq!(d.app.error, None);
}

#[test]
fn add_without_funding_sources_opens_the_funding_sources_dictionary() {
    let mut d = Driver::new("funding-none", wbs::tests::with_project_node());
    open_funding(&mut d);
    d.click("project_funding_buttons_0");
    d.frames(2);
    assert!(
        d.app.enterprise_open(),
        "Enterprise > Funding Sources opens"
    );
    assert!(funding::list(&d.app.project).is_empty());
    d.key(Key::Escape);
    d.frames(2);
    assert!(!d.app.enterprise_open());
    assert_eq!(d.app.error, None);
}

#[test]
fn funding_round_trips_through_xer_and_the_workspace() {
    let mut p = funded();
    funding::add_by_source(&mut p, "Capital", Some("1250.5"), Some("25")).unwrap();
    funding::add_by_source(&mut p, "Operating Grant", Some("3000"), Some("75")).unwrap();
    let text = p.updated_document().text();
    assert!(text.contains("%T\tPROJFUND"), "the table is exported");
    let back = ProjectData::from_document(
        Document::parse(&text).unwrap(),
        p.model.project.proj_id,
        "rt".into(),
    )
    .unwrap();
    assert_eq!(funding::list(&back), funding::list(&p));
    assert_eq!(
        back.document.table(funding::TABLE).unwrap().fields,
        openxer_xer::standard_fields(funding::TABLE).unwrap(),
    );
    // A second export is the same text.
    assert_eq!(back.updated_document().text(), text);
    // The workspace keeps it across a save and reopen.
    let path = std::env::temp_dir().join(format!("openxer-funding-{}.db", std::process::id()));
    let mut store = crate::store::Store::open(path.clone()).unwrap();
    store.save(&back).unwrap();
    let reopened = store.last().unwrap().unwrap();
    assert_eq!(funding::list(&reopened), funding::list(&p));
    drop(store);
    let _ = std::fs::remove_file(path);
}
