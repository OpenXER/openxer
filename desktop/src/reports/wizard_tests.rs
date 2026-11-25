//! Report Wizard: definitions round trip through the preferences, each
//! subject area builds the rows it should, every output format writes, and
//! the wizard pages, saved report list and preview work and render
//! (gitignored `artifacts/g3/`).
use super::doc::{self, Output, Tone};
use super::test_support::{app, artifacts, crop_dialog, scheduled_starter, temp};
use super::wizard::{self, Context, Definition, Subject};
use super::{html, pdf};
use crate::grid::columns::Index;
use crate::grid::layout::SortKey;
use crate::headless_render::SoftwarePainter;
use openxer_xer::ProjectData;

fn build(def: &Definition, p: &ProjectData) -> super::doc::Report {
    let ix = Index::new((false, 0.));
    wizard::build(
        def,
        p,
        &Context {
            ix: &ix,
            users: &[],
        },
    )
}

fn activity_ids(r: &super::doc::Report) -> Vec<String> {
    r.sections[0].tables[0]
        .rows
        .iter()
        .filter(|row| row.tone == Tone::Normal)
        .map(|row| row.cells[0].text().to_string())
        .collect()
}

#[test]
fn definitions_round_trip_through_json() {
    let mut a = Definition::new(Subject::Activities);
    a.name = "Milestones".into();
    a.filters = vec!["std:milestone".into()];
    a.contains = "handover".into();
    a.group_by = Some("code:Area".into());
    a.sort = vec![SortKey {
        column: "finish".into(),
        descending: true,
    }];
    a.show_totals = true;
    a.output = Some(Output::Pdf);
    a.path = "C:\\Reports\\m.pdf".into();
    a.header = "Weekly".into();
    a.footer = "Contractor copy".into();
    let mut b = Definition::new(Subject::Relationships);
    b.name = "Logic".into();
    let text = wizard::definitions_to_json(&[a.clone(), b.clone()]);
    assert_eq!(wizard::definitions_from_json(&text), vec![a, b]);
    // A broken entry is skipped, not fatal.
    let mixed = r#"[{"subject":"nope","columns":["x"]},{"name":"ok","subject":"resources","columns":["rsrc_id"]}]"#;
    let list = wizard::definitions_from_json(mixed);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "ok");
    assert!(wizard::definitions_from_json("not json").is_empty());
}

#[test]
fn activity_reports_filter_group_sort_and_total() {
    let p = scheduled_starter();
    let mut def = Definition::new(Subject::Activities);
    let all = build(&def, &p);
    assert_eq!(activity_ids(&all).len(), 14);
    assert!(
        all.sections[0].tables[0]
            .rows
            .iter()
            .any(|r| matches!(r.tone, Tone::Band(_))),
        "grouped by WBS"
    );
    def.filters = vec!["std:milestone".into()];
    def.group_by = None;
    def.sort = vec![SortKey {
        column: "activity_id".into(),
        descending: true,
    }];
    let r = build(&def, &p);
    assert_eq!(activity_ids(&r), vec!["RB-1130", "RB-1030", "RB-1000"]);
    assert_eq!(r.sections[0].paragraphs[0], "3 activities");
    assert_eq!(r.sections[0].paragraphs[1], "Filter: Milestone");
    def.filters.clear();
    def.contains = "install".into();
    let r = build(&def, &p);
    assert_eq!(activity_ids(&r), vec!["RB-1090", "RB-1080"]);
    def.contains.clear();
    def.show_totals = true;
    let r = build(&def, &p);
    let first = &r.sections[0].tables[0].rows[0];
    assert_eq!(first.tone, Tone::Band(0));
    assert_eq!(first.cells[0].text(), "Total");
    // Typed cells: durations are numbers, dates real dates.
    let row = r.sections[0].tables[0]
        .rows
        .iter()
        .find(|r| r.tone == Tone::Normal)
        .unwrap();
    assert!(matches!(row.cells[2], doc::Cell::Number { .. }));
    assert!(matches!(row.cells[4], doc::Cell::Date { .. }));
}

#[test]
fn other_subject_areas() {
    let p = crate::views::resources::tests::with_resource();
    let r = build(&Definition::new(Subject::Resources), &p);
    let rows = &r.sections[0].tables[0].rows;
    assert!(rows.iter().any(|row| row.cells[0].text() == "PIPE"));
    let mut def = Definition::new(Subject::Assignments);
    def.show_totals = true;
    let r = build(&def, &p);
    let rows = &r.sections[0].tables[0].rows;
    assert_eq!(rows[0].cells[0].text(), "Total");
    assert!(rows
        .iter()
        .any(|row| row.cells[0].text() == "RB-1010" && row.cells[2].text() == "PIPE"));
    let p = scheduled_starter();
    let mut def = Definition::new(Subject::Relationships);
    def.group_by = Some("type".into());
    let r = build(&def, &p);
    let rows = &r.sections[0].tables[0].rows;
    assert_eq!(rows.iter().filter(|r| r.tone == Tone::Normal).count(), 16);
    assert_eq!(rows[0].cells[0].text(), "FS");
    def.contains = "RB-1130".into();
    def.group_by = None;
    let r = build(&def, &p);
    assert_eq!(r.sections[0].tables[0].rows.len(), 1);
}

#[test]
fn reports_write_every_format() {
    let p = scheduled_starter();
    let def = Definition::new(Subject::Activities);
    let r = build(&def, &p);
    let dir = temp("wizard-formats");
    for format in Output::ALL {
        let path = dir.join(format!("r.{}", format.extension()));
        doc::write(&r, format, &path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        match format {
            Output::Html => html::well_formed(&String::from_utf8(bytes).unwrap()).unwrap(),
            Output::Pdf => assert_eq!(pdf::check(&bytes).unwrap(), 1),
            Output::Excel => {
                let wb = openxer_interchange::xlsx_import::read_workbook(&path).unwrap();
                let s = &wb.sheets[wb.sheet("Activities").unwrap()];
                assert!(s
                    .rows
                    .iter()
                    .any(|r| r.first().is_some_and(|c| c.text() == "RB-1000")));
            }
            Output::Csv => assert!(String::from_utf8(bytes).unwrap().contains("RB-1130")),
        }
    }
}

#[test]
fn wizard_pages_save_rerun_and_render() {
    let dir = temp("wizard-ui");
    let (mut app, ctx) = app(&dir);
    let mut painter = SoftwarePainter::default();
    let mut time = 0.;
    for _ in 0..2 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    painter.click_action(&mut app, &ctx, &mut time, "menu_bar_tools");
    painter.click_action(&mut app, &ctx, &mut time, "menu_report_wizard");
    assert!(app.reports.wizard.is_some());
    let out = artifacts();
    let shot = |painter: &mut SoftwarePainter,
                app: &mut crate::App,
                time: &mut f64,
                name: &str,
                salt: &str| {
        let _ = painter.frame(app, &ctx, time, vec![], false);
        let (image, _) = painter.frame(app, &ctx, time, vec![], true).unwrap();
        crop_dialog(&image, &ctx, salt)
            .save(out.join(name))
            .unwrap();
    };
    shot(
        &mut painter,
        &mut app,
        &mut time,
        "report-wizard-1-subject.png",
        "report_wizard",
    );
    painter.click_action(&mut app, &ctx, &mut time, "wizard_subject_relationships");
    painter.click_action(&mut app, &ctx, &mut time, "wizard_subject_activities");
    painter.click_action(&mut app, &ctx, &mut time, "wizard_next");
    assert_eq!(app.reports.wizard.as_ref().unwrap().page, 1);
    shot(
        &mut painter,
        &mut app,
        &mut time,
        "report-wizard-2-columns.png",
        "report_wizard",
    );
    // Add the first available column, then remove it again.
    painter.click_action(&mut app, &ctx, &mut time, "wizard_available_row_0");
    painter.click_action(&mut app, &ctx, &mut time, "wizard_add");
    assert_eq!(app.reports.wizard.as_ref().unwrap().def.columns.len(), 8);
    painter.click_action(&mut app, &ctx, &mut time, "wizard_selected_row_7");
    painter.click_action(&mut app, &ctx, &mut time, "wizard_remove");
    assert_eq!(app.reports.wizard.as_ref().unwrap().def.columns.len(), 7);
    painter.click_action(&mut app, &ctx, &mut time, "wizard_next");
    app.reports.wizard.as_mut().unwrap().def.title = "Three Week Lookahead".into();
    shot(
        &mut painter,
        &mut app,
        &mut time,
        "report-wizard-3-title.png",
        "report_wizard",
    );
    painter.click_action(&mut app, &ctx, &mut time, "wizard_next");
    app.reports.wizard.as_mut().unwrap().save_name = "Lookahead".into();
    shot(
        &mut painter,
        &mut app,
        &mut time,
        "report-wizard-4-run.png",
        "report_wizard",
    );
    painter.click_action(&mut app, &ctx, &mut time, "wizard_finish");
    assert!(app.reports.wizard.is_none(), "{:?}", app.status);
    let saved = app.report_definitions();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].name, "Lookahead");
    assert_eq!(saved[0].title, "Three Week Lookahead");
    let ids = app.reports.tabs.ids();
    assert_eq!(ids.len(), 1, "Screen output opens a report tab");
    let tab = app.reports.tabs.get(ids[0]).unwrap();
    assert_eq!(tab.name, "Lookahead");
    match &tab.doc {
        crate::reports::report_tab::Doc::Sheet(report) => {
            assert_eq!(report.title, "Three Week Lookahead")
        }
        _ => panic!("a wizard report is a sheet"),
    }
    for _ in 0..3 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    image.save(out.join("report-preview.png")).unwrap();
    app.close_tab(crate::views::MainView::Report(ids[0]));
    assert!(app.reports.tabs.ids().is_empty());
    // Tools > Reports > Reports: rerun to a file, then modify.
    let mut def = saved[0].clone();
    def.output = Some(Output::Html);
    def.path = dir.join("rerun.html").display().to_string();
    app.save_report_definitions(&[def]);
    app.show_view(crate::views::MainView::Reports);
    for _ in 0..2 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    // The wizard report sits below the starter library's group (the rows
    // the view draws: saved and built-in reports).
    let entries = app.report_list();
    let lines = crate::reports::reports_view::build_lines(
        &entries,
        &app.report_groups(),
        &app.views.reports,
    );
    let row = lines
        .iter()
        .position(|l| {
            l.key
                == crate::reports::reports_view::RowKey::Report(
                    crate::reports::rpt_entries::Which::Wizard(0),
                )
        })
        .unwrap();
    painter.click_action(&mut app, &ctx, &mut time, &format!("reports_row_{row}"));
    app.reports_command("run_to_file");
    {
        let run = app.reports.run.as_mut().unwrap();
        run.target = crate::reports::rpt_run::Target::Html;
        run.path = dir.join("rerun.html").display().to_string();
    }
    for _ in 0..3 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    painter.click_action(&mut app, &ctx, &mut time, "run_report_run");
    html::well_formed(&std::fs::read_to_string(dir.join("rerun.html")).unwrap()).unwrap();
    assert!(
        app.report_entries()[0].last_run.is_some(),
        "the wizard report records its run"
    );
    app.reports_command("modify");
    let w = app.reports.wizard.as_ref().unwrap();
    assert_eq!((w.page, w.editing), (1, Some(0)));
    assert_eq!(w.def.title, "Three Week Lookahead");
    // Use Current Screen builds from the activity layout.
    app.reports.wizard = None;
    app.open_report_wizard();
    app.reports.wizard.as_mut().unwrap().start = 1;
    app.wizard_go(1);
    let cols = app.reports.wizard.as_ref().unwrap().def.columns.clone();
    let layout: Vec<String> = app
        .grid
        .layout
        .columns
        .iter()
        .map(|c| c.id.clone())
        .collect();
    assert_eq!(cols, layout);
}

/// Reports go through the print module's PDF writer: Letter landscape,
/// the two Helvetica faces, the title, cells and page footer as text, and
/// row marks as filled rectangles.
#[test]
fn report_pdf_uses_the_print_writer() {
    let p = scheduled_starter();
    let mut def = Definition::new(Subject::Activities);
    def.title = "Weekly (report) \u{2013} \u{2192} check".into();
    let r = build(&def, &p);
    let bytes = pdf::write(&r);
    assert_eq!(pdf::check(&bytes).unwrap(), 1);
    let has = |hay: &[u8], needle: &[u8]| hay.windows(needle.len()).any(|w| w == needle);
    assert!(has(&bytes, b"/MediaBox [0 0 792 612]"));
    assert!(has(&bytes, b"/BaseFont /Helvetica-Bold"));
    assert!(has(&bytes, b"/FlateDecode"));
    let at = bytes.windows(7).position(|w| w == b"stream\n").unwrap() + 7;
    let end = at
        + bytes[at..]
            .windows(10)
            .position(|w| w == b"\nendstream")
            .unwrap();
    let content = miniz_oxide::inflate::decompress_to_vec_zlib(&bytes[at..end]).unwrap();
    // Balanced parentheses stay unescaped in a PDF string literal.
    assert!(has(&content, b"(Weekly (report) - -> check) Tj"));
    assert!(has(&content, b"(RB-1000) Tj"));
    assert!(has(&content, b"(Page 1 of 1) Tj"));
    assert!(has(&content, b" re\n"));
}

/// A duration on an activity whose calendar has 10-hour days is exported as
/// the same number of days its text shows, not project-calendar days.
#[test]
fn duration_numbers_use_the_activity_calendar() {
    let mut doc = scheduled_starter().updated_document();
    let cal = doc.table("CALENDAR").unwrap().rows[0].clone();
    let t = doc.table_mut("CALENDAR");
    t.rows.push(cal);
    let last = t.rows.len() - 1;
    t.set(last, "clndr_id", "2".into());
    t.set(last, "clndr_name", "Ten hour days".into());
    t.set(last, "day_hr_cnt", "10".into());
    t.set(last, "default_flag", "N".into());
    let tasks = doc.table_mut("TASK");
    let r = tasks
        .rows
        .iter()
        .position(|r| tasks.value(r, "task_code") == "RB-1020")
        .unwrap();
    tasks.set(r, "clndr_id", "2".into());
    tasks.set(r, "target_drtn_hr_cnt", "60".into());
    tasks.set(r, "remain_drtn_hr_cnt", "60".into());
    let p = ProjectData::from_document(doc, 1, "ten".into()).unwrap();
    let mut def = Definition::new(Subject::Activities);
    def.group_by = None;
    let r = build(&def, &p);
    let row = r.sections[0].tables[0]
        .rows
        .iter()
        .find(|r| r.cells[0].text() == "RB-1020")
        .unwrap();
    let mut seen = 0;
    for cell in &row.cells {
        if let doc::Cell::Number { value, text } = cell {
            if let Some(days) = text.strip_suffix('d') {
                assert_eq!(*value, days.parse::<f64>().unwrap(), "{text}");
                seen += 1;
            }
        }
    }
    assert!(seen > 0, "{:?}", row.cells);
}
