use super::*;
#[test]
fn schedule_uses_applied_saved_options_and_ignores_unapplied_drafts() {
    let path =
        std::env::temp_dir().join(format!("openxer-saved-options-{}.db", std::process::id()));
    {
        let store = Store::open(path.clone()).unwrap();
        let project = openxer_xer::starter();
        let context = egui::Context::default();
        let mut app = App::new(&context, store, project);
        app.open_options();
        let use_expected = !app.project.model.options.use_expected_finish;
        app.draft_options.as_mut().unwrap().use_expected_finish = use_expected;
        app.apply_options();
        assert!(!app.show_schedule);
        assert_eq!(app.schedule_runs, 0);
        let saved = app.store.last().unwrap().unwrap();
        assert_eq!(saved.model.options.use_expected_finish, use_expected);

        // The Schedule dialog applies a selected data date, then runs.
        let origin = app.project.model.origin_serial;
        let data_date = app.project.model.project.data_date.unwrap() + DAY;
        app.open_schedule_dialog();
        app.sched.own_data_date = false;
        app.sched.data_date_text = "not a date".into();
        app.run_schedule_dialog();
        assert!(app.error.take().is_some());
        assert!(app.sched.schedule_open, "A bad date keeps the dialog open");
        assert_eq!(app.schedule_runs, 0);
        assert_ne!(app.project.model.project.data_date, Some(data_date));
        app.sched.data_date_text = schedule_dialogs::display_date(origin, Some(data_date));
        app.run_schedule_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        assert_eq!(app.schedule_runs, 1);
        assert!(!app.sched.schedule_open);
        assert_eq!(app.project.model.project.data_date, Some(data_date));
        assert!(app.log[0].starts_with("Scheduling/Leveling Report - "));

        // Unapplied option drafts never reach a schedule.
        app.open_options();
        app.draft_options.as_mut().unwrap().use_expected_finish = !use_expected;
        app.schedule();
        assert!(app.error.is_none(), "{:?}", app.error);
        assert_eq!(app.schedule_runs, 2);
        assert!(!app.show_schedule);
        assert_eq!(app.project.model.options.use_expected_finish, use_expected);
        assert_eq!(
            app.store
                .last()
                .unwrap()
                .unwrap()
                .model
                .options
                .use_expected_finish,
            use_expected
        );
        let saved = app.store.last().unwrap().unwrap();
        // Reloading chooses its own date origin; compare calendar dates.
        assert_eq!(
            date_string(saved.model.origin_serial, saved.model.project.data_date),
            date_string(origin, Some(data_date))
        );
    }
    let _ = std::fs::remove_file(path);
}
#[test]
fn failed_save_cannot_replace_the_current_project() {
    let path = std::env::temp_dir().join(format!("openxer-save-failure-{}.db", std::process::id()));
    {
        let mut store = Store::open(path.clone()).unwrap();
        let project = openxer_xer::starter();
        store.save(&project).unwrap();
        let context = egui::Context::default();
        let mut app = App::new(&context, store, project);
        app.project.details.get_mut(&1010).unwrap().name = "Unsaved current edit".into();
        app.dirty = true;
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection.execute_batch("CREATE TRIGGER reject_test_update BEFORE UPDATE ON projects BEGIN SELECT RAISE(ABORT, 'forced write failure'); END;").unwrap();
        let mut replacement = openxer_xer::starter();
        replacement.model.project.name = "Replacement".into();
        app.load(replacement);
        assert_eq!(app.project.model.project.name, "RIVERBEND");
        assert_eq!(app.project.details[&1010].name, "Unsaved current edit");
        assert!(app.dirty);
        assert!(app.error.unwrap().contains("forced write failure"));
    }
    let _ = std::fs::remove_file(path);
}
#[test]
fn resource_dependent_activities_schedule_without_leveling() {
    let mut project = openxer_xer::starter();
    project.model.activities[1].task_type = TaskType::Resource;
    assert!(!project.model.options.do_level);
    let mut plain = project.model.clone();
    schedule_model(&mut plain, &project.resources).unwrap();
    assert!(plain
        .activities
        .iter()
        .all(|a| a.early_start.is_some() && a.early_finish.is_some()));
    // With no resource calendar to work on, the activity keeps its own
    // calendar and dates as a Task Dependent activity.
    let mut task_dependent = project.model.clone();
    task_dependent.activities[1].task_type = TaskType::Task;
    schedule_model(&mut task_dependent, &project.resources).unwrap();
    let dates = |m: &Model| {
        m.activities
            .iter()
            .map(|a| {
                (
                    a.restart,
                    a.reend,
                    a.early_start,
                    a.early_finish,
                    a.late_start,
                    a.late_finish,
                    a.total_float,
                    a.free_float,
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(dates(&plain), dates(&task_dependent));
}
