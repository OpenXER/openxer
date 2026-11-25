//! Headless tests of the Reports window, Run Report, the editor and
//! Properties, import and export, and batch runs. They drive the production
//! UI through `qa_sweep::Driver` (synthetic egui input only).
use super::groups;
use super::rpt::store;
use super::test_support::{scheduled_starter, temp};
use crate::qa_sweep::Driver;
use eframe::egui::{self, Key};

fn ctrl() -> egui::Modifiers {
    egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    }
}

fn driver(name: &str) -> Driver {
    let mut d = Driver::new(name, scheduled_starter());
    d.home();
    d
}

/// Opens Tools > Reports > Reports (the Reports view) and shows the Reports
/// toolbar, whose buttons run the commands below.
fn open_reports(d: &mut Driver) {
    d.click("menu_bar_tools");
    assert!(d.hover_text("Reports"), "the Reports submenu");
    let pos = d.rect("menu_reports").center();
    d.click_at(pos);
    d.frames(2);
    assert_eq!(d.app.views.main, crate::views::MainView::Reports);
    d.app.views.shown_toolbars.insert("Reports".into());
    d.frames(2);
}

fn type_into(d: &mut Driver, action: &str, text: &str) {
    d.click(action);
    d.key_with(Key::A, ctrl());
    d.type_text(text);
    d.frames(2);
}

/// Clicks the Reports view row of a saved report by name.
fn pick(d: &mut Driver, name: &str) {
    let entries = d.app.report_list();
    let lines =
        super::reports_view::build_lines(&entries, &d.app.report_groups(), &d.app.views.reports);
    let row = lines
        .iter()
        .position(|l| {
            matches!(&l.key, super::reports_view::RowKey::Report(w)
                if entries.iter().any(|e| &e.which == w && e.name == name))
        })
        .unwrap_or_else(|| panic!("{name} is not listed"));
    let r = d.rect(&format!("reports_row_{row}"));
    d.click_at(egui::Pos2::new(r.left() + 120., r.center().y));
}

/// Right-clicks the row of a saved report and returns the rectangle of the
/// menu item `item`'s key after it opens.
fn context_menu(d: &mut Driver, name: &str) {
    pick(d, name);
    let entries = d.app.report_list();
    let lines =
        super::reports_view::build_lines(&entries, &d.app.report_groups(), &d.app.views.reports);
    let row = lines
        .iter()
        .position(|l| {
            matches!(&l.key, super::reports_view::RowKey::Report(w)
                if entries.iter().any(|e| &e.which == w && e.name == name))
        })
        .unwrap();
    let r = d.rect(&format!("reports_row_{row}"));
    d.press_at(
        egui::Pos2::new(r.left() + 120., r.center().y),
        egui::PointerButton::Secondary,
        Default::default(),
    );
    d.frames(2);
}

#[test]
fn a_new_workspace_holds_the_starter_library_and_deleted_reports_stay_deleted() {
    let dir = temp("r1-library");
    let (app, _) = super::test_support::app(&dir);
    let stored = app.stored_reports();
    assert_eq!(stored.len(), super::rpt::library::entries().len());
    assert!(stored.iter().all(|s| s.kind == store::KIND_LIBRARY));
    let groups = app.report_groups();
    let lib = groups
        .iter()
        .find(|g| g.name == store::LIBRARY_GROUP)
        .unwrap();
    assert!(stored.iter().all(|s| s.group == Some(lib.id)));
    // Delete one, reopen the same workspace: it does not come back.
    store::delete(app.store.connection(), stored[0].id).unwrap();
    drop(app);
    let again = crate::store::Store::open(dir.join("workspace.db")).unwrap();
    assert_eq!(
        store::list(again.connection()).unwrap().len(),
        stored.len() - 1
    );
}

#[test]
fn run_report_writes_html_text_and_pdf_and_opens_a_tab() {
    let mut d = driver("r1-run");
    let model = d.app.project.model.clone();
    open_reports(&mut d);
    pick(&mut d, "Critical Activities");
    d.app.reports_command("run_to_file");
    d.frames(2);
    assert!(d.app.reports.run.is_some());
    // HTML.
    d.click("run_report_target_0");
    let html = d.dir.join("critical.html");
    type_into(&mut d, "run_report_path", &html.display().to_string());
    d.click("run_report_run");
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
    assert!(d.app.reports.run.is_none(), "the dialog closes after a run");
    let text = std::fs::read_to_string(&html).unwrap();
    assert!(text.contains("Critical Activities"));
    assert!(text.contains("RB-"));
    // Text with a comma delimiter and a single quote qualifier.
    d.app.reports_command("run_to_file");
    d.click("run_report_target_1");
    let txt = d.dir.join("critical.txt");
    type_into(&mut d, "run_report_path", &txt.display().to_string());
    d.click("run_report_delimiter");
    assert!(d.click_text("Comma"));
    d.click("run_report_qualifier");
    assert!(d.click_text("Single quote"));
    d.click("run_report_notes");
    d.type_text("my notes");
    d.click("run_report_run");
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
    let text = std::fs::read_to_string(&txt).unwrap();
    assert!(
        text.lines()
            .any(|l| l.starts_with("'RB-") && l.contains("','")),
        "{text}"
    );
    // PDF.
    d.app.reports_command("run_to_file");
    d.click("run_report_target_2");
    let pdf = d.dir.join("critical.pdf");
    type_into(&mut d, "run_report_path", &pdf.display().to_string());
    d.click("run_report_run");
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
    assert!(crate::print::pdf::check(&std::fs::read(&pdf).unwrap()).unwrap() >= 1);
    // A report tab.
    d.click("tool_run_report");
    assert!(d.app.reports.run.is_none());
    let ids = d.app.reports.tabs.ids();
    assert_eq!(ids.len(), 1);
    let tab = d.app.reports.tabs.get(ids[0]).unwrap();
    assert!(!tab.pages.is_empty());
    assert_eq!(d.app.views.main, crate::views::MainView::Report(ids[0]));
    d.click("report_tab_zoom_in");
    assert!(d.app.reports.tabs.get(ids[0]).unwrap().zoom.is_some());
    d.key_with(Key::F4, egui::Modifiers::COMMAND);
    d.frames(2);
    assert!(
        d.app.reports.tabs.ids().is_empty(),
        "Ctrl+F4 closes the tab"
    );
    assert_eq!(d.app.views.main, crate::views::MainView::Reports);
    assert!(d.app.project.model == model);
    assert!(!d.app.dirty);
}

#[test]
fn cancel_and_escape_leave_everything_as_it_was() {
    let mut d = driver("r1-cancel");
    let before = d.app.stored_reports();
    let model = d.app.project.model.clone();
    open_reports(&mut d);
    pick(&mut d, "Milestones");
    // Run Report to File: Cancel, then Escape.
    d.app.reports_command("run_to_file");
    d.click("run_report_cancel");
    assert!(d.app.reports.run.is_none());
    d.app.reports_command("run_to_file");
    d.key(Key::Escape);
    d.frames(2);
    assert!(d.app.reports.run.is_none());
    assert_eq!(d.app.views.main, crate::views::MainView::Reports);
    // Import and Export dialogs.
    d.click("tool_import_report");
    assert!(d.app.reports.import.is_some());
    d.click("import_reports_cancel");
    d.click("tool_import_report");
    d.key(Key::Escape);
    d.frames(2);
    assert!(d.app.reports.import.is_none());
    d.click("tool_export_report");
    assert!(d.app.reports.export.is_some());
    d.click("export_report_cancel");
    d.click("tool_export_report");
    d.key(Key::Escape);
    d.frames(2);
    assert!(d.app.reports.export.is_none());
    // The editor: change something, Cancel discards it.
    d.click("tool_modify_report");
    assert!(d.app.reports.editor.is_some());
    d.click("report_editor_add_row");
    assert!(d.app.reports.editor.as_ref().unwrap().dirty);
    d.click("report_editor_cancel");
    assert!(d.app.reports.editor.is_none());
    d.click("tool_modify_report");
    d.click("report_editor_add_row");
    d.key(Key::Escape);
    d.frames(2);
    assert!(d.app.reports.editor.is_none());
    assert_eq!(d.app.stored_reports(), before);
    // Escape with nothing open leaves the view where it is.
    d.key(Key::Escape);
    d.frames(2);
    assert_eq!(d.app.views.main, crate::views::MainView::Reports);
    assert!(d.app.project.model == model);
    assert!(!d.app.dirty);
    // The stored text of every report is untouched.
    for s in &before {
        let text = store::load(d.app.store.connection(), s.id).unwrap();
        let lib = super::rpt::library::entries()
            .iter()
            .find(|e| Some(e.id) == s.lib_id.as_deref())
            .unwrap();
        assert_eq!(text, lib.text.as_bytes(), "{}", s.name);
    }
}

#[test]
fn copy_paste_delete_export_and_import_a_definition_file() {
    let mut d = driver("r1-copy");
    open_reports(&mut d);
    pick(&mut d, "Milestones");
    d.click("dock_copy");
    d.click("dock_paste");
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
    let names: Vec<String> = d.app.stored_reports().into_iter().map(|s| s.name).collect();
    assert!(names.contains(&"Milestones (2)".to_string()), "{names:?}");
    // The copy keeps the library group and its text.
    let copy = d
        .app
        .stored_reports()
        .into_iter()
        .find(|s| s.name == "Milestones (2)")
        .unwrap();
    assert_eq!(copy.kind, store::KIND_USER);
    assert!(copy.group.is_some());
    // Export it to a file and import that file again.
    pick(&mut d, "Milestones (2)");
    d.click("tool_export_report");
    let file = d.dir.join("milestones.rpt");
    type_into(&mut d, "export_report_path", &file.display().to_string());
    d.click("export_report_ok");
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
    assert!(d.app.reports.export.is_none());
    let exported = std::fs::read(&file).unwrap();
    assert!(exported.starts_with(b"object TCVReport"));
    d.click("tool_import_report");
    type_into(&mut d, "import_reports_path", &file.display().to_string());
    d.click("import_reports_ok");
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
    assert!(d.app.reports.import.is_none());
    let again = d
        .app
        .stored_reports()
        .into_iter()
        .find(|s| s.name == "Milestones (2) (2)")
        .expect("imported under a free name");
    assert_eq!(
        store::load(d.app.store.connection(), again.id).unwrap(),
        exported,
        "export and import keep the text byte for byte"
    );
    // A file that is not a definition is refused and nothing is added.
    let bad = d.dir.join("bad.rpt");
    std::fs::write(&bad, "not a report").unwrap();
    let count = d.app.stored_reports().len();
    d.click("tool_import_report");
    type_into(&mut d, "import_reports_path", &bad.display().to_string());
    d.click("import_reports_ok");
    assert!(d.app.reports.import.as_ref().unwrap().error.is_some());
    assert_eq!(d.app.stored_reports().len(), count);
    d.click("import_reports_cancel");
    // Delete the copies.
    pick(&mut d, "Milestones (2) (2)");
    d.click("dock_delete");
    pick(&mut d, "Milestones (2)");
    d.click("dock_delete");
    assert_eq!(d.app.stored_reports().len(), count - 2);
}

/// A database copy with the report tables the reference uses, made by the
/// test (never the real database).
fn tiny_reference(path: &std::path::Path) {
    let db = rusqlite::Connection::open(path).unwrap();
    let report = String::from_utf8(
        std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .join("assets/openxer_reports/milestones.rpt"),
        )
        .unwrap(),
    )
    .unwrap();
    db.execute_batch(
        "CREATE TABLE RPT(RPT_ID INTEGER, RPT_GROUP_ID INTEGER, RPT_TYPE TEXT, RPT_NAME TEXT, RPT_DATA BLOB);
         CREATE TABLE RPTGROUP(RPT_GROUP_ID INTEGER, RPT_GROUP_SEQ_NUM INTEGER, RPT_GROUP_NAME TEXT, PARENT_GROUP_ID INTEGER);
         CREATE TABLE RPTBATCH(RPT_BATCH_ID INTEGER, RPT_BATCH_NAME TEXT);
         CREATE TABLE RPTLIST(RPT_BATCH_ID INTEGER, RPT_ID INTEGER, SEQ_NUM INTEGER);
         INSERT INTO RPTGROUP VALUES (10, 1, 'Site', NULL), (11, 2, 'Weekly', 10);
         INSERT INTO RPTBATCH VALUES (1, 'Friday package');
         INSERT INTO RPTLIST VALUES (1, 100, 0), (1, 101, 100), (1, 102, 200);",
    )
    .unwrap();
    for (id, group, kind, name, data) in [
        (100, 11, "RP_User", "Site lookahead", report.clone()),
        (101, 10, "RP_User", "Site milestones", report.clone()),
        (102, 10, "RP_Web", "Web page", report.clone()),
        (
            103,
            10,
            "RP_User",
            "Broken",
            "this is not a report".to_string(),
        ),
    ] {
        db.execute(
            "INSERT INTO RPT VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![id, group, kind, name, data],
        )
        .unwrap();
    }
}

#[test]
fn import_from_a_database_copy_lists_user_reports_and_brings_groups_and_batches() {
    let mut d = driver("r1-db");
    let db = d.dir.join("copy.db");
    tiny_reference(&db);
    let before = std::fs::read(&db).unwrap();
    open_reports(&mut d);
    d.click("tool_import_report");
    d.click("import_reports_db");
    type_into(&mut d, "import_reports_path", &db.display().to_string());
    d.click("import_reports_ok");
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
    let list = d
        .app
        .reports
        .import
        .as_ref()
        .and_then(|i| i.db.as_ref())
        .expect("the database list opens");
    // Only the user-made type is listed; the web page is not.
    let names: Vec<_> = list
        .shown()
        .iter()
        .map(|&i| list.reference.reports[i].name.clone())
        .collect();
    assert_eq!(names, ["Site lookahead", "Site milestones", "Broken"]);
    // Cancel imports nothing.
    let count = d.app.stored_reports().len();
    d.click("import_database_cancel");
    assert_eq!(d.app.stored_reports().len(), count);
    // Choose two and import.
    d.click("tool_import_report");
    d.click("import_reports_db");
    type_into(&mut d, "import_reports_path", &db.display().to_string());
    d.click("import_reports_ok");
    d.click("import_database_rows_row_0");
    d.click("import_database_rows_row_1");
    d.click("import_database_rows_row_2");
    d.click("import_database_import");
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
    assert!(d.app.reports.import.is_none());
    let stored = d.app.stored_reports();
    // The broken one is skipped, the other two arrive in their groups.
    assert_eq!(stored.len(), count + 2);
    let groups = d.app.report_groups();
    let weekly = groups.iter().find(|g| g.name == "Weekly").unwrap();
    let site = groups.iter().find(|g| g.name == "Site").unwrap();
    assert_eq!(weekly.parent, Some(site.id));
    let lookahead = stored.iter().find(|s| s.name == "Site lookahead").unwrap();
    assert_eq!(lookahead.group, Some(weekly.id));
    let batches = store::batches(d.app.store.connection()).unwrap();
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0].name, "Friday package");
    assert_eq!(batches[0].reports.len(), 2);
    // The imported batch is offered in Batch Reports and checks its reports.
    d.click("menu_bar_tools");
    assert!(d.hover_text("Reports"));
    let pos = d.rect("menu_batch_reports").center();
    d.click_at(pos);
    d.frames(2);
    d.click("batch_reports_saved");
    assert!(d.click_text("Friday package"));
    d.frames(2);
    assert_eq!(d.app.reports.batch.as_ref().unwrap().checked.len(), 2);
    d.click("batch_reports_close");
    // The database copy was opened read-only and is unchanged.
    assert_eq!(std::fs::read(&db).unwrap(), before);
}

#[test]
fn the_editor_adds_edits_undoes_and_saves() {
    let mut d = driver("r1-editor");
    open_reports(&mut d);
    context_menu(&mut d, "Milestones");
    d.click("reports_menu_blank");
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
    let ed = d.app.reports.editor.as_ref().expect("a blank report opens");
    let id = ed.id;
    assert_eq!(ed.name, "New Report");
    // Outline row 0 is the report; select the page header and add a row and a
    // text cell.
    let rows = |d: &Driver| d.app.reports.editor.as_ref().unwrap().outline();
    let header = rows(&d)
        .iter()
        .position(|(_, _, c)| c == "Page header")
        .unwrap();
    d.click(&format!("report_editor_rows_row_{header}"));
    d.click("report_editor_add_row");
    d.click("report_editor_add_text");
    d.click("report_editor_add_text");
    let ed = d.app.reports.editor.as_ref().unwrap();
    assert_eq!(ed.undo.len(), 3);
    assert!(
        rows(&d)
            .iter()
            .filter(|(_, _, c)| c.starts_with("Cell:"))
            .count()
            == 2
    );
    // Undo and redo.
    d.click("report_editor_undo");
    assert!(
        rows(&d)
            .iter()
            .filter(|(_, _, c)| c.starts_with("Cell:"))
            .count()
            == 1
    );
    d.click("report_editor_redo");
    assert!(
        rows(&d)
            .iter()
            .filter(|(_, _, c)| c.starts_with("Cell:"))
            .count()
            == 2
    );
    // Properties of the selected cell: change its text; Cancel changes nothing.
    d.click("report_editor_properties");
    assert!(d.app.reports.editor.as_ref().unwrap().props.is_some());
    type_into(&mut d, "props_text", "Edited cell");
    d.click("props_cancel");
    assert!(d.app.reports.editor.as_ref().unwrap().props.is_none());
    assert!(rows(&d).iter().all(|(_, _, c)| c != "Cell: Edited cell"));
    // Escape also leaves the editor alone and only closes Properties.
    d.click("report_editor_properties");
    type_into(&mut d, "props_text", "Edited cell");
    d.key(Key::Escape);
    d.frames(2);
    let ed = d
        .app
        .reports
        .editor
        .as_ref()
        .expect("the editor stays open");
    assert!(ed.props.is_none());
    assert!(rows(&d).iter().all(|(_, _, c)| c != "Cell: Edited cell"));
    // OK applies it as one undo step.
    let undo = d.app.reports.editor.as_ref().unwrap().undo.len();
    d.click("report_editor_properties");
    type_into(&mut d, "props_text", "Edited cell");
    d.click("props_ok");
    let ed = d.app.reports.editor.as_ref().unwrap();
    assert_eq!(ed.undo.len(), undo + 1);
    assert!(rows(&d).iter().any(|(_, _, c)| c == "Cell: Edited cell"));
    // Report tab: title and page setup.
    d.click("report_editor_rows_row_0");
    d.click("report_editor_properties");
    type_into(&mut d, "props_title", "Field Report");
    d.click("props_page_portrait");
    d.click("props_ok");
    // Add a data source with a field cell.
    let detail = rows(&d)
        .iter()
        .position(|(_, _, c)| c == "Sections")
        .unwrap();
    d.click(&format!("report_editor_rows_row_{detail}"));
    d.click("report_editor_add_source");
    d.click("report_editor_save");
    assert!(d.app.error.is_none());
    // Saved text reads back with the changes.
    let bytes = store::load(d.app.store.connection(), id).unwrap();
    let s = super::rpt::stream::Stream::parse_bytes(&bytes).unwrap();
    let r = super::rpt::model::Report::from_stream(&s).unwrap();
    assert_eq!(r.title, "Field Report");
    assert_eq!(r.page.orientation, super::rpt::page::Orientation::Portrait);
    assert_eq!(r.page.width, 8.5);
    assert_eq!(r.body.len(), 1);
    // OK saves and closes; the report runs.
    d.click("report_editor_ok");
    assert!(d.app.reports.editor.is_none());
    let g = d.app.generate_stored(id, "").unwrap();
    assert!(g.warnings.is_empty(), "{:?}", g.warnings);
    assert_eq!(
        d.app
            .stored_reports()
            .iter()
            .find(|s| s.id == id)
            .unwrap()
            .name,
        "New Report"
    );
}

#[test]
fn batch_runs_definition_and_wizard_reports_together() {
    let mut d = driver("r1-batch");
    let mut w = super::wizard::Definition::new(super::wizard::Subject::Resources);
    w.name = "Crew list".into();
    d.app.save_report_definitions(&[w]);
    d.click("menu_bar_tools");
    assert!(d.hover_text("Reports"));
    let pos = d.rect("menu_batch_reports").center();
    d.click_at(pos);
    d.frames(2);
    let entries = d.app.report_entries();
    let member: Vec<Option<u32>> = entries.iter().map(|e| e.group).collect();
    let rows = groups::outline_by(&d.app.report_groups(), &member);
    for name in ["Crew list", "Critical Activities"] {
        let row = rows
            .iter()
            .position(|(n, _)| matches!(n, groups::Node::Report(i) if entries[*i].name == name))
            .unwrap();
        d.click(&format!("batch_rows_row_{row}"));
    }
    let folder = d.dir.join("batch out");
    type_into(
        &mut d,
        "batch_reports_folder",
        &folder.display().to_string(),
    );
    d.click("batch_reports_format");
    assert!(d.click_text("HTML file"));
    d.frames(2);
    d.click("batch_reports_run");
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
    assert!(folder.join("Crew list.html").exists());
    let critical = std::fs::read_to_string(folder.join("Critical Activities.html")).unwrap();
    assert!(critical.contains("Critical Activities"));
    // Excel is refused for definition reports.
    d.click("batch_reports_format");
    assert!(d.click_text("Excel file"));
    d.frames(2);
    d.click("batch_reports_run");
    assert!(d
        .app
        .error
        .as_deref()
        .unwrap_or("")
        .contains("definition report"));
}

#[test]
fn render_reports_windows() {
    use crate::headless_render::SoftwarePainter;
    use crate::reports::test_support::{app, crop_dialog};
    let out = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/r1");
    std::fs::create_dir_all(&out).unwrap();
    let dir = temp("r1-render");
    let (mut app, ctx) = app(&dir);
    let mut painter = SoftwarePainter::default();
    let mut time = 0.;
    for _ in 0..2 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    let mut shot = |app: &mut crate::App, name: &str, salt: &str| {
        for _ in 0..3 {
            let _ = painter.frame(app, &ctx, &mut time, vec![], false);
        }
        let (image, _) = painter.frame(app, &ctx, &mut time, vec![], true).unwrap();
        crop_dialog(&image, &ctx, salt)
            .save(out.join(name))
            .unwrap();
    };
    let first = app.stored_reports()[2].id;
    app.open_run_report(
        super::rpt_entries::Which::Stored(first),
        "Critical Activities",
    );
    shot(&mut app, "run-report.png", "run_report");
    app.reports.run = None;
    app.open_report_editor(first);
    app.reports.editor.as_mut().unwrap().selected = 12;
    shot(&mut app, "report-editor.png", "report_editor");
    for (tab, name) in [(0usize, "report"), (1, "source"), (2, "row"), (3, "cell")] {
        let ed = app.reports.editor.as_mut().unwrap();
        let outline = ed.outline();
        let want = match tab {
            0 => 0,
            1 => outline
                .iter()
                .position(|(_, _, c)| c.starts_with("Data source"))
                .unwrap(),
            2 => outline
                .iter()
                .rposition(|(_, _, c)| c.starts_with("Row"))
                .unwrap(),
            _ => outline
                .iter()
                .rposition(|(_, _, c)| c.starts_with("Cell"))
                .unwrap(),
        };
        ed.selected = want;
        let path = ed.selected_path();
        ed.props = Some(crate::reports::rpt_props::Props::new(&ed.stream, path));
        shot(
            &mut app,
            &format!("properties-{name}.png"),
            "report_properties",
        );
    }
    app.reports.editor = None;
    app.run_stored_report(
        first,
        crate::reports::rpt_run::Target::Html,
        &dir.join("preview.html"),
        &Default::default(),
        "",
    )
    .unwrap();
    app.reports.page_preview = Some(crate::reports::rpt_run::PagePreview::new(
        &app.generate_stored(first, "").unwrap(),
        app.viewer_start(),
    ));
    shot(&mut app, "report-preview.png", "report_preview_pages");
    assert!(app.error.is_none(), "{:?}", app.error);
}

/// A database import that fails part way shows the error and leaves the
/// workspace as it was: the reports already written are rolled back.
#[test]
fn a_failed_database_import_shows_the_error_and_rolls_back() {
    let out = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/bugfix-help-audit");
    std::fs::create_dir_all(&out).unwrap();
    let mut d = Driver::probed(
        "r1-db-fail",
        scheduled_starter(),
        egui::Vec2::new(1500., 900.),
        Some(out.clone()),
    );
    d.home();
    let db = d.dir.join("copy.db");
    tiny_reference(&db);
    open_reports(&mut d);
    let count = d.app.stored_reports().len();
    let groups = d.app.report_groups().len();
    // The second report cannot be written (as on a full disk).
    d.app
        .store
        .connection()
        .execute_batch(
            "CREATE TEMP TRIGGER fail_second BEFORE INSERT ON report_defs \
             WHEN NEW.rpt_name = 'Site milestones' \
             BEGIN SELECT RAISE(ABORT, 'database or disk is full'); END",
        )
        .unwrap();
    d.click("tool_import_report");
    d.click("import_reports_db");
    type_into(&mut d, "import_reports_path", &db.display().to_string());
    d.click("import_reports_ok");
    d.click("import_database_rows_row_0");
    d.click("import_database_rows_row_1");
    d.click("import_database_import");
    d.frames(2);
    let error = d.app.error.clone().expect("the error is shown");
    assert!(error.contains("disk is full"), "{error}");
    assert!(error.contains("Nothing was imported"), "{error}");
    d.screenshot(&out.join("report-import-failed.png"));
    assert_eq!(
        d.app.stored_reports().len(),
        count,
        "the first report is rolled back"
    );
    assert_eq!(d.app.report_groups().len(), groups, "no new groups");
    assert!(store::batches(d.app.store.connection()).unwrap().is_empty());
    assert!(
        d.app
            .reports
            .import
            .as_ref()
            .is_some_and(|i| i.db.is_some()),
        "the list stays open to try again"
    );
}

/// The report Print Preview fits a 1280 x 720 screen, and its "Saved ..."
/// note shows in the status line.
#[test]
fn report_print_preview_fits_a_small_screen_and_shows_the_saved_note() {
    let screen = egui::Vec2::new(1280., 720.);
    let mut d = Driver::probed("rpt-preview-small", scheduled_starter(), screen, None);
    d.home();
    let first = d.app.stored_reports()[2].id;
    let g = d.app.generate_stored(first, "").unwrap();
    let mut p = crate::reports::rpt_run::PagePreview::new(&g, d.app.viewer_start());
    p.note = Some("Saved C:\\Reports\\variance.pdf".into());
    d.app.reports.page_preview = Some(p);
    d.frames(3);
    let client = crate::dialog_kit::noted_client(&d.ctx, "report_preview_pages").unwrap();
    let whole = egui::Rect::from_min_size(egui::Pos2::ZERO, screen);
    assert!(
        whole.contains_rect(client),
        "{client:?} on a {screen:?} screen"
    );
    let save = d.rect("report_preview_save");
    assert!(whole.contains_rect(save), "{save:?}");
    assert!(
        d.texts()
            .iter()
            .any(|(t, _)| t.contains("Saved C:\\Reports\\variance.pdf")),
        "the note is drawn"
    );
}
