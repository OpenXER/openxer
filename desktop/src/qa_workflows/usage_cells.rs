//! Workflow: typing units into the Resource Usage Spreadsheet's period
//! cells, its cell commands, and taking the Manual curve off again.
//!
//! - A Tuesday cell of the Pipefitter's assignment (24 units over Monday to
//!   Wednesday, 8 a day): click, F2, 20, Enter. Only Tuesday changes; the
//!   assignment's remaining and budgeted units become 36 (it has not
//!   started), the runs `8:8;20:8;8:8` are stored, the curve reads Manual
//!   and the activity's labor units follow. One undo step.
//! - A key typed on a selected cell opens its editor without typing that
//!   key (as the reference was seen to do); the value typed next goes in.
//! - The resource line and periods before the data date take no value.
//! - The runs survive an XER round trip; Undo puts everything back.
//! - Fill Across (Ctrl+H), Range Copy (Shift+Ctrl+C, a copy event),
//!   Range Paste and Fill Down (right-click menu), each one undo step, and
//!   the menu in the reference's order.
//! - Clearing the Manual curve asks first (No by default, not observed);
//!   Yes empties the runs.
use super::*;
use crate::details::removals::{Removal, MANUAL_CURVE_ID};
use crate::panes::usage::Interval;
use crate::panes::BottomView;
use openxer_xer::edit::{curves, records};

const DAY: i64 = 24 * 60;

fn shift() -> egui::Modifiers {
    egui::Modifiers {
        shift: true,
        ..Default::default()
    }
}

/// The Resource Usage Spreadsheet of `p` at the bottom, the Pipefitter
/// (resource 7) selected, one period a day.
fn sheet(name: &str, p: ProjectData) -> Driver {
    let mut d = Driver::new(name, p);
    d.home();
    d.app.set_bottom(Some(BottomView::ResourceUsageSheet));
    d.app.panes.usage.resource = Some(7);
    d.app.panes.usage.interval = Interval::Day;
    d.frames(4);
    d
}

/// The day activity RB-1010 starts on (a Monday), at midnight.
fn monday(d: &Driver) -> i64 {
    let i = index_of(&d.app.project, "RB-1010");
    let start = d.app.project.model.activities[i].early_start.unwrap();
    start.div_euclid(DAY) * DAY
}

/// The spreadsheet's period holding 09:00 of `day`.
#[track_caller]
fn period(d: &Driver, day: i64) -> usize {
    let at = day + 9 * 60;
    let periods = &d.app.panes.sheet_periods;
    periods
        .iter()
        .position(|p| p.0 <= at && at < p.1)
        .unwrap_or_else(|| panic!("day {day} is on show: {periods:?}"))
}

/// The spreadsheet line of TASKRSRC row `row`.
fn line_of(d: &Driver, row: usize) -> usize {
    d.app
        .sheet_lines()
        .unwrap()
        .iter()
        .position(|l| l.assignment == Some(row))
        .expect("the assignment's line")
}

fn cell(d: &Driver, line: usize, day: i64) -> Pos2 {
    d.rect(&format!("usage_sheet_cell_{line}_{}", period(d, day)))
        .center()
}

/// Line `line`'s values on Monday, Tuesday and Wednesday.
fn week(d: &Driver, line: usize, monday: i64) -> Vec<f64> {
    let lines = d.app.sheet_lines().unwrap();
    (0..3)
        .map(|k| lines[line].values[period(d, monday + k * DAY)])
        .collect()
}

#[track_caller]
fn close(got: &[f64], want: &[f64]) {
    assert_eq!(got.len(), want.len(), "{got:?}");
    for (g, w) in got.iter().zip(want) {
        assert!((g - w).abs() < 1e-9, "{got:?} vs {want:?}");
    }
}

fn get(d: &Driver, row: usize, column: &str) -> String {
    records::get(&d.app.project.document, "TASKRSRC", row, column)
}

fn task(d: &Driver, column: &str) -> String {
    let pid = d.app.project.model.project.proj_id;
    records::task_value(&d.app.project.document, 1010, pid, column)
}

fn rows(d: &Driver) -> Vec<Vec<String>> {
    d.app
        .project
        .document
        .table("TASKRSRC")
        .unwrap()
        .rows
        .clone()
}

fn steps(d: &Driver) -> usize {
    d.app.history.counts().0
}

fn undo(d: &mut Driver) {
    d.key_with(Key::Z, ctrl());
    d.frames(2);
}

/// Click a cell, F2, type `text`, Enter.
fn type_cell(d: &mut Driver, at: Pos2, text: &str) {
    d.click_at(at);
    d.key(Key::F2);
    d.frames(1);
    assert!(
        d.app.panes.sheet_cells.editing.is_some(),
        "F2 opens the cell"
    );
    d.type_text(text);
    d.key(Key::Enter);
    d.frames(2);
}

#[test]
fn workflow_typing_a_period_cell_sets_that_period_alone() {
    let mut d = sheet(
        "qa_usage_cells_type",
        crate::views::resources::tests::with_resource(),
    );
    let mon = monday(&d);
    let line = line_of(&d, 0);
    close(&week(&d, line, mon), &[8., 8., 8.]);
    let (start, start_task, start_steps) = (rows(&d), task(&d, "remain_work_qty"), steps(&d));

    // Tuesday: F2, 20, Enter.
    let at = cell(&d, line, mon + DAY);
    type_cell(&mut d, at, "20");
    assert!(d.app.panes.sheet_cells.editing.is_none(), "Enter commits");
    assert_eq!(get(&d, 0, "remain_crv"), "8:8;20:8;8:8");
    assert_eq!(get(&d, 0, "target_crv"), "8:8;20:8;8:8");
    assert_eq!(get(&d, 0, "remain_qty"), "36");
    assert_eq!(get(&d, 0, "target_qty"), "36");
    assert_eq!(get(&d, 0, "remain_qty_per_hr"), "1.5");
    assert_eq!(task(&d, "remain_work_qty"), "36");
    assert_eq!(task(&d, "target_work_qty"), "36");
    let label = curves::curve_label(&d.app.project.document, 0);
    assert_eq!(label.as_deref(), Some("Manual"));
    close(&week(&d, line, mon), &[8., 20., 8.]);
    assert_eq!(steps(&d), start_steps + 1, "one undo step");

    // A key on a selected cell opens it; that key is not typed.
    d.click_at(cell(&d, line, mon + 2 * DAY));
    d.type_text("9");
    d.frames(1);
    let open = d.app.panes.sheet_cells.editing.clone();
    assert!(open.is_some(), "a typed key opens the cell");
    assert!(!open.unwrap().contains('9'), "the key itself is not typed");
    d.type_text("12");
    d.key(Key::Enter);
    d.frames(2);
    close(&week(&d, line, mon), &[8., 20., 12.]);
    assert_eq!(get(&d, 0, "remain_qty"), "40");
    assert_eq!(steps(&d), start_steps + 2);

    // The resource line takes no value (periods before the data date are
    // refused by the same check, `xer/tests/period_units.rs`).
    let at = cell(&d, line, mon);
    let band = d.rect("usage_assignment_row_0");
    d.click_at(Pos2::new(at.x, band.center().y));
    d.key(Key::F2);
    d.frames(1);
    assert!(d.app.panes.sheet_cells.editing.is_none());
    assert!(
        d.app.status.contains("assignment's line"),
        "{}",
        d.app.status
    );
    assert_eq!(steps(&d), start_steps + 2, "a refusal changes nothing");

    // The runs travel in the XER file.
    let back = xer_round_trip(&mut d, "usage-cells.xer");
    let runs = records::get(&back.document, "TASKRSRC", 0, "remain_crv");
    assert_eq!(runs, get(&d, 0, "remain_crv"));
    assert_eq!(runs, "8:8;20:8;12:8");

    // Undo twice: as before.
    undo(&mut d);
    undo(&mut d);
    assert_eq!(rows(&d), start, "undo restores the assignment");
    assert_eq!(task(&d, "remain_work_qty"), start_task);
    close(&week(&d, line, mon), &[8., 8., 8.]);
    d.assert_clean("typed period units");
}

/// `with_resource` with a second Pipefitter assignment on RB-1010 (24
/// units, 8 a day).
fn two_assignments() -> ProjectData {
    let mut p = crate::views::resources::tests::with_resource();
    p.document.table_mut("TASKRSRC").add(&[
        ("taskrsrc_id", "2".into()),
        ("task_id", "1010".into()),
        ("proj_id", "1".into()),
        ("rsrc_id", "7".into()),
        ("remain_qty", "24".into()),
        ("remain_qty_per_hr", "1".into()),
        ("target_qty", "24".into()),
        ("rollup_dates_flag", "N".into()),
    ]);
    p.rebuild_assignments();
    p
}

fn right_click(d: &mut Driver, at: Pos2) {
    d.press_at(at, egui::PointerButton::Secondary, Default::default());
    d.frames(1);
}

#[test]
fn workflow_fill_and_range_commands_work_on_the_cells() {
    let mut d = sheet("qa_usage_cells_commands", two_assignments());
    let mon = monday(&d);
    let (first, second) = (line_of(&d, 0), line_of(&d, 1));
    let start = rows(&d);
    let base = steps(&d);

    // The menu, in the reference's order.
    let at = cell(&d, first, mon);
    right_click(&mut d, at);
    let order = [
        "show_all_projects",
        "timescale",
        "options",
        "user_preferences",
        "zoom_in",
        "zoom_out",
        "fill_down",
        "fill_across",
        "range_copy",
        "range_paste",
        "spreadsheet_fields",
    ]
    .map(|k| format!("usage_sheet_menu_{k}"));
    let tops: Vec<f32> = order.iter().map(|k| d.rect(k).top()).collect();
    assert!(tops.windows(2).all(|w| w[0] < w[1]), "{order:?} {tops:?}");
    // Spreadsheet Fields: three fields and Customize....
    d.click_at(d.rect("usage_sheet_menu_spreadsheet_fields").center());
    d.frames(2);
    for k in ["fields_remaining", "fields_actual", "fields_planned"] {
        assert!(d.has(&format!("usage_sheet_menu_{k}")), "{k}");
    }
    assert!(d.has("usage_sheet_menu_time_fields_customize"));
    d.key(Key::Escape);
    d.frames(2);

    // Monday of the first line to 10, then Monday to Wednesday selected and
    // Ctrl+H: Fill Across.
    let at = cell(&d, first, mon);
    type_cell(&mut d, at, "10");
    d.click_at(cell(&d, first, mon));
    // Past the double-click time, so Shift+click is a click of its own.
    d.frames(5);
    let at = cell(&d, first, mon + 2 * DAY);
    d.press_at(at, egui::PointerButton::Primary, shift());
    d.frames(1);
    d.key_with(Key::H, ctrl());
    d.frames(2);
    close(&week(&d, first, mon), &[10., 10., 10.]);
    assert_eq!(steps(&d), base + 2, "Fill Across is one undo step");
    assert!(
        d.app.grid.dialogs.find.is_none(),
        "Ctrl+H stays with the cells"
    );

    // Shift+Ctrl+C (a copy event on Windows) keeps the three cells; Range
    // Paste from the second line's Monday writes them there.
    let both = egui::Modifiers {
        shift: true,
        ..ctrl()
    };
    let _ = d.frame_with(vec![egui::Event::Copy], both);
    d.frames(1);
    assert!(d.app.panes.sheet_cells.copied.is_some(), "Range Copy");
    assert_eq!(steps(&d), base + 2, "Range Copy is no edit");
    d.click_at(cell(&d, second, mon));
    let at = cell(&d, second, mon);
    right_click(&mut d, at);
    d.click_at(d.rect("usage_sheet_menu_range_paste").center());
    d.frames(2);
    close(&week(&d, second, mon), &[10., 10., 10.]);
    assert_eq!(get(&d, 1, "remain_qty"), "30");
    assert_eq!(steps(&d), base + 3, "Range Paste is one undo step");

    // Tuesday of the first line to 4; both lines' three days selected and
    // Fill Down from the menu copies the first line into the second.
    let at = cell(&d, first, mon + DAY);
    type_cell(&mut d, at, "4");
    d.click_at(cell(&d, first, mon));
    d.frames(5);
    let at = cell(&d, second, mon + 2 * DAY);
    d.press_at(at, egui::PointerButton::Primary, shift());
    d.frames(1);
    let at = cell(&d, second, mon + DAY);
    right_click(&mut d, at);
    d.click_at(d.rect("usage_sheet_menu_fill_down").center());
    d.frames(2);
    close(&week(&d, second, mon), &[10., 4., 10.]);
    assert_eq!(get(&d, 1, "remain_crv"), "10:8;4:8;10:8");
    assert_eq!(steps(&d), base + 5, "Fill Down is one undo step");

    for _ in 0..5 {
        undo(&mut d);
    }
    assert_eq!(rows(&d), start, "undo restores both assignments");
    d.assert_clean("cell commands");
}

#[test]
fn workflow_clearing_the_manual_curve_asks_first() {
    let mut d = sheet(
        "qa_usage_cells_clear",
        crate::views::resources::tests::with_resource(),
    );
    let mon = monday(&d);
    let line = line_of(&d, 0);
    let at = cell(&d, line, mon);
    type_cell(&mut d, at, "2");
    let doc = &d.app.project.document;
    assert!(openxer_xer::manual_units::is_manual(doc, 0));
    let manual = rows(&d);
    let ask = |d: &mut Driver| {
        d.app.detail.removal = Some(Removal::ManualCurve {
            row: 0,
            task_id: 1010,
            curve: None,
        });
        d.frames(2);
        assert!(
            d.has(&format!("{MANUAL_CURVE_ID}_yes")),
            "the question is up"
        );
        assert_eq!(
            crate::dialog_keys::top_default_label(&d.ctx).as_deref(),
            Some("No"),
            "default not observed: No"
        );
    };
    // No (Enter) keeps the typed periods.
    ask(&mut d);
    d.key(Key::Enter);
    d.frames(2);
    assert!(d.app.detail.removal.is_none());
    assert_eq!(rows(&d), manual);
    // Yes takes them off; the units stay and spread evenly.
    ask(&mut d);
    d.click(&format!("{MANUAL_CURVE_ID}_yes"));
    d.frames(2);
    assert_eq!(get(&d, 0, "remain_crv"), "");
    assert_eq!(get(&d, 0, "target_crv"), "");
    assert_eq!(get(&d, 0, "curv_id"), "");
    assert_eq!(get(&d, 0, "remain_qty"), "18");
    assert_eq!(curves::curve_label(&d.app.project.document, 0), None);
    close(&week(&d, line, mon), &[6., 6., 6.]);
    // Undo brings the Manual curve back.
    undo(&mut d);
    assert_eq!(rows(&d), manual);
    d.assert_clean("clear the manual curve");
}
