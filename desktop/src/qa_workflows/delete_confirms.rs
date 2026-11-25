//! Workflow: removals that ask first, as the reference does (backlog 1.12).
//! Each question is reached through the real button or menu, answered No
//! (nothing changes), then Yes (the removal happens, one undo step where it
//! is project data, and one undo brings it back). Enter presses the default
//! observed in the reference: Yes for a relationship, a layout, a filter, a
//! code and the conflicting options question; No for a resource assignment
//! and its primary resource question.
use super::*;
use crate::kit_window::Question;

/// The sample project: activities with predecessors, resources and codes.
fn sample(name: &str) -> Driver {
    Driver::new(name, openxer_xer::sample_project::sample_project().current)
}

/// The default caption of the dialog on top.
fn default_label(d: &Driver) -> Option<String> {
    crate::dialog_keys::top_default_label(&d.ctx)
}

fn undo_steps(d: &Driver) -> usize {
    d.app.history.counts().0
}

fn undo(d: &mut Driver) {
    d.click("menu_bar_edit");
    d.click("undo");
    d.frames(2);
}

/// Select activity `code` in the table and show Activity Details tab `tab`.
fn details_of(d: &mut Driver, code: &str, tab: &str) -> usize {
    d.home();
    let i = index_of(&d.app.project, code);
    select_row(d, i);
    d.click(&format!("detail_tab_{tab}"));
    d.frames(2);
    i
}

/// The box `id` is up with the observed default framed.
#[track_caller]
fn asks(d: &mut Driver, id: &str, default: &str) {
    d.frames(2);
    assert!(
        d.has(&format!("{id}_yes")) && d.has(&format!("{id}_no")),
        "{id}: the question is up"
    );
    assert_eq!(default_label(d).as_deref(), Some(default), "{id}: default");
}

#[track_caller]
fn gone(d: &mut Driver, id: &str) {
    d.frames(2);
    assert!(!d.has(&format!("{id}_yes")), "{id}: the question closed");
}

fn assignments(d: &Driver) -> Option<Vec<Vec<String>>> {
    d.app
        .project
        .document
        .table("TASKRSRC")
        .map(|t| t.rows.clone())
}

fn codes(d: &Driver) -> Option<Vec<Vec<String>>> {
    d.app
        .project
        .document
        .table("TASKACTV")
        .map(|t| t.rows.clone())
}

#[test]
fn workflow_removing_a_relationship_asks_first() {
    assert!(Question::RemoveRelationship.is_default("Yes"));
    let mut d = sample("wf-delete-confirm-link");
    details_of(&mut d, "PM0110", "relationships");
    let start = d.app.project.model.clone();
    assert!(!start.links.is_empty());
    let steps = undo_steps(&d);
    // Its predecessor PM0100, selected in the Predecessors grid.
    d.app.detail.rows[0] = Some(0);
    d.frames(1);

    // Remove asks; No keeps the relationship.
    d.click("detail_pred_remove_0");
    asks(&mut d, "remove_relationship", "Yes");
    d.click("remove_relationship_no");
    gone(&mut d, "remove_relationship");
    assert_eq!(d.app.project.model.links, start.links, "No keeps it");
    assert_eq!(undo_steps(&d), steps, "No adds no undo step");

    // Escape is No too.
    d.click("detail_pred_remove_0");
    asks(&mut d, "remove_relationship", "Yes");
    d.key(Key::Escape);
    gone(&mut d, "remove_relationship");
    assert_eq!(d.app.project.model.links, start.links, "Escape keeps it");

    // Remove again; Enter answers Yes and removes it, one undo step.
    d.app.detail.rows[0] = Some(0);
    d.frames(1);
    d.click("detail_pred_remove_0");
    asks(&mut d, "remove_relationship", "Yes");
    d.key(Key::Enter);
    gone(&mut d, "remove_relationship");
    d.assert_clean("remove relationship");
    assert_eq!(
        d.app.project.model.links.len(),
        start.links.len() - 1,
        "Yes removes the relationship"
    );
    assert_eq!(undo_steps(&d), steps + 1, "one undo step");

    undo(&mut d);
    assert_eq!(d.app.project.model.links, start.links, "undo restores it");
}

#[test]
fn workflow_removing_an_assignment_asks_and_asks_again_for_the_primary() {
    assert!(Question::RemoveAssignment.is_default("No"));
    assert!(Question::RemovePrimaryResource.is_default("No"));
    let mut d = sample("wf-delete-confirm-assignment");
    let i = details_of(&mut d, "PM0100", "resources");
    let task_id = d.app.project.model.activities[i].task_id;
    let views: Vec<_> = d
        .app
        .project
        .assignment_rows(task_id)
        .into_iter()
        .filter_map(|r| d.app.project.assignment_view(r))
        .collect();
    assert!(views.len() >= 2, "PM0100 has several assignments");
    // One of them is the primary resource (made so when the sample has none).
    let primary = match views.iter().position(|v| v.primary) {
        Some(k) => k,
        None => {
            let row = views[0].row;
            let project = &mut d.app.project;
            project.set_primary_resource(row, true).unwrap();
            d.app.touch_records();
            0
        }
    };
    let other = (0..views.len()).find(|&k| k != primary).unwrap();
    d.frames(2);
    let before = assignments(&d);
    let steps = undo_steps(&d);

    // A plain assignment: Enter answers No, nothing goes.
    d.app.detail.rows[0] = Some(other);
    d.frames(1);
    d.click("detail_rsrc_remove");
    asks(&mut d, "remove_assignment", "No");
    d.key(Key::Enter);
    gone(&mut d, "remove_assignment");
    assert_eq!(assignments(&d), before, "Enter (No) removes nothing");
    assert_eq!(undo_steps(&d), steps);

    // Yes removes it, one undo step; undo brings it back.
    d.app.detail.rows[0] = Some(other);
    d.frames(1);
    d.click("detail_rsrc_remove");
    asks(&mut d, "remove_assignment", "No");
    d.click("remove_assignment_yes");
    gone(&mut d, "remove_assignment");
    assert!(!d.has("remove_primary_yes"), "no second question");
    d.assert_clean("remove assignment");
    assert_eq!(
        d.app.project.assignment_rows(task_id).len(),
        views.len() - 1,
        "Yes removes it"
    );
    assert_eq!(undo_steps(&d), steps + 1, "one undo step");
    undo(&mut d);
    assert_eq!(assignments(&d), before, "undo restores it");

    // The primary resource: Yes to the first question asks again; No there
    // keeps it.
    let steps = undo_steps(&d);
    d.home();
    select_row(&mut d, i);
    d.app.detail.rows[0] = Some(primary);
    d.frames(1);
    d.click("detail_rsrc_remove");
    asks(&mut d, "remove_assignment", "No");
    d.click("remove_assignment_yes");
    asks(&mut d, "remove_primary", "No");
    d.key(Key::Enter);
    gone(&mut d, "remove_primary");
    assert_eq!(assignments(&d), before, "No to the second keeps it");
    assert_eq!(undo_steps(&d), steps);

    // Yes to both removes it, one undo step.
    d.app.detail.rows[0] = Some(primary);
    d.frames(1);
    d.click("detail_rsrc_remove");
    asks(&mut d, "remove_assignment", "No");
    d.click("remove_assignment_yes");
    asks(&mut d, "remove_primary", "No");
    d.click("remove_primary_yes");
    gone(&mut d, "remove_primary");
    d.assert_clean("remove primary");
    assert_eq!(
        d.app.project.assignment_rows(task_id).len(),
        views.len() - 1,
        "Yes to both removes it"
    );
    assert_eq!(undo_steps(&d), steps + 1, "one undo step");
    undo(&mut d);
    assert_eq!(assignments(&d), before, "undo restores it");
}

#[test]
fn workflow_removing_a_code_assignment_asks_first() {
    assert!(Question::RemoveCode.is_default("Yes"));
    let mut d = sample("wf-delete-confirm-code");
    let i = details_of(&mut d, "PM0100", "codes");
    let task_id = d.app.project.model.activities[i].task_id;
    let pid = d.app.project.model.project.proj_id;
    let count = |d: &Driver| {
        let rows = crate::records::rows_for(&d.app.project.document, "TASKACTV", task_id, pid);
        rows.len()
    };
    let n = count(&d);
    assert!(n > 0, "PM0100 has codes");
    let before = codes(&d);
    let steps = undo_steps(&d);
    d.app.detail.rows[0] = Some(0);
    d.frames(1);

    d.click("detail_codes_remove");
    asks(&mut d, "remove_code", "Yes");
    d.click("remove_code_no");
    gone(&mut d, "remove_code");
    assert_eq!(codes(&d), before, "No removes nothing");
    assert_eq!(undo_steps(&d), steps);

    d.app.detail.rows[0] = Some(0);
    d.frames(1);
    d.click("detail_codes_remove");
    asks(&mut d, "remove_code", "Yes");
    d.key(Key::Enter);
    gone(&mut d, "remove_code");
    d.assert_clean("remove code");
    assert_eq!(count(&d), n - 1, "Enter (Yes) removes the code");
    assert_eq!(undo_steps(&d), steps + 1, "one undo step");
    undo(&mut d);
    assert_eq!(codes(&d), before, "undo restores it");
}

#[test]
fn workflow_deleting_a_layout_asks_first() {
    assert!(Question::DeleteLayout.is_default("Yes"));
    let mut d = sample("wf-delete-confirm-layout");
    d.home();
    d.app.save_layout_as("Crew View").unwrap();
    d.frames(2);
    let has = |d: &Driver| d.app.grid.library.iter().any(|l| l.name == "Crew View");
    assert!(has(&d));
    let open = |d: &mut Driver| {
        d.home();
        d.click("menu_bar_view");
        assert!(d.hover_text("Layout"), "View > Layout");
        d.click("layout_open");
        d.frames(2);
        let k = d
            .app
            .layout_names()
            .iter()
            .position(|n| n == "Crew View")
            .unwrap();
        d.click(&format!("open_layout_item_{k}"));
    };

    open(&mut d);
    d.click("open_layout_delete");
    asks(&mut d, "remove_layout", "Yes");
    d.click("remove_layout_no");
    gone(&mut d, "remove_layout");
    assert!(has(&d), "No keeps the layout");
    assert!(
        d.app.grid.dialogs.open_layout.is_some(),
        "Open Layout stays up"
    );

    d.click("open_layout_delete");
    asks(&mut d, "remove_layout", "Yes");
    d.key(Key::Enter);
    gone(&mut d, "remove_layout");
    d.assert_clean("delete layout");
    assert!(!has(&d), "Enter (Yes) deletes the layout");
    // Layouts are user settings, not project data: Edit > Undo does not
    // cover them, as before.
    d.click("open_layout_cancel");
}

const FILTER: &str = "Pump work";

/// The user filter is saved (OK in Filters keeps the dialog's list).
fn filter_saved(d: &Driver) -> bool {
    d.app.grid.user_filters.iter().any(|u| u.name == FILTER)
}

#[test]
fn workflow_deleting_a_filter_asks_first() {
    assert!(Question::DeleteFilter.is_default("Yes"));
    let mut d = sample("wf-delete-confirm-filter");
    let filter = crate::grid::filters::UserFilter {
        name: FILTER.into(),
        match_all: true,
        rows: vec![crate::grid::filters::Criterion::default()],
    };
    d.app.grid.user_filters.push(filter);
    let open = |d: &mut Driver| {
        d.home();
        d.click("menu_bar_view");
        assert!(d.hover_text("Filter By"), "View > Filter By");
        assert!(d.has("menu_filters"), "View > Filter By > Filters...");
        d.click("menu_filters");
        d.frames(2);
        // The user filters come after the standard ones, below the list's
        // first screen: select it as a click would (the list scrolls to it).
        let draft = d.app.grid.dialogs.filters.as_mut().expect("Filters is up");
        draft.current = Some(format!("user:{FILTER}"));
        d.frames(2);
    };
    let listed = |d: &Driver| {
        d.app
            .grid
            .dialogs
            .filters
            .as_ref()
            .is_some_and(|f| f.users.iter().any(|u| u.name == FILTER))
    };

    open(&mut d);
    d.click("filters_delete");
    asks(&mut d, "remove_filter", "Yes");
    d.click("remove_filter_no");
    gone(&mut d, "remove_filter");
    assert!(listed(&d), "No keeps the filter");

    d.click("filters_delete");
    asks(&mut d, "remove_filter", "Yes");
    d.key(Key::Enter);
    gone(&mut d, "remove_filter");
    assert!(!listed(&d), "Enter (Yes) takes it out of the list");
    // Cancel drops the dialog's changes: the filter is still there.
    d.click("filters_cancel");
    d.frames(2);
    assert!(filter_saved(&d), "Cancel keeps it");
    // Yes then OK deletes it.
    open(&mut d);
    d.click("filters_delete");
    d.click("remove_filter_yes");
    d.click("filters_ok");
    d.frames(2);
    d.assert_clean("delete filter");
    assert!(!filter_saved(&d), "Yes and OK delete it");
}

#[test]
fn workflow_scheduling_open_projects_with_different_options_asks_first() {
    assert!(Question::ConflictingScheduleOptions.is_default("Yes"));
    let mut d = fresh("qa_delete_confirm_options");
    let a = new_project(&mut d, "OPT-A", "Alpha Unit", "04-Jan-27", "");
    show(&mut d, "activities");
    add_activity(&mut d, "Set pump skid", "3d");
    d.key_with(Key::S, ctrl());
    let b = new_project(&mut d, "OPT-B", "Bravo Unit", "04-Jan-27", "");
    show(&mut d, "activities");
    add_activity(&mut d, "Set new pump", "2d");
    d.key_with(Key::S, ctrl());
    open_projects(&mut d, &[a, b]);
    // The two projects' options differ.
    let parked = &mut d.app.open_set.parked[0].project.model.options;
    parked.lag_from_early_start = !parked.lag_from_early_start;
    let own: Vec<_> = d
        .app
        .open_projects()
        .iter()
        .map(|p| p.model.options.clone())
        .collect();
    assert!(d.app.conflicting_options().is_some());

    // F9, Schedule: the question; No runs nothing and keeps the dialog.
    d.home();
    d.key(Key::F9);
    let runs = d.app.schedule_runs;
    d.click("schedule_run");
    asks(&mut d, "options_conflict", "Yes");
    d.click("options_conflict_no");
    gone(&mut d, "options_conflict");
    assert_eq!(d.app.schedule_runs, runs, "No schedules nothing");
    assert!(d.app.sched.schedule_open, "the Schedule dialog stays up");

    // Schedule again; Enter answers Yes and both are scheduled.
    let before: Vec<_> = d.app.open_projects().into_iter().cloned().collect();
    d.click("schedule_run");
    asks(&mut d, "options_conflict", "Yes");
    d.key(Key::Enter);
    gone(&mut d, "options_conflict");
    d.frames(2);
    d.assert_clean("schedule with the default's options");
    assert_eq!(d.app.schedule_runs, runs + 1, "Yes schedules");
    assert!(!d.app.sched.schedule_open);
    let options: Vec<_> = d
        .app
        .open_projects()
        .iter()
        .map(|p| p.model.options.clone())
        .collect();
    assert_eq!(options, own, "each project keeps the options it stores");

    // One undo puts both back.
    undo(&mut d);
    for (p, was) in d.app.open_projects().into_iter().zip(&before) {
        let same = p.model.activities == was.model.activities;
        assert!(same, "undo restores {}", p.model.project.name);
    }
}
