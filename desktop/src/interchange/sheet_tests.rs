//! Spreadsheet import through the activity setters: a one-cell import equals
//! the same value typed into the table on a second copy, cells the table
//! refuses are refused in the preview with its message, an import that starts
//! an activity out of sequence lists it, and the whole import is one undo step.
use super::*;
use crate::details::setters::Edit;
use openxer_interchange::xlsx_import::{Cell, RowKind, Sheet, Workbook};

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "openxer-sheet-{}-{name}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn minute(p: &ProjectData, text: &str) -> i64 {
    openxer_xer::parse_relative(p.model.origin_serial, text)
        .unwrap()
        .unwrap()
}

fn index_of(p: &ProjectData, task: i64) -> usize {
    p.model
        .activities
        .iter()
        .position(|a| a.task_id == task)
        .unwrap()
}

/// The starter with a code type, UDFs, a labor resource on 1050, the data
/// date two weeks in and a schedule.
fn project() -> ProjectData {
    let mut p = openxer_xer::starter();
    let doc = &mut p.document;
    doc.table_mut("ACTVTYPE").add(&[
        ("actv_code_type_id", "7".into()),
        ("actv_code_type", "Area".into()),
        ("actv_code_type_scope", "AS_Project".into()),
        ("proj_id", "1".into()),
        ("seq_num", "1".into()),
    ]);
    for (id, short, seq) in [(71, "NORTH", 1), (72, "SOUTH", 2)] {
        doc.table_mut("ACTVCODE").add(&[
            ("actv_code_id", id.to_string()),
            ("actv_code_type_id", "7".into()),
            ("short_name", short.into()),
            ("actv_code_name", short.into()),
            ("seq_num", seq.to_string()),
        ]);
    }
    for (id, label, kind) in [(10, "Crew Size", "FT_INT"), (11, "Permit", "FT_END_DATE")] {
        doc.table_mut("UDFTYPE").add(&[
            ("udf_type_id", id.to_string()),
            ("table_name", "TASK".into()),
            ("udf_type_name", format!("user_field_{id}")),
            ("udf_type_label", label.into()),
            ("logical_data_type", kind.into()),
        ]);
    }
    doc.table_mut("RSRC").add(&[
        ("rsrc_id", "10".into()),
        ("rsrc_short_name", "PIPE".into()),
        ("rsrc_name", "Pipe crew".into()),
        ("rsrc_type", "RT_Labor".into()),
        ("def_qty_per_hr", "1".into()),
    ]);
    p.model.project.data_date = Some(minute(&p, "2026-11-16 08:00"));
    p.add_assignment_with(1050, Some(10), None, false).unwrap();
    schedule_model(&mut p.model, &p.resources).unwrap();
    p
}

fn app_in(dir: &Path, file: &str, base: &ProjectData) -> (App, egui::Context) {
    let store = Store::open(dir.join(file)).unwrap();
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.);
    let mut app = App::new(&ctx, store, base.clone());
    app.save();
    // 1010 and 1050 in progress, 1020 done, as the table tests start.
    for (task, status) in [
        (1010, StatusType::InProgress),
        (1020, StatusType::Completed),
        (1050, StatusType::InProgress),
    ] {
        let i = index_of(&app.project, task);
        assert_eq!(app.edit_activity(i, Edit::Status(status)), Ok(true));
    }
    let i = index_of(&app.project, 1060);
    let edit = Edit::Constraint {
        secondary: false,
        kind: CstrType::StartOnAfter,
    };
    assert!(app.edit_activity(i, edit).is_ok());
    app.sched.sequence = None;
    app.history
        .record(&app.project, app.records_revision, None, 0.);
    (app, ctx)
}

/// A one-row workbook: Activity ID plus one more column.
fn sheet(code: &str, header: &str, cell: Cell) -> Workbook {
    Workbook {
        sheets: vec![Sheet {
            name: "Updates".into(),
            rows: vec![
                vec![Cell::Text("Activity ID".into()), Cell::Text(header.into())],
                vec![Cell::Text(code.into()), cell],
            ],
        }],
    }
}

/// Plan `wb` in the import dialog's state; returns the plan.
fn plan_sheet(app: &mut App, wb: Workbook) -> xi::ImportPlan {
    app.open_sheet_import(String::new());
    let lookup = Lookup::new(&app.project);
    let detected = xi::detect(&wb, &lookup).unwrap();
    let mapping = xi::auto_map(&detected, &lookup);
    {
        let w = app.interchange.sheet.as_mut().unwrap();
        w.wb = Some(wb);
        w.detected = Some(detected);
        w.mapping = Some(mapping);
        w.step = 1;
    }
    app.sheet_plan();
    app.interchange
        .sheet
        .as_ref()
        .unwrap()
        .plan
        .clone()
        .unwrap()
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

fn same_state(a: &App, b: &App) -> bool {
    a.project.model == b.project.model
        && a.project.details == b.project.details
        && tables(a) == tables(b)
}

fn assert_same(a: &App, b: &App, what: &str) {
    for (x, y) in a
        .project
        .model
        .activities
        .iter()
        .zip(&b.project.model.activities)
    {
        assert!(x == y, "{what}: {x:?}\n differs from\n{y:?}");
    }
    assert!(a.project.model == b.project.model, "{what}: models differ");
    assert!(
        a.project.details == b.project.details,
        "{what}: details differ"
    );
    for (x, y) in tables(a).into_iter().zip(tables(b)) {
        assert_eq!(x.1, y.1, "{what}: {} fields differ", x.0);
        for (r, s) in x.2.iter().zip(&y.2) {
            let diff: Vec<_> =
                x.1.iter()
                    .zip(r.iter().zip(s))
                    .filter(|(_, (m, n))| m != n)
                    .collect();
            assert!(diff.is_empty(), "{what}: {} rows differ: {diff:?}", x.0);
        }
        assert!(x == y, "{what}: {} row counts differ", x.0);
    }
}

struct Case {
    what: &'static str,
    task: i64,
    header: &'static str,
    cell: Cell,
    column: &'static str,
    typed: String,
}

fn serial(p: &ProjectData, text: &str) -> f64 {
    (minute(p, text) + p.model.origin_serial * DAY) as f64 / DAY as f64
}

fn cases(p: &ProjectData) -> Vec<Case> {
    let date = |what, task, header, column, text: &str| Case {
        what,
        task,
        header,
        cell: Cell::Date(serial(p, text)),
        column,
        typed: text.to_string(),
    };
    vec![
        date(
            "actual start",
            1010,
            "Actual Start",
            "actual_start",
            "2026-11-03 08:00",
        ),
        date(
            "actual finish",
            1010,
            "Actual Finish",
            "actual_finish",
            "2026-11-18 17:00",
        ),
        date(
            "expected finish",
            1010,
            "Expected Finish",
            "expected_finish",
            "2026-11-27 17:00",
        ),
        date(
            "suspend date",
            1010,
            "Suspend Date",
            "suspend_date",
            "2026-11-17 12:00",
        ),
        date(
            "resume date",
            1010,
            "Resume Date",
            "resume_date",
            "2026-11-18 08:00",
        ),
        date(
            "constraint date",
            1060,
            "Primary Constraint Date",
            "primary_constraint_date",
            "2026-12-01 08:00",
        ),
        Case {
            what: "primary constraint",
            task: 1040,
            header: "Primary Constraint",
            cell: Cell::Text("Finish On or Before".into()),
            column: "primary_constraint",
            typed: "Finish On or Before".into(),
        },
        Case {
            what: "remaining duration",
            task: 1010,
            header: "Remaining Duration (h)",
            cell: Cell::Number(32.),
            column: "remaining_duration",
            typed: "32h".into(),
        },
        Case {
            what: "original duration",
            task: 1040,
            header: "Original Duration (h)",
            cell: Cell::Number(24.),
            column: "original_duration",
            typed: "24h".into(),
        },
        Case {
            what: "actual duration",
            task: 1020,
            header: "Actual Duration (h)",
            cell: Cell::Number(16.),
            column: "actual_duration",
            typed: "16h".into(),
        },
        Case {
            what: "physical percent",
            task: 1010,
            header: "Physical % Complete",
            cell: Cell::Number(35.),
            column: "physical_percent_complete",
            typed: "35".into(),
        },
        Case {
            what: "activity percent",
            task: 1010,
            header: "Activity % Complete",
            cell: Cell::Number(40.),
            column: "activity_percent_complete",
            typed: "40".into(),
        },
        Case {
            what: "duration percent",
            task: 1010,
            header: "Duration % Complete",
            cell: Cell::Number(25.),
            column: "duration_percent_complete",
            typed: "25".into(),
        },
        Case {
            what: "actual labor units",
            task: 1050,
            header: "Actual Labor Units",
            cell: Cell::Number(6.),
            column: "actual_labor_units",
            typed: "6".into(),
        },
        Case {
            what: "remaining labor units",
            task: 1050,
            header: "Remaining Labor Units",
            cell: Cell::Number(10.),
            column: "remaining_labor_units",
            typed: "10".into(),
        },
        Case {
            what: "wbs",
            task: 1040,
            header: "WBS Code",
            cell: Cell::Text("RB.4".into()),
            column: "wbs",
            typed: "RB.4".into(),
        },
        Case {
            what: "activity code",
            task: 1040,
            header: "Area",
            cell: Cell::Text("SOUTH".into()),
            column: "code:Area",
            typed: "SOUTH".into(),
        },
        Case {
            what: "number udf",
            task: 1040,
            header: "Crew Size",
            cell: Cell::Number(6.),
            column: "udf:Crew Size",
            typed: "6".into(),
        },
    ]
}

#[test]
fn a_one_cell_import_equals_typing_the_cell_in_the_table() {
    let dir = temp("equal");
    let probe = project();
    let (fresh, _) = app_in(&dir, "c.db", &probe);
    let (mut accepted, mut refused) = (vec![], vec![]);
    for case in cases(&probe) {
        let (mut imported, _) = app_in(&dir, "a.db", &probe);
        let (mut typed, _) = app_in(&dir, "b.db", &probe);
        let code = format!("RB-{}", case.task);
        let i = index_of(&typed.project, case.task);
        // The table's column id for a code or UDF column is its own id.
        let result = typed.try_apply_cell(i, case.column, &case.typed);
        let plan = plan_sheet(&mut imported, sheet(&code, case.header, case.cell.clone()));
        let row = &plan.rows[0];
        match result {
            Ok(_) => {
                assert!(
                    matches!(row.kind, RowKind::Changed | RowKind::Unchanged),
                    "{}: the table accepted it, the preview says {:?} {:?}",
                    case.what,
                    row.kind,
                    row.messages
                );
                imported.sheet_apply(1.0);
                assert_eq!(row.kind, RowKind::Changed, "{}", case.what);
                assert!(
                    !same_state(&imported, &fresh),
                    "{}: the import changed nothing",
                    case.what
                );
                accepted.push(case.what);
                assert!(imported.interchange.sheet.is_none() || row.kind == RowKind::Unchanged);
                typed.sched.sequence = None;
                imported.sched.sequence = None;
                assert_same(&imported, &typed, case.what);
            }
            Err(e) => {
                refused.push(case.what);
                assert_eq!(row.kind, RowKind::Error, "{}: {e}", case.what);
                assert!(
                    row.messages.iter().any(|m| m.contains(&e)),
                    "{}: {:?} should carry {e:?}",
                    case.what,
                    row.messages
                );
            }
        }
    }
    assert_eq!(refused, Vec::<&str>::new(), "every case starts valid");
    assert_eq!(accepted.len(), 18);
}

#[test]
fn cells_the_table_refuses_are_refused_in_the_preview_with_its_message() {
    let dir = temp("refuse");
    let probe = project();
    let refused = [
        (
            1020,
            "Remaining Duration (h)",
            Cell::Number(8.),
            "remaining_duration",
            "8h",
        ),
        (
            1040,
            "Duration % Complete",
            Cell::Number(50.),
            "duration_percent_complete",
            "50",
        ),
        (
            1010,
            "Actual Duration (h)",
            Cell::Number(8.),
            "actual_duration",
            "8h",
        ),
        (
            1040,
            "Suspend Date",
            Cell::Date(serial(&probe, "2026-11-17 12:00")),
            "suspend_date",
            "2026-11-17 12:00",
        ),
        (
            1020,
            "Expected Finish",
            Cell::Date(serial(&probe, "2026-11-20 17:00")),
            "expected_finish",
            "2026-11-20 17:00",
        ),
        (
            1040,
            "Actual Labor Units",
            Cell::Number(4.),
            "actual_labor_units",
            "4",
        ),
    ];
    for (task, header, cell, column, typed) in refused {
        let (mut imported, _) = app_in(&dir, "a.db", &probe);
        let (mut table, _) = app_in(&dir, "b.db", &probe);
        let i = index_of(&table.project, task);
        let message = table.try_apply_cell(i, column, typed).expect_err(&format!(
            "{header} on {task} should be refused by the table"
        ));
        let before = (
            imported.records_revision,
            imported.dirty,
            imported.status.clone(),
        );
        let plan = plan_sheet(&mut imported, sheet(&format!("RB-{task}"), header, cell));
        let row = &plan.rows[0];
        assert_eq!(row.kind, RowKind::Error, "{header}: {:?}", row.messages);
        assert!(
            row.messages.iter().any(|m| m.contains(&message)),
            "{header}: {:?} should carry {message:?}",
            row.messages
        );
        assert!(
            !plan.has_changes(),
            "a refused cell leaves nothing to apply"
        );
        // The preview left the application as it was.
        assert_eq!(
            before,
            (
                imported.records_revision,
                imported.dirty,
                imported.status.clone()
            )
        );
        assert!(imported.sched.sequence.is_none());
        assert!(imported
            .history
            .is_current(&imported.project, imported.records_revision));
        assert!(same_state(&imported, &table), "{header}: nothing changed");
    }
}

#[test]
fn a_row_can_start_finish_and_set_the_remaining_duration_at_once() {
    let dir = temp("row");
    let probe = project();
    let (mut app, _) = app_in(&dir, "a.db", &probe);
    let wb = Workbook {
        sheets: vec![Sheet {
            name: "Updates".into(),
            rows: vec![
                [
                    "Activity ID",
                    "Actual Start",
                    "Actual Finish",
                    "Remaining Duration (h)",
                ]
                .map(|h| Cell::Text(h.into()))
                .to_vec(),
                vec![
                    Cell::Text("RB-1040".into()),
                    Cell::Date(serial(&probe, "2026-11-16 08:00")),
                    Cell::Date(serial(&probe, "2026-11-17 17:00")),
                    Cell::Number(0.),
                ],
            ],
        }],
    };
    let plan = plan_sheet(&mut app, wb);
    assert_eq!(plan.rows[0].kind, RowKind::Changed, "{:?}", plan.rows[0]);
    app.sheet_apply(1.0);
    let a = &app.project.model.activities[index_of(&app.project, 1040)];
    assert_eq!(a.status, StatusType::Completed);
    assert_eq!(a.act_start, Some(minute(&app.project, "2026-11-16 08:00")));
    assert_eq!(a.act_finish, Some(minute(&app.project, "2026-11-17 17:00")));
    assert_eq!(a.rem, 0);
}

/// An activity not started whose predecessor has not finished.
fn behind_a_predecessor(p: &ProjectData) -> (i64, i64) {
    let m = &p.model;
    m.links
        .iter()
        .find(|l| {
            let (a, b) = (&m.activities[l.pred], &m.activities[l.succ]);
            a.status == StatusType::NotStarted
                && b.status == StatusType::NotStarted
                && l.link_type == openxer_engine::model::LinkType::FS
                && b.task_type == openxer_engine::model::TaskType::Task
        })
        .map(|l| (m.activities[l.pred].task_id, m.activities[l.succ].task_id))
        .expect("a finish to start tie between two activities not started")
}

#[test]
fn an_import_that_starts_an_activity_out_of_sequence_lists_it_like_a_typed_edit() {
    let dir = temp("sequence");
    let probe = project();
    let (pred, succ) = behind_a_predecessor(&probe);
    let start = "2026-11-17 08:00";
    // Typed: the prompt names the activity.
    let (mut typed, _) = app_in(&dir, "a.db", &probe);
    // User Preferences, Assistance: the warning is on.
    typed.editing.options.warn_out_of_sequence = true;
    let i = index_of(&typed.project, succ);
    typed.try_apply_cell(i, "actual_start", start).unwrap();
    let prompt = typed.sched.sequence.take().expect("the typed edit prompts");
    assert_eq!(prompt.activity, i);
    // Imported: the preview warns and the result lists the same activity.
    let (mut imported, _) = app_in(&dir, "b.db", &probe);
    imported.editing.options.warn_out_of_sequence = true;
    let cell = Cell::Date(serial(&probe, start));
    let plan = plan_sheet(
        &mut imported,
        sheet(&format!("RB-{succ}"), "Actual Start", cell),
    );
    let succ_code = format!("RB-{succ}");
    let pred_code = format!("RB-{pred}");
    assert!(
        plan.notes
            .iter()
            .any(|n| n.contains(&succ_code) && n.contains(&pred_code)),
        "{:?}",
        plan.notes
    );
    imported.sheet_apply(1.0);
    let message = imported
        .interchange
        .message
        .take()
        .expect("the import lists it");
    assert!(message
        .lines
        .iter()
        .any(|l| l.contains(&succ_code) && l.contains(&pred_code)));
    assert!(
        imported.sched.sequence.is_none(),
        "no prompt is left behind"
    );
    assert!(same_state(&imported, &typed));
    // With the warning off (the default) neither asks nor lists.
    let (mut quiet, _) = app_in(&dir, "c.db", &probe);
    assert!(!quiet.editing.options.warn_out_of_sequence);
    let cell = Cell::Date(serial(&probe, start));
    plan_sheet(&mut quiet, sheet(&succ_code, "Actual Start", cell));
    quiet.sheet_apply(1.0);
    assert!(quiet.interchange.message.is_none());
}

#[test]
fn applying_a_progress_import_is_one_undo_step() {
    let dir = temp("undo");
    let probe = project();
    let (mut app, ctx) = app_in(&dir, "a.db", &probe);
    let before = app.project.clone();
    let steps = app.history.counts().0;
    let wb = Workbook {
        sheets: vec![Sheet {
            name: "Updates".into(),
            rows: vec![
                [
                    "Activity ID",
                    "Physical % Complete",
                    "Remaining Duration (h)",
                ]
                .map(|h| Cell::Text(h.into()))
                .to_vec(),
                vec![
                    Cell::Text("RB-1010".into()),
                    Cell::Number(60.),
                    Cell::Number(16.),
                ],
                vec![
                    Cell::Text("RB-1050".into()),
                    Cell::Number(10.),
                    Cell::Number(24.),
                ],
            ],
        }],
    };
    let plan = plan_sheet(&mut app, wb);
    assert_eq!(plan.counts().changed, 2, "{:?}", plan.rows);
    app.sheet_apply(2.0);
    let after = app.project.clone();
    assert_ne!(after.model, before.model);
    assert_eq!(app.history.counts().0, steps + 1, "one undo step");
    app.history_move(&ctx, false);
    assert_eq!(app.project.model, before.model);
    assert_eq!(app.project.details, before.details);
    app.history_move(&ctx, true);
    assert_eq!(app.project.model, after.model);
    assert_eq!(app.project.details, after.details);
}
