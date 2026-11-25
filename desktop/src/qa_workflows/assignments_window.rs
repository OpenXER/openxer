//! Workflow: the Resource Assignments window as a working table.
//!
//! - Columns > Customize... on the layout options bar adds Remaining Units
//!   and Actual Regular Units. Typing 30 into the Pipefitter's Remaining
//!   Units cell sets its remaining units through the edit layer, one undo
//!   step; Actual Regular Units opens no editor before the activity starts,
//!   as on the Resources tab.
//! - Group and Sort By > Activity puts the survey activity's two
//!   assignments under one band; Fill Down (Ctrl+E) copies the first one's
//!   remaining units to the second, one undo step.
//! - The spreadsheet beside the table is the Resource Usage Spreadsheet's:
//!   a Tuesday cell takes 20 (F2, Enter) and only Tuesday changes, one undo
//!   step; Spreadsheet Fields > Budgeted Units on its right-click menu
//!   shows the budget instead.
//! - Insert (Add) opens Assign Resources for the selected assignment's
//!   activity; Delete asks first and Yes deletes every selected assignment
//!   as one undo step.
//!
//! Each change is checked, then one Undo puts the project back.
use super::*;

const DAY: i64 = 24 * 60;

/// The starter project with the Pipefitter (resource 7) and the Crane
/// (resource 8) on the survey activity (task 1010).
fn project() -> ProjectData {
    let mut p = crate::views::resources::tests::with_resource();
    p.add_assignment(1010, Some(8), None).unwrap();
    p
}

/// The window opened from Project > Resource Assignments.
fn open_assignments(name: &str) -> Driver {
    let mut d = Driver::new(name, project());
    d.home();
    d.open_menu("menu_bar_project");
    d.click("menu_resource_assignments");
    d.frames(3);
    assert_eq!(d.app.views.main, views::MainView::Assignments);
    d
}

/// TASKRSRC column `column` of resource `rsrc`'s assignment.
fn value(d: &Driver, rsrc: &str, column: &str) -> String {
    let t = d.app.project.document.table("TASKRSRC").unwrap();
    t.rows
        .iter()
        .find(|r| t.value(r, "rsrc_id") == rsrc)
        .map(|r| t.value(r, column).to_string())
        .unwrap_or_else(|| panic!("an assignment of resource {rsrc}"))
}

fn count(d: &Driver) -> usize {
    d.app
        .project
        .document
        .table("TASKRSRC")
        .map_or(0, |t| t.rows.len())
}

fn steps(d: &Driver) -> usize {
    d.app.history.counts().0
}

fn undo(d: &mut Driver) {
    d.key_with(Key::Z, ctrl());
    d.frames(2);
}

/// Open the layout options bar's menu, hover submenu `sub` and click
/// `item` in it.
#[track_caller]
fn bar_item(d: &mut Driver, sub: &str, item: &str) {
    d.click("assignments_layout");
    let sub = format!("assignments_layout_menu_{sub}");
    assert!(d.has(&sub), "{sub} is on the layout options bar's menu");
    let hover = d.rect(&sub).center();
    let _ = d.frame(vec![egui::Event::PointerMoved(hover)]);
    d.frames(3);
    let key = format!("assignments_layout_menu_{item}");
    assert!(d.has(&key), "{key} is in that submenu");
    d.click_at(d.rect(&key).center());
    d.frames(2);
}

/// The control `<prefix><n>` whose rectangle shows `text`, if one does,
/// with the visible part of that text.
fn find_shown(d: &mut Driver, prefix: &str, text: &str) -> Option<(usize, egui::Rect)> {
    let texts = d.texts();
    (0..80)
        .filter(|n| d.has(&format!("{prefix}{n}")))
        .find_map(|n| {
            let r = d.rect(&format!("{prefix}{n}"));
            texts
                .iter()
                .find(|(t, at)| t == text && r.contains(at.center()))
                .map(|(_, at)| (n, *at))
        })
}

/// The control `<prefix><n>` whose rectangle shows `text`, if one does.
fn find_control(d: &mut Driver, prefix: &str, text: &str) -> Option<usize> {
    find_shown(d, prefix, text).map(|(n, _)| n)
}

#[track_caller]
fn control_with(d: &mut Driver, prefix: &str, text: &str) -> usize {
    match find_control(d, prefix, text) {
        Some(n) => n,
        None => panic!("a {prefix} control shows {text:?}"),
    }
}

/// The table's field (column) titled `title`.
fn column(d: &mut Driver, title: &str) -> usize {
    control_with(d, "assignments_col_", title)
}

/// The table row that shows `text`.
fn row(d: &mut Driver, text: &str) -> usize {
    control_with(d, "assignments_row_", text)
}

/// The centre of the cell of table row `row`, field `field`.
fn cell(d: &Driver, row: usize, field: usize) -> Pos2 {
    let x = d.rect(&format!("assignments_col_{field}")).center().x;
    let y = d.rect(&format!("assignments_row_{row}")).center().y;
    Pos2::new(x, y)
}

/// Where the Available Options item titled `title` shows, scrolled into
/// view: the centre of its visible text.
fn available(d: &mut Driver, title: &str) -> Pos2 {
    let prefix = "detail_columns_available_";
    d.frames(2);
    assert!(d.has("detail_columns_ok"), "the Columns dialog is open");
    for _ in 0..16 {
        if let Some((_, text)) = find_shown(d, prefix, title) {
            return text.center();
        }
        // The wheel over the middle of the list (220 px high, under its
        // header). The first item scrolls out of the list, so the pointer
        // cannot follow it.
        let header = d.rect("detail_columns_available_header");
        let list = Pos2::new(header.center().x, header.bottom() + 110.);
        let _ = d.frame(vec![
            egui::Event::PointerMoved(list),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::Vec2::new(0., -120.),
                modifiers: Default::default(),
            },
        ]);
        d.frames(30);
    }
    panic!("Available Options lists {title:?}")
}

/// Columns > Customize...: add the columns titled `titles`, then OK.
fn add_columns(d: &mut Driver, titles: &[&str]) {
    bar_item(d, "columns", "columns_customize");
    for title in titles {
        let at = available(d, title);
        d.click_at(at);
        d.click("detail_columns_add");
        let chosen = d.app.detail.column_dialog.as_ref().map(|c| &c.selected);
        assert!(
            chosen.is_some_and(|c| c.iter().any(|t| t == title)),
            "{title} moved to Selected Options"
        );
    }
    d.click("detail_columns_ok");
    d.frames(2);
}

/// Click the cell twice: the second click on the focus cell of the current
/// row opens its editor, when the cell takes a value.
fn open_cell(d: &mut Driver, at: Pos2) {
    d.click_at(at);
    d.frames(2);
    d.click_at(at);
    d.frames(2);
}

#[test]
fn workflow_assignment_window_columns_cells_group_and_fill_down() {
    let mut d = open_assignments("wf-asg-window-cells");
    // A table wide enough for the columns added below.
    d.app.views.assignments.table_width = 1000.;
    d.frames(2);
    add_columns(
        &mut d,
        &[
            "Resource ID Name",
            "Remaining Units",
            "Actual Regular Units",
        ],
    );
    let remaining = column(&mut d, "Remaining Units");
    let actual = column(&mut d, "Actual Regular Units");
    let pipe = row(&mut d, "Pipefitter") + 1;
    let start = steps(&d);
    assert_eq!(value(&d, "7", "remain_qty"), "24");

    // Remaining Units takes a typed value through the edit layer.
    let at = cell(&d, pipe, remaining);
    open_cell(&mut d, at);
    assert!(d.has("assignments_edit"), "the cell opens its editor");
    d.type_text("30");
    d.key(Key::Enter);
    d.frames(2);
    d.assert_clean("typed remaining units");
    assert_eq!(value(&d, "7", "remain_qty"), "30");
    assert_eq!(steps(&d), start + 1, "one undo step");

    // Actual Regular Units: the activity has not started.
    let at = cell(&d, pipe, actual);
    open_cell(&mut d, at);
    assert!(
        !d.has("assignments_edit"),
        "no editor before the activity starts"
    );

    // Group and Sort By > Activity: both assignments under the activity.
    bar_item(&mut d, "group_by", "group_activity");
    let pipe = row(&mut d, "PIPE  Pipefitter");
    let crane = row(&mut d, "PIPE.CRANE  Crane");
    assert_eq!(crane, pipe + 1, "the survey's two assignments together");
    let crane_before = value(&d, "8", "remain_qty");
    assert_ne!(crane_before, "30");
    // Fill Down: the Pipefitter's 30 to the Crane.
    d.click_at(cell(&d, pipe, remaining));
    let at = cell(&d, crane, remaining);
    d.press_at(
        at,
        egui::PointerButton::Primary,
        egui::Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    d.key_with(Key::E, ctrl());
    d.frames(2);
    d.assert_clean("fill down");
    assert_eq!(value(&d, "8", "remain_qty"), "30", "{}", d.app.status);
    assert_eq!(steps(&d), start + 2, "Fill Down is one undo step");

    // Undo twice: as before.
    undo(&mut d);
    assert_eq!(value(&d, "8", "remain_qty"), crane_before);
    assert_eq!(value(&d, "7", "remain_qty"), "30");
    undo(&mut d);
    assert_eq!(value(&d, "7", "remain_qty"), "24");
}

/// The day activity 1010 starts on, at midnight.
fn monday(d: &Driver) -> i64 {
    let i = index_of(&d.app.project, "RB-1010");
    let start = d.app.project.model.activities[i].early_start.unwrap();
    start.div_euclid(DAY) * DAY
}

/// The spreadsheet cell of table row `line` on `day`.
#[track_caller]
fn sheet_cell(d: &Driver, line: usize, day: i64) -> String {
    let at = day + 9 * 60;
    let periods = &d.app.panes.sheet_periods;
    let found = periods.iter().position(|p| p.0 <= at && at < p.1);
    let n = found.unwrap_or_else(|| panic!("the spreadsheet shows day {day}: {periods:?}"));
    format!("assignments_sheet_cell_{line}_{n}")
}

#[test]
fn workflow_assignment_window_spreadsheet_takes_period_values() {
    let mut d = open_assignments("wf-asg-window-sheet");
    d.app.views.assignments.zoom = 400.;
    d.frames(3);
    let mon = monday(&d);
    let line = row(&mut d, "Pipefitter") + 1;
    let start = steps(&d);
    let key = sheet_cell(&d, line, mon + DAY);
    assert!(d.has(&key), "the window's spreadsheet cells take a click");
    d.click(&key);
    d.key(Key::F2);
    d.frames(1);
    assert!(
        d.app.panes.sheet_cells.editing.is_some(),
        "F2 opens the cell"
    );
    d.type_text("20");
    d.key(Key::Enter);
    d.frames(2);
    d.assert_clean("typed period value");
    assert_eq!(value(&d, "7", "remain_crv"), "8:8;20:8;8:8");
    assert_eq!(value(&d, "7", "remain_qty"), "36");
    assert_eq!(steps(&d), start + 1, "one undo step");

    // Spreadsheet Fields > Budgeted Units on the spreadsheet's menu.
    let at = d.rect(&sheet_cell(&d, line, mon)).center();
    d.press_at(at, egui::PointerButton::Secondary, Default::default());
    d.frames(2);
    let sub = "assignments_chart_menu_spreadsheet_fields";
    assert!(d.has(sub), "the Resource Usage Spreadsheet's menu");
    assert!(d.has("assignments_chart_menu_fill_across"));
    let hover = d.rect(sub).center();
    let _ = d.frame(vec![egui::Event::PointerMoved(hover)]);
    d.frames(3);
    d.click_at(d.rect("assignments_chart_menu_fields_planned").center());
    d.frames(2);
    assert!(d.texts().iter().any(|(t, _)| t == "Planned Units"));

    undo(&mut d);
    assert_eq!(value(&d, "7", "remain_crv"), "");
    assert_eq!(value(&d, "7", "remain_qty"), "24");
}

#[test]
fn workflow_assignment_window_add_and_delete() {
    let mut d = open_assignments("wf-asg-window-add");
    let before = count(&d);
    let start = steps(&d);
    let pipe = row(&mut d, "Pipefitter") + 1;
    d.click(&format!("assignments_row_{pipe}"));
    // Add: Assign Resources for the selected assignment's activity.
    d.key(Key::Insert);
    d.frames(2);
    assert!(d.has("rsrc_picker_assign"), "Add opens Assign Resources");
    pick_and_assign(&mut d, "Unused");
    assert_eq!(count(&d), before + 1);
    assert_eq!(
        value(&d, "9", "task_id"),
        "1010",
        "on the selected activity"
    );
    assert_eq!(steps(&d), start + 1);
    undo(&mut d);
    assert_eq!(count(&d), before);

    // Delete both assignments of the survey: asked first, one undo step.
    d.frames(2);
    let start = steps(&d);
    let pipe = row(&mut d, "Pipefitter") + 1;
    let crane = row(&mut d, "Crane") + 1;
    d.click(&format!("assignments_row_{pipe}"));
    let at = d.rect(&format!("assignments_row_{crane}")).center();
    d.press_at(at, egui::PointerButton::Primary, ctrl());
    d.key(Key::Delete);
    d.frames(2);
    assert!(d.has("delete_resource_assignment_yes"), "Delete asks first");
    d.click("delete_resource_assignment_yes");
    d.frames(2);
    d.assert_clean("delete");
    assert_eq!(count(&d), before - 2, "every selected assignment goes");
    assert_eq!(steps(&d), start + 1, "one undo step");
    undo(&mut d);
    assert_eq!(count(&d), before);
}

#[test]
fn workflow_assignment_window_user_defined_column_edits_and_undoes() {
    let mut p = project();
    p.document.table_mut("UDFTYPE").add(&[
        ("udf_type_id", "831".into()),
        ("table_name", "TASKRSRC".into()),
        ("udf_type_name", "user_field_831".into()),
        ("udf_type_label", "Crew Note".into()),
        ("logical_data_type", "FT_TEXT".into()),
    ]);
    let mut d = Driver::new("wf-asg-window-udf", p);
    d.home();
    d.open_menu("menu_bar_project");
    d.click("menu_resource_assignments");
    d.frames(3);
    assert_eq!(d.app.views.main, views::MainView::Assignments);
    d.app.views.assignments.table_width = 1000.;
    d.frames(2);

    // Columns > Customize... offers assignment User Defined columns under
    // "User Defined" and adds Crew Note to the table.
    add_columns(&mut d, &["Crew Note"]);
    let udf_col = column(&mut d, "Crew Note");
    let pipe = row(&mut d, "Pipefitter") + 1;
    let start = steps(&d);

    // Edit the assignment UDF cell in place; one undo step.
    let at = cell(&d, pipe, udf_col);
    open_cell(&mut d, at);
    assert!(
        d.has("assignments_edit"),
        "assignment UDF cell opens its editor"
    );
    d.type_text("Night shift");
    d.key(Key::Enter);
    d.frames(2);
    d.assert_clean("typed assignment udf");

    let stored = |d: &Driver| {
        let origin = d.app.project.model.origin_serial;
        openxer_xer::edit::udf::fields_of(&d.app.project.document, "TASKRSRC", 1, origin)
            .into_iter()
            .find(|f| f.label == "Crew Note")
            .expect("Crew Note field")
            .text
    };
    assert_eq!(stored(&d), "Night shift");
    assert_eq!(
        row(&mut d, "Night shift"),
        pipe,
        "the UDF column displays the value"
    );
    assert_eq!(steps(&d), start + 1, "one undo step");

    // Undo once restores the blank value.
    undo(&mut d);
    assert_eq!(stored(&d), "");
    assert_eq!(steps(&d), start);
}
