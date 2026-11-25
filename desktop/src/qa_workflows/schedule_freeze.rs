//! Workflow tests for schedule freeze fixes:
//! 1. Dismissing a schedule error with Enter never re-triggers Schedule in the
//!    same frame.
//! 2. Opening the Schedule Options float-path ending activity dropdown on a
//!    large synthetic project keeps popup height bounded and lays out only
//!    visible rows.
//! 3. End-to-end F9 -> Schedule through the background worker and progress
//!    window commits dates, writes structural-only diagnostic log lines in the
//!    temp workspace dir, and Undo restores everything.
//! 4. The modal progress dialog passes `dialog_sweep::Probe` with zero
//!    findings and cancels cleanly on Escape without mutating `App.project`.
use super::*;

fn synthetic_project(extra_activities: i64, with_links: bool) -> ProjectData {
    let mut project = openxer_xer::starter();
    let template = project.model.activities[0].clone();
    let base_len = project.model.activities.len();
    let next_id = project
        .model
        .activities
        .iter()
        .map(|a| a.task_id)
        .max()
        .unwrap_or(1000)
        + 10;
    for k in 0..extra_activities {
        let mut a = template.clone();
        a.task_id = next_id + k;
        a.task_code = format!("SYN{k:04}");
        a.preds.clear();
        a.succs.clear();
        project.model.activities.push(a);
    }
    if with_links && extra_activities > 1 {
        for k in 0..(extra_activities as usize - 1) {
            let pred = base_len + k;
            let succ = base_len + k + 1;
            project.model.links.push(openxer_engine::model::Link {
                pred,
                succ,
                link_type: LinkType::FS,
                lag: 0,
                driving_path: false,
            });
            if k + 3 < extra_activities as usize {
                project.model.links.push(openxer_engine::model::Link {
                    pred,
                    succ: base_len + k + 3,
                    link_type: LinkType::FS,
                    lag: 0,
                    driving_path: false,
                });
            }
        }
        project.model.rebuild_links().unwrap();
    }
    project
}

#[test]
fn workflow_enter_dismissing_schedule_error_does_not_retrigger_schedule() {
    let mut d = fresh("wf-schedule-enter-bleed");
    let runs_before = d.app.schedule_runs;
    d.key(Key::F9);
    assert!(d.app.sched.schedule_open, "F9 opens Schedule");
    // Choose "Apply selected data date" and enter an invalid date so
    // pressing Schedule fails with an error message box above Schedule.
    d.click("schedule_apply_date");
    type_into(&mut d, "schedule_data_date", "not-a-valid-date");
    d.click("schedule_run");
    d.frames(2);
    assert!(
        d.app.error.is_some(),
        "invalid data date shows an error box"
    );
    assert!(
        d.app.sched.schedule_open,
        "Schedule dialog stays open behind the error"
    );
    assert_eq!(d.app.schedule_runs, runs_before);

    // Pressing Enter dismisses the error box and must NOT bleed through to
    // Schedule in the same frame (which would immediately re-open the error).
    d.key(Key::Enter);
    d.frames(2);
    assert!(
        d.app.error.is_none(),
        "Enter dismisses the error box without re-triggering Schedule: {:?}",
        d.app.error
    );
    assert!(
        d.app.sched.schedule_open,
        "Schedule dialog remains open after dismissing the error"
    );
    assert_eq!(d.app.schedule_runs, runs_before);
}

#[test]
fn workflow_mlp_end_dropdown_bounds_height_for_many_activities() {
    let project = synthetic_project(2_000, false);
    let mut d = Driver::new("wf-mlp-dropdown-height", project);
    d.key(Key::F9);
    assert!(d.app.sched.schedule_open);
    d.click("schedule_options");
    assert!(d.app.show_schedule);
    d.click("options_tab_1");
    assert!(d.app.sched.options_advanced);
    if !d
        .app
        .draft_options
        .as_ref()
        .is_some_and(|o| o.multiple_float_paths)
    {
        d.click("options_mlp");
    }
    d.click("mlp_end");
    d.frames(2);

    // Count how many dropdown item widgets were laid out this frame and
    // measure the total vertical span of the popup list.
    let item_rects: Vec<Rect> = d
        .app
        .action_rects
        .iter()
        .filter(|(k, _)| k.starts_with("mlp_end_item_"))
        .map(|(_, r)| Rect::from_min_max(Pos2::new(r[0], r[1]), Pos2::new(r[2], r[3])))
        .collect();
    assert!(
        !item_rects.is_empty(),
        "opening mlp_end must render visible items"
    );
    assert!(
        item_rects.len() <= 30,
        "mlp_end dropdown should virtualize rows, got {} items laid out",
        item_rects.len()
    );
    let min_top = item_rects
        .iter()
        .map(|r| r.top())
        .fold(f32::INFINITY, f32::min);
    let max_bottom = item_rects
        .iter()
        .map(|r| r.bottom())
        .fold(f32::NEG_INFINITY, f32::max);
    assert!(
        max_bottom - min_top <= 280.,
        "mlp_end dropdown height should be bounded, got {:.1} px",
        max_bottom - min_top
    );
}

#[test]
fn workflow_f9_schedule_through_progress_window_logs_and_undoes() {
    let mut project = openxer_xer::starter();
    project.model.project.name = "SECRET_CLIENT_PROJ".into();
    schedule_model(&mut project.model, &project.resources).unwrap();
    let mut d = Driver::new("wf-schedule-progress-log", project);

    let before_data_date = d.app.project.model.project.data_date;
    let before_starts: Vec<Option<i64>> = d
        .app
        .project
        .model
        .activities
        .iter()
        .map(|a| a.early_start)
        .collect();
    let origin = d.app.project.model.origin_serial;
    let moved_date = before_data_date.unwrap() + 7 * DAY;
    let moved_text = crate::schedule_dialogs::display_date(origin, Some(moved_date));
    let runs_before = d.app.schedule_runs;

    d.key(Key::F9);
    assert!(d.app.sched.schedule_open);
    d.click("schedule_apply_date");
    type_into(&mut d, "schedule_data_date", &moved_text);
    d.click("schedule_run");

    assert!(
        d.app.error.is_none(),
        "schedule succeeded: {:?}",
        d.app.error
    );
    assert!(!d.app.busy_active(), "background worker completed");
    assert!(!d.app.sched.schedule_open, "Schedule dialog closed");
    assert_eq!(d.app.schedule_runs, runs_before + 1);
    assert_eq!(d.app.project.model.project.data_date, Some(moved_date));
    let after_starts: Vec<Option<i64>> = d
        .app
        .project
        .model
        .activities
        .iter()
        .map(|a| a.early_start)
        .collect();
    assert_ne!(after_starts, before_starts, "early dates moved forward");

    // Verify the diagnostic log exists in the temp workspace dir, records all
    // major phases, and never contains client project or activity names.
    let log_path = d.dir.join("openxer-diag.log");
    let log_text = std::fs::read_to_string(&log_path)
        .unwrap_or_else(|e| panic!("expected diag log at {}: {e}", log_path.display()));
    for expected in [
        "schedule state=start",
        "phase=intercept_loops",
        "phase=refresh_derived_durations",
        "phase=validate",
        "phase=schedule_model_report",
        "phase=write_assignment_schedule",
        "phase=schedule_log",
        "phase=save",
        "schedule state=done",
    ] {
        assert!(
            log_text.contains(expected),
            "diag log missing {expected:?}:\n{log_text}"
        );
    }
    assert!(
        !log_text.contains("SECRET_CLIENT_PROJ"),
        "diag log must never record project names:\n{log_text}"
    );
    let first_code = d.app.project.model.activities[0].task_code.clone();
    assert!(
        !log_text.contains(&first_code),
        "diag log must never record activity IDs:\n{log_text}"
    );

    // Undo restores the pre-schedule data date and early dates in one step.
    d.click("menu_bar_edit");
    d.click("undo");
    d.frames(2);
    assert_eq!(d.app.project.model.project.data_date, before_data_date);
    let undone_starts: Vec<Option<i64>> = d
        .app
        .project
        .model
        .activities
        .iter()
        .map(|a| a.early_start)
        .collect();
    assert_eq!(undone_starts, before_starts, "Undo restores all dates");
}

#[test]
fn workflow_progress_dialog_passes_dialog_sweep_and_cancels_cleanly() {
    let mut project = synthetic_project(3_000, true);
    project.model.options.multiple_float_paths = true;
    project.model.options.max_float_paths = 50;
    let mut d = Driver::probed(
        "wf-schedule-progress-cancel",
        project,
        crate::dialog_sweep::SMALL_SCREEN,
        None,
    );
    let before_model = d.app.project.model.clone();
    let runs_before = d.app.schedule_runs;
    let origin = d.app.project.model.origin_serial;
    let moved = d.app.project.model.project.data_date.unwrap() + 14 * DAY;
    let moved_text = crate::schedule_dialogs::display_date(origin, Some(moved));

    d.key(Key::F9);
    assert!(d.app.sched.schedule_open);
    d.click("schedule_apply_date");
    type_into(&mut d, "schedule_data_date", &moved_text);
    d.frames(2);

    // Trigger Schedule via raw pointer release so we observe the live modal
    // progress dialog on the very frame it opens, with dialog_sweep::Probe
    // checking its layout and button size/order.
    let pos = d.rect("schedule_run").center();
    let press = |pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    let _ = d.frame(vec![egui::Event::PointerMoved(pos), press(true)]);
    let _ = d.frame(vec![press(false)]);

    assert!(d.app.busy_active(), "background worker is running");
    assert!(
        d.app.modal_open(&d.ctx),
        "modal_open blocks main-window input while busy"
    );
    assert!(
        d.has("busy_cancel"),
        "progress dialog renders a Cancel button"
    );
    let progress_findings: Vec<String> = d
        .probe
        .findings
        .iter()
        .filter(|f| f.dialog.contains("busy_progress") || f.dialog.contains("Scheduling"))
        .map(|f| format!("{}: {} ({})", f.dialog, f.issue, f.measure))
        .collect();
    assert!(
        progress_findings.is_empty(),
        "progress dialog must pass dialog_sweep with zero findings: {progress_findings:?}"
    );

    // Cancel immediately via Escape while running: project stays untouched.
    let _ = d.frame(vec![egui::Event::Key {
        key: Key::Escape,
        physical_key: Some(Key::Escape),
        pressed: true,
        repeat: false,
        modifiers: Default::default(),
    }]);
    let _ = d.frame(vec![egui::Event::Key {
        key: Key::Escape,
        physical_key: Some(Key::Escape),
        pressed: false,
        repeat: false,
        modifiers: Default::default(),
    }]);
    assert!(!d.app.busy_active(), "Escape cancels the running worker");
    assert_eq!(
        d.app.schedule_runs, runs_before,
        "cancelled run did not commit"
    );
    assert_eq!(
        d.app.project.model.project.data_date, before_model.project.data_date,
        "cancelled run left project data date untouched"
    );
    assert_eq!(
        d.app.project.model.activities[0].early_start, before_model.activities[0].early_start,
        "cancelled run left activity dates untouched"
    );
    let log_text = std::fs::read_to_string(d.dir.join("openxer-diag.log")).unwrap_or_default();
    assert!(
        log_text.contains("schedule state=cancelled"),
        "diag log recorded cancellation:\n{log_text}"
    );
}
