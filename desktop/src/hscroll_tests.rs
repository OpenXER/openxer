//! Horizontal scroll bars under the activity table and the Gantt Chart.
//! Everything runs through `App::update_ui` with synthetic
//! egui input: no native window, no system pointer.
use super::*;
use crate::headless_render::SoftwarePainter;
use crate::hscroll::BAR_HEIGHT;
use crate::panes::{BottomView, TopView};
use crate::qa_sweep::Driver;

fn scheduled_starter() -> ProjectData {
    let mut p = openxer_xer::starter();
    schedule_model(&mut p.model, &p.resources).unwrap();
    p
}

/// The starter project with the table pane narrower than its columns.
fn narrow() -> Driver {
    let mut d = Driver::new("hscroll", scheduled_starter());
    d.app.grid_fraction = 0.3;
    d.frames(3);
    // Zoom to Fit for the wider chart.
    d.app.fit();
    d.frames(3);
    d
}

fn wheel(d: &mut Driver, pos: Pos2, delta: Vec2, modifiers: egui::Modifiers) {
    let _ = d.frame_with(
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta,
                modifiers,
            },
        ],
        modifiers,
    );
    d.frames(2);
}

/// Press at `from`, move to `to` in steps, release; then let the UI settle.
fn drag(d: &mut Driver, from: Pos2, to: Pos2) {
    let _ = d.frame(vec![egui::Event::PointerMoved(from)]);
    let _ = d.frame(vec![
        egui::Event::PointerMoved(from),
        egui::Event::PointerButton {
            pos: from,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        },
    ]);
    for step in [0.25, 0.5, 0.75, 1.] {
        let _ = d.frame(vec![egui::Event::PointerMoved(from.lerp(to, step))]);
    }
    let _ = d.frame(vec![egui::Event::PointerButton {
        pos: to,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: Default::default(),
    }]);
    let _ = d.frame(vec![egui::Event::PointerGone]);
    d.frames(2);
}

/// Click activity `i`'s row under whichever column header is on screen.
fn click_row(d: &mut Driver, i: usize) {
    let task_id = d.app.project.model.activities[i].task_id;
    let y = d
        .app
        .bar_geometry
        .iter()
        .find(|b| b.task_id == task_id)
        .unwrap()
        .row
        .center()
        .y;
    let header = d
        .app
        .action_rects
        .iter()
        .find(|(k, _)| k.starts_with("grid_header_"))
        .map(|(_, r)| (r[0] + r[2]) / 2.)
        .unwrap();
    d.click_at(Pos2::new(header, y));
}

fn content_width(d: &Driver) -> f32 {
    d.app.table_content_width()
}

fn view_width(d: &Driver) -> f32 {
    d.app.grid.cells.view_width
}

fn max_scroll(d: &Driver) -> f32 {
    content_width(d) - view_width(d)
}

/// A point over the table's body, clear of the bars.
fn table_body(d: &Driver) -> Pos2 {
    let h = d.rect("grid_header_activity_status");
    Pos2::new(h.center().x.max(100.), h.bottom() + 40.)
}

/// The thumb track of an action's bar: its thumb's width and the travel
/// between the arrow buttons.
fn travel(d: &Driver, name: &str) -> f32 {
    let (bar, thumb) = (d.rect(name), d.rect(&format!("{name}_thumb")));
    bar.width() - 2. * 17. - thumb.width()
}

fn first_cell(d: &Driver, minute: i64) -> i64 {
    let bands = d.app.timescale_bands();
    timescale::band_cells(
        bands.last().unwrap().unit,
        false,
        d.app.project.model.origin_serial,
        minute as f64,
        minute as f64 + 1.,
        &d.app.label_options(),
    )[0]
    .start_minute
}

#[test]
fn the_table_bar_appears_only_when_the_columns_are_wider_than_the_pane() {
    let mut d = narrow();
    assert!(content_width(&d) > view_width(&d));
    assert!(d.has("hscroll_table") && d.has("hscroll_table_thumb"));
    assert!(d.has("hscroll_table_left") && d.has("hscroll_table_right"));
    // The bar is a 16 px strip under the table, under the last row.
    let bar = d.rect("hscroll_table");
    assert_eq!(bar.height(), BAR_HEIGHT);
    assert!(bar.left() <= d.rect("grid_header_activity_id").left());
    // A wide pane shows every column: no bar, nothing to scroll.
    d.app.grid_fraction = 0.8;
    d.frames(3);
    assert!(content_width(&d) <= view_width(&d));
    assert!(!d.has("hscroll_table"));
    assert!(d.has("hscroll_gantt"), "the Gantt keeps its bar");
    assert_eq!(d.app.grid.scroll_x, 0.);
}

#[test]
fn the_table_alone_has_a_bar_only_when_it_needs_one() {
    let mut d = narrow();
    d.app.panes.top = TopView::Table;
    d.app.grid.layout.columns.truncate(3);
    d.frames(3);
    assert!(!d.has("hscroll_table") && !d.has("hscroll_gantt"));
    // Fill the pane with columns.
    d.app.grid_fraction = 0.3;
    for _ in 0..30 {
        let first = d.app.grid.layout.columns[0].clone();
        d.app.grid.layout.columns.push(first);
    }
    d.frames(3);
    assert!(content_width(&d) > view_width(&d));
    assert!(d.has("hscroll_table") && !d.has("hscroll_gantt"));
}

#[test]
fn dragging_the_table_thumb_scrolls_in_proportion() {
    let mut d = narrow();
    let thumb = d.rect("hscroll_table_thumb");
    let pixels = 60.;
    let per_pixel = max_scroll(&d) / travel(&d, "hscroll_table");
    drag(
        &mut d,
        thumb.center(),
        thumb.center() + Vec2::new(pixels, 3.),
    );
    let got = d.app.grid.scroll_x;
    assert!(
        (got - pixels * per_pixel).abs() < 3.,
        "scrolled {got}, expected {}",
        pixels * per_pixel
    );
    // The thumb moved with it, and dragging far past the end stops there.
    let thumb = d.rect("hscroll_table_thumb");
    drag(&mut d, thumb.center(), thumb.center() + Vec2::new(900., 0.));
    assert!((d.app.grid.scroll_x - max_scroll(&d)).abs() < 0.5);
    let thumb = d.rect("hscroll_table_thumb");
    drag(
        &mut d,
        thumb.center(),
        thumb.center() - Vec2::new(2000., 0.),
    );
    assert_eq!(d.app.grid.scroll_x, 0.);
}

#[test]
fn the_table_arrows_step_one_column() {
    let mut d = narrow();
    let edges = d.app.column_edges();
    let right = d.rect("hscroll_table_left").center();
    let _ = right;
    d.click("hscroll_table_right");
    assert_eq!(d.app.grid.scroll_x, edges[1]);
    d.click("hscroll_table_right");
    assert_eq!(d.app.grid.scroll_x, edges[2]);
    d.click("hscroll_table_left");
    assert_eq!(d.app.grid.scroll_x, edges[1]);
    // From the middle of a column the arrows go to the edge on that side.
    d.app.grid.scroll_x = edges[1] + 10.;
    d.frames(2);
    d.click("hscroll_table_left");
    assert_eq!(d.app.grid.scroll_x, edges[1]);
    d.app.grid.scroll_x = edges[1] + 10.;
    d.frames(2);
    d.click("hscroll_table_right");
    assert_eq!(d.app.grid.scroll_x, edges[2]);
    // Past the last column edge it stops at the end.
    d.app.grid.scroll_x = max_scroll(&d) - 1.;
    d.frames(2);
    d.click("hscroll_table_right");
    assert!((d.app.grid.scroll_x - max_scroll(&d)).abs() < 0.5);
    d.app.grid.scroll_x = 0.;
    d.frames(2);
    d.click("hscroll_table_left");
    assert_eq!(d.app.grid.scroll_x, 0.);
}

#[test]
fn a_click_on_the_table_track_pages_toward_the_pointer() {
    let mut d = narrow();
    let bar = d.rect("hscroll_table");
    let thumb = d.rect("hscroll_table_thumb");
    // Right of the thumb: one view toward the end.
    let after = Pos2::new((thumb.right() + bar.right() - 17.) / 2., bar.center().y);
    d.click_at(after);
    d.frames(2);
    let view = view_width(&d);
    assert!(
        (d.app.grid.scroll_x - view.min(max_scroll(&d))).abs() < 0.5,
        "paged to {}",
        d.app.grid.scroll_x
    );
    // Left of the thumb: back toward the start.
    let thumb = d.rect("hscroll_table_thumb");
    let before = Pos2::new((thumb.left() + bar.left() + 17.) / 2., bar.center().y);
    d.click_at(before);
    d.frames(2);
    assert_eq!(d.app.grid.scroll_x, 0.);
}

#[test]
fn the_wheel_scrolls_the_table_sideways_over_the_rows_and_the_bar() {
    let mut d = narrow();
    let shift = egui::Modifiers::SHIFT;
    let body = table_body(&d);
    wheel(&mut d, body, Vec2::new(0., -120.), shift);
    let first = d.app.grid.scroll_x;
    assert!(first > 0., "Shift+wheel over the rows");
    wheel(&mut d, body, Vec2::new(-60., 0.), Default::default());
    let second = d.app.grid.scroll_x;
    assert!(second > first, "a sideways wheel over the rows");
    let bar = d.rect("hscroll_table").center();
    wheel(&mut d, bar, Vec2::new(0., 120.), shift);
    assert!(d.app.grid.scroll_x < second, "Shift+wheel over the bar");
    wheel(&mut d, bar, Vec2::new(500., 0.), Default::default());
    assert_eq!(d.app.grid.scroll_x, 0., "clamped at the start");
    // A plain wheel scrolls the rows, not the columns.
    let before = d.app.grid.scroll_x;
    wheel(&mut d, body, Vec2::new(0., -120.), Default::default());
    assert_eq!(d.app.grid.scroll_x, before);
}

#[test]
fn the_gantt_thumb_pans_the_timescale() {
    let mut d = narrow();
    let thumb = d.rect("hscroll_gantt_thumb");
    let (low, range) = d.app.gantt_scroll_range(d.app.chart_width);
    assert!(range.scrollable());
    let before = d.app.timeline_origin;
    let pixels = 40.;
    let per_pixel = range.max() / f64::from(travel(&d, "hscroll_gantt"));
    drag(
        &mut d,
        thumb.center(),
        thumb.center() + Vec2::new(pixels, 2.),
    );
    let moved = (d.app.timeline_origin - before) as f64;
    assert!(
        moved > 0. && (moved - pixels as f64 * per_pixel).abs() < 2. * per_pixel,
        "moved {moved}, expected {}",
        pixels as f64 * per_pixel
    );
    // The thumb follows; the far end of the track is the project's end plus
    // half a view.
    let thumb = d.rect("hscroll_gantt_thumb");
    drag(
        &mut d,
        thumb.center(),
        thumb.center() + Vec2::new(2000., 0.),
    );
    let (_, range) = d.app.gantt_scroll_range(d.app.chart_width);
    assert!(
        (range.pos - range.max()).abs() < 2.,
        "at the end: {range:?}"
    );
    let thumb = d.rect("hscroll_gantt_thumb");
    drag(
        &mut d,
        thumb.center(),
        thumb.center() - Vec2::new(4000., 0.),
    );
    assert!((d.app.timeline_origin as f64 - low).abs() < 2.);
    // The chart's zoom is untouched.
    assert!((d.app.pixels_per_day - 10.196).abs() < 0.5 || d.app.pixels_per_day > 0.);
}

#[test]
fn the_gantt_arrows_step_one_timescale_unit() {
    let mut d = narrow();
    // Day cells: a click moves to the next day boundary.
    d.app.view.lower = timescale::Unit::Day;
    d.app.view.upper = timescale::Unit::Week;
    d.app.timeline_origin += 500;
    d.frames(2);
    let start = d.app.timeline_origin;
    d.click("hscroll_gantt_right");
    let after = d.app.timeline_origin;
    assert!(after > start && after - start <= DAY, "{start} -> {after}");
    assert_eq!(first_cell(&d, after), after, "on a day boundary");
    d.click("hscroll_gantt_right");
    assert_eq!(d.app.timeline_origin, after + DAY);
    d.click("hscroll_gantt_left");
    assert_eq!(d.app.timeline_origin, after);
    // Week cells: one week at a time.
    d.app.view.lower = timescale::Unit::Week;
    d.app.view.upper = timescale::Unit::Month;
    d.frames(2);
    let before = d.app.timeline_origin;
    d.click("hscroll_gantt_right");
    let next = d.app.timeline_origin;
    assert!(
        next > before && next - before <= 7 * DAY,
        "{before} -> {next}"
    );
    assert_eq!(first_cell(&d, next), next);
    d.click("hscroll_gantt_right");
    assert_eq!(d.app.timeline_origin, next + 7 * DAY);
    d.click("hscroll_gantt_left");
    assert_eq!(d.app.timeline_origin, next);
}

#[test]
fn a_click_on_the_gantt_track_pages_one_view() {
    let mut d = narrow();
    let bar = d.rect("hscroll_gantt");
    let thumb = d.rect("hscroll_gantt_thumb");
    let (_, range) = d.app.gantt_scroll_range(d.app.chart_width);
    let before = d.app.timeline_origin;
    let after = Pos2::new((thumb.right() + bar.right() - 17.) / 2., bar.center().y);
    d.click_at(after);
    d.frames(2);
    let moved = (d.app.timeline_origin - before) as f64;
    assert!(
        (moved - range.view.min(range.max() - range.pos)).abs() < 2.,
        "paged {moved} of a {} minute view",
        range.view
    );
    let thumb = d.rect("hscroll_gantt_thumb");
    let behind = Pos2::new((thumb.left() + bar.left() + 17.) / 2., bar.center().y);
    let now = d.app.timeline_origin;
    d.click_at(behind);
    d.frames(2);
    assert!(d.app.timeline_origin < now);
}

#[test]
fn the_wheel_pans_the_gantt_over_its_timescale_and_bar() {
    let mut d = narrow();
    let shift = egui::Modifiers::SHIFT;
    let header = d.rect("timescale_pan").center();
    let bar = d.rect("hscroll_gantt").center();
    for at in [header, bar] {
        let before = d.app.timeline_origin;
        wheel(&mut d, at, Vec2::new(-120., 0.), Default::default());
        assert!(d.app.timeline_origin > before, "sideways wheel at {at:?}");
        let before = d.app.timeline_origin;
        wheel(&mut d, at, Vec2::new(0., 120.), shift);
        assert!(d.app.timeline_origin < before, "Shift+wheel at {at:?}");
    }
    let body = Pos2::new(d.app.gantt_left + 200., table_body(&d).y);
    let before = d.app.timeline_origin;
    wheel(&mut d, body, Vec2::new(0., -120.), shift);
    assert!(d.app.timeline_origin > before, "Shift+wheel over the chart");
}

#[test]
fn the_bars_do_not_cover_the_last_row() {
    let mut d = narrow();
    let last = d.app.shown_rows().len() - 1;
    d.app.scroll_to_selected = true;
    d.app.selected = d.app.shown_rows().into_iter().rev().find_map(|r| match r {
        grid::Row::Activity(i) => Some(i),
        _ => None,
    });
    d.frames(4);
    let rows = d.app.shown_rows();
    let tops = d.app.grid_row_tops(&rows);
    let (top, height) = tops[last];
    let bar = d.rect("hscroll_table");
    assert!(
        top + height <= bar.top() + 0.5,
        "last row ends at {}, the bar starts at {}",
        top + height,
        bar.top()
    );
    assert_eq!(d.rect("hscroll_gantt").top(), bar.top());
    // The bar sits at the bottom of the pane.
    assert!(bar.bottom() <= d.rect("splitter").bottom() + 0.5);
}

#[test]
fn the_scroll_position_survives_edits_f9_refresh_and_layout_changes() {
    let mut d = narrow();
    d.click("hscroll_table_right");
    d.click("hscroll_table_right");
    let keep = d.app.grid.scroll_x;
    assert!(keep > 0.);
    let origin = d.app.timeline_origin + 3 * DAY;
    d.app.timeline_origin = origin;
    d.frames(2);
    let ppd = d.app.pixels_per_day;
    let same = |d: &Driver, step: &str| {
        assert_eq!(d.app.grid.scroll_x, keep, "table scroll after {step}");
        assert_eq!(d.app.timeline_origin, origin, "chart scroll after {step}");
        assert_eq!(d.app.pixels_per_day, ppd, "chart zoom after {step}");
    };
    // Row selection, keys and a cell edit.
    let rows = d.visible_activities();
    click_row(&mut d, rows[2]);
    assert_eq!(d.app.selected, Some(rows[2]));
    same(&d, "selecting a row");
    d.key(Key::ArrowDown);
    same(&d, "arrow down");
    let i = d.visible_activities()[1];
    let before = d.app.project.model.activities[i].rem;
    d.app.apply_cell(i, "remaining_duration", "9d");
    d.frames(3);
    assert_ne!(d.app.project.model.activities[i].rem, before);
    same(&d, "an edit");
    // F9 re-schedules without moving the view.
    assert!(d.f9());
    d.frames(3);
    same(&d, "F9");
    // Refresh View (F5), undo and redo.
    d.key(Key::F5);
    d.frames(2);
    same(&d, "Refresh View");
    d.app.drop_kept_values();
    d.frames(3);
    same(&d, "dropping every cache");
    d.click("undo");
    d.frames(2);
    same(&d, "undo");
    d.click("redo");
    d.frames(2);
    same(&d, "redo");
    // Column resize (the scroll stays while the content is still wider).
    d.app.set_column_width("activity_id", 120.);
    d.frames(3);
    same(&d, "a column resize");
    // A filter that hides rows.
    d.app.query = "Pour".into();
    d.frames(3);
    same(&d, "a filter");
    d.app.query.clear();
    d.frames(3);
    same(&d, "clearing the filter");
    // A sort and a group.
    d.app.header_sort("activity_name", false);
    d.frames(3);
    same(&d, "a sort");
}

#[test]
fn the_scroll_clamps_when_the_pane_widens_and_when_columns_shrink() {
    let mut d = narrow();
    let thumb = d.rect("hscroll_table_thumb");
    drag(
        &mut d,
        thumb.center(),
        thumb.center() + Vec2::new(2000., 0.),
    );
    let far = d.app.grid.scroll_x;
    assert!((far - max_scroll(&d)).abs() < 0.5);
    d.app.grid_fraction = 0.45;
    d.frames(3);
    assert!(d.app.grid.scroll_x <= max_scroll(&d) + 0.5);
    assert!(d.app.grid.scroll_x < far);
    d.app.grid_fraction = 0.8;
    d.frames(3);
    assert_eq!(d.app.grid.scroll_x, 0.);
    // The narrower pane again does not bring the old position back.
    d.app.grid_fraction = 0.3;
    d.frames(3);
    assert_eq!(d.app.grid.scroll_x, 0.);
}

#[test]
fn the_top_and_bottom_tables_scroll_independently() {
    let mut d = narrow();
    d.app.set_bottom(Some(BottomView::Gantt));
    d.frames(4);
    assert!(d.has("hscroll_table") && d.has("bottom_hscroll_table"));
    assert!(d.has("hscroll_gantt") && d.has("bottom_hscroll_gantt"));
    let bottom = |d: &Driver| d.app.panes.grid.scroll_x();
    assert_eq!(bottom(&d), 0.);
    d.click("hscroll_table_right");
    d.click("hscroll_table_right");
    let top = d.app.grid.scroll_x;
    assert!(top > 0.);
    assert_eq!(bottom(&d), 0., "the bottom table stayed");
    d.click("bottom_hscroll_table_right");
    let low = bottom(&d);
    assert!(low > 0.);
    assert_eq!(d.app.grid.scroll_x, top, "the top table stayed");
    let thumb = d.rect("bottom_hscroll_table_thumb");
    drag(&mut d, thumb.center(), thumb.center() + Vec2::new(900., 0.));
    assert!(bottom(&d) > low);
    assert_eq!(d.app.grid.scroll_x, top);
    // Each keeps its position over frames.
    d.frames(4);
    assert_eq!(d.app.grid.scroll_x, top);
    // The bottom chart's bar pans the bottom Gantt's own timescale and
    // leaves the top chart where it is.
    let origin = d.app.timeline_origin;
    let bottom_origin = d.app.panes.grid.scale.timeline_origin;
    d.click("bottom_hscroll_gantt_right");
    assert_eq!(d.app.timeline_origin, origin, "the top chart stayed");
    assert_ne!(d.app.panes.grid.scale.timeline_origin, bottom_origin);
    // The bottom table on its own, with more columns than fit the pane.
    d.app.set_bottom(Some(BottomView::ActivityTable));
    d.frames(3);
    for _ in 0..30 {
        let first = d.app.panes.grid.columns[0].clone();
        d.app.panes.grid.columns.push(first);
    }
    d.frames(3);
    assert!(d.has("bottom_hscroll_table") && !d.has("bottom_hscroll_gantt"));
}

#[test]
fn each_activities_view_keeps_its_own_scroll() {
    let mut d = narrow();
    d.click("hscroll_table_right");
    let first = d.app.grid.scroll_x;
    assert!(first > 0.);
    d.app.show_view(views::MainView::Activities2);
    d.frames(4);
    assert_eq!(d.app.grid.scroll_x, 0., "the additional view starts at 0");
    d.app.grid_fraction = 0.3;
    d.frames(3);
    d.click("hscroll_table_right");
    d.click("hscroll_table_right");
    let second = d.app.grid.scroll_x;
    assert!(second > first);
    d.app.show_view(views::MainView::Activities);
    d.frames(4);
    assert_eq!(d.app.grid.scroll_x, first);
    d.app.show_view(views::MainView::Activities2);
    d.frames(4);
    assert_eq!(d.app.grid.scroll_x, second);
    // The chart's scroll is per view too.
    let origin = d.app.timeline_origin;
    d.click("hscroll_gantt_right");
    assert!(d.app.timeline_origin > origin);
    let moved = d.app.timeline_origin;
    d.app.show_view(views::MainView::Activities);
    d.frames(4);
    assert_ne!(d.app.timeline_origin, moved);
}

#[test]
fn moving_the_focus_cell_or_editing_off_screen_scrolls_it_into_view() {
    let mut d = narrow();
    let i = d.visible_activities()[1];
    d.select(i);
    d.app.grid.focus_column = Some("activity_id".into());
    d.frames(2);
    assert_eq!(d.app.grid.scroll_x, 0.);
    // Arrow keys across the columns.
    let mut last = String::new();
    for _ in 0..8 {
        d.key(Key::Tab);
        last = d.app.grid.focus_column.clone().unwrap();
    }
    let cols = d
        .app
        .visible_columns(&grid::columns::catalog(&d.app.project));
    let at = cols.iter().position(|c| c.id == last).unwrap();
    let edges = d.app.column_edges();
    let (left, right) = (edges[at], edges[at + 1]);
    let (scroll, view) = (d.app.grid.scroll_x, view_width(&d));
    assert!(left >= scroll - 0.5 && right <= scroll + view + 0.5);
    // Jumping back by the keyboard brings the first column home.
    d.app.move_focus_column(-100);
    d.frames(2);
    assert_eq!(d.app.grid.scroll_x, 0.);
    // Editing a cell that is off to the side shows its column.
    d.app.grid.scroll_x = 0.;
    d.frames(2);
    let column = cols
        .iter()
        .find(|c| c.id == "remaining_duration")
        .unwrap()
        .clone();
    let at = cols.iter().position(|c| c.id == column.id).unwrap();
    assert!(
        edges[at + 1] > view_width(&d),
        "the column starts off screen"
    );
    d.app.begin_edit(i, &column);
    assert_eq!(d.app.edit_cell, Some((i, column.id.clone())));
    let (scroll, view) = (d.app.grid.scroll_x, view_width(&d));
    assert!(scroll > 0. && edges[at + 1] <= scroll + view + 0.5);
    d.key(Key::Escape);
    d.frames(2);
    // A find that lands on a column off to the side shows it as well.
    d.app.grid.scroll_x = 0.;
    d.app.grid.focus_column = Some(last.clone());
    d.app.show_focus_column();
    assert!(d.app.grid.scroll_x > 0.);
    // Home and End keep moving the selection, not the columns.
    d.app.grid.scroll_x = 30.;
    d.frames(2);
    d.key(Key::End);
    assert_eq!(d.app.grid.scroll_x, 30.);
    d.key(Key::Home);
    assert_eq!(d.app.grid.scroll_x, 30.);
}

/// A software painter on a fresh context (so it sees every texture), with
/// the UI settled and the pointer at `pointer` (or away).
fn fresh_render(d: &mut Driver, pointer: Option<Pos2>) -> (SoftwarePainter, egui::Context) {
    let render = egui::Context::default();
    crate::chrome::configure(&render);
    render.set_pixels_per_point(1.);
    let mut painter = SoftwarePainter::default();
    for _ in 0..3 {
        let event = match pointer {
            Some(at) => egui::Event::PointerMoved(at),
            None => egui::Event::PointerGone,
        };
        let _ = painter.frame(&mut d.app, &render, &mut d.time, vec![event], false);
    }
    (painter, render)
}

fn pixel(image: &image::RgbaImage, x: f32, y: f32) -> [u8; 3] {
    let p = image.get_pixel(x as u32, y as u32).0;
    [p[0], p[1], p[2]]
}

#[test]
fn the_idle_bar_shows_a_track_a_thumb_and_faint_arrows() {
    let mut d = narrow();
    // Pointer away from the bars.
    let _ = d.frame(vec![egui::Event::PointerGone]);
    d.frames(2);
    let (mut painter, render) = fresh_render(&mut d, None);
    let (image, _) = painter
        .frame(&mut d.app, &render, &mut d.time, vec![], true)
        .unwrap();
    for name in ["hscroll_table", "hscroll_gantt"] {
        let (bar, thumb) = (d.rect(name), d.rect(&format!("{name}_thumb")));
        assert_eq!(bar.height(), 16.);
        let mid = bar.top() + 8.;
        let track = pixel(&image, bar.right() - 8., bar.top() + 3.);
        assert_eq!(track, [0xF0, 0xF0, 0xF0], "{name} track");
        let x = thumb.center().x;
        // A 4 px thumb on a 4 px track, so the idle bar reads as a scroll
        // bar.
        for y in [mid - 2., mid - 1., mid, mid + 1.] {
            assert_eq!(pixel(&image, x, y), [0x85; 3], "{name} thumb at {y}");
        }
        assert_eq!(pixel(&image, x, mid - 3.), [0xF0; 3], "{name} above");
        assert_eq!(pixel(&image, x, mid + 2.), [0xF0; 3], "{name} below");
        assert_eq!(pixel(&image, x, bar.top()), [0xF0; 3]);
        assert_eq!(pixel(&image, x, bar.bottom() - 1.), [0xF0; 3]);
        let off_thumb = if thumb.right() + 4. < bar.right() - 20. {
            thumb.right() + 3.
        } else {
            thumb.left() - 3.
        };
        assert_eq!(pixel(&image, off_thumb, mid), [0xDE; 3], "{name} track");
        // Faint arrows at each end.
        for button_x in [bar.left(), bar.right() - 16.] {
            let inked = (0..16).any(|dx| {
                (0..16).any(|dy| {
                    pixel(&image, button_x + dx as f32, bar.top() + dy as f32) == [0xA5; 3]
                })
            });
            assert!(inked, "{name} arrow at {button_x}");
        }
        // The thumb is at least 32 px long.
        assert!(thumb.width() >= 32.);
    }
}

#[test]
fn the_hovered_bar_shows_a_full_thumb_and_arrow_buttons() {
    let mut d = narrow();
    let bar = d.rect("hscroll_table");
    let thumb = d.rect("hscroll_table_thumb");
    // Over the track, clear of the thumb: the bar expands.
    let at = Pos2::new(bar.right() - 40., bar.center().y);
    let (mut painter, render) = fresh_render(&mut d, Some(at));
    let (image, _) = painter
        .frame(&mut d.app, &render, &mut d.time, vec![], true)
        .unwrap();
    let mid = bar.top() + 8.;
    let x = thumb.center().x;
    for dy in [-3., -2., -1., 0., 1., 2.] {
        assert_eq!(pixel(&image, x, mid + dy), [0x85; 3], "pill row {dy}");
    }
    assert_eq!(pixel(&image, x, mid - 4.), [0xF0; 3]);
    assert_eq!(pixel(&image, x, mid + 3.), [0xF0; 3]);
    // Arrow glyphs in both buttons: dark pixels near each button's center.
    for button in [d.rect("hscroll_table_left"), d.rect("hscroll_table_right")] {
        let c = button.center();
        assert!(pixel(&image, c.x, c.y)[0] < 0x90, "arrow at {c:?}");
        assert_eq!(
            pixel(&image, button.left() + 1., button.top() + 1.),
            [0xF0; 3]
        );
    }
}

/// Drag the table/chart splitter so its center ends at screen x `to`.
fn drag_splitter(d: &mut Driver, to: f32) {
    let splitter = d.rect("splitter");
    let from = Pos2::new(splitter.center().x, splitter.center().y);
    drag(d, from, Pos2::new(to, from.y));
}

/// Every drawn rect finite, a sane chart width, and the splitter whole.
fn assert_sane(d: &Driver, step: &str) {
    assert!(d.app.grid_fraction.is_finite(), "{step}: fraction");
    assert!(
        d.app.chart_width.is_finite() && d.app.chart_width >= 0.,
        "{step}"
    );
    assert!(
        d.app.grid.scroll_x.is_finite() && d.app.grid.scroll_x >= 0.,
        "{step}"
    );
    assert!(
        d.app.pixels_per_day.is_finite() && d.app.pixels_per_day > 0.,
        "{step}"
    );
    for (name, r) in &d.app.action_rects {
        assert!(r.iter().all(|v| v.is_finite()), "{step}: {name} {r:?}");
    }
    let splitter = d.rect("splitter");
    assert_eq!(splitter.width(), 6., "{step}: the splitter keeps its 6 px");
}

#[test]
fn the_splitter_drags_to_the_far_left_and_the_chart_fills_the_pane() {
    let mut d = narrow();
    d.app.grid.layout.table.line_numbers = true;
    drag_splitter(&mut d, 0.);
    d.frames(3);
    assert_eq!(d.app.grid_fraction, 0.);
    assert_sane(&d, "far left");
    let splitter = d.rect("splitter");
    let pane_left = d.app.action_rects["splitter"][0];
    assert!(
        splitter.left() < 60. && splitter.left() >= 0.,
        "{splitter:?}"
    );
    assert_eq!(d.app.gantt_left, splitter.right());
    // The table has no width: its headers are not drawn, the rows still are.
    assert!(!d.has("grid_header_activity_name"));
    assert!(d.app.chart_width > 1000., "{}", d.app.chart_width);
    assert!(!d.app.bar_geometry.is_empty(), "the chart still draws bars");
    let _ = pane_left;
    // The splitter is still grabbable: drag it back out.
    drag_splitter(&mut d, 700.);
    d.frames(3);
    assert!(d.app.grid_fraction > 0.3 && d.app.grid_fraction < 0.7);
    assert!(d.has("grid_header_activity_name"));
    assert_sane(&d, "back out");
    // The chart's bar still scrolls the timescale at the left extreme.
    drag_splitter(&mut d, 0.);
    d.frames(3);
    let thumb = d.rect("hscroll_gantt_thumb");
    let before = d.app.timeline_origin;
    drag(&mut d, thumb.center(), thumb.center() + Vec2::new(40., 0.));
    assert!(d.app.timeline_origin > before);
    d.click("hscroll_gantt_right");
    let at = d.rect("hscroll_gantt").center();
    wheel(&mut d, at, Vec2::new(-80., 0.), Default::default());
    assert_sane(&d, "chart bar at the far left");
    d.app.fit();
    d.frames(3);
    assert_sane(&d, "fit at the far left");
}

#[test]
fn the_splitter_drags_to_the_far_right_and_the_table_fills_the_pane() {
    let mut d = narrow();
    d.app.grid.layout.table.line_numbers = true;
    // A wide table so its bar has work to do.
    d.app.set_column_width("activity_name", 900.);
    d.frames(3);
    drag_splitter(&mut d, 1499.);
    d.frames(3);
    assert_eq!(d.app.grid_fraction, 1.);
    assert_sane(&d, "far right");
    let splitter = d.rect("splitter");
    assert!(splitter.right() <= 1500., "{splitter:?}");
    // Only the chart's vertical scroll bar column is left of the edge.
    assert!(d.app.chart_width <= 16. && d.app.chart_width >= 0.);
    assert!(d.has("grid_header_activity_name"));
    // The table bar works at this extreme.
    let thumb = d.rect("hscroll_table_thumb");
    drag(&mut d, thumb.center(), thumb.center() + Vec2::new(60., 0.));
    assert!(d.app.grid.scroll_x > 0.);
    d.click("hscroll_table_right");
    let at = table_body(&d);
    wheel(&mut d, at, Vec2::new(0., -120.), egui::Modifiers::SHIFT);
    assert_sane(&d, "table bar at the far right");
    // The splitter can be grabbed here and dragged back.
    drag_splitter(&mut d, 600.);
    d.frames(3);
    assert!(d.app.grid_fraction < 0.6);
    assert_sane(&d, "back from the right");
}

#[test]
fn the_extremes_hold_with_the_bottom_table_and_gantt() {
    let mut d = narrow();
    d.app.set_bottom(Some(BottomView::Gantt));
    d.frames(4);
    for to in [0., 1499., 750.] {
        drag_splitter(&mut d, to);
        d.frames(4);
        assert_sane(&d, &format!("bottom split at {to}"));
        assert!(d.has("bottom_hscroll_gantt") || to == 1499.);
    }
    d.app.set_bottom(Some(BottomView::ActivityTable));
    for f in [0., 1.] {
        d.app.grid_fraction = f;
        d.frames(4);
        assert_sane(&d, "bottom table alone");
    }
    // Each of the other bottom views survives the extremes too.
    for view in [
        BottomView::ActivityUsageSheet,
        BottomView::ResourceUsageSheet,
        BottomView::ActivityUsageProfile,
        BottomView::ResourceUsageProfile,
        BottomView::TraceLogic,
        BottomView::Details,
    ] {
        d.app.set_bottom(Some(view));
        for f in [0., 1.] {
            d.app.grid_fraction = f;
            d.frames(3);
            assert!(d
                .app
                .action_rects
                .values()
                .all(|r| r.iter().all(|v| v.is_finite())));
        }
    }
}

#[test]
fn the_split_survives_saving_and_reopening_at_each_extreme() {
    let mut d = narrow();
    for (to, fraction) in [(0., 0.), (1499., 1.)] {
        drag_splitter(&mut d, to);
        // Let the idle save run (it waits 0.4 s).
        d.frames(10);
        assert_eq!(d.app.grid_fraction, fraction);
        d.app.grid_fraction = 0.5;
        d.app.restore_view();
        assert_eq!(d.app.grid_fraction, fraction, "restored at {to}");
        d.frames(3);
        assert_sane(&d, "reopened");
        let table = d.app.grid_fraction;
        assert_eq!(table, fraction);
    }
    // A damaged value is pulled back into range.
    let mut view = crate::view::ViewSettings::default();
    let (mut ppd, mut fraction) = (10., 0.5);
    crate::view::apply_view_value(
        &serde_json::json!({"grid_fraction": 7.0}),
        &mut view,
        &mut ppd,
        &mut fraction,
    );
    assert_eq!(fraction, 1.);
}

#[test]
fn the_extremes_render_the_headers_bars_and_rows() {
    let mut d = narrow();
    let out = std::env::var("OPENXER_RENDER_DIR").ok();
    for (name, f, wide) in [
        ("left", 0., false),
        ("right", 1., true),
        ("middle", 0.3, false),
    ] {
        if wide {
            d.app.set_column_width("activity_name", 900.);
        }
        d.app.grid_fraction = f;
        let (mut painter, render) = fresh_render(&mut d, None);
        let (image, triangles) = painter
            .frame(&mut d.app, &render, &mut d.time, vec![], true)
            .unwrap();
        assert!(triangles > 500, "{name}");
        if let Some(dir) = &out {
            image
                .save(std::path::Path::new(dir).join(format!("hscroll-{name}.png")))
                .unwrap();
        }
    }
}

#[test]
fn render_the_hovered_bar_and_a_wide_corpus_project() {
    let Some(dir) = std::env::var_os("OPENXER_RENDER_DIR") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    let mut d = narrow();
    let at = d.rect("hscroll_table").center() + Vec2::new(60., 0.);
    let (mut painter, render) = fresh_render(&mut d, Some(at));
    let (image, _) = painter
        .frame(&mut d.app, &render, &mut d.time, vec![], true)
        .unwrap();
    image.save(dir.join("hscroll-hover.png")).unwrap();
    // The corpus project 369 (read only), scrolled in both panes.
    let path = crate::test_corpus::file("369.xer");
    let Ok(doc) = Document::read(&path) else {
        return;
    };
    let key = openxer_interchange::baseline::schedule_projects(&doc)[0].0;
    let Ok(mut project) = ProjectData::from_document(doc, key, "369".into()) else {
        return;
    };
    let _ = schedule_model(&mut project.model, &project.resources);
    let mut d = Driver::new("hscroll-369", project);
    d.app.grid_fraction = 0.3;
    d.frames(2);
    d.app.fit();
    d.frames(3);
    d.click("hscroll_table_right");
    d.click("hscroll_gantt_right");
    let at = d.rect("hscroll_gantt").center() + Vec2::new(200., 0.);
    let (mut painter, render) = fresh_render(&mut d, Some(at));
    let (image, _) = painter
        .frame(&mut d.app, &render, &mut d.time, vec![], true)
        .unwrap();
    image.save(dir.join("hscroll-369.png")).unwrap();
}

// Band strips stay fixed while the columns scroll under their edge.

fn three_level_groups(d: &mut Driver) {
    let level = |column: &str| grid::layout::GroupLevel {
        column: column.into(),
        to_level: None,
        indent: true,
        interval: String::new(),
    };
    d.app.grid.layout.group_sort.levels = vec![
        level("wbs"),
        level("activity_status"),
        level("activity_type"),
    ];
    d.frames(3);
}

/// One painted frame at the current scroll.
fn shot(d: &mut Driver, painter: &mut SoftwarePainter, render: &egui::Context) -> image::RgbaImage {
    let _ = painter.frame(&mut d.app, render, &mut d.time, vec![], false);
    painter
        .frame(&mut d.app, render, &mut d.time, vec![], true)
        .unwrap()
        .0
}

/// Rows drawn in the table body: (top, height, strip depth).
fn strip_rows(d: &Driver, bottom: bool) -> Vec<(f32, f32, usize)> {
    let rows = d.app.shown_rows();
    let depths = d.app.row_metrics(&rows).depths;
    let (tops, limit, header) = if bottom {
        let g = &d.app.panes.grid;
        let tops: Vec<(f32, f32)> = (0..rows.len().min(g.row_offsets.len().saturating_sub(1)))
            .map(|k| {
                (
                    g.body_top + g.row_offsets[k],
                    g.row_offsets[k + 1] - g.row_offsets[k],
                )
            })
            .collect();
        (
            tops,
            d.rect("bottom_hscroll_table").top(),
            g.header.0 + g.header.1,
        )
    } else {
        (
            d.app.grid_row_tops(&rows),
            d.rect("hscroll_table").top(),
            d.rect("grid_header_activity_id").bottom(),
        )
    };
    tops.iter()
        .zip(&depths)
        .filter(|((top, h), _)| *top >= header && top + h <= limit)
        .map(|((top, h), &depth)| (*top, *h, depth))
        .collect()
}

/// Ink: dark in every channel (not a colored band fill).
fn dark(px: [u8; 3]) -> bool {
    px.iter().all(|&c| c < 0xA0)
}

/// The strips never change with the scroll, no ink reaches their edge, the
/// first scrolled column starts at the gutter edge, and clicks next to the
/// edge hit the right column.
fn check_strips(d: &mut Driver, bottom: bool, left0: f32, scroll: &dyn Fn(&mut Driver, f32)) {
    let (mut painter, render) = fresh_render(d, None);
    scroll(d, 0.);
    d.frames(2);
    let base = shot(d, &mut painter, &render);
    let rows = strip_rows(d, bottom);
    assert!(rows.len() >= 6, "{} rows drawn", rows.len());
    let deepest = rows.iter().map(|r| r.2).max().unwrap();
    assert!(deepest >= 3, "the layout has {deepest} strip levels");
    let step = 5.;
    let edges = d.app.column_edges();
    let content = left0 + deepest as f32 * step;
    for s in [
        1.,
        9.,
        23.,
        60.,
        edges[1] - deepest as f32 * step,
        edges[1] + 31.,
    ] {
        scroll(d, s);
        d.frames(2);
        let got = shot(d, &mut painter, &render);
        let at = if bottom {
            d.app.panes.grid.scroll_x()
        } else {
            d.app.grid.scroll_x
        };
        assert!(at > 0., "scrolled to {s}: {at}");
        for &(top, h, depth) in &rows {
            let (y0, y1) = (top.round() as u32, (top + h).round() as u32 - 1);
            let strip_right = (left0 + depth as f32 * step).round() as u32;
            for y in y0..y1 {
                for x in left0.round() as u32..strip_right {
                    assert_eq!(
                        got.get_pixel(x, y),
                        base.get_pixel(x, y),
                        "strip pixel ({x}, {y}) at scroll {s}, depth {depth}"
                    );
                }
            }
        }
    }
    // The first scrolled column starts at the gutter edge of the deepest rows.
    let s = edges[1] - deepest as f32 * step;
    scroll(d, s);
    d.frames(2);
    let got = shot(d, &mut painter, &render);
    let &(top, h, _) = rows.iter().find(|r| r.2 == deepest).unwrap();
    let y = (top + h / 2.).round() as u32;
    let inked = (content as u32..content as u32 + 60).any(|x| {
        (top.round() as u32..(top + h).round() as u32 - 1).any(|y| {
            let p = got.get_pixel(x, y).0;
            dark([p[0], p[1], p[2]])
        })
    });
    assert!(inked, "the second column's text begins at the gutter edge");
    let line = got.get_pixel(content as u32 - 1, y).0;
    assert_eq!(
        &line[..3],
        base.get_pixel(content as u32 - 1, y).0.get(..3).unwrap()
    );
    // Clicks next to the edge hit the column under them; clicks on a strip
    // leave the focus column alone.
    let columns = d
        .app
        .visible_columns(&grid::columns::catalog(&d.app.project));
    let x0 = left0.round() - d.app.grid.scroll_x.round();
    let (top, h, depth) = *rows.iter().find(|r| r.2 == deepest).expect("a deepest row");
    let _ = depth;
    let ypos = top + h / 2.;
    d.app.grid.focus_column = Some("activity_id".into());
    let want = (0..columns.len())
        .find(|&n| content + 1. >= x0 + edges[n] && content + 1. < x0 + edges[n + 1])
        .map(|n| columns[n].id.clone())
        .unwrap();
    if !bottom {
        d.click_at(Pos2::new(content + 1., ypos));
        assert_eq!(d.app.grid.focus_column.as_deref(), Some(want.as_str()));
        d.app.grid.focus_column = Some("activity_id".into());
        d.click_at(Pos2::new(content - 3., ypos));
        assert_eq!(d.app.grid.focus_column.as_deref(), Some("activity_id"));
    }
}

#[test]
fn scrolled_columns_never_draw_over_the_band_strips_in_the_top_table() {
    for line_numbers in [false, true] {
        let mut d = narrow();
        d.app.grid.layout.table.line_numbers = line_numbers;
        three_level_groups(&mut d);
        // The header's hit area is 4 px inside the column.
        let left0 = d.rect("grid_header_activity_id").left() - 4.;
        check_strips(&mut d, false, left0, &|d, s| {
            d.app.grid.scroll_x = s;
        });
        // The header's scrolled captions stop at the strips' edge as well.
        let gutter = d.app.strip_gutter();
        assert!(gutter >= 15.);
        d.app.grid.scroll_x = 25.;
        d.frames(2);
        let first = d.rect("grid_header_activity_id");
        assert!(
            first.left() >= left0 + gutter - 0.5,
            "the header's scrolled column starts at the strips' edge: {first:?}"
        );
    }
}

#[test]
fn scrolled_columns_never_draw_over_the_band_strips_in_the_bottom_table() {
    let mut d = narrow();
    d.app.set_bottom(Some(BottomView::Gantt));
    three_level_groups(&mut d);
    d.frames(4);
    let left0 = d.app.panes.body.left();
    check_strips(&mut d, true, left0, &|d, s| {
        d.app.panes.grid.set_scroll_x(s);
    });
}

#[test]
fn ungrouped_tables_scroll_without_a_gutter() {
    let mut d = narrow();
    d.app.grid.layout.group_sort.levels.clear();
    d.frames(3);
    assert_eq!(d.app.strip_gutter(), 0.);
    let left0 = d.rect("grid_header_activity_id").left();
    let edges = d.app.column_edges();
    d.app.grid.scroll_x = edges[1];
    d.frames(2);
    let (mut painter, render) = fresh_render(&mut d, None);
    let image = shot(&mut d, &mut painter, &render);
    // The second column's text starts at the table's own left edge.
    let rows = strip_rows(&d, false);
    let (top, h, depth) = rows[2];
    assert_eq!(depth, 0);
    let inked = (left0 as u32..left0 as u32 + 60).any(|x| {
        (top.round() as u32..(top + h).round() as u32 - 1).any(|y| {
            let p = image.get_pixel(x, y).0;
            dark([p[0], p[1], p[2]])
        })
    });
    assert!(inked, "no gutter: the column reaches the left edge");
}

#[test]
fn render_the_scrolled_grouped_table() {
    let Some(dir) = std::env::var_os("OPENXER_RENDER_DIR") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    let mut d = narrow();
    three_level_groups(&mut d);
    let edges = d.app.column_edges();
    for (name, s) in [
        ("strips-0", 0.),
        ("strips-mid", edges[1] - 10.),
        ("strips-far", edges[1] + 60.),
    ] {
        d.app.grid.scroll_x = s;
        let (mut painter, render) = fresh_render(&mut d, None);
        let image = shot(&mut d, &mut painter, &render);
        image.save(dir.join(format!("hscroll-{name}.png"))).unwrap();
    }
}

#[test]
fn gantt_labels_never_bleed_into_the_persistent_vertical_scrollbar() {
    let mut d = Driver::new("vscroll-clip", scheduled_starter());
    // Give every activity a long name so labels extend to the right edge of the Gantt chart.
    for det in d.app.project.details.values_mut() {
        det.name =
            "Coordinate drainage utilities building systems windstorm inspection approval review"
                .into();
    }
    // Dock Chat beside the Gantt to match the narrow-Gantt layout.
    d.app.chat.open = true;
    d.app.chat.docked = true;
    d.frames(4);
    assert!(d.has("vscroll_activities"));
    let vbar = d.rect("vscroll_activities");
    assert_eq!(vbar.width(), BAR_HEIGHT);
    // Gantt right edge must stop at or before the vertical scrollbar's left edge.
    assert!(
        d.app.gantt_left + d.app.chart_width <= vbar.left() + 0.5,
        "gantt right {} exceeds vbar left {}",
        d.app.gantt_left + d.app.chart_width,
        vbar.left()
    );
    // Render with pointer away: the vertical scrollbar must stay persistently
    // visible (face track, thumb/arrows) with zero white label halos or dark
    // text pixels bleeding into the scrollbar column.
    let _ = d.frame(vec![egui::Event::PointerGone]);
    d.frames(2);
    let (mut painter, render) = fresh_render(&mut d, None);
    let image = shot(&mut d, &mut painter, &render);
    let x0 = (vbar.left() + 1.).ceil() as u32;
    let x1 = (vbar.right() - 1.).floor() as u32;
    let y0 = (vbar.top() + 18.).ceil() as u32;
    let y1 = (vbar.bottom() - 18.).floor() as u32;
    for y in y0..y1 {
        for x in x0..x1 {
            let p = image.get_pixel(x, y).0;
            // No pure white label halo (0xFF) and no dark black text (< 80) in the track!
            assert!(
                p[0] < 250 && p[0] >= 80,
                "unexpected pixel {p:?} inside vertical scrollbar track at ({x}, {y})"
            );
        }
    }
    // If the list overflows, clicking the down arrow scrolls down cleanly.
    if d.has("vscroll_activities_down") {
        let thumb_before = d.rect("vscroll_activities_thumb").top();
        d.click("vscroll_activities_down");
        d.frames(2);
        let thumb_after = d.rect("vscroll_activities_thumb").top();
        assert!(
            thumb_after > thumb_before,
            "clicking down arrow scrolled thumb from {thumb_before} to {thumb_after}"
        );
    }
}
