//! Workflow: a first start looks like the reference's first launch. On a
//! fresh workspace with the sample project the Activities table shows the
//! reference columns, groups by WBS under a project band, shows dates
//! without a time and durations without a unit; the details tab strip is
//! General, Status, Resources, Predecessors, Successors; the timescale reads
//! quarters over months. A preference changed in User Preferences is kept
//! across a restart, and the other defaults stay as they were.
use super::*;
use crate::format::TimeDisplay;
use crate::grid::columns;

/// A first start with the reference defaults, on the first-run sample.
fn first_start(name: &str) -> Driver {
    crate::defaults::use_reference();
    Driver::new(name, openxer_xer::sample_project::sample_project().current)
}

/// The text the table shows for activity `i` in column `id`.
fn shown(d: &Driver, i: usize, id: &str) -> String {
    let p = &d.app.project;
    let catalog = columns::catalog(p);
    let ix = columns::Index::new((false, 0.));
    let column = columns::find(&catalog, id).unwrap_or_else(|| panic!("column {id}"));
    columns::display(p, Some(i), &columns::value(p, &ix, column, i))
}

/// Drawn controls whose action starts with `prefix`, left to right, without
/// the prefix.
fn drawn_in_order(d: &Driver, prefix: &str) -> Vec<String> {
    let mut found: Vec<(f32, String)> = d
        .app
        .action_rects
        .iter()
        .filter_map(|(k, r)| k.strip_prefix(prefix).map(|rest| (r[0], rest.to_string())))
        .collect();
    found.sort_by(|a, b| a.0.total_cmp(&b.0));
    found.into_iter().map(|(_, k)| k).collect()
}

/// The Activities table's header cells, left to right.
fn header(d: &Driver) -> Vec<String> {
    let ids: Vec<String> = columns::catalog(&d.app.project)
        .iter()
        .map(|c| c.id.clone())
        .collect();
    drawn_in_order(d, "grid_header_")
        .into_iter()
        .filter(|id| ids.contains(id))
        .collect()
}

/// The details tab strip, left to right, by tab key.
fn tab_strip(d: &Driver) -> Vec<String> {
    let keys: Vec<&str> = crate::details::DetailTab::ALL
        .iter()
        .map(|t| t.key())
        .collect();
    drawn_in_order(d, "detail_tab_")
        .into_iter()
        .filter(|k| keys.contains(&k.as_str()))
        .collect()
}

/// The words the timescale header paints in one frame: (upper, lower).
fn timescale_text(d: &mut Driver) -> (Vec<String>, Vec<String>) {
    let output = d.frame(vec![]);
    let header = d.rect("timescale_pan").union(d.rect("timescale_zoom"));
    let middle = header.center().y;
    let (mut upper, mut lower) = (vec![], vec![]);
    for clipped in &output.shapes {
        let egui::Shape::Text(shape) = &clipped.shape else {
            continue;
        };
        let rect = shape.galley.rect.translate(shape.pos.to_vec2());
        if !header.contains(rect.center()) {
            continue;
        }
        let text = shape.galley.text().to_string();
        if rect.center().y < middle {
            upper.push(text);
        } else {
            lower.push(text);
        }
    }
    (upper, lower)
}

/// A task of the sample with dates and work left.
fn a_task(d: &Driver) -> usize {
    let m = &d.app.project.model;
    (0..m.activities.len())
        .find(|&i| m.activities[i].rem > 0 && m.activities[i].early_start.is_some())
        .expect("a task with dates")
}

const REFERENCE_COLUMNS: [&str; 8] = [
    "activity_id",
    "activity_name",
    "original_duration",
    "remaining_duration",
    "schedule_percent_complete",
    "start",
    "finish",
    "total_float",
];
const REFERENCE_TABS: [&str; 5] = [
    "general",
    "status",
    "resources",
    "predecessors",
    "successors",
];
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

#[test]
fn workflow_a_first_start_shows_the_reference_defaults() {
    let mut d = first_start("wf-reference-defaults");
    d.frames(2);
    d.assert_clean("first start");

    // The Activities table: columns, grouping, dates and durations.
    assert_eq!(header(&d), REFERENCE_COLUMNS, "the default columns");
    let _ = d.app.rows();
    {
        let bands = d.app.grid.bands.borrow();
        let project = views::wbs::project_node(&d.app.project).expect("a project node");
        assert_eq!(
            bands[0].target,
            grid::group::BandTarget::Wbs(project),
            "grouped by WBS with the project as the top band"
        );
        assert_eq!(bands[0].level, 0);
        assert!(bands[1..].iter().all(|b| b.level >= 1));
    }
    let task = a_task(&d);
    let start = shown(&d, task, "start");
    assert!(
        !start.is_empty() && !start.contains(':'),
        "a date without a time: {start:?}"
    );
    let remaining = shown(&d, task, "remaining_duration");
    assert!(
        !remaining.is_empty() && remaining.chars().all(|c| c.is_ascii_digit()),
        "a duration without a unit: {remaining:?}"
    );
    assert!(d.app.grid.layout.table.show_icons, "activity icons");

    // The details tab strip.
    assert_eq!(tab_strip(&d), REFERENCE_TABS, "the details tabs");

    // The timescale: quarters over months.
    let (upper, lower) = timescale_text(&mut d);
    assert!(
        !upper.is_empty()
            && upper
                .iter()
                .all(|t| t.starts_with("Qtr") || t.starts_with('Q')),
        "quarters on top: {upper:?}"
    );
    assert!(
        !lower.is_empty()
            && lower
                .iter()
                .all(|t| MONTHS.iter().any(|m| t.starts_with(m))),
        "months below: {lower:?}"
    );

    // Edit > User Preferences > Dates: show the time (24 hour).
    d.home();
    d.click("menu_bar_edit");
    d.click("menu_user_preferences");
    d.click("prefs_tab_dates");
    d.click("prefs_time_24");
    d.click("prefs_close");
    d.frames(1);
    assert_eq!(d.app.formats.dates.time, TimeDisplay::Hour24);
    assert!(
        shown(&d, task, "start").contains(':'),
        "the time shows at once"
    );

    // After a restart the choice is kept and the rest is still the default.
    restart(&mut d);
    d.frames(2);
    assert_eq!(d.app.formats.dates.time, TimeDisplay::Hour24, "kept");
    let task = a_task(&d);
    assert!(
        shown(&d, task, "start").contains(':'),
        "dates show the time after the restart"
    );
    assert!(
        !d.app.formats.durations.show_label,
        "durations still without a unit"
    );
    assert_eq!(header(&d), REFERENCE_COLUMNS, "columns kept");
    assert_eq!(tab_strip(&d), REFERENCE_TABS, "details tabs kept");
    let (upper, _) = timescale_text(&mut d);
    assert!(
        upper.iter().all(|t| t.starts_with('Q')),
        "still quarters on top: {upper:?}"
    );
    d.assert_clean("after the restart");
}
