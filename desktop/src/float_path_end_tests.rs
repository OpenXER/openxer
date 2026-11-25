//! Multiple float paths ending with an activity that leaves the project:
//! deleting it, and Schedule Options opened on an ID the project no longer
//! holds. Driven through the real dialogs and commands, headless.
use super::*;
use crate::qa_sweep::Driver;
use crate::qa_workflows::schedule_options_close::close_schedule_options;

fn driver(name: &str) -> Driver {
    Driver::new(name, crate::reports::test_support::scheduled_starter())
}

fn index_of(d: &Driver, task_id: i64) -> Option<usize> {
    d.app
        .project
        .model
        .activities
        .iter()
        .position(|a| a.task_id == task_id)
}

/// F9, Options..., the Advanced tab: the Schedule dialog stays open under it.
fn open_advanced(d: &mut Driver) {
    d.key(Key::F9);
    assert!(d.app.sched.schedule_open);
    d.click("schedule_options");
    assert!(d.app.show_schedule);
    d.click("options_tab_1");
    assert!(d.app.sched.options_advanced);
}

/// Turn multiple float paths on ending with `task_id` through Schedule
/// Options, Close and Yes, then Schedule.
fn end_float_paths_with(d: &mut Driver, task_id: i64) {
    let i = index_of(d, task_id).unwrap();
    open_advanced(d);
    d.click("options_mlp");
    d.click("mlp_end");
    d.click(&format!("mlp_end_item_{}", i + 1));
    close_schedule_options(d, Some("Yes"));
    assert!(!d.app.show_schedule);
    let o = &d.app.project.model.options;
    assert!(o.multiple_float_paths);
    assert_eq!(o.float_paths_end_activity, Some(task_id));
    let runs = d.app.schedule_runs;
    d.click("schedule_run");
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
    assert_eq!(d.app.schedule_runs, runs + 1);
}

#[test]
fn deleting_the_float_path_ending_activity_keeps_scheduling_and_undo_restores_both() {
    let mut d = driver("float-path-end-delete");
    let end = 1050;
    end_float_paths_with(&mut d, end);
    // Edit > Delete through the Delete key and its confirmation.
    let i = index_of(&d, end).unwrap();
    d.select(i);
    d.key(Key::Delete);
    assert_eq!(d.app.delete_confirm, Some(vec![end]));
    d.click("delete_yes");
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
    assert!(index_of(&d, end).is_none(), "the activity is deleted");
    let o = &d.app.project.model.options;
    assert!(o.multiple_float_paths);
    assert_eq!(
        o.float_paths_end_activity, None,
        "the ending activity is blank"
    );
    // F9 schedules (it panics on an error).
    d.f9();
    // Undo back to before the delete: the activity and the option together.
    let mut steps = 0;
    while index_of(&d, end).is_none() {
        assert!(d.app.history.counts().0 > 0, "nothing left to undo");
        d.click("undo");
        steps += 1;
        assert!(steps <= 5, "the delete is undone within a few steps");
    }
    assert_eq!(
        d.app.project.model.options.float_paths_end_activity,
        Some(end),
        "undo restores the option with the activity"
    );
    d.f9();
}

/// An ID that matches no activity is blank in the dialog: Close (nothing
/// to ask, as the dialog showed it blank) leaves a valid option whether or
/// not the Advanced tab was drawn.
#[test]
fn schedule_options_close_repairs_an_ending_activity_the_project_does_not_hold() {
    for advanced in [true, false] {
        let mut d = driver("float-path-end-stale");
        let o = &mut d.app.project.model.options;
        o.multiple_float_paths = true;
        o.float_paths_end_activity = Some(987_654);
        o.max_float_paths = 0;
        if advanced {
            open_advanced(&mut d);
        } else {
            d.key(Key::F9);
            d.click("schedule_options");
            assert!(!d.app.sched.options_advanced);
        }
        close_schedule_options(&mut d, None);
        assert!(!d.app.show_schedule);
        let o = &d.app.project.model.options;
        assert_eq!(o.float_paths_end_activity, None, "advanced {advanced}");
        assert!(o.max_float_paths >= 1, "advanced {advanced}");
        assert!(o.multiple_float_paths);
        d.app.project.model.validate().unwrap();
        let runs = d.app.schedule_runs;
        d.click("schedule_run");
        assert!(d.app.error.is_none(), "{:?}", d.app.error);
        assert_eq!(d.app.schedule_runs, runs + 1);
    }
}

/// Close with nothing touched asks nothing and leaves the options exactly as they were, on
/// both tabs (a project without stored options keeps its defaults).
#[test]
fn schedule_options_close_without_a_change_leaves_the_options_alone() {
    for advanced in [false, true] {
        let mut d = driver("float-path-options-unchanged");
        d.app.project.model.options.out_of_sequence = OutOfSequence::RetainedLogic;
        let before = d.app.project.model.options.clone();
        if advanced {
            open_advanced(&mut d);
            d.click("options_tab_0");
        } else {
            d.key(Key::F9);
            d.click("schedule_options");
        }
        close_schedule_options(&mut d, None);
        assert!(!d.app.show_schedule);
        assert_eq!(d.app.project.model.options, before, "advanced {advanced}");
        // The checkbox still sets the choice when it is clicked.
        d.click("schedule_options");
        d.click("options_ignore_external");
        close_schedule_options(&mut d, Some("Yes"));
        assert_eq!(
            d.app.project.model.options.out_of_sequence,
            OutOfSequence::None
        );
        d.click("schedule_options");
        d.click("options_ignore_external");
        close_schedule_options(&mut d, Some("Yes"));
        assert_eq!(
            d.app.project.model.options.out_of_sequence,
            OutOfSequence::Both
        );
    }
}
