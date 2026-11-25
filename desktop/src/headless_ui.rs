use super::*;

pub(crate) fn frame(
    app: &mut App,
    ctx: &egui::Context,
    events: Vec<egui::Event>,
    time: &mut f64,
) -> egui::FullOutput {
    frame_with_modifiers(app, ctx, events, time, Default::default())
}

fn frame_with_modifiers(
    app: &mut App,
    ctx: &egui::Context,
    events: Vec<egui::Event>,
    time: &mut f64,
    modifiers: egui::Modifiers,
) -> egui::FullOutput {
    *time += 0.1;
    ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1500., 900.))),
            time: Some(*time),
            events,
            modifiers,
            focused: true,
            ..Default::default()
        },
        |ctx| app.update_ui(ctx),
    )
}

#[track_caller]
pub(crate) fn click(app: &mut App, ctx: &egui::Context, action: &str, time: &mut f64) {
    let at = std::panic::Location::caller();
    if !app.action_rects.contains_key(action) {
        if let Some(menu) = menu::menu_holding(action) {
            // Commands without a toolbar button live in a main menu.
            click_text(app, ctx, menu, time);
        }
    }
    let [left, top, right, bottom] = *app
        .action_rects
        .get(action)
        .unwrap_or_else(|| panic!("No rendered control {action:?} (asked at {at})"));
    let pos = Pos2::new((left + right) / 2., (top + bottom) / 2.);
    for pressed in [true, false] {
        let _ = frame(
            app,
            ctx,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                },
            ],
            time,
        );
    }
    let _ = frame(app, ctx, vec![], time);
    wait_busy(app, ctx, time);
}

pub(crate) fn wait_busy(app: &mut App, ctx: &egui::Context, time: &mut f64) {
    let deadline = Instant::now() + std::time::Duration::from_secs(60);
    while app.busy_active() {
        assert!(
            Instant::now() < deadline,
            "background scheduling/leveling worker timed out"
        );
        std::thread::sleep(std::time::Duration::from_millis(2));
        let _ = frame(app, ctx, vec![], time);
    }
}

fn pointer(
    app: &mut App,
    ctx: &egui::Context,
    pos: Pos2,
    button: egui::PointerButton,
    pressed: bool,
    time: &mut f64,
) {
    let _ = frame(
        app,
        ctx,
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button,
                pressed,
                modifiers: Default::default(),
            },
        ],
        time,
    );
}

pub(crate) fn key(app: &mut App, ctx: &egui::Context, key: Key, time: &mut f64) {
    for pressed in [true, false] {
        let _ = frame(
            app,
            ctx,
            vec![egui::Event::Key {
                key,
                physical_key: Some(key),
                pressed,
                repeat: false,
                modifiers: Default::default(),
            }],
            time,
        );
    }
    wait_busy(app, ctx, time);
}

/// Where a relationship drag starts or ends at a bar end: the ring of the
/// end's relationship zone (the bar end itself stretches a not-started bar).
fn endpoint(app: &App, task_id: i64, edge: Edge) -> Pos2 {
    app.bar_geometry
        .iter()
        .find(|bar| bar.task_id == task_id)
        .unwrap()
        .link_grip(edge)
}

fn drag(app: &mut App, ctx: &egui::Context, start: Pos2, end: Pos2, cancel: bool, time: &mut f64) {
    pointer(app, ctx, start, egui::PointerButton::Primary, true, time);
    for fraction in [0.25, 0.5, 0.75, 1.] {
        let output = frame(
            app,
            ctx,
            vec![egui::Event::PointerMoved(start.lerp(end, fraction))],
            time,
        );
        assert!(!output.shapes.is_empty());
    }
    assert!(
        app.link_drag.is_some(),
        "The production endpoint widget must start a live drag"
    );
    if cancel {
        key(app, ctx, Key::Escape, time);
    }
    pointer(app, ctx, end, egui::PointerButton::Primary, false, time);
    assert!(app.link_drag.is_none());
    let _ = frame(app, ctx, vec![], time);
}

fn context_link(app: &mut App, ctx: &egui::Context, index: usize, time: &mut f64) {
    let points = &app
        .link_geometry
        .iter()
        .find(|(i, _)| *i == index)
        .unwrap()
        .1;
    let pos = points
        .windows(2)
        .flat_map(|pair| [0.25, 0.5, 0.75].map(|t| pair[0].lerp(pair[1], t)))
        .max_by(|a, b| {
            let clearance = |pos| {
                app.link_geometry
                    .iter()
                    .filter(|(i, _)| *i != index)
                    .flat_map(|(_, points)| points.windows(2))
                    .map(|pair| segment_distance(pos, pair[0], pair[1]))
                    .fold(f32::INFINITY, f32::min)
            };
            clearance(*a).total_cmp(&clearance(*b))
        })
        .unwrap();
    pointer(app, ctx, pos, egui::PointerButton::Secondary, true, time);
    pointer(app, ctx, pos, egui::PointerButton::Secondary, false, time);
    let _ = frame(app, ctx, vec![], time);
    // The chart menu opens first; closing it opens Edit Relationship.
    assert!(
        app.gantt_ui.menu.is_some(),
        "the chart menu opens on a line"
    );
    key(app, ctx, Key::Escape, time);
    let _ = frame(app, ctx, vec![], time);
    assert_eq!(
        app.link_editor.as_ref().map(|editor| editor.index),
        Some(index)
    );
}

#[test]
fn endpoint_drags_validation_context_edit_and_undo_use_real_ui_without_windows() {
    let path =
        std::env::temp_dir().join(format!("openxer-headless-drag-{}.db", std::process::id()));
    {
        let store = Store::open(path.clone()).unwrap();
        let mut project = openxer_xer::starter();
        schedule_model(&mut project.model, &project.resources).unwrap();
        let ctx = egui::Context::default();
        let mut app = App::new(&ctx, store, project);
        let mut time = 0.;
        let _ = frame(&mut app, &ctx, vec![], &mut time);
        let _ = frame(&mut app, &ctx, vec![], &mut time);
        let index = |app: &App, id| {
            app.project
                .model
                .activities
                .iter()
                .position(|a| a.task_id == id)
                .unwrap()
        };
        let pred = index(&app, 1010);
        let succ = index(&app, 1040);
        let original_count = app.project.model.links.len();
        for (count, (from, to, kind)) in [
            (Edge::Finish, Edge::Start, LinkType::FS),
            (Edge::Finish, Edge::Finish, LinkType::FF),
            (Edge::Start, Edge::Start, LinkType::SS),
            (Edge::Start, Edge::Finish, LinkType::SF),
        ]
        .into_iter()
        .enumerate()
        {
            let start = endpoint(&app, 1010, from);
            let finish = endpoint(&app, 1040, to);
            drag(&mut app, &ctx, start, finish, false, &mut time);
            assert_eq!(app.project.model.links.len(), original_count + count + 1);
            assert!(app
                .project
                .model
                .links
                .iter()
                .any(|link| link.pred == pred && link.succ == succ && link.link_type == kind));
        }
        let created = app.project.model.links.clone();
        for (from_id, from_edge, to_id, to_edge, cancel, message) in [
            (
                1010,
                Edge::Finish,
                1040,
                Edge::Start,
                false,
                "already exists",
            ),
            (
                1040,
                Edge::Finish,
                1010,
                Edge::Start,
                false,
                "circular path",
            ),
            (
                1010,
                Edge::Start,
                1010,
                Edge::Finish,
                false,
                "own predecessor",
            ),
            (1010, Edge::Finish, 1050, Edge::Start, true, "cancelled"),
        ] {
            let from = endpoint(&app, from_id, from_edge);
            let to = endpoint(&app, to_id, to_edge);
            drag(&mut app, &ctx, from, to, cancel, &mut time);
            assert_eq!(app.project.model.links, created);
            assert!(app.status.contains(message), "{}", app.status);
            assert!(
                app.error.is_none(),
                "Gesture validation must not open a blocking modal"
            );
        }
        click(&mut app, &ctx, "undo", &mut time);
        assert_eq!(app.project.model.links.len(), created.len() - 1);
        click(&mut app, &ctx, "redo", &mut time);
        assert_eq!(app.project.model.links, created);

        let context_pred = index(&app, 1020);
        let context_succ = index(&app, 1030);
        let link_index = app
            .project
            .model
            .links
            .iter()
            .position(|link| link.pred == context_pred && link.succ == context_succ)
            .unwrap();
        context_link(&mut app, &ctx, link_index, &mut time);
        // The dialog names both ends.
        assert_eq!(app.link_editor.as_ref().unwrap().lag_text, "0d");
        click(&mut app, &ctx, "link_type", &mut time);
        // Items: FS, SS, FF, SF.
        click(&mut app, &ctx, "link_type_item_2", &mut time);
        click(&mut app, &ctx, "link_lag", &mut time);
        key(&mut app, &ctx, Key::End, &mut time);
        for _ in 0..3 {
            key(&mut app, &ctx, Key::Backspace, &mut time);
        }
        // An explicit unit keeps the typed lag in hours whatever the duration format.
        let _ = frame(
            &mut app,
            &ctx,
            vec![egui::Event::Text("6h".into())],
            &mut time,
        );
        let undo_before = app.history.counts().0;
        // Enter is OK.
        key(&mut app, &ctx, Key::Enter, &mut time);
        assert!(app.link_editor.is_none());
        assert_eq!(app.history.counts().0, undo_before + 1, "one undo step");
        assert_eq!(app.project.model.links[link_index].link_type, LinkType::FF);
        assert_eq!(app.project.model.links[link_index].lag, 360);
        click(&mut app, &ctx, "save", &mut time);
        let changed = app.project.model.links.clone();
        context_link(&mut app, &ctx, link_index, &mut time);
        click(&mut app, &ctx, "link_delete", &mut time);
        let _ = frame(&mut app, &ctx, vec![], &mut time);
        click(&mut app, &ctx, "delete_relationship_yes", &mut time);
        click(&mut app, &ctx, "save", &mut time);
        assert_eq!(app.project.model.links.len(), changed.len() - 1);
        click(&mut app, &ctx, "undo", &mut time);
        assert_eq!(app.project.model.links, changed);
        click(&mut app, &ctx, "redo", &mut time);
        assert_eq!(app.project.model.links.len(), changed.len() - 1);
        click(&mut app, &ctx, "undo", &mut time);
        click(&mut app, &ctx, "save", &mut time);
        let reopened = app.store.last().unwrap().unwrap();
        assert_eq!(reopened.model.links.len(), changed.len());
        assert!(reopened
            .model
            .links
            .iter()
            .any(|link| reopened.model.activities[link.pred].task_id == 1020
                && reopened.model.activities[link.succ].task_id == 1030
                && link.link_type == LinkType::FF
                && link.lag == 360));
    }
    let _ = std::fs::remove_file(path);
}

#[test]
fn layout_f9_and_options_work_without_a_native_window() {
    use crate::qa_workflows::schedule_options_close::{
        answer_options_save, close_schedule_options, Frames,
    };
    let path = std::env::temp_dir().join(format!("openxer-headless-ui-{}.db", std::process::id()));
    {
        let store = Store::open(path.clone()).unwrap();
        let mut project = openxer_xer::starter();
        schedule_model(&mut project.model, &project.resources).unwrap();
        let ctx = egui::Context::default();
        let mut app = App::new(&ctx, store, project);
        let mut time = 0.;
        let _ = frame(&mut app, &ctx, vec![], &mut time);
        let output = frame(&mut app, &ctx, vec![], &mut time);
        assert!(
            output.shapes.len() > 100,
            "Actual UI must produce layout/paint shapes"
        );
        assert_eq!(app.bar_geometry.len(), 14);
        assert!(app
            .bar_geometry
            .iter()
            .all(|bar| (bar.row.height() - gantt_geometry::ROW_HEIGHT).abs() < 0.01));
        assert_eq!(ctx.theme(), egui::Theme::Light);
        assert_eq!(ctx.style().visuals.selection.bg_fill, SELECT);
        assert!((ctx.style().text_styles[&egui::TextStyle::Body].size - FONT_SIZE).abs() < 0.01);

        // F9 opens the Schedule dialog; nothing runs until Schedule.
        key(&mut app, &ctx, Key::F9, &mut time);
        assert!(app.sched.schedule_open);
        assert_eq!(app.schedule_runs, 0);
        // Options... stacks Schedule Options; Cancel after a change asks
        // to save it, and No discards the draft.
        let before = app.project.model.options.use_expected_finish;
        click(&mut app, &ctx, "schedule_options", &mut time);
        assert!(app.show_schedule);
        click(&mut app, &ctx, "options_expected_finish", &mut time);
        assert_eq!(
            app.draft_options.as_ref().unwrap().use_expected_finish,
            !before
        );
        click(&mut app, &ctx, "options_cancel", &mut time);
        answer_options_save(&mut Frames(&mut app, &ctx, &mut time), Some("No"));
        assert!(!app.show_schedule);
        assert!(
            app.sched.schedule_open,
            "Cancel closes only Schedule Options"
        );
        assert_eq!(app.project.model.options.use_expected_finish, before);
        // Esc does nothing on Schedule Options, as in the reference; Close
        // closes it, then Esc closes the Schedule dialog.
        click(&mut app, &ctx, "schedule_options", &mut time);
        click(&mut app, &ctx, "options_tab_1", &mut time);
        assert!(app.sched.options_advanced);
        click(&mut app, &ctx, "options_tab_0", &mut time);
        key(&mut app, &ctx, Key::Escape, &mut time);
        assert!(app.show_schedule && app.sched.schedule_open);
        close_schedule_options(&mut Frames(&mut app, &ctx, &mut time), None);
        assert!(!app.show_schedule && app.sched.schedule_open);
        key(&mut app, &ctx, Key::Escape, &mut time);
        assert!(!app.sched.schedule_open);
        assert_eq!(app.schedule_runs, 0);
        // Cancel button, then Schedule button.
        key(&mut app, &ctx, Key::F9, &mut time);
        click(&mut app, &ctx, "schedule_cancel", &mut time);
        assert!(!app.sched.schedule_open);
        click(&mut app, &ctx, "schedule", &mut time);
        assert!(app.sched.schedule_open, "The toolbar opens the dialog");
        click(&mut app, &ctx, "schedule_run", &mut time);
        assert_eq!(app.schedule_runs, 1);
        assert!(!app.sched.schedule_open);
        assert!(app.error.is_none());
        // Enter presses the default Schedule button.
        key(&mut app, &ctx, Key::F9, &mut time);
        key(&mut app, &ctx, Key::Enter, &mut time);
        assert_eq!(app.schedule_runs, 2);
        assert!(!app.sched.schedule_open);
        // View Log shows the last run's log.
        key(&mut app, &ctx, Key::F9, &mut time);
        click(&mut app, &ctx, "schedule_view_log", &mut time);
        assert!(app.show_log);
        assert!(app.log[0].starts_with("Scheduling/Leveling Report"));
        key(&mut app, &ctx, Key::Escape, &mut time);
        app.show_log = false;
        // The accelerators wait a frame for the window that was up.
        frame(&mut app, &ctx, vec![], &mut time);
        // Options... from the Schedule dialog; Close and Yes save without
        // scheduling.
        key(&mut app, &ctx, Key::F9, &mut time);
        click(&mut app, &ctx, "schedule_options", &mut time);
        assert!(app.show_schedule);
        click(&mut app, &ctx, "options_expected_finish", &mut time);
        click(&mut app, &ctx, "options_critical_longest", &mut time);
        close_schedule_options(&mut Frames(&mut app, &ctx, &mut time), Some("Yes"));
        assert!(!app.show_schedule);
        key(&mut app, &ctx, Key::Escape, &mut time);
        assert!(!app.sched.schedule_open);
        assert_eq!(app.schedule_runs, 2, "Close must not schedule");
        assert_eq!(app.project.model.options.use_expected_finish, !before);
        assert!(app.critical_definition().0);
        let saved = app.store.last().unwrap().unwrap();
        assert_eq!(saved.model.options.use_expected_finish, !before);
        // Combo boxes open a list and select an item.
        key(&mut app, &ctx, Key::F9, &mut time);
        click(&mut app, &ctx, "schedule_options", &mut time);
        click(&mut app, &ctx, "float_type", &mut time);
        click(&mut app, &ctx, "float_type_item_0", &mut time);
        assert_eq!(
            app.draft_options.as_ref().unwrap().float_type,
            FloatType::Start
        );
        click(&mut app, &ctx, "lag_calendar", &mut time);
        click(&mut app, &ctx, "lag_calendar_item_3", &mut time);
        // The project default calendar is a supported lag calendar.
        assert_eq!(
            app.draft_options.as_ref().unwrap().lag_calendar,
            LagCalendar::ProjCal
        );
        // Cancel asks about the change; No drops it.
        click(&mut app, &ctx, "options_cancel", &mut time);
        answer_options_save(&mut Frames(&mut app, &ctx, &mut time), Some("No"));
        key(&mut app, &ctx, Key::Escape, &mut time);
        assert!(!app.show_schedule);
        assert_eq!(
            app.project.model.options.float_type,
            saved.model.options.float_type
        );
    }
    let _ = std::fs::remove_file(path);
}

#[test]
fn check_schedule_runs_lists_results_and_jumps_to_activities() {
    let (path, ctx, mut app, mut time) = test_view("check");
    let report =
        std::env::temp_dir().join(format!("openxer-check-report-{}.html", std::process::id()));
    app.open_check_schedule();
    app.sched.check_report_path = report.display().to_string();
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    assert!(app.action_rects.contains_key("check_run"));
    // Esc cancels.
    key(&mut app, &ctx, Key::Escape, &mut time);
    assert!(!app.sched.check_open);
    app.open_check_schedule();
    app.sched.check_report_path = report.display().to_string();
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    // Uncheck Logic, then run.
    click(&mut app, &ctx, "check_0", &mut time);
    assert!(!app.sched.check_draft.enabled[0]);
    click(&mut app, &ctx, "check_run", &mut time);
    assert!(!app.sched.check_open);
    let results = app.sched.check_results.clone().unwrap();
    assert_eq!(results.len(), 14);
    assert!(results
        .iter()
        .all(|r| r.check != schedule_check::Check::Logic));
    let html = std::fs::read_to_string(&report).unwrap();
    assert!(html.contains("Relationship Types"));
    // Select a check with offenders and jump to one.
    let index = results.iter().position(|r| !r.items.is_empty()).unwrap();
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, &format!("check_result_{index}"), &mut time);
    assert_eq!(app.sched.check_selected, index);
    click(&mut app, &ctx, "check_item_0", &mut time);
    let expected = match results[index].items[0] {
        schedule_check::Item::Activity(a) => a,
        schedule_check::Item::Relationship(l) => app.project.model.links[l].succ,
    };
    assert_eq!(app.selected, Some(expected));
    let _ = std::fs::remove_file(report);
    let _ = std::fs::remove_file(path);
}

#[test]
fn out_of_sequence_relationship_prompt_keeps_or_reverts_the_change() {
    let path = std::env::temp_dir().join(format!("openxer-headless-oos-{}.db", std::process::id()));
    {
        let store = Store::open(path.clone()).unwrap();
        let mut project = openxer_xer::starter();
        let index = |p: &ProjectData, id| {
            p.model
                .activities
                .iter()
                .position(|a| a.task_id == id)
                .unwrap()
        };
        let succ = index(&project, 1040);
        let data_date = project.model.project.data_date.unwrap();
        let a = &mut project.model.activities[succ];
        a.status = StatusType::InProgress;
        a.act_start = Some(data_date - 60);
        schedule_model(&mut project.model, &project.resources).unwrap();
        let ctx = egui::Context::default();
        let mut app = App::new(&ctx, store, project);
        app.editing.options.warn_out_of_sequence = true;
        let mut time = 0.;
        let _ = frame(&mut app, &ctx, vec![], &mut time);
        let _ = frame(&mut app, &ctx, vec![], &mut time);
        let count = app.project.model.links.len();
        let start = endpoint(&app, 1010, Edge::Finish);
        let finish = endpoint(&app, 1040, Edge::Start);
        drag(&mut app, &ctx, start, finish, false, &mut time);
        assert_eq!(app.project.model.links.len(), count + 1);
        let prompt = app.sched.sequence.clone().expect("out-of-sequence prompt");
        assert_eq!(prompt.activity, succ);
        let _ = frame(&mut app, &ctx, vec![], &mut time);
        click(&mut app, &ctx, "sequence_view", &mut time);
        assert!(app.sched.sequence_detail);
        key(&mut app, &ctx, Key::Escape, &mut time);
        assert!(!app.sched.sequence_detail && app.sched.sequence.is_some());
        click(&mut app, &ctx, "sequence_no", &mut time);
        assert!(app.sched.sequence.is_none());
        assert_eq!(app.project.model.links.len(), count, "No reverts the link");
        let start = endpoint(&app, 1010, Edge::Finish);
        let finish = endpoint(&app, 1040, Edge::Start);
        drag(&mut app, &ctx, start, finish, false, &mut time);
        let _ = frame(&mut app, &ctx, vec![], &mut time);
        click(&mut app, &ctx, "sequence_yes", &mut time);
        assert!(app.sched.sequence.is_none());
        assert_eq!(app.project.model.links.len(), count + 1, "Yes keeps it");
    }
    let _ = std::fs::remove_file(path);
}

/// The agents' Check Schedule (`openxer-cli check-schedule`, the chat's
/// `openxer_check_schedule`) gives the rows the dialog shows, on the sample
/// project opened without a project baseline.
#[test]
fn check_schedule_tool_rows_equal_the_dialog() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "openxer-check-tool-{}-{unique}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let project = openxer_xer::sample_project::sample_project().current;
    let xer = dir.join("sample.xer");
    project.updated_document().write(&xer).unwrap();
    let store = Store::open(dir.join("openxer.db")).unwrap();
    let ctx = egui::Context::default();
    let mut app = App::new(&ctx, store, project);
    let mut time = 0.;
    for _ in 0..3 {
        let _ = frame(&mut app, &ctx, vec![], &mut time);
    }
    app.sched.check_report_path = dir.join("report.html").display().to_string();
    app.open_check_schedule();
    // The dialog settles where it is drawn over a few frames.
    for _ in 0..3 {
        let _ = frame(&mut app, &ctx, vec![], &mut time);
    }
    assert!(app.action_rects.contains_key("check_run"));
    click(&mut app, &ctx, "check_run", &mut time);
    let dialog = app
        .sched
        .check_results
        .clone()
        .expect("the dialog ran its checks");
    let tool = openxer_automation::response(openxer_automation::dispatch(
        openxer_automation::parse_command(
            serde_json::json!({"command":"check_schedule","input":xer}),
        )
        .unwrap(),
    ));
    let rows = tool["result"]["rows"].as_array().unwrap();
    assert_eq!(rows.len(), dialog.len());
    for (row, d) in rows.iter().zip(&dialog) {
        assert_eq!(row["check"], d.check.name());
        assert_eq!(row["count"], d.items.len(), "{}", d.check.name());
        assert_eq!(row["of"], d.population, "{}", d.check.name());
        assert_eq!(
            row["pass"],
            serde_json::json!(d.passed),
            "{}",
            d.check.name()
        );
        assert_eq!(
            row["target"],
            serde_json::json!(d.target),
            "{}",
            d.check.name()
        );
        match d.value {
            Some(v) => assert!((row["result"].as_f64().unwrap() - v).abs() < 0.006),
            None => assert!(row["result"].is_null()),
        }
    }
    let _ = std::fs::remove_dir_all(dir);
}

pub(crate) fn test_view(name: &str) -> (PathBuf, egui::Context, App, f64) {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "openxer-headless-{name}-{}-{unique}.db",
        std::process::id()
    ));
    let store = Store::open(path.clone()).unwrap();
    let mut project = openxer_xer::starter();
    schedule_model(&mut project.model, &project.resources).unwrap();
    let ctx = egui::Context::default();
    let mut app = App::new(&ctx, store, project);
    let mut time = 0.;
    for _ in 0..3 {
        let _ = frame(&mut app, &ctx, vec![], &mut time);
    }
    (path, ctx, app, time)
}

fn action_rect(app: &App, action: &str) -> Rect {
    let [left, top, right, bottom] = app.action_rects[action];
    Rect::from_min_max(Pos2::new(left, top), Pos2::new(right, bottom))
}

fn axis(app: &App) -> timescale::TimeScale {
    timescale::TimeScale::new(
        app.timeline_origin as f64,
        app.pixels_per_day as f64 * 7.,
        app.gantt_left,
    )
}

fn drag_widget(app: &mut App, ctx: &egui::Context, start: Pos2, end: Pos2, time: &mut f64) {
    let on_header = ["timescale_pan", "timescale_zoom"]
        .into_iter()
        .any(|action| {
            app.action_rects.contains_key(action) && action_rect(app, action).contains(start)
        });
    let before_press = (app.timeline_origin, app.pixels_per_day);
    pointer(app, ctx, start, egui::PointerButton::Primary, true, time);
    if on_header {
        assert_eq!(
            (app.timeline_origin, app.pixels_per_day),
            before_press,
            "Moving to a header and pressing must not include movement before the press"
        );
    }
    for fraction in [0.25, 0.5, 0.75, 1.] {
        let _ = frame(
            app,
            ctx,
            vec![egui::Event::PointerMoved(start.lerp(end, fraction))],
            time,
        );
    }
    pointer(app, ctx, end, egui::PointerButton::Primary, false, time);
    let _ = frame(app, ctx, vec![], time);
    assert!(
        app.link_drag.is_none(),
        "Header controls must not drag a link"
    );
}

/// Dropdown options have no action IDs; locate their actual rendered text.
fn click_text(app: &mut App, ctx: &egui::Context, text: &str, time: &mut f64) {
    let output = frame(app, ctx, vec![], time);
    let pos = output
        .shapes
        .iter()
        .find_map(|clipped| {
            if let egui::Shape::Text(shape) = &clipped.shape {
                let rect = shape.visual_bounding_rect().intersect(clipped.clip_rect);
                if shape.galley.text() == text && rect.is_positive() {
                    return Some(rect.center());
                }
            }
            None
        })
        .unwrap_or_else(|| panic!("No visible UI text {text:?}"));
    pointer(app, ctx, pos, egui::PointerButton::Primary, true, time);
    pointer(app, ctx, pos, egui::PointerButton::Primary, false, time);
    let _ = frame(app, ctx, vec![], time);
}

#[test]
fn timescale_header_drag_toolbar_zoom_and_labels_use_the_rendered_controls() {
    let (path, ctx, mut app, mut time) = test_view("timescale");
    let output = frame(&mut app, &ctx, vec![], &mut time);
    let upper = action_rect(&app, "timescale_pan");
    let lower = action_rect(&app, "timescale_zoom");
    // Measured bands: 16 + 1 and 17 + 1 pixels.
    assert_eq!(upper.height(), 17.);
    assert_eq!(lower.height(), 18.);
    assert_eq!(upper.bottom(), lower.top());
    assert!(!upper.intersect(lower).is_positive());

    for band in [upper, lower] {
        let labels: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|clipped| {
                if let egui::Shape::Text(text) = &clipped.shape {
                    // Header cells have their own scissors inside one of these bands.
                    if band.contains_rect(clipped.clip_rect) {
                        let visible = text.visual_bounding_rect().intersect(clipped.clip_rect);
                        return visible.is_positive().then_some(visible);
                    }
                }
                None
            })
            .collect();
        assert!(!labels.is_empty(), "Both timescale bands must paint labels");
        for (i, rect) in labels.iter().enumerate() {
            assert!(band.contains_rect(*rect));
            assert!(
                labels[i + 1..]
                    .iter()
                    .all(|other| !rect.intersect(*other).is_positive()),
                "Visible timescale labels must not overlap adjacent cells"
            );
        }
    }

    let before = app.timeline_origin;
    let original_scale = app.pixels_per_day;
    drag_widget(
        &mut app,
        &ctx,
        upper.center(),
        upper.center() + Vec2::new(72., 0.),
        &mut time,
    );
    assert!(app.timeline_origin < before);
    assert_eq!(app.pixels_per_day, original_scale);
    assert!(
        (axis(&app).x(before as f64) - app.gantt_left - 72.).abs() <= 1.1,
        "A 72-pixel pan must move the original left-edge date by 72 pixels"
    );

    let first = app.timeline_origin;
    let lower = action_rect(&app, "timescale_zoom");
    drag_widget(
        &mut app,
        &ctx,
        lower.center(),
        lower.center() + Vec2::new(40., 0.),
        &mut time,
    );
    assert!(
        app.pixels_per_day > original_scale * 1.3,
        "Lower header drag: {} -> {} pixels/day",
        original_scale,
        app.pixels_per_day
    );
    assert_eq!(
        app.timeline_origin, first,
        "Lower-band zoom keeps its left-edge anchor"
    );

    for action in ["zoom_in", "zoom_out"] {
        let center = app.gantt_left + app.chart_width / 2.;
        let anchor_date = axis(&app).x_date(center);
        let scale_before = app.pixels_per_day;
        click(&mut app, &ctx, action, &mut time);
        assert_ne!(app.pixels_per_day, scale_before);
        assert!(
            (axis(&app).x(anchor_date) - center).abs() <= 1.1,
            "Toolbar zoom must retain the date under the chart center within one pixel"
        );
    }

    assert!(app.error.is_none());
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn ctrl_wheel_zooms_the_chart_at_the_pointer_without_rescaling_ui() {
    let (path, ctx, mut app, mut time) = test_view("wheel-zoom");
    let lower = action_rect(&app, "timescale_zoom");
    let pointer = Pos2::new(app.gantt_left + app.chart_width * 0.4, lower.bottom() + 80.);
    let anchor_date = axis(&app).x_date(pointer.x);
    let scale_before = app.pixels_per_day;
    let pixels_per_point = ctx.pixels_per_point();
    let _ = frame_with_modifiers(
        &mut app,
        &ctx,
        vec![
            egui::Event::PointerMoved(pointer),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: Vec2::new(0., 120.),
                modifiers: egui::Modifiers::CTRL,
            },
        ],
        &mut time,
        egui::Modifiers::CTRL,
    );
    assert!(
        app.pixels_per_day > scale_before,
        "Ctrl+wheel must zoom the chart"
    );
    assert_eq!(
        ctx.pixels_per_point(),
        pixels_per_point,
        "Chart zoom must not change the application's UI scale"
    );
    assert!(
        (axis(&app).x(anchor_date) - pointer.x).abs() <= 1.1,
        "Ctrl+wheel must preserve the pointer's date within one pixel"
    );
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    assert_eq!(ctx.pixels_per_point(), pixels_per_point);
    assert!(app.error.is_none());
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn view_cancel_apply_and_real_column_layout_persist_across_app_reopen() {
    let (path, ctx, mut app, mut time) = test_view("view-persistence");
    let original = app.view.clone();
    let original_gantt = app.gantt.clone();
    click(&mut app, &ctx, "timescale", &mut time);
    assert!(
        app.gantt_ui.timescale.is_some(),
        "Toolbar opens the Timescale dialog"
    );
    click(&mut app, &ctx, "ts_cancel", &mut time);
    assert!(app.gantt_ui.timescale.is_none());
    app.open_chart_options();
    for _ in 0..2 {
        let _ = frame(&mut app, &ctx, vec![], &mut time);
    }
    click(&mut app, &ctx, "chart_shade_nonwork", &mut time);
    click(&mut app, &ctx, "chart_show_relationships", &mut time);
    assert!(app.gantt_ui.options.as_ref().is_some_and(|draft| {
        draft.options.nonwork.shade != original_gantt.options.nonwork.shade
            && draft.options.show_relationships != original_gantt.options.show_relationships
    }));
    assert!(
        app.gantt == original_gantt,
        "Dialog edits remain a draft before OK"
    );
    click(&mut app, &ctx, "chart_cancel", &mut time);
    assert!(app.gantt_ui.options.is_none());
    assert!(app.gantt == original_gantt && app.view == original);

    app.open_chart_options();
    for _ in 0..2 {
        let _ = frame(&mut app, &ctx, vec![], &mut time);
    }
    click(&mut app, &ctx, "chart_shade_nonwork", &mut time);
    click(&mut app, &ctx, "chart_hatch_nonwork", &mut time);
    for _ in 0..4 {
        click(&mut app, &ctx, "row_height_up", &mut time);
    }
    let draft = app.gantt_ui.options.as_ref().unwrap();
    let expected_height = draft.row_height;
    let expected_options = draft.options;
    assert!(expected_height > original.row_height);
    click(&mut app, &ctx, "chart_ok", &mut time);
    assert!(app.gantt_ui.options.is_none());
    assert_eq!(app.gantt.options, expected_options);
    assert!(!app.gantt.options.nonwork.shade && app.gantt.options.nonwork.hatch);
    assert_eq!(app.view.row_height, expected_height);
    let expected = app.view.clone();
    let expected_gantt = app.gantt.clone();
    let _ = frame(&mut app, &ctx, vec![], &mut time);
    assert!(app
        .bar_geometry
        .iter()
        .all(|bar| (bar.row.height() - expected.row_height).abs() < 0.01));

    // Columns are pixel widths in the layout; header rectangles are real widgets.
    let ids = |app: &App| -> Vec<String> {
        app.grid
            .layout
            .columns
            .iter()
            .map(|c| c.id.clone())
            .collect()
    };
    let order = ids(&app);
    let first = action_rect(&app, &format!("grid_header_{}", order[0]));
    let second = action_rect(&app, &format!("grid_header_{}", order[1]));
    drag_widget(&mut app, &ctx, first.center(), second.center(), &mut time);
    let mut expected_order = order.clone();
    expected_order.swap(0, 1);
    assert_eq!(
        ids(&app),
        expected_order,
        "Actual header drag must reorder columns"
    );
    let width_before = app.grid.layout.columns[0].width;
    let edge = action_rect(&app, &format!("grid_resize_{}", expected_order[0])).center();
    drag_widget(&mut app, &ctx, edge, edge + Vec2::new(20., 0.), &mut time);
    assert!(app.grid.layout.columns[0].width > width_before);
    let expected_columns = app.grid.layout.columns.clone();
    for _ in 0..8 {
        let _ = frame(&mut app, &ctx, vec![], &mut time);
    }
    assert!(app.error.is_none());
    assert!(app
        .store
        .preference("activities_view_v1")
        .unwrap()
        .is_some());
    assert!(app.store.preference(grid::CURRENT_KEY).unwrap().is_some());
    drop(app);

    let reopened_ctx = egui::Context::default();
    let mut reopened = App::new(
        &reopened_ctx,
        Store::open(path.clone()).unwrap(),
        openxer_xer::starter(),
    );
    assert!(reopened.view == expected);
    assert!(reopened.gantt == expected_gantt, "Gantt settings persist");
    assert_eq!(reopened.grid.layout.columns, expected_columns);
    let _ = frame(&mut reopened, &reopened_ctx, vec![], &mut time);
    assert!(reopened.error.is_none());
    drop(reopened);
    let _ = std::fs::remove_file(path);
}

#[test]
fn lower_day_band_shades_calendar_nonwork_and_hour_band_does_not() {
    let (path, ctx, mut app, mut time) = test_view("nonwork-paint");
    click(&mut app, &ctx, "timescale", &mut time);
    click_text(&mut app, &ctx, "Automatic (follows zoom)", &mut time);
    click_text(&mut app, &ctx, "Week/Day 1", &mut time);
    click(&mut app, &ctx, "ts_ok", &mut time);
    assert_eq!(app.view.lower, timescale::Unit::Day);
    assert!(!app.view.automatic_units);
    let output = frame(&mut app, &ctx, vec![], &mut time);
    let lower = action_rect(&app, "timescale_zoom");
    let shade = app.gantt.options.nonwork.color.color();
    let shade_count = |output: &egui::FullOutput, band: Rect| {
        output
            .shapes
            .iter()
            .filter(|clipped| {
                matches!(&clipped.shape, egui::Shape::Rect(shape)
                if shape.fill == shade
                && shape.rect.intersect(clipped.clip_rect).intersect(band).is_positive())
            })
            .count()
    };
    // The reference shades nonwork in the chart body, not in the timescale.
    assert_eq!(
        shade_count(&output, lower),
        0,
        "The timescale is not shaded"
    );
    let lower = Rect::from_min_max(
        Pos2::new(lower.left(), lower.bottom()),
        Pos2::new(lower.right(), lower.bottom() + 300.),
    );
    assert!(
        shade_count(&output, lower) > 0,
        "Day zoom must actually paint nonworking calendar dates in the chart"
    );

    app.open_chart_options();
    for _ in 0..2 {
        let _ = frame(&mut app, &ctx, vec![], &mut time);
    }
    click(&mut app, &ctx, "chart_shade_nonwork", &mut time);
    click(&mut app, &ctx, "chart_ok", &mut time);
    let output = frame(&mut app, &ctx, vec![], &mut time);
    assert_eq!(
        shade_count(&output, lower),
        0,
        "Disabling shading changes real paint output"
    );

    app.open_chart_options();
    for _ in 0..2 {
        let _ = frame(&mut app, &ctx, vec![], &mut time);
    }
    click(&mut app, &ctx, "chart_shade_nonwork", &mut time);
    click(&mut app, &ctx, "chart_ok", &mut time);
    click(&mut app, &ctx, "timescale", &mut time);
    click_text(&mut app, &ctx, "Week/Day 1", &mut time);
    click_text(&mut app, &ctx, "Day/Hour", &mut time);
    click(&mut app, &ctx, "ts_ok", &mut time);
    assert_eq!(app.view.lower, timescale::Unit::Hour);
    assert!(app.gantt.options.nonwork.shade);
    let output = frame(&mut app, &ctx, vec![], &mut time);
    assert_eq!(
        shade_count(&output, lower),
        0,
        "Hour-axis nonwork shading is disabled even when the preference is enabled"
    );
    assert!(app.error.is_none());
    drop(app);
    let _ = std::fs::remove_file(path);
}
