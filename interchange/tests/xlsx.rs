use openxer_engine::model::{LinkType, StatusType};
use openxer_engine::scheduler::schedule_with_resources;
use openxer_interchange::columns::{Column, DurationUnit, Lookup};
use openxer_interchange::xlsx_export::{write_modern, write_tables, ExportOptions};
use openxer_interchange::xlsx_import::{
    apply, auto_map, detect, plan, plan_file, read_workbook, Cell, Format, RowKind, Target,
};
use openxer_xer::ProjectData;
use std::path::{Path, PathBuf};

fn artifacts() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../artifacts/interchange");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Starter with an activity code type, a text UDF, a date UDF, and a schedule.
fn project() -> ProjectData {
    let mut p = openxer_xer::starter();
    let doc = &mut p.document;
    doc.table_mut("ACTVTYPE").add(&[
        ("actv_code_type_id", "7".into()),
        ("actv_code_type", "Area".into()),
        ("actv_code_type_scope", "AS_Project".into()),
        ("proj_id", "1".into()),
    ]);
    for (id, name) in [(71, "NORTH"), (72, "SOUTH")] {
        doc.table_mut("ACTVCODE").add(&[
            ("actv_code_id", id.to_string()),
            ("actv_code_type_id", "7".into()),
            ("short_name", name.into()),
            ("actv_code_name", format!("{name} area")),
        ]);
    }
    doc.table_mut("TASKACTV").add(&[
        ("task_id", "1010".into()),
        ("actv_code_type_id", "7".into()),
        ("actv_code_id", "71".into()),
        ("proj_id", "1".into()),
    ]);
    for (id, label, kind) in [
        (900, "Foreman", "FT_TEXT"),
        (901, "Permit Date", "FT_START_DATE"),
    ] {
        doc.table_mut("UDFTYPE").add(&[
            ("udf_type_id", id.to_string()),
            ("table_name", "TASK".into()),
            ("udf_type_name", format!("user_field_{id}")),
            ("udf_type_label", label.into()),
            ("logical_data_type", kind.into()),
        ]);
    }
    doc.table_mut("UDFVALUE").add(&[
        ("udf_type_id", "900".into()),
        ("fk_id", "1020".into()),
        ("proj_id", "1".into()),
        ("udf_text", "J. Ortiz".into()),
    ]);
    let doc = p.updated_document();
    let mut p = ProjectData::from_document(doc, 1, "test".into()).unwrap();
    schedule_with_resources(&mut p.model, &p.resources).unwrap();
    p
}

fn layout_columns(p: &ProjectData) -> Vec<Column> {
    let lookup = Lookup::new(p);
    lookup
        .catalog()
        .into_iter()
        .filter(|c| *c != Column::Critical)
        .collect()
}

fn export(p: &ProjectData, name: &str, columns: Vec<Column>, unit: DurationUnit) -> PathBuf {
    let path = artifacts().join(name);
    let options = ExportOptions {
        columns,
        unit,
        ..Default::default()
    };
    write_modern(p, &options, &path).unwrap();
    path
}

fn set(
    wb: &mut openxer_interchange::xlsx_import::Workbook,
    sheet: &str,
    header: &str,
    id: &str,
    v: Cell,
) {
    let s = wb.sheet(sheet).unwrap();
    let sheet = &mut wb.sheets[s];
    let col = sheet.rows[0]
        .iter()
        .position(|c| c.text() == header)
        .unwrap_or_else(|| panic!("no header {header}"));
    let row = sheet
        .rows
        .iter()
        .position(|r| r.first().is_some_and(|c| c.text() == id))
        .unwrap_or_else(|| panic!("no row {id}"));
    let r = &mut sheet.rows[row];
    r.resize(r.len().max(col + 1), Cell::Empty);
    r[col] = v;
}

fn plan_for(
    wb: &openxer_interchange::xlsx_import::Workbook,
    p: &ProjectData,
) -> openxer_interchange::xlsx_import::ImportPlan {
    let lookup = Lookup::new(p);
    let d = detect(wb, &lookup).unwrap();
    let m = auto_map(&d, &lookup);
    plan(wb, &d, &m, p)
}

#[test]
fn unedited_modern_export_reimports_with_no_changes() {
    let p = project();
    for unit in [DurationUnit::Days, DurationUnit::Hours] {
        let path = export(
            &p,
            &format!("roundtrip-{}.xlsx", unit.suffix()),
            layout_columns(&p),
            unit,
        );
        let (_, detected, mapping, plan) = plan_file(&path, &p).unwrap();
        assert_eq!(detected.format, Format::OpenXer);
        assert_eq!(detected.settings.unit, unit);
        assert!(mapping.activity.contains(&Target::Key));
        let c = plan.counts();
        assert_eq!(
            c.unchanged,
            14,
            "{:?}",
            plan.rows
                .iter()
                .filter(|r| r.kind != RowKind::Unchanged && r.kind != RowKind::Skipped)
                .collect::<Vec<_>>()
        );
        assert_eq!(c.skipped, 4, "four WBS bands");
        assert_eq!((c.changed, c.new, c.delete, c.errors), (0, 0, 0, 0));
        assert_eq!(c.links_unchanged, 16);
        assert!(!plan.has_changes());
    }
}

#[test]
fn edited_export_produces_expected_diff_then_applies() {
    let p = project();
    let path = export(
        &p,
        "edit-source.xlsx",
        layout_columns(&p),
        DurationUnit::Days,
    );
    let mut wb = read_workbook(&path).unwrap();
    set(
        &mut wb,
        "Activities",
        "Activity Name",
        "RB-1010",
        Cell::Text("Survey and soils".into()),
    );
    set(
        &mut wb,
        "Activities",
        "Remaining Duration (d)",
        "RB-1020",
        Cell::Number(10.0),
    );
    set(
        &mut wb,
        "Activities",
        "Physical % Complete",
        "RB-1020",
        Cell::Number(40.0),
    );
    set(
        &mut wb,
        "Activities",
        "Actual Start",
        "RB-1020",
        Cell::Text("03-Nov-26".into()),
    );
    set(
        &mut wb,
        "Activities",
        "Area",
        "RB-1040",
        Cell::Text("south".into()),
    );
    set(
        &mut wb,
        "Activities",
        "Foreman",
        "RB-1050",
        Cell::Text("K. Lee".into()),
    );
    set(
        &mut wb,
        "Activities",
        "Primary Constraint",
        "RB-1060",
        Cell::Text("Start On or After".into()),
    );
    set(
        &mut wb,
        "Activities",
        "Primary Constraint Date",
        "RB-1060",
        Cell::Text("2026-12-01 08:00".into()),
    );
    // A new activity row and a renamed Activity ID (the hidden key keeps identity).
    let s = wb.sheet("Activities").unwrap();
    let mut new_row = vec![
        Cell::Text("RB-2000".into()),
        Cell::Text("Punch list".into()),
    ];
    new_row.resize(wb.sheets[s].rows[0].len(), Cell::Empty);
    wb.sheets[s].rows.push(new_row);
    set(
        &mut wb,
        "Activities",
        "Activity ID",
        "RB-1130",
        Cell::Text("RB-9999".into()),
    );
    // Relationships: change a lag, add one, delete one by flag.
    let l = wb.sheet("Relationships").unwrap();
    let rows = &mut wb.sheets[l].rows;
    rows[1][5] = Cell::Number(2.0);
    rows[2][7] = Cell::Text("D".into());
    rows.push(vec![
        Cell::Text("RB-1000".into()),
        Cell::Empty,
        Cell::Text("RB-2000".into()),
        Cell::Empty,
        Cell::Text("FS".into()),
        Cell::Number(0.0),
    ]);
    let plan = plan_for(&wb, &p);
    let c = plan.counts();
    assert_eq!(
        c.errors,
        0,
        "{:?}",
        plan.rows
            .iter()
            .filter(|r| r.kind == RowKind::Error)
            .collect::<Vec<_>>()
    );
    assert_eq!(c.new, 1);
    assert_eq!(
        c.changed,
        6,
        "{:#?}",
        plan.rows
            .iter()
            .filter(|r| r.kind == RowKind::Changed)
            .collect::<Vec<_>>()
    );
    assert_eq!((c.links_changed, c.links_deleted, c.links_new), (1, 1, 1));
    let row = plan
        .rows
        .iter()
        .find(|r| r.activity_id == "RB-1020")
        .unwrap();
    let cols: Vec<_> = row.changes.iter().map(|c| c.column.as_str()).collect();
    assert!(
        cols.contains(&"Remaining Duration") && cols.contains(&"Actual Start"),
        "{cols:?}"
    );
    let start = row
        .changes
        .iter()
        .find(|c| c.column == "Actual Start")
        .unwrap();
    assert_eq!(
        start.new, "03-Nov-26 08:00",
        "date-only input snaps to the calendar's day start"
    );

    let mut q = p.clone();
    let report = apply(&plan, &mut q).unwrap();
    assert_eq!(
        (
            report.changed,
            report.added,
            report.links_added,
            report.links_deleted
        ),
        (6, 1, 1, 1)
    );
    let find = |code: &str| {
        q.model
            .activities
            .iter()
            .position(|a| a.task_code == code)
            .unwrap()
    };
    assert_eq!(q.name(find("RB-1010")), "Survey and soils");
    let a = &q.model.activities[find("RB-1020")];
    assert_eq!(a.status, StatusType::InProgress);
    assert_eq!(a.rem, 10 * 8 * 60);
    assert_eq!(q.details[&a.task_id].percent, 40.0);
    assert!(q
        .model
        .activities
        .iter()
        .any(|a| a.task_code == "RB-9999" && a.task_id == 1130));
    assert_eq!(q.model.links.len(), p.model.links.len());
    assert_eq!(q.model.links[0].lag, 2 * 8 * 60);
    let lookup = Lookup::new(&q);
    assert_eq!(
        lookup.codes.get(&(1040, 7)).map(String::as_str),
        Some("SOUTH")
    );
    // The edited project exports and reloads with the changes intact.
    let back = ProjectData::from_document(q.updated_document(), 1, "x".into()).unwrap();
    assert_eq!(back.model.activities.len(), 15);
    assert_eq!(back.details[&1010].name, "Survey and soils");
}

#[test]
fn deletes_only_when_flagged_and_errors_never_drop_rows() {
    let p = project();
    let path = export(
        &p,
        "delete-source.xlsx",
        vec![
            Column::ActivityId,
            Column::ActivityName,
            Column::Calendar,
            Column::ActualStart,
            Column::ActualFinish,
        ],
        DurationUnit::Days,
    );
    let mut wb = read_workbook(&path).unwrap();
    let s = wb.sheet("Activities").unwrap();
    // Add a Delete column (users type D) and flag one activity.
    let width = wb.sheets[s].rows[0].len();
    wb.sheets[s].rows[0].push(Cell::Text("Delete".into()));
    for r in wb.sheets[s].rows.iter_mut() {
        r.resize(width + 1, Cell::Empty);
    }
    set(
        &mut wb,
        "Activities",
        "Delete",
        "RB-1120",
        Cell::Text("d".into()),
    );
    set(
        &mut wb,
        "Activities",
        "Calendar",
        "RB-1010",
        Cell::Text("No such calendar".into()),
    );
    set(
        &mut wb,
        "Activities",
        "Actual Start",
        "RB-1030",
        Cell::Text("not a date".into()),
    );
    set(
        &mut wb,
        "Activities",
        "Actual Finish",
        "RB-1040",
        Cell::Text("2026-11-10 17:00".into()),
    );
    // A duplicate Activity ID row.
    let dup = wb.sheets[s]
        .rows
        .iter()
        .find(|r| r.first().is_some_and(|c| c.text() == "RB-1050"))
        .unwrap()
        .clone();
    wb.sheets[s].rows.push(dup);
    // Removing a row from the sheet does not delete the activity.
    let pos = wb.sheets[s]
        .rows
        .iter()
        .position(|r| r.first().is_some_and(|c| c.text() == "RB-1060"))
        .unwrap();
    wb.sheets[s].rows.remove(pos);
    let plan = plan_for(&wb, &p);
    let c = plan.counts();
    assert_eq!(c.delete, 1);
    let errs: Vec<_> = plan
        .rows
        .iter()
        .filter(|r| r.kind == RowKind::Error)
        .map(|r| (r.activity_id.clone(), r.messages.clone()))
        .collect();
    assert_eq!(c.errors, 4, "{errs:?}");
    let messages: Vec<String> = plan.rows.iter().flat_map(|r| r.messages.clone()).collect();
    for needle in [
        "Calendar \"No such calendar\" does not exist",
        "is not a date",
        "Actual Finish needs an Actual Start",
        "already appears on row",
    ] {
        assert!(
            messages.iter().any(|m| m.contains(needle)),
            "missing {needle}: {messages:?}"
        );
    }
    let mut q = p.clone();
    let report = apply(&plan, &mut q).unwrap();
    assert_eq!(report.deleted, 1);
    assert_eq!(report.skipped_errors, 4);
    assert_eq!(q.model.activities.len(), 13);
    assert!(q.model.activities.iter().any(|a| a.task_code == "RB-1060"));
    let doc = q.updated_document();
    assert!(!doc
        .table("TASK")
        .unwrap()
        .rows
        .iter()
        .any(|r| r.contains(&"RB-1120".to_string())));
}

#[test]
fn relationship_cycles_and_type_changes() {
    let p = project();
    let path = export(
        &p,
        "links-source.xlsx",
        vec![Column::ActivityId],
        DurationUnit::Hours,
    );
    let mut wb = read_workbook(&path).unwrap();
    let l = wb.sheet("Relationships").unwrap();
    // First row is RB-1000 -> RB-1010 FS; change its type to SS.
    wb.sheets[l].rows[1][4] = Cell::Text("Start to Start".into());
    wb.sheets[l].rows.push(vec![
        Cell::Text("RB-1130".into()),
        Cell::Empty,
        Cell::Text("RB-1000".into()),
        Cell::Empty,
        Cell::Text("FS".into()),
    ]);
    let plan = plan_for(&wb, &p);
    let c = plan.counts();
    assert_eq!(c.links_changed, 1);
    assert_eq!(c.link_errors, 1);
    assert!(plan.links.iter().any(|l| l.message.contains("loop")));
    let mut q = p.clone();
    apply(&plan, &mut q).unwrap();
    assert_eq!(q.model.links[0].link_type, LinkType::SS);
    assert_eq!(q.model.links.len(), p.model.links.len());
}

#[test]
fn tables_layout_exports_and_reimports() {
    let p = project();
    let path = artifacts().join("tables-layout.xlsx");
    write_tables(&p, DurationUnit::Days, &path).unwrap();
    let wb = read_workbook(&path).unwrap();
    let task = &wb.sheets[wb.sheet("TASK").unwrap()];
    assert_eq!(task.rows[0][0].text(), "task_code");
    assert_eq!(task.rows[1][0].text(), "Activity ID");
    assert!(task.rows[1].iter().any(|c| c.text() == "(*)Start"));
    assert_eq!(task.rows[0].last().unwrap().text(), "delete_record_flag");
    let (_, detected, _, plan) = plan_file(&path, &p).unwrap();
    assert_eq!(detected.format, Format::Tables);
    assert!(detected.settings.unit_known);
    let c = plan.counts();
    assert_eq!(
        (c.unchanged, c.changed, c.errors),
        (14, 0, 0),
        "{:#?}",
        plan.rows
            .iter()
            .filter(|r| r.kind != RowKind::Unchanged)
            .collect::<Vec<_>>()
    );
    assert_eq!(c.links_unchanged, 16);
}

/// Regression (D5): the tables layout rounded durations and lags to 2 decimals
/// of the export unit, so 50 hours on an 8-hour day (6.25 d) survived but
/// 16 h 40 min (2.0833 d) came back as 2.08 d and the reimport planned a
/// change on every such activity. Corpus 5867 showed 20 changed rows.
#[test]
fn tables_layout_keeps_durations_and_lags_that_are_not_whole_hundredths() {
    let mut p = project();
    p.model.activities[2].rem = 1000;
    p.details
        .get_mut(&p.model.activities[2].task_id)
        .unwrap()
        .original_duration = 1000;
    p.model.links[0].lag = 100;
    for unit in [DurationUnit::Days, DurationUnit::Hours] {
        let path = artifacts().join(format!("tables-layout-precision-{unit:?}.xlsx"));
        write_tables(&p, unit, &path).unwrap();
        let (_, _, _, plan) = plan_file(&path, &p).unwrap();
        let c = plan.counts();
        assert_eq!(
            (c.changed, c.errors, c.links_changed),
            (0, 0, 0),
            "{unit:?}: {:#?}",
            plan.rows
                .iter()
                .filter(|r| r.kind != RowKind::Unchanged)
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn plain_sheet_with_titles_and_text_durations() {
    let p = project();
    let path = artifacts().join("plain.xlsx");
    let mut book = rust_xlsxwriter::Workbook::new();
    let ws = book.add_worksheet();
    ws.set_name("Tracker").unwrap();
    for (c, h) in ["ID", "Description", "RD", "% Complete"].iter().enumerate() {
        ws.write_string(0, c as u16, *h).unwrap();
    }
    ws.write_string(1, 0, "RB-1040").unwrap();
    ws.write_string(1, 1, "Long lead buys").unwrap();
    ws.write_string(1, 2, "24h").unwrap();
    ws.write_number(1, 3, 25).unwrap();
    book.save(&path).unwrap();
    let (_, detected, _, plan) = plan_file(&path, &p).unwrap();
    assert_eq!(detected.format, Format::Plain);
    let row = &plan.rows[0];
    assert_eq!(row.kind, RowKind::Changed, "{row:?}");
    let mut q = p.clone();
    apply(&plan, &mut q).unwrap();
    let a = q
        .model
        .activities
        .iter()
        .find(|a| a.task_code == "RB-1040")
        .unwrap();
    assert_eq!(a.rem, 24 * 60);
    assert_eq!(q.details[&a.task_id].percent, 25.0);
}

#[test]
fn malformed_spreadsheets_give_clear_errors() {
    let p = project();
    let missing = plan_file(Path::new("C:/definitely/not/here.xlsx"), &p).unwrap_err();
    assert!(missing.contains("could not be read"), "{missing}");
    let fake = artifacts().join("not-a-workbook.xlsx");
    std::fs::write(&fake, "this is plain text").unwrap();
    let err = plan_file(&fake, &p).unwrap_err();
    assert!(
        err.contains("could not be read as an Excel workbook"),
        "{err}"
    );
    let no_id = artifacts().join("no-activity-id.xlsx");
    let mut book = rust_xlsxwriter::Workbook::new();
    let ws = book.add_worksheet();
    ws.write_string(0, 0, "Name").unwrap();
    ws.write_string(1, 0, "Something").unwrap();
    book.save(&no_id).unwrap();
    let err = plan_file(&no_id, &p).unwrap_err();
    assert!(err.contains("No sheet has an Activity ID column"), "{err}");
}

/// Cross-check the modern file with openpyxl when Windows Python is available.
#[test]
fn modern_export_opens_in_openpyxl() {
    let p = project();
    let path = export(
        &p,
        "openpyxl-check.xlsx",
        vec![
            Column::ActivityId,
            Column::ActivityName,
            Column::RemainingDuration,
            Column::Start,
            Column::Finish,
            Column::TotalFloat,
        ],
        DurationUnit::Days,
    );
    let script = r#"
import sys, datetime, openpyxl
wb = openpyxl.load_workbook(sys.argv[1])
assert wb.sheetnames == ['Activities', 'Relationships', 'Resource Assignments', 'About'], wb.sheetnames
ws = wb['Activities']
assert ws.freeze_panes == 'C2', ws.freeze_panes
assert ws.auto_filter.ref and ws.auto_filter.ref.startswith('A1:F'), ws.auto_filter.ref
assert ws['C1'].value == 'Remaining Duration (d)'
dates = [c for c in ws['D'][1:] if c.value is not None]
assert dates and all(isinstance(c.value, datetime.datetime) for c in dates), [c.value for c in dates[:3]]
assert all(c.is_date for c in dates)
levels = [ws.row_dimensions[r].outline_level for r in range(2, ws.max_row + 1)]
assert 0 in levels and 1 in levels, levels
assert ws['A2'].font.b, 'band rows are bold'
assert len(ws.conditional_formatting) >= 2
assert isinstance(ws['C3'].value, (int, float))
about = {r[0]: r[1] for r in wb['About'].iter_rows(values_only=True) if r[0]}
assert about['Format'] == 'OpenXER activity export' and about['Duration Unit'] == 'Days'
print('openpyxl ok', len(dates))
"#;
    let out = std::process::Command::new("python.exe")
        .arg("-c")
        .arg(script)
        .arg(&path)
        .output();
    match out {
        Ok(o) if o.status.success() => eprintln!("{}", String::from_utf8_lossy(&o.stdout)),
        Ok(o) if String::from_utf8_lossy(&o.stderr).contains("No module named") => {
            eprintln!("SKIPPED: openpyxl is not installed")
        }
        Ok(o) => panic!(
            "openpyxl check failed: {}",
            String::from_utf8_lossy(&o.stderr)
        ),
        Err(e) => eprintln!("SKIPPED: python.exe not available ({e})"),
    }
}

#[test]
fn control_characters_cannot_break_xer_rows() {
    let p = project();
    let path = export(
        &p,
        "control-chars.xlsx",
        vec![Column::ActivityId, Column::ActivityName, Column::Udf(900)],
        DurationUnit::Days,
    );
    let mut wb = read_workbook(&path).unwrap();
    set(
        &mut wb,
        "Activities",
        "Activity Name",
        "RB-1010",
        Cell::Text("Line one\nline two\r\tend".into()),
    );
    set(
        &mut wb,
        "Activities",
        "Foreman",
        "RB-1020",
        Cell::Text("A.\tB\nC".into()),
    );
    // A new row whose Activity ID carries a tab, and a renamed ID with a newline.
    let s = wb.sheet("Activities").unwrap();
    let mut bad = vec![Cell::Text("A1\tX".into()), Cell::Text("Injected".into())];
    bad.resize(wb.sheets[s].rows[0].len(), Cell::Empty);
    wb.sheets[s].rows.push(bad);
    set(
        &mut wb,
        "Activities",
        "Activity ID",
        "RB-1030",
        Cell::Text("RB-1030\n%R\tevil".into()),
    );
    let plan = plan_for(&wb, &p);
    let errors: Vec<_> = plan
        .rows
        .iter()
        .filter(|r| r.kind == RowKind::Error)
        .collect();
    assert_eq!(errors.len(), 2, "{errors:?}");
    assert!(errors
        .iter()
        .all(|r| r.messages[0].contains("control character")));
    let mut q = p.clone();
    apply(&plan, &mut q).unwrap();
    assert_eq!(q.details[&1010].name, "Line one line two  end");
    assert!(q
        .model
        .activities
        .iter()
        .all(|a| !a.task_code.chars().any(char::is_control)));
    let doc = q.updated_document();
    let text = doc.text();
    let back = openxer_xer::Document::parse(&text).unwrap();
    for t in &doc.tables {
        assert_eq!(
            back.table(&t.name).unwrap().rows.len(),
            t.rows.len(),
            "{}",
            t.name
        );
    }
    assert_eq!(back.table("TASK").unwrap().rows.len(), 14);
    let reloaded = ProjectData::from_document(back, 1, "x".into()).unwrap();
    assert_eq!(reloaded.details[&1010].name, "Line one line two  end");
}

/// The test project with a pipe crew assigned 24 hours on RB-1010.
fn with_assignment() -> ProjectData {
    let mut p = project();
    p.document.table_mut("RSRC").add(&[
        ("rsrc_id", "10".into()),
        ("rsrc_short_name", "PIPE".into()),
        ("rsrc_name", "Pipe crew".into()),
        ("rsrc_type", "RT_Labor".into()),
        ("def_qty_per_hr", "1".into()),
        ("clndr_id", "1".into()),
    ]);
    p.add_assignment(1010, Some(10), None).unwrap();
    p
}

#[test]
fn assignment_sheet_plans_then_applies_in_one_step() {
    use openxer_interchange::xlsx_import::AssignTarget;
    let p = with_assignment();
    let path = artifacts().join("assignments.xlsx");
    let mut book = rust_xlsxwriter::Workbook::new();
    let ws = book.add_worksheet();
    ws.set_name("Crew loading").unwrap();
    for (c, h) in [
        "Activity ID",
        "Resource ID",
        "Budgeted Units (h)",
        "Remaining Units (d)",
        "Delete",
    ]
    .iter()
    .enumerate()
    {
        ws.write_string(0, c as u16, *h).unwrap();
    }
    type Line<'a> = (&'a str, &'a str, Option<f64>, Option<f64>, &'a str);
    let rows: [Line; 5] = [
        ("RB-1010", "PIPE", Some(30.), None, ""),
        ("RB-1020", "pipe", Some(16.), Some(2.), ""),
        ("RB-1030", "NOPE", Some(8.), None, ""),
        ("RB-1000", "PIPE", Some(8.), None, ""),
        ("RB-1040", "PIPE", None, None, "Y"),
    ];
    for (r, (a, rsrc, budget, left, delete)) in rows.iter().enumerate() {
        let r = r as u32 + 1;
        ws.write_string(r, 0, *a).unwrap();
        ws.write_string(r, 1, *rsrc).unwrap();
        if let Some(v) = budget {
            ws.write_number(r, 2, *v).unwrap();
        }
        if let Some(v) = left {
            ws.write_number(r, 3, *v).unwrap();
        }
        ws.write_string(r, 4, *delete).unwrap();
    }
    book.save(&path).unwrap();
    let (_, detected, mapping, plan) = plan_file(&path, &p).unwrap();
    assert_eq!(detected.format, Format::Plain);
    assert!(detected.assignments.is_some());
    assert_eq!(
        mapping.assignment,
        [
            AssignTarget::Activity,
            AssignTarget::Resource,
            AssignTarget::BudgetedUnits,
            AssignTarget::RemainingUnits,
            AssignTarget::Delete
        ]
    );
    assert!(
        plan.rows.is_empty(),
        "an assignments sheet alone updates assignments only"
    );
    let kinds: Vec<RowKind> = plan.assignments.iter().map(|a| a.kind).collect();
    assert_eq!(
        kinds,
        [
            RowKind::Changed,
            RowKind::New,
            RowKind::Error,
            RowKind::Error,
            RowKind::Error
        ],
        "{:?}",
        plan.assignments
    );
    assert!(plan.assignments[2].message.contains("NOPE"));
    assert!(plan.assignments[3].message.contains("Milestones"));
    assert!(plan.assignments[4].message.contains("No such assignment"));
    let c = plan.counts();
    assert_eq!(
        (
            c.assignments_new,
            c.assignments_changed,
            c.assignment_errors
        ),
        (1, 1, 3)
    );
    assert!(plan.has_changes());
    // Nothing changes until apply; apply changes the project in one step.
    let mut q = p.clone();
    let report = apply(&plan, &mut q).unwrap();
    assert_eq!(
        (report.assignments_added, report.assignments_changed),
        (1, 1)
    );
    let units = |q: &ProjectData, task: i64| {
        let row = q.assignment_rows(task)[0];
        let v = q.assignment_view(row).unwrap();
        (v.budgeted.units, v.remaining.units)
    };
    assert_eq!(units(&q, 1010), (30., 30.), "linked before the start");
    assert_eq!(units(&q, 1020), (16., 16.));
    assert_eq!(
        units(&p, 1010),
        (24., 24.),
        "the source project is untouched"
    );
    // A second import of the same sheet changes nothing more.
    let again = plan_file(&path, &q).unwrap().3;
    assert!(again
        .assignments
        .iter()
        .all(|a| matches!(a.kind, RowKind::Unchanged | RowKind::Error)));
}

#[test]
fn exported_assignments_reimport_unchanged_and_delete_when_flagged() {
    let p = with_assignment();
    let path = export(
        &p,
        "assignments-modern.xlsx",
        layout_columns(&p),
        DurationUnit::Days,
    );
    let (_, detected, _, plan) = plan_file(&path, &p).unwrap();
    assert!(detected.assignments.is_some());
    assert_eq!(plan.assignments.len(), 1);
    assert_eq!(plan.assignments[0].kind, RowKind::Unchanged);
    // The tables layout's TASKRSRC sheet, with the delete flag set.
    let tables = artifacts().join("assignments-tables.xlsx");
    write_tables(&p, DurationUnit::Days, &tables).unwrap();
    let mut wb = read_workbook(&tables).unwrap();
    let s = wb.sheet("TASKRSRC").unwrap();
    let sheet = &mut wb.sheets[s];
    sheet.rows[2].resize(5, Cell::Empty);
    sheet.rows[2][4] = Cell::Text("D".into());
    let plan = plan_for(&wb, &p);
    assert_eq!(
        plan.assignments[0].kind,
        RowKind::Delete,
        "{:?}",
        plan.assignments
    );
    let mut q = p.clone();
    let report = apply(&plan, &mut q).unwrap();
    assert_eq!(report.assignments_deleted, 1);
    assert!(q.assignment_rows(1010).is_empty());
}

/// A project read from a file that writes quote marks doubled: the workbook
/// holds single quote marks, re-importing it changes nothing, and a name typed
/// with a quote mark goes back to the file in the doubled form.
#[test]
fn spreadsheet_shows_single_quote_marks_and_the_file_gets_doubled_ones() {
    let mut p = project();
    p.details.get_mut(&1010).unwrap().name = "Set 8\" valve \"V-1\"".into();
    let file = p.updated_document().text();
    assert!(file.contains("Set 8\"\" valve \"\"V-1\"\""), "{file}");
    let p = {
        let mut q =
            ProjectData::from_document(openxer_xer::Document::parse(&file).unwrap(), 1, "q".into())
                .unwrap();
        schedule_with_resources(&mut q.model, &q.resources).unwrap();
        q
    };
    assert_eq!(p.details[&1010].name, "Set 8\" valve \"V-1\"");
    let path = export(
        &p,
        "quotes.xlsx",
        vec![Column::ActivityId, Column::ActivityName],
        DurationUnit::Days,
    );
    let mut wb = read_workbook(&path).unwrap();
    let s = wb.sheet("Activities").unwrap();
    assert!(
        wb.sheets[s]
            .rows
            .iter()
            .any(|r| r.iter().any(|c| c.text() == "Set 8\" valve \"V-1\"")),
        "the sheet holds the single-quote text"
    );
    assert!(!plan_for(&wb, &p).has_changes());
    set(
        &mut wb,
        "Activities",
        "Activity Name",
        "RB-1020",
        Cell::Text("Pour 6\" slab".into()),
    );
    let plan = plan_for(&wb, &p);
    let mut q = p.clone();
    apply(&plan, &mut q).unwrap();
    assert_eq!(q.details[&1020].name, "Pour 6\" slab");
    let out = q.updated_document().text();
    assert!(out.contains("\tPour 6\"\" slab\t"), "{out}");
    assert!(out.contains("Set 8\"\" valve \"\"V-1\"\""), "{out}");
}

fn plain_book(rows: Vec<Vec<Cell>>) -> openxer_interchange::xlsx_import::Workbook {
    openxer_interchange::xlsx_import::Workbook {
        sheets: vec![openxer_interchange::xlsx_import::Sheet {
            name: "Updates".into(),
            rows,
        }],
    }
}

fn texts(cells: &[&str]) -> Vec<Cell> {
    cells.iter().map(|t| Cell::Text(t.to_string())).collect()
}

/// Regression: an Excel error value (#N/A) in a mapped column was read as an
/// empty cell. It is now a row error that names the cell, on every sheet.
#[test]
fn excel_error_cells_are_row_errors_that_name_the_cell() {
    let p = project();
    let mut wb = plain_book(vec![
        texts(&["Activity ID", "Actual Start", "Notes"]),
        vec![
            Cell::Text("RB-1010".into()),
            Cell::Error("#N/A".into()),
            Cell::Empty,
        ],
        // An error in a column the import does not read is no problem.
        vec![
            Cell::Text("RB-1020".into()),
            Cell::Text("03-Nov-26".into()),
            Cell::Error("#REF!".into()),
        ],
    ]);
    wb.sheets.push(openxer_interchange::xlsx_import::Sheet {
        name: "Links".into(),
        rows: vec![
            texts(&["Predecessor", "Successor", "Lag"]),
            vec![
                Cell::Text("RB-1000".into()),
                Cell::Text("RB-1010".into()),
                Cell::Error("#VALUE!".into()),
            ],
        ],
    });
    let plan = plan_for(&wb, &p);
    assert_eq!(plan.rows[0].kind, RowKind::Error, "{:?}", plan.rows[0]);
    assert!(
        plan.rows[0]
            .messages
            .iter()
            .any(|m| m.contains("B2") && m.contains("#N/A")),
        "{:?}",
        plan.rows[0].messages
    );
    assert_eq!(plan.rows[1].kind, RowKind::Changed, "{:?}", plan.rows[1]);
    assert_eq!(plan.links[0].kind, RowKind::Error, "{:?}", plan.links[0]);
    assert!(plan.links[0].message.contains("C2"), "{:?}", plan.links[0]);
}

/// Fractions in a percent column (0.5 meaning 50%) that were not formatted
/// as percentages are flagged in the preview.
#[test]
fn percent_column_of_fractions_is_flagged() {
    let p = project();
    let book = |a: f64, b: f64| {
        plain_book(vec![
            texts(&["Activity ID", "Physical % Complete"]),
            vec![Cell::Text("RB-1010".into()), Cell::Number(a)],
            vec![Cell::Text("RB-1020".into()), Cell::Number(b)],
        ])
    };
    let flagged = |plan: &openxer_interchange::xlsx_import::ImportPlan| {
        plan.notes.iter().any(|n| n.contains("between 0 and 1"))
    };
    assert!(flagged(&plan_for(&book(0.5, 0.25), &p)));
    assert!(!flagged(&plan_for(&book(50.0, 0.5), &p)));
    assert!(!flagged(&plan_for(&book(0.0, 0.0), &p)));
}

/// Regression: a blank date cell cleared the date, so a blank Actual Start
/// on a started activity reset its start, finish and remaining duration;
/// blank code and UDF cells removed the value. A blank cell now changes
/// nothing in every column, and `<none>` clears a value on purpose.
#[test]
fn blank_cells_change_nothing_and_none_clears() {
    let mut p = project();
    let i = p
        .model
        .activities
        .iter()
        .position(|a| a.task_code == "RB-1010")
        .unwrap();
    let start = p.model.project.data_date.unwrap();
    p.model.activities[i].act_start = Some(start);
    p.model.activities[i].status = StatusType::InProgress;
    let header = texts(&[
        "Activity ID",
        "Actual Start",
        "Area",
        "Primary Constraint",
        "Foreman",
    ]);
    let blank = plain_book(vec![
        header.clone(),
        vec![
            Cell::Text("RB-1010".into()),
            Cell::Empty,
            Cell::Text(String::new()),
            Cell::Empty,
            Cell::Empty,
        ],
        vec![
            Cell::Text("RB-1020".into()),
            Cell::Empty,
            Cell::Empty,
            Cell::Empty,
            Cell::Empty,
        ],
    ]);
    let plan = plan_for(&blank, &p);
    assert!(
        plan.rows.iter().all(|r| r.kind == RowKind::Unchanged),
        "{:#?}",
        plan.rows
    );
    let clear = plain_book(vec![
        header,
        vec![
            Cell::Text("RB-1010".into()),
            Cell::Empty,
            Cell::Text("<none>".into()),
            Cell::Empty,
            Cell::Empty,
        ],
        vec![
            Cell::Text("RB-1020".into()),
            Cell::Empty,
            Cell::Empty,
            Cell::Empty,
            Cell::Text("<None>".into()),
        ],
    ]);
    let plan = plan_for(&clear, &p);
    assert_eq!(
        plan.rows.iter().map(|r| r.kind).collect::<Vec<_>>(),
        [RowKind::Changed, RowKind::Changed],
        "{:#?}",
        plan.rows
    );
    let mut q = p.clone();
    apply(&plan, &mut q).unwrap();
    let lookup = Lookup::new(&q);
    assert_eq!(lookup.codes.get(&(1010, 7)), None, "the code was removed");
    assert_eq!(q.model.activities[i].act_start, Some(start), "start kept");
    let udfs = q.document.table("UDFVALUE").unwrap();
    assert!(!udfs.rows.iter().any(|r| udfs.value(r, "fk_id") == "1020"));
}

/// Regression: a date-only constraint date snapped to the start or end of
/// the day by the activity's old constraint type, not the type in the same
/// row; the secondary date never snapped to the finish; and snapping used
/// the old calendar when the row also changed the calendar.
#[test]
fn date_only_dates_snap_by_the_rows_own_constraint_and_calendar() {
    let mut doc = project().updated_document();
    let mut data = openxer_xer::standard_calendar_data();
    data = data
        .replace("s|08:00|f|12:00", "s|07:00|f|11:00")
        .replace("s|13:00|f|17:00", "s|11:30|f|15:30");
    doc.table_mut("CALENDAR").add(&[
        ("clndr_id", "2".into()),
        ("clndr_name", "Early shift".into()),
        ("clndr_data", data),
    ]);
    let mut p = ProjectData::from_document(doc, 1, "test".into()).unwrap();
    schedule_with_resources(&mut p.model, &p.resources).unwrap();
    let wb = plain_book(vec![
        texts(&[
            "Activity ID",
            "Primary Constraint Date",
            "Primary Constraint",
            "Secondary Constraint Date",
            "Secondary Constraint",
            "Actual Start",
            "Calendar",
        ]),
        texts(&[
            "RB-1040",
            "2026-12-04",
            "Finish On or Before",
            "",
            "",
            "",
            "",
        ]),
        texts(&[
            "RB-1050",
            "2026-12-01",
            "Start On or After",
            "2026-12-11",
            "Finish On or Before",
            "",
            "",
        ]),
        texts(&["RB-1010", "", "", "", "", "2026-11-03", "Early shift"]),
    ]);
    let plan = plan_for(&wb, &p);
    let new = |code: &str, column: &str| {
        let row = plan.rows.iter().find(|r| r.activity_id == code).unwrap();
        assert_ne!(row.kind, RowKind::Error, "{row:?}");
        row.changes
            .iter()
            .find(|c| c.column == column)
            .unwrap_or_else(|| panic!("{code} {column}: {row:?}"))
            .new
            .clone()
    };
    assert_eq!(new("RB-1040", "Primary Constraint Date"), "04-Dec-26 17:00");
    assert_eq!(new("RB-1050", "Primary Constraint Date"), "01-Dec-26 08:00");
    assert_eq!(
        new("RB-1050", "Secondary Constraint Date"),
        "11-Dec-26 17:00"
    );
    assert_eq!(new("RB-1010", "Actual Start"), "03-Nov-26 07:00");
}

/// Regression: a row whose Activity ID cell was cleared but whose hidden key
/// still matched planned an empty Activity ID, and apply wrote it without
/// the edit layer's check. Relationship lags now go through the edit layer
/// too, so a lag it refuses is a row error in the preview, not a surprise.
#[test]
fn cleared_activity_id_is_an_error_and_edits_use_the_edit_layer() {
    let p = project();
    let path = export(
        &p,
        "cleared-id.xlsx",
        vec![Column::ActivityId, Column::ActivityName],
        DurationUnit::Days,
    );
    let mut wb = read_workbook(&path).unwrap();
    set(
        &mut wb,
        "Activities",
        "Activity ID",
        "RB-1010",
        Cell::Text(String::new()),
    );
    let l = wb.sheet("Relationships").unwrap();
    wb.sheets[l].rows[1][5] = Cell::Number(20_000.0);
    let plan = plan_for(&wb, &p);
    let errors: Vec<_> = plan
        .rows
        .iter()
        .filter(|r| r.kind == RowKind::Error)
        .collect();
    assert_eq!(errors.len(), 1, "{:#?}", plan.rows);
    assert!(errors[0].messages[0].contains("Activity ID is empty"));
    assert_eq!(plan.links[0].kind, RowKind::Error, "{:?}", plan.links[0]);
    let mut q = p.clone();
    apply(&plan, &mut q).unwrap();
    assert!(q.model.activities.iter().all(|a| !a.task_code.is_empty()));
    assert_eq!(q.model.links, p.model.links);
}

/// The import takes suspend and resume dates with the table's rules: a
/// suspend date before the actual start, or a resume date on or before the
/// suspend date, is a row error naming the column and the cell.
#[test]
fn suspend_and_resume_dates_follow_the_shared_rules() {
    let p = project();
    let wb = plain_book(vec![
        texts(&["Activity ID", "Actual Start", "Suspend Date", "Resume Date"]),
        texts(&["RB-1010", "26-Oct-26", "23-Oct-26", ""]),
        texts(&["RB-1020", "26-Oct-26", "28-Oct-26", "28-Oct-26"]),
        texts(&["RB-1040", "26-Oct-26", "28-Oct-26", "29-Oct-26"]),
    ]);
    let plan = plan_for(&wb, &p);
    let messages = |k: usize| plan.rows[k].messages.join(" | ");
    assert_eq!(plan.rows[0].kind, RowKind::Error, "{:?}", plan.rows[0]);
    assert!(
        messages(0).contains("Suspend Date (cell C2)")
            && messages(0).contains("cannot be before the actual start"),
        "{}",
        messages(0)
    );
    assert_eq!(plan.rows[1].kind, RowKind::Error, "{:?}", plan.rows[1]);
    assert!(
        messages(1).contains("Resume Date (cell D3)")
            && messages(1).contains("must be after the suspend date"),
        "{}",
        messages(1)
    );
    assert_eq!(plan.rows[2].kind, RowKind::Changed, "{:?}", plan.rows[2]);
    let mut q = p.clone();
    apply(&plan, &mut q).unwrap();
    let task = |q: &ProjectData, code: &str| {
        q.model
            .activities
            .iter()
            .find(|a| a.task_code == code)
            .unwrap()
            .clone()
    };
    // The refused rows changed nothing; the good one has both dates.
    assert_eq!(task(&q, "RB-1010"), task(&p, "RB-1010"));
    assert_eq!(task(&q, "RB-1020"), task(&p, "RB-1020"));
    let a = task(&q, "RB-1040");
    let resume = openxer_xer::parse_relative(q.model.origin_serial, "2026-10-29 00:00")
        .unwrap()
        .unwrap();
    assert_eq!(a.resume.map(|r| r / 1440), Some(resume / 1440));
    assert!(openxer_xer::edit::records::task_value(
        &q.document,
        a.task_id,
        q.model.project.proj_id,
        "suspend_date"
    )
    .starts_with("2026-10-28"));
}

/// Regression: a negative lag typed as text ("-2d") was refused.
#[test]
fn negative_text_lag_is_accepted() {
    let p = project();
    let mut wb = plain_book(vec![texts(&["Activity ID"])]);
    wb.sheets.push(openxer_interchange::xlsx_import::Sheet {
        name: "Links".into(),
        rows: vec![
            texts(&["Predecessor", "Successor", "Lag"]),
            texts(&["RB-1000", "RB-1010", "-2d"]),
            texts(&["RB-1010", "RB-1020", "- 4h"]),
        ],
    });
    let plan = plan_for(&wb, &p);
    assert!(
        plan.links.iter().all(|l| l.kind == RowKind::Changed),
        "{:#?}",
        plan.links
    );
    let mut q = p.clone();
    apply(&plan, &mut q).unwrap();
    assert_eq!(q.model.links[0].lag, -2 * 8 * 60);
    assert_eq!(q.model.links[1].lag, -4 * 60);
}

/// Regression: with the hidden key column deleted, the WBS band rows were
/// planned as new activities named after the band.
#[test]
fn band_rows_are_skipped_without_the_key_column() {
    let p = project();
    let path = export(
        &p,
        "no-key.xlsx",
        vec![Column::ActivityId, Column::ActivityName],
        DurationUnit::Days,
    );
    let mut wb = read_workbook(&path).unwrap();
    let s = wb.sheet("Activities").unwrap();
    let key = wb.sheets[s].rows[0]
        .iter()
        .position(|c| c.text() == "OpenXER Key")
        .unwrap();
    for r in wb.sheets[s].rows.iter_mut() {
        if r.len() > key {
            r.remove(key);
        }
    }
    let plan = plan_for(&wb, &p);
    let c = plan.counts();
    assert_eq!((c.new, c.errors, c.skipped), (0, 0, 4), "{:#?}", plan.rows);
    assert_eq!(c.unchanged, 14);
    assert!(
        plan.notes.iter().any(|n| n.contains("Key column")),
        "{:?}",
        plan.notes
    );
}

/// Regression: the tables layout assumed a field-name row and a title row,
/// so a TASK sheet with one header row lost its first activity.
#[test]
fn tables_layout_with_one_header_row_keeps_the_first_activity() {
    let p = project();
    let wb = openxer_interchange::xlsx_import::Workbook {
        sheets: vec![openxer_interchange::xlsx_import::Sheet {
            name: "TASK".into(),
            rows: vec![
                texts(&["task_code", "task_name"]),
                texts(&["RB-1010", "Survey first"]),
                texts(&["RB-1020", "Design second"]),
            ],
        }],
    };
    let plan = plan_for(&wb, &p);
    assert_eq!(plan.rows.len(), 2, "{:#?}", plan.rows);
    assert!(plan.rows.iter().all(|r| r.kind == RowKind::Changed));
    // The two-row form still reads both header rows as headers.
    let wb = openxer_interchange::xlsx_import::Workbook {
        sheets: vec![openxer_interchange::xlsx_import::Sheet {
            name: "TASK".into(),
            rows: vec![
                texts(&["task_code", "task_name"]),
                texts(&["Activity ID", "Activity Name"]),
                texts(&["RB-1010", "Survey first"]),
            ],
        }],
    };
    assert_eq!(plan_for(&wb, &p).rows.len(), 1);
}

/// Tables-layout dates are text; they now carry four-digit years, and the
/// two-digit form written before still reads back.
#[test]
fn tables_layout_dates_have_four_digit_years() {
    let p = project();
    let path = artifacts().join("tables-four-digit.xlsx");
    write_tables(&p, DurationUnit::Days, &path).unwrap();
    let wb = read_workbook(&path).unwrap();
    let task = &wb.sheets[wb.sheet("TASK").unwrap()];
    let dates: Vec<String> = task
        .rows
        .iter()
        .flatten()
        .map(Cell::text)
        .filter(|t| t.contains("-Nov-") || t.contains("-Dec-"))
        .collect();
    assert!(!dates.is_empty());
    assert!(dates.iter().all(|d| d.contains("-2026 ")), "{dates:?}");
    let mut old = wb.clone();
    let s = old.sheet("TASK").unwrap();
    for c in old.sheets[s].rows.iter_mut().flatten() {
        if let Cell::Text(t) = c {
            *t = t.replace("-2026 ", "-26 ");
        }
    }
    let c = plan_for(&old, &p).counts();
    assert_eq!((c.changed, c.errors), (0, 0));
}

/// Regression: an activity with the same resource assigned twice exported
/// two rows, and re-importing the untouched sheet reported a duplicate.
#[test]
fn the_same_resource_twice_round_trips_cleanly() {
    let mut p = with_assignment();
    let second = p.add_assignment(1010, Some(10), None).unwrap();
    p.edit_assignment(
        second,
        openxer_xer::assignments::AssignmentEdit::BudgetedUnits(8.),
    )
    .unwrap();
    let path = export(
        &p,
        "twice.xlsx",
        vec![Column::ActivityId, Column::ActivityName],
        DurationUnit::Days,
    );
    let (_, _, _, plan) = plan_file(&path, &p).unwrap();
    assert_eq!(plan.assignments.len(), 2);
    assert!(
        plan.assignments
            .iter()
            .all(|a| a.kind == RowKind::Unchanged),
        "{:#?}",
        plan.assignments
    );
    // Changing the second row changes the second assignment only.
    let mut wb = read_workbook(&path).unwrap();
    let s = wb.sheet("Resource Assignments").unwrap();
    // Budgeted and remaining units move together before the start.
    wb.sheets[s].rows[2][4] = Cell::Number(12.);
    wb.sheets[s].rows[2][5] = Cell::Number(12.);
    let plan = plan_for(&wb, &p);
    assert_eq!(
        plan.counts().assignments_changed,
        1,
        "{:#?}",
        plan.assignments
    );
    let mut q = p.clone();
    apply(&plan, &mut q).unwrap();
    let units: Vec<f64> = q
        .assignment_rows(1010)
        .into_iter()
        .map(|r| q.assignment_view(r).unwrap().budgeted.units)
        .collect();
    assert_eq!(units, [24., 12.]);
}

/// Regression: a number UDF accepted "NaN" and "inf".
#[test]
fn number_udf_refuses_non_finite_values() {
    let mut doc = project().updated_document();
    doc.table_mut("UDFTYPE").add(&[
        ("udf_type_id", "902".into()),
        ("table_name", "TASK".into()),
        ("udf_type_name", "user_field_902".into()),
        ("udf_type_label", "Crew Size".into()),
        ("logical_data_type", "FT_FLOAT_2_DECIMALS".into()),
    ]);
    let p = ProjectData::from_document(doc, 1, "test".into()).unwrap();
    for bad in ["NaN", "inf", "-infinity"] {
        let wb = plain_book(vec![
            texts(&["Activity ID", "Crew Size"]),
            texts(&["RB-1010", bad]),
        ]);
        let plan = plan_for(&wb, &p);
        assert_eq!(
            plan.rows[0].kind,
            RowKind::Error,
            "{bad}: {:?}",
            plan.rows[0]
        );
    }
    let wb = plain_book(vec![
        texts(&["Activity ID", "Crew Size"]),
        texts(&["RB-1010", "6"]),
    ]);
    assert_eq!(plan_for(&wb, &p).rows[0].kind, RowKind::Changed);
}

/// The number format of cell (`row`, `col`) of `sheet` in the file at `path`.
fn number_format(path: &Path, sheet: &str, row: u32, col: u32) -> String {
    let mut book: calamine::Xlsx<_> = calamine::open_workbook(path).unwrap();
    let styles = book.worksheet_style(sheet).unwrap();
    styles
        .get((row, col))
        .and_then(|s| s.number_format.clone())
        .map(|f| f.format_code)
        .unwrap_or_default()
}

/// Regression: percent cells used `0.##`, which Excel shows as "50." for a
/// whole number. Whole numbers now get a format without decimals.
#[test]
fn whole_percentages_have_no_trailing_decimal_point() {
    let mut p = project();
    p.details.get_mut(&1010).unwrap().percent = 50.0;
    p.details.get_mut(&1020).unwrap().percent = 12.5;
    let path = export(
        &p,
        "percent-format.xlsx",
        vec![Column::ActivityId, Column::PercentComplete],
        DurationUnit::Days,
    );
    let wb = read_workbook(&path).unwrap();
    let s = &wb.sheets[wb.sheet("Activities").unwrap()];
    let row = |code: &str| {
        s.rows
            .iter()
            .position(|r| r.first().is_some_and(|c| c.text() == code))
            .unwrap() as u32
    };
    for (code, value) in [("RB-1010", 50.0), ("RB-1020", 12.5)] {
        let r = row(code);
        assert_eq!(*s.cell(r as usize, 1), Cell::Number(value));
        let format = number_format(&path, "Activities", r, 1);
        if value.fract() == 0.0 {
            assert_eq!(format, "0", "{code}");
        } else {
            assert!(!format.ends_with(".##"), "{code}: {format}");
        }
    }
}

/// Set a value on the project's PROJECT row.
fn set_project_value(p: &mut ProjectData, key: &str, value: &str) {
    let pid = p.model.project.proj_id.to_string();
    let t = p.document.table_mut("PROJECT");
    let row = t
        .rows
        .iter()
        .position(|r| t.value(r, "proj_id") == pid)
        .unwrap();
    t.set(row, key, value.into());
}

/// The exported Critical column and the hidden critical flag of activities
/// 0 and 1, read back from the written workbook.
fn exported_critical(p: &ProjectData, name: &str) -> Vec<(bool, bool)> {
    use openxer_interchange::xlsx_export::{ACTIVITY_SHEET, CRITICAL_HEADER};
    let path = export(
        p,
        name,
        vec![Column::ActivityId, Column::Critical],
        DurationUnit::Days,
    );
    let wb = read_workbook(&path).unwrap();
    let sheet = &wb.sheets[wb.sheet(ACTIVITY_SHEET).unwrap()];
    let hidden = sheet.rows[0]
        .iter()
        .position(|c| c.text() == CRITICAL_HEADER)
        .unwrap();
    let flag = |c: Option<&Cell>| matches!(c, Some(Cell::Bool(true)));
    [0usize, 1]
        .iter()
        .map(|&i| {
            let code = &p.model.activities[i].task_code;
            let row = sheet
                .rows
                .iter()
                .find(|r| r.first().is_some_and(|c| c.text() == *code))
                .unwrap_or_else(|| panic!("no row {code}"));
            (flag(row.get(1)), flag(row.get(hidden)))
        })
        .collect()
}

#[test]
fn critical_column_follows_the_project_longest_path_definition() {
    use openxer_interchange::columns::{value, Value};
    let mut p = project();
    set_project_value(&mut p, "critical_path_type", "CT_DrivPath");
    // Zero float, off the longest path: not critical by this project's rule.
    let a = &mut p.model.activities[0];
    a.status = StatusType::NotStarted;
    a.total_float = Some(0);
    a.driving_path = false;
    // Positive float, on the longest path: critical.
    let b = &mut p.model.activities[1];
    b.status = StatusType::NotStarted;
    b.total_float = Some(40 * 60);
    b.driving_path = true;
    let lookup = Lookup::new(&p);
    assert_eq!(value(&p, &lookup, 0, Column::Critical), Value::Bool(false));
    assert_eq!(value(&p, &lookup, 1, Column::Critical), Value::Bool(true));
    assert_eq!(
        exported_critical(&p, "critical-longest-path.xlsx"),
        [(false, false), (true, true)]
    );
}

#[test]
fn critical_column_follows_the_project_float_threshold() {
    use openxer_interchange::columns::{value, Value};
    let mut p = project();
    set_project_value(&mut p, "critical_path_type", "CT_TotFloat");
    set_project_value(&mut p, "critical_drtn_hr_cnt", "16");
    // 8 hours of float is within a 16 hour threshold: critical.
    let a = &mut p.model.activities[0];
    a.status = StatusType::NotStarted;
    a.total_float = Some(8 * 60);
    a.driving_path = false;
    // 24 hours of float is over it: not critical.
    let b = &mut p.model.activities[1];
    b.status = StatusType::NotStarted;
    b.total_float = Some(24 * 60);
    b.driving_path = true;
    let lookup = Lookup::new(&p);
    assert_eq!(value(&p, &lookup, 0, Column::Critical), Value::Bool(true));
    assert_eq!(value(&p, &lookup, 1, Column::Critical), Value::Bool(false));
    assert_eq!(
        exported_critical(&p, "critical-threshold.xlsx"),
        [(true, true), (false, false)]
    );
}
