//! Workflow: the right-click menus follow the reference's, through real
//! right-clicks on the production UI.
//!
//! - Activities: a row, several rows, a group band and a column title open
//!   one table menu (27 entries, the same order and separators); the
//!   greyed items follow the selection (several rows enable Fill Down,
//!   Link and Unlink and grey Dissolve; a band greys the selection
//!   commands and the Assign items). The empty chart area and the
//!   timescale open one chart menu.
//! - WBS, Projects and Resources: the table menu of a row in the
//!   reference's order (items OpenXER does not have left out, its own kept
//!   where they were); the WBS chart opens its short chart menu.
//! - End to end: Dissolve from a one-row menu opens its confirmation, and
//!   Bars... from the timescale menu opens the Bars dialog.
//!
//! Each menu is read from the shared menu's log of what it drew
//! (`context_menu::drawn`), so separators and greyed states are compared
//! as drawn, not as built.
use super::*;
use crate::context_menu::Drawn;
use crate::grid::layout::GroupLevel;

/// A separator in an expected menu.
const SEP: &str = "----";

/// The Activities table menu, the same for a row, several rows, a band,
/// the empty area and a column title.
const ACTIVITIES_TABLE: [&str; 27] = [
    "Activity Details",
    SEP,
    "Cut",
    "Copy",
    "Paste",
    "Fill Down",
    SEP,
    "Add",
    "Delete",
    "Dissolve",
    "Renumber Activity IDs",
    "Export to Excel",
    SEP,
    "Assign",
    "Link Activities",
    "Unlink Activities",
    SEP,
    "Columns...",
    "Line Numbers",
    "Table Font and Row...",
    SEP,
    "Filters...",
    "Group and Sort...",
    SEP,
    "Expand All",
    "Collapse All",
    "Collapse To...",
];

/// The Activities chart menu: a bar, the empty chart area, the timescale.
const ACTIVITIES_CHART: [&str; 6] = [
    "Activity Details",
    SEP,
    "Bars...",
    "Bar Chart Options...",
    "Timescale...",
    "Attachments",
];

/// The WBS table menu. The reference's Columns sets, Table Font and Row,
/// the chart view items, Group and Sort By and Collapse To wait on
/// features OpenXER lacks; the Shift group is OpenXER's own.
const WBS_TABLE: [&str; 23] = [
    "WBS Details",
    SEP,
    "Cut",
    "Copy",
    "Paste",
    "Fill Down",
    SEP,
    "Add",
    "Delete",
    SEP,
    "Shift up",
    "Shift down",
    "Shift left",
    "Shift right",
    SEP,
    "Columns",
    "Table Font and Row...",
    SEP,
    "Group and Sort By",
    SEP,
    "Expand All",
    "Collapse All",
    "Collapse To...",
];

/// The WBS chart menu (Bars... and Timescale... wait on a WBS chart
/// dialog).
const WBS_CHART: [&str; 1] = ["WBS Details"];

/// The Projects table menu of a project row.
const PROJECTS_TABLE: [&str; 22] = [
    "Open Project",
    "Close Project",
    "Project Details",
    "Edit Project Details",
    SEP,
    "Cut",
    "Copy",
    "Paste",
    "Fill Down",
    SEP,
    "Add",
    "Delete",
    SEP,
    "Columns",
    "Table Font and Row...",
    SEP,
    "Filter By",
    "Group and Sort By",
    SEP,
    "Expand All",
    "Collapse All",
    "Collapse To...",
];

/// The Resources table menu.
const RESOURCES_TABLE: [&str; 18] = [
    "Details",
    SEP,
    "Cut",
    "Copy",
    "Paste",
    "Fill Down",
    SEP,
    "Add",
    "Delete",
    "Delete or Merge...",
    SEP,
    "Columns",
    SEP,
    "Filter By",
    "Group and Sort By",
    SEP,
    "Expand All",
    "Collapse All",
];

fn right_click(d: &mut Driver, at: Pos2) {
    d.press_at(at, egui::PointerButton::Secondary, Default::default());
}

/// The entries the open menu `prefix` drew at `depth`, as drawn.
fn entries(d: &Driver, prefix: &str, depth: usize) -> Vec<Drawn> {
    crate::context_menu::drawn(&d.ctx)
        .into_iter()
        .filter(|e| e.prefix == prefix && e.depth == depth)
        .collect()
}

/// The open menu's captions in order, `----` for a separator.
fn captions(d: &Driver, prefix: &str) -> Vec<String> {
    entries(d, prefix, 0)
        .iter()
        .map(|e| {
            if e.separator {
                SEP.to_string()
            } else {
                e.label.clone()
            }
        })
        .collect()
}

#[track_caller]
fn assert_menu(d: &Driver, prefix: &str, want: &[&str], what: &str) {
    assert_eq!(
        captions(d, prefix),
        want,
        "{what}: items, order, separators"
    );
}

/// Whether the item `label` of the open menu is enabled.
#[track_caller]
fn enabled(d: &Driver, prefix: &str, label: &str) -> bool {
    entries(d, prefix, 0)
        .iter()
        .find(|e| e.label == label)
        .unwrap_or_else(|| panic!("{prefix}: no {label:?}"))
        .enabled
}

/// The labels of the open menu that are greyed.
fn greyed(d: &Driver, prefix: &str) -> Vec<String> {
    entries(d, prefix, 0)
        .iter()
        .filter(|e| !e.separator && !e.enabled)
        .map(|e| e.label.clone())
        .collect()
}

fn close(d: &mut Driver) {
    d.key(Key::Escape);
    d.frames(2);
}

/// An activity with a predecessor and a successor (Dissolve applies),
/// shown in the table.
fn dissolvable(d: &Driver) -> usize {
    let model = &d.app.project.model;
    (0..model.activities.len())
        .find(|&i| crate::editing::ops::can_dissolve(model, i))
        .expect("the sample has an activity with a predecessor and a successor")
}

#[test]
fn workflow_activities_menus_match_the_reference() {
    let mut d = fresh("wf-ctx-activities");
    assert!(d.app.gantt_on_screen(), "the Gantt chart is on screen");

    // One row: the 27 entries; Paste (nothing copied), Fill Down, Link,
    // Unlink and Expand All (nothing collapsed) greyed.
    let i = dissolvable(&d);
    select_row(&mut d, i);
    let at = row_cell(&mut d, i, "activity_name");
    right_click(&mut d, at);
    assert_eq!(d.app.selected, Some(i), "the row stays selected");
    assert_menu(&d, "ctx", &ACTIVITIES_TABLE, "one row");
    assert_eq!(
        greyed(&d, "ctx"),
        [
            "Paste",
            "Fill Down",
            "Link Activities",
            "Unlink Activities",
            "Expand All"
        ],
        "one row: greyed items"
    );
    assert!(enabled(&d, "ctx", "Dissolve"), "one dissolvable row");
    // End to end: Dissolve runs the shared command (its confirmation).
    d.click("ctx_dissolve");
    assert!(
        d.app.editing.dialogs.dissolve.is_some(),
        "Dissolve asks first"
    );
    close(&mut d);
    assert!(d.app.editing.dialogs.dissolve.is_none());

    // Several rows (click, then Ctrl+click a second dissolvable one, so
    // each row alone could be dissolved): Fill Down, Link and Unlink come
    // on, Dissolve goes grey.
    let shown = d.app.rows();
    let model = &d.app.project.model;
    let j = (0..model.activities.len())
        .filter(|&j| j != i && shown.contains(&grid::Row::Activity(j)))
        .find(|&j| crate::editing::ops::can_dissolve(model, j))
        .expect("a second activity with a predecessor and a successor");
    select_row(&mut d, i);
    let other = row_cell(&mut d, j, "activity_name");
    d.press_at(other, egui::PointerButton::Primary, ctrl());
    assert_eq!(d.app.selected_in_order().len(), 2, "two rows");
    assert!(d.app.can_dissolve_selection(), "each row alone dissolves");
    right_click(&mut d, other);
    assert_eq!(d.app.selected_in_order().len(), 2, "the selection stays");
    assert_menu(&d, "ctx", &ACTIVITIES_TABLE, "several rows");
    for label in ["Fill Down", "Link Activities", "Unlink Activities"] {
        assert!(enabled(&d, "ctx", label), "several rows: {label} on");
    }
    assert!(
        !enabled(&d, "ctx", "Dissolve"),
        "several rows: Dissolve off"
    );
    assert!(!enabled(&d, "ctx", "Paste") && !enabled(&d, "ctx", "Expand All"));
    close(&mut d);

    // A column title: no menu of its own, the table menu for the current
    // selection; neither the sort nor the selection changes.
    let sort = d.app.grid.layout.group_sort.clone();
    let picked = d.app.selected_in_order();
    let title = d.rect("grid_header_activity_name").center();
    right_click(&mut d, title);
    assert_menu(&d, "header_menu", &ACTIVITIES_TABLE, "column title");
    assert_eq!(d.app.grid.layout.group_sort, sort, "no sort change");
    assert_eq!(d.app.selected_in_order(), picked, "no selection change");
    close(&mut d);

    // A group band: selected by the right-click, the same 27 entries; the
    // commands that need an activity are greyed, and Assign opens with
    // its six items greyed.
    d.app.grid.layout.group_sort.levels = vec![GroupLevel {
        column: "activity_status".into(),
        to_level: None,
        indent: true,
        interval: String::new(),
    }];
    d.app.drop_frame_cache();
    d.frames(3);
    let band = d.rect("grid_band_0");
    right_click(&mut d, Pos2::new(band.left() + 120., band.center().y));
    assert!(d.app.grid.cells.selected_band.is_some(), "the band selects");
    assert_menu(&d, "band", &ACTIVITIES_TABLE, "group band");
    let off = greyed(&d, "band");
    for label in [
        "Cut",
        "Paste",
        "Fill Down",
        "Delete",
        "Dissolve",
        "Renumber Activity IDs",
        "Link Activities",
        "Unlink Activities",
        "Expand All",
    ] {
        assert!(off.contains(&label.to_string()), "band: {label} greyed");
    }
    for label in [
        "Activity Details",
        "Add",
        "Export to Excel",
        "Assign",
        "Columns...",
        "Line Numbers",
        "Table Font and Row...",
        "Filters...",
        "Group and Sort...",
        "Collapse All",
        "Collapse To...",
    ] {
        assert!(enabled(&d, "band", label), "band: {label} enabled");
    }
    let assign = d.rect("band_assign").center();
    let _ = d.frame(vec![egui::Event::PointerMoved(assign)]);
    d.frames(3);
    let items = entries(&d, "band", 1);
    assert_eq!(items.len(), 6, "Assign lists six items: {items:?}");
    assert!(items.iter().all(|e| !e.enabled), "all greyed on a band");
    close(&mut d);
    close(&mut d);

    // The empty chart area and the timescale: one chart menu.
    let last = d
        .app
        .bar_geometry
        .iter()
        .map(|b| b.row.bottom())
        .fold(0., f32::max);
    let x = d.app.bar_geometry[0].start.x + 4.;
    right_click(&mut d, Pos2::new(x, last + 30.));
    assert_menu(&d, "gantt_menu", &ACTIVITIES_CHART, "empty chart area");
    close(&mut d);
    for band in ["timescale_zoom", "timescale_pan"] {
        let at = d.rect(band).center();
        right_click(&mut d, at);
        assert_menu(&d, "gantt_menu", &ACTIVITIES_CHART, band);
        close(&mut d);
    }
    // End to end: Bars... from the timescale menu opens the Bars dialog.
    let at = d.rect("timescale_zoom").center();
    right_click(&mut d, at);
    d.click("gantt_menu_bars");
    assert!(d.app.gantt_ui.bars.is_some(), "Bars... opens");
    d.assert_clean("chart menu");
}

#[test]
fn workflow_wbs_projects_and_resources_menus_match_the_reference() {
    let mut d = fresh("wf-ctx-views");

    // WBS: a row's table menu; the chart's short menu.
    show(&mut d, "wbs");
    let r = d.rect("wbs_row_1");
    right_click(&mut d, Pos2::new(r.left() + 20., r.center().y));
    assert_menu(&d, "wbs_menu", &WBS_TABLE, "WBS row");
    assert!(
        !enabled(&d, "wbs_menu", "Fill Down"),
        "greyed in this window"
    );
    assert!(enabled(&d, "wbs_menu", "Delete"), "a WBS element");
    close(&mut d);
    // A column title opens the same menu.
    let title = d.rect("wbs_col_1").center();
    right_click(&mut d, title);
    assert_menu(&d, "wbs_menu", &WBS_TABLE, "WBS column title");
    close(&mut d);
    let chart = d.rect("wbs_chart");
    right_click(&mut d, Pos2::new(chart.center().x, chart.bottom() - 20.));
    assert_menu(&d, "wbs_chart_menu", &WBS_CHART, "WBS chart");
    close(&mut d);
    let scale = d.rect("wbs_timescale_zoom").center();
    right_click(&mut d, scale);
    assert_menu(&d, "wbs_chart_menu", &WBS_CHART, "WBS timescale");
    close(&mut d);

    // Projects: a project row (the EPS rows have the same menu).
    show(&mut d, "projects");
    let rows = d
        .app
        .action_rects
        .keys()
        .filter(|k| k.starts_with("projects_row_"))
        .count();
    let mut found = false;
    for i in 0..rows {
        d.app.views.projects.selected = None;
        let r = d.rect(&format!("projects_row_{i}"));
        right_click(&mut d, Pos2::new(r.left() + 90., r.center().y));
        if d.app.views.projects.selected.is_some() {
            found = true;
            break;
        }
        close(&mut d);
    }
    assert!(found, "a project row");
    assert_menu(&d, "projects_menu", &PROJECTS_TABLE, "project row");
    assert!(enabled(&d, "projects_menu", "Open Project"));
    assert!(!enabled(&d, "projects_menu", "Fill Down"));
    close(&mut d);

    d.assert_clean("WBS and Projects menus");

    // Resources: a row (the sample has no resources; this project has).
    let mut d = Driver::new(
        "wf-ctx-resources",
        crate::views::resources::tests::with_resource(),
    );
    show(&mut d, "resources");
    d.app.views.resources.assigned_only = false;
    d.frames(3);
    let r = d.rect("resources_row_0");
    right_click(&mut d, Pos2::new(r.left() + 90., r.center().y));
    assert_menu(&d, "resources_menu", &RESOURCES_TABLE, "resource row");
    assert!(!enabled(&d, "resources_menu", "Fill Down"));
    close(&mut d);
    d.assert_clean("view menus");
}
