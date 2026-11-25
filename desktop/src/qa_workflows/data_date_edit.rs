//! Workflow: the data date on Project Details, Dates tab, typed in the
//! Projects window (one undo step, nothing rescheduled), and scheduling
//! open projects whose data dates differ: no question, as the reference
//! asks none; the Schedule dialog's two radios decide (each project its own
//! date, the default, or the selected date for all).
use super::*;

fn minutes(p: &ProjectData, text: &str) -> i64 {
    openxer_xer::parse_relative(p.model.origin_serial, text)
        .unwrap()
        .unwrap()
}

/// Early start of the activity named `name` in `p`.
fn start_of(p: &ProjectData, name: &str) -> Option<i64> {
    (0..p.model.activities.len())
        .find(|&i| p.name(i) == name)
        .and_then(|i| p.model.activities[i].early_start)
}

/// Projects window, Dates tab of workspace project `id`.
fn dates_tab(d: &mut Driver, id: i64) {
    projects_select(d, id);
    let tab = views::projects::TABS
        .iter()
        .position(|t| *t == "Dates")
        .unwrap();
    d.click(&format!("projects_tab_{tab}"));
    assert_eq!(d.app.views.projects.tab, tab);
}

#[test]
fn workflow_data_date_is_typed_on_the_dates_tab_as_one_undo_step() {
    let mut d = fresh("qa_data_date_tab");
    let a = new_project(&mut d, "DD-A", "Alpha Unit", "04-Jan-27", "");
    show(&mut d, "activities");
    add_activity(&mut d, "Set pump skid", "5d");
    d.f9();
    d.key_with(Key::S, ctrl());
    let p = d.app.project.clone();
    let start = start_of(&p, "Set pump skid");
    assert_eq!(start, Some(minutes(&p, "2027-01-04 08:00")));
    let old = p.model.project.data_date;
    let new = minutes(&p, "2027-01-11 08:00");

    // Project Details, Dates tab: type the new data date and press Enter.
    dates_tab(&mut d, a);
    let steps = d.app.history.counts().0;
    type_into(&mut d, "project_data_date", "11-Jan-27 08:00");
    d.key(Key::Enter);
    d.frames(2);
    d.assert_clean("data date");
    assert_eq!(
        d.app.project.model.project.data_date,
        Some(new),
        "the Dates tab sets the data date: {}",
        d.app.status
    );
    assert_eq!(d.app.history.counts().0, steps + 1, "one undo step");
    assert_eq!(
        start_of(&d.app.project, "Set pump skid"),
        start,
        "nothing is rescheduled by the change itself"
    );
    // Undo and redo it.
    d.home();
    d.click("undo");
    d.frames(2);
    assert_eq!(d.app.project.model.project.data_date, old, "undo");
    d.click("redo");
    d.frames(2);
    assert_eq!(d.app.project.model.project.data_date, Some(new), "redo");
    // Saved, it is the stored data date; F9 then schedules from it.
    // (A stored project may count minutes from another origin: compare
    // each against the date read in its own terms.)
    d.key_with(Key::S, ctrl());
    d.assert_clean("save");
    let back = stored(&d, a);
    assert_eq!(
        back.model.project.data_date,
        Some(minutes(&back, "2027-01-11 08:00")),
        "the stored data date"
    );
    d.f9();
    let p = &d.app.project;
    assert_eq!(
        start_of(p, "Set pump skid"),
        Some(minutes(p, "2027-01-11 08:00")),
        "F9 schedules from the new data date"
    );
}

#[test]
fn workflow_scheduling_open_projects_with_different_data_dates_asks_nothing() {
    let mut d = fresh("qa_data_date_radios");
    let a = new_project(&mut d, "DD-A", "Alpha Unit", "04-Jan-27", "");
    show(&mut d, "activities");
    add_activity(&mut d, "Demo old pump base", "3d");
    d.key_with(Key::S, ctrl());
    let b = new_project(&mut d, "DD-B", "Bravo Unit", "11-Jan-27", "");
    show(&mut d, "activities");
    add_activity(&mut d, "Set new pump", "2d");
    d.key_with(Key::S, ctrl());
    open_projects(&mut d, &[a, b]);
    let dates = |d: &Driver| -> Vec<Option<i64>> {
        [a, b]
            .iter()
            .map(|&id| {
                let p = open_project(d, id);
                p.model
                    .project
                    .data_date
                    .map(|x| x + p.model.origin_serial * 1440)
            })
            .collect()
    };
    let before = dates(&d);
    assert_ne!(before[0], before[1], "the two data dates differ");

    // F9: the Schedule dialog offers the two data date choices, each
    // project its own date chosen.
    d.home();
    d.key(Key::F9);
    assert!(d.app.sched.schedule_open, "F9 opens the Schedule dialog");
    assert!(
        d.has("schedule_own_date") && d.has("schedule_apply_date"),
        "both data date radios are offered"
    );
    assert!(d.app.sched.own_data_date, "own data date is the default");
    // Schedule: no question, both run at once, each from its own date.
    let runs = d.app.schedule_runs;
    d.click("schedule_run");
    d.frames(2);
    d.assert_clean("schedule, own dates");
    assert_eq!(
        d.app.schedule_runs,
        runs + 1,
        "scheduled without a question: {}",
        d.app.status
    );
    assert!(
        !d.app.sched.schedule_open,
        "the dialog closed after the run"
    );
    assert_eq!(dates(&d), before, "each project keeps its own data date");
    let pa = open_project(&d, a).clone();
    assert_eq!(
        start_of(&pa, "Demo old pump base"),
        Some(minutes(&pa, "2027-01-04 08:00")),
        "DD-A runs from its own date"
    );
    let pb = open_project(&d, b).clone();
    assert_eq!(
        start_of(&pb, "Set new pump"),
        Some(minutes(&pb, "2027-01-11 08:00")),
        "DD-B runs from its own date"
    );
    // Apply selected data date to all open projects: every project takes
    // the date typed in the box, again without a question.
    d.home();
    d.key(Key::F9);
    d.click("schedule_apply_date");
    d.frames(1);
    assert!(!d.app.sched.own_data_date);
    type_into(&mut d, "schedule_data_date", "18-Jan-27 08:00");
    d.click("schedule_run");
    d.frames(2);
    d.assert_clean("schedule, one date");
    assert_eq!(
        d.app.schedule_runs,
        runs + 2,
        "scheduled without a question"
    );
    assert!(!d.app.sched.schedule_open);
    let after = dates(&d);
    assert_eq!(after[0], after[1], "both take the one date");
    for id in [a, b] {
        let p = open_project(&d, id).clone();
        assert_eq!(
            p.model.project.data_date,
            Some(minutes(&p, "2027-01-18 08:00")),
            "{} takes the typed date",
            p.model.project.name
        );
    }
    let pb = open_project(&d, b).clone();
    assert_eq!(
        start_of(&pb, "Set new pump"),
        Some(minutes(&pb, "2027-01-18 08:00"))
    );
}
