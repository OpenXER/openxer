//! The timeline control every chart with a timescale shares: the Gantt
//! Chart of the Activities view (top and bottom pane), the usage profiles
//! and spreadsheets under it, and the charts and spreadsheets of the WBS,
//! Projects and Resource Assignments views.
//!
//! A timeline is a first visible minute and a scale in pixels a week
//! (`Span`). What the user can do with it is the same everywhere, measured on
//! the Activities Gantt:
//!
//! - a 16 px horizontal scroll bar under the chart (`hscroll`): the thumb
//!   moves over the data with half a view to spare at each end, the arrows
//!   step one cell of the lowest timescale band, a click on the track pages
//!   one view, and a view already outside that range may only move back in;
//! - a drag on the upper timescale band pans, a drag on the lowest band
//!   zooms (by e to the power of a hundredth of the drag) about the chart's
//!   left edge;
//! - a sideways wheel, or the wheel with Shift, over the chart, its
//!   timescale or its bar pans; Ctrl with the wheel over the chart body
//!   zooms around the pointer;
//! - Zoom In and Zoom Out step along `timescale::ZOOM_LADDER` around the
//!   chart's centre, and Zoom To Best Fit shows the whole data range.
//!
//! The functions here hold that behavior once. Each chart keeps its own
//! state and calls `control` (or, for the Activities Gantt, the pieces it is
//! made of) and `paint_bar`.
use super::*;
use crate::hscroll::{self, Geometry, Move, Range};
use crate::timescale::{Cell, LabelOptions, TimeScale, Unit};

const MINUTES_PER_WEEK: f64 = 7. * DAY as f64;
/// Zoom per pixel of a drag on the lowest timescale band: the scale is
/// multiplied by e to the power of a hundredth of the drag.
pub(crate) const DRAG_ZOOM: f64 = 0.01;

/// What a timeline shows: its first visible minute (from the model origin)
/// and its scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Span {
    pub first: f64,
    pub pixels_per_week: f64,
}

impl Span {
    pub fn new(first: f64, pixels_per_week: f64) -> Self {
        Self {
            first,
            pixels_per_week,
        }
    }

    pub fn axis(&self, left: f32) -> TimeScale {
        TimeScale::new(self.first, self.pixels_per_week, left)
    }

    /// Minutes shown across `width` pixels.
    pub fn view_minutes(&self, width: f32) -> f64 {
        f64::from(width.max(0.)) / self.pixels_per_week.max(1e-3) * MINUTES_PER_WEEK
    }

    /// The same span zoomed to `scale` (clamped to `limits`), keeping the
    /// date under `anchor` in place on an axis that starts at `left`.
    pub fn zoomed(&self, scale: f64, anchor: f32, left: f32, limits: (f64, f64)) -> Self {
        let mut axis = self.axis(left);
        axis.zoom_at(scale.clamp(limits.0, limits.1.max(limits.0)), anchor);
        Self::new(axis.first_minute, axis.pixels_per_week)
    }

    /// The same span moved by a drag of `dx` pixels (positive reveals
    /// earlier dates).
    pub fn panned(&self, dx: f32) -> Self {
        let mut axis = self.axis(0.);
        axis.pan_pixels(dx);
        Self::new(axis.first_minute, self.pixels_per_week)
    }
}

/// What a chart's timeline covers and how far it zooms.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Spec {
    /// First and last minute of the chart's data (bars, values).
    pub extent: (i64, i64),
    /// Smallest and largest scale in pixels a week.
    pub limits: (f64, f64),
    /// The model's origin, for timescale cells.
    pub origin_serial: i64,
    /// The lowest band's unit: what an arrow click steps.
    pub unit: Unit,
    pub options: LabelOptions,
}

/// Where a chart was drawn last, for the commands that act on it from a
/// menu or toolbar: Zoom In and Out keep its centre, Fit fills its width.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Place {
    pub left: f32,
    pub width: f32,
    pub extent: Option<(i64, i64)>,
}

/// What the scroll bar of a chart `width` pixels wide stands for, in
/// minutes: the data with half a view beyond each end, so there is always
/// room to move the data across the chart. Returns the low end too.
pub(crate) fn scroll_range(first: f64, view: f64, extent: (i64, i64)) -> (f64, Range) {
    let low = extent.0 as f64 - view / 2.;
    let high = extent.1.max(extent.0) as f64 + view / 2.;
    (
        low,
        Range {
            total: high - low,
            view,
            pos: first - low,
        },
    )
}

/// The first visible minute after a request from the scroll bar. A view
/// already outside the range may only move back in.
pub(crate) fn moved(
    first: f64,
    low: f64,
    range: &Range,
    request: Move,
    line: impl FnOnce(i32) -> f64,
) -> f64 {
    let next = match request {
        Move::To(pos) => low + pos,
        Move::Page(dir) => first + f64::from(dir) * range.view,
        Move::Line(dir) => line(dir),
    };
    let lowest = low.min(first);
    let highest = (low + range.total - range.view).max(first);
    next.clamp(lowest, highest)
}

/// The start of the `unit` cell after `first` (`dir` 1) or of the one
/// before it (-1): what a scroll bar arrow moves to.
pub(crate) fn unit_step(
    first: f64,
    dir: i32,
    unit: Unit,
    origin_serial: i64,
    options: &LabelOptions,
) -> f64 {
    let from = if dir > 0 { first } else { first - 1. };
    let cells = timescale::band_cells(unit, false, origin_serial, from, from + 1., options);
    match (cells.first(), dir > 0) {
        (Some(cell), true) => cell.end_minute as f64,
        (Some(cell), false) => cell.start_minute as f64,
        (None, _) => first + f64::from(dir) * DAY as f64,
    }
}

/// One rung along the zoom ladder around the centre of a chart at `place`.
pub(crate) fn ladder(span: Span, zoom_in: bool, place: &Place, limits: (f64, f64)) -> Span {
    let scale = timescale::zoom_step(span.pixels_per_week, zoom_in);
    span.zoomed(scale, place.left + place.width / 2., place.left, limits)
}

/// The whole of `extent` in a chart `width` pixels wide, two days to spare
/// before it and about a week after, as Zoom To Fit does on the Gantt.
pub(crate) fn fit(extent: (i64, i64), width: f32, limits: (f64, f64)) -> Span {
    let (start, finish) = (extent.0, extent.1.max(extent.0));
    let first = (start.div_euclid(DAY) * DAY - 2 * DAY) as f64;
    let days = ((finish - start) / DAY + 8).max(8) as f64;
    let per_day = (f64::from(width.max(1.)) / days).clamp(1. / 7., 40.);
    Span::new(
        first,
        (per_day * 7.).clamp(limits.0, limits.1.max(limits.0)),
    )
}

/// The header bands of a chart: dragging `pan` pans, dragging `zoom` zooms
/// about the chart's left edge. Returns the two responses.
pub(crate) fn header(
    ui: &egui::Ui,
    id: egui::Id,
    pan: Rect,
    zoom: Rect,
) -> (egui::Response, egui::Response) {
    let pan = ui
        .interact(pan, id.with("timescale_pan"), Sense::click_and_drag())
        .on_hover_cursor(egui::CursorIcon::Grab);
    let zoom = ui
        .interact(zoom, id.with("timescale_zoom"), Sense::click_and_drag())
        .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
    (pan, zoom)
}

/// The span after this frame's drags on the header bands.
pub(crate) fn header_drags(
    ctx: &egui::Context,
    span: Span,
    pan: &egui::Response,
    zoom: &egui::Response,
    left: f32,
    limits: (f64, f64),
) -> Span {
    let mut span = span;
    if pan.dragged() {
        span = span.panned(drag_motion(pan, ctx).x);
    }
    if zoom.dragged() {
        let factor = (f64::from(drag_motion(zoom, ctx).x) * DRAG_ZOOM).exp();
        span = span.zoomed(span.pixels_per_week * factor, left, left, limits);
    }
    span
}

/// A sideways wheel, or Shift with the wheel (which egui reports as
/// sideways), over any of `over`: how many pixels to pan by.
pub(crate) fn wheel_pan(ctx: &egui::Context, over: &[Rect]) -> Option<f32> {
    let pos = ctx.input(|i| i.pointer.hover_pos())?;
    if !over.iter().any(|r| r.contains(pos)) || ctx.input(|i| i.zoom_delta()) != 1. {
        return None;
    }
    let dx = ctx.input(|i| i.smooth_scroll_delta.x);
    (dx != 0.).then_some(dx)
}

/// Ctrl with the wheel over `body`: the zoom factor and the pointer's x.
pub(crate) fn wheel_zoom(ctx: &egui::Context, body: Rect) -> Option<(f32, f32)> {
    let pos = ctx.input(|i| i.pointer.hover_pos())?;
    if !body.contains(pos) || crate::print::viewer::wheel_taken(ctx) {
        return None;
    }
    let factor = ctx.input(|i| i.zoom_delta());
    (factor != 1.).then_some((factor, pos.x))
}

/// The parts of one chart, in screen coordinates.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Chart {
    /// The timescale band(s) a drag pans.
    pub pan: Rect,
    /// The lowest timescale band, which a drag zooms.
    pub zoom: Rect,
    /// The chart body: Ctrl+wheel zooms here.
    pub body: Rect,
    /// The scroll bar's strip, when the chart has one.
    pub bar: Option<Rect>,
    /// Where the time axis starts (x of `span.first`).
    pub left: f32,
    /// Width of the time axis in view.
    pub width: f32,
}

/// What `control` did this frame.
pub(crate) struct Outcome {
    pub span: Span,
    /// The pointer pressed or wheeled on the chart: zoom commands from the
    /// menu and toolbar now act on it.
    pub touched: bool,
    /// The header bands' responses, for actions and menus.
    pub pan: egui::Response,
    pub zoom: egui::Response,
}

/// Handles every pointer gesture of one chart: its scroll bar, header drags
/// and the wheel. Done before the chart is drawn, so a change shows in the
/// same frame. Draw the bar later with `paint_bar`.
pub(crate) fn control(
    ui: &mut egui::Ui,
    id: egui::Id,
    chart: &Chart,
    span: Span,
    spec: &Spec,
) -> Outcome {
    let ctx = ui.ctx().clone();
    let mut span = span;
    if let Some(bar) = chart.bar {
        let (low, range) = scroll_range(span.first, span.view_minutes(chart.width), spec.extent);
        if let Some(request) = hscroll::interact(ui, id.with("bar"), bar, range) {
            span.first = moved(span.first, low, &range, request, |dir| {
                unit_step(
                    span.first,
                    dir,
                    spec.unit,
                    spec.origin_serial,
                    &spec.options,
                )
            });
        }
    }
    let (pan, zoom) = header(ui, id, chart.pan, chart.zoom);
    span = header_drags(&ctx, span, &pan, &zoom, chart.left, spec.limits);
    let mut over = vec![chart.pan, chart.zoom];
    over.extend(chart.bar);
    over.push(chart.body);
    let mut wheeled = false;
    if let Some((factor, x)) = wheel_zoom(&ctx, chart.body) {
        span = span.zoomed(
            span.pixels_per_week * f64::from(factor),
            x,
            chart.left,
            spec.limits,
        );
        wheeled = true;
    } else if let Some(dx) = wheel_pan(&ctx, &over) {
        span = span.panned(dx);
        wheeled = true;
    }
    let pressed = ctx.input(|i| {
        i.pointer.any_pressed()
            && i.pointer
                .interact_pos()
                .is_some_and(|p| over.iter().any(|r| r.contains(p)))
    });
    Outcome {
        span,
        touched: wheeled || pressed,
        pan,
        zoom,
    }
}

/// Draws a chart's scroll bar (after whatever else is drawn in its strip)
/// and returns its geometry with the range it stands for.
pub(crate) fn paint_bar(
    ui: &egui::Ui,
    id: egui::Id,
    bar: Rect,
    span: Span,
    width: f32,
    extent: (i64, i64),
) -> (Geometry, Range) {
    let (_, range) = scroll_range(span.first, span.view_minutes(width), extent);
    (hscroll::paint(ui, id.with("bar"), bar, range), range)
}

/// Make a bar's parts known to the headless drivers (only when it has
/// something to scroll): `name`, `name_thumb`, `name_left`, `name_right`.
pub(crate) fn register_bar(
    actions: &mut BTreeMap<String, [f32; 4]>,
    name: &str,
    geo: &Geometry,
    range: &Range,
) {
    if !range.scrollable() {
        return;
    }
    for (suffix, rect) in [
        ("", geo.bar),
        ("_thumb", geo.thumb),
        ("_left", geo.left_button),
        ("_right", geo.right_button),
    ] {
        actions.insert(
            format!("{name}{suffix}"),
            [rect.left(), rect.top(), rect.right(), rect.bottom()],
        );
    }
}

/// One timescale band in the style of the Gantt's header: face fill, a
/// full tick at each labelled cell and a short one at the cells a stepped
/// band skips, and labels chosen by `timescale::place_labels` so every label
/// fits whole.
pub(crate) fn paint_band(
    p: &egui::Painter,
    rect: Rect,
    axis: &TimeScale,
    unit: Unit,
    cells: &[Cell],
    font: &egui::FontId,
    line: Color32,
) {
    p.rect_filled(rect, 0., CHROME);
    let spans: Vec<(f32, f32)> = cells
        .iter()
        .map(|c| {
            (
                axis.x(c.start_minute as f64).round(),
                axis.x(c.end_minute as f64).round(),
            )
        })
        .collect();
    let placed = timescale::place_labels(
        unit,
        cells,
        &spans,
        (rect.left() + 1., rect.right()),
        &mut |text| {
            p.layout_no_wrap(text.to_owned(), font.clone(), INK)
                .size()
                .x
        },
    );
    for ((cell, &(x, end)), label) in cells.iter().zip(&spans).zip(placed.labels.iter()) {
        let tick_top = if placed.is_major(cell) {
            rect.top()
        } else {
            (rect.bottom() - timescale::MINOR_TICK).max(rect.top())
        };
        chrome::vline(p, x, tick_top, rect.bottom(), line);
        let Some(label) = label else { continue };
        let room = Rect::from_min_max(Pos2::new(x, rect.top()), Pos2::new(end, rect.bottom()))
            .intersect(rect);
        let galley = p.layout_no_wrap(label.text.clone(), font.clone(), INK);
        let top = (rect.center().y - galley.size().y / 2.).round();
        let own = Rect::from_min_size(Pos2::new(label.left, top), galley.size());
        p.with_clip_rect(room.union(own).intersect(rect))
            .galley(own.min, galley, INK);
    }
}

/// The timeline that the View menu's and the Display toolbar's zoom
/// commands act on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Focus {
    /// The Activities view's top Gantt Chart, with the usage profiles and
    /// spreadsheets under it (they share its timescale).
    Activities,
    /// The bottom pane's own Gantt Chart.
    BottomGantt,
    /// The chart of the WBS, Projects or Resource Assignments view.
    View(crate::views::MainView),
}

/// The state a view keeps for its own timeline: first minute (None until
/// the user moves it: the view then starts at its default), zoom and where
/// it was drawn.
pub(crate) struct ViewTimeline<'a> {
    pub first: &'a mut Option<f64>,
    pub zoom: &'a mut f64,
    pub place: Place,
    pub limits: (f64, f64),
}

impl ViewTimeline<'_> {
    fn store(&mut self, span: Span) {
        *self.first = Some(span.first);
        *self.zoom = span.pixels_per_week;
    }

    /// Zoom In or Out one rung around the chart's centre.
    pub fn zoom_step(&mut self, zoom_in: bool, default_first: f64) {
        let span = Span::new(self.first.unwrap_or(default_first), *self.zoom);
        let next = ladder(span, zoom_in, &self.place, self.limits);
        self.store(next);
    }

    /// Zoom to `scale` around the chart's centre.
    pub fn zoom_to(&mut self, scale: f64, default_first: f64) {
        let span = Span::new(self.first.unwrap_or(default_first), *self.zoom);
        let next = span.zoomed(
            scale,
            self.place.left + self.place.width / 2.,
            self.place.left,
            self.limits,
        );
        self.store(next);
    }

    /// The whole data range in view.
    pub fn fit(&mut self) {
        if let Some(extent) = self.place.extent {
            let next = fit(extent, self.place.width, self.limits);
            self.store(next);
        }
    }
}

impl App {
    /// The Activities Gantt's timeline (top pane, or the bottom pane's while
    /// its state is swapped in).
    pub(crate) fn main_span(&self) -> Span {
        Span::new(
            self.timeline_origin as f64,
            f64::from(self.pixels_per_day) * 7.,
        )
    }

    /// Store a new Activities timeline: whole minutes, and the bands follow
    /// the zoom when they are automatic.
    pub(crate) fn set_main_span(&mut self, span: Span) {
        self.timeline_origin = span.first.round() as i64;
        let scale = (span.pixels_per_week / 7.) as f32;
        if scale != self.pixels_per_day {
            self.pixels_per_day = scale;
            self.update_timescale_units();
        }
    }

    /// Zoom limits of the Activities timeline.
    pub(crate) fn main_limits(&self) -> (f64, f64) {
        (self.minimum_scale(), view::MAX_SCALE)
    }

    /// The Activities timeline's spec for a chart: the displayed project
    /// range and the lowest band of the shown timescale.
    pub(crate) fn main_spec(&self) -> Spec {
        Spec {
            extent: self.gantt_data_extent(),
            limits: self.main_limits(),
            origin_serial: self.shown().model.origin_serial,
            unit: self.view.lower,
            options: self.label_options(),
        }
    }

    /// Run `control` on the Activities timeline (the usage panes under the
    /// Gantt share it).
    pub(crate) fn main_control(
        &mut self,
        ui: &mut egui::Ui,
        id: egui::Id,
        chart: &Chart,
    ) -> Outcome {
        let spec = self.main_spec();
        let outcome = control(ui, id, chart, self.main_span(), &spec);
        if outcome.span != self.main_span() {
            self.set_main_span(outcome.span);
        }
        if outcome.touched && self.views.main.is_activities() {
            self.panes.timeline_focus = Focus::Activities;
        }
        outcome
    }

    /// Draw a scroll bar for the Activities timeline and register it as
    /// `name`.
    pub(crate) fn main_bar(&mut self, ui: &egui::Ui, id: egui::Id, name: &str, chart: &Chart) {
        let Some(bar) = chart.bar else {
            return;
        };
        let (geo, range) = paint_bar(
            ui,
            id,
            bar,
            self.main_span(),
            chart.width,
            self.gantt_data_extent(),
        );
        register_bar(&mut self.action_rects, name, &geo, &range);
    }

    /// The timeline the zoom commands act on now.
    pub(crate) fn timeline_focus(&self) -> Focus {
        use crate::views::MainView;
        match self.views.main {
            MainView::Activities | MainView::Activities2 => {
                if self.panes.timeline_focus == Focus::BottomGantt
                    && self.panes.bottom == panes::BottomView::Gantt
                    && self.show_details
                {
                    Focus::BottomGantt
                } else {
                    Focus::Activities
                }
            }
            other => Focus::View(other),
        }
    }

    /// True when the current view shows a timeline the zoom commands act on.
    pub(crate) fn timeline_on_screen(&self) -> bool {
        use crate::views::MainView;
        match self.timeline_focus() {
            Focus::Activities | Focus::BottomGantt => true,
            Focus::View(view) => matches!(
                view,
                MainView::Wbs | MainView::Projects | MainView::Assignments
            ),
        }
    }

    /// A view's own timeline, for the zoom commands.
    fn view_timeline(&mut self, view: crate::views::MainView) -> Option<(ViewTimeline<'_>, f64)> {
        use crate::views::MainView;
        let v = &mut self.views;
        Some(match view {
            MainView::Wbs => {
                let s = &mut v.wbs;
                (
                    ViewTimeline {
                        first: &mut s.timeline_first,
                        zoom: &mut s.zoom,
                        place: s.timeline_place,
                        limits: crate::views::chart::SUMMARY_ZOOM,
                    },
                    s.timeline_default,
                )
            }
            MainView::Projects => {
                let s = &mut v.projects;
                (
                    ViewTimeline {
                        first: &mut s.timeline_first,
                        zoom: &mut s.zoom,
                        place: s.timeline_place,
                        limits: crate::views::chart::SUMMARY_ZOOM,
                    },
                    s.timeline_default,
                )
            }
            MainView::Assignments => {
                let s = &mut v.assignments;
                let default = s.timeline_default;
                (
                    ViewTimeline {
                        first: &mut s.first_minute,
                        zoom: &mut s.zoom,
                        place: s.timeline_place,
                        limits: crate::views::assignments::ZOOM_LIMITS,
                    },
                    default,
                )
            }
            _ => return None,
        })
    }

    /// View > Zoom In and Zoom Out, the Display toolbar's and the timescale
    /// menu's: one rung on the focused timeline.
    pub(crate) fn focused_zoom(&mut self, zoom_in: bool) {
        match self.timeline_focus() {
            Focus::BottomGantt => self.panes.grid.pending_zoom = Some(Some(zoom_in)),
            Focus::Activities => {
                if self.panes.top == panes::TopView::Network {
                    self.network_zoom(zoom_in);
                } else {
                    self.zoom_ladder(zoom_in);
                }
            }
            // The spreadsheet's day cells keep its own zoom steps.
            Focus::View(crate::views::MainView::Assignments) => {
                self.views.assignments.zoom_step(zoom_in)
            }
            Focus::View(view) => {
                if let Some((mut t, default)) = self.view_timeline(view) {
                    t.zoom_step(zoom_in, default);
                } else {
                    self.zoom_ladder(zoom_in);
                }
            }
        }
    }

    /// View > Zoom To Best Fit on the focused timeline.
    pub(crate) fn focused_fit(&mut self) {
        match self.timeline_focus() {
            Focus::BottomGantt => self.panes.grid.pending_zoom = Some(None),
            Focus::Activities => match self.panes.top {
                panes::TopView::Network => self.network_fit(),
                _ => self.fit(),
            },
            Focus::View(view) => {
                if let Some((mut t, _)) = self.view_timeline(view) {
                    t.fit();
                } else {
                    self.fit();
                }
            }
        }
    }

    /// A View > Zoom preset on a view's own timeline. Returns false when the
    /// focused timeline is the Activities one (the caller applies it there).
    pub(crate) fn focused_zoom_to(&mut self, scale: f64) -> bool {
        let Focus::View(view) = self.timeline_focus() else {
            return false;
        };
        match self.view_timeline(view) {
            Some((mut t, default)) => {
                t.zoom_to(scale, default);
                true
            }
            None => false,
        }
    }
}
