//! In-place progress editing in the activity table: every newly editable
//! column against the matching Activity Details field, the table keyboard,
//! band selection, add-into-band, dragging activities onto bands, the date
//! editor's calendar and band totals. Production UI through egui `RawInput`
//! only.
use super::*;
use crate::details::setters::{Edit, UdfValue};
use crate::details::UnitsMode;
use crate::grid::columns::{Measure, Phase};
use openxer_xer::assignments::AssignmentEdit;

/// The center of activity `i`'s cell in column `id`, from the rendered
/// header and the table's own row offsets.
fn row_cell(app: &App, i: usize, id: &str) -> Pos2 {
    let rows = app.rows();
    let k = rows
        .iter()
        .position(|r| *r == Row::Activity(i))
        .expect("the activity is shown");
    let (top, height) = app.grid_row_tops(&rows)[k];
    Pos2::new(
        rect(app, &format!("grid_header_{id}")).center().x,
        top + height / 2.,
    )
}

fn minute(p: &ProjectData, text: &str) -> i64 {
    openxer_xer::parse_relative(p.model.origin_serial, text)
        .unwrap()
        .unwrap()
}

/// The rich starter with a number and a date user defined field, the data
/// date two weeks in, scheduled.
fn progress_project() -> ProjectData {
    let mut p = rich_starter();
    for (id, label, kind) in [(10, "Crew Size", "FT_INT"), (11, "Permit", "FT_END_DATE")] {
        p.document.table_mut("UDFTYPE").add(&[
            ("udf_type_id", id.to_string()),
            ("table_name", "TASK".into()),
            ("udf_type_name", format!("user_field_{id}")),
            ("udf_type_label", label.into()),
            ("logical_data_type", kind.into()),
        ]);
    }
    p.model.project.data_date = Some(minute(&p, "2026-11-16 08:00"));
    schedule_model(&mut p.model, &p.resources).unwrap();
    p
}

/// Progress both copies start from: 1010 and 1050 in progress, 1020 done.
fn prepare(app: &mut App) {
    for (task, status) in [
        (1010, StatusType::InProgress),
        (1020, StatusType::Completed),
        (1050, StatusType::InProgress),
    ] {
        let i = index_of(&app.project, task);
        assert_eq!(app.edit_activity(i, Edit::Status(status)), Ok(true));
    }
    app.sched.sequence = None;
}

fn tables(app: &App) -> Vec<(String, Vec<String>, Vec<Vec<String>>)> {
    let doc = app.project.updated_document();
    ["TASK", "TASKACTV", "UDFVALUE", "TASKRSRC"]
        .iter()
        .map(|name| {
            doc.table(name).map_or_else(
                || (name.to_string(), vec![], vec![]),
                |t| (name.to_string(), t.fields.clone(), t.rows.clone()),
            )
        })
        .collect()
}

/// Model, details and the TASK, TASKACTV, UDFVALUE and TASKRSRC rows match.
fn assert_same(table: &App, details: &App, what: &str) {
    for (x, y) in table
        .project
        .model
        .activities
        .iter()
        .zip(&details.project.model.activities)
    {
        assert!(x == y, "{what}: {x:?}\n differs from\n{y:?}");
    }
    assert!(
        table.project.model == details.project.model,
        "{what}: the models differ"
    );
    for (k, x) in &table.project.details {
        let y = details.project.details.get(k);
        assert!(Some(x) == y, "{what}: {x:?}\n differs from\n{y:?}");
    }
    assert!(
        table.project.details == details.project.details,
        "{what}: the activity details differ"
    );
    for (a, b) in tables(table).into_iter().zip(tables(details)) {
        assert_eq!(a.1, b.1, "{what}: {} fields differ", a.0);
        for (x, y) in a.2.iter().zip(&b.2) {
            let diff: Vec<_> =
                a.1.iter()
                    .zip(x.iter().zip(y))
                    .filter(|(_, (x, y))| x != y)
                    .collect();
            assert!(diff.is_empty(), "{what}: {} rows differ: {diff:?}", a.0);
        }
        assert!(a == b, "{what}: {} rows differ", a.0);
    }
}

/// Show one column after Activity ID.
fn show_column(app: &mut App, ctx: &egui::Context, id: &str, time: &mut f64) {
    app.grid.layout.columns = vec![
        layout::ColumnSetting::new("activity_id", 90.),
        layout::ColumnSetting::new(id, 130.),
    ];
    app.drop_frame_cache();
    frame(app, ctx, vec![], time);
    frame(app, ctx, vec![], time);
}

/// Edit a cell the way a user does: double-click, type, Enter (text, date
/// and number cells) or pick from the drop-down (choice cells).
fn table_edit(
    app: &mut App,
    ctx: &egui::Context,
    task: i64,
    column: &str,
    text: &str,
    t: &mut f64,
) {
    show_column(app, ctx, column, t);
    let i = index_of(&app.project, task);
    let pos = row_cell(app, i, column);
    double_click(app, ctx, pos, t);
    assert_eq!(
        app.edit_cell,
        Some((i, column.to_string())),
        "{column} on {task} did not open its editor: {}",
        app.status
    );
    let catalog = columns::catalog(&app.project);
    let col = columns::find(&catalog, column).unwrap().clone();
    if col.editor == columns::Editor::Choice {
        let ix = app.grid_index();
        let n = columns::choices(&app.project, &ix, &col, i)
            .iter()
            .position(|(k, _)| k == text)
            .unwrap_or_else(|| panic!("{text:?} is not offered for {column}"));
        click(app, ctx, &format!("grid_choice_{n}"), t);
    } else {
        select_all(app, ctx, t);
        type_text(app, ctx, text, t);
        key(app, ctx, Key::Enter, Default::default(), t);
    }
    assert!(app.edit_cell.is_none());
    assert!(app.error.is_none(), "{column}: {:?}", app.error);
    app.sched.sequence = None;
}

/// Type into an Activity Details field of activity `task`'s Status tab.
fn details_type(
    app: &mut App,
    ctx: &egui::Context,
    task: i64,
    field: &str,
    text: &str,
    t: &mut f64,
) {
    let i = index_of(&app.project, task);
    app.jump_to(i);
    click(app, ctx, "detail_tab_status", t);
    let action = field.replace("{i}", &i.to_string());
    click(app, ctx, &action, t);
    select_all(app, ctx, t);
    type_text(app, ctx, text, t);
    key(app, ctx, Key::Enter, Default::default(), t);
    frame(app, ctx, vec![], t);
    assert!(app.error.is_none(), "{field}: {:?}", app.error);
    app.sched.sequence = None;
}

/// Click a Status tab control of activity `task`.
fn details_click(app: &mut App, ctx: &egui::Context, task: i64, action: &str, t: &mut f64) {
    let i = index_of(&app.project, task);
    app.jump_to(i);
    click(app, ctx, "detail_tab_status", t);
    click(app, ctx, action, t);
    assert!(app.error.is_none(), "{action}: {:?}", app.error);
    app.sched.sequence = None;
}

/// The setter Activity Details calls for fields edited through pickers.
fn details_set(app: &mut App, task: i64, edit: Edit) {
    let i = index_of(&app.project, task);
    assert!(app.edit_activity(i, edit).is_ok(), "{:?}", app.error);
    app.sched.sequence = None;
}

fn hours(m: i64) -> String {
    assert_eq!(m % 60, 0);
    format!("{}h", m / 60)
}

#[test]
fn duration_percent_is_editable_in_the_status_tab_as_in_the_table() {
    let (path_a, ctx_a, mut a, mut ta) = setup("dpct-table", progress_project());
    let (path_b, ctx_b, mut b, mut tb) = setup("dpct-details", progress_project());
    prepare(&mut a);
    prepare(&mut b);
    for app in [&mut a, &mut b] {
        details_set(app, 1050, Edit::PercentType("CP_Drtn"));
    }
    table_edit(
        &mut a,
        &ctx_a,
        1050,
        "duration_percent_complete",
        "25",
        &mut ta,
    );
    details_type(
        &mut b,
        &ctx_b,
        1050,
        "detail_duration_percent",
        "25",
        &mut tb,
    );
    assert_same(&a, &b, "duration percent in the Status tab");
    let k = index_of(&a.project, 1050);
    assert_ne!(
        a.project.model.activities[k].rem,
        progress_project().model.activities[k].rem
    );
    drop((a, b));
    let _ = std::fs::remove_file(path_a);
    let _ = std::fs::remove_file(path_b);
}

#[test]
fn every_new_table_editor_matches_activity_details() {
    let (path_a, ctx_a, mut a, mut ta) = setup("cells-table", progress_project());
    let (path_b, ctx_b, mut b, mut tb) = setup("cells-details", progress_project());
    prepare(&mut a);
    prepare(&mut b);
    assert_same(&a, &b, "prepared");
    let p = a.project.clone();
    let at = |text: &str| minute(&p, text);
    let cal = p.model.calendars[p.model.project_calendar].clone();

    // Activity Status: not started to in progress, as the Started box.
    table_edit(
        &mut a,
        &ctx_a,
        1040,
        "activity_status",
        "In Progress",
        &mut ta,
    );
    details_click(&mut b, &ctx_b, 1040, "detail_started", &mut tb);
    assert_same(&a, &b, "status in progress");
    assert!(a.project.model.activities[index_of(&a.project, 1040)]
        .act_start
        .is_some());
    // Actual Start of an activity in progress.
    table_edit(
        &mut a,
        &ctx_a,
        1010,
        "actual_start",
        "2026-11-03 08:00",
        &mut ta,
    );
    details_type(
        &mut b,
        &ctx_b,
        1010,
        "detail_date_{i}-act_start",
        "2026-11-03 08:00",
        &mut tb,
    );
    assert_same(&a, &b, "actual start");
    // Activity Status to completed, as the Finished box.
    table_edit(
        &mut a,
        &ctx_a,
        1010,
        "activity_status",
        "Completed",
        &mut ta,
    );
    details_click(&mut b, &ctx_b, 1010, "detail_finished", &mut tb);
    assert_same(&a, &b, "status completed");
    // Actual Finish of a completed activity.
    table_edit(
        &mut a,
        &ctx_a,
        1010,
        "actual_finish",
        "2026-11-12 17:00",
        &mut ta,
    );
    details_type(
        &mut b,
        &ctx_b,
        1010,
        "detail_date_{i}-act_finish",
        "2026-11-12 17:00",
        &mut tb,
    );
    assert_same(&a, &b, "actual finish");
    // Start of a started activity is its actual start.
    table_edit(&mut a, &ctx_a, 1020, "start", "2026-11-06 08:00", &mut ta);
    details_type(
        &mut b,
        &ctx_b,
        1020,
        "detail_date_{i}-act_start",
        "2026-11-06 08:00",
        &mut tb,
    );
    assert_same(&a, &b, "start of a started activity");
    // Start of an activity not started: both ask the same question, and
    // Yes holds it with a Start On or After constraint.
    table_edit(&mut a, &ctx_a, 1070, "start", "2026-11-30 08:00", &mut ta);
    details_type(
        &mut b,
        &ctx_b,
        1070,
        "detail_date_{i}-act_start",
        "2026-11-30 08:00",
        &mut tb,
    );
    for (app, ctx, t) in [(&mut a, &ctx_a, &mut ta), (&mut b, &ctx_b, &mut tb)] {
        assert!(app.sched.start_hold.is_some(), "the Start asks first");
        click(app, ctx, "start_hold_yes", t);
    }
    assert_same(&a, &b, "start of an activity not started");
    let k = index_of(&a.project, 1070);
    assert_eq!(
        a.project.model.activities[k].cstr_date,
        Some(at("2026-11-30 08:00"))
    );
    // Finish of an activity not started: its Original Duration, the start
    // kept, no question.
    let i = index_of(&p, 1060);
    let span = cal.between(
        p.model.activities[i].early_start.unwrap(),
        at("2026-12-04 17:00"),
    );
    table_edit(&mut a, &ctx_a, 1060, "finish", "2026-12-04 17:00", &mut ta);
    details_type(
        &mut b,
        &ctx_b,
        1060,
        "detail_date_{i}-act_finish",
        "2026-12-04 17:00",
        &mut tb,
    );
    assert!(a.sched.start_hold.is_none() && b.sched.start_hold.is_none());
    assert_same(&a, &b, "finish of an activity not started");
    assert_eq!(
        a.project.details[&p.model.activities[i].task_id].original_duration,
        span
    );
    // Actual Duration of a completed activity moves its actual finish.
    let start = a.project.model.activities[index_of(&a.project, 1010)]
        .act_start
        .unwrap();
    table_edit(&mut a, &ctx_a, 1010, "actual_duration", "40h", &mut ta);
    let finish = date_string(p.model.origin_serial, Some(cal.add(start, 40 * 60)));
    details_type(
        &mut b,
        &ctx_b,
        1010,
        "detail_date_{i}-act_finish",
        &finish,
        &mut tb,
    );
    assert_same(&a, &b, "actual duration");
    // At Completion Duration of an activity in progress sets its remaining.
    // 1050's planned start (its default actual start) is after the data
    // date, so it has no actual duration yet: start it a week earlier.
    table_edit(
        &mut a,
        &ctx_a,
        1050,
        "actual_start",
        "2026-11-09 08:00",
        &mut ta,
    );
    details_type(
        &mut b,
        &ctx_b,
        1050,
        "detail_date_{i}-act_start",
        "2026-11-09 08:00",
        &mut tb,
    );
    assert_same(&a, &b, "actual start before the data date");
    let k = index_of(&p, 1050);
    let actual = cal.between(
        a.project.model.activities[k].act_start.unwrap(),
        p.model.project.data_date.unwrap(),
    );
    table_edit(
        &mut a,
        &ctx_a,
        1050,
        "at_completion_duration",
        &hours(actual + 64 * 60),
        &mut ta,
    );
    details_type(&mut b, &ctx_b, 1050, "detail_remaining", "64h", &mut tb);
    assert_same(&a, &b, "at completion duration");
    // Duration % Complete sets the remaining from the original.
    table_edit(
        &mut a,
        &ctx_a,
        1050,
        "duration_percent_complete",
        "25",
        &mut ta,
    );
    details_type(&mut b, &ctx_b, 1050, "detail_remaining", "36h", &mut tb);
    assert_same(&a, &b, "duration percent");
    // Percent Complete Type, then Activity % Complete as the Physical %.
    table_edit(
        &mut a,
        &ctx_a,
        1050,
        "percent_complete_type",
        "Physical",
        &mut ta,
    );
    details_set(&mut b, 1050, Edit::PercentType("CP_Phys"));
    assert_same(&a, &b, "percent type");
    table_edit(
        &mut a,
        &ctx_a,
        1050,
        "activity_percent_complete",
        "40",
        &mut ta,
    );
    details_type(
        &mut b,
        &ctx_b,
        1050,
        "detail_physical_percent",
        "40",
        &mut tb,
    );
    assert_same(&a, &b, "activity percent");
    // Suspend and Resume of an activity in progress.
    table_edit(
        &mut a,
        &ctx_a,
        1050,
        "suspend_date",
        "2026-11-17 08:00",
        &mut ta,
    );
    details_type(
        &mut b,
        &ctx_b,
        1050,
        "detail_date_{i}-suspend",
        "2026-11-17 08:00",
        &mut tb,
    );
    table_edit(
        &mut a,
        &ctx_a,
        1050,
        "resume_date",
        "2026-11-24 08:00",
        &mut ta,
    );
    details_type(
        &mut b,
        &ctx_b,
        1050,
        "detail_date_{i}-resume",
        "2026-11-24 08:00",
        &mut tb,
    );
    assert_same(&a, &b, "suspend and resume");
    // Planned dates (no Status tab field; the shared setter).
    table_edit(
        &mut a,
        &ctx_a,
        1080,
        "planned_start",
        "2026-12-01 08:00",
        &mut ta,
    );
    details_set(&mut b, 1080, Edit::PlannedStart(at("2026-12-01 08:00")));
    table_edit(
        &mut a,
        &ctx_a,
        1080,
        "planned_finish",
        "2026-12-10 17:00",
        &mut ta,
    );
    details_set(&mut b, 1080, Edit::PlannedFinish(at("2026-12-10 17:00")));
    assert_same(&a, &b, "planned dates");
    // General tab fields: Duration Type, WBS and Primary Resource.
    table_edit(
        &mut a,
        &ctx_a,
        1050,
        "duration_type",
        "Fixed Units",
        &mut ta,
    );
    details_set(&mut b, 1050, Edit::DurationType("DT_FixedQty"));
    table_edit(&mut a, &ctx_a, 1050, "wbs", "RIVERBEND.RB.3", &mut ta);
    details_set(&mut b, 1050, Edit::Wbs(30));
    table_edit(&mut a, &ctx_a, 1010, "primary_resource", "", &mut ta);
    details_set(&mut b, 1010, Edit::PrimaryResource(0));
    assert_same(&a, &b, "primary resource cleared");
    table_edit(
        &mut a,
        &ctx_a,
        1010,
        "primary_resource",
        "Pipefitter",
        &mut ta,
    );
    details_set(&mut b, 1010, Edit::PrimaryResource(5));
    assert_same(&a, &b, "general fields");
    // An activity code value, as the Codes tab assigns it.
    table_edit(&mut a, &ctx_a, 1050, "code:Area", "N", &mut ta);
    let k = index_of(&b.project, 1050);
    b.assign_code(k, 7, 71);
    assert_same(&a, &b, "activity code");
    // User defined fields by type, as the User Defined Fields tab sets them.
    table_edit(&mut a, &ctx_a, 1050, "udf:Crew", "Pipe crew B", &mut ta);
    details_set(
        &mut b,
        1050,
        Edit::Udf {
            type_id: 9,
            value: UdfValue::Text("Pipe crew B".into()),
        },
    );
    table_edit(&mut a, &ctx_a, 1050, "udf:Crew Size", "6", &mut ta);
    details_set(
        &mut b,
        1050,
        Edit::Udf {
            type_id: 10,
            value: UdfValue::Number(Some(6.)),
        },
    );
    table_edit(
        &mut a,
        &ctx_a,
        1050,
        "udf:Permit",
        "2026-12-01 17:00",
        &mut ta,
    );
    details_set(
        &mut b,
        1050,
        Edit::Udf {
            type_id: 11,
            value: UdfValue::Date(Some(at("2026-12-01 17:00"))),
        },
    );
    assert_same(&a, &b, "user defined fields");
    let doc = a.project.updated_document();
    let udf = doc.table("UDFVALUE").unwrap();
    assert_eq!(udf.rows.len(), 4, "Crew on 1010 plus three on 1050");

    // Invalid input is refused with a message and changes nothing.
    for (task, column, bad) in [
        (1040, "activity_status", "Paused"),
        (1010, "actual_start", "2026-12-30 08:00"),
        (1040, "actual_finish", "2026-11-01 08:00"),
        (1070, "start", "never"),
        (1060, "finish", "2026-11-01 08:00"),
        (1080, "planned_finish", "2026-11-20 08:00"),
        (1050, "actual_duration", "8h"),
        (1010, "at_completion_duration", "8h"),
        (1050, "duration_percent_complete", "140"),
        (1060, "duration_percent_complete", "10"),
        (1060, "suspend_date", "2026-11-20 08:00"),
        (1010, "remaining_duration", "8h"),
        (1050, "duration_type", "Hammer"),
        (1050, "percent_complete_type", "Most"),
        (1050, "wbs", "RB.9"),
        (1010, "primary_resource", "Nobody"),
        (1050, "code:Area", "Z"),
        (1050, "udf:Crew Size", "2.5"),
        (1050, "udf:Crew Size", "many"),
        (1050, "udf:Permit", "soon"),
    ] {
        let i = index_of(&a.project, task);
        let (model, details, rows) = (
            a.project.model.clone(),
            a.project.details.clone(),
            tables(&a),
        );
        a.error = None;
        assert!(!a.apply_cell(i, column, bad), "{column} accepted {bad:?}");
        assert!(a.error.is_some(), "{column} {bad:?} without a message");
        assert!(
            a.project.model == model,
            "{column} {bad:?} changed the model"
        );
        assert!(
            a.project.details == details,
            "{column} {bad:?} changed details"
        );
        assert!(tables(&a) == rows, "{column} {bad:?} changed rows");
    }
    a.error = None;

    // Exported and read back, the edited progress survives.
    let text = a.project.updated_document().text();
    let back = openxer_xer::ProjectData::from_document(
        openxer_xer::Document::parse(&text).unwrap(),
        1,
        "export".into(),
    )
    .unwrap();
    for task in [1010, 1020, 1040, 1050] {
        let (x, y) = (
            &a.project.model.activities[index_of(&a.project, task)],
            &back.model.activities[index_of(&back, task)],
        );
        // Dates as text: the reread project counts minutes from its own origin.
        let (ox, oy) = (a.project.model.origin_serial, back.model.origin_serial);
        assert_eq!(
            (
                x.status,
                date_string(ox, x.act_start),
                date_string(ox, x.act_finish),
                x.rem
            ),
            (
                y.status,
                date_string(oy, y.act_start),
                date_string(oy, y.act_finish),
                y.rem
            ),
            "{task} after export"
        );
    }
    let k = index_of(&back, 1050);
    assert_eq!(back.details[&1050].wbs, 30);
    assert_eq!(back.details[&1050].percent, 40.);
    assert_eq!(
        records::task_value(&back.document, 1050, 1, "complete_pct_type"),
        "CP_Phys"
    );
    assert_eq!(
        date_string(back.model.origin_serial, back.model.activities[k].resume),
        "2026-11-24 08:00"
    );
    drop((a, b));
    for path in [path_a, path_b] {
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn a_table_edit_is_one_undo_step_and_redo_restores_it() {
    let (path, ctx, mut app, mut time) = setup("cells-undo", progress_project());
    let i = index_of(&app.project, 1040);
    let before = app.history.counts().0;
    table_edit(
        &mut app,
        &ctx,
        1040,
        "activity_status",
        "Completed",
        &mut time,
    );
    let done = app.project.model.activities[i].clone();
    assert_eq!(done.status, StatusType::Completed);
    assert!(done.act_start.is_some() && done.act_finish.is_some());
    assert_eq!(done.rem, 0);
    assert_eq!(app.history.counts().0, before + 1);
    app.history_move(&ctx, false);
    assert_eq!(
        app.project.model.activities[i].status,
        StatusType::NotStarted
    );
    assert!(app.project.model.activities[i].act_start.is_none());
    app.history_move(&ctx, true);
    assert!(app.project.model.activities[i] == done);
    drop(app);
    let _ = std::fs::remove_file(path);
}

fn ctrl() -> egui::Modifiers {
    egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    }
}

fn shift() -> egui::Modifiers {
    egui::Modifiers {
        shift: true,
        ..Default::default()
    }
}

#[test]
fn table_keys_move_the_focus_cell_and_edit_it() {
    let (path, ctx, mut app, mut time) = setup("cells-keys", progress_project());
    app.grid.layout.columns = ["activity_id", "activity_name", "original_duration", "start"]
        .iter()
        .map(|id| layout::ColumnSetting::new(id, 120.))
        .collect();
    app.drop_frame_cache();
    frame(&mut app, &ctx, vec![], &mut time);
    let i = index_of(&app.project, 1040);
    // A click selects the row and focuses the cell.
    let pos = row_cell(&app, i, "activity_id");
    press_at(
        &mut app,
        &ctx,
        pos,
        egui::PointerButton::Primary,
        Default::default(),
        &mut time,
    );
    assert_eq!(app.selected, Some(i));
    assert_eq!(app.grid.focus_column.as_deref(), Some("activity_id"));
    assert!(app.edit_cell.is_none());
    let focus = |app: &App| app.grid.focus_column.clone().unwrap();
    key(
        &mut app,
        &ctx,
        Key::ArrowRight,
        Default::default(),
        &mut time,
    );
    assert_eq!(focus(&app), "activity_name");
    key(&mut app, &ctx, Key::Tab, Default::default(), &mut time);
    assert_eq!(focus(&app), "original_duration");
    assert!(
        ctx.memory(|m| m.focused()).is_none(),
        "Tab leaves no control focused"
    );
    key(
        &mut app,
        &ctx,
        Key::ArrowLeft,
        Default::default(),
        &mut time,
    );
    assert_eq!(focus(&app), "activity_name");
    key(&mut app, &ctx, Key::Tab, shift(), &mut time);
    assert_eq!(focus(&app), "activity_id");
    key(
        &mut app,
        &ctx,
        Key::ArrowLeft,
        Default::default(),
        &mut time,
    );
    assert_eq!(
        focus(&app),
        "activity_id",
        "the first column stops the move"
    );
    // Narrow table: moving right scrolls the focused column into view.
    key(&mut app, &ctx, Key::Tab, Default::default(), &mut time);
    key(&mut app, &ctx, Key::Tab, Default::default(), &mut time);
    key(&mut app, &ctx, Key::Tab, Default::default(), &mut time);
    assert_eq!(focus(&app), "start");
    let view = app.grid.cells.view_width;
    assert!(app.grid.scroll_x + view >= 480. - 0.5 || view >= 480.);
    // A click on the focused cell of the current row opens its editor.
    key(&mut app, &ctx, Key::Tab, shift(), &mut time);
    key(&mut app, &ctx, Key::Tab, shift(), &mut time);
    app.grid.scroll_x = 0.;
    frame(&mut app, &ctx, vec![], &mut time);
    assert_eq!(focus(&app), "activity_name");
    let pos = row_cell(&app, i, "activity_name");
    press_at(
        &mut app,
        &ctx,
        pos,
        egui::PointerButton::Primary,
        Default::default(),
        &mut time,
    );
    assert_eq!(app.edit_cell, Some((i, "activity_name".to_string())));
    // Enter commits; Escape cancels.
    select_all(&mut app, &ctx, &mut time);
    type_text(&mut app, &ctx, "Order long-lead pumps", &mut time);
    key(&mut app, &ctx, Key::Enter, Default::default(), &mut time);
    assert!(app.edit_cell.is_none());
    assert_eq!(app.project.name(i), "Order long-lead pumps");
    key(&mut app, &ctx, Key::F2, Default::default(), &mut time);
    assert!(app.edit_cell.is_some());
    type_text(&mut app, &ctx, " later", &mut time);
    key(&mut app, &ctx, Key::Escape, Default::default(), &mut time);
    assert!(app.edit_cell.is_none());
    assert_eq!(app.project.name(i), "Order long-lead pumps");
    // Tab inside the editor commits and moves on.
    key(&mut app, &ctx, Key::F2, Default::default(), &mut time);
    select_all(&mut app, &ctx, &mut time);
    type_text(&mut app, &ctx, "Order pumps", &mut time);
    key(&mut app, &ctx, Key::Tab, Default::default(), &mut time);
    assert_eq!(app.project.name(i), "Order pumps");
    assert_eq!(focus(&app), "original_duration");
    // Shift+Down selects two rows.
    let rows = app.table_activities();
    let at = rows.iter().position(|&r| r == i).unwrap();
    key(&mut app, &ctx, Key::ArrowDown, shift(), &mut time);
    assert_eq!(app.selected_in_order(), vec![i, rows[at + 1]]);
    // Ctrl+Home and Ctrl+End go to the first and last row.
    key(&mut app, &ctx, Key::End, ctrl(), &mut time);
    assert_eq!(app.selected, rows.last().copied());
    key(&mut app, &ctx, Key::Home, ctrl(), &mut time);
    assert_eq!(app.selected, rows.first().copied());
    // Ctrl+Num - collapses every band, Ctrl+Num + expands them.
    key(&mut app, &ctx, Key::Minus, ctrl(), &mut time);
    let bands: BTreeSet<String> = app
        .grid
        .bands
        .borrow()
        .iter()
        .map(|b| b.key.clone())
        .collect();
    assert!(!bands.is_empty());
    assert!(bands.iter().all(|k| app.collapsed.contains(k)));
    assert_eq!(ctx.zoom_factor(), 1., "the window does not zoom");
    key(&mut app, &ctx, Key::Plus, ctrl(), &mut time);
    assert!(app.collapsed.is_empty());
    assert_eq!(ctx.zoom_factor(), 1.);
    // Shift+F9 opens Level Resources and not Schedule.
    key(&mut app, &ctx, Key::F9, shift(), &mut time);
    assert!(app.sched.progress.level.is_some());
    assert!(!app.sched.schedule_open);
    drop(app);
    let _ = std::fs::remove_file(path);
}

fn level(column: &str) -> GroupLevel {
    GroupLevel {
        column: column.into(),
        to_level: None,
        indent: true,
        interval: String::new(),
    }
}

fn band_index(app: &App, label: &str) -> usize {
    app.grid
        .bands
        .borrow()
        .iter()
        .position(|b| b.label.contains(label))
        .unwrap_or_else(|| panic!("No band {label}"))
}

/// A point on band `index`'s row inside the table, clear of its expand box.
fn band_point(app: &App, index: usize) -> Pos2 {
    let r = rect(app, &format!("grid_band_{index}"));
    Pos2::new(r.left() + 120., r.center().y)
}

#[test]
fn a_band_click_selects_it_and_insert_adds_into_it() {
    let (path, ctx, mut app, mut time) = setup("cells-bands", progress_project());
    frame(&mut app, &ctx, vec![], &mut time);
    let count = app.project.model.activities.len();
    let b = band_index(&app, "Procurement");
    let key_of = app.grid.bands.borrow()[b].key.clone();
    let pos = band_point(&app, b);
    press_at(
        &mut app,
        &ctx,
        pos,
        egui::PointerButton::Primary,
        Default::default(),
        &mut time,
    );
    assert_eq!(
        app.grid.cells.selected_band.as_deref(),
        Some(key_of.as_str())
    );
    assert_eq!(app.selected, None, "a band row deselects the activities");
    assert!(
        !app.collapsed.contains(&key_of),
        "a click on the caption does not collapse"
    );
    // The expand box still toggles the band.
    let boxed = rect(&app, &format!("grid_band_box_{b}")).center();
    press_at(
        &mut app,
        &ctx,
        boxed,
        egui::PointerButton::Primary,
        Default::default(),
        &mut time,
    );
    assert!(app.collapsed.contains(&key_of));
    press_at(
        &mut app,
        &ctx,
        boxed,
        egui::PointerButton::Primary,
        Default::default(),
        &mut time,
    );
    assert!(!app.collapsed.contains(&key_of));
    // Insert adds the activity under the selected band, as one undo step.
    let undo = app.history.counts().0;
    key(&mut app, &ctx, Key::Insert, Default::default(), &mut time);
    frame(&mut app, &ctx, vec![], &mut time);
    assert_eq!(app.project.model.activities.len(), count + 1);
    let new = app.selected.unwrap();
    let task = app.project.model.activities[new].task_id;
    assert_eq!(app.project.details[&task].wbs, 20);
    assert!(app.grid.cells.selected_band.is_none());
    assert_eq!(app.history.counts().0, undo + 1);
    app.history_move(&ctx, false);
    assert_eq!(app.project.model.activities.len(), count);
    app.history_move(&ctx, true);
    assert_eq!(app.project.model.activities.len(), count + 1);
    // Under a code band the new activity takes the band's code value too.
    app.grid.layout.group_sort.levels = vec![level("code:Area")];
    frame(&mut app, &ctx, vec![], &mut time);
    frame(&mut app, &ctx, vec![], &mut time);
    let b = band_index(&app, "South");
    let pos = band_point(&app, b);
    press_at(
        &mut app,
        &ctx,
        pos,
        egui::PointerButton::Primary,
        Default::default(),
        &mut time,
    );
    key(&mut app, &ctx, Key::Insert, Default::default(), &mut time);
    let new = app.selected.unwrap();
    let ix = app.grid_index();
    let code = ix.code_of(&app.project, app.project.model.activities[new].task_id, 7);
    assert_eq!(code.map(|c| c.short_name).as_deref(), Some("S"));
    drop(app);
    let _ = std::fs::remove_file(path);
}

fn drag(app: &mut App, ctx: &egui::Context, from: Pos2, to: Pos2, time: &mut f64) {
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    frame(app, ctx, vec![egui::Event::PointerMoved(from)], time);
    frame(app, ctx, vec![button(from, true)], time);
    for k in 1..=5 {
        let p = from + (to - from) * (k as f32 / 5.);
        frame(app, ctx, vec![egui::Event::PointerMoved(p)], time);
    }
    frame(app, ctx, vec![button(to, false)], time);
    frame(app, ctx, vec![egui::Event::PointerGone], time);
}

#[test]
fn dragging_activities_onto_a_band_reassigns_them() {
    let (path, ctx, mut app, mut time) = setup("cells-drag", progress_project());
    frame(&mut app, &ctx, vec![], &mut time);
    let (i, j) = (index_of(&app.project, 1060), index_of(&app.project, 1070));
    app.select_activities(&[i, j]);
    frame(&mut app, &ctx, vec![], &mut time);
    let from = row_cell(&app, i, "activity_name");
    let b = band_index(&app, "Commissioning");
    let to = band_point(&app, b);
    let undo = app.history.counts().0;
    drag(&mut app, &ctx, from, to, &mut time);
    assert!(app.error.is_none(), "{:?}", app.error);
    for task in [1060, 1070] {
        assert_eq!(app.project.details[&task].wbs, 40, "{task} moved");
    }
    assert!(app.grid.cells.row_drag.is_none());
    assert_eq!(app.history.counts().0, undo + 1, "one undo step");
    app.history_move(&ctx, false);
    assert_eq!(app.project.details[&1060].wbs, 30);
    assert_eq!(app.project.details[&1070].wbs, 30);
    app.history_move(&ctx, true);
    assert_eq!(app.project.details[&1060].wbs, 40);
    // Code bands set the code; a status band refuses the drop.
    app.grid.layout.group_sort.levels = vec![level("code:Area")];
    frame(&mut app, &ctx, vec![], &mut time);
    frame(&mut app, &ctx, vec![], &mut time);
    let k = index_of(&app.project, 1000);
    app.select_activities(&[k]);
    frame(&mut app, &ctx, vec![], &mut time);
    let from = row_cell(&app, k, "activity_name");
    let to = band_point(&app, band_index(&app, "South"));
    drag(&mut app, &ctx, from, to, &mut time);
    let ix = app.grid_index();
    assert_eq!(
        ix.code_of(&app.project, 1000, 7)
            .map(|c| c.short_name)
            .as_deref(),
        Some("S")
    );
    app.grid.layout.group_sort.levels = vec![level("activity_status")];
    frame(&mut app, &ctx, vec![], &mut time);
    frame(&mut app, &ctx, vec![], &mut time);
    let model = app.project.model.clone();
    let rows = tables(&app);
    let from = row_cell(&app, k, "activity_name");
    let to = band_point(&app, band_index(&app, "Not Started"));
    drag(&mut app, &ctx, from, to, &mut time);
    assert!(app
        .error
        .as_deref()
        .unwrap()
        .contains("WBS or activity code band"));
    assert!(app.project.model == model && tables(&app) == rows);
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn date_cells_drop_down_a_calendar() {
    let (path, ctx, mut app, mut time) = setup("cells-calendar", progress_project());
    prepare(&mut app);
    app.editing.options.warn_out_of_sequence = true;
    show_column(&mut app, &ctx, "actual_start", &mut time);
    let i = index_of(&app.project, 1050);
    let pos = row_cell(&app, i, "actual_start");
    double_click(&mut app, &ctx, pos, &mut time);
    assert_eq!(app.edit_cell, Some((i, "actual_start".to_string())));
    let start = app.project.model.activities[i].act_start.unwrap();
    click(&mut app, &ctx, "grid_date_button", &mut time);
    assert!(app.grid.cells.calendar.is_some());
    assert!(
        app.edit_cell.is_some(),
        "opening the calendar keeps the editor"
    );
    let shown = date_string(app.project.model.origin_serial, Some(start));
    let day = &shown[..10];
    assert!(app
        .action_rects
        .contains_key(&format!("grid_calendar_{day}")));
    // Back a month and forward again.
    click(&mut app, &ctx, "grid_calendar_prev", &mut time);
    click(&mut app, &ctx, "grid_calendar_next", &mut time);
    // A day keeps the time of day and commits.
    let undo = app.history.counts().0;
    click(&mut app, &ctx, "grid_calendar_2026-11-10", &mut time);
    assert!(app.edit_cell.is_none());
    // An earlier actual start while the predecessor is unfinished asks
    // about the out-of-sequence progress, as Activity Details does.
    assert!(app.sched.sequence.take().is_some());
    // The accelerators wait a frame for the dialog that was up.
    frame(&mut app, &ctx, vec![], &mut time);
    assert!(app.grid.cells.calendar.is_none());
    let picked = app.project.model.activities[i].act_start.unwrap();
    assert_eq!(
        date_string(app.project.model.origin_serial, Some(picked)),
        format!("2026-11-10 {}", &shown[11..])
    );
    assert_eq!(app.history.counts().0, undo + 1);
    // Escape closes the calendar first, then the editor.
    key(&mut app, &ctx, Key::F2, Default::default(), &mut time);
    click(&mut app, &ctx, "grid_date_button", &mut time);
    key(&mut app, &ctx, Key::Escape, Default::default(), &mut time);
    assert!(app.grid.cells.calendar.is_none());
    assert!(app.edit_cell.is_some());
    key(&mut app, &ctx, Key::Escape, Default::default(), &mut time);
    assert!(app.edit_cell.is_none());
    assert_eq!(app.project.model.activities[i].act_start, Some(picked));
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn bands_total_original_duration_and_show_names_when_asked() {
    let (path, ctx, mut app, mut time) = setup("cells-totals", progress_project());
    app.grid.layout.group_sort.show_group_totals = true;
    app.grid.layout.columns = ["activity_id", "activity_name", "original_duration", "start"]
        .iter()
        .map(|id| layout::ColumnSetting::new(id, 100.))
        .collect();
    frame(&mut app, &ctx, vec![], &mut time);
    let snap = app.current_layout_table();
    let od = snap
        .columns
        .iter()
        .position(|c| c.id == "original_duration")
        .unwrap();
    let p = &app.project;
    let cal = &p.model.calendars[p.model.project_calendar];
    let members: Vec<usize> = (0..p.model.activities.len())
        .filter(|&i| p.details[&p.model.activities[i].task_id].wbs == 30)
        .collect();
    let start = members
        .iter()
        .filter_map(|&i| p.model.activities[i].early_start)
        .min()
        .unwrap();
    let finish = members
        .iter()
        .filter_map(|&i| p.model.activities[i].early_finish)
        .max()
        .unwrap();
    let expected = columns::display(p, None, &Value::Duration(cal.between(start, finish)));
    let construction = snap
        .rows
        .iter()
        .find_map(|r| match r {
            TableRow::Band { label, totals, .. } if label.contains("Construction") => {
                Some(totals[od].clone())
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(construction, expected);
    // Code and name by default; the name alone when the options ask.
    assert!(snap.rows.iter().any(|r| matches!(r,
        TableRow::Band { label, .. } if label == "RIVERBEND.RB.3  Construction")));
    app.grid.layout.group_sort.show_code = false;
    let snap = app.current_layout_table();
    assert!(snap.rows.iter().any(|r| matches!(r,
        TableRow::Band { label, .. } if label == "Construction")));
    drop(app);
    let _ = std::fs::remove_file(path);
}

/// `progress_project` with the rich starter's one priced labor assignment on
/// 1010, two labor assignments on 1050 and the period link switched on.
fn units_project() -> ProjectData {
    let mut p = progress_project();
    assert_eq!(p.model.activities[1].task_id, 1010);
    {
        let t = p.document.table_mut("TASKRSRC");
        t.set(0, "cost_per_qty", "50".into());
        t.set(0, "rsrc_type", "RT_Labor".into());
        t.set(0, "cost_qty_link_flag", "Y".into());
        for id in [2, 3] {
            t.add(&[
                ("taskrsrc_id", id.to_string()),
                ("task_id", "1050".into()),
                ("proj_id", "1".into()),
                ("rsrc_id", "5".into()),
                ("rsrc_type", "RT_Labor".into()),
                ("target_qty", "10".into()),
                ("remain_qty", "10".into()),
                ("cost_per_qty", "50".into()),
            ]);
        }
    }
    p.document
        .table_mut("PROJECT")
        .set(0, "act_this_per_link_flag", "Y".into());
    p.rebuild_assignments();
    p
}

fn rows_of(p: &ProjectData, table: &str) -> Vec<Vec<String>> {
    p.document.table(table).map_or(vec![], |t| t.rows.clone())
}

/// The figure of activity `task` in column `column` as the table shows it.
fn shown(app: &App, task: i64, column: &str) -> String {
    let catalog = columns::catalog(&app.project);
    let col = columns::find(&catalog, column).unwrap();
    let i = index_of(&app.project, task);
    let ix = app.grid_index();
    columns::display(
        &app.project,
        Some(i),
        &columns::value(&app.project, &ix, col, i),
    )
}

#[test]
fn unit_and_cost_figures_match_between_the_status_tab_and_the_table() {
    SCREEN_WIDTH.with(|w| w.set(2000.));
    let (path_a, ctx_a, mut a, mut ta) = setup("units-table", units_project());
    let (path_b, ctx_b, mut b, mut tb) = setup("units-details", units_project());
    prepare(&mut a);
    prepare(&mut b);
    assert_same(&a, &b, "prepared");
    frame(&mut a, &ctx_a, vec![], &mut ta);
    frame(&mut a, &ctx_a, vec![], &mut ta);
    let undo = a.history.counts().0;

    // One assignment: Actual, Remaining and Budgeted Labor Units.
    table_edit(&mut a, &ctx_a, 1010, "actual_labor_units", "8", &mut ta);
    details_type(&mut b, &ctx_b, 1010, "detail_units_actual", "8", &mut tb);
    assert_same(&a, &b, "actual labor units");
    assert_eq!(a.history.counts().0, undo + 1, "one undo step per edit");
    let row = a.project.assignment_rows(1010)[0];
    let view = a.project.assignment_view(row).unwrap();
    assert_eq!((view.actual_units, view.this_period_units), (8., 8.));
    assert_eq!(view.actual_regular_cost, 400.);
    assert_eq!(shown(&a, 1010, "actual_labor_units"), "8h");
    assert_eq!(shown(&a, 1010, "actual_this_period_labor_units"), "8h");
    assert_eq!(shown(&a, 1010, "actual_labor_cost"), "$400.00");

    table_edit(&mut a, &ctx_a, 1010, "remaining_labor_units", "10", &mut ta);
    details_type(
        &mut b,
        &ctx_b,
        1010,
        "detail_units_remaining",
        "10",
        &mut tb,
    );
    assert_same(&a, &b, "remaining labor units");
    table_edit(&mut a, &ctx_a, 1010, "budgeted_labor_units", "30", &mut ta);
    details_type(&mut b, &ctx_b, 1010, "detail_units_budgeted", "30", &mut tb);
    assert_same(&a, &b, "budgeted labor units");
    // The table edit is the Resources tab edit.
    let mut plain = a.project.clone();
    plain
        .edit_assignment(row, AssignmentEdit::ActualRegularUnits(12.))
        .unwrap();
    table_edit(&mut a, &ctx_a, 1010, "actual_labor_units", "12", &mut ta);
    for name in ["TASKRSRC", "TASK"] {
        assert_eq!(
            rows_of(&a.project, name),
            rows_of(&plain, name),
            "{name}: the table edit equals the Resources tab edit"
        );
    }
    // Details copy catches up the same way.
    details_type(&mut b, &ctx_b, 1010, "detail_units_actual", "12", &mut tb);
    assert_same(&a, &b, "actual labor units again");

    // Actual This Period, linked to the actual to date.
    table_edit(
        &mut a,
        &ctx_a,
        1010,
        "actual_this_period_labor_units",
        "5",
        &mut ta,
    );
    assert_eq!(shown(&a, 1010, "actual_labor_units"), "5h");
    assert_eq!(shown(&a, 1010, "actual_this_period_labor_units"), "5h");

    // Costs: the Labor Cost mode of the Status tab and the cost columns.
    b.detail.units_mode = UnitsMode::LaborCost;
    details_type(&mut b, &ctx_b, 1010, "detail_units_actual", "$300", &mut tb);
    assert_eq!(shown(&b, 1010, "actual_labor_cost"), "$300.00");
    table_edit(&mut a, &ctx_a, 1010, "actual_labor_cost", "300", &mut ta);
    table_edit(&mut a, &ctx_a, 1010, "actual_labor_units", "12", &mut ta);
    table_edit(&mut a, &ctx_a, 1010, "actual_labor_cost", "300", &mut ta);
    assert_eq!(shown(&a, 1010, "actual_labor_cost"), "$300.00");
    table_edit(&mut a, &ctx_a, 1010, "budgeted_labor_cost", "1000", &mut ta);
    assert_eq!(shown(&a, 1010, "budgeted_labor_units"), "20h");

    // Undo takes the edit back, redo restores it.
    let rows_now = rows_of(&a.project, "TASKRSRC");
    frame(&mut a, &ctx_a, vec![], &mut ta);
    a.history_move(&ctx_a, false);
    assert_ne!(rows_of(&a.project, "TASKRSRC"), rows_now);
    a.history_move(&ctx_a, true);
    assert_eq!(rows_of(&a.project, "TASKRSRC"), rows_now);

    // No assignment: the values land in the activity's own TASK quantities.
    let i = index_of(&a.project, 1040);
    table_edit(&mut a, &ctx_a, 1040, "budgeted_labor_units", "100", &mut ta);
    assert_eq!(
        records::task_value(&a.project.document, 1040, 1, "target_work_qty"),
        "100"
    );
    assert_eq!(
        records::task_value(&a.project.document, 1040, 1, "remain_work_qty"),
        "100"
    );
    assert_eq!(shown(&a, 1040, "remaining_labor_units"), "100h");
    table_edit(
        &mut a,
        &ctx_a,
        1040,
        "budgeted_nonlabor_units",
        "6",
        &mut ta,
    );
    assert_eq!(
        records::task_value(&a.project.document, 1040, 1, "target_equip_qty"),
        "6"
    );
    // Costs have nowhere to go without an assignment.
    let catalog = columns::catalog(&a.project);
    let cost = columns::find(&catalog, "budgeted_labor_cost").unwrap();
    let why = a.cell_editable(i, cost).unwrap_err();
    assert!(why.contains("Assign a labor resource"), "{why}");
    // A started activity of this kind takes actuals; F9, save and reread keep them.
    table_edit(
        &mut a,
        &ctx_a,
        1040,
        "activity_status",
        "In Progress",
        &mut ta,
    );
    table_edit(&mut a, &ctx_a, 1040, "actual_labor_units", "25", &mut ta);
    assert_eq!(
        records::task_value(&a.project.document, 1040, 1, "act_work_qty"),
        "25"
    );
    schedule_model(&mut a.project.model, &a.project.resources).unwrap();
    let text = a.project.updated_document().text();
    let back = openxer_xer::ProjectData::from_document(
        openxer_xer::Document::parse(&text).unwrap(),
        1,
        "export".into(),
    )
    .unwrap();
    for column in [
        "target_work_qty",
        "act_work_qty",
        "remain_work_qty",
        "act_this_per_work_qty",
    ] {
        assert_eq!(
            records::task_value(&back.document, 1040, 1, column),
            records::task_value(&a.project.document, 1040, 1, column),
            "{column}"
        );
    }
    let ix = columns::Index::new((false, 0.));
    let catalog = columns::catalog(&back);
    let col = columns::find(&catalog, "actual_labor_units").unwrap();
    let k = index_of(&back, 1040);
    assert_eq!(columns::value(&back, &ix, col, k), Value::Units(25.));

    // Several assignments of the type: read-only with a reason.
    let catalog = columns::catalog(&a.project);
    let actual = columns::find(&catalog, "actual_labor_units").unwrap();
    let why = a
        .cell_editable(index_of(&a.project, 1050), actual)
        .unwrap_err();
    assert!(why.contains("several labor resource assignments"), "{why}");
    let why = b.figure_lock(
        index_of(&b.project, 1050),
        Measure::LaborUnits,
        Some(Phase::Actual),
    );
    assert!(why.unwrap_err().contains("Resources tab"));

    for path in [path_a, path_b] {
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn bad_unit_entries_are_refused_and_change_nothing() {
    let (path, ctx, mut app, mut time) = setup("units-bad", units_project());
    prepare(&mut app);
    let _ = (&ctx, &mut time);
    let before = (
        rows_of(&app.project, "TASK"),
        rows_of(&app.project, "TASKRSRC"),
        app.project.model.activities.clone(),
    );
    let undo = app.history.counts().0;
    let i = index_of(&app.project, 1010);
    let not_started = index_of(&app.project, 1030);
    for (activity, column, text, reason) in [
        (i, "actual_labor_units", "-4", "zero or more"),
        (i, "remaining_labor_units", "abc", "not a number"),
        (i, "budgeted_labor_cost", "-1", "zero or more"),
        (not_started, "actual_labor_units", "5", "started activity"),
        (
            index_of(&app.project, 1040),
            "actual_labor_units",
            "5",
            "started activity",
        ),
        (i, "at_completion_labor_units", "5", "not editable"),
        (i, "actual_total_cost", "5", "not editable"),
        (
            index_of(&app.project, 1020),
            "remaining_labor_units",
            "3",
            "no remaining",
        ),
    ] {
        let e = app.try_apply_cell(activity, column, text).unwrap_err();
        assert!(e.contains(reason), "{column} {text}: {e}");
    }
    assert!(before.0 == rows_of(&app.project, "TASK"));
    assert!(before.1 == rows_of(&app.project, "TASKRSRC"));
    assert!(before.2 == app.project.model.activities);
    assert_eq!(
        app.history.counts().0,
        undo,
        "a refused edit is no undo step"
    );
    // Typing the shown text back is not a change either.
    let shown_text = shown(&app, 1010, "budgeted_labor_units");
    assert_eq!(
        app.try_apply_cell(i, "budgeted_labor_units", &shown_text),
        Ok(false)
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn paste_and_fill_down_reach_unit_and_cost_columns_through_the_setters() {
    let (path, ctx, mut app, mut time) = setup("units-paste", units_project());
    prepare(&mut app);
    let i = index_of(&app.project, 1010);
    let j = index_of(&app.project, 1040);
    // Paste one cell: Remaining Labor Units of the current activity.
    app.grid.layout.columns = vec![
        layout::ColumnSetting::new("activity_id", 90.),
        layout::ColumnSetting::new("remaining_labor_units", 130.),
        layout::ColumnSetting::new("budgeted_labor_units", 130.),
    ];
    app.drop_frame_cache();
    frame(&mut app, &ctx, vec![], &mut time);
    app.selected = Some(i);
    app.grid.focus_column = Some("remaining_labor_units".into());
    app.paste_text("7", time);
    assert_eq!(shown(&app, 1010, "remaining_labor_units"), "7h");
    // The same text pasted into an activity without assignments goes to TASK.
    app.selected = Some(j);
    app.paste_text("7", time);
    assert_eq!(
        records::task_value(&app.project.document, 1040, 1, "remain_work_qty"),
        "7"
    );
    // Fill Down copies the first selected row's text over the others.
    app.editing.selection = [1010, 1040].into_iter().collect();
    app.selected = Some(i);
    app.grid.focus_column = Some("budgeted_labor_units".into());
    app.fill_down(time);
    assert_eq!(
        shown(&app, 1040, "budgeted_labor_units"),
        shown(&app, 1010, "budgeted_labor_units")
    );
    assert!(app.history.counts().0 > 0);
    let _ = (&ctx, &mut time);
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn indicator_fields_take_one_of_four_choices_in_the_table_and_the_details() {
    let mut project = progress_project();
    project.document.table_mut("UDFTYPE").add(&[
        ("udf_type_id", "12".into()),
        ("table_name", "TASK".into()),
        ("udf_type_name", "user_field_12".into()),
        ("udf_type_label", "Status Flag".into()),
        ("logical_data_type", "FT_STATICTYPE".into()),
    ]);
    let (path, ctx, mut app, mut time) = setup("indicator", project);
    let catalog = columns::catalog(&app.project);
    let column = columns::find(&catalog, "udf:Status Flag").unwrap();
    assert_eq!(column.editor, columns::Editor::Choice);
    let i = index_of(&app.project, 1010);
    let ix = app.grid_index();
    let offered: Vec<String> = columns::choices(&app.project, &ix, column, i)
        .into_iter()
        .map(|(k, _)| k)
        .collect();
    assert_eq!(offered, ["", "Red", "Yellow", "Green", "Blue"]);
    table_edit(&mut app, &ctx, 1010, "udf:Status Flag", "Yellow", &mut time);
    let udf = |app: &App| {
        let t = app.project.document.table("UDFVALUE").unwrap();
        t.rows
            .iter()
            .filter(|r| t.value(r, "udf_type_id") == "12")
            .map(|r| {
                (
                    t.value(r, "fk_id").to_string(),
                    t.value(r, "udf_text").to_string(),
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(udf(&app), [("1010".to_string(), "Yellow".to_string())]);
    // The details setter checks the name too; the case is normalized.
    let edit = |text: &str| Edit::Udf {
        type_id: 12,
        value: UdfValue::Text(text.into()),
    };
    assert_eq!(app.edit_activity(i, edit("green")), Ok(true));
    assert_eq!(udf(&app), [("1010".to_string(), "Green".to_string())]);
    assert!(app.edit_activity(i, edit("Purple")).is_err());
    assert_eq!(udf(&app), [("1010".to_string(), "Green".to_string())]);
    assert_eq!(app.edit_activity(i, edit("")), Ok(true));
    assert!(udf(&app).iter().all(|(_, t)| t.is_empty()));
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn store_period_performance_resets_actual_this_period() {
    use crate::enterprise::ops;
    let (path, ctx, mut app, mut time) = setup("units-period", units_project());
    prepare(&mut app);
    let _ = (&ctx, &mut time);
    let i = index_of(&app.project, 1010);
    assert_eq!(app.try_apply_cell(i, "actual_labor_units", "8"), Ok(true));
    assert_eq!(shown(&app, 1010, "actual_this_period_labor_units"), "8h");
    assert_eq!(
        shown(&app, 1010, "actual_this_period_labor_cost"),
        "$400.00"
    );
    let doc = &mut app.project.document;
    let cal = ops::add_fin_calendar(doc).unwrap();
    let day = |m, d| chrono::NaiveDate::from_ymd_opt(2026, m, d).unwrap();
    let ids = ops::batch_create(
        doc,
        cal,
        day(11, 1),
        day(11, 30),
        crate::enterprise::ops::Interval::Monthly(30),
    )
    .unwrap();
    app.sched.sequence = None;
    app.store_period_now(cal, ids[0], 1.);
    assert_eq!(shown(&app, 1010, "actual_this_period_labor_units"), "0h");
    assert_eq!(shown(&app, 1010, "actual_this_period_labor_cost"), "$0.00");
    assert_eq!(
        shown(&app, 1010, "actual_labor_units"),
        "8h",
        "actuals to date stay"
    );
    let fin = app.project.document.table("TASKFIN").unwrap();
    assert!(fin.rows.iter().any(|r| {
        fin.value(r, "task_id") == "1010" && records::float(fin.value(r, "act_work_qty")) == 8.
    }));
    // The period figure starts again from zero.
    assert_eq!(app.try_apply_cell(i, "actual_labor_units", "11"), Ok(true));
    assert_eq!(shown(&app, 1010, "actual_this_period_labor_units"), "3h");
    let _ = std::fs::remove_file(path);
}

#[test]
fn actual_this_period_columns_match_the_stored_data_of_a_corpus_project() {
    let dir = crate::test_corpus::dir();
    let mut checked = 0;
    let mut names: Vec<String> = vec!["5932.xer".into(), "6074.xer".into()];
    // A third file's name identifies its job, so it comes from the private
    // fixture `corpus-names.txt` (`leveled-selected <file>`); without it the
    // two files above still run and a SKIPPED line says what did not.
    if let Some(listed) = openxer_engine::fixture::corpus_names("leveled-selected") {
        names.extend(listed.into_iter().map(|(file, _)| file));
    }
    for name in &names {
        let name = name.trim_end_matches(".xer");
        let Ok(doc) = openxer_xer::Document::read(&dir.join(format!("{name}.xer"))) else {
            continue;
        };
        let key = doc.projects()[0].0;
        let p = ProjectData::from_document(doc, key, name.into()).unwrap();
        let catalog = columns::catalog(&p);
        let ix = columns::Index::new((false, 0.));
        let col = columns::find(&catalog, "actual_this_period_labor_units").unwrap();
        let tasks = p.document.table("TASK").unwrap();
        let mut nonzero = 0;
        for (i, a) in p.model.activities.iter().enumerate() {
            let stored = tasks
                .rows
                .iter()
                .find(|r| tasks.value(r, "task_id") == a.task_id.to_string())
                .map_or(0., |r| {
                    records::float(tasks.value(r, "act_this_per_work_qty"))
                });
            let Value::Units(shown) = columns::value(&p, &ix, col, i) else {
                panic!("units expected");
            };
            assert!(
                (shown - stored).abs() < 1e-6,
                "{name} {}: {shown} vs {stored}",
                a.task_code
            );
            nonzero += usize::from(stored != 0.);
        }
        assert!(nonzero > 0, "{name} has actual this period data");
        checked += 1;
    }
    if checked == 0 {
        eprintln!("SKIPPED: corpus not found in {}", dir.display());
    }
}

/// Writes `artifacts/h1/*.png` for visual review: a selected band, the
/// status choice editor, the date editor with its calendar and the band
/// totals. Run on demand:
/// `cargo.exe test -p openxer-desktop render_table_editing -- --ignored`.
#[test]
#[ignore = "writes review renders to artifacts/h1; run on demand"]
fn render_table_editing_to_png() {
    use crate::headless_render::SoftwarePainter;
    let output = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/h1");
    std::fs::create_dir_all(&output).unwrap();
    let (path, ctx, mut app, mut time) = setup_with("cells-render", progress_project(), false);
    prepare(&mut app);
    let mut painter = SoftwarePainter::default();
    painter.screen = Some(Vec2::new(1500., 900.));
    app.grid.layout.columns = [
        ("activity_id", 90.),
        ("activity_name", 200.),
        ("original_duration", 60.),
        ("activity_status", 90.),
        ("actual_start", 110.),
        ("finish", 110.),
    ]
    .into_iter()
    .map(|(id, w)| layout::ColumnSetting::new(id, w))
    .collect();
    let mut shot = |app: &mut App, name: &str| {
        for _ in 0..3 {
            let _ = painter.frame(app, &ctx, &mut time, vec![], false);
        }
        let (image, _) = painter.frame(app, &ctx, &mut time, vec![], true).unwrap();
        image.save(output.join(format!("{name}.png"))).unwrap();
    };
    shot(&mut app, "table");
    let b = band_index(&app, "Construction");
    let key = app.grid.bands.borrow()[b].key.clone();
    app.select_band(&key);
    shot(&mut app, "band-selected");
    let i = index_of(&app.project, 1050);
    app.click_row(i, Default::default());
    let catalog = columns::catalog(&app.project);
    let status = columns::find(&catalog, "activity_status").unwrap().clone();
    app.begin_edit(i, &status);
    shot(&mut app, "edit-status");
    app.edit_cell = None;
    let start = columns::find(&catalog, "actual_start").unwrap().clone();
    app.begin_edit(i, &start);
    shot(&mut app, "edit-date");
    app.grid.cells.calendar = Some(chrono::NaiveDate::from_ymd_opt(2026, 11, 1).unwrap());
    shot(&mut app, "edit-date-calendar");
    drop(app);
    let _ = std::fs::remove_file(path);
}

/// Writes `artifacts/m1/*.png` for visual review: the table with unit,
/// period and indicator columns and the Status tab's Units section. Run on
/// demand: `cargo.exe test -p openxer-desktop render_units_progress -- --ignored`.
#[test]
#[ignore = "writes review renders to artifacts/m1; run on demand"]
fn render_units_progress_to_png() {
    use crate::headless_render::SoftwarePainter;
    let output = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/m1");
    std::fs::create_dir_all(&output).unwrap();
    let mut project = units_project();
    project.document.table_mut("UDFTYPE").add(&[
        ("udf_type_id", "12".into()),
        ("table_name", "TASK".into()),
        ("udf_type_name", "user_field_12".into()),
        ("udf_type_label", "Status Flag".into()),
        ("logical_data_type", "FT_STATICTYPE".into()),
    ]);
    let (path, ctx, mut app, mut time) = setup_with("units-render", project, false);
    prepare(&mut app);
    for (n, task) in [1010, 1020, 1040, 1050].into_iter().enumerate() {
        let i = index_of(&app.project, task);
        let flag = ["Red", "Yellow", "Green", "Blue"][n];
        app.edit_activity(
            i,
            Edit::Udf {
                type_id: 12,
                value: UdfValue::Text(flag.into()),
            },
        )
        .unwrap();
    }
    let i = index_of(&app.project, 1010);
    app.try_apply_cell(i, "actual_labor_units", "8").unwrap();
    let mut painter = SoftwarePainter::default();
    painter.screen = Some(Vec2::new(1920., 1000.));
    app.grid.layout.columns = [
        ("activity_id", 90.),
        ("activity_name", 200.),
        ("activity_status", 90.),
        ("budgeted_labor_units", 110.),
        ("actual_labor_units", 110.),
        ("remaining_labor_units", 120.),
        ("actual_this_period_labor_units", 170.),
        ("actual_labor_cost", 110.),
        ("udf:Status Flag", 110.),
    ]
    .into_iter()
    .map(|(id, w)| layout::ColumnSetting::new(id, w))
    .collect();
    let mut shot = |app: &mut App, name: &str| {
        for _ in 0..3 {
            let _ = painter.frame(app, &ctx, &mut time, vec![], false);
        }
        let (image, _) = painter.frame(app, &ctx, &mut time, vec![], true).unwrap();
        image.save(output.join(format!("{name}.png"))).unwrap();
    };
    app.jump_to(i);
    shot(&mut app, "table");
    let catalog = columns::catalog(&app.project);
    let flag = columns::find(&catalog, "udf:Status Flag").unwrap().clone();
    app.begin_edit(i, &flag);
    shot(&mut app, "indicator-choice");
    app.edit_cell = None;
    app.detail.tab = DetailTab::Status;
    shot(&mut app, "status-tab");
    let j = index_of(&app.project, 1050);
    app.jump_to(j);
    shot(&mut app, "status-tab-several");
    drop(app);
    let _ = std::fs::remove_file(path);
}
