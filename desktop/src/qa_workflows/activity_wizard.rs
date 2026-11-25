//! Workflow: Edit > Add with the New Activity wizard, against the observed
//! pages (private observation notes). The switch is
//! turned on in User Preferences, Assistance (test harnesses start with it
//! off), then: the nine pages with both questions answered No, to Finish;
//! both answered Yes, through Predecessor Activities, Successor Activities,
//! Expenses, Activity Codes and Work Products and Documents; Finish on the
//! first page; the Activity ID refusals; Cancel part way and Escape; "Skip
//! this wizard next time" turning the wizard off so the next Add inserts
//! the row at once; and back on in User Preferences. Each wizard result is
//! one undo step.
use super::*;
use crate::activity_wizard::Page;
use openxer_engine::model::{LinkType, TaskType};

const DAY: i64 = 8 * 60;

/// User Preferences > Assistance > Add activities through the wizard,
/// ticked or cleared; returns what the box showed before.
fn set_wizard(d: &mut Driver, on: bool) -> bool {
    d.home();
    d.click("menu_bar_edit");
    d.click("menu_user_preferences");
    d.click("prefs_tab_assistance");
    let before = d.app.activity_wizard.assistance.activity_wizard;
    if before != on {
        d.click("prefs_wizard_activity");
    }
    d.click("prefs_close");
    d.frames(1);
    assert_eq!(d.app.activity_wizard.assistance.activity_wizard, on);
    before
}

/// The switch as the workspace keeps it.
fn stored_switch(d: &Driver) -> bool {
    let text = d
        .app
        .store
        .preference(openxer_automation::user_prefs::ASSISTANCE_KEY)
        .unwrap()
        .expect("the switch is stored");
    openxer_automation::user_prefs::Assistance::from_json(&text).activity_wizard
}

/// Edit > Add from the menu bar.
fn edit_add(d: &mut Driver) {
    d.click("menu_bar_edit");
    d.click("menu_add");
    d.frames(2);
}

fn wizard(d: &Driver) -> &crate::activity_wizard::Wizard {
    d.app
        .activity_wizard
        .open
        .as_ref()
        .expect("the wizard is open")
}

fn page(d: &Driver) -> Page {
    wizard(d).page
}

fn next(d: &mut Driver, to: Page) {
    d.click("new_activity_wizard_next");
    assert_eq!(page(d), to, "Next goes to {to:?}");
}

fn finish(d: &mut Driver) {
    d.click("new_activity_wizard_finish");
    d.frames(2);
    d.assert_clean("finish");
    assert!(
        d.app.activity_wizard.open.is_none(),
        "Finish closes the wizard: {:?}",
        d.app
            .activity_wizard
            .open
            .as_ref()
            .and_then(|w| w.message.clone())
    );
}

/// The refusal shown, then OK.
fn refused(d: &mut Driver) -> String {
    let text = wizard(d)
        .message
        .as_ref()
        .map(|(_, t)| t.clone())
        .expect("a refusal is shown");
    d.click("new_activity_message_ok");
    d.frames(2);
    assert!(wizard(d).message.is_none(), "OK closes the message");
    text
}

/// Click a text field, select what it holds and delete it.
fn clear(d: &mut Driver, action: &str) {
    d.click(action);
    d.key_with(Key::A, ctrl());
    d.key(Key::Backspace);
    d.frames(1);
}

fn by_code(p: &ProjectData, code: &str) -> Option<usize> {
    p.model.activities.iter().position(|a| a.task_code == code)
}

/// Rows of `table` that belong to activity `task_id`.
fn rows_of(p: &ProjectData, table: &str, task_id: i64) -> usize {
    p.document.table(table).map_or(0, |t| {
        t.rows
            .iter()
            .filter(|r| crate::records::num(t.value(r, "task_id")) == task_id)
            .count()
    })
}

/// Budgeted labor units of the activity's assignments, in hours.
fn assigned_labor(p: &ProjectData, task_id: i64) -> f64 {
    let t = p.document.table("TASKRSRC").unwrap();
    t.rows
        .iter()
        .filter(|r| crate::records::num(t.value(r, "task_id")) == task_id)
        .map(|r| t.value(r, "target_qty").parse::<f64>().unwrap_or(0.))
        .sum()
}

/// Every relationship by Activity ID, sorted.
fn links(p: &ProjectData) -> Vec<(String, String, LinkType, i64)> {
    let code = |i: usize| p.model.activities[i].task_code.clone();
    let mut all: Vec<_> = p
        .model
        .links
        .iter()
        .map(|l| (code(l.pred), code(l.succ), l.link_type, l.lag))
        .collect();
    all.sort_by(|x, y| (&x.0, &x.1).cmp(&(&y.0, &y.1)));
    all
}

/// The scheduled sample project with an activity code ("Area", value A1)
/// and one document, for the More Details pages.
fn project_with_codes_and_a_document() -> ProjectData {
    let mut p = openxer_xer::starter();
    openxer_xer::edit::tables::add_code_type(&mut p, "Area", "project", None).unwrap();
    openxer_xer::edit::tables::add_code_value(&mut p, "Area", "A1").unwrap();
    openxer_xer::edit::documents::add(&mut p, None);
    schedule_model(&mut p.model, &p.resources).unwrap();
    p
}

#[test]
fn workflow_new_activity_wizard_all_pages_finish_and_undo() {
    let mut d = fresh("wf-activity-wizard");
    // A resource for the Resources page (the sample project has none).
    let pipefitters = d
        .app
        .add_named_resource("PF", "Pipefitter crew", 0.)
        .unwrap();
    d.frames(2);
    assert!(
        openxer_automation::user_prefs::Assistance::default().activity_wizard,
        "the wizard is on by default"
    );
    assert!(!set_wizard(&mut d, true), "test harnesses start it off");
    assert!(stored_switch(&d), "kept in the workspace");

    // Select RB-1020: the wizard starts in its WBS with the next ID.
    d.home();
    let from = index_of(&d.app.project, "RB-1020");
    select_row(&mut d, from);
    let wbs_of = |p: &ProjectData, i: usize| p.details[&p.model.activities[i].task_id].wbs;
    let from_wbs = wbs_of(&d.app.project, from);
    let expected_id = openxer_xer::edit::new_activity::new_activity_id(&d.app.project, Some(from));
    let start = d.app.project.clone();
    let undo = d.app.history.counts().0;
    let other_wbs = d
        .app
        .project
        .wbs
        .iter()
        .find(|w| w.id != from_wbs)
        .map(|w| (w.id, format!("{}  {}", w.code, w.name)))
        .expect("a second WBS element");

    edit_add(&mut d);
    let mut visited = vec![page(&d)];
    {
        let w = wizard(&d);
        assert_eq!(w.page, Page::Name);
        assert_eq!(w.id, expected_id, "the ID from the numbering defaults");
        assert_eq!(w.name, "New Activity");
        assert_eq!(w.wbs, from_wbs, "the selected activity's WBS");
        assert_eq!(w.task_type, TaskType::Task, "Defaults tab type");
        assert_eq!(w.duration_type, "DT_FixedDUR2");
    }
    assert_eq!(d.app.project.model, start.model, "nothing added yet");
    // Cancel stands apart at the bottom left, as observed.
    let cancel = d.rect("new_activity_wizard_cancel");
    let prev = d.rect("new_activity_wizard_back");
    assert!(
        cancel.right() < prev.left() - 50. && (cancel.top() - prev.top()).abs() < 1.,
        "Cancel at the left of the button line: {cancel:?} {prev:?}"
    );
    // The ID is selected, so typing replaces it.
    d.type_text("WZ-100");
    d.frames(1);
    assert_eq!(wizard(&d).id, "WZ-100");
    type_into(&mut d, "new_activity_name", "Set pump skid P-101");

    next(&mut d, Page::Wbs);
    visited.push(Page::Wbs);
    d.click("new_activity_wbs");
    assert!(
        d.click_text(&other_wbs.1),
        "WBS {:?} is offered",
        other_wbs.1
    );
    assert_eq!(wizard(&d).wbs, other_wbs.0);

    next(&mut d, Page::Type);
    visited.push(Page::Type);
    d.click("new_activity_type");
    assert!(d.click_text("Resource Dependent"));
    assert_eq!(wizard(&d).task_type, TaskType::Resource);

    next(&mut d, Page::Resources);
    visited.push(Page::Resources);
    let assignments_before = d
        .app
        .project
        .document
        .table("TASKRSRC")
        .map_or(0, |t| t.rows.len());
    d.click("new_activity_resources_add");
    assert!(
        d.app.detail.assign.is_some(),
        "Assign opens Assign Resources"
    );
    assert!(d.click_text("Pipefitter crew"));
    d.click("rsrc_picker_assign");
    d.click("rsrc_picker_close");
    assert!(d.app.detail.assign.is_none());
    assert_eq!(wizard(&d).resources, vec![pipefitters]);
    assert_eq!(
        d.app
            .project
            .document
            .table("TASKRSRC")
            .map_or(0, |t| t.rows.len()),
        assignments_before,
        "picking for the wizard assigns nothing yet"
    );

    next(&mut d, Page::DurationType);
    visited.push(Page::DurationType);
    d.click("new_activity_duration_type");
    assert!(d.click_text("Fixed Units"));
    assert_eq!(wizard(&d).duration_type, "DT_FixedQty");

    next(&mut d, Page::Units);
    visited.push(Page::Units);
    type_into(&mut d, "new_activity_labor", "24");
    type_into(&mut d, "new_activity_duration", "3");

    next(&mut d, Page::Dependent);
    visited.push(Page::Dependent);
    assert!(!wizard(&d).relationships, "No is the default");
    next(&mut d, Page::MoreDetails);
    visited.push(Page::MoreDetails);
    assert!(!wizard(&d).more_details, "No is the default");
    next(&mut d, Page::Done);
    visited.push(Page::Done);
    assert_eq!(visited.len(), 9, "nine pages: {visited:?}");
    // Prev goes back a page and Next returns.
    d.click("new_activity_wizard_back");
    assert_eq!(page(&d), Page::MoreDetails);
    next(&mut d, Page::Done);
    finish(&mut d);

    let p = &d.app.project;
    let i = by_code(p, "WZ-100").expect("the activity was added");
    assert_eq!(d.app.selected, Some(i), "the new activity is selected");
    let a = &p.model.activities[i];
    assert_eq!(p.name(i), "Set pump skid P-101");
    assert_eq!(wbs_of(p, i), other_wbs.0);
    assert_eq!(a.task_type, TaskType::Resource);
    assert_eq!(
        crate::records::task_value(
            &p.document,
            a.task_id,
            p.model.project.proj_id,
            "duration_type"
        ),
        "DT_FixedQty"
    );
    assert_eq!(p.details[&a.task_id].original_duration, 3 * DAY);
    assert_eq!(p.assignment_rows(a.task_id).len(), 1, "one assignment");
    assert!((assigned_labor(p, a.task_id) - 24.).abs() < 1e-6);
    assert!(d.app.edit_cell.is_none(), "no cell opens for typing");
    assert_eq!(d.app.history.counts().0, undo + 1, "one undo step");

    // One undo takes the activity and its assignment back.
    d.click("menu_bar_edit");
    d.click("undo");
    d.frames(2);
    assert!(by_code(&d.app.project, "WZ-100").is_none());
    assert_eq!(d.app.project.model, start.model, "undo restores it all");
    assert_eq!(
        d.app
            .project
            .document
            .table("TASKRSRC")
            .map_or(0, |t| t.rows.len()),
        assignments_before
    );
    assert_eq!(d.app.history.counts().0, undo);
}

#[test]
fn workflow_new_activity_wizard_yes_pages_and_early_finish() {
    let mut d = Driver::new(
        "wf-activity-wizard-yes",
        project_with_codes_and_a_document(),
    );
    set_wizard(&mut d, true);
    d.home();
    let rb1000 = index_of(&d.app.project, "RB-1000");
    select_row(&mut d, rb1000);
    let start = d.app.project.clone();
    let undo = d.app.history.counts().0;

    // Finish on the first page, from the Add button: the activity Add
    // makes, with the name typed there.
    let expected_id =
        openxer_xer::edit::new_activity::new_activity_id(&d.app.project, d.app.selected);
    d.click("add");
    d.frames(2);
    assert_eq!(page(&d), Page::Name, "the Add button opens the wizard");
    type_into(&mut d, "new_activity_name", "Pour pump base");
    finish(&mut d);
    let p = &d.app.project;
    let i = by_code(p, &expected_id).expect("added with the numbered ID");
    assert_eq!(p.name(i), "Pour pump base", "the typed name is kept");
    assert_eq!(
        p.details[&p.model.activities[i].task_id].original_duration,
        5 * DAY
    );
    assert_eq!(p.model.activities[i].task_type, TaskType::Task);
    assert_eq!(d.app.history.counts().0, undo + 1, "one undo step");
    d.click("menu_bar_edit");
    d.click("undo");
    d.frames(2);
    assert_eq!(d.app.project.model, start.model);

    // Insert opens it as well. Yes on Dependent Activities adds the
    // Predecessor and Successor Activities pages; Yes on More Details adds
    // Expenses, Activity Codes and Work Products and Documents.
    d.home();
    select_row(&mut d, rb1000);
    d.key(Key::Insert);
    d.frames(2);
    assert_eq!(page(&d), Page::Name, "Insert opens the wizard");
    d.type_text("WZ-200");
    for to in [
        Page::Wbs,
        Page::Type,
        Page::Resources,
        Page::DurationType,
        Page::Units,
        Page::Dependent,
    ] {
        next(&mut d, to);
    }
    d.click("new_activity_relationships_on");
    next(&mut d, Page::Predecessors);
    d.click("new_activity_predecessors_add");
    assert!(d.app.detail.picker.is_some(), "Assign Predecessors opens");
    assert!(d.click_text("RB-1010"));
    d.click("picker_assign");
    d.click("picker_close");
    next(&mut d, Page::Successors);
    d.click("new_activity_successors_add");
    assert!(d.click_text("RB-1040"));
    d.click("picker_assign");
    d.click("picker_close");
    assert_eq!(wizard(&d).links.len(), 2, "two relationships listed");
    assert_eq!(
        d.app.project.model.links, start.model.links,
        "none added yet"
    );
    next(&mut d, Page::MoreDetails);
    d.click("new_activity_more_on");
    next(&mut d, Page::Expenses);
    d.click("new_activity_expenses_add");
    type_into(&mut d, "new_activity_expense_name", "Crane hire");
    type_into(&mut d, "new_activity_expense_cost", "250");
    assert_eq!(wizard(&d).expenses.len(), 1);
    next(&mut d, Page::Codes);
    d.click("new_activity_codes_add");
    assert!(d.app.detail.picker.is_some(), "Assign Activity Codes opens");
    assert!(d.click_text("A1"), "code value A1 is offered");
    d.click("picker_assign");
    d.click("picker_close");
    assert_eq!(wizard(&d).codes.len(), 1);
    next(&mut d, Page::Documents);
    d.click("new_activity_documents_add");
    assert!(d.click_text("(New Document)"), "the document is offered");
    d.click("picker_assign");
    d.click("picker_close");
    assert_eq!(wizard(&d).documents.len(), 1);
    next(&mut d, Page::Done);
    assert_eq!(
        wizard(&d).pages().len(),
        14,
        "nine pages and the five Yes adds"
    );
    finish(&mut d);

    let p = &d.app.project;
    let i = by_code(p, "WZ-200").expect("WZ-200 was added");
    let task_id = p.model.activities[i].task_id;
    let mut expected = links(&start);
    expected.push(("RB-1010".into(), "WZ-200".into(), LinkType::FS, 0));
    expected.push(("WZ-200".into(), "RB-1040".into(), LinkType::FS, 0));
    expected.sort_by(|x, y| (&x.0, &x.1).cmp(&(&y.0, &y.1)));
    assert_eq!(links(p), expected);
    assert_eq!(rows_of(p, "PROJCOST", task_id), 1, "the expense");
    assert_eq!(rows_of(p, "TASKACTV", task_id), 1, "the code value");
    assert_eq!(rows_of(p, "TASKDOC", task_id), 1, "the document");
    let cost = p.document.table("PROJCOST").unwrap();
    let expense = cost
        .rows
        .iter()
        .find(|r| crate::records::num(cost.value(r, "task_id")) == task_id)
        .unwrap();
    assert_eq!(cost.value(expense, "cost_name"), "Crane hire");
    assert_eq!(
        cost.value(expense, "target_cost").parse::<f64>().ok(),
        Some(250.)
    );
    assert_eq!(d.app.history.counts().0, undo + 1, "everything is one step");
    d.click("menu_bar_edit");
    d.click("undo");
    d.frames(2);
    assert_eq!(d.app.project.model, start.model, "one undo takes all back");
    assert_eq!(rows_of(&d.app.project, "PROJCOST", task_id), 0);
    // The relationships survive an XER round trip.
    d.click("menu_bar_edit");
    d.click("redo");
    d.frames(2);
    let reread = xer_round_trip(&mut d, "wf-activity-wizard-yes");
    assert_eq!(links(&reread), links(&d.app.project));
}

#[test]
fn workflow_new_activity_wizard_cancel_refusal_and_switch_off() {
    let mut d = fresh("wf-activity-wizard-cancel");
    set_wizard(&mut d, true);
    d.home();
    let start = d.app.project.clone();
    let undo = d.app.history.counts().0;

    // Cancel part way adds nothing.
    edit_add(&mut d);
    next(&mut d, Page::Wbs);
    next(&mut d, Page::Type);
    next(&mut d, Page::Resources);
    d.click("new_activity_wizard_cancel");
    d.frames(2);
    assert!(d.app.activity_wizard.open.is_none());
    assert_eq!(d.app.project.model, start.model, "Cancel adds nothing");
    assert_eq!(d.app.history.counts().0, undo);
    // So does Escape.
    edit_add(&mut d);
    d.key(Key::Escape);
    d.frames(2);
    assert!(d.app.activity_wizard.open.is_none(), "Escape cancels");
    assert_eq!(d.app.project.model, start.model);

    // Only the Activity ID is required. A blank one stops Next on the
    // first page with a message; a blank name does not.
    edit_add(&mut d);
    clear(&mut d, "new_activity_id");
    clear(&mut d, "new_activity_name");
    assert_eq!(wizard(&d).id, "");
    d.click("new_activity_wizard_next");
    assert!(refused(&mut d).contains("Activity ID"));
    assert_eq!(page(&d), Page::Name, "still on the first page");
    // An ID in use is refused by Next and by Finish.
    type_into(&mut d, "new_activity_id", "RB-1000");
    d.click("new_activity_wizard_next");
    assert!(refused(&mut d).contains("RB-1000"));
    assert_eq!(page(&d), Page::Name);
    d.click("new_activity_wizard_finish");
    d.frames(2);
    assert!(refused(&mut d).contains("RB-1000"));
    assert_eq!(d.app.project.model, start.model);
    assert_eq!(d.app.history.counts().0, undo);
    // A free ID with a blank name finishes.
    type_into(&mut d, "new_activity_id", "WZ-300");
    finish(&mut d);
    let i = by_code(&d.app.project, "WZ-300").expect("WZ-300 was added");
    assert_eq!(d.app.project.name(i), "", "a blank name is accepted");
    d.click("menu_bar_edit");
    d.click("undo");
    d.frames(2);
    assert_eq!(d.app.project.model, start.model);

    // "Skip this wizard next time", then Cancel: the switch is off.
    d.home();
    edit_add(&mut d);
    d.click("new_activity_skip");
    d.click("new_activity_wizard_cancel");
    d.frames(2);
    assert!(!d.app.activity_wizard.assistance.activity_wizard);
    assert!(!stored_switch(&d), "off in the workspace");
    // The next Add inserts the row at once with its name open for typing.
    d.home();
    edit_add(&mut d);
    assert!(d.app.activity_wizard.open.is_none(), "no wizard");
    assert_eq!(
        d.app.project.model.activities.len(),
        start.model.activities.len() + 1,
        "the row is in"
    );
    let i = d.app.selected.unwrap();
    assert_eq!(d.app.edit_cell, Some((i, "activity_name".to_string())));
    d.key(Key::Escape);
    // User Preferences shows it off; tick it again and Add opens the wizard.
    assert!(!set_wizard(&mut d, true), "Assistance reads off");
    assert!(stored_switch(&d));
    d.home();
    edit_add(&mut d);
    assert_eq!(page(&d), Page::Name, "the wizard is back");
    d.click("new_activity_wizard_cancel");
}
