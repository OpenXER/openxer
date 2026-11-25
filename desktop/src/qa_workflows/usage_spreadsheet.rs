//! Workflow: the usage spreadsheets show the time-phased fields the user
//! picks (Spreadsheet Fields > Customize..., the Columns dialog's chooser),
//! one line per field under each activity, resource and assignment, and
//! the choice is kept with the layout.
//!
//! - Activity Usage Spreadsheet: right-click a row, Spreadsheet Fields >
//!   Customize..., add Planned Nonlabor and Cumulative Remaining Labor to
//!   the starting Remaining Labor, OK. Each activity row then has three
//!   lines with the right totals and running totals, and the value column
//!   is titled with the measure.
//! - Cancel and Escape leave the fields as they were; Default goes back to
//!   the one starting field.
//! - Resource Usage Spreadsheet: its own list (Planned added), a resource
//!   line and an assignment line per field.
//! - The choice survives the layout's stored form.
//!
//! The fields are view settings, not project data: nothing here is an undo
//! step, and the project is checked unchanged at the end.
use super::*;
use crate::grid;
use crate::panes::BottomView;

/// `with_resource` (24 labor hours of PIPE on activity 1010) with 16 hours
/// of the Crane (nonlabor) on the same activity.
fn crew_and_crane() -> ProjectData {
    let mut p = crate::views::resources::tests::with_resource();
    p.document.table_mut("TASKRSRC").add(&[
        ("taskrsrc_id", "2".into()),
        ("task_id", "1010".into()),
        ("proj_id", "1".into()),
        ("rsrc_id", "8".into()),
        ("remain_qty", "16".into()),
        ("remain_qty_per_hr", "1".into()),
        ("target_qty", "16".into()),
        ("rollup_dates_flag", "N".into()),
    ]);
    p
}

fn secondary(d: &mut Driver, pos: egui::Pos2) {
    d.press_at(pos, egui::PointerButton::Secondary, Default::default());
}

/// Right-click `row`, then Spreadsheet Fields... (the Activity Usage
/// Spreadsheet's plain item) or Spreadsheet Fields > Customize... (the
/// Resource Usage Spreadsheet's submenu).
fn customize_from(d: &mut Driver, row: &str) {
    let r = d.rect(row);
    secondary(d, egui::Pos2::new(r.left() + 20., r.center().y));
    assert!(
        d.has("usage_sheet_menu_spreadsheet_fields"),
        "the spreadsheet menu has Spreadsheet Fields"
    );
    d.click_at(d.rect("usage_sheet_menu_spreadsheet_fields").center());
    d.frames(2);
    if d.app.panes.time_fields.is_none() {
        d.click_at(d.rect("usage_sheet_menu_time_fields_customize").center());
        d.frames(2);
    }
    assert!(d.app.panes.time_fields.is_some(), "Customize... opens");
    assert!(d.has("time_fields_ok"));
}

/// Opens `id`'s group of Available Options (closing the other), picks the
/// field and adds it with the right arrow.
fn add_field(d: &mut Driver, id: &str) {
    let group = if id.starts_with("cum_") {
        "Cumulative"
    } else {
        "Time Interval"
    };
    for g in ["Time Interval", "Cumulative"] {
        let open = d
            .app
            .panes
            .time_fields
            .as_ref()
            .is_some_and(|t| t.open_groups.contains(g));
        if open != (g == group) {
            d.click_at(d.rect(&format!("time_fields_group_{g}")).center());
            d.frames(1);
        }
    }
    d.click_at(d.rect(&format!("time_fields_available_{id}")).center());
    d.frames(1);
    d.click_at(d.rect("time_fields_add").center());
    d.frames(2);
}

/// The number of activity `i`'s row on the activity spreadsheet.
fn activity_row(d: &Driver, i: usize) -> usize {
    d.app
        .rows()
        .iter()
        .position(|r| matches!(r, grid::Row::Activity(x) if *x == i))
        .expect("the activity's row")
}

fn text_shown(output: &egui::FullOutput, text: &str) -> bool {
    output
        .shapes
        .iter()
        .any(|s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.text() == text))
}

#[test]
fn workflow_usage_spreadsheets_show_the_chosen_time_phased_fields() {
    let mut d = Driver::new("qa_usage_time_fields", crew_and_crane());
    d.home();
    let steps = d.app.history.counts();
    d.app.set_bottom(Some(BottomView::ActivityUsageSheet));
    d.frames(4);
    // The activity both resources work on.
    let i = crate::views::assignments::assignments(&d.app.project)[0].task;
    let row = format!("usage_sheet_row_{}", activity_row(&d, i));

    // The spreadsheet starts with remaining labor units alone.
    let one = d.frame(vec![]);
    assert!(
        text_shown(&one, "Remaining Labor Units"),
        "one field's title"
    );

    // Cancel leaves the fields alone.
    customize_from(&mut d, &row);
    add_field(&mut d, "planned_nonlabor");
    d.click_at(d.rect("time_fields_cancel").center());
    d.frames(2);
    assert!(d.app.panes.time_fields.is_none());
    assert_eq!(d.app.panes.usage.activity_time_fields, ["remaining_labor"]);
    // So does Escape.
    customize_from(&mut d, &row);
    add_field(&mut d, "planned_nonlabor");
    d.key(Key::Escape);
    d.frames(2);
    assert!(d.app.panes.time_fields.is_none(), "Escape closes it");
    assert_eq!(d.app.panes.usage.activity_time_fields, ["remaining_labor"]);

    // Add two fields and OK: each goes after the picked selected one.
    customize_from(&mut d, &row);
    add_field(&mut d, "planned_nonlabor");
    add_field(&mut d, "cum_remaining_labor");
    d.click_at(d.rect("time_fields_ok").center());
    d.frames(3);
    assert!(d.app.panes.time_fields.is_none());
    assert_eq!(
        d.app.panes.usage.activity_time_fields,
        ["remaining_labor", "planned_nonlabor", "cum_remaining_labor"]
    );

    // Three lines under the activity, with the right totals.
    let periods = d.app.usage_export_periods();
    let fields = crate::panes::usage_fields::fields_of(
        &d.app.project,
        crate::panes::usage_fields::Which::Activity,
        &d.app.panes.usage.activity_fields,
    );
    let ix = d.app.shared_index();
    let line = d.app.activity_sheet_line(
        grid::Row::Activity(i),
        &fields,
        &ix,
        &periods,
        d.app.project_serial(),
        false,
    );
    let labels: Vec<&str> = line.lines.iter().map(|f| f.label.as_str()).collect();
    assert_eq!(
        labels,
        [
            "Remaining Labor",
            "Planned Nonlabor",
            "Cumulative Remaining Labor"
        ]
    );
    let totals: Vec<f64> = line.lines.iter().map(|f| f.total).collect();
    assert_eq!(totals, [24., 16., 24.], "labor 24 h, crane 16 h");
    for f in &line.lines {
        let sum = f.before + f.values.iter().sum::<f64>() + f.after;
        assert!((sum - f.total).abs() < 1e-9, "{}: {sum}", f.label);
    }
    // The cumulative line runs up to the total.
    let shown = d.app.panes.usage.shown_field(&line.lines[2]);
    assert!((shown.last().copied().unwrap_or(0.) - 24.).abs() < 1e-9);
    assert!(shown.windows(2).all(|w| w[1] >= w[0] - 1e-9), "running");
    // Drawn: the measure over the value column, each line its field.
    let three = d.frame(vec![]);
    assert!(text_shown(&three, "Units"), "the value column's title");
    for label in labels {
        assert!(text_shown(&three, label), "{label} is drawn");
    }

    // Kept with the layout.
    let stored = d.app.panes.usage.json();
    let mut again = crate::panes::usage::UsageState::default();
    again.restore(&stored);
    assert_eq!(
        again.activity_time_fields,
        d.app.panes.usage.activity_time_fields
    );

    // Default goes back to the starting field.
    customize_from(&mut d, &row);
    d.click_at(d.rect("time_fields_default").center());
    d.frames(1);
    d.click_at(d.rect("time_fields_ok").center());
    d.frames(2);
    assert_eq!(d.app.panes.usage.activity_time_fields, ["remaining_labor"]);

    // The Resource Usage Spreadsheet keeps its own list.
    d.app.set_bottom(Some(BottomView::ResourceUsageSheet));
    d.app.panes.usage.resource = Some(7);
    d.frames(4);
    customize_from(&mut d, "usage_assignment_row_0");
    assert!(
        !d.has("time_fields_available_planned_labor"),
        "no resource kinds on the resource spreadsheet"
    );
    add_field(&mut d, "planned");
    d.click_at(d.rect("time_fields_ok").center());
    d.frames(3);
    assert_eq!(
        d.app.panes.usage.resource_time_fields,
        ["remaining", "planned"]
    );
    assert_eq!(d.app.panes.usage.activity_time_fields, ["remaining_labor"]);
    let afields = crate::panes::usage_fields::fields_of(
        &d.app.project,
        crate::panes::usage_fields::Which::Assignment,
        &d.app.panes.usage.assignment_fields,
    );
    let rows = d.app.selection_sheet_rows(&afields, &periods).unwrap();
    let got: Vec<(bool, usize, f64)> = rows.iter().map(|r| (r.band, r.field, r.total)).collect();
    assert_eq!(
        got,
        [
            (true, 0, 24.),
            (true, 1, 24.),
            (false, 0, 24.),
            (false, 1, 24.)
        ],
        "the resource, then its assignment, a line per field"
    );
    let drawn = d.frame(vec![]);
    for label in ["Units", "Remaining", "Planned"] {
        assert!(text_shown(&drawn, label), "{label} is drawn");
    }
    // A view setting only: no undo step was made.
    assert_eq!(d.app.history.counts(), steps, "no project change");
    d.assert_clean("time-phased fields");
}
