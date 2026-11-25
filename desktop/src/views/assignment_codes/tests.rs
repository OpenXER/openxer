use super::*;
use crate::qa_sweep::Driver;
use crate::views::assignments::{assignments, Key as RowKey};

const KIND: CodeKind = CodeKind::Assignment;

/// The starter with one assignment code type and two values.
fn with_codes() -> (ProjectData, i64, i64, i64) {
    let mut p = crate::views::resources::tests::with_resource();
    let doc = &mut p.document;
    let t = ops::add_code_type(doc, KIND, ops::Scope::Global, 0, Some("Crew Shift")).unwrap();
    let day = ops::add_code_value(doc, KIND, t, None).unwrap();
    ops::set_code_value(doc, KIND, t, day, ops::ValueField::Short, "DAY").unwrap();
    ops::set_code_value(doc, KIND, t, day, ops::ValueField::Name, "Day shift").unwrap();
    let night = ops::add_code_value(doc, KIND, t, Some(day)).unwrap();
    ops::set_code_value(doc, KIND, t, night, ops::ValueField::Short, "NIGHT").unwrap();
    (p, t, day, night)
}

#[test]
fn assignment_codes_round_trip_through_xer_and_the_workspace() {
    let (mut p, t, day, night) = with_codes();
    let a = assignments(&p)[0].id;
    assign(&mut p, a, t, day);
    assign(&mut p, a, t, night);
    let got = assigned(&p, a);
    assert_eq!(got.len(), 1, "one value per code");
    assert_eq!(
        got[0].2,
        ["Crew Shift".to_string(), "NIGHT".into(), String::new()]
    );
    // Export and reimport.
    let text = p.updated_document().text();
    for table in ["ASGNMNTCATTYPE", "ASGNMNTCATVAL", "ASGNMNTACAT"] {
        assert!(
            text.contains(&format!("%T\t{table}")),
            "{table} is exported"
        );
    }
    let doc = Document::parse(&text).unwrap();
    let pid = p.model.project.proj_id;
    let q = ProjectData::from_document(doc, pid, "x".into()).unwrap();
    assert_eq!(assigned(&q, a), got);
    assert_eq!(ops::code_values(&q.document, KIND, t).len(), 2);
    // Save and reload through the workspace.
    let dir = crate::reports::test_support::temp("assignment-codes");
    let mut store = Store::open(dir.join("w.db")).unwrap();
    let id = store.save(&p).unwrap();
    assert_eq!(assigned(&store.load(id).unwrap(), a), got);
    // Deleting the activity takes its assignment's code rows along.
    let task = p.model.activities[assignments(&p)[0].task].task_id;
    p.delete_activities(&[task]).unwrap();
    assert!(assigned(&p, a).is_empty());
    assert!(p
        .document
        .table(TABLE)
        .is_none_or(|t| t.rows.iter().all(|r| num(t.value(r, "taskrsrc_id")) != a)));
}

#[test]
fn codes_tab_assigns_and_removes_as_undo_steps() {
    let (p, _, _, _) = with_codes();
    let mut d = Driver::new("assignment-codes", p);
    d.app.show_view(MainView::Assignments);
    d.frames(2);
    let a = assignments(&d.app.project)[0].id;
    d.app.views.assignments.selected = Some(RowKey::Assignment(a));
    d.app.views.assignments.tab = 3;
    d.frames(2);
    let undo = d.app.history.counts().0;
    d.click("assignment_codes_buttons_0");
    assert!(d.app.dictionaries.assignment_pick.is_some());
    // Escape closes the picker without a change.
    d.key(Key::Escape);
    assert!(d.app.dictionaries.assignment_pick.is_none());
    assert_eq!(d.app.history.counts().0, undo);
    d.click("assignment_codes_buttons_0");
    d.click("assignment_pick_value_1");
    d.click("assignment_pick_assign");
    d.click("assignment_pick_close");
    assert_eq!(assigned(&d.app.project, a)[0].2[1], "NIGHT");
    assert_eq!(d.app.history.counts().0, undo + 1);
    d.click("assignment_codes_row_0");
    d.click("assignment_codes_buttons_1");
    assert!(assigned(&d.app.project, a).is_empty());
    assert_eq!(d.app.history.counts().0, undo + 2);
    let ctx = d.ctx.clone();
    d.app.history_move(&ctx, false);
    assert_eq!(assigned(&d.app.project, a).len(), 1, "undo puts it back");
}

#[test]
fn assignment_codes_dictionary_opens_from_the_enterprise_menu_and_toolbar() {
    let mut d = Driver::new(
        "assignment-dict",
        crate::reports::test_support::scheduled_starter(),
    );
    d.click("menu_bar_enterprise");
    d.click("menu_assignment_codes");
    assert_eq!(
        d.app.dictionaries.codes.as_ref().map(|c| c.kind),
        Some(KIND)
    );
    d.key(Key::Escape);
    d.key(Key::Escape);
    d.frames(2);
    assert!(!d.app.dictionary_open());
    d.click("tool_assignment_codes");
    assert!(d.app.dictionary_open());
}
