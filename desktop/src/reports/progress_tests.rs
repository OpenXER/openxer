//! S-Curve and Earned Hours in the app: the numbers reconcile with the
//! Activity Usage Profile and the earned value columns; the dialogs open
//! from Tools > Reports and from their rows in the Reports view, keep their
//! defaults, close on Escape and Cancel without a change, and open the
//! report in a tab whose totals are the workbook's, which Refresh and
//! Options... run again and Export > Excel writes. On synthetic samples
//! only (the README sample and the crews sample).
use super::progress_ui::{ScurveDialog, EARNED_SALT, SCURVE_SALT};
use super::progress_view::{Figures, ProgressDoc, Spec};
use super::report_tab::{seam, Doc, Export, ReportTabId, Source as TabSource};
use super::reports_view::{build_lines, RowKey};
use super::reports_view_tests::{
    menu_item, open_view, right_click, row_of, row_point, show_toolbar,
};
use super::rpt::store;
use super::rpt_entries::{Builtin, Entry, Which};
use crate::panes::usage;
use crate::qa_sweep::Driver;
use crate::timescale::{self, Unit};
use crate::views::assignments as asg;
use crate::views::MainView;
use crate::views::ViewEdit;
use crate::*;
use openxer_xer::scurve::{self, Breakdown, Detection, Group, Measure, Options, Period, Source};

fn scheduled(doc: openxer_xer::Document) -> ProjectData {
    let mut p = ProjectData::from_document(doc, 1, "Crews".into()).unwrap();
    openxer_engine::scheduler::schedule_with_resources(&mut p.model, &p.resources).unwrap();
    p
}

fn crews() -> ProjectData {
    scheduled(openxer_xer::workspace::sample::crews(24., true))
}

fn weekly(measure: Measure) -> Options {
    Options {
        measure,
        period: Period::Week,
        breakdown: Breakdown::Resource,
        late: true,
        week_starts_sunday: true,
    }
}

/// The budgeted labor units the activity table reports, summed.
fn table_labor_budget(p: &ProjectData) -> f64 {
    let ix = crate::grid::columns::Index::new((false, 0.));
    p.model
        .activities
        .iter()
        .filter_map(|a| ix.figures(p, a.task_id, crate::grid::columns::Measure::LaborUnits))
        .map(|f| f.budget)
        .sum()
}

#[test]
fn weekly_labor_matches_the_activity_usage_profile_and_the_budget_total() {
    let (_, readme) = crate::readme_hero::sample_schedules();
    for p in [crews(), readme] {
        let c = scurve::curve(&[Source::new(&p)], weekly(Measure::Labor)).unwrap();
        // The same weeks as the usage views' Week interval over that range.
        let (first, last) = (c.periods[0].0, c.periods.last().unwrap().1);
        let cells: Vec<(i64, i64)> = timescale::cells(
            Unit::Week,
            None,
            p.model.origin_serial,
            first as f64,
            (last - 1) as f64,
        )
        .iter()
        .map(|c| (c.start_minute, c.end_minute))
        .collect();
        assert_eq!(cells, c.periods, "week boundaries");
        // Every period equals the profile's for the same periods.
        let by_task = usage::labor_assignments_in(&p, &asg::assignments(&p));
        let u = usage::activity_usage_of(&p, &by_task, &c.periods, None, false);
        for k in 0..c.periods.len() {
            for (what, mine, pane) in [
                ("budget", c.total.budget[k], u.budget[k]),
                ("actual", c.total.actual[k], u.actual[k]),
                ("remaining", c.total.remaining[k], u.remaining[k]),
                ("late", c.total.late[k], u.actual[k] + u.remaining_late[k]),
            ] {
                assert!(
                    (mine - pane).abs() < 1e-9,
                    "{what} week {k}: {mine} vs {pane}"
                );
            }
        }
        // The breakdown's grand total is the table's budgeted labor units.
        let grand: f64 = c
            .lines
            .iter()
            .map(|l| l.series.budget.iter().sum::<f64>())
            .sum();
        assert!(
            (grand - table_labor_budget(&p)).abs() < 1e-6,
            "{grand} vs {}",
            table_labor_budget(&p)
        );
    }
}

#[test]
fn earned_figures_match_the_earned_value_columns_with_a_baseline() {
    let (plan, current) = crate::readme_hero::sample_schedules();
    let loaded = openxer_xer::ev::load(
        "Plan",
        plan.updated_document(),
        plan.model.project.proj_id,
        current.model.origin_serial,
    )
    .unwrap();
    let loaded = std::sync::Arc::new(loaded);
    let mut compare = crate::baselines::Compare::default();
    compare.slots[0] = crate::baselines::data::Source::Baseline(loaded.clone());
    let mut ix = crate::grid::columns::Index::new((false, 0.));
    ix.baselines = Some(std::sync::Arc::new(compare));
    let src = Source {
        project_baseline: Some(loaded),
        ..Source::new(&current)
    };
    let e = scurve::earned_hours(&src, Group::Activity).unwrap();
    let mut columns = 0.;
    for (i, a) in current.model.activities.iter().enumerate() {
        let ev =
            crate::grid::columns_ev::parts(&current, &ix, i, openxer_xer::ev::Basis::LaborUnits).ev;
        columns += ev;
        let mine: f64 = e
            .details
            .iter()
            .filter(|d| d.activity_id == a.task_code)
            .map(|d| d.earned)
            .sum();
        assert!((mine - ev).abs() < 1e-6, "{}: {mine} vs {ev}", a.task_code);
    }
    let c = scurve::curve(std::slice::from_ref(&src), weekly(Measure::Labor)).unwrap();
    assert!((c.earned.unwrap() - columns).abs() < 1e-6);
    assert!(c.budget_source.starts_with("Baseline"));
}

fn open(d: &mut Driver, action: &str) {
    d.home();
    d.click("menu_bar_tools");
    assert!(d.hover_text("Reports"), "the Reports submenu");
    let pos = d.rect(action).center();
    d.click_at(pos);
    d.frames(2);
}

fn is_open(d: &Driver, salt: &str) -> bool {
    crate::dialog_kit::open_dialogs(&d.ctx)
        .iter()
        .any(|n| n == salt)
}

#[test]
fn the_dialogs_open_with_defaults_and_close_on_escape_and_cancel() {
    let mut d = Driver::new("scurve-dialog", crews());
    let before = d.app.project.document.text();
    open(&mut d, "menu_scurve");
    assert!(is_open(&d, SCURVE_SALT), "S-Curve opens");
    let dialog = d.app.reports.scurve.as_ref().unwrap();
    let o = dialog.options(true);
    assert_eq!(
        (o.measure, o.period, o.breakdown, o.late),
        (Measure::Labor, Period::Week, Breakdown::Resource, false)
    );
    assert_eq!(
        dialog.detection.line(Measure::Labor),
        "Labor hours found on 3 of 14 activities"
    );
    d.key(egui::Key::Escape);
    d.frames(2);
    assert!(d.app.reports.scurve.is_none(), "Escape closes it");
    open(&mut d, "menu_scurve");
    d.click("scurve_cancel");
    d.frames(2);
    assert!(d.app.reports.scurve.is_none(), "Cancel closes it");
    open(&mut d, "menu_earned_hours");
    assert!(is_open(&d, EARNED_SALT), "Earned Hours opens");
    d.key(egui::Key::Escape);
    d.frames(2);
    assert!(d.app.reports.earned_hours.is_none());
    open(&mut d, "menu_earned_hours");
    d.click("earned_hours_cancel");
    d.frames(2);
    assert!(d.app.reports.earned_hours.is_none());
    assert_eq!(d.app.project.document.text(), before, "nothing changed");
    assert!(seam::offered().is_empty(), "nothing asked where to save");
}

/// The report tab shown now.
fn shown_tab(d: &Driver) -> ReportTabId {
    d.app.current_report_tab().expect("a report tab is shown")
}

/// The built-in report document of a tab.
fn progress_doc(d: &Driver, id: ReportTabId) -> &ProgressDoc {
    match &d.app.reports.tabs.get(id).expect("the tab is open").doc {
        Doc::Progress(p) => p,
        _ => panic!("not a built-in report's tab"),
    }
}

/// The number in column `col` of the last (total) row of table `table`.
fn total_cell(p: &ProgressDoc, section: usize, table: usize, col: usize) -> f64 {
    let row = p.report.sections[section].tables[table]
        .rows
        .last()
        .expect("a total row");
    assert_eq!(row.cells[0].text(), "Total");
    match &row.cells[col] {
        crate::reports::doc::Cell::Number { value, .. } => *value,
        other => panic!("column {col} of the total row is {other:?}"),
    }
}

/// The number in column `col` of the row of `sheet` whose first cell is
/// Total.
fn sheet_total(path: &std::path::Path, sheet: &str, col: usize) -> f64 {
    let wb = openxer_interchange::xlsx_import::read_workbook(path).unwrap();
    let s = &wb.sheets[wb
        .sheet(sheet)
        .unwrap_or_else(|| panic!("no sheet {sheet}"))];
    let row = (0..s.rows.len())
        .find(|&r| s.cell(r, 0).text() == "Total")
        .expect("a Total row");
    match s.cell(row, col) {
        openxer_interchange::xlsx_import::Cell::Number(v) => *v,
        other => panic!("{sheet} total column {col} is {other:?}"),
    }
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6 * (1. + a.abs().max(b.abs()))
}

#[test]
fn ok_opens_the_report_in_a_tab_and_export_writes_the_workbooks() {
    let mut d = Driver::new("scurve-save", crews());
    open(&mut d, "menu_scurve");
    d.click("scurve_ok");
    d.frames(2);
    assert!(d.app.reports.scurve.is_none());
    assert!(seam::offered().is_empty(), "viewing first: no Save As");
    let id = shown_tab(&d);
    assert_eq!(d.app.reports.tabs.get(id).unwrap().name, "S-Curve");
    let pages = &d.app.reports.tabs.get(id).unwrap().pages;
    assert!(!pages.is_empty());
    // The chart is on the first page: the shared painter's legend and the
    // scale captions.
    let first = pages[0].texts().join("|");
    for text in [
        "Budgeted per period",
        "Cum Actual (right scale)",
        "Hours per week",
        "Cumulative hours",
        "Period Start",
    ] {
        assert!(first.contains(text), "{text} on page 1: {first}");
    }
    // Export > Excel asks where to save and writes the workbook.
    let curve = d.dir.join("curve.xlsx");
    seam::answer(Some(curve.clone()));
    d.app.export_report_tab(id, Export::Excel);
    let offered = seam::offered();
    assert_eq!(offered.len(), 1);
    assert!(offered[0].1.ends_with(".xlsx"), "{:?}", offered[0].1);
    assert!(offered[0].1.contains("S-Curve"), "{:?}", offered[0].1);
    let wb = openxer_interchange::xlsx_import::read_workbook(&curve).unwrap();
    let names: Vec<&str> = wb.sheets.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names[0], "S-Curve");
    assert!(names.contains(&"By Resource - Budgeted"));
    // The on-screen totals are the workbook's.
    let p = progress_doc(&d, id);
    for (col, what) in [(2, "budgeted"), (3, "actual"), (4, "remaining")] {
        let screen = total_cell(p, 0, 0, col);
        let book = sheet_total(&curve, "S-Curve", col);
        assert!(close(screen, book), "{what}: {screen} vs {book}");
    }
    let screen = total_cell(p, 1, 0, 2);
    let book = sheet_total(
        &curve,
        "By Resource - Budgeted",
        2 + {
            let Figures::Curve(c) = &p.figures else {
                unreachable!()
            };
            c.periods.len()
        },
    );
    assert!(close(screen, book), "breakdown: {screen} vs {book}");
    // The workbook is the one the export wrote before: the same writer on
    // the same figures.
    let options = match d.app.reports.tabs.get(id).unwrap().source {
        TabSource::Progress(Spec::Scurve { options, .. }) => options,
        _ => unreachable!(),
    };
    let fresh = d
        .app
        .with_report_sources(false, |s| scurve::curve(s, options))
        .unwrap();
    let Figures::Curve(shown) = &progress_doc(&d, id).figures else {
        unreachable!()
    };
    assert_eq!(**shown, fresh);

    let earned = d.dir.join("earned.xlsx");
    open(&mut d, "menu_earned_hours");
    d.click("earned_hours_ok");
    d.frames(2);
    let id = shown_tab(&d);
    assert_eq!(d.app.reports.tabs.get(id).unwrap().name, "Earned Hours");
    seam::answer(Some(earned.clone()));
    d.app.export_report_tab(id, Export::Excel);
    assert_eq!(seam::offered().len(), 1);
    let wb = openxer_interchange::xlsx_import::read_workbook(&earned).unwrap();
    assert_eq!(wb.sheets[0].name, "Earned to date");
    let p = progress_doc(&d, id);
    for col in 2..=6 {
        let screen = total_cell(p, 0, 0, col);
        let book = sheet_total(&earned, "Earned to date", col);
        assert!(close(screen, book), "column {col}: {screen} vs {book}");
    }
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

#[test]
fn a_project_without_labor_gets_a_message_and_no_tab() {
    let mut d = Driver::new("earned-none", openxer_xer::starter());
    open(&mut d, "menu_earned_hours");
    d.click("earned_hours_ok");
    d.frames(2);
    assert_eq!(d.app.error.as_deref(), Some(scurve::NO_LABOR));
    assert!(seam::offered().is_empty(), "no Save As");
    assert!(d.app.reports.tabs.ids().is_empty(), "no tab");
    // The starter has no units or costs: the S-curve offers duration only.
    let detection = d.app.with_report_sources(false, Detection::of);
    let dialog = ScurveDialog::new(detection);
    assert_eq!(dialog.measure(), Measure::Duration);
    assert_eq!(dialog.options(true).breakdown, Breakdown::Total);
}

#[test]
fn refresh_recomputes_and_options_reopen_the_dialog_for_the_same_tab() {
    let mut d = Driver::new("scurve-refresh", crews());
    open(&mut d, "menu_scurve");
    d.click("scurve_ok");
    d.frames(2);
    let id = shown_tab(&d);
    let before = total_cell(progress_doc(&d, id), 0, 0, 2);
    // An edit: one more labor hour budgeted on every labor assignment.
    let labor = openxer_xer::spread::labor_resources(&d.app.project.document);
    let t = d.app.project.document.table_mut("TASKRSRC");
    let mut added = 0.;
    for r in 0..t.rows.len() {
        let rsrc: i64 = t.value(&t.rows[r], "rsrc_id").trim().parse().unwrap_or(0);
        if labor.contains(&rsrc) {
            let q: f64 = t
                .value(&t.rows[r], "target_qty")
                .trim()
                .parse()
                .unwrap_or(0.);
            t.set(r, "target_qty", (q + 10.).to_string());
            added += 10.;
        }
    }
    assert!(added > 0.);
    d.frames(2);
    assert!(d.has("report_tab_refresh"));
    d.click("report_tab_refresh");
    d.frames(2);
    let after = total_cell(progress_doc(&d, id), 0, 0, 2);
    assert!(
        close(after, before + added),
        "{before} + {added} vs {after}"
    );
    // Options... reopens the dialog with the tab's choices; OK runs it again
    // in the same tab.
    assert!(d.has("report_tab_options"));
    d.click("report_tab_options");
    d.frames(2);
    let dialog = d.app.reports.scurve.as_ref().expect("the dialog opens");
    assert_eq!(dialog.tab, Some(id));
    assert_eq!(dialog.options(true).breakdown, Breakdown::Resource);
    d.app.reports.scurve.as_mut().unwrap().late = true;
    d.click("scurve_ok");
    d.frames(2);
    assert_eq!(d.app.reports.tabs.ids(), vec![id], "the same tab");
    let Figures::Curve(c) = &progress_doc(&d, id).figures else {
        unreachable!()
    };
    assert!(c.options.late, "the new options");
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

/// The Reports view's rows that are built-in reports, with their band.
fn builtin_rows(d: &Driver) -> Vec<(String, String)> {
    let lines = build_lines(
        &d.app.report_list(),
        &d.app.report_groups(),
        &d.app.views.reports,
    );
    let mut band = String::new();
    let mut out = vec![];
    for l in &lines {
        if l.row.band {
            band = l.row.cells[0].clone();
        } else if let RowKey::Report(Which::Builtin(_)) = l.key {
            out.push((band.clone(), l.row.cells[0].clone()));
        }
    }
    out
}

#[test]
fn the_built_in_reports_are_listed_once_in_scheduling_reports() {
    // A fresh workspace.
    let mut d = Driver::new("builtin-rows", crews());
    d.click("dock_reports");
    d.frames(2);
    let want = vec![
        (store::LIBRARY_GROUP.to_string(), "Earned Hours".to_string()),
        (store::LIBRARY_GROUP.to_string(), "S-Curve".to_string()),
    ];
    let mut rows = builtin_rows(&d);
    rows.sort();
    assert_eq!(rows, want);
    // Sorted by name with the library reports, under their band.
    let lines = build_lines(
        &d.app.report_list(),
        &d.app.report_groups(),
        &d.app.views.reports,
    );
    let names: Vec<String> = lines
        .iter()
        .skip(1)
        .take_while(|l| !l.row.band)
        .map(|l| l.row.cells[0].to_lowercase())
        .collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted);
    assert_eq!(
        names.len(),
        super::rpt::library::entries().len() + Builtin::ALL.len()
    );
    // A built-in row reads like the others: its scope, its icon.
    let row = lines
        .iter()
        .find(|l| l.key == RowKey::Report(Which::Builtin(Builtin::Scurve)))
        .unwrap();
    assert_eq!(row.row.cells[1], "Global");
    assert_eq!(
        row.row.icon,
        Some(crate::chrome::Icon::ActivityUsageProfile)
    );
    // Nothing was stored for them.
    assert_eq!(
        d.app.stored_reports().len(),
        super::rpt::library::entries().len()
    );

    // A workspace from before: the library already loaded, started twice.
    let dir = super::test_support::temp("builtin-upgrade");
    let path = dir.join("workspace.db");
    {
        let old = crate::store::Store::open(path.clone()).unwrap();
        store::ensure(old.connection()).unwrap();
        store::load_library(old.connection()).unwrap();
    }
    for start in 0..2 {
        let ctx = egui::Context::default();
        let app = App::new(
            &ctx,
            crate::store::Store::open(path.clone()).unwrap(),
            crews(),
        );
        let listed: Vec<Entry> = app
            .report_list()
            .into_iter()
            .filter(Entry::is_builtin)
            .collect();
        assert_eq!(listed.len(), 2, "start {start}: {listed:?}");
        let lib = app
            .report_groups()
            .iter()
            .find(|g| g.name == store::LIBRARY_GROUP)
            .map(|g| g.id);
        assert!(listed.iter().all(|e| e.group == lib && lib.is_some()));
        assert_eq!(
            app.stored_reports().len(),
            super::rpt::library::entries().len(),
            "start {start}: nothing stored"
        );
    }
}

/// Selects the built-in report's row with a click.
fn select_builtin(d: &mut Driver, b: Builtin) -> RowKey {
    let key = RowKey::Report(Which::Builtin(b));
    let row = row_of(d, &key);
    d.click_at(row_point(d, row));
    d.frames(2);
    assert_eq!(d.app.views.reports.selected.as_ref(), Some(&key));
    key
}

#[test]
fn running_a_built_in_row_opens_its_dialog_and_then_its_tab() {
    let mut d = Driver::new("builtin-run", crews());
    open_view(&mut d);
    show_toolbar(&mut d);
    // Double click.
    let key = select_builtin(&mut d, Builtin::Scurve);
    let row = row_of(&d, &key);
    d.double_click_at(row_point(&d, row));
    d.frames(2);
    assert!(is_open(&d, SCURVE_SALT), "double click opens S-Curve");
    d.key(egui::Key::Escape);
    d.frames(2);
    assert!(d.app.reports.scurve.is_none());
    // The toolbar's Run Report.
    d.click("tool_run_report");
    d.frames(2);
    assert!(is_open(&d, SCURVE_SALT), "Run Report opens S-Curve");
    d.click("scurve_cancel");
    d.frames(2);
    // The right-click Run > Report and Run > Report to File.
    for item in ["reports_menu_run_report", "reports_menu_run_to_file"] {
        right_click(&mut d, &key);
        assert!(d.hover_text("Run"));
        assert!(menu_item(&mut d, item), "{item} is enabled");
        assert!(is_open(&d, SCURVE_SALT), "{item} opens S-Curve");
        d.click("scurve_cancel");
        d.frames(2);
    }
    assert!(d.app.reports.run.is_none(), "not the definition run dialog");
    // OK: the report in a tab, and the row's Last Run Date.
    d.app.show_view(MainView::Reports);
    d.frames(2);
    let row = row_of(&d, &key);
    d.double_click_at(row_point(&d, row));
    d.frames(2);
    d.click("scurve_ok");
    d.frames(2);
    let id = shown_tab(&d);
    assert_eq!(d.app.reports.tabs.get(id).unwrap().name, "S-Curve");
    let listed = d
        .app
        .report_list()
        .into_iter()
        .find(|e| e.which == Which::Builtin(Builtin::Scurve))
        .unwrap();
    assert!(listed.last_run.is_some(), "the run is recorded");
    // Earned Hours the same way.
    d.app.show_view(MainView::Reports);
    d.frames(2);
    select_builtin(&mut d, Builtin::EarnedHours);
    d.click("tool_run_report");
    d.frames(2);
    assert!(is_open(&d, EARNED_SALT), "Run Report opens Earned Hours");
    d.click("earned_hours_ok");
    d.frames(2);
    let id = shown_tab(&d);
    assert_eq!(d.app.reports.tabs.get(id).unwrap().name, "Earned Hours");
    assert_eq!(d.app.reports.tabs.ids().len(), 2);
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
}

#[test]
fn the_definition_commands_are_off_for_a_built_in_row() {
    let mut d = Driver::new("builtin-off", crews());
    open_view(&mut d);
    show_toolbar(&mut d);
    let key = select_builtin(&mut d, Builtin::Scurve);
    let stored = d.app.stored_reports().len();
    // The toolbar: Modify and Export do nothing.
    let baseline = d.keys();
    for off in ["tool_modify_report", "tool_export_report"] {
        d.click(off);
        d.frames(2);
        assert_eq!(d.keys(), baseline, "{off} is off");
        assert!(d.app.reports.editor.is_none() && d.app.reports.wizard.is_none());
        assert!(d.app.reports.export.is_none());
    }
    // The Edit commands.
    for edit in [ViewEdit::Delete, ViewEdit::Cut, ViewEdit::Copy] {
        assert_eq!(d.app.view_edit_enabled(edit), Some(false), "{edit:?}");
    }
    // The right-click menu.
    for off in [
        "reports_menu_modify",
        "reports_menu_export",
        "reports_menu_delete",
        "reports_menu_cut",
        "reports_menu_copy",
        "reports_menu_report_group",
    ] {
        right_click(&mut d, &key);
        assert!(
            !menu_item(&mut d, off),
            "{off} is off for a built-in report"
        );
        d.key(egui::Key::Escape);
        d.frames(2);
    }
    // Commands sent anyway change nothing.
    for command in ["delete", "cut", "copy", "modify", "export", "scope_project"] {
        d.app.views.reports.selected = Some(key.clone());
        d.app.reports_command(command);
        d.frames(2);
    }
    assert!(d.app.reports.clipboard.is_none());
    assert_eq!(d.app.stored_reports().len(), stored);
    assert_eq!(
        builtin_rows(&d).len(),
        2,
        "both rows are still listed after Delete"
    );
    // A batch holds saved reports only: the batch dialogs do not list them.
    assert!(d.app.report_entries().iter().all(|e| !e.is_builtin()));
    d.assert_clean("built-in row");
}

/// Renders both tabs to the ignored `artifacts/progress-tabs/` for review.
#[test]
#[ignore]
fn render_progress_tabs() {
    let out = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/progress-tabs");
    std::fs::create_dir_all(&out).unwrap();
    let mut d = Driver::probed(
        "progress-render",
        crews(),
        egui::Vec2::new(1500., 950.),
        Some(out.clone()),
    );
    d.home();
    open_view(&mut d);
    show_toolbar(&mut d);
    select_builtin(&mut d, Builtin::Scurve);
    d.frames(2);
    d.screenshot(&out.join("01-reports-view.png"));
    d.click("tool_run_report");
    d.frames(2);
    d.click("scurve_ok");
    d.frames(4);
    d.screenshot(&out.join("02-scurve-tab.png"));
    let id = shown_tab(&d);
    assert!(d.click_text("Last Page"));
    d.frames(4);
    d.screenshot(&out.join("02b-scurve-page-2.png"));
    let bytes = d
        .app
        .reports
        .tabs
        .get(id)
        .unwrap()
        .bytes(Export::Pdf, &Default::default())
        .unwrap();
    std::fs::write(out.join("02-scurve.pdf"), bytes).unwrap();
    open(&mut d, "menu_earned_hours");
    d.click("earned_hours_ok");
    d.frames(4);
    d.screenshot(&out.join("03-earned-hours-tab.png"));
}
