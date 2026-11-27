//! A dense tree table shared by the WBS, Projects, Resources and Resource
//! Assignments views: fixed header (35 px, wrapped titles), 18 px rows, band
//! rows in the activity table's look by level (`grid::band`: colors, caption
//! font and height per level, totals in the row font), full-row selection
//! with a focus cell,
//! +/- expanders, an indented tree column, header drag to reorder and edge
//! drag to resize. An optional right-hand pane (a Gantt chart or a
//! time-phased spreadsheet) shares the header height and vertical scroll.
use super::super::chrome::{self, Icon, CHROME, INK, SELECT};
use crate::context_menu::{ContextMenu, Entry};
use crate::grid::band;
use crate::grid::layout::BandStyle;
use crate::hscroll::BAR_HEIGHT;
use crate::timeline;
use crate::timescale::TimeScale;
use eframe::egui::{self, Align2, Color32, Pos2, Rect, Sense, Stroke, Vec2};
use std::collections::{BTreeMap, BTreeSet};

pub const HEADER_HEIGHT: f32 = 35.;
/// Single-line header used by the Resources view.
pub const SHORT_HEADER: f32 = 18.;
pub const ROW_HEIGHT: f32 = 18.;
/// A top level band row: 23 px plus its bottom line (12 pt caption).
pub const BAND_HEIGHT: f32 = 24.;
pub const MIN_WIDTH: f32 = 30.;
pub const INDENT: f32 = 14.;
/// Width of the strip a band draws down the left of its child rows.
pub const STRIP: f32 = 13.;
pub const SPLITTER: f32 = 6.;
/// The top band level's fill and text, for panes that draw a band row
/// beside the table (the assignments spreadsheet).
pub const BAND: Color32 = crate::theme::color(crate::theme::BANDS[0].0);
pub const BAND_TEXT: Color32 = crate::theme::color(crate::theme::BANDS[0].1);
pub const LINE: Color32 = Color32::from_rgb(192, 192, 192);
pub const FOCUS: Color32 = crate::theme::color(crate::theme::AMBER);
pub const SHADOW: Color32 = Color32::from_rgb(160, 160, 160);

/// How tall a table's band rows are.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BandPitch {
    /// Each level as tall as its band style needs, as in the activity
    /// table: 24, 22, then 18 px.
    Styled,
    /// Every band this pitch, for a list whose rows stay in step with
    /// another pane; a caption font too large for it is cut to 9 pt.
    Fixed(f32),
}

/// The style a band row at `level` is drawn in (`grid::band`).
pub fn band_style(level: usize, pitch: BandPitch) -> BandStyle {
    let mut style = band::level_style(level);
    if let BandPitch::Fixed(height) = pitch {
        if band::pitch(&style) > height {
            style.font_size = style.font_size.min(9.);
        }
        style.row_height = height;
    }
    style
}

/// Column order and pixel widths for one view, saved with the view's layout.
#[derive(Clone, Debug, PartialEq)]
pub struct Columns {
    pub order: Vec<usize>,
    pub widths: Vec<f32>,
    drag: Option<usize>,
    /// How far the columns are scrolled left when they are wider than the
    /// table (not saved).
    pub scroll_x: f32,
    /// The field of the focus cell: the cell of the current row clicked
    /// last, which a table's cell commands work on (not saved; `None`: the
    /// first column).
    pub focus: Option<usize>,
    /// Ids of the fields after the view's own (its User Defined columns,
    /// `views::udf_columns`): field `own + k` is `extra[k]`, where `own` is
    /// the number of the view's own fields.
    pub extra: Vec<String>,
}

impl Columns {
    pub fn new(widths: &[f32]) -> Self {
        Self {
            order: (0..widths.len()).collect(),
            widths: widths.to_vec(),
            drag: None,
            scroll_x: 0.,
            focus: None,
            extra: vec![],
        }
    }
    pub fn json(&self) -> serde_json::Value {
        serde_json::json!({"order": self.order, "widths": self.widths, "extra": self.extra})
    }
    /// The number of the view's own fields.
    pub fn own(&self) -> usize {
        self.widths.len() - self.extra.len()
    }
    /// The id of field `field` when it is one of the extra fields.
    pub fn extra_of(&self, field: usize) -> Option<&str> {
        field
            .checked_sub(self.own())
            .and_then(|k| self.extra.get(k))
            .map(String::as_str)
    }
    /// Show the view's own fields `own` (indexes) and the extra fields
    /// `extra` (ids), in the order `shown` lists them (`Ok`: an own field,
    /// `Err`: an extra field's id). An extra field shown before keeps its
    /// width; a new one is `width` wide.
    pub fn show_fields(&mut self, shown: &[Result<usize, String>], width: f32) {
        let own = self.own();
        let mut widths: Vec<f32> = self.widths[..own].to_vec();
        let mut extra: Vec<String> = vec![];
        let mut order = vec![];
        for item in shown {
            match item {
                Ok(field) if *field < own => order.push(*field),
                Ok(_) => {}
                Err(id) => {
                    let at = extra.iter().position(|e| e == id).unwrap_or_else(|| {
                        let kept = self.extra.iter().position(|e| e == id);
                        widths.push(kept.map_or(width, |k| self.widths[own + k]));
                        extra.push(id.clone());
                        extra.len() - 1
                    });
                    order.push(own + at);
                }
            }
        }
        order.dedup();
        if order.is_empty() {
            return;
        }
        self.widths = widths;
        self.extra = extra;
        self.order = order;
        self.focus = self.focus.filter(|f| *f < self.widths.len());
    }
    /// Restore saved values only when they still describe this view's columns.
    pub fn restore(&mut self, value: &serde_json::Value) {
        // Extra fields first: the saved order and widths count them.
        if let Ok(extra) = serde_json::from_value::<Vec<String>>(value["extra"].clone()) {
            let own = self.own();
            let mut seen = BTreeSet::new();
            if extra.len() <= 200 && extra.iter().all(|e| e.len() < 100 && seen.insert(e)) {
                self.widths.truncate(own);
                self.widths.extend(vec![100_f32; extra.len()]);
                self.extra = extra;
            }
        }
        // The columns shown, in order: each one once, at least one. A view
        // may hide columns (Spreadsheet Fields).
        if let Ok(order) = serde_json::from_value::<Vec<usize>>(value["order"].clone()) {
            let mut sorted = order.clone();
            sorted.sort_unstable();
            sorted.dedup();
            if !order.is_empty()
                && sorted.len() == order.len()
                && sorted.iter().all(|c| *c < self.widths.len())
            {
                self.order = order;
            }
        }
        if let Ok(widths) = serde_json::from_value::<Vec<f32>>(value["widths"].clone()) {
            // Columns added to the view since were stored without a width:
            // they keep their default, the stored ones read as stored.
            if !widths.is_empty()
                && widths.len() <= self.widths.len()
                && widths
                    .iter()
                    .all(|w| w.is_finite() && (MIN_WIDTH..=2000.).contains(w))
            {
                self.widths[..widths.len()].copy_from_slice(&widths);
            }
        }
    }
    /// A column header dragged from slot `from` and dropped on slot `to`:
    /// the column moves there and the ones between shift over by one.
    pub fn move_slot(&mut self, from: usize, to: usize) {
        if from < self.order.len() && to < self.order.len() && from != to {
            let field = self.order.remove(from);
            self.order.insert(to, field);
        }
    }

    /// Left edge of each displayed slot, plus the right edge of the last one.
    pub fn edges(&self) -> Vec<f32> {
        let mut edges = vec![0.];
        for &field in &self.order {
            edges.push(edges.last().unwrap() + self.widths[field]);
        }
        edges
    }
}

#[derive(Clone, Default)]
pub struct Row {
    pub depth: usize,
    /// `Some(expanded)` draws a +/- box; `None` draws no expander.
    pub expander: Option<bool>,
    pub icon: Option<Icon>,
    pub cells: Vec<String>,
    pub strong: bool,
    /// A band row (EPS node, resource group); its level is `depth`.
    pub band: bool,
    /// Number of enclosing band rows: each draws its strip down this row.
    pub strips: usize,
    /// A band whose tree-column text runs across all columns.
    pub span: bool,
    /// Greyed text: a row listed only as the parent of a match.
    pub dim: bool,
}

impl Row {
    pub fn height(&self, pitch: BandPitch) -> f32 {
        match (self.band, pitch) {
            (false, _) => ROW_HEIGHT,
            (true, BandPitch::Fixed(height)) => height,
            (true, BandPitch::Styled) => band::pitch(&band_style(self.depth, pitch)),
        }
    }
}

/// Static description of one view table.
pub struct Table<'a> {
    pub id: &'a str,
    pub titles: &'a [&'a str],
    /// Fields whose text (and title) is right aligned.
    pub right: &'a [usize],
    pub tree_field: usize,
    pub header_height: f32,
    /// Horizontal `#C0C0C0` line under every row (Resource Assignments).
    pub row_lines: bool,
    /// Pitch of band rows: by level, or fixed for a list kept in step
    /// with another pane.
    pub band_pitch: BandPitch,
    /// Fields a band row writes in its caption font (the tree field always
    /// does); its other cells are totals in the row font.
    pub band_caption: &'a [usize],
    /// The row's context menu, built fresh each time it opens (`None`: no
    /// menu). The key of the item clicked comes back in `Response::menu`.
    pub menu: Option<&'a dyn Fn(usize) -> Vec<Entry>>,
    /// Shift+F10 or the Menu key asked for the menu of the selected row.
    pub open_menu: bool,
    /// The chart pane's menu: one menu for a bar, the empty chart area and
    /// the timescale, as in the reference (`None`: no menu there). The key
    /// of the item clicked comes back in `Response::chart_menu`.
    pub chart_menu: Option<&'a dyn Fn() -> Vec<Entry>>,
}

/// Width of the line number column ([`Extras::line_numbers`]).
pub const NUMBER_W: f32 = 30.;

/// How a cell is edited in place.
#[derive(Clone, Debug, PartialEq)]
pub enum EditKind {
    /// Typed text; the caller reads numbers and dates back from it.
    Text,
    /// One of these values, picked from a list under the cell (an empty
    /// one clears the cell).
    Choice(Vec<String>),
}

/// The cell editor open in a table: its row and field, the text it holds
/// and how it edits.
#[derive(Clone, Debug, PartialEq)]
pub struct CellEdit {
    pub row: usize,
    pub field: usize,
    pub text: String,
    pub kind: EditKind,
    /// Opened this frame: the field takes the keyboard (its text selected)
    /// and the opening click does not close a list.
    fresh: bool,
}

impl CellEdit {
    pub fn new(row: usize, field: usize, text: &str, kind: EditKind) -> Self {
        Self {
            row,
            field,
            text: text.to_string(),
            kind,
            fresh: true,
        }
    }
}

/// In-place editing of a table's cells ([`Extras::editing`]): the editor
/// a cell takes, and the editor open now. A click on the focus cell of the
/// current row (so a double click too) or F2 opens it; Enter or a click
/// elsewhere commits a typed value, a pick commits a choice, and Escape
/// closes it unchanged. The value comes back in `Response::committed`.
pub struct Editing<'a> {
    /// The editor of the cell at (row, field) (`None`: not edited here).
    pub kind: &'a dyn Fn(usize, usize) -> Option<EditKind>,
    pub open: &'a mut Option<CellEdit>,
}

/// A view table's own font, text colour, cell fill and row height, from
/// its Table Font and Row settings ([`Extras::look`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Look {
    pub font: egui::FontId,
    pub color: Color32,
    pub fill: Color32,
    pub row_height: f32,
}

impl Look {
    /// The look the Table Font and Row settings `t` and row height give.
    pub fn of(t: &crate::grid::layout::TableDisplay, row_height: f32) -> Self {
        let f = &t.font;
        let rgb = |c: [u8; 3]| Color32::from_rgb(c[0], c[1], c[2]);
        Self {
            font: if f.is_default() {
                chrome::table_font()
            } else {
                f.face.font(f.points, f.bold)
            },
            color: rgb(f.color),
            fill: rgb(t.cell_color),
            row_height: row_height.clamp(ROW_HEIGHT - 1., 48.),
        }
    }
}

/// What a table may add to [`show`]; the default adds nothing, so
/// [`show`] is [`show_with`] and `Extras::default()`.
#[derive(Default)]
pub struct Extras<'a> {
    /// A line number column (`NUMBER_W`) left of the columns: row 1, 2, ...
    pub line_numbers: bool,
    /// The sorted field and whether it sorts descending: its title carries
    /// the arrow (at the left of a right aligned title, else at the right).
    pub sorted: Option<(usize, bool)>,
    /// The rows' vertical scroll offset kept by the caller: the rows show
    /// from it, and the wheel or the scroll bar moves it.
    pub scroll: Option<&'a mut f32>,
    /// Fields whose cells hold an indicator's name: drawn with OpenXER's
    /// indicator mark before the name.
    pub indicators: &'a [usize],
    /// Cells that edit in place.
    pub editing: Option<Editing<'a>>,
    /// The table's own font, colours and row height (Table Font and Row);
    /// `None` draws the standard look.
    pub look: Option<&'a Look>,
}

/// A pane drawn to the right of the table, in step with its rows.
pub trait Pane {
    /// The pane's part of the column header.
    fn header(&mut self, ui: &mut egui::Ui, rect: Rect);
    /// Background over the visible body (grid and sight lines), before rows.
    fn body(&mut self, _painter: &egui::Painter, _rect: Rect, _rows: &[(usize, Rect)]) {}
    /// One row of the pane.
    fn row(&mut self, painter: &egui::Painter, rect: Rect, index: usize, selected: bool);
    /// The pane's time axis, when it has one the user scrolls and zooms
    /// like the Gantt Chart (see crate::timeline).
    fn timeline(&mut self) -> Option<PaneTimeline<'_>> {
        None
    }
}

/// A pane's time axis as show scrolls and zooms it: a scroll bar under
/// the pane, the upper header band pans, the lower one zooms, the wheel pans
/// and Ctrl+wheel zooms around the pointer.
pub struct PaneTimeline<'a> {
    pub axis: &'a mut TimeScale,
    pub spec: timeline::Spec,
    /// The axis starts this far right of the pane's left edge.
    pub inset: f32,
    /// Height of the header band a drag pans; the rest of the header zooms.
    pub pan_height: f32,
    /// Where the chart was drawn, filled in by show.
    pub place: &'a mut timeline::Place,
}

#[derive(Debug, Default)]
pub struct Response {
    /// A row clicked (left or right button), with `modifiers` and
    /// `secondary` telling how.
    pub clicked: Option<usize>,
    /// The Ctrl, Shift and Alt keys held for the click.
    pub modifiers: egui::Modifiers,
    /// The field of the cell clicked (the table's focus column follows it).
    pub clicked_field: Option<usize>,
    /// The click was a right-click (the row's menu opens).
    pub secondary: bool,
    pub double_clicked: Option<usize>,
    pub toggled: Option<usize>,
    /// The row of the menu and the key of the item clicked; a row past the
    /// last one is the empty area under the rows.
    pub menu: Option<(usize, &'static str)>,
    /// The key of the chart menu item clicked.
    pub chart_menu: Option<&'static str>,
    /// A column title clicked (the field), for tables that sort on it.
    pub header_clicked: Option<usize>,
    /// An in-place edit finished: row, field and the value typed or picked.
    pub committed: Option<(usize, usize, String)>,
    /// The area under the header where the rows are drawn.
    pub body: Option<Rect>,
    /// The drawn rows' and column titles' responses (row index, field), for
    /// a caller that opens its own menus on them.
    pub rows: Vec<(usize, egui::Response)>,
    pub titles: Vec<(usize, egui::Response)>,
}

/// The sort arrow of a sorted column title, its tip at `tip`: up when
/// ascending, down when descending.
pub fn sort_arrow(p: &egui::Painter, tip: Pos2, descending: bool) {
    let points = if descending {
        vec![
            tip + Vec2::new(-3., -2.),
            tip + Vec2::new(3., -2.),
            tip + Vec2::new(0., 2.),
        ]
    } else {
        vec![
            tip + Vec2::new(-3., 2.),
            tip + Vec2::new(3., 2.),
            tip + Vec2::new(0., -2.),
        ]
    };
    p.add(egui::Shape::convex_polygon(points, INK, Stroke::NONE));
}

/// The line an empty table shows under its header (`response` from
/// [`show`]), recorded as `{id}_empty`.
pub fn empty_note(
    ui: &egui::Ui,
    response: &Response,
    id: &str,
    text: &str,
    actions: &mut BTreeMap<String, [f32; 4]>,
) {
    let Some(body) = response.body else {
        return;
    };
    let painter = ui.painter().with_clip_rect(body.intersect(ui.clip_rect()));
    let at = body.left_top() + Vec2::new(6., 6.);
    let r = painter.text(at, egui::Align2::LEFT_TOP, text, chrome::table_font(), INK);
    record(actions, format!("{id}_empty"), r);
}

fn record(actions: &mut BTreeMap<String, [f32; 4]>, name: String, r: Rect) {
    actions.insert(name, [r.left(), r.top(), r.right(), r.bottom()]);
}

/// Left edge of the tree column's expander for a row at `depth`.
fn expander_x(cell_left: f32, depth: usize) -> f32 {
    cell_left + 2. + depth as f32 * INDENT
}

/// Draws the 9 x 9 boxed plus or minus.
pub fn expander(painter: &egui::Painter, b: Rect, expanded: bool) {
    painter.rect_filled(b, 0., Color32::WHITE);
    painter.rect_stroke(b, 0., Stroke::new(1_f32, SHADOW), egui::StrokeKind::Inside);
    let pen = Stroke::new(1_f32, INK);
    let y = b.top() + 4.5;
    painter.line_segment(
        [Pos2::new(b.left() + 2., y), Pos2::new(b.right() - 2., y)],
        pen,
    );
    if !expanded {
        let x = b.left() + 4.5;
        painter.line_segment(
            [Pos2::new(x, b.top() + 2.), Pos2::new(x, b.bottom() - 2.)],
            pen,
        );
    }
}

/// The 6 px raised splitter between the table and its pane.
pub fn splitter(painter: &egui::Painter, rect: Rect) {
    painter.rect_filled(rect, 0., CHROME);
    for (dx, color) in [(0., SHADOW), (1., Color32::WHITE), (4., SHADOW)] {
        painter.rect_filled(
            Rect::from_min_size(
                Pos2::new(rect.left() + dx, rect.top()),
                Vec2::new(1., rect.height()),
            ),
            0.,
            color,
        );
    }
}

/// Header cell text: wrapped to the column, top aligned (glyphs 4 px
/// below the header top, as measured), 3 px in.
fn header_title(painter: &egui::Painter, rect: Rect, text: &str, right: bool) {
    let width = (rect.width() - 6.).max(1.);
    let galley = painter.layout(text.to_string(), chrome::table_font(), INK, width);
    let x = if right {
        rect.right() - 3. - galley.size().x
    } else {
        rect.left() + 3.
    };
    painter
        .with_clip_rect(rect.intersect(painter.clip_rect()))
        .galley(Pos2::new(x, rect.top() + 1.), galley, INK);
}

/// Draws the table (and pane) into the remaining space of `ui`. Context-menu
/// entries are returned in `Response::menu` instead of running callbacks.
/// `pane` carries the pane and the table width (the splitter position),
/// which the user can drag.
#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut egui::Ui,
    table: &Table,
    columns: &mut Columns,
    rows: &[Row],
    selection: &Selection,
    scroll_to: Option<usize>,
    pane: Option<(&mut dyn Pane, &mut f32)>,
    actions: &mut BTreeMap<String, [f32; 4]>,
) -> Response {
    show_with(
        ui,
        table,
        columns,
        rows,
        selection,
        scroll_to,
        pane,
        actions,
        Extras::default(),
    )
}

/// [`show`] with the [`Extras`] a table asks for.
#[allow(clippy::too_many_arguments)]
pub fn show_with(
    ui: &mut egui::Ui,
    table: &Table,
    columns: &mut Columns,
    rows: &[Row],
    selection: &Selection,
    scroll_to: Option<usize>,
    mut pane: Option<(&mut dyn Pane, &mut f32)>,
    actions: &mut BTreeMap<String, [f32; 4]>,
    extras: Extras,
) -> Response {
    let Extras {
        line_numbers,
        sorted,
        scroll: kept_scroll,
        indicators,
        mut editing,
        look,
    } = extras;
    // Row pitch: a plain row takes the table's own row height.
    let pitch = |row: &Row| match look {
        Some(l) if !row.band => l.row_height,
        _ => row.height(table.band_pitch),
    };
    let cell_fill = look.map_or(Color32::WHITE, |l| l.fill);
    let id = table.id;
    let mut out = Response::default();
    let full = ui.available_width();
    let table_w = match &pane {
        Some((_, w)) => w.clamp(150., (full - 120.).max(150.)),
        None => full,
    };
    // The line number column comes first: the columns start after it.
    let lead = if line_numbers { NUMBER_W } else { 0. };
    let edges: Vec<f32> = columns.edges().into_iter().map(|e| e + lead).collect();
    let (head_all, _) =
        ui.allocate_exact_size(Vec2::new(full, table.header_height), Sense::hover());
    let head = Rect::from_min_size(head_all.min, Vec2::new(table_w, head_all.height()));
    // Columns wider than the table scroll sideways: a bar under the table
    // (in the pane's strip when it has one) and Shift+wheel over the rows.
    let columns_right = edges.last().copied().unwrap_or(0.);
    let hrange = |pos: f32| crate::hscroll::Range {
        total: f64::from(columns_right + 2.),
        view: f64::from(table_w),
        pos: f64::from(pos),
    };
    let wide = hrange(0.).scrollable();
    let max_x = (columns_right + 2. - table_w).max(0.);
    let hbar = Rect::from_min_max(
        Pos2::new(
            head_all.left(),
            (ui.max_rect().bottom() - BAR_HEIGHT).round(),
        ),
        Pos2::new(
            head.right(),
            (ui.max_rect().bottom() - BAR_HEIGHT).round() + BAR_HEIGHT,
        ),
    );
    let hbar_id = ui.id().with((id, "columns_hscroll"));
    if wide {
        let wheel = ui.input(|i| i.smooth_scroll_delta.x);
        let body = Rect::from_min_max(Pos2::new(head_all.left(), head_all.bottom()), hbar.max);
        if wheel != 0. && ui.rect_contains_pointer(body) {
            columns.scroll_x -= wheel;
        }
        match crate::hscroll::interact(ui, hbar_id, hbar, hrange(columns.scroll_x)) {
            Some(crate::hscroll::Move::To(x)) => columns.scroll_x = x as f32,
            Some(crate::hscroll::Move::Line(dir)) => columns.scroll_x += 40. * dir as f32,
            Some(crate::hscroll::Move::Page(dir)) => columns.scroll_x += table_w * dir as f32,
            None => {}
        }
        columns.scroll_x = columns.scroll_x.clamp(0., max_x).round();
    } else {
        columns.scroll_x = 0.;
    }
    let dx = -columns.scroll_x;
    let painter = ui.painter().with_clip_rect(head.intersect(ui.clip_rect()));
    painter.rect_filled(head, 0., CHROME);
    painter.hline(
        head.x_range(),
        head.top() + 0.5,
        Stroke::new(1_f32, Color32::WHITE),
    );
    if lead > 0. {
        // The line number column's title cell: blank, with its dividers.
        let x = head.left() + dx + lead;
        painter.vline(x - 0.5, head.y_range(), Stroke::new(1_f32, INK));
        painter.vline(x + 0.5, head.y_range(), Stroke::new(1_f32, Color32::WHITE));
    }
    for (slot, &field) in columns.order.clone().iter().enumerate() {
        let rect = Rect::from_min_max(
            Pos2::new(head.left() + dx + edges[slot], head.top()),
            Pos2::new(head.left() + dx + edges[slot + 1], head.bottom()),
        );
        let right = table.right.contains(&field);
        header_title(
            &painter,
            Rect::from_min_max(rect.min, Pos2::new(rect.right() - 1., rect.bottom())),
            table.titles[field],
            right,
        );
        if let Some((_, descending)) = sorted.filter(|s| s.0 == field) {
            // At the left of a right aligned (number) title, else at the
            // right; 9 px above the bottom (centered in a short header).
            let x = if right {
                rect.left() + 8.
            } else {
                rect.right() - 9.
            };
            sort_arrow(
                &painter.with_clip_rect(rect.intersect(painter.clip_rect())),
                Pos2::new(x, rect.bottom() - 9.),
                descending,
            );
        }
        painter.vline(rect.right() - 0.5, rect.y_range(), Stroke::new(1_f32, INK));
        painter.vline(
            rect.right() + 0.5,
            rect.y_range(),
            Stroke::new(1_f32, Color32::WHITE),
        );
        let visible = rect.intersect(head);
        if !visible.is_positive() {
            continue;
        }
        let header = ui.interact(
            visible.shrink2(Vec2::new(4., 0.)),
            ui.id().with((id, "column", field)),
            Sense::click_and_drag(),
        );
        record(actions, format!("{id}_col_{field}"), visible);
        if header.clicked() {
            out.header_clicked = Some(field);
        }
        out.titles.push((field, header.clone()));
        if let Some(menu) = table.menu {
            // A column title has no menu of its own: it opens the table's
            // menu for the current row (the empty area's without one), and
            // changes neither the sort nor the selection.
            let row = selection.current.unwrap_or(rows.len());
            ContextMenu::new(&format!("{id}_menu"))
                .min_width(120.)
                .show(&header, &mut out, |out, m| {
                    if let Some(key) = m.entries(&menu(row)) {
                        out.menu = Some((row, key));
                    }
                });
        }
        if header.drag_started() {
            columns.drag = Some(slot);
        }
        if header.drag_stopped() {
            if let (Some(from), Some(pos)) = (
                columns.drag.take(),
                ui.ctx().input(|i| i.pointer.interact_pos()),
            ) {
                if let Some(to) = (0..columns.order.len()).find(|&n| {
                    pos.x >= head.left() + dx + edges[n] && pos.x < head.left() + dx + edges[n + 1]
                }) {
                    columns.move_slot(from, to);
                }
            }
        }
        let edge = Rect::from_center_size(rect.right_center(), Vec2::new(7., rect.height()));
        let resize = ui
            .interact(
                edge,
                ui.id().with((id, "resize", field)),
                Sense::click_and_drag(),
            )
            .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
        record(actions, format!("{id}_resize_{field}"), edge);
        if resize.dragged() {
            if let Some(pos) = ui.ctx().input(|i| i.pointer.interact_pos()) {
                columns.widths[field] = (pos.x - rect.left()).clamp(MIN_WIDTH, 2000.);
            }
        }
    }
    painter.hline(head.x_range(), head.bottom() - 0.5, Stroke::new(1_f32, INK));
    // The pane header and the splitter.
    let pane_left = head.right() + SPLITTER;
    let body_top = head_all.bottom();
    let body_bottom = ui.max_rect().bottom();
    // A pane with a time axis gets the Gantt's scroll bar strip, its
    // header drags and its wheel.
    let mut chart = None;
    // Where the chart menu opens: the timescale bands and the chart body.
    let mut chart_spots: Vec<egui::Response> = vec![];
    if let Some(t) = pane.as_mut().and_then(|(p, _)| p.timeline()) {
        // The pane paints its axis from here (not rounded, so a zoom keeps
        // the date under the pointer exactly).
        let left = pane_left + t.inset;
        let right = head_all.right();
        let strip_top = (body_bottom - BAR_HEIGHT).round();
        let c = timeline::Chart {
            pan: Rect::from_min_max(
                Pos2::new(left, head.top()),
                Pos2::new(right, head.top() + t.pan_height),
            ),
            zoom: Rect::from_min_max(
                Pos2::new(left, head.top() + t.pan_height),
                Pos2::new(right, head_all.bottom()),
            ),
            body: Rect::from_min_max(Pos2::new(left, body_top), Pos2::new(right, strip_top)),
            bar: Some(Rect::from_min_max(
                Pos2::new(left, strip_top),
                Pos2::new((right - BAR_HEIGHT).max(left), strip_top + BAR_HEIGHT),
            )),
            left,
            width: (right - left).max(0.),
        };
        let span = timeline::Span::new(t.axis.first_minute, t.axis.pixels_per_week);
        let outcome = timeline::control(ui, ui.id().with((id, "timeline")), &c, span, &t.spec);
        t.axis.first_minute = outcome.span.first;
        t.axis.pixels_per_week = outcome.span.pixels_per_week;
        *t.place = timeline::Place {
            left,
            width: c.width,
            extent: Some(t.spec.extent),
        };
        record(actions, format!("{id}_timescale_pan"), c.pan);
        record(actions, format!("{id}_timescale_zoom"), c.zoom);
        record(actions, format!("{id}_chart"), c.body);
        if table.chart_menu.is_some() {
            // The body under the rows; the rows, made later, sit above it
            // and route a click on their chart part the same way.
            let body = ui.interact(c.body, ui.id().with((id, "chart_body")), Sense::click());
            chart_spots.extend([outcome.pan, outcome.zoom, body]);
        }
        chart = Some((c, t.spec.extent));
    }
    if let Some(menu) = table.chart_menu {
        for spot in &chart_spots {
            ContextMenu::new(&format!("{id}_chart_menu"))
                .min_width(120.)
                .show(spot, &mut out, |out, m| {
                    if let Some(key) = m.entries(&menu()) {
                        out.chart_menu = Some(key);
                    }
                });
        }
    }
    if let Some((pane, _)) = pane.as_mut() {
        let rect = Rect::from_min_max(Pos2::new(pane_left, head.top()), head_all.max);
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
        child.set_clip_rect(rect.intersect(ui.clip_rect()));
        pane.header(&mut child, rect);
    }
    if let Some((_, width)) = pane.as_mut() {
        let bar = Rect::from_min_max(
            Pos2::new(head.right(), head.top()),
            Pos2::new(pane_left, body_bottom),
        );
        splitter(ui.painter(), bar);
        let drag = ui
            .interact(bar, ui.id().with((id, "splitter")), Sense::drag())
            .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
        record(actions, format!("{id}_splitter"), bar);
        if drag.dragged() {
            **width = (table_w + drag.drag_delta().x).clamp(150., (full - 120.).max(150.));
        }
    }
    // Row geometry.
    let mut tops = Vec::with_capacity(rows.len() + 1);
    let mut y = 0.;
    for row in rows {
        tops.push(y);
        y += pitch(row);
    }
    tops.push(y);
    let total = y;
    let strip = if chart.is_some() || wide {
        BAR_HEIGHT
    } else {
        0.
    };
    let body_bottom = body_bottom - strip;
    let ctrl = ui.input(|i| i.modifiers.ctrl);
    let mut scroll = egui::ScrollArea::vertical()
        .id_salt((id, "rows"))
        .auto_shrink([false, false])
        .max_height((body_bottom - body_top).max(0.))
        // Ctrl+wheel zooms the pane's timescale instead. No drag-to-scroll,
        // as in the activity table: a press would grab the rows and scroll
        // them by the pointer's travel that frame.
        .scroll_source(egui::scroll_area::ScrollSource {
            mouse_wheel: !(ctrl && chart.is_some()),
            drag: false,
            ..Default::default()
        });
    if let Some(index) = scroll_to.filter(|i| *i < rows.len()) {
        scroll = scroll.vertical_scroll_offset(tops[index]);
    } else if let Some(offset) = kept_scroll.as_deref() {
        scroll = scroll.vertical_scroll_offset(*offset);
    }
    let reveal = selection
        .current
        .filter(|i| selection.reveal && *i < rows.len() && scroll_to.is_none());
    let body_clip = Rect::from_min_max(
        Pos2::new(head_all.left(), body_top),
        Pos2::new(head_all.right(), body_bottom),
    );
    out.body = Some(Rect::from_min_max(
        body_clip.min,
        Pos2::new(head.right(), body_bottom),
    ));
    let table_body = Rect::from_min_max(body_clip.min, Pos2::new(head.right(), body_bottom));
    record(actions, format!("{id}_body"), table_body);
    ui.painter().rect_filled(table_body, 0., cell_fill);
    let chart_menu_on = table.chart_menu.is_some() && chart.is_some();
    let menu_side = egui::Id::new((id, "menu_in_chart"));
    // The column slot of the focus cell.
    let focus_slot = columns
        .focus
        .and_then(|f| columns.order.iter().position(|&x| x == f))
        .unwrap_or(0);
    let scrolled = scroll.show_viewport(ui, |ui, viewport| {
        ui.set_min_size(Vec2::new(full, total));
        let origin = ui.max_rect().min;
        if let Some(index) = reveal {
            // The keyboard moved the current row: scroll just enough to show it.
            let rect = Rect::from_min_size(
                origin + Vec2::new(0., tops[index]),
                Vec2::new(1., pitch(&rows[index])),
            );
            if !viewport.contains_rect(rect.translate(-origin.to_vec2())) {
                ui.scroll_to_rect(rect, None);
            }
        }
        let first = tops
            .partition_point(|t| *t + 30. < viewport.top())
            .saturating_sub(1);
        let mut visible = vec![];
        for index in first..rows.len() {
            if tops[index] > viewport.bottom() {
                break;
            }
            let rect = Rect::from_min_size(
                origin + Vec2::new(0., tops[index]),
                Vec2::new(full, pitch(&rows[index])),
            );
            visible.push((index, rect));
        }
        let clip = body_clip.intersect(ui.clip_rect());
        if let Some((pane, _)) = pane.as_mut() {
            let rect = Rect::from_min_max(Pos2::new(pane_left, clip.top()), clip.max);
            let pane_rows: Vec<(usize, Rect)> = visible
                .iter()
                .map(|(i, r)| (*i, Rect::from_min_max(Pos2::new(pane_left, r.top()), r.max)))
                .collect();
            pane.body(&ui.painter().with_clip_rect(rect), rect, &pane_rows);
        }
        // The area under the rows takes the menu of "no row" (index
        // `rows.len()`); the rows are created after it, so they sit above.
        let empty = table.menu.map(|_| {
            let area = Rect::from_min_max(
                Pos2::new(origin.x, body_top),
                Pos2::new(origin.x + table_w, body_bottom),
            );
            ui.interact(
                area.intersect(clip),
                ui.id().with((id, "empty")),
                Sense::click(),
            )
        });
        if let (Some(menu), Some(background)) = (table.menu, empty) {
            ContextMenu::new(&format!("{id}_menu"))
                .min_width(120.)
                .show(&background, &mut out, |out, m| {
                    if let Some(key) = m.entries(&menu(rows.len())) {
                        out.menu = Some((rows.len(), key));
                    }
                });
        }
        for (index, rect) in visible {
            let row = &rows[index];
            let response = ui.interact(rect, ui.id().with((id, "row", index)), Sense::click());
            let table_rect =
                Rect::from_min_max(rect.min, Pos2::new(rect.left() + table_w, rect.bottom()));
            record(
                actions,
                format!("{id}_row_{index}"),
                table_rect.intersect(clip),
            );
            let painter = ui.painter().with_clip_rect(table_rect.intersect(clip));
            let is_selected = selection.contains(index);
            let is_current = selection.current == Some(index);
            // The columns as scrolled sideways.
            let x0 = rect.left() + dx;
            let cols = Rect::from_min_max(
                Pos2::new(x0, rect.top()),
                Pos2::new(x0 + columns_right, rect.bottom()),
            );
            let fill_bottom = if row.band || table.row_lines {
                rect.bottom() - 1.
            } else {
                rect.bottom()
            };
            let fill = Rect::from_min_max(cols.min, Pos2::new(cols.right(), fill_bottom));
            painter.rect_filled(table_rect, 0., cell_fill);
            let style = row.band.then(|| band_style(row.depth, table.band_pitch));
            if let Some(style) = &style {
                painter.rect_filled(fill, 0., band::fill(style, false));
            }
            if row.strips > 0 {
                // One strip per enclosing band, each in its level's color.
                let x = x0 + lead;
                for level in 0..row.strips {
                    let (from, to) = if level == 0 {
                        (0., STRIP)
                    } else {
                        (
                            STRIP + (level - 1) as f32 * INDENT,
                            STRIP + level as f32 * INDENT,
                        )
                    };
                    painter.rect_filled(
                        Rect::from_min_max(
                            Pos2::new(x + from, rect.top()),
                            Pos2::new(x + to, rect.bottom()),
                        ),
                        0.,
                        band::fill(&band_style(level, table.band_pitch), false),
                    );
                }
                let w = STRIP + (row.strips - 1) as f32 * INDENT;
                painter.vline(x + w + 0.5, rect.y_range(), Stroke::new(1_f32, LINE));
            }
            if is_selected {
                painter.rect_filled(fill, 0., SELECT);
            }
            if row.band || table.row_lines {
                painter.hline(
                    x0..=cols.right(),
                    rect.bottom() - 0.5,
                    Stroke::new(1_f32, LINE),
                );
            }
            let text_color = if let Some(style) = &style {
                band::ink(style, is_selected)
            } else if is_selected {
                Color32::WHITE
            } else if row.dim {
                chrome::pal::TEXT_DISABLED
            } else {
                look.map_or(INK, |l| l.color)
            };
            if lead > 0. {
                let cell = Rect::from_min_max(
                    Pos2::new(x0, rect.top()),
                    Pos2::new(x0 + lead, fill_bottom),
                );
                painter
                    .with_clip_rect(cell.intersect(painter.clip_rect()))
                    .text(
                        Pos2::new(cell.right() - 3., cell.center().y),
                        Align2::RIGHT_CENTER,
                        (index + 1).to_string(),
                        chrome::table_font(),
                        text_color,
                    );
            }
            let font = if row.band {
                band::total_font()
            } else if row.strong {
                chrome::bold_font()
            } else {
                look.map_or_else(chrome::table_font, |l| l.font.clone())
            };
            let mut expander_rect = None;
            let mut editor_cell = None;
            for (slot, &field) in columns.order.iter().enumerate() {
                let cell = Rect::from_min_max(
                    Pos2::new(x0 + edges[slot], rect.top()),
                    Pos2::new(x0 + edges[slot + 1], fill_bottom),
                );
                let mut x = cell.left() + 3.;
                if field == table.tree_field {
                    let ex = expander_x(cell.left(), row.depth);
                    if let Some(expanded) = row.expander {
                        let b = Rect::from_min_size(
                            Pos2::new(ex, (cell.center().y - 4.5).floor()),
                            Vec2::splat(9.),
                        );
                        expander(&painter, b, expanded);
                        record(actions, format!("{id}_expand_{index}"), b);
                        expander_rect = Some(b.expand(2.));
                    }
                    x = ex + 13.;
                    if let Some(icon) = row.icon {
                        let icon_rect = Rect::from_min_size(
                            Pos2::new(x, (cell.center().y - 8.).floor()),
                            Vec2::splat(16.),
                        );
                        chrome::paint_icon(
                            &painter.with_clip_rect(cell.intersect(painter.clip_rect())),
                            icon,
                            icon_rect,
                            true,
                        );
                        x += 21.;
                    }
                }
                let text = row.cells.get(field).map_or("", String::as_str);
                if row.span && field != table.tree_field {
                    continue;
                }
                let editing_here = editing
                    .as_ref()
                    .and_then(|e| e.open.as_ref())
                    .is_some_and(|o| o.row == index && o.field == field);
                if editing_here {
                    editor_cell = Some(cell);
                    continue;
                }
                if indicators.contains(&field) {
                    // An indicator: OpenXER's mark before its name.
                    if let Some((_, color)) = crate::details::record_tabs::indicator_named(text) {
                        let mark = painter.with_clip_rect(cell.intersect(painter.clip_rect()));
                        let center = Pos2::new(x + 5., cell.center().y);
                        crate::details::record_tabs::paint_indicator(&mark, center, color);
                        x += 14.;
                    }
                }
                let right_edge = if row.span {
                    x0 + columns_right
                } else {
                    cell.right()
                };
                let clip = Rect::from_min_max(
                    Pos2::new(x, cell.top()),
                    Pos2::new(right_edge - 2., cell.bottom()),
                )
                .intersect(painter.clip_rect());
                let right = table.right.contains(&field);
                match &style {
                    // The caption in the level's font, as the activity
                    // table draws it.
                    Some(style)
                        if field == table.tree_field || table.band_caption.contains(&field) =>
                    {
                        band::caption(&painter, clip, x, rect.top(), style, text, is_selected);
                    }
                    // Totals in the row font, top aligned like the
                    // activity table's band totals.
                    Some(_) => {
                        let top = band::total_top(rect.top());
                        let (pos, align) = if right {
                            (Pos2::new(cell.right() - 3., top), Align2::RIGHT_TOP)
                        } else {
                            (Pos2::new(x, top), Align2::LEFT_TOP)
                        };
                        painter.with_clip_rect(clip).text(
                            pos,
                            align,
                            text,
                            font.clone(),
                            text_color,
                        );
                    }
                    None => {
                        let (pos, align) = if right {
                            (
                                Pos2::new(cell.right() - 3., cell.center().y),
                                Align2::RIGHT_CENTER,
                            )
                        } else {
                            (Pos2::new(x, cell.center().y), Align2::LEFT_CENTER)
                        };
                        painter.with_clip_rect(clip).text(
                            pos,
                            align,
                            text,
                            font.clone(),
                            text_color,
                        );
                    }
                }
                if is_current && slot == focus_slot {
                    let focus = Rect::from_min_max(
                        Pos2::new(x.min(cell.right() - 4.) - 2., cell.top()),
                        cell.max,
                    );
                    painter.rect_stroke(
                        focus,
                        0.,
                        Stroke::new(1_f32, FOCUS),
                        egui::StrokeKind::Inside,
                    );
                }
            }
            if let (Some(cell), Some(ed)) = (editor_cell, editing.as_mut()) {
                if let Some(open) = ed.open.as_mut() {
                    let at = (open.row, open.field);
                    match cell_editor(ui, id, cell.intersect(clip), open, actions) {
                        Some(Some(text)) => {
                            out.committed = Some((at.0, at.1, text));
                            *ed.open = None;
                        }
                        Some(None) => *ed.open = None,
                        None => {}
                    }
                }
            }
            if let Some((pane, _)) = pane.as_mut() {
                let pane_rect = Rect::from_min_max(Pos2::new(pane_left, rect.top()), rect.max);
                let clip_rect = pane_rect.intersect(clip);
                pane.row(
                    &ui.painter().with_clip_rect(clip_rect),
                    pane_rect,
                    index,
                    is_selected,
                );
            }
            out.rows.push((index, response.clone()));
            let on_expander = |pos: Option<Pos2>| {
                expander_rect.is_some_and(|b| pos.is_some_and(|p| b.contains(p)))
            };
            let field_at = |pos: Option<Pos2>| {
                let x = pos?.x;
                (0..columns.order.len())
                    .find(|&s| x >= x0 + edges[s] && x < x0 + edges[s + 1])
                    .map(|s| columns.order[s])
            };
            if response.clicked() {
                if on_expander(response.interact_pointer_pos()) {
                    out.toggled = Some(index);
                    // Draw the new rows at once, so the selection leaves
                    // rows the collapse hid before any key can act on them.
                    ui.ctx().request_repaint();
                } else {
                    out.clicked = Some(index);
                    out.modifiers = ui.input(|i| i.modifiers);
                    out.clicked_field = field_at(response.interact_pointer_pos());
                    let focus_before = columns.focus.or(columns.order.first().copied());
                    if out.clicked_field.is_some() {
                        columns.focus = out.clicked_field;
                    }
                    // A click on the focus cell of the current row opens
                    // its editor (so does a double click).
                    if let (Some(ed), Some(field)) = (editing.as_mut(), out.clicked_field) {
                        let again = is_current && focus_before == Some(field);
                        if again && out.modifiers.is_none() && ed.open.is_none() {
                            if let Some(kind) = (ed.kind)(index, field) {
                                let text = row.cells.get(field).map_or("", String::as_str);
                                *ed.open = Some(CellEdit::new(index, field, text, kind));
                            }
                        }
                    }
                }
            }
            if response.double_clicked() && !on_expander(response.interact_pointer_pos()) {
                out.double_clicked = Some(index);
            }
            if let Some(menu) = table.menu {
                if response.secondary_clicked() {
                    out.clicked = Some(index);
                    out.modifiers = ui.input(|i| i.modifiers);
                    out.secondary = true;
                    // A right-click on the row's chart part opens the chart
                    // menu, on its table part the table menu; the choice
                    // holds while the menu is open.
                    let in_chart = chart_menu_on
                        && response
                            .interact_pointer_pos()
                            .is_some_and(|p| p.x >= pane_left);
                    ui.ctx().data_mut(|d| d.insert_temp(menu_side, in_chart));
                }
                // Keyboard: the menu of the current row, just below it.
                let at = (table.open_menu && is_current).then(|| {
                    Pos2::new(
                        (rect.left() + 40.).min(table_rect.right() - 20.),
                        rect.bottom(),
                    )
                });
                if at.is_some() {
                    ui.ctx().data_mut(|d| d.insert_temp(menu_side, false));
                }
                let in_chart = ui.ctx().data(|d| d.get_temp::<bool>(menu_side)) == Some(true);
                match table.chart_menu.filter(|_| in_chart && chart_menu_on) {
                    Some(chart_menu) => {
                        ContextMenu::new(&format!("{id}_chart_menu"))
                            .min_width(120.)
                            .show(&response, &mut out, |out, m| {
                                if let Some(key) = m.entries(&chart_menu()) {
                                    out.chart_menu = Some(key);
                                }
                            });
                    }
                    None => {
                        ContextMenu::new(&format!("{id}_menu"))
                            .min_width(120.)
                            .open_at(at)
                            .show(&response, &mut out, |out, m| {
                                if let Some(key) = m.entries(&menu(index)) {
                                    out.menu = Some((index, key));
                                }
                            });
                    }
                }
            }
        }
    });
    if let Some(offset) = kept_scroll {
        *offset = scrolled.state.offset.y;
    }
    if let Some(ed) = editing.as_mut() {
        // F2 opens the focus cell of the current row, when no text field
        // has the keyboard.
        let free = ui.ctx().memory(|m| m.focused().is_none());
        if ed.open.is_none() && free && ui.input(|i| i.key_pressed(egui::Key::F2)) {
            if let Some(row) = selection.current.filter(|r| *r < rows.len()) {
                let field = columns.focus.or(columns.order.first().copied());
                if let Some(field) = field {
                    if let Some(kind) = (ed.kind)(row, field) {
                        let text = rows[row].cells.get(field).map_or("", String::as_str);
                        *ed.open = Some(CellEdit::new(row, field, text, kind));
                    }
                }
            }
        }
    }
    if let (Some(menu), true) = (table.menu, chart.is_some() && !wide) {
        // The strip under the table is still the area under the rows
        // (unless it holds the columns' scroll bar).
        let under = Rect::from_min_max(
            Pos2::new(head_all.left(), body_bottom),
            Pos2::new(head.right(), body_bottom + BAR_HEIGHT),
        );
        let response = ui.interact(under, ui.id().with((id, "empty_strip")), Sense::click());
        ContextMenu::new(&format!("{id}_menu"))
            .min_width(120.)
            .show(&response, &mut out, |out, m| {
                if let Some(key) = m.entries(&menu(rows.len())) {
                    out.menu = Some((rows.len(), key));
                }
            });
    }
    let has_chart = chart.is_some();
    if let Some((c, extent)) = chart {
        // The strip under the table and the pane, then the pane's bar.
        let strip = Rect::from_min_max(
            Pos2::new(head_all.left(), body_bottom),
            Pos2::new(head_all.right(), body_bottom + BAR_HEIGHT),
        );
        ui.painter().rect_filled(strip, 0., chrome::pal::FACE);
        if let (Some(bar), Some(t)) = (c.bar, pane.as_mut().and_then(|(p, _)| p.timeline())) {
            let span = timeline::Span::new(t.axis.first_minute, t.axis.pixels_per_week);
            let (geo, range) = timeline::paint_bar(
                ui,
                ui.id().with((id, "timeline")),
                bar,
                span,
                c.width,
                extent,
            );
            timeline::register_bar(actions, &format!("{id}_hscroll"), &geo, &range);
        }
    }
    if wide {
        if !has_chart {
            ui.painter().rect_filled(hbar, 0., chrome::pal::FACE);
        }
        let geo = crate::hscroll::paint(ui, hbar_id, hbar, hrange(columns.scroll_x));
        record(actions, format!("{id}_columns_hscroll"), hbar);
        record(actions, format!("{id}_columns_hscroll_thumb"), geo.thumb);
    }
    out
}

/// Draws the open editor in `cell` (`{table}_edit`; a list's items are
/// `{table}_choice_{n}`). Returns `Some(Some(text))` to commit,
/// `Some(None)` to close it unchanged, `None` while it stays open.
fn cell_editor(
    ui: &mut egui::Ui,
    table: &str,
    cell: Rect,
    ed: &mut CellEdit,
    actions: &mut BTreeMap<String, [f32; 4]>,
) -> Option<Option<String>> {
    if !cell.is_positive() {
        return None;
    }
    let painter = ui.painter().with_clip_rect(cell);
    painter.rect_filled(cell, 0., Color32::WHITE);
    let frame = Stroke::new(1_f32, FOCUS);
    painter.rect_stroke(cell, 0., frame, egui::StrokeKind::Inside);
    record(actions, format!("{table}_edit"), cell);
    let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
    let fresh = std::mem::take(&mut ed.fresh);
    let inner = cell.shrink2(Vec2::new(3., 1.));
    match ed.kind.clone() {
        EditKind::Text => {
            let edit_id = ui.id().with((table, "cell_edit"));
            let r = ui.put(
                inner,
                egui::TextEdit::singleline(&mut ed.text)
                    .id(edit_id)
                    .frame(false)
                    .margin(egui::Margin::ZERO)
                    .font(chrome::table_font())
                    .text_color(INK),
            );
            if fresh {
                // The cell's text, selected: typing replaces it.
                let mut state = egui::TextEdit::load_state(ui.ctx(), r.id).unwrap_or_default();
                state
                    .cursor
                    .set_char_range(Some(egui::text::CCursorRange::two(
                        egui::text::CCursor::new(0),
                        egui::text::CCursor::new(ed.text.chars().count()),
                    )));
                state.store(ui.ctx(), r.id);
                r.request_focus();
                return None;
            }
            if escape {
                return Some(None);
            }
            let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
            (enter || r.lost_focus()).then(|| Some(ed.text.clone()))
        }
        EditKind::Choice(items) => {
            // A drop-down: the value, the drop arrow, the list under the cell.
            let shown = match crate::details::record_tabs::indicator_named(&ed.text) {
                Some((_, color)) => {
                    let center = Pos2::new(inner.left() + 5., inner.center().y);
                    crate::details::record_tabs::paint_indicator(&painter, center, color);
                    inner.left() + 14.
                }
                None => inner.left(),
            };
            painter.text(
                Pos2::new(shown, inner.center().y),
                Align2::LEFT_CENTER,
                &ed.text,
                chrome::table_font(),
                INK,
            );
            chrome::chevron(
                &painter,
                Pos2::new(cell.right() - 8., cell.center().y - 2.),
                true,
                INK,
            );
            let mut picked = None;
            let mut open = true;
            let mut rects = vec![];
            let list = egui::Popup::new(
                ui.id().with((table, "cell_choice")),
                ui.ctx().clone(),
                egui::PopupAnchor::Position(cell.left_bottom()),
                ui.layer_id(),
            )
            .kind(egui::PopupKind::Popup)
            .gap(0.)
            .open_bool(&mut open)
            .close_behavior(egui::PopupCloseBehavior::IgnoreClicks)
            .show(|ui| {
                ui.set_min_width(cell.width().max(100.));
                ui.spacing_mut().item_spacing.y = 0.;
                for (n, item) in items.iter().enumerate() {
                    let r = ui
                        .horizontal(|ui| {
                            let (mark, _) =
                                ui.allocate_exact_size(Vec2::new(12., 16.), Sense::hover());
                            if let Some((_, color)) =
                                crate::details::record_tabs::indicator_named(item)
                            {
                                crate::details::record_tabs::paint_indicator(
                                    ui.painter(),
                                    mark.center(),
                                    color,
                                );
                            }
                            let label = if item.is_empty() { " " } else { item.as_str() };
                            crate::dialog_kit::flow_selectable(
                                ui,
                                item.eq_ignore_ascii_case(&ed.text),
                                label,
                                80.,
                            )
                        })
                        .inner;
                    rects.push((format!("{table}_choice_{n}"), r.rect));
                    if r.clicked() {
                        picked = Some(item.clone());
                    }
                }
            });
            for (name, r) in rects {
                record(actions, name, r);
            }
            if let Some(item) = picked {
                return Some(Some(item));
            }
            if escape {
                return Some(None);
            }
            let area = list.map(|l| l.response.rect);
            let pressed = ui.input(|i| i.pointer.any_pressed());
            let at = ui.input(|i| i.pointer.interact_pos());
            let inside = |p: Pos2| cell.contains(p) || area.is_some_and(|a| a.contains(p));
            let outside = !fresh && pressed && at.is_some_and(|p| !inside(p));
            outside.then_some(None)
        }
    }
}

/// The rows `show` paints as selected: every selected row, and the current
/// one, which carries the focus cell and the keyboard menu.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Selection {
    pub rows: BTreeSet<usize>,
    pub current: Option<usize>,
    /// Scroll just enough to bring the current row into view (the keyboard
    /// moved it).
    pub reveal: bool,
}

impl Selection {
    pub fn contains(&self, row: usize) -> bool {
        self.current == Some(row) || self.rows.contains(&row)
    }
}

/// A keyboard move of the current row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// Rows down (positive) or up.
    By(isize),
    First,
    Last,
}

/// Rows a page key moves.
pub const PAGE: isize = 20;

/// The row keys pressed this frame: Up, Down, PageUp, PageDown, Home and
/// End, and whether Shift (without Ctrl) extends the selection.
pub fn step_keys(i: &egui::InputState) -> Option<(Step, bool)> {
    use egui::Key;
    let step = if i.key_pressed(Key::ArrowDown) {
        Step::By(1)
    } else if i.key_pressed(Key::ArrowUp) {
        Step::By(-1)
    } else if i.key_pressed(Key::PageDown) {
        Step::By(PAGE)
    } else if i.key_pressed(Key::PageUp) {
        Step::By(-PAGE)
    } else if i.key_pressed(Key::Home) {
        Step::First
    } else if i.key_pressed(Key::End) {
        Step::Last
    } else {
        return None;
    };
    let extend = i.modifiers.shift && !i.modifiers.command;
    Some((step, extend))
}

/// Multi-row selection of a view table, keyed by the view's stable row id.
/// The view keeps its current row itself (the row single-row commands and
/// the details follow); this holds the rows selected along with it and the
/// Shift anchor. The extra rows apply only while they hold the current row,
/// so a view that sets its current row directly selects that row alone.
///
/// Semantics, as in the activity table: a plain click selects one row and
/// sets the anchor; Ctrl+click toggles a row; Shift+click selects the rows
/// from the anchor to the clicked one in display order; Ctrl+A selects every
/// shown row; Shift+Up/Down extends from the anchor; plain Up/Down moves the
/// current row alone. Nothing here is saved with the layout.
#[derive(Clone, Debug)]
pub struct RowSelection<K> {
    rows: BTreeSet<K>,
    anchor: Option<K>,
    /// The row the selection was started on (a plain click, the start of a
    /// Shift range); Ctrl+click leaves it. Fill Down copies from it.
    first: Option<K>,
    /// Selectable rows as last shown, in display order.
    order: Vec<K>,
    /// The keyboard moved the current row: bring it into view.
    reveal: bool,
    /// One row at a time: the view's commands act on its current row only,
    /// so Ctrl, Shift and Ctrl+A do not select more.
    single: bool,
}

impl<K> Default for RowSelection<K> {
    fn default() -> Self {
        Self {
            rows: BTreeSet::new(),
            anchor: None,
            first: None,
            order: vec![],
            reveal: false,
            single: false,
        }
    }
}

impl<K> RowSelection<K> {
    /// A selection of one row at a time, for a view whose commands act on
    /// the current row only.
    pub fn single() -> Self {
        Self {
            single: true,
            ..Default::default()
        }
    }
}

impl<K: Ord + Clone> RowSelection<K> {
    /// Whether the extra rows apply (they hold the current row).
    fn applies(&self, current: Option<&K>) -> bool {
        current.is_some_and(|c| self.rows.contains(c))
    }

    /// The row the selection was started on, while it is still selected
    /// along with the current row.
    pub fn first(&self, current: Option<&K>) -> Option<K> {
        let first = self.first.as_ref()?;
        self.picked(current).contains(first).then(|| first.clone())
    }

    /// Is the row of `key` selected?
    pub fn contains(&self, current: Option<&K>, key: &K) -> bool {
        current == Some(key) || (self.applies(current) && self.rows.contains(key))
    }

    /// Record the rows as shown this frame (`None`: a row that cannot be
    /// selected, such as a band) and describe the selection for `show`.
    /// A current row that is no longer shown moves to the first shown row
    /// (`None` when none is), as a record view keeps its selection.
    pub fn shown(&mut self, current: &mut Option<K>, keys: &[Option<K>]) -> Selection {
        self.order = keys.iter().flatten().cloned().collect();
        // Rows a filter, regroup or collapse hid leave the selection, the
        // current row too: a command must never act on a row the user
        // cannot see.
        let order: BTreeSet<&K> = self.order.iter().collect();
        self.rows.retain(|k| order.contains(k));
        if current.as_ref().is_some_and(|c| !order.contains(c)) {
            *current = self.order.first().cloned();
        }
        let current = current.as_ref();
        let applies = self.applies(current);
        let mut out = Selection {
            reveal: std::mem::take(&mut self.reveal),
            ..Default::default()
        };
        for (i, key) in keys.iter().enumerate() {
            let Some(key) = key else {
                continue;
            };
            if current == Some(key) {
                out.current = Some(i);
                out.rows.insert(i);
            } else if applies && self.rows.contains(key) {
                out.rows.insert(i);
            }
        }
        out
    }

    /// The selected rows in display order, only rows as last shown; just
    /// the current row when nothing else is selected or the current row is
    /// not among the shown selection.
    pub fn picked(&self, current: Option<&K>) -> Vec<K> {
        let Some(current) = current else {
            return vec![];
        };
        if !self.applies(Some(current)) {
            return vec![current.clone()];
        }
        let out: Vec<K> = self
            .order
            .iter()
            .filter(|k| self.rows.contains(k))
            .cloned()
            .collect();
        if out.contains(current) {
            out
        } else {
            vec![current.clone()]
        }
    }

    /// A click on the row of `key`; returns the new current row. A
    /// right-click inside the selection keeps it; outside, it selects the
    /// row alone.
    pub fn click(
        &mut self,
        current: Option<&K>,
        key: K,
        modifiers: egui::Modifiers,
        secondary: bool,
    ) -> K {
        if self.single {
            self.rows = [key.clone()].into();
            self.anchor = Some(key.clone());
            self.first = Some(key.clone());
            return key;
        }
        if secondary {
            if self.contains(current, &key) {
                return current.cloned().unwrap_or(key);
            }
            return self.click(current, key, egui::Modifiers::default(), false);
        }
        if !self.applies(current) {
            self.rows = current.cloned().into_iter().collect();
            self.first = current.cloned();
        }
        let at = |order: &[K], k: &K| order.iter().position(|x| x == k);
        if modifiers.shift {
            let anchor = self
                .anchor
                .clone()
                .filter(|a| at(&self.order, a).is_some())
                .or_else(|| current.cloned())
                .unwrap_or_else(|| key.clone());
            self.rows = match (at(&self.order, &anchor), at(&self.order, &key)) {
                (Some(a), Some(b)) => self.order[a.min(b)..=a.max(b)].iter().cloned().collect(),
                _ => [key.clone()].into(),
            };
            self.first = Some(anchor.clone());
            self.anchor = Some(anchor);
            key
        } else if modifiers.command {
            self.anchor = Some(key.clone());
            if !self.rows.remove(&key) {
                self.rows.insert(key.clone());
                return key;
            }
            if current != Some(&key) {
                return current.cloned().unwrap_or(key);
            }
            // The current row moves to another selected row, if any.
            let next = self
                .order
                .iter()
                .find(|k| self.rows.contains(k))
                .or_else(|| self.rows.iter().next())
                .cloned();
            match next {
                Some(next) => next,
                None => {
                    self.rows.insert(key.clone());
                    key
                }
            }
        } else {
            self.rows = [key.clone()].into();
            self.anchor = Some(key.clone());
            self.first = Some(key.clone());
            key
        }
    }

    /// Select these rows (a paste's new rows); the first becomes current
    /// and the anchor.
    pub fn select(&mut self, keys: &[K]) -> Option<K> {
        let first = keys.first()?.clone();
        self.rows = keys.iter().cloned().collect();
        self.anchor = Some(first.clone());
        self.first = Some(first.clone());
        // New rows count as shown until the table is next drawn.
        for key in keys {
            if !self.order.contains(key) {
                self.order.push(key.clone());
            }
        }
        Some(first)
    }

    /// The row to make current once the rows `gone` are deleted: the first
    /// shown row after the last of them that stays, else the nearest one
    /// before the first of them.
    pub fn neighbour(&self, gone: &[K]) -> Option<K> {
        let at: Vec<usize> = gone
            .iter()
            .filter_map(|k| self.order.iter().position(|x| x == k))
            .collect();
        let (first, last) = (*at.iter().min()?, *at.iter().max()?);
        self.order[last + 1..]
            .iter()
            .find(|k| !gone.contains(k))
            .or_else(|| self.order[..first].iter().rev().find(|k| !gone.contains(k)))
            .cloned()
    }

    /// Ctrl+A: every row shown. The current row stays when shown, else the
    /// first row becomes current. `None` when no row is shown.
    pub fn select_all(&mut self, current: Option<&K>) -> Option<K> {
        let first = self.order.first()?.clone();
        if self.single {
            return Some(current.cloned().unwrap_or(first));
        }
        self.rows = self.order.iter().cloned().collect();
        self.anchor = Some(first.clone());
        Some(
            current
                .filter(|c| self.rows.contains(c))
                .cloned()
                .unwrap_or(first),
        )
    }

    /// A row key: moves the current row (`extend`: Shift with the key,
    /// which extends the selection from the anchor). Returns the new
    /// current row. With no current row shown, a key starts at the first
    /// row, as in the activity table.
    pub fn step(&mut self, current: Option<&K>, step: Step, extend: bool) -> Option<K> {
        let last = self.order.len().checked_sub(1)?;
        let at = current.and_then(|c| self.order.iter().position(|k| k == c));
        let next = match (step, at) {
            (Step::By(delta), Some(at)) => (at as isize + delta).clamp(0, last as isize) as usize,
            (Step::By(_), None) | (Step::First, _) => 0,
            (Step::Last, _) => last,
        };
        let key = self.order[next].clone();
        self.reveal = true;
        let modifiers = if extend {
            egui::Modifiers::SHIFT
        } else {
            egui::Modifiers::default()
        };
        Some(self.click(current, key, modifiers, false))
    }
}

#[cfg(test)]
mod selection_tests {
    use super::*;

    const CTRL: egui::Modifiers = egui::Modifiers::COMMAND;
    const SHIFT: egui::Modifiers = egui::Modifiers::SHIFT;

    fn shown(s: &mut RowSelection<i64>, mut current: Option<i64>) -> Selection {
        let keys: Vec<Option<i64>> = vec![Some(10), None, Some(20), Some(30), Some(40)];
        s.shown(&mut current, &keys)
    }

    #[test]
    fn click_ctrl_shift_and_select_all_follow_the_activity_table() {
        let mut s = RowSelection::default();
        let mut current = None;
        shown(&mut s, current);
        current = Some(s.click(current.as_ref(), 10, Default::default(), false));
        assert_eq!(s.picked(current.as_ref()), [10]);
        // Shift+click: anchor..row in display order (the band is skipped).
        current = Some(s.click(current.as_ref(), 30, SHIFT, false));
        assert_eq!(current, Some(30));
        assert_eq!(s.picked(current.as_ref()), [10, 20, 30]);
        let painted = shown(&mut s, current);
        assert_eq!(painted.rows, BTreeSet::from([0, 2, 3]));
        assert_eq!(painted.current, Some(3));
        // Ctrl+click toggles; dropping the current row moves it.
        current = Some(s.click(current.as_ref(), 20, CTRL, false));
        assert_eq!(s.picked(current.as_ref()), [10, 30]);
        current = Some(s.click(current.as_ref(), 30, CTRL, false));
        assert_eq!(current, Some(10));
        assert_eq!(s.picked(current.as_ref()), [10]);
        current = Some(s.click(current.as_ref(), 40, CTRL, false));
        assert_eq!(s.picked(current.as_ref()), [10, 40]);
        // Right-click inside keeps the selection; outside selects alone.
        current = Some(s.click(current.as_ref(), 10, Default::default(), true));
        assert_eq!(current, Some(40), "the current row stays");
        assert_eq!(s.picked(current.as_ref()), [10, 40]);
        current = Some(s.click(current.as_ref(), 20, Default::default(), true));
        assert_eq!(s.picked(current.as_ref()), [20]);
        // Ctrl+A keeps the current row.
        current = s.select_all(current.as_ref());
        assert_eq!(current, Some(20));
        assert_eq!(s.picked(current.as_ref()), [10, 20, 30, 40]);
        // A view setting its current row directly selects it alone.
        assert_eq!(s.picked(Some(&99)), [99]);
    }

    #[test]
    fn keys_move_the_current_row_and_shift_extends_from_the_anchor() {
        let mut s = RowSelection::default();
        shown(&mut s, None);
        let mut current = Some(s.click(None, 20, Default::default(), false));
        current = s.step(current.as_ref(), Step::By(1), true);
        current = s.step(current.as_ref(), Step::By(1), true);
        assert_eq!(current, Some(40));
        assert_eq!(s.picked(current.as_ref()), [20, 30, 40]);
        current = s.step(current.as_ref(), Step::By(-1), true);
        assert_eq!(s.picked(current.as_ref()), [20, 30]);
        // A plain key moves the current row alone.
        current = s.step(current.as_ref(), Step::By(-1), false);
        assert_eq!(s.picked(current.as_ref()), [20]);
        current = s.step(current.as_ref(), Step::By(PAGE), false);
        assert_eq!(current, Some(40));
        current = s.step(current.as_ref(), Step::First, false);
        assert_eq!(current, Some(10));
        assert!(shown(&mut s, current).reveal, "the key reveals the row");
        assert!(!shown(&mut s, current).reveal, "once");
    }

    #[test]
    fn shift_with_page_home_and_end_extends_and_single_never_selects_more() {
        let mut s = RowSelection::default();
        shown(&mut s, None);
        let mut current = Some(s.click(None, 20, Default::default(), false));
        current = s.step(current.as_ref(), Step::Last, true);
        assert_eq!(s.picked(current.as_ref()), [20, 30, 40]);
        current = s.step(current.as_ref(), Step::First, true);
        assert_eq!(s.picked(current.as_ref()), [10, 20]);
        current = s.step(current.as_ref(), Step::By(PAGE), true);
        assert_eq!(s.picked(current.as_ref()), [20, 30, 40]);
        // The neighbour of deleted rows: after the last, else before.
        assert_eq!(s.neighbour(&[20, 30]), Some(40));
        assert_eq!(s.neighbour(&[30, 40]), Some(20));
        assert_eq!(s.neighbour(&[10, 20, 30, 40]), None);
        // A single-row view: Ctrl, Shift and Ctrl+A keep one row.
        let mut s = RowSelection::single();
        shown(&mut s, None);
        let mut current = Some(s.click(None, 10, Default::default(), false));
        current = Some(s.click(current.as_ref(), 30, SHIFT, false));
        assert_eq!(s.picked(current.as_ref()), [30]);
        current = Some(s.click(current.as_ref(), 40, CTRL, false));
        assert_eq!(s.picked(current.as_ref()), [40]);
        current = s.select_all(current.as_ref());
        assert_eq!(s.picked(current.as_ref()), [40]);
        current = s.step(current.as_ref(), Step::First, true);
        assert_eq!(s.picked(current.as_ref()), [10]);
        assert!(s.single);
    }

    #[test]
    fn a_row_key_from_a_hidden_current_row_starts_at_the_first_row() {
        let mut s = RowSelection::default();
        shown(&mut s, None);
        // 99 is not shown (a collapse or filter hid it).
        assert_eq!(s.step(Some(&99), Step::By(1), false), Some(10));
        assert_eq!(s.step(Some(&99), Step::By(-1), false), Some(10));
        assert_eq!(s.step(Some(&99), Step::By(PAGE), false), Some(10));
        assert_eq!(s.step(Some(&99), Step::Last, false), Some(40));
        // No current row: the same.
        assert_eq!(s.step(None, Step::By(1), false), Some(10));
    }

    #[test]
    fn view_bands_take_the_activity_table_levels() {
        let layout = crate::grid::layout::Layout::default();
        let band = |depth| Row {
            depth,
            band: true,
            ..Default::default()
        };
        // Styled: each level's style and height, as the activity table's
        // default layout draws its bands.
        for level in 0..5 {
            let style = band_style(level, BandPitch::Styled);
            assert_eq!(style, layout.group_sort.band_style(level), "level {level}");
            assert_eq!(
                band(level).height(BandPitch::Styled),
                layout.group_sort.band_height(level)
            );
        }
        assert_eq!(band(0).height(BandPitch::Styled), 24.);
        assert_eq!(band(1).height(BandPitch::Styled), 22.);
        assert_eq!(band(2).height(BandPitch::Styled), 18.);
        assert_eq!(Row::default().height(BandPitch::Styled), ROW_HEIGHT);
        // Fixed: the pitch holds and a caption too large for it is cut.
        let fixed = BandPitch::Fixed(ROW_HEIGHT);
        assert_eq!(band(0).height(fixed), ROW_HEIGHT);
        let top = band_style(0, fixed);
        assert_eq!(
            (top.font_size, top.background),
            (9., layout.group_sort.bands[0].background)
        );
        assert_eq!(band_style(3, fixed).font_size, 8.);
        // The spreadsheet's band row matches the top level.
        assert_eq!(band::fill(&band_style(0, fixed), false), BAND);
        assert_eq!(band::ink(&band_style(0, fixed), false), BAND_TEXT);
    }

    #[test]
    fn a_dragged_column_moves_to_the_drop_slot() {
        let mut c = Columns::new(&[50., 60., 70., 80.]);
        c.move_slot(0, 2);
        assert_eq!(c.order, [1, 2, 0, 3], "moved, not swapped with 2");
        c.move_slot(3, 0);
        assert_eq!(c.order, [3, 1, 2, 0]);
        c.move_slot(1, 1);
        c.move_slot(9, 0);
        assert_eq!(c.order, [3, 1, 2, 0]);
    }

    #[test]
    fn rows_a_filter_or_collapse_hides_leave_the_selection() {
        let mut s = RowSelection::default();
        shown(&mut s, None);
        let mut current = Some(s.click(None, 10, Default::default(), false));
        current = s.select_all(current.as_ref());
        assert_eq!(s.picked(current.as_ref()), [10, 20, 30, 40]);
        // A filter shows two of the four.
        let keys = vec![Some(10), Some(30)];
        let painted = s.shown(&mut current, &keys);
        assert_eq!(painted.rows, BTreeSet::from([0, 1]));
        assert_eq!(s.picked(current.as_ref()), [10, 30]);
        // Showing all again does not bring the hidden rows back.
        shown(&mut s, current);
        assert_eq!(s.picked(current.as_ref()), [10, 30]);
        // The current row hidden: it moves to the first shown row, and only
        // shown rows stay selected. No command can reach the hidden 10.
        let keys = vec![Some(30), Some(40)];
        let painted = s.shown(&mut current, &keys);
        assert_eq!(current, Some(30));
        assert_eq!(painted.current, Some(0));
        assert_eq!(s.picked(current.as_ref()), [30]);
        // Nothing shown: no current row.
        s.shown(&mut current, &[None]);
        assert_eq!(current, None);
        assert!(s.picked(current.as_ref()).is_empty());
    }
}
