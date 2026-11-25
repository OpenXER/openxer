//! Builds printed pages from the current layout. The table, timescale, bars
//! and relationship lines are painted by the production painters into a
//! private egui context (no window, no input) at the print timescale; the
//! shapes become print ops and are tiled onto pages with the header and
//! footer bands.
use super::ops::{self, Clipped, Op, Page};
use super::paginate::{self, Area, Extent, Flow, RowSlot, DPI};
use super::setup::{Band, Content, Orientation, PageSetup, Scaling, Section, TextAlign};
use crate::grid::{columns, Row};
use crate::*;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

/// Gap between a band and the body, in page pixels.
const BAND_GAP: f32 = 4.;
/// Cell line color of the activity table (left out without Grid Lines).
const TABLE_LINE: Color32 = Color32::from_rgb(0xC0, 0xC0, 0xC0);
/// Longest printed timescale.
pub const MAX_SPAN_DAYS: i64 = 50 * 366;
/// Vertical distance between pages painted in one band pass.
const STRIDE: f32 = 20000.;
/// Room after the last bar for its labels, kept when trailing timescale is
/// cut from a printout that runs to several pages across.
const TAIL_ROOM: f32 = 240.;
/// Gap between the last table row and a printed bottom pane.
const BOTTOM_GAP: f32 = 12.;
/// Height of a printed usage profile.
pub const PROFILE_H: f32 = 320.;
/// Narrowest left part of a printed usage view.
const BOTTOM_LEFT_MIN: f32 = 240.;

/// The bottom pane as printed: which view, its size and its unbreakable
/// pieces (top from the block's top, height).
struct BottomBlock {
    view: panes::BottomView,
    size: Vec2,
    pieces: Vec<(f32, f32)>,
    break_before: bool,
}

/// How much of a job the layout pass has to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Paint {
    /// Paint the canvas unless the kept one matches.
    Now,
    /// Use the kept canvas when it matches; otherwise lay out only.
    Kept,
    /// Lay out only: no painting, no look at the kept canvas.
    Skip,
}

/// The pages of one print job.
#[derive(Clone, Debug, Default)]
pub struct Printed {
    pub pages: Vec<Page>,
    pub scale: f32,
    pub across: usize,
    pub pixels_per_day: f32,
    /// Timescale range printed (model minutes), when the Gantt is printed.
    pub timescale: Option<(i64, i64)>,
    /// The orientation printed (Automatic is resolved to one of the two)
    /// and the paper.
    pub orientation: Orientation,
    pub paper: String,
}

/// What a job comes to, for the result line of Page Setup and the preview.
#[derive(Clone, Debug, PartialEq)]
pub struct Summary {
    pub pages: usize,
    pub across: usize,
    pub scale: f32,
    pub orientation: Orientation,
    pub paper: String,
    pub timescale: Option<(i64, i64)>,
}

/// Text height of the screen's table in points; a printout scales it.
const SCREEN_POINTS: f32 = 8.;
/// Below this scale the text is too small to read comfortably (about 5 pt).
pub const READABLE_SCALE: f32 = 0.6;
/// Below this scale the warning is bold (under 4 pt).
pub const TINY_SCALE: f32 = 0.45;

impl Summary {
    pub fn small(&self) -> bool {
        self.scale < READABLE_SCALE - 1e-4
    }
    pub fn tiled(&self) -> bool {
        self.across > 1
    }
    /// `Prints on 2 pages, 1 across, at 63% of normal size (about 5 pt text)`.
    pub fn line(&self) -> String {
        scale_line(self.pages, self.across, self.scale)
    }
    /// The second line when pages have to be placed side by side.
    pub fn tiled_line(&self) -> Option<String> {
        self.tiled()
            .then(|| format!("{} pages across: tape or place side by side", self.across))
    }
}

/// The result line for a page count, pages across and scale.
pub fn scale_line(pages: usize, across: usize, scale: f32) -> String {
    let points = SCREEN_POINTS * scale;
    let text = if points < 1. {
        "under 1 pt text".to_string()
    } else {
        format!("about {points:.0} pt text")
    };
    let size = format!("at {:.0}% of normal size ({text})", scale * 100.);
    if pages == 1 {
        format!("Prints on 1 page {size}")
    } else {
        format!("Prints on {pages} pages, {across} across, {size}")
    }
}

/// The painted canvas of a print job and what the page layout needs of it.
pub(super) struct Canvas {
    /// The ops; `None` when the job was asked not to paint (a deferred
    /// preview build) and the canvas is not kept.
    pub ops: Option<Rc<ops::Indexed>>,
    pub size: Vec2,
    pub header_h: f32,
    pub slots: Vec<RowSlot>,
    pub body_h: f32,
    pub plan: paginate::Plan,
    pub timescale: Option<(i64, i64)>,
}

impl Canvas {
    /// A canvas that has not been painted yet.
    pub fn unpainted() -> Self {
        Self {
            ops: None,
            size: Vec2::ZERO,
            header_h: 0.,
            slots: vec![],
            body_h: 0.,
            plan: paginate::Plan {
                scale: 1.,
                pixels_per_day: 1.,
                headerless_from: None,
                span_days: 0.,
                extended: false,
            },
            timescale: None,
        }
    }
}

/// Copies a part of the canvas (a source rectangle) to a page: scaled, with
/// its top left at the given page position.
type Place<'a> = &'a dyn Fn(Rect, f32, Pos2, &mut Vec<Clipped>);

/// What a kept canvas was painted for: the project snapshot and a hash of
/// everything else the painters read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CanvasKey {
    serial: u64,
    hash: u64,
}

/// A print job whose pages are not assembled yet: the painted canvas, the
/// page grid and the header and footer bands. `App::job_page` builds one
/// page; `Printed` is every page built.
pub(crate) struct PrintJob {
    setup: PageSetup,
    page: Vec2,
    left: f32,
    /// Page y of the body when the header is not printed, and the room the
    /// header takes (with its gap) when it is.
    top: f32,
    head_room: f32,
    header_rect: Rect,
    footer_rect: Rect,
    /// The painted canvas; `None` for a layout that was not painted.
    canvas: Option<Rc<ops::Indexed>>,
    header_h: f32,
    scale: f32,
    tiles: Vec<paginate::Tile>,
    across: usize,
    blocks: usize,
    pixels_per_day: f32,
    timescale: Option<(i64, i64)>,
    /// Canvas depth from which pages print without the column header (the
    /// Trace Logic part of a printout has no table columns).
    headerless_from: Option<f32>,
    network: bool,
    /// The clock the `[print_date]` and `[print_time]` fields show.
    now: chrono::DateTime<chrono::Local>,
}

impl PrintJob {
    pub fn count(&self) -> usize {
        self.tiles.len()
    }

    /// The column header height printed on tile `t`: none on the pages of
    /// the Trace Logic part.
    fn header_for(&self, t: &paginate::Tile) -> f32 {
        if self.headerless_from.is_some_and(|from| t.y0 >= from - 0.5) {
            0.
        } else {
            self.header_h
        }
    }
    pub fn across(&self) -> usize {
        self.across
    }
    /// Size of every page at 96 pixels per inch.
    pub fn page_size(&self) -> Vec2 {
        self.page
    }
    #[cfg(test)]
    pub(super) fn tiles(&self) -> &[paginate::Tile] {
        &self.tiles
    }
    pub fn scale(&self) -> f32 {
        self.scale
    }
    /// The page y of the top of tile `t`'s column header: below the header
    /// band when that page row prints one.
    fn top_for(&self, t: &paginate::Tile) -> f32 {
        let (first, last) = (t.block == 0, t.block + 1 == self.blocks);
        if self.setup.header.include_on.on_block(first, last) {
            self.top + self.head_room
        } else {
            self.top
        }
    }
    /// Band row repeated above the body of tile `t`: its height.
    fn repeat_h(t: &paginate::Tile) -> f32 {
        t.repeat.map_or(0., |r| r.1)
    }
    /// What the job comes to.
    pub fn summary(&self) -> Summary {
        Summary {
            pages: self.count(),
            across: self.across,
            scale: self.scale,
            orientation: self.setup.orientation,
            paper: self.setup.paper.clone(),
            timescale: self.timescale,
        }
    }
}

/// A private context for laying out print shapes, with the app's fonts.
pub fn print_context() -> egui::Context {
    let ctx = egui::Context::default();
    chrome::configure(&ctx);
    ctx.set_pixels_per_point(1.);
    // Fonts load during the first pass.
    let _ = ctx.run(egui::RawInput::default(), |_| {});
    ctx
}

fn raw_input(size: Vec2) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size.max(Vec2::splat(1.)))),
        ..Default::default()
    }
}

/// Runs one pass that paints with `paint` and returns the shapes as ops.
pub(crate) fn capture(
    ctx: &egui::Context,
    size: Vec2,
    mut paint: impl FnMut(&mut egui::Ui),
) -> Vec<Clipped> {
    let _dashes = crate::gantt::paint::compact_dashes();
    let output = ctx.run(raw_input(size), |ctx| {
        let mut ui = egui::Ui::new(
            ctx.clone(),
            egui::Id::new("print_canvas"),
            egui::UiBuilder::new()
                .layer_id(egui::LayerId::background())
                .max_rect(Rect::from_min_size(Pos2::ZERO, size)),
        );
        paint(&mut ui);
    });
    ops::from_shapes_coalesced(&output.shapes)
}

/// Model minutes of the local clock.
pub fn now_minutes(origin: i64) -> i64 {
    let epoch = chrono::NaiveDate::from_ymd_opt(1899, 12, 30)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap();
    (chrono::Local::now().naive_local() - epoch).num_minutes() - origin * DAY
}

impl App {
    /// The day of a model minute in the user's date format, without a time.
    pub(super) fn day_text(&self, minutes: i64) -> String {
        let epoch = chrono::NaiveDate::from_ymd_opt(1899, 12, 30).unwrap();
        let serial = self.project.model.origin_serial + minutes.div_euclid(DAY);
        epoch
            .checked_add_signed(chrono::Duration::days(serial))
            .map(format::day)
            .unwrap_or_default()
    }
}

/// App fields the production painters change, restored after printing.
struct Saved {
    timeline_origin: i64,
    pixels_per_day: f32,
    upper: timescale::Unit,
    lower: timescale::Unit,
    gantt_left: f32,
    scroll_x: f32,
    show_relationships: bool,
    show_legend: bool,
    bar_geometry: Vec<BarGeometry>,
    row_geometry: Vec<(RowAnchor, Rect)>,
    link_geometry: Vec<(usize, Vec<Pos2>)>,
    collapsed_geometry: Vec<BarGeometry>,
    action_rects: BTreeMap<String, [f32; 4]>,
    link_drag: Option<LinkDrag>,
    edit_cell: Option<(usize, String)>,
}

impl App {
    fn save_print_state(&mut self) -> Saved {
        Saved {
            timeline_origin: self.timeline_origin,
            pixels_per_day: self.pixels_per_day,
            upper: self.view.upper,
            lower: self.view.lower,
            gantt_left: self.gantt_left,
            scroll_x: self.grid.scroll_x,
            show_relationships: self.gantt.options.show_relationships,
            show_legend: self.gantt.options.show_legend,
            bar_geometry: std::mem::take(&mut self.bar_geometry),
            row_geometry: std::mem::take(&mut self.row_geometry),
            link_geometry: std::mem::take(&mut self.link_geometry),
            collapsed_geometry: std::mem::take(&mut self.gantt_ui.collapsed_geometry),
            action_rects: self.action_rects.clone(),
            link_drag: self.link_drag.take(),
            edit_cell: self.edit_cell.take(),
        }
    }

    fn restore_print_state(&mut self, s: Saved) {
        self.timeline_origin = s.timeline_origin;
        self.pixels_per_day = s.pixels_per_day;
        self.view.upper = s.upper;
        self.view.lower = s.lower;
        self.gantt_left = s.gantt_left;
        self.grid.scroll_x = s.scroll_x;
        self.gantt.options.show_relationships = s.show_relationships;
        self.gantt.options.show_legend = s.show_legend;
        self.bar_geometry = s.bar_geometry;
        self.row_geometry = s.row_geometry;
        self.link_geometry = s.link_geometry;
        self.gantt_ui.collapsed_geometry = s.collapsed_geometry;
        self.action_rects = s.action_rects;
        self.link_drag = s.link_drag;
        self.edit_cell = s.edit_cell;
        self.drop_frame_cache();
    }

    /// Earliest start and latest finish shown on bars.
    pub(crate) fn print_project_span(&self) -> (i64, i64) {
        let m = &self.project.model;
        let leveled = m.options.do_level;
        let start = m
            .activities
            .iter()
            .filter_map(|a| displayed_start(a, leveled))
            .min()
            .or(m.project.data_date)
            .unwrap_or(0);
        let finish = m
            .activities
            .iter()
            .filter_map(|a| displayed_finish(a, leveled))
            .max()
            .unwrap_or(start + 30 * DAY);
        (start, finish.max(start))
    }

    /// The span the chart pane shows (model minutes): from its left edge,
    /// as wide as the pane at its zoom. `None` while no chart is on screen
    /// (a headless run, or the table alone).
    pub(crate) fn screen_timescale(&self) -> Option<(i64, i64)> {
        if !self.print.screen_seen || self.chart_width < 1. {
            return None;
        }
        let start = self.timeline_origin;
        let days = f64::from(self.chart_width) / f64::from(self.pixels_per_day.max(1e-3));
        let span = ((days * DAY as f64).ceil() as i64).clamp(DAY / 24, MAX_SPAN_DAYS * DAY);
        Some((start, start + span))
    }

    /// The minute a timescale expression (`DD-1W`, `PF+1W`, `SW`, a date)
    /// stands for in this project, before the printout rounds it to whole
    /// days. The Options tab's date picker shows this same value.
    pub(crate) fn resolve_print_expr(&self, text: &str) -> Result<i64, String> {
        self.print_expr(text).map(|(_, minutes)| minutes)
    }

    /// [`resolve_print_expr`] with the expression it read.
    fn print_expr(&self, text: &str) -> Result<(super::setup::DateExpr, i64), String> {
        let origin = self.project.model.origin_serial;
        let (ps, pf) = self.print_project_span();
        let dd = self.project.model.project.data_date.unwrap_or(ps);
        let now = now_minutes(origin);
        let anchor = |a: super::setup::Anchor| match a {
            super::setup::Anchor::ProjectStart => ps,
            super::setup::Anchor::ProjectFinish => pf,
            super::setup::Anchor::DataDate => dd,
            super::setup::Anchor::CurrentDate => now,
            week_or_month => crate::grid::relative::resolve_at(
                &self.project,
                &crate::grid::relative::Relative {
                    anchor: week_or_month,
                    offset: 0,
                    unit: crate::grid::relative::Unit::Days,
                },
                now,
            )
            .unwrap_or(now),
        };
        let parse = |t: &str| format::parse_date(origin, t, Some(0)).ok().flatten();
        let expr = super::setup::parse_expr(text, parse)?;
        Ok((expr, expr.resolve(anchor)?))
    }

    /// Resolves the timescale range (model minutes): the chart pane's with
    /// Same as screen, else the Options tab's expressions.
    pub(crate) fn print_timescale(&self, setup: &PageSetup) -> Result<(i64, i64), String> {
        if setup.options.same_as_screen {
            if let Some(range) = self.screen_timescale() {
                return Ok(range);
            }
        }
        let start = self
            .resolve_print_expr(&setup.options.timescale_start)?
            .div_euclid(DAY)
            * DAY;
        let (expr, mut finish) = self.print_expr(&setup.options.timescale_finish)?;
        // A finish given as a date includes that whole day.
        if matches!(expr, super::setup::DateExpr::Fixed(m) if m.rem_euclid(DAY) == 0) {
            finish = finish.saturating_add(DAY);
        }
        let finish = finish
            .checked_add(DAY - 1)
            .ok_or("The timescale finish is out of range.")?
            .div_euclid(DAY)
            * DAY;
        if finish <= start {
            return Err("The timescale finish must be after its start.".into());
        }
        if finish - start > MAX_SPAN_DAYS * DAY {
            return Err("The printed timescale can span at most 50 years.".into());
        }
        Ok((start, finish))
    }

    /// Values of the `[field]` tokens for one page.
    pub(crate) fn print_field(&self, token: &str, page: usize, pages: usize) -> Option<String> {
        self.print_field_at(chrono::Local::now(), token, (page, pages, 1.))
    }

    /// `print_field` with the clock of a print job.
    fn print_field_at(
        &self,
        now: chrono::DateTime<chrono::Local>,
        token: &str,
        (page, pages, scale): (usize, usize, f32),
    ) -> Option<String> {
        let origin = self.project.model.origin_serial;
        let (ps, pf) = self.print_project_span();
        Some(match token {
            "project_id" => self.project.model.project.name.clone(),
            "project_name" => views::project_title(&self.project),
            "layout_name" => match self.print_view() {
                Some((_, caption)) => caption.to_string(),
                None => self.grid.layout.name.clone(),
            },
            "filter_list" if self.print_view().is_some() => String::new(),
            "filter_list" => self.grid.layout.filter.summary(&self.grid.user_filters),
            "data_date" => format::date(origin, self.project.model.project.data_date),
            "start_date" => format::date(origin, Some(ps)),
            "finish_date" => format::date(origin, Some(pf)),
            "print_date" => format::day(now.date_naive()),
            "print_time" => format::time_of_day(now.time()),
            "page_number" => (page + 1).to_string(),
            "total_pages" => pages.to_string(),
            "scale" => format!("{:.0}%", scale * 100.),
            _ => return None,
        })
    }

    /// The bottom pane view that prints under the table and Gantt, as its
    /// Page Setup option (0 Profile, 1 Spreadsheet, 2 Trace Logic).
    pub(crate) fn printable_bottom(&self) -> Option<(usize, panes::BottomView)> {
        use panes::BottomView as B;
        if !self.views.main.is_activities()
            || !self.show_details
            || self.panes.top == panes::TopView::Network
        {
            return None;
        }
        let kind = match self.panes.bottom {
            B::ActivityUsageProfile | B::ResourceUsageProfile => 0,
            B::ActivityUsageSheet | B::ResourceUsageSheet => 1,
            B::TraceLogic => 2,
            _ => return None,
        };
        Some((kind, self.panes.bottom))
    }

    /// The bottom pane to print with `setup`, and its size and pieces.
    fn print_bottom_block(&mut self, setup: &PageSetup, rows: &[Row]) -> Option<BottomBlock> {
        use panes::BottomView as B;
        let (kind, view) = self.printable_bottom()?;
        let o = &setup.options;
        if ![o.profile, o.spreadsheet, o.trace_logic][kind] {
            return None;
        }
        let head = panes::usage::HEADER_H;
        let row = panes::usage::ROW_H;
        Some(match view {
            B::ActivityUsageProfile | B::ResourceUsageProfile => BottomBlock {
                view,
                size: Vec2::new(0., PROFILE_H),
                pieces: vec![(0., PROFILE_H)],
                break_before: false,
            },
            B::ActivityUsageSheet => {
                let metrics = self.row_metrics(rows);
                let mut pieces = vec![];
                for k in 0..rows.len() {
                    let (top, h) = (head + metrics.offsets[k], metrics.heights[k]);
                    // The header stays with the first row.
                    pieces.push(if k == 0 { (0., top + h) } else { (top, h) });
                }
                let total = head + metrics.offsets.last().copied().unwrap_or(0.);
                BottomBlock {
                    view,
                    size: Vec2::new(0., total.max(head)),
                    pieces,
                    break_before: false,
                }
            }
            B::ResourceUsageSheet => {
                let count = self.resource_sheet_line_count();
                let pieces = (0..count)
                    .map(|k| {
                        if k == 0 {
                            (0., head + row)
                        } else {
                            (head + k as f32 * row, row)
                        }
                    })
                    .collect();
                BottomBlock {
                    view,
                    size: Vec2::new(0., head + count as f32 * row),
                    pieces,
                    break_before: false,
                }
            }
            B::TraceLogic => {
                let (_, graph) = self.current_trace()?;
                let size = Vec2::new(
                    panes::trace::ORIGIN.x + graph.columns() as f32 * panes::trace::COLUMN_PITCH,
                    panes::trace::ORIGIN.y + graph.rows().max(1) as f32 * panes::trace::ROW_PITCH,
                );
                BottomBlock {
                    view,
                    size,
                    pieces: vec![(0., size.y)],
                    break_before: true,
                }
            }
            B::Details | B::ActivityTable | B::Gantt => return None,
        })
    }

    /// Paint the bottom pane at `rect`. The usage views' timescale lines up
    /// with the printed Gantt, whose chart starts at `chart_left` with
    /// `start` (model minutes).
    fn paint_print_bottom(
        &mut self,
        ui: &mut egui::Ui,
        view: panes::BottomView,
        rect: Rect,
        chart_left: f32,
        start: Option<i64>,
    ) {
        use panes::BottomView as B;
        let p = ui.painter().clone();
        p.rect_filled(rect, 0., Color32::WHITE);
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect)
                .layout(egui::Layout::top_down(egui::Align::LEFT)),
        );
        let extra = match view {
            B::ActivityUsageProfile | B::ResourceUsageProfile => 6. + panes::usage::AXIS_W,
            // The spreadsheets' value column, as wide as their widest total.
            B::ActivityUsageSheet => 6. + self.usage_value_width(ui.ctx(), true),
            B::ResourceUsageSheet => 6. + self.usage_value_width(ui.ctx(), false),
            _ => 6. + panes::usage::VALUE_W,
        };
        let left = (chart_left - rect.left() - extra).max(BOTTOM_LEFT_MIN);
        if let Some(start) = start {
            // The axis starts at `left + extra`; shift its first date so a
            // date sits at the same x as on the chart above.
            let offset = left + extra - (chart_left - rect.left());
            self.timeline_origin =
                start + (offset as f64 / self.pixels_per_day.max(1e-3) as f64 * DAY as f64) as i64;
        }
        self.panes.printing = true;
        match view {
            B::ActivityUsageProfile => self.activity_usage_profile(&mut child, rect, left),
            B::ResourceUsageProfile => self.resource_usage_profile(&mut child, rect, left),
            B::ActivityUsageSheet => self.activity_usage_sheet(&mut child, rect, left),
            B::ResourceUsageSheet => self.resource_usage_sheet(&mut child, rect, left),
            B::TraceLogic => {
                let boxes = std::mem::take(&mut self.panes.trace.boxes);
                self.trace_logic_pane(&mut child, rect);
                self.panes.trace.boxes = boxes;
            }
            B::Details | B::ActivityTable | B::Gantt => {}
        }
        self.panes.printing = false;
        chrome::hline(&p, rect.left(), rect.right(), rect.top(), INK);
    }

    /// Width of the table on screen (the part visible without scrolling).
    pub(super) fn screen_table_width(&self) -> f32 {
        let f = self.grid_fraction.clamp(0.3, 0.95);
        (self.chart_width * f / (1. - f)).max(100.)
    }

    /// The printed pages of the Activities view with `setup`.
    pub(crate) fn compose_print(
        &mut self,
        ctx: &egui::Context,
        setup: &PageSetup,
    ) -> Result<Printed, String> {
        let job = self
            .compose_job(ctx, setup, true)?
            .ok_or("The pages could not be painted.")?;
        Ok(self.printed_pages(ctx, &job))
    }

    /// Every page of `job`.
    pub(crate) fn printed_pages(&self, ctx: &egui::Context, job: &PrintJob) -> Printed {
        Printed {
            pages: (0..job.count())
                .map(|k| self.job_page(ctx, job, k))
                .collect(),
            scale: job.scale,
            across: job.across,
            pixels_per_day: job.pixels_per_day,
            timescale: job.timescale,
            orientation: job.setup.orientation,
            paper: job.setup.paper.clone(),
        }
    }

    /// Lays out the pages of the current view with `setup`: the canvas
    /// (painted, or taken from the kept one) and the page grid. With
    /// `paint` false a canvas that is not kept is not painted and the
    /// result is `None`; every error that does not need the painted canvas
    /// (margins, timescale, page limit) is still reported.
    pub(crate) fn compose_job(
        &mut self,
        ctx: &egui::Context,
        setup: &PageSetup,
        paint: bool,
    ) -> Result<Option<PrintJob>, String> {
        let paint = if paint { Paint::Now } else { Paint::Kept };
        let job = self.compose_layout(ctx, setup, paint)?;
        Ok(job.canvas.is_some().then_some(job))
    }

    /// What printing the current view with `setup` comes to, without
    /// painting anything. `None` for the views whose layout is known only
    /// by painting them.
    pub(crate) fn plan_summary(&mut self, setup: &PageSetup) -> Option<Result<Summary, String>> {
        if !self.views.main.is_activities() {
            return None;
        }
        let ctx = self.print_context();
        Some(
            self.compose_layout(&ctx, setup, Paint::Skip)
                .map(|job| job.summary()),
        )
    }

    /// Portrait or Landscape, whichever prints the Activities view larger;
    /// the same size goes to the one with fewer pages, then to Landscape.
    fn best_orientation(&mut self, ctx: &egui::Context, setup: &PageSetup) -> Orientation {
        if !self.views.main.is_activities() {
            return Orientation::Landscape;
        }
        let mut best: Option<(Orientation, f32, usize)> = None;
        for orientation in [Orientation::Landscape, Orientation::Portrait] {
            let mut trial = setup.clone();
            trial.orientation = orientation;
            let Ok(job) = self.compose_layout(ctx, &trial, Paint::Skip) else {
                continue;
            };
            let (scale, pages) = (job.scale, job.count());
            let better = best.is_none_or(|(_, s, p)| {
                scale > s + 1e-4 || ((scale - s).abs() <= 1e-4 && pages < p)
            });
            if better {
                best = Some((orientation, scale, pages));
            }
        }
        best.map_or(Orientation::Landscape, |b| b.0)
    }

    /// The page layout of the current view with `setup`, and the canvas as
    /// `paint` says.
    pub(super) fn compose_layout(
        &mut self,
        ctx: &egui::Context,
        setup: &PageSetup,
        paint: Paint,
    ) -> Result<PrintJob, String> {
        let mut setup = setup.clone();
        setup.sanitize();
        if setup.orientation == Orientation::Automatic {
            setup.orientation = self.best_orientation(ctx, &setup);
        }
        let (w_in, h_in) = setup.page_inches();
        let page = Vec2::new(w_in * DPI, h_in * DPI);
        let m = setup.margins;
        let header_px = if setup.header.include_on == super::setup::IncludeOn::NoPages {
            0.
        } else {
            setup.header.height * DPI
        };
        let footer_px = if setup.footer.include_on == super::setup::IncludeOn::NoPages {
            0.
        } else {
            setup.footer.height * DPI
        };
        let left = m.left * DPI;
        let top = m.top * DPI;
        let head_room = if header_px > 0. {
            header_px + BAND_GAP
        } else {
            0.
        };
        let foot_room = if footer_px > 0. {
            footer_px + BAND_GAP
        } else {
            0.
        };
        let page_body = page.y - (m.top + m.bottom) * DPI;
        // The smallest page: every band that prints anywhere reserved.
        let area = Area {
            width: page.x - (m.left + m.right) * DPI,
            height: page_body - head_room - foot_room,
        };
        if area.width < 72. || area.height < 72. {
            return Err(
                "The margins, header and footer leave less than an inch to print on.".into(),
            );
        }
        let activities = self.views.main.is_activities();
        let network = activities && self.panes.top == panes::TopView::Network;
        let saved = self.save_print_state();
        // The chart's context menu must not paint into the printout.
        let menu = self.gantt_ui.menu.take();
        let result = if !activities {
            self.compose_view(ctx, &setup, area, paint)
        } else if network {
            self.compose_network(ctx, &setup, area, paint)
        } else {
            self.compose_table_gantt(ctx, &setup, area, paint)
        };
        self.gantt_ui.menu = menu;
        self.restore_print_state(saved);
        let canvas = result?;
        // A page row that prints a band gives up room for it; the others
        // keep the room.
        let (header_on, footer_on) = (setup.header.include_on, setup.footer.include_on);
        let mut scale = canvas.plan.scale.max(0.01);
        let header_h = canvas.header_h;
        let room_at = |scale: f32, k: usize, last: bool| -> f32 {
            let first = k == 0;
            let used = if header_on.on_block(first, last) {
                head_room
            } else {
                0.
            } + if footer_on.on_block(first, last) {
                foot_room
            } else {
                0.
            };
            (page_body - used) / scale - header_h
        };
        // Fit to N pages tall: pages break between rows, so the rows can
        // need one page more than the height allowed for; shrink until not.
        if let Scaling::Fit { tall, .. } = setup.effective_scaling() {
            if tall > 0 && !network {
                scale = paginate::fit_tall(tall, scale, &|s| {
                    paginate::blocks_with(&canvas.slots, canvas.body_h, &|k, last| {
                        room_at(s, k, last)
                    })
                    .len()
                });
            }
        }
        let room = |k: usize, last: bool| room_at(scale, k, last);
        let tiles = paginate::tiles_with(
            canvas.size.x,
            &canvas.slots,
            canvas.body_h,
            scale,
            area.width,
            &room,
        )?;
        let across = tiles.iter().map(|t| t.column + 1).max().unwrap_or(1);
        let blocks = tiles.iter().map(|t| t.block + 1).max().unwrap_or(1);
        let header_rect = Rect::from_min_size(
            Pos2::new(left, m.top * DPI),
            Vec2::new(area.width, header_px),
        );
        let footer_rect = Rect::from_min_size(
            Pos2::new(left, page.y - m.bottom * DPI - footer_px),
            Vec2::new(area.width, footer_px),
        );
        Ok(PrintJob {
            setup,
            page,
            left,
            top,
            head_room,
            header_rect,
            footer_rect,
            canvas: canvas.ops,
            header_h: canvas.header_h,
            scale,
            tiles,
            across,
            blocks,
            pixels_per_day: canvas.plan.pixels_per_day,
            timescale: canvas.timescale,
            headerless_from: canvas.plan.headerless_from,
            network,
            now: chrono::Local::now(),
        })
    }

    /// The ops of tile `t` of `job` (its column header, repeated band row and
    /// slice of the body, and a thin frame) with `place` copying a part of
    /// the canvas: scaled by `s`, its top left at the given page position.
    fn tile_ops(job: &PrintJob, t: &paginate::Tile, place: Place) -> Vec<Clipped> {
        let s = job.scale;
        let (left, top) = (job.left, job.top_for(t));
        let full_header = job.header_h;
        // Pages from the Trace Logic on print without the header.
        let header_h = job.header_for(t);
        let repeat_h = PrintJob::repeat_h(t);
        let mut out = vec![];
        if header_h > 0. {
            place(
                Rect::from_min_max(Pos2::new(t.x0, 0.), Pos2::new(t.x1, header_h)),
                s,
                Pos2::new(left, top),
                &mut out,
            );
        }
        if let Some((band_top, band_h)) = t.repeat {
            place(
                Rect::from_min_max(
                    Pos2::new(t.x0, full_header + band_top),
                    Pos2::new(t.x1, full_header + band_top + band_h),
                ),
                s,
                Pos2::new(left, top + header_h * s),
                &mut out,
            );
        }
        place(
            Rect::from_min_max(
                Pos2::new(t.x0, full_header + t.y0),
                Pos2::new(t.x1, full_header + t.y1),
            ),
            s,
            Pos2::new(left, top + (header_h + repeat_h) * s),
            &mut out,
        );
        // A thin frame around the printed part of the canvas.
        let frame = Rect::from_min_size(
            Pos2::new(left, top),
            Vec2::new((t.x1 - t.x0) * s, (header_h + repeat_h + t.y1 - t.y0) * s),
        );
        out.push(Clipped {
            clip: frame.expand(1.),
            op: Op::Stroke {
                points: vec![
                    frame.left_top(),
                    frame.right_top(),
                    frame.right_bottom(),
                    frame.left_bottom(),
                ],
                closed: true,
                width: 1.,
                color: Color32::BLACK,
            },
        });
        out
    }

    /// Page `k` of `job` (from 0): its piece of the canvas, a frame around
    /// it and the header and footer bands.
    pub(crate) fn job_page(&self, ctx: &egui::Context, job: &PrintJob, k: usize) -> Page {
        let t = &job.tiles[k];
        let count = job.tiles.len();
        let canvas = job.canvas.as_ref().expect("a painted job");
        let mut out = Self::tile_ops(job, t, &|source, s, dest, out| {
            canvas.place(source, s, dest, out)
        });
        // Header and footer bands. The page is painted at a vertical
        // offset of `STRIDE` pages, as when all pages share one pass, so
        // the positions are the same to the last bit.
        let bands = capture(ctx, Vec2::new(job.page.x, STRIDE * (k + 1) as f32), |ui| {
            let painter = ui.painter().clone();
            let shift = Vec2::new(0., STRIDE * k as f32);
            for (band, rect) in [
                (&job.setup.header, job.header_rect),
                (&job.setup.footer, job.footer_rect),
            ] {
                if rect.height() > 0. && band.include_on.on(k, count) {
                    self.paint_band(
                        &painter,
                        rect.translate(shift),
                        band,
                        (job.now, k, count, job.scale),
                        job.network,
                    );
                }
            }
        });
        for c in bands {
            let at = (c.clip.center().y / STRIDE).floor().max(0.) as usize;
            if at == k {
                let shift = Vec2::new(0., -STRIDE * k as f32);
                out.push(Clipped {
                    clip: c.clip.translate(shift),
                    op: c.op.transformed(1., shift),
                });
            }
        }
        Page {
            size: job.page,
            ops: out,
        }
    }

    /// Every page as the first version built them, all in one pass with
    /// the bands of all pages together; tests compare `job_page` with it.
    #[cfg(test)]
    pub(crate) fn reference_pages(&self, ctx: &egui::Context, job: &PrintJob) -> Vec<Page> {
        let count = job.tiles.len();
        let canvas = job.canvas.as_ref().expect("a painted job");
        let mut pages: Vec<Page> = job
            .tiles
            .iter()
            .map(|t| Page {
                size: job.page,
                ops: Self::tile_ops(job, t, &|source, s, dest, out| {
                    ops::place(&canvas.ops, source, s, dest, out)
                }),
            })
            .collect();
        let bands = capture(ctx, Vec2::new(job.page.x, STRIDE * count as f32), |ui| {
            let painter = ui.painter().clone();
            for k in 0..count {
                let shift = Vec2::new(0., STRIDE * k as f32);
                for (band, rect) in [
                    (&job.setup.header, job.header_rect),
                    (&job.setup.footer, job.footer_rect),
                ] {
                    if rect.height() > 0. && band.include_on.on(k, count) {
                        self.paint_band(
                            &painter,
                            rect.translate(shift),
                            band,
                            (job.now, k, count, job.scale),
                            job.network,
                        );
                    }
                }
            }
        });
        for c in bands {
            let k = (c.clip.center().y / STRIDE).floor().max(0.) as usize;
            if let Some(p) = pages.get_mut(k) {
                let shift = Vec2::new(0., -STRIDE * k as f32);
                p.ops.push(Clipped {
                    clip: c.clip.translate(shift),
                    op: c.op.transformed(1., shift),
                });
            }
        }
        pages
    }

    /// Keeps `ops` for the next job when `key` is set; the kept canvas is
    /// dropped when the print dialogs close (see `print_dialogs`).
    fn painted_canvas(
        &mut self,
        key: Option<CanvasKey>,
        paint: bool,
        make: impl FnOnce(&mut Self) -> Vec<Clipped>,
    ) -> Option<Rc<ops::Indexed>> {
        let key = key.filter(|_| self.print.keep_canvas);
        if let (Some(key), Some((kept, ops))) = (key, self.print.canvas.as_ref()) {
            if *kept == key {
                return Some(ops.clone());
            }
        }
        if !paint {
            return None;
        }
        #[cfg(test)]
        {
            self.print.paints += 1;
        }
        let ops = Rc::new(ops::Indexed::new(make(self)));
        if let Some(key) = key {
            self.print.canvas = Some((key, ops.clone()));
        }
        Some(ops)
    }

    /// A hash of the user-visible state the painters read besides the
    /// project: the layout, chart settings, formats, panes and the rows.
    /// `None` while the project is not exactly one undo snapshot.
    fn canvas_key(&self, rows: &[Row], extra: impl Hash) -> Option<CanvasKey> {
        let serial = self.project_serial()?;
        let mut h = std::collections::hash_map::DefaultHasher::new();
        format!(
            "{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}",
            self.grid.layout,
            self.grid.user_filters,
            self.gantt,
            self.formats,
            self.panes.top,
            self.panes.bottom,
            self.panes.trace.options,
            self.panes.trace.focus,
            self.panes.usage,
            self.panes.network.options,
            self.panes.network.positions,
            self.selected,
            self.show_details,
            (
                self.grid.scroll_x.to_bits(),
                self.grid_fraction.to_bits(),
                self.chart_width.to_bits(),
                self.pixels_per_day.to_bits(),
                self.timeline_origin,
            ),
            self.grid.highlighted.borrow(),
        )
        .hash(&mut h);
        for row in rows {
            match row {
                Row::Group(b) => (0u8, *b).hash(&mut h),
                Row::Activity(i) => (1u8, *i).hash(&mut h),
            }
        }
        extra.hash(&mut h);
        Some(CanvasKey {
            serial,
            hash: h.finish(),
        })
    }

    fn compose_table_gantt(
        &mut self,
        ctx: &egui::Context,
        setup: &PageSetup,
        area: Area,
        paint: Paint,
    ) -> Result<Canvas, String> {
        let o = &setup.options;
        if !o.activity_table && !o.gantt_chart {
            return Err("Choose Activity Table, Gantt Chart or both on the Options tab.".into());
        }
        let catalog = columns::catalog(&self.project);
        let all: f32 = self.visible_columns(&catalog).iter().map(|c| c.width).sum();
        let table_w = if !o.activity_table {
            0.
        } else if o.all_columns {
            self.grid.scroll_x = 0.;
            all
        } else {
            self.screen_table_width()
                .min(all - self.grid.scroll_x)
                .max(1.)
        }
        .round();
        let timescale = if o.gantt_chart {
            Some(self.print_timescale(setup)?)
        } else {
            None
        };
        let span_days = timescale.map_or(0., |(a, b)| (b - a) as f64 / DAY as f64);
        let rows = self.rows();
        let metrics = self.row_metrics(&rows);
        let rows_h = *metrics.offsets.last().unwrap_or(&0.);
        let header_h = self.gantt_header_height();
        let bottom = self.print_bottom_block(setup, &rows);
        let body_h = rows_h + bottom.as_ref().map_or(0., |b| BOTTOM_GAP + b.size.y);
        // Width the bottom pane needs across the pages.
        let min_width = bottom.as_ref().map_or(0., |b| {
            if b.view == panes::BottomView::TraceLogic {
                b.size.x
            } else {
                // Room for the usage view's left part and some periods.
                BOTTOM_LEFT_MIN + 6. + panes::usage::VALUE_W + 300.
            }
        });
        let flow = Flow {
            one_wide: !setup.allow_tiling,
            extend: o.same_as_screen && self.screen_timescale().is_some(),
            min_width,
        };
        let mut plan = paginate::plan_with(
            setup.effective_scaling(),
            area,
            Extent {
                table_width: table_w,
                span_days,
                header_height: header_h,
                body_height: body_h,
            },
            self.pixels_per_day,
            flow,
        );
        // The Trace Logic page has no table columns, so it does not repeat
        // the table's column header.
        plan.headerless_from = bottom
            .as_ref()
            .filter(|b| b.view == panes::BottomView::TraceLogic)
            .map(|_| rows_h + BOTTOM_GAP);
        // An extended timescale fills the page to the pixel.
        let mut chart_w = if plan.extended {
            (plan.span_days * f64::from(plan.pixels_per_day) + 1e-3).floor() as f32
        } else {
            (plan.span_days as f32 * plan.pixels_per_day).ceil()
        };
        // Pages side by side at a set percent: padding past the last bar
        // that would make a page of its own is cut.
        if let (Some((start, _)), Scaling::Adjust { .. }, true, false) =
            (timescale, setup.scaling, setup.allow_tiling, flow.extend)
        {
            let (_, finish) = self.print_project_span();
            let drawn = ((finish - start).max(0) as f64 / DAY as f64) as f32 * plan.pixels_per_day
                + TAIL_ROOM;
            let used = (drawn.ceil()).max(1.);
            let across =
                |chart: f32| paginate::columns((table_w + chart).max(min_width), plan.scale, area);
            if used < chart_w && across(used) < across(chart_w) {
                chart_w = used;
                plan.span_days = f64::from(used) / f64::from(plan.pixels_per_day);
                plan.extended = true;
            }
        }
        let timescale = timescale
            .map(|(start, _)| (start, start + (plan.span_days * DAY as f64).round() as i64));
        let mut width = table_w + chart_w;
        if let Some(b) = &bottom {
            width = width.max(b.size.x);
            if b.view != panes::BottomView::TraceLogic {
                width = width.max(min_width);
            }
        }
        paginate::check_pages(width, header_h, body_h, plan.scale, area)?;
        let size = Vec2::new(width, header_h + body_h);
        let break_groups = o.break_every_group && !self.grid.layout.group_sort.levels.is_empty();
        // Group and Sort's Page Break marks band levels that start a page.
        let page_breaks = self.grid.layout.group_sort.page_breaks.clone();
        let mut slots: Vec<RowSlot> = {
            let bands = self.grid.bands.borrow();
            // The band row above each row, to repeat on a page that starts
            // inside its group.
            let mut band_row: Option<(f32, f32)> = None;
            rows.iter()
                .enumerate()
                .map(|(k, row)| {
                    let (top, height) = (metrics.offsets[k], metrics.heights[k]);
                    match row {
                        Row::Group(b) => {
                            let level = bands.get(*b).map(|b| b.level);
                            band_row = Some((top, height));
                            RowSlot {
                                top,
                                height,
                                break_before: level.is_some_and(|level| {
                                    (break_groups && level == 0) || page_breaks.contains(&level)
                                }),
                                keep_with_next: true,
                                repeat: None,
                            }
                        }
                        Row::Activity(_) => RowSlot {
                            top,
                            height,
                            break_before: false,
                            keep_with_next: false,
                            repeat: band_row.filter(|_| o.repeat_band),
                        },
                    }
                })
                .collect()
        };
        if let Some(b) = &bottom {
            let top = rows_h + BOTTOM_GAP;
            for (k, (y, h)) in b.pieces.iter().enumerate() {
                slots.push(RowSlot {
                    top: top + y,
                    height: *h,
                    break_before: k == 0 && b.break_before,
                    ..RowSlot::default()
                });
            }
        }
        let canvas = |ops| Canvas {
            ops,
            size,
            header_h,
            slots,
            body_h,
            plan,
            timescale,
        };
        if paint == Paint::Skip {
            return Ok(canvas(None));
        }
        if let Some((start, _)) = timescale {
            self.timeline_origin = start;
            self.pixels_per_day = plan.pixels_per_day;
            self.update_timescale_units();
        }
        self.gantt.options.show_relationships = o.relationships;
        self.gantt.options.show_legend = false;
        let start = timescale.map(|(start, _)| start);
        let key = self.canvas_key(
            &rows,
            (
                size.x.to_bits(),
                size.y.to_bits(),
                table_w.to_bits(),
                chart_w.to_bits(),
                header_h.to_bits(),
                plan.pixels_per_day.to_bits(),
                timescale,
                (
                    o.activity_table,
                    o.gantt_chart,
                    o.relationships,
                    o.grid_lines,
                ),
                bottom
                    .as_ref()
                    .map(|b| (format!("{:?}", b.view), b.size.y.to_bits())),
            ),
        );
        let ops = self.painted_canvas(key, paint == Paint::Now, |app| {
            let mut ops = capture(ctx, size, |ui| {
                app.paint_print_canvas(ui, &rows, table_w, chart_w, header_h);
                if let Some(b) = &bottom {
                    let rect = Rect::from_min_max(
                        Pos2::new(0., header_h + rows_h + BOTTOM_GAP),
                        Pos2::new(width, header_h + body_h),
                    );
                    app.paint_print_bottom(ui, b.view, rect, table_w, start);
                }
            });
            if !o.grid_lines {
                ops.retain(|c| match &c.op {
                    Op::Fill { rect, color } => {
                        !(*color == TABLE_LINE
                            && rect.right() <= table_w + 0.5
                            && rect.top() >= header_h
                            && (rect.width() <= 1.01 || rect.height() <= 1.01))
                    }
                    _ => true,
                });
            }
            ops
        });
        Ok(canvas(ops))
    }

    /// The table and Gantt as the Activities view paints them, every row,
    /// at the canvas origin.
    fn paint_print_canvas(
        &mut self,
        ui: &mut egui::Ui,
        rows: &[Row],
        table_w: f32,
        chart_w: f32,
        header_h: f32,
    ) {
        let ctx = ui.ctx().clone();
        let width = table_w + chart_w;
        let head = Rect::from_min_size(Pos2::ZERO, Vec2::new(width, header_h));
        let p = ui.painter().clone();
        p.rect_filled(head, 0., CHROME);
        self.drop_frame_cache();
        if table_w > 0. {
            self.grid_header(ui, &ctx, head, table_w);
        }
        let body_bottom = ui.max_rect().bottom();
        let gantt = (chart_w > 0.).then(|| {
            let chart = Rect::from_min_max(Pos2::new(table_w, 0.), head.max);
            // A header-high child keeps the timescale from painting a
            // backdrop under every row (each row paints its own).
            let mut header = ui.new_child(egui::UiBuilder::new().max_rect(head));
            let mut g = self.gantt_header(&mut header, &ctx, chart, rows);
            g.body = Rect::from_min_max(
                Pos2::new(chart.left(), chart.bottom()),
                Pos2::new(chart.right(), body_bottom),
            );
            g
        });
        let metrics = self.row_metrics(rows);
        for (k, row) in rows.iter().enumerate() {
            let rect = Rect::from_min_size(
                Pos2::new(0., header_h + metrics.offsets[k]),
                Vec2::new(width, metrics.heights[k]),
            );
            let chart_row = Rect::from_min_max(Pos2::new(table_w, rect.top()), rect.max);
            let response = ui.interact(rect, ui.id().with(("print_row", k)), Sense::hover());
            let painter = ui.painter().clone();
            match *row {
                Row::Group(id) => {
                    if table_w > 0. {
                        self.grid_band(&painter, rect, table_w, id, &response);
                    }
                    if let Some(g) = &gantt {
                        self.gantt_group_row(&painter, g, chart_row, k, id);
                    }
                }
                Row::Activity(i) => {
                    let a = self.project.model.activities[i].clone();
                    painter.rect_filled(rect, 0., Color32::WHITE);
                    if table_w > 0. {
                        self.grid_cells(
                            ui,
                            &ctx,
                            &painter,
                            rect,
                            table_w,
                            i,
                            metrics.depths[k],
                            false,
                            &response,
                        );
                    }
                    if let Some(g) = &gantt {
                        self.gantt_activity_row(ui, &painter, g, rect, chart_row, k, i, &a, false);
                    }
                }
            }
        }
        if let Some(g) = &gantt {
            let body = g.body;
            self.gantt_overlay(ui, &ctx, g, body, None);
        }
        if table_w > 0. && chart_w > 0. {
            // The divider between the table and the chart.
            chrome::vline(
                &p,
                table_w - 1.,
                0.,
                header_h + metrics.offsets.last().unwrap_or(&0.),
                INK,
            );
        }
    }

    fn compose_network(
        &mut self,
        ctx: &egui::Context,
        setup: &PageSetup,
        area: Area,
        paint: Paint,
    ) -> Result<Canvas, String> {
        let net = self.network_layout();
        let size = net.size.max(Vec2::splat(1.));
        let plan = paginate::plan_with(
            setup.effective_scaling(),
            area,
            Extent {
                table_width: size.x,
                span_days: 0.,
                header_height: 0.,
                body_height: size.y,
            },
            self.pixels_per_day,
            Flow {
                one_wide: !setup.allow_tiling,
                ..Flow::default()
            },
        );
        paginate::check_pages(size.x, 0., size.y, plan.scale, area)?;
        // Boxes are kept whole: rows break between box rows.
        let mut slots: Vec<RowSlot> = vec![];
        let mut tops: Vec<(f32, f32)> = net
            .boxes
            .iter()
            .map(|b| (b.rect.top() - 4., b.rect.bottom() + 8.))
            .collect();
        tops.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut last = 0.;
        for (t, b) in tops {
            if t >= last {
                slots.push(RowSlot::new(last, b - last));
                last = b;
            } else if b > last {
                if let Some(s) = slots.last_mut() {
                    s.height = b - s.top;
                }
                last = b;
            }
        }
        let canvas = |ops| Canvas {
            ops,
            size,
            header_h: 0.,
            slots,
            body_h: size.y,
            plan,
            timescale: None,
        };
        if paint == Paint::Skip {
            return Ok(canvas(None));
        }
        let key = self.canvas_key(&[], (size.x.to_bits(), size.y.to_bits(), 1u8));
        let ops = self.painted_canvas(key, paint == Paint::Now, |app| {
            capture(ctx, size, |ui| app.paint_print_network(ui, &net))
        });
        Ok(canvas(ops))
    }

    /// The Activity Network at zoom 1: band titles, relationships, boxes.
    fn paint_print_network(&self, ui: &mut egui::Ui, net: &panes::network::NetworkLayout) {
        use panes::boxes;
        let p = ui.painter().clone();
        p.rect_filled(
            Rect::from_min_size(Pos2::ZERO, net.size),
            0.,
            Color32::WHITE,
        );
        let style = self.grid.layout.group_sort.band_style(0);
        let [r, g, b] = style.background;
        let band_fill = Color32::from_rgb(r, g, b);
        let [r, g, b] = style.text;
        let band_text = Color32::from_rgb(r, g, b);
        for band in &net.bands {
            let title = Rect::from_min_max(
                Pos2::new(0., band.top),
                Pos2::new(net.size.x, band.top + panes::network::BAND_TITLE),
            );
            p.rect_filled(title, 0., band_fill);
            p.hline(
                0.0..=net.size.x,
                band.bottom,
                Stroke::new(1_f32, Color32::from_gray(0xC0)),
            );
            p.with_clip_rect(title).text(
                Pos2::new(6., title.center().y),
                Align2::LEFT_CENTER,
                &band.label,
                chrome::bold_font(),
                band_text,
            );
        }
        let index: BTreeMap<usize, Rect> = net.boxes.iter().map(|b| (b.activity, b.rect)).collect();
        for link in &self.project.model.links {
            let (Some(&a), Some(&b)) = (index.get(&link.pred), index.get(&link.succ)) else {
                continue;
            };
            let (points, dir) = panes::network::link_path(a, b, link.link_type, 1.);
            let critical = self.box_critical(link.pred) && self.box_critical(link.succ);
            let color = if critical { CRITICAL } else { Color32::BLACK };
            boxes::relationship(&p, &points, dir, color, 1.);
        }
        let options = self.panes.network.options;
        for b in &net.boxes {
            let i = b.activity;
            let colors = options.colors(self.box_critical(i));
            let progress = if options.show_progress {
                self.box_progress(i)
            } else {
                boxes::Progress::None
            };
            boxes::paint_box_font(
                &p,
                b.rect.min,
                1.,
                &self.box_text(i),
                colors,
                options.template,
                false,
                progress,
                options.font_size,
            );
        }
    }

    /// One header or footer band on a page.
    fn paint_band(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        band: &Band,
        (now, page, pages, scale): (chrono::DateTime<chrono::Local>, usize, usize, f32),
        network: bool,
    ) {
        let p = painter.with_clip_rect(rect.expand(1.));
        p.rect_stroke(rect, 0., Stroke::new(1_f32, INK), egui::StrokeKind::Inside);
        let edges = band.edges();
        for (k, section) in band.sections.iter().enumerate() {
            let x0 = rect.left() + edges[k] * rect.width();
            let x1 = rect.left() + edges[k + 1] * rect.width();
            if band.dividers && k > 0 {
                chrome::vline(&p, x0.round(), rect.top(), rect.bottom(), INK);
            }
            let cell = Rect::from_min_max(Pos2::new(x0, rect.top()), Pos2::new(x1, rect.bottom()))
                .shrink2(Vec2::new(4., 3.));
            let cp = painter.with_clip_rect(cell);
            match section.content {
                Content::None => {}
                Content::Text => {
                    self.paint_section_text(&cp, cell, section, (now, page, pages, scale))
                }
                Content::Legend if self.print_view().is_some() => {}
                Content::Legend => {
                    if network {
                        self.paint_network_legend(&cp, cell);
                    } else {
                        self.paint_gantt_legend(&cp, cell);
                    }
                }
                Content::Revisions => paint_revisions(&cp, cell, section),
            }
        }
    }

    fn paint_section_text(
        &self,
        p: &egui::Painter,
        cell: Rect,
        section: &Section,
        (now, page, pages, scale): (chrono::DateTime<chrono::Local>, usize, usize, f32),
    ) {
        let text = super::setup::expand(&section.text, |t| {
            self.print_field_at(now, t, (page, pages, scale))
        });
        let font = chrome::table_font();
        let mut y = cell.top();
        for line in text.lines() {
            let galley = p.layout_no_wrap(line.to_string(), font.clone(), INK);
            let w = galley.size().x;
            let x = match section.align {
                TextAlign::Left => cell.left(),
                TextAlign::Center => ((cell.left() + cell.right() - w) / 2.).round(),
                TextAlign::Right => cell.right() - w,
            };
            let h = galley.size().y;
            p.galley(Pos2::new(x, y), galley, INK);
            y += h + 1.;
        }
    }

    /// Displayed bars with a shape, as legend entries in columns. Every
    /// name prints whole: when the columns the entries need do not fit the
    /// section, the names shrink (down to a floor), and only below that are
    /// they cut short with "...".
    fn paint_gantt_legend(&self, p: &egui::Painter, cell: Rect) {
        let shown: Vec<_> = self
            .gantt
            .bars
            .iter()
            .filter(|b| {
                b.display
                    && (b.middle.shape != gantt::settings::BodyShape::None
                        || b.start.shape != gantt::settings::EndShape::None)
            })
            .collect();
        if shown.is_empty() {
            return;
        }
        let pitch = 15.;
        // Bar sample, the gap before the name, the gap after it.
        let (sample, after) = (46., 12.);
        let per_column = ((cell.height() / pitch).floor() as usize).max(1);
        let columns = shown.len().div_ceil(per_column);
        let base = chrome::table_font();
        let widest_in = |font: &egui::FontId| {
            shown
                .iter()
                .map(|d| p.layout_no_wrap(d.name.clone(), font.clone(), INK).size().x)
                .fold(0., f32::max)
        };
        let widest = widest_in(&base);
        let room = (cell.width() / columns as f32 - sample - after).max(0.);
        let font = if widest <= room || widest <= 0. {
            base
        } else {
            let size = (base.size * room / widest).max(LEGEND_MIN_FONT);
            egui::FontId::new(size, base.family.clone())
        };
        let name_w = widest_in(&font).min(room.max(0.));
        let column_w = sample + name_w + after;
        for (k, def) in shown.iter().enumerate() {
            let col = k / per_column;
            let row = k % per_column;
            let left = cell.left() + col as f32 * column_w;
            let top = cell.top() + row as f32 * pitch - 2.;
            if left + sample > cell.right() {
                break;
            }
            let draw = gantt::bars::layout_bar(def, left, top, left + 4., left + 34., &[]);
            gantt::bars::paint(p, &draw);
            let space = (cell.right() - (left + sample)).min(name_w + after);
            let name = fit_text(p, &def.name, &font, space);
            p.text(
                Pos2::new(left + sample, top + 7.5),
                Align2::LEFT_CENTER,
                name,
                font.clone(),
                INK,
            );
        }
    }

    fn paint_network_legend(&self, p: &egui::Painter, cell: Rect) {
        use panes::boxes;
        for (k, (colors, label)) in [
            (boxes::CRITICAL_BOX, "Critical activity"),
            (boxes::NORMAL, "Non-critical activity"),
        ]
        .into_iter()
        .enumerate()
        {
            let top = cell.top() + k as f32 * 15.;
            let sample = Rect::from_min_size(Pos2::new(cell.left(), top + 2.), Vec2::new(24., 10.));
            p.rect_filled(sample, 0., colors.fill);
            p.rect_stroke(
                sample,
                0.,
                Stroke::new(1_f32, colors.dark),
                egui::StrokeKind::Inside,
            );
            p.text(
                Pos2::new(cell.left() + 32., top + 7.),
                Align2::LEFT_CENTER,
                label,
                chrome::table_font(),
                INK,
            );
        }
    }
}

/// The smallest font a footer legend name shrinks to before it is cut.
const LEGEND_MIN_FONT: f32 = 8.;

/// `text` as it fits `room` pixels in `font`: whole, or cut with "...".
fn fit_text(p: &egui::Painter, text: &str, font: &egui::FontId, room: f32) -> String {
    let width = |t: &str| p.layout_no_wrap(t.to_string(), font.clone(), INK).size().x;
    if width(text) <= room {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    for n in (0..chars.len()).rev() {
        let cut: String = chars[..n].iter().collect::<String>() + "...";
        if width(&cut) <= room {
            return cut;
        }
    }
    String::new()
}

/// The revision box: title, column captions and one row per revision.
fn paint_revisions(p: &egui::Painter, cell: Rect, section: &Section) {
    let font = chrome::table_font();
    let pitch = 13.;
    p.text(
        Pos2::new(cell.center().x, cell.top()),
        Align2::CENTER_TOP,
        &section.revision_title,
        chrome::bold_font(),
        INK,
    );
    let fractions = [0., 0.25, 0.6, 0.8, 1.];
    let captions = ["Date", "Revision", "Checked", "Approved"];
    let top = cell.top() + pitch + 1.;
    let rows = 1 + section.revisions.len();
    let bottom = (top + rows as f32 * pitch).min(cell.bottom());
    let x = |f: f32| (cell.left() + f * cell.width()).round();
    for f in fractions {
        chrome::vline(p, x(f).min(cell.right() - 1.), top, bottom, INK);
    }
    for k in 0..=rows {
        let y = (top + k as f32 * pitch).min(bottom - 1.);
        chrome::hline(p, cell.left(), cell.right(), y, INK);
    }
    let mut texts = vec![captions.map(String::from)];
    for r in &section.revisions {
        texts.push([
            r.date.clone(),
            r.revision.clone(),
            r.checked.clone(),
            r.approved.clone(),
        ]);
    }
    for (k, row) in texts.iter().enumerate() {
        let y = top + k as f32 * pitch + 1.;
        if y + pitch > cell.bottom() + 1. {
            break;
        }
        for (c, text) in row.iter().enumerate() {
            let left = x(fractions[c]) + 3.;
            let right = x(fractions[c + 1]) - 2.;
            p.with_clip_rect(Rect::from_min_max(
                Pos2::new(left, y),
                Pos2::new(right, y + pitch),
            ))
            .text(
                Pos2::new(left, y),
                Align2::LEFT_TOP,
                text,
                font.clone(),
                INK,
            );
        }
    }
}
