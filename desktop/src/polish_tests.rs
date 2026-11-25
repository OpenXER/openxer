//! Headless click tests for the commands finished in the polish package
//! (File > Print Setup... and Tools > Summarize were removed 2026-11-07):
//! Tools > Reports > Report Groups... and Batch Reports..., and Tools >
//! Spread Budget Hours (Top-Down).... They
//! drive the production UI through `qa_sweep::Driver` (synthetic egui input
//! only) and render the new dialogs to the ignored `artifacts/g6/`.
use crate::qa_sweep::Driver;
use crate::reports::groups;
use crate::reports::wizard::{Definition, Subject};
use eframe::egui::{self, Key};

fn ctrl() -> egui::Modifiers {
    egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    }
}

fn driver(name: &str) -> Driver {
    let mut d = Driver::new(name, crate::reports::test_support::scheduled_starter());
    d.home();
    crate::reports::test_support::without_library(&mut d.app);
    d
}

/// Open a menu, hover its submenu and click the item.
fn menu(d: &mut Driver, bar: &str, submenu: Option<&str>, action: &str) {
    d.click(&format!("menu_bar_{bar}"));
    if let Some(label) = submenu {
        assert!(d.hover_text(label), "submenu {label:?}");
    }
    let pos = d.rect(action).center();
    d.click_at(pos);
    d.frames(2);
}

/// Replace the text of an edit field.
fn type_into(d: &mut Driver, action: &str, text: &str) {
    d.click(action);
    d.key_with(Key::A, ctrl());
    d.type_text(text);
    d.frames(2);
}

#[test]
fn print_setup_send_project_and_summarize_are_gone() {
    let mut d = driver("print-setup");
    d.click("menu_bar_file");
    d.frames(2);
    assert!(
        d.has("menu_page_setup"),
        "Page Setup covers the printer page"
    );
    assert!(!d.has("menu_print_setup"));
    assert!(!d.texts().iter().any(|(t, _)| t.starts_with("Send Project")));
    d.key(Key::Escape);
    d.frames(2);
    d.click("menu_bar_tools");
    d.frames(2);
    for gone in ["Summarize", "Visualizer", "Publish"] {
        assert!(!d.texts().iter().any(|(t, _)| t == gone), "{gone} is gone");
    }
    d.key(Key::Escape);
}

#[test]
fn report_groups_add_rename_assign_and_delete() {
    let mut d = driver("report-groups");
    let mut a = Definition::new(Subject::Activities);
    a.name = "Lookahead".into();
    let mut b = Definition::new(Subject::Resources);
    b.name = "Crew list".into();
    d.app.save_report_definitions(&[a, b]);
    menu(&mut d, "tools", Some("Reports"), "menu_report_groups");
    assert!(d.app.reports.groups.is_some());
    // Add a top-level group and rename it.
    d.click("report_groups_add");
    type_into(&mut d, "report_groups_name", "Field Reports");
    let list = d.app.report_groups();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "Field Reports");
    // A child group under it.
    d.click("report_groups_add");
    let list = d.app.report_groups();
    assert_eq!(list[1].parent, Some(list[0].id));
    // Move the Lookahead report (the last row) into Field Reports.
    let rows = groups::outline(&list, &d.app.report_definitions());
    let row = rows
        .iter()
        .position(|(n, _)| *n == groups::Node::Report(0))
        .unwrap();
    d.click(&format!("report_groups_rows_row_{row}"));
    d.click("report_groups_group");
    assert!(d.click_text("Field Reports > New Report Group"));
    d.frames(2);
    assert_eq!(d.app.report_definitions()[0].group, Some(list[1].id));
    // Deleting the child group moves the report up to Field Reports.
    let rows = groups::outline(&d.app.report_groups(), &d.app.report_definitions());
    let row = rows
        .iter()
        .position(|(n, _)| *n == groups::Node::Group(list[1].id))
        .unwrap();
    d.click(&format!("report_groups_rows_row_{row}"));
    d.click("report_groups_delete");
    assert_eq!(d.app.report_groups().len(), 1);
    assert_eq!(d.app.report_definitions()[0].group, Some(list[0].id));
    d.click("report_groups_close");
    assert!(d.app.reports.groups.is_none());
    // Escape closes it too.
    menu(&mut d, "tools", Some("Reports"), "menu_report_groups");
    d.key(Key::Escape);
    d.frames(2);
    assert!(d.app.reports.groups.is_none());
    assert!(
        !d.app.dirty,
        "groups live in the preferences, not the project"
    );
}

#[test]
fn batch_reports_write_the_checked_reports_to_a_folder() {
    let mut d = driver("batch-reports");
    let mut a = Definition::new(Subject::Activities);
    a.name = "Lookahead".into();
    let mut b = Definition::new(Subject::Relationships);
    b.name = "Logic".into();
    let mut c = Definition::new(Subject::Resources);
    c.name = "Not run".into();
    d.app.save_report_definitions(&[a, b, c]);
    menu(&mut d, "tools", Some("Reports"), "menu_batch_reports");
    assert!(d.app.reports.batch.is_some());
    // Rows 0 and 1 (no groups): check them.
    d.click("batch_rows_row_0");
    d.click("batch_rows_row_1");
    let batch = d.app.reports.batch.as_ref().unwrap();
    assert_eq!(batch.checked.iter().copied().collect::<Vec<_>>(), [0, 1]);
    let folder = d.dir.join("batch out");
    type_into(
        &mut d,
        "batch_reports_folder",
        &folder.display().to_string(),
    );
    // CSV from the format list.
    d.click("batch_reports_format");
    assert!(d.click_text("CSV file"));
    d.frames(2);
    d.click("batch_reports_run");
    assert!(d.app.error.is_none(), "{:?}", d.app.error);
    assert!(
        d.app.status.contains("2 reports written"),
        "{}",
        d.app.status
    );
    let lookahead = std::fs::read_to_string(folder.join("Lookahead.csv")).unwrap();
    assert!(lookahead.contains("RB-1000"));
    assert!(folder.join("Logic.csv").exists());
    assert!(!folder.join("Not run.csv").exists());
    // The folder is remembered for the next batch.
    d.click("batch_reports_close");
    assert!(d.app.reports.batch.is_none());
    menu(&mut d, "tools", Some("Reports"), "menu_batch_reports");
    assert_eq!(
        d.app.reports.batch.as_ref().unwrap().folder,
        folder.display().to_string()
    );
    d.key(Key::Escape);
    d.frames(2);
    assert!(d.app.reports.batch.is_none());
}

#[test]
fn top_down_estimation_applies_as_one_undo_step() {
    let mut d = driver("top-down");
    let model = d.app.project.model.clone();
    let cell = |d: &Driver, task: i64, k: &str| {
        let t = d.app.project.document.table("TASK").unwrap();
        let r = t
            .rows
            .iter()
            .find(|r| t.value(r, "task_id") == task.to_string())
            .unwrap();
        // A blank cell holds no units.
        match t.value(r, k) {
            "" => "0".to_string(),
            v => v.to_string(),
        }
    };
    // Cancel leaves the project alone.
    menu(&mut d, "tools", None, "menu_top_down");
    type_into(&mut d, "top_down_total", "1000");
    d.click("top_down_cancel");
    assert!(d.app.estimation.is_none());
    assert_eq!(cell(&d, 1010, "target_work_qty"), "0");
    // Escape too.
    menu(&mut d, "tools", None, "menu_top_down");
    d.key(Key::Escape);
    d.frames(2);
    assert!(d.app.estimation.is_none());
    let undo = d.app.history.counts().0;
    // Weight 2 on RB.1 (row 0), then apply 1000 units.
    menu(&mut d, "tools", None, "menu_top_down");
    type_into(&mut d, "top_down_total", "1000");
    d.click("top_down_rows_row_0");
    type_into(&mut d, "top_down_weight", "2");
    d.click("top_down_apply");
    assert!(
        d.app.estimation.is_none(),
        "{:?}",
        d.app.estimation.as_ref().map(|e| e.error.clone())
    );
    assert_eq!(d.app.history.counts().0, undo + 1, "one undo step");
    assert_eq!(cell(&d, 1010, "target_work_qty"), "200");
    assert_eq!(cell(&d, 1060, "target_work_qty"), "40");
    assert_eq!(d.app.project.wbs_estimation_weight(10), 2.);
    assert!(d.app.project.model == model, "no dates or durations move");
    assert!(d.app.dirty);
    // The Budgeted Labor Units column shows the activity's units.
    let ix = d.app.grid_index();
    let catalog = crate::grid::columns::catalog(&d.app.project);
    let column = crate::grid::columns::find(&catalog, "budgeted_labor_units").unwrap();
    let column = column.clone();
    let i = d
        .app
        .project
        .model
        .activities
        .iter()
        .position(|a| a.task_id == 1010)
        .unwrap();
    let value = crate::grid::columns::value(&d.app.project, &ix, &column, i);
    let shown = crate::grid::columns::display(&d.app.project, Some(i), &value);
    assert!(shown.starts_with("200"), "{shown}");
    // Undo restores units and weight together.
    d.app.history_move(&d.ctx.clone(), false);
    assert_eq!(cell(&d, 1010, "target_work_qty"), "0");
    assert_eq!(d.app.project.wbs_estimation_weight(10), 1.);
    // A WBS element of the WBS view starts as the target.
    d.click("directory_wbs");
    d.app.views.wbs.selected = Some(30);
    menu(&mut d, "tools", None, "menu_top_down");
    assert_eq!(d.app.estimation.as_ref().unwrap().root, 3);
    type_into(&mut d, "top_down_total", "50");
    d.click("top_down_apply");
    assert_eq!(cell(&d, 1060, "target_work_qty"), "10");
    assert_eq!(cell(&d, 1010, "target_work_qty"), "0");
}

#[test]
fn render_polish_dialogs() {
    use crate::headless_render::SoftwarePainter;
    use crate::reports::test_support::{app, crop_dialog, temp};
    let out = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/g6");
    std::fs::create_dir_all(&out).unwrap();
    let dir = temp("g6-render");
    let (mut app, ctx) = app(&dir);
    crate::reports::test_support::without_library(&mut app);
    let mut list = vec![];
    let field = groups::add_group(&mut list, None, "Field Reports");
    groups::add_group(&mut list, Some(field), "Weekly");
    app.store
        .set_preference(groups::GROUPS_KEY, &groups::groups_to_json(&list))
        .unwrap();
    let mut a = Definition::new(Subject::Activities);
    a.name = "Lookahead".into();
    a.group = Some(2);
    let mut b = Definition::new(Subject::Resources);
    b.name = "Crew list".into();
    b.group = Some(field);
    let mut c = Definition::new(Subject::Relationships);
    c.name = "Logic".into();
    app.save_report_definitions(&[a, b, c]);
    let mut painter = SoftwarePainter::default();
    let mut time = 0.;
    for _ in 0..2 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    let mut shot = |app: &mut crate::App, name: &str, salt: &str| {
        for _ in 0..2 {
            let _ = painter.frame(app, &ctx, &mut time, vec![], false);
        }
        let (image, _) = painter.frame(app, &ctx, &mut time, vec![], true).unwrap();
        crop_dialog(&image, &ctx, salt)
            .save(out.join(name))
            .unwrap();
    };
    app.open_report_groups();
    app.reports.groups.as_mut().unwrap().selected = Some(groups::Node::Group(field));
    app.reports.groups.as_mut().unwrap().name = "Field Reports".into();
    shot(&mut app, "report-groups.png", "report_groups");
    app.reports.groups = None;
    app.open_batch_reports();
    app.reports.batch.as_mut().unwrap().checked = [0, 1].into_iter().collect();
    shot(&mut app, "batch-reports.png", "batch_reports");
    app.reports.batch = None;
    app.open_top_down_estimation();
    app.estimation.as_mut().unwrap().total = "1200".into();
    shot(&mut app, "top-down-estimation.png", "top_down");
    assert!(app.error.is_none(), "{:?}", app.error);
}
