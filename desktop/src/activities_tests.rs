//! Headless checks for the Activities window: the table, the Gantt chart
//! and the Activity Details header. They drive `App::update_ui` through egui
//! input events only; no native window is opened.
use super::*;

pub(crate) fn temp_db(name: &str) -> PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "openxer-activities-{name}-{}-{unique}.db",
        std::process::id()
    ))
}

pub(crate) fn cleanup(path: PathBuf) {
    for suffix in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
    }
}

pub(crate) fn scheduled_starter() -> ProjectData {
    let mut project = openxer_xer::starter();
    schedule_model(&mut project.model, &project.resources).unwrap();
    project
}

pub(crate) fn open(name: &str, project: ProjectData) -> (PathBuf, egui::Context, App, f64) {
    let path = temp_db(name);
    let store = Store::open(path.clone()).unwrap();
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.);
    let mut app = App::new(&ctx, store, project);
    let mut time = 0.;
    for _ in 0..3 {
        frame(&mut app, &ctx, vec![], &mut time);
    }
    (path, ctx, app, time)
}

pub(crate) fn frame_mod(
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

pub(crate) fn frame(
    app: &mut App,
    ctx: &egui::Context,
    events: Vec<egui::Event>,
    time: &mut f64,
) -> egui::FullOutput {
    frame_mod(app, ctx, events, time, Default::default())
}

pub(crate) fn key_mod(
    app: &mut App,
    ctx: &egui::Context,
    key: Key,
    modifiers: egui::Modifiers,
    time: &mut f64,
) {
    for pressed in [true, false] {
        frame_mod(
            app,
            ctx,
            vec![egui::Event::Key {
                key,
                physical_key: Some(key),
                pressed,
                repeat: false,
                modifiers,
            }],
            time,
            modifiers,
        );
    }
}

pub(crate) fn key(app: &mut App, ctx: &egui::Context, key: Key, time: &mut f64) {
    key_mod(app, ctx, key, Default::default(), time);
}

pub(crate) fn ctrl() -> egui::Modifiers {
    egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    }
}

pub(crate) fn shift() -> egui::Modifiers {
    egui::Modifiers {
        shift: true,
        ..Default::default()
    }
}

pub(crate) fn type_text(app: &mut App, ctx: &egui::Context, text: &str, time: &mut f64) {
    frame(app, ctx, vec![egui::Event::Text(text.into())], time);
}

pub(crate) fn click_at(app: &mut App, ctx: &egui::Context, pos: Pos2, time: &mut f64) {
    for pressed in [true, false] {
        frame(
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
    frame(app, ctx, vec![], time);
}

pub(crate) fn click(app: &mut App, ctx: &egui::Context, action: &str, time: &mut f64) {
    let [l, t, r, b] = *app
        .action_rects
        .get(action)
        .unwrap_or_else(|| panic!("No rendered control {action}"));
    click_at(app, ctx, Pos2::new((l + r) / 2., (t + b) / 2.), time);
}

pub(crate) fn index_of(app: &App, task_id: i64) -> usize {
    app.project
        .model
        .activities
        .iter()
        .position(|a| a.task_id == task_id)
        .unwrap()
}

/// Click a Details header field, replace its text and leave it with
/// `finish` (Enter, or None to leave it focused).
fn retype_header(
    app: &mut App,
    ctx: &egui::Context,
    action: &str,
    text: &str,
    finish: Option<Key>,
    time: &mut f64,
) {
    click(app, ctx, action, time);
    key_mod(app, ctx, Key::A, ctrl(), time);
    if text.is_empty() {
        key(app, ctx, Key::Backspace, time);
    } else {
        type_text(app, ctx, text, time);
    }
    if let Some(k) = finish {
        key(app, ctx, k, time);
        frame(app, ctx, vec![], time);
    }
}

#[test]
fn details_header_id_and_name_commit_through_the_setters() {
    let (path, ctx, mut app, mut time) = open("header", scheduled_starter());
    let i = 1;
    let other = app.project.model.activities[0].task_code.clone();
    let before = app.project.model.activities[i].task_code.clone();
    app.jump_to(i);
    frame(&mut app, &ctx, vec![], &mut time);
    let steps = app.history.steps();

    // Typing writes nothing until the field commits.
    retype_header(
        &mut app,
        &ctx,
        "detail_activity_id",
        "NEW-1",
        None,
        &mut time,
    );
    assert_eq!(app.project.model.activities[i].task_code, before);
    key(&mut app, &ctx, Key::Enter, &mut time);
    frame(&mut app, &ctx, vec![], &mut time);
    assert_eq!(app.project.model.activities[i].task_code, "NEW-1");
    assert_eq!(app.history.steps(), steps + 1, "One commit, one undo step");

    // A duplicate and an empty ID are refused with a message.
    for bad in [other.as_str(), ""] {
        app.error = None;
        retype_header(
            &mut app,
            &ctx,
            "detail_activity_id",
            bad,
            Some(Key::Enter),
            &mut time,
        );
        assert_eq!(app.project.model.activities[i].task_code, "NEW-1");
        assert!(
            app.error.as_deref().unwrap_or("").contains("unique"),
            "{bad:?} must be refused: {:?}",
            app.error
        );
    }
    app.error = None;

    // The name goes through the same setter (trimmed).
    retype_header(
        &mut app,
        &ctx,
        "detail_activity_name",
        "  Pour slab  ",
        Some(Key::Enter),
        &mut time,
    );
    assert_eq!(app.project.name(i), "Pour slab");
    assert!(app.error.is_none());
    cleanup(path);
}

/// Whole-project comparisons and row builds one still frame makes with the
/// pointer at `pos`.
fn costs_in_a_frame(app: &mut App, ctx: &egui::Context, pos: Pos2, time: &mut f64) -> (u64, u64) {
    frame(app, ctx, vec![egui::Event::PointerMoved(pos)], time);
    let compares = crate::history::STATE_COMPARES.with(|n| n.get());
    let builds = crate::grid::ROW_BUILDS.with(|n| n.get());
    frame(app, ctx, vec![egui::Event::PointerMoved(pos)], time);
    (
        crate::history::STATE_COMPARES.with(|n| n.get()) - compares,
        crate::grid::ROW_BUILDS.with(|n| n.get()) - builds,
    )
}

#[test]
fn a_still_frame_compares_the_project_once_and_builds_no_rows() {
    let (path, ctx, mut app, mut time) = open("perframe", scheduled_starter());
    app.jump_to(2);
    frame(&mut app, &ctx, vec![], &mut time);
    // Over the rows and over the column headers, where every row and
    // header used to work out the context menu's capabilities each frame.
    let over_rows = Pos2::new(200., app.grid.body_top + 40.);
    let over_header = Pos2::new(200., app.grid.body_top - 10.);
    for pos in [over_rows, over_header] {
        let (compares, builds) = costs_in_a_frame(&mut app, &ctx, pos, &mut time);
        assert_eq!(builds, 0, "A still frame reuses the row build");
        assert!(
            compares <= 1,
            "The table and Gantt compare the project with its snapshot once per frame, not {compares}"
        );
    }
    // The cached rows are handed out without a copy.
    let a = app.shown_rows_shared();
    let b = app.shown_rows_shared();
    assert!(std::rc::Rc::ptr_eq(&a, &b));
    cleanup(path);
}

#[test]
fn a_date_typed_without_a_time_takes_the_calendar_day_edge() {
    let (path, ctx, mut app, mut time) = open("dayedge", scheduled_starter());
    let i = 1;
    let m = &app.project.model;
    let cal = m.calendars[m.activities[i].cal].cal.clone();
    let data_day = m.project.data_date.unwrap().div_euclid(DAY);
    // The first working day ten or more days after the data date.
    let day = (data_day + 10..data_day + 30)
        .find(|d| !cal.shifts_on(*d).is_empty())
        .unwrap();
    let shifts = cal.shifts_on(day).to_vec();
    let (day_start, day_end) = (shifts[0].0, shifts.last().unwrap().1);
    assert!(day_start > 0 && day_end < DAY, "{shifts:?}");
    let text = date_string(m.origin_serial, Some(day * DAY))[..10].to_string();

    // A finish-type field takes the end of the working day.
    assert!(app.apply_cell(i, "expected_finish", &text));
    assert_eq!(
        app.project.model.activities[i].expect_end,
        Some(day * DAY + day_end)
    );
    // A typed time still wins.
    app.apply_cell(i, "expected_finish", "");
    assert!(app.apply_cell(i, "expected_finish", &format!("{text} 10:30")));
    assert_eq!(
        app.project.model.activities[i].expect_end,
        Some(day * DAY + 10 * 60 + 30)
    );

    // Start-type fields take the start of the working day; a constraint
    // date follows its constraint type (the calendar popup uses the same).
    use crate::grid::columns::Field;
    use crate::grid::edit::finish_type;
    assert_eq!(app.day_edge(i, false, day), day_start);
    assert_eq!(app.day_edge(i, true, day), day_end);
    let mut a = app.project.model.activities[i].clone();
    assert!(!finish_type(Field::ActualStart, &a));
    assert!(finish_type(Field::ActualFinish, &a));
    assert!(!finish_type(Field::SuspendDate, &a));
    a.cstr = CstrType::StartOnAfter;
    assert!(!finish_type(Field::PrimaryConstraintDate, &a));
    a.cstr = CstrType::FinishOnBefore;
    assert!(finish_type(Field::PrimaryConstraintDate, &a));
    // A day off takes the next working day's hours.
    if let Some(off) = (day..day + 7).find(|d| cal.shifts_on(*d).is_empty()) {
        let next = (off..off + 14)
            .find(|d| !cal.shifts_on(*d).is_empty())
            .unwrap();
        assert_eq!(app.day_edge(i, false, off), cal.shifts_on(next)[0].0);
    }
    frame(&mut app, &ctx, vec![], &mut time);
    cleanup(path);
}

/// The day edge on calendars whose shifts are not one tidy daytime block:
/// an overnight shift ends the next morning, unsorted shifts are read in
/// time order, and a split day spans its lunch break.
#[test]
fn the_day_edge_follows_overnight_unsorted_and_split_shifts() {
    use openxer_engine::calendar::WorkCalendar;
    let (path, _ctx, mut app, _time) = open("dayedge-shifts", scheduled_starter());
    let i = 1;
    let data_day = app.project.model.project.data_date.unwrap().div_euclid(DAY);
    let day = data_day + 10;
    let weekday = day.rem_euclid(7) as usize;
    let cases = [
        // 22:00 to 06:00 the next morning.
        ("overnight", vec![(1320, 360)], 1320, DAY + 360),
        // Stored afternoon first.
        ("unsorted", vec![(780, 1020), (480, 720)], 480, 1020),
        // 07:00 to 11:30, lunch, 12:30 to 16:00.
        ("split", vec![(420, 690), (750, 960)], 420, 960),
    ];
    for (name, shifts, start, end) in cases {
        let mut template: [Vec<(i64, i64)>; 7] = Default::default();
        template[weekday] = shifts;
        let m = &mut app.project.model;
        let c = m.activities[i].cal;
        let origin = m.calendars[c].cal.origin_serial;
        m.calendars[c].cal =
            WorkCalendar::new(name.into(), false, origin, template, Default::default());
        assert_eq!(app.day_edge(i, false, day), start, "{name} start");
        assert_eq!(app.day_edge(i, true, day), end, "{name} end");
        // A day off before it takes its hours.
        assert_eq!(app.day_edge(i, false, day - 1), start, "{name} day off");
        // A finish typed without a time lands on that edge.
        let text = date_string(app.project.model.origin_serial, Some(day * DAY))[..10].to_string();
        app.apply_cell(i, "expected_finish", "");
        assert!(app.apply_cell(i, "expected_finish", &text), "{name}");
        assert_eq!(
            app.project.model.activities[i].expect_end,
            Some(day * DAY + end),
            "{name} typed finish"
        );
    }
    cleanup(path);
}

#[test]
fn undo_and_redo_keep_the_selected_activity() {
    let (path, ctx, mut app, mut time) = open("undosel", scheduled_starter());
    let first = app.project.model.activities[0].task_id;
    let kept = app.project.model.activities[3].task_id;
    app.delete_activities(&[first], time);
    frame(&mut app, &ctx, vec![], &mut time);
    app.selected = Some(index_of(&app, kept));
    for redo in [false, true] {
        app.history_move(&ctx, redo);
        frame(&mut app, &ctx, vec![], &mut time);
        let shown = app
            .selected
            .map(|i| app.project.model.activities[i].task_id);
        assert_eq!(shown, Some(kept), "redo {redo}");
    }
    cleanup(path);
}

#[test]
fn fill_down_and_paste_reach_user_fields_and_skip_unchanged_cells() {
    let mut project = scheduled_starter();
    project.document.table_mut("UDFTYPE").add(&[
        ("udf_type_id", "10".into()),
        ("table_name", "TASK".into()),
        ("udf_type_name", "user_field_10".into()),
        ("udf_type_label", "Crew Size".into()),
        ("logical_data_type", "FT_INT".into()),
    ]);
    let (path, ctx, mut app, mut time) = open("filldown", project);
    let column = "udf:Crew Size";
    app.grid.layout.columns = vec![
        crate::grid::layout::ColumnSetting::new("activity_id", 90.),
        crate::grid::layout::ColumnSetting::new(column, 100.),
    ];
    app.drop_frame_cache();
    frame(&mut app, &ctx, vec![], &mut time);
    let rows: Vec<usize> = app.table_activities().into_iter().take(3).collect();
    assert!(app.apply_cell(rows[0], column, "4"));
    frame(&mut app, &ctx, vec![], &mut time);
    app.select_activities(&rows);
    app.grid.focus_column = Some(column.into());
    let shown = |app: &App, i: usize| {
        let catalog = crate::grid::columns::catalog(&app.project);
        app.input_text(i, crate::grid::columns::find(&catalog, column).unwrap())
    };

    let steps = app.history.steps();
    app.fill_down(time);
    assert!(app.error.is_none(), "{:?}", app.error);
    for &i in &rows {
        assert_eq!(shown(&app, i), "4", "{}", app.status);
    }
    assert_eq!(app.history.steps(), steps + 1);

    // Filling the same values again changes nothing: no undo step, no
    // modified mark.
    app.dirty = false;
    app.fill_down(time);
    assert_eq!(app.history.steps(), steps + 1);
    assert!(!app.dirty);
    assert!(app.status.contains(" 0 cells changed"), "{}", app.status);

    // Pasted text reaches the field too.
    app.paste_text("7", time);
    for &i in &rows {
        assert_eq!(shown(&app, i), "7", "{}", app.status);
    }
    cleanup(path);
}

#[test]
fn details_lag_commits_once_on_enter() {
    let (path, ctx, mut app, mut time) = open("lag", scheduled_starter());
    let i = app.project.model.links[0].succ;
    let lags = |app: &App| -> Vec<i64> {
        let m = &app.project.model;
        m.links.iter().map(|l| l.lag).collect()
    };
    app.jump_to(i);
    frame(&mut app, &ctx, vec![], &mut time);
    app.show_detail_tab(crate::details::DetailTab::Predecessors);
    frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "detail_tab_predecessors", &mut time);
    app.detail.rows[0] = Some(0);
    frame(&mut app, &ctx, vec![], &mut time);
    frame(&mut app, &ctx, vec![], &mut time);
    let before = lags(&app);
    let steps = app.history.steps();
    click(&mut app, &ctx, "detail_pred_lag_0", &mut time);
    key_mod(&mut app, &ctx, Key::A, ctrl(), &mut time);
    type_text(&mut app, &ctx, "2d", &mut time);
    assert_eq!(lags(&app), before, "Typing alone changes nothing");
    key(&mut app, &ctx, Key::Enter, &mut time);
    frame(&mut app, &ctx, vec![], &mut time);
    assert!(app.error.is_none(), "{:?}", app.error);
    let after = lags(&app);
    let changed: Vec<usize> = (0..after.len())
        .filter(|&k| after[k] != before[k])
        .collect();
    assert_eq!(changed.len(), 1, "{before:?} -> {after:?}");
    let link = &app.project.model.links[changed[0]];
    assert_eq!(link.succ, i);
    let periods = format::lag_periods(&app.project, link.pred, link.succ);
    assert_eq!(link.lag, format::parse_duration("2d", periods).unwrap());
    assert_eq!(app.history.steps(), steps + 1, "One commit, one undo step");

    // Text that is not a duration is refused and changes nothing.
    click(&mut app, &ctx, "detail_pred_lag_0", &mut time);
    key_mod(&mut app, &ctx, Key::A, ctrl(), &mut time);
    type_text(&mut app, &ctx, "soon", &mut time);
    key(&mut app, &ctx, Key::Enter, &mut time);
    frame(&mut app, &ctx, vec![], &mut time);
    assert!(app.error.is_some());
    assert_eq!(lags(&app), after);
    cleanup(path);
}

#[test]
fn the_status_bar_says_when_the_schedule_needs_a_run() {
    let (path, ctx, mut app, mut time) = open("needed", scheduled_starter());
    app.sched.auto_schedule = false;
    let needed = |app: &App| {
        app.status_panels()
            .iter()
            .any(|p| p == "Schedule needed: press F9")
    };
    assert!(!needed(&app), "A freshly scheduled project");
    // A name is not a schedule input.
    assert!(app.apply_cell(1, "activity_name", "Renamed"));
    frame(&mut app, &ctx, vec![], &mut time);
    assert!(!needed(&app));
    // A duration is.
    assert!(app.apply_cell(1, "original_duration", "9d"));
    frame(&mut app, &ctx, vec![], &mut time);
    assert!(needed(&app));
    // With automatic scheduling on the engine runs by itself: no panel.
    app.sched.auto_schedule = true;
    assert!(!needed(&app));
    app.sched.auto_schedule = false;
    // Undoing the edit returns to the scheduled inputs.
    app.history_move(&ctx, false);
    frame(&mut app, &ctx, vec![], &mut time);
    assert!(!needed(&app));
    app.history_move(&ctx, true);
    frame(&mut app, &ctx, vec![], &mut time);
    assert!(needed(&app));
    // Scheduling (F9) clears it.
    app.schedule();
    frame(&mut app, &ctx, vec![], &mut time);
    assert!(!needed(&app), "{}", app.status);
    cleanup(path);
}

#[test]
fn reveal_offset_moves_the_least() {
    use super::reveal_offset;
    // In view: no scroll.
    assert_eq!(reveal_offset(Some(100.), 200., 150., 170.), None);
    // Above: its top at the top; below: its bottom at the bottom.
    assert_eq!(reveal_offset(Some(100.), 200., 80., 100.), Some(80.));
    assert_eq!(reveal_offset(Some(100.), 200., 290., 310.), Some(110.));
    // No scroll state yet: to the top, as before.
    assert_eq!(reveal_offset(None, 200., 290., 310.), Some(290.));
}

#[test]
fn keyboard_moves_scroll_only_to_reveal_the_row() {
    let mut d = crate::qa_sweep::Driver::new("reveal", scheduled_starter());
    // A short window, so the table scrolls.
    d.screen = Vec2::new(1500., 640.);
    d.frames(3);
    let activities: Vec<usize> = d
        .app
        .shown_rows()
        .into_iter()
        .filter_map(|r| match r {
            crate::grid::Row::Activity(i) => Some(i),
            _ => None,
        })
        .collect();
    let top_of = |d: &crate::qa_sweep::Driver, i: usize| {
        let rows = d.app.shown_rows();
        let k = rows
            .iter()
            .position(|r| *r == crate::grid::Row::Activity(i))
            .unwrap();
        d.app.grid_row_tops(&rows)[k]
    };
    let select = |d: &mut crate::qa_sweep::Driver, i: usize| {
        d.app.selected = Some(i);
        d.app.scroll_to_selected = true;
        d.frames(3);
    };
    select(&mut d, activities[0]);
    let before = top_of(&d, activities[1]).0;
    // The next row is in view: nothing scrolls (it used to jump to the top).
    select(&mut d, activities[1]);
    assert_eq!(top_of(&d, activities[1]).0, before);
    // The last row scrolls in at the bottom, the least it can, and moving
    // back one row keeps the view.
    let last = *activities.last().unwrap();
    select(&mut d, last);
    let (top, height) = top_of(&d, last);
    let body_bottom = d.rect("hscroll_gantt").top();
    assert!(top + height <= body_bottom + 0.5);
    assert!(
        top + height > body_bottom - height,
        "{top} {height} {body_bottom}"
    );
    let previous = activities[activities.len() - 2];
    let kept = top_of(&d, previous).0;
    select(&mut d, previous);
    assert_eq!(top_of(&d, previous).0, kept);
}

#[test]
fn an_open_choice_list_takes_the_arrow_keys_enter_and_delete() {
    let (path, ctx, mut app, mut time) = open("choicekeys", scheduled_starter());
    app.grid.layout.columns = vec![
        crate::grid::layout::ColumnSetting::new("activity_id", 90.),
        crate::grid::layout::ColumnSetting::new("activity_type", 130.),
    ];
    app.drop_frame_cache();
    let i = 2;
    app.selected = Some(i);
    app.grid.focus_column = Some("activity_type".into());
    frame(&mut app, &ctx, vec![], &mut time);
    let before_type = app.project.model.activities[i].task_type;
    key(&mut app, &ctx, Key::F2, &mut time);
    frame(&mut app, &ctx, vec![], &mut time);
    assert!(app.choice_editor_open(), "F2 opens the list");
    let shown = app.edit_text.clone();
    // Down moves the highlight, not the row selection.
    key(&mut app, &ctx, Key::ArrowDown, &mut time);
    assert_eq!(app.selected, Some(i));
    assert!(app.choice_editor_open());
    assert_ne!(app.edit_text, shown, "the highlight moved");
    // Delete does not ask to delete the activity behind the list.
    key(&mut app, &ctx, Key::Delete, &mut time);
    assert!(app.delete_confirm.is_none());
    // Enter takes the highlighted value.
    let chosen = app.edit_text.clone();
    key(&mut app, &ctx, Key::Enter, &mut time);
    frame(&mut app, &ctx, vec![], &mut time);
    assert!(!app.choice_editor_open());
    assert!(app.error.is_none(), "{:?}", app.error);
    let after = app.project.model.activities[i].task_type;
    assert_ne!(after, before_type, "{chosen} applied");
    assert_eq!(app.selected, Some(i));
    cleanup(path);
}

#[test]
fn cut_only_marks_the_selection() {
    let (path, ctx, mut app, mut time) = open("cutask", scheduled_starter());
    let count = app.project.model.activities.len();
    let undo = app.history.counts().0;
    app.select_activities(&[1]);
    app.cut_activities(&ctx, time);
    frame(&mut app, &ctx, vec![], &mut time);
    assert!(app.delete_confirm.is_none(), "no confirmation");
    assert_eq!(app.project.model.activities.len(), count);
    assert_eq!(app.history.counts().0, undo, "no undo step");
    assert!(app.editing.clipboard.as_ref().is_some_and(|c| c.cut));
    cleanup(path);
}

#[test]
fn row_keys_page_by_the_view_extend_with_shift_and_start_at_the_first_row() {
    let mut d = crate::qa_sweep::Driver::new("rowkeys", scheduled_starter());
    d.screen = Vec2::new(1500., 640.);
    d.frames(3);
    let order = d.visible_activities();
    // Nothing selected: Down starts at the first row, not the second.
    d.app.selected = None;
    d.app.editing.selection.clear();
    d.frames(1);
    d.key(Key::ArrowDown);
    assert_eq!(d.app.selected, Some(order[0]));
    // PageDown moves by the rows in view (the bands count as rows there).
    let page = d.app.grid.page_rows;
    assert!(page > 1 && page < order.len(), "{page} of {}", order.len());
    d.key(Key::PageDown);
    assert_eq!(d.app.selected, Some(order[page]));
    d.key(Key::PageUp);
    assert_eq!(d.app.selected, Some(order[0]));
    // Shift+End extends the selection to the last row; Shift+Home back.
    d.key_with(Key::End, shift());
    assert_eq!(d.app.selected_in_order(), order);
    d.key(Key::Home);
    d.key_with(Key::PageDown, shift());
    assert_eq!(d.app.selected_in_order().len(), page + 1);
}

#[test]
fn ctrl_shift_z_redoes() {
    let (path, ctx, mut app, mut time) = open("redo", scheduled_starter());
    assert!(app.apply_cell(1, "activity_name", "Renamed"));
    frame(&mut app, &ctx, vec![], &mut time);
    key_mod(&mut app, &ctx, Key::Z, ctrl(), &mut time);
    assert_ne!(app.project.name(1), "Renamed");
    let ctrl_shift = egui::Modifiers {
        shift: true,
        ..ctrl()
    };
    key_mod(&mut app, &ctx, Key::Z, ctrl_shift, &mut time);
    assert_eq!(app.project.name(1), "Renamed");
    cleanup(path);
}

#[test]
fn delete_and_dissolve_select_the_next_table_row() {
    let (path, ctx, mut app, mut time) = open("nextrow", scheduled_starter());
    // Sorted by Activity ID, descending: table order is not model order.
    app.grid.layout.group_sort.sort = vec![crate::grid::layout::SortKey {
        column: "activity_id".into(),
        descending: true,
    }];
    app.drop_frame_cache();
    frame(&mut app, &ctx, vec![], &mut time);
    let task = |app: &App, i: usize| app.project.model.activities[i].task_id;
    for dissolve in [false, true] {
        let order: Vec<i64> = app
            .table_activities()
            .into_iter()
            .map(|i| task(&app, i))
            .collect();
        // Dissolve needs a predecessor and a successor: take the first such
        // row from the third on.
        let k = if dissolve {
            (2..order.len() - 1)
                .find(|&k| {
                    let i = app
                        .project
                        .model
                        .activities
                        .iter()
                        .position(|a| a.task_id == order[k])
                        .unwrap();
                    crate::editing::ops::can_dissolve(&app.project.model, i)
                })
                .expect("a row that can be dissolved")
        } else {
            2
        };
        let (gone, next) = (order[k], order[k + 1]);
        if dissolve {
            app.dissolve_activities(&[gone], time);
        } else {
            app.delete_activities(&[gone], time);
        }
        frame(&mut app, &ctx, vec![], &mut time);
        assert_eq!(
            app.selected.map(|i| task(&app, i)),
            Some(next),
            "dissolve {dissolve}"
        );
        assert!(!app.scroll_to_selected || app.selected.is_some());
    }
    cleanup(path);
}

/// Assign Predecessors / Successors lists every activity under the bands
/// and in the order of the activity table, whatever the table filters,
/// searches for or has collapsed.
#[test]
fn relationship_picker_lists_activities_in_table_order() {
    use crate::grid::{PickLine, Row};
    let (path, ctx, mut app, mut time) = open("pickorder", scheduled_starter());
    // Sorted by Activity ID, descending: table order is not model order.
    app.grid.layout.group_sort.sort = vec![crate::grid::layout::SortKey {
        column: "activity_id".into(),
        descending: true,
    }];
    app.drop_frame_cache();
    frame(&mut app, &ctx, vec![], &mut time);
    let rows = app.built_rows();
    let table: Vec<usize> = rows
        .iter()
        .filter_map(|r| match r {
            Row::Activity(i) => Some(*i),
            Row::Group(_) => None,
        })
        .collect();
    let model: Vec<usize> = (0..app.project.model.activities.len()).collect();
    assert_ne!(table, model, "the sort reorders the table");
    let activities = |lines: &[PickLine]| -> Vec<usize> {
        lines
            .iter()
            .filter_map(|l| match l {
                PickLine::Activity(k) => Some(*k),
                PickLine::Band { .. } => None,
            })
            .collect()
    };
    let none = std::collections::BTreeSet::new();
    let lines = app.picker_lines("", &none, &|_| false);
    assert_eq!(activities(&lines), table, "the table's order");
    let bands = rows.iter().filter(|r| matches!(r, Row::Group(_))).count();
    assert!(bands > 0, "the starter layout groups by WBS");
    assert_eq!(lines.len() - table.len(), bands, "the table's bands");
    assert!(matches!(lines[0], PickLine::Band { level: 0, .. }));

    // An activity that takes the relationship is not offered.
    let lines = app.picker_lines("", &none, &|k| k == table[0]);
    assert_eq!(activities(&lines), table[1..]);

    // Find narrows the list by ID or name.
    let code = app.project.model.activities[table[0]].task_code.clone();
    let found = activities(&app.picker_lines(&code, &none, &|_| false));
    assert!(found.contains(&table[0]) && found.len() < table.len());

    // A band folded in the picker keeps its row and hides what is under it.
    let PickLine::Band { key, collapsed, .. } = &lines[0] else {
        unreachable!()
    };
    assert!(!collapsed);
    let folded = std::collections::BTreeSet::from([key.clone()]);
    let lines = app.picker_lines("", &folded, &|_| false);
    assert!(matches!(
        &lines[0],
        PickLine::Band {
            collapsed: true,
            ..
        }
    ));
    assert!(activities(&lines).len() < table.len());

    // The table's own search and collapsed bands hide nothing here.
    let keys: Vec<String> = app
        .grid
        .bands
        .borrow()
        .iter()
        .map(|b| b.key.clone())
        .collect();
    app.collapsed.extend(keys);
    app.query = "no activity is called this".into();
    assert_eq!(activities(&app.picker_lines("", &none, &|_| false)), table);
    cleanup(path);
}

#[test]
fn a_refused_cell_edit_keeps_the_typed_text() {
    let (path, ctx, mut app, mut time) = open("refused", scheduled_starter());
    app.grid.layout.columns = vec![
        crate::grid::layout::ColumnSetting::new("activity_id", 90.),
        crate::grid::layout::ColumnSetting::new("original_duration", 100.),
    ];
    app.drop_frame_cache();
    let i = 2;
    app.selected = Some(i);
    app.grid.focus_column = Some("original_duration".into());
    frame(&mut app, &ctx, vec![], &mut time);
    let before = app.project.details[&app.project.model.activities[i].task_id].original_duration;
    key(&mut app, &ctx, Key::F2, &mut time);
    key_mod(&mut app, &ctx, Key::A, ctrl(), &mut time);
    type_text(&mut app, &ctx, "abc", &mut time);
    key(&mut app, &ctx, Key::Enter, &mut time);
    frame(&mut app, &ctx, vec![], &mut time);
    assert!(app.error.is_some(), "refused");
    assert_eq!(app.edit_cell, Some((i, "original_duration".into())));
    assert_eq!(app.edit_text, "abc");
    // Closing the message (Enter) does not commit it again.
    key(&mut app, &ctx, Key::Enter, &mut time);
    frame(&mut app, &ctx, vec![], &mut time);
    assert!(app.error.is_none(), "{:?}", app.error);
    assert_eq!(app.edit_cell, Some((i, "original_duration".into())));
    assert_eq!(app.edit_text, "abc");
    // Corrected, it commits.
    key_mod(&mut app, &ctx, Key::A, ctrl(), &mut time);
    type_text(&mut app, &ctx, "4d", &mut time);
    key(&mut app, &ctx, Key::Enter, &mut time);
    frame(&mut app, &ctx, vec![], &mut time);
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(app.edit_cell.is_none());
    let after = app.project.details[&app.project.model.activities[i].task_id].original_duration;
    assert_ne!(after, before);
    cleanup(path);
}

#[test]
fn the_duration_range_message_uses_the_shown_unit() {
    let (path, _ctx, mut app, _time) = open("durunit", scheduled_starter());
    let periods = format::activity_periods(&app.project, 1);
    let top = format::duration(100_000 * 60, periods);
    assert!(!top.ends_with('h'), "the starter shows days: {top}");
    for text in ["-2d", "20000d"] {
        let err = app
            .try_apply_cell(1, "original_duration", text)
            .unwrap_err();
        assert_eq!(
            err,
            format!(
                "Original Duration must be from {} to {top}.",
                format::duration(0, periods)
            )
        );
    }
    cleanup(path);
}

#[test]
fn go_to_says_when_it_changed_the_filter() {
    let (path, ctx, mut app, mut time) = open("gotofilter", scheduled_starter());
    let i = 3;
    let code = app.project.model.activities[i].task_code.clone();
    // Nothing hides it: no message.
    app.status.clear();
    app.jump_to(i);
    assert_eq!(app.status, "");
    // A find text and a WBS filter hide it: both are cleared, and said.
    let other_wbs = app
        .project
        .wbs
        .iter()
        .map(|w| w.id)
        .find(|id| app.project.details[&app.project.model.activities[i].task_id].wbs != *id)
        .unwrap();
    app.query = "no such activity".into();
    app.wbs_filter = Some(other_wbs);
    frame(&mut app, &ctx, vec![], &mut time);
    app.jump_to(i);
    assert_eq!(
        app.status,
        format!("To show {code}: the find text cleared, the WBS filter cleared.")
    );
    assert!(app.query.is_empty() && app.wbs_filter.is_none());
    cleanup(path);
}

#[test]
fn an_empty_table_says_why_it_is_empty() {
    let (path, ctx, mut app, mut time) = open("emptytext", scheduled_starter());
    app.query = "no such activity".into();
    frame(&mut app, &ctx, vec![], &mut time);
    assert!(app.rows().is_empty());
    assert_eq!(
        app.empty_table_text(),
        "No activities match the current filter."
    );
    app.query.clear();
    let ids: Vec<i64> = app
        .project
        .model
        .activities
        .iter()
        .map(|a| a.task_id)
        .collect();
    app.delete_activities(&ids, time);
    frame(&mut app, &ctx, vec![], &mut time);
    assert!(app
        .rows()
        .iter()
        .all(|r| !matches!(r, crate::grid::Row::Activity(_))));
    assert!(app
        .empty_table_text()
        .starts_with("This project has no activities"));
    cleanup(path);
}

#[test]
fn relationship_lines_reach_bars_out_of_view() {
    let (path, ctx, mut app, mut time) = open("offscreen", scheduled_starter());
    let m = &app.project.model;
    let (index, link) = m
        .links
        .iter()
        .enumerate()
        .find(|(_, l)| {
            let (p, s) = (&m.activities[l.pred], &m.activities[l.succ]);
            l.link_type == LinkType::FS
                && p.early_finish
                    .zip(s.early_finish)
                    .is_some_and(|(pf, sf)| sf > pf + 3 * DAY)
        })
        .map(|(k, l)| (k, l.clone()))
        .unwrap();
    let pred_finish = m.activities[link.pred].early_finish.unwrap();
    frame(&mut app, &ctx, vec![], &mut time);
    assert!(app.link_geometry.iter().any(|(k, _)| *k == index));
    // Pan the chart so the predecessor's bar ends just left of it.
    app.timeline_origin = pred_finish + DAY;
    frame(&mut app, &ctx, vec![], &mut time);
    let output = frame(&mut app, &ctx, vec![], &mut time);
    assert!(
        !app.bar_geometry.iter().any(|b| b.activity == link.pred),
        "The predecessor's bar is out of view"
    );
    assert!(app.bar_geometry.iter().any(|b| b.activity == link.succ));
    let points = &app
        .link_geometry
        .iter()
        .find(|(k, _)| *k == index)
        .expect("The successor's incoming line is still drawn")
        .1;
    // It comes in from the chart's left edge.
    let left = points.iter().map(|p| p.x).fold(f32::INFINITY, f32::min);
    assert!(left < app.gantt_left, "{left} vs {}", app.gantt_left);
    assert!(!output.shapes.is_empty());
    cleanup(path);
}
