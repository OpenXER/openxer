//! Painting and pointer handling for the table half of the Activities view:
//! the layout bar, column headers (sort, reorder, resize, context menu),
//! band rows, cells, the focused cell and in-place editors.
use super::band;
use super::columns::{self, Align as ColAlign, Column, Editor, Index, Kind};
use super::*;
use std::rc::Rc;

/// Hover text of a column header: its title, and for Start and Finish what
/// their markers mean.
pub(crate) fn header_hint(column: &Column) -> String {
    match column.source {
        columns::Source::Field(columns::Field::Start | columns::Field::Finish) => {
            format!("{}\n{}", column.title, super::marks::HEADER_HINT)
        }
        _ => column.title.clone(),
    }
}

/// Focused cell outline of the selected row (2 px, inside the cell).
const FOCUS: Color32 = crate::theme::color(crate::theme::AMBER);
/// Header divider (followed by a white pixel) and the line under the header.
const HEADER_LINE: Color32 = Color32::BLACK;
/// Cell lines: 1 px on every row and column.
const LINE: Color32 = Color32::from_rgb(0xC0, 0xC0, 0xC0);
/// Inner top and left line of a sunken edit field.
const DARK_SHADOW: Color32 = Color32::from_rgb(0x69, 0x69, 0x69);
/// Expand box border on band rows.
const BOX_BORDER: Color32 = Color32::from_rgb(0xA0, 0xA0, 0xA0);
/// Header sort glyph: etched shadow and highlight.
const ETCH_DARK: Color32 = Color32::from_rgb(0xA0, 0xA0, 0xA0);
/// Rows matching the filter when it highlights instead of hiding.
const HIGHLIGHT: Color32 = crate::theme::color(crate::theme::HIGHLIGHT);
const MIN_WIDTH: f32 = 20.;
/// Width of one band level's strip at the left of every row: 4 px of the
/// level's color and a 1 px cell line.
pub const STRIP: f32 = 5.;
/// Space in the first column of grouped rows between the strips and the
/// activity's cell (where a band row has its expand box).
const GUTTER: f32 = 18.;
/// Galley padding in a cell, so that ink starts 3 px into the cell and ends
/// 4 px before its line (Arial's side bearings make up the rest).
const PAD_LEFT: f32 = 2.;
const PAD_RIGHT: f32 = 2.;
// Galley top of 8 pt text below the row top: the shared band painter's
// measure.
use super::band::TEXT_TOP;
/// Header caption line pitch.
const CAPTION_PITCH: f32 = 14.;
/// Left bearing allowance of bold band captions.
const BAND_INK: f32 = 1.;

/// Pitch, band depth and top offset of every table row for one frame.
pub struct RowMetrics {
    pub heights: Vec<f32>,
    /// Band levels above each row: one indent strip each.
    pub depths: Vec<usize>,
    /// Row tops from the first row's top; one extra entry holds the total.
    pub offsets: Vec<f32>,
}

impl RowMetrics {
    /// The row at `y` (from the first row's top), clamped to the list.
    pub fn row_at(&self, y: f32) -> usize {
        self.offsets
            .partition_point(|&top| top <= y)
            .saturating_sub(1)
            .min(self.heights.len().saturating_sub(1))
    }
}

/// Lookups shared by every row of one frame.
#[derive(Clone)]
pub struct FrameCache {
    pub cols: Rc<Vec<Column>>,
    pub ix: Rc<Index>,
    /// Column left edges relative to the table's left edge; one extra entry
    /// holds the total width.
    pub edges: Rc<Vec<f32>>,
}

fn color(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

fn place(cell: Rect, align: ColAlign, galley_width: f32) -> f32 {
    match align {
        ColAlign::Left => cell.left(),
        ColAlign::Center => (cell.center().x - galley_width / 2.).round(),
        ColAlign::Right => cell.right() - galley_width,
    }
}

/// Snap a row or cell to whole pixels so 1 px lines stay crisp at any
/// scroll offset.
fn snap(r: Rect) -> Rect {
    Rect::from_min_max(r.min.round(), r.max.round())
}

/// The text box of a cell spanning `left..right` (`right` is one past the
/// cell's line pixel).
fn text_box(left: f32, right: f32, top: f32, bottom: f32) -> Rect {
    Rect::from_min_max(
        Pos2::new(left + PAD_LEFT, top),
        Pos2::new(right - 1. - PAD_RIGHT, bottom),
    )
}

/// Greedy word wrap of a header caption into lines that fit `width`.
fn wrap_caption(ui: &egui::Ui, text: &str, font: &FontId, width: f32) -> Vec<String> {
    let measure =
        |t: &str| ui.fonts_mut(|f| f.layout_no_wrap(t.into(), font.clone(), INK).size().x);
    let mut lines: Vec<String> = vec![];
    for word in text.split_whitespace() {
        match lines.last_mut() {
            Some(line) if measure(&format!("{line} {word}")) <= width => {
                line.push(' ');
                line.push_str(word);
            }
            _ => lines.push(word.to_string()),
        }
    }
    lines
}

/// The focused cell's 2 px orange outline, inside the cell.
fn focus_outline(p: &egui::Painter, cell: Rect) {
    let (l, t, r, b) = (cell.left(), cell.top(), cell.right(), cell.bottom());
    chrome::fill(
        p,
        Rect::from_min_max(Pos2::new(l, t), Pos2::new(r, t + 2.)),
        FOCUS,
    );
    chrome::fill(
        p,
        Rect::from_min_max(Pos2::new(l, b - 2.), Pos2::new(r, b)),
        FOCUS,
    );
    chrome::fill(
        p,
        Rect::from_min_max(Pos2::new(l, t), Pos2::new(l + 2., b)),
        FOCUS,
    );
    chrome::fill(
        p,
        Rect::from_min_max(Pos2::new(r - 2., t), Pos2::new(r, b)),
        FOCUS,
    );
}

/// Width of a combo box's drop button.
const DROP_BUTTON: f32 = 17.;

/// The sunken white field of an in-place editor filling `cell`: gray and
/// dark gray lines top and left, light gray and white bottom and right.
/// Returns the field inside the frame.
fn sunken_field(p: &egui::Painter, cell: Rect) -> Rect {
    let (l, t, r, b) = (cell.left(), cell.top(), cell.right(), cell.bottom());
    chrome::fill(p, cell, Color32::WHITE);
    chrome::hline(p, l, r, t, chrome::pal::SHADOW);
    chrome::vline(p, l, t, b, chrome::pal::SHADOW);
    chrome::hline(p, l + 1., r - 1., t + 1., DARK_SHADOW);
    chrome::vline(p, l + 1., t + 1., b - 1., DARK_SHADOW);
    chrome::hline(p, l + 1., r - 1., b - 2., chrome::pal::LIGHT);
    chrome::vline(p, r - 2., t + 1., b - 1., chrome::pal::LIGHT);
    cell.shrink(2.)
}

/// A raised drop button with a black down arrow (7, 5, 3 and 1 px rows).
fn drop_button(p: &egui::Painter, b: Rect) {
    let (l, t, r, bottom) = (b.left(), b.top(), b.right(), b.bottom());
    chrome::fill(p, b, chrome::pal::FACE);
    chrome::hline(p, l, r, t, chrome::pal::LIGHT);
    chrome::vline(p, l, t, bottom, chrome::pal::LIGHT);
    chrome::hline(p, l + 1., r - 1., t + 1., Color32::WHITE);
    chrome::vline(p, l + 1., t + 1., bottom - 1., Color32::WHITE);
    chrome::hline(p, l + 1., r - 1., bottom - 2., chrome::pal::SHADOW);
    chrome::vline(p, r - 2., t + 1., bottom - 1., chrome::pal::SHADOW);
    chrome::hline(p, l, r, bottom - 1., DARK_SHADOW);
    chrome::vline(p, r - 1., t, bottom, DARK_SHADOW);
    let cx = ((l + r) / 2.).floor();
    let cy = ((t + bottom) / 2.).floor() - 2.;
    for k in 0..4 {
        let k = k as f32;
        chrome::hline(p, cx - 3. + k, cx + 4. - k, cy + k, INK);
    }
}

/// A dotted focus rectangle (every other pixel black).
fn dotted_rect(p: &egui::Painter, r: Rect) {
    let dot = |x: f32, y: f32| {
        chrome::fill(
            p,
            Rect::from_min_size(Pos2::new(x, y), Vec2::splat(1.)),
            INK,
        );
    };
    let (w, h) = (r.width() as i32, r.height() as i32);
    for k in (0..w).step_by(2) {
        dot(r.left() + k as f32, r.top());
        dot(r.left() + k as f32, r.bottom() - 1.);
    }
    for k in (0..h).step_by(2) {
        dot(r.left(), r.top() + k as f32);
        dot(r.right() - 1., r.top() + k as f32);
    }
}

/// Expand box of a band row: 9x9, gray border, white inside, black minus,
/// plus when collapsed.
pub(crate) fn expand_box(p: &egui::Painter, min: Pos2, collapsed: bool) {
    let b = Rect::from_min_size(min, Vec2::splat(9.));
    chrome::fill(p, b, BOX_BORDER);
    chrome::fill(p, b.shrink(1.), Color32::WHITE);
    chrome::hline(p, b.left() + 2., b.left() + 7., b.top() + 4., INK);
    if collapsed {
        chrome::vline(p, b.left() + 4., b.top() + 2., b.top() + 7., INK);
    }
}

/// Header sort glyph: an etched triangle, 8 px wide, whose base ends 3 px
/// before the column divider; it points up for ascending order.
fn sort_glyph(p: &egui::Painter, divider: f32, base_y: f32, descending: bool) {
    let left = divider - 10.;
    let dot = |x: f32, y: f32, c: Color32| {
        chrome::fill(p, Rect::from_min_size(Pos2::new(x, y), Vec2::splat(1.)), c)
    };
    if descending {
        let top = base_y - 7.;
        chrome::hline(p, left, left + 8., top, ETCH_DARK);
        for k in 0..7 {
            let y = top + 1. + k as f32;
            dot(left + (k / 2) as f32, y, ETCH_DARK);
            dot(left + 7. - ((k + 1) / 2) as f32, y, Color32::WHITE);
        }
    } else {
        for k in 0..7 {
            let y = base_y - 7. + k as f32;
            dot(left + 3. - (k / 2) as f32, y, ETCH_DARK);
            if k > 0 {
                dot(left + 4. + ((k - 1) / 2) as f32, y, Color32::WHITE);
            }
        }
        chrome::hline(p, left, left + 8., base_y, Color32::WHITE);
    }
}

impl App {
    fn frame_cache(&self) -> FrameCache {
        if let Some(c) = self.grid.frame.borrow().as_ref() {
            return c.clone();
        }
        let catalog = columns::catalog(self.shown());
        let cols = self.visible_columns(&catalog);
        let mut edges = vec![0.];
        for c in &cols {
            edges.push(edges.last().unwrap() + c.width);
        }
        let cache = FrameCache {
            cols: Rc::new(cols),
            ix: self.shared_index(),
            edges: Rc::new(edges),
        };
        *self.grid.frame.borrow_mut() = Some(cache.clone());
        cache
    }

    /// Row metrics for `rows` (from the last `rows()` call): activity rows
    /// take the Gantt's row height, band rows their level's height.
    pub(crate) fn row_metrics(&self, rows: &[Row]) -> RowMetrics {
        let bands = self.grid.bands.borrow();
        let gs = &self.grid.layout.group_sort;
        let mut heights = Vec::with_capacity(rows.len());
        let mut depths = Vec::with_capacity(rows.len());
        let mut offsets = Vec::with_capacity(rows.len() + 1);
        offsets.push(0.);
        let mut under = 0;
        for row in rows {
            let (height, depth) = match *row {
                Row::Group(b) => {
                    let level = bands.get(b).map_or(0, |b| b.level);
                    under = level + 1;
                    (gs.band_height(level), level)
                }
                Row::Activity(i) => (self.activity_height(i), under),
            };
            heights.push(height);
            depths.push(depth);
            offsets.push(offsets.last().unwrap() + height);
        }
        RowMetrics {
            heights,
            depths,
            offsets,
        }
    }

    /// Screen top and height of each row in the last painted frame.
    #[cfg(test)]
    pub(crate) fn grid_row_tops(&self, rows: &[Row]) -> Vec<(f32, f32)> {
        let offsets = &self.grid.row_offsets;
        (0..rows.len().min(offsets.len().saturating_sub(1)))
            .map(|k| (self.grid.body_top + offsets[k], offsets[k + 1] - offsets[k]))
            .collect()
    }

    /// Left edge of each column in the table's own coordinates, then the
    /// right edge of the last one (so the last entry is the content width).
    pub(crate) fn column_edges(&self) -> Rc<Vec<f32>> {
        self.frame_cache().edges
    }

    /// Total width of the table's columns, before any scrolling.
    pub(crate) fn table_content_width(&self) -> f32 {
        self.column_edges().last().copied().unwrap_or(0.)
    }

    /// Right edge of the last visible column for rows whose table starts at
    /// `left`: the end of the columns, or `pane_right` when they run past the
    /// pane. Fills (bands, selection) stop here; the space after the last
    /// column stays the plain table background.
    pub(crate) fn columns_right(&self, left: f32, pane_right: f32) -> f32 {
        let cache = self.frame_cache();
        let total = *cache.edges.last().unwrap_or(&0.);
        let x0 = (left.round() - self.grid.scroll_x).round();
        (x0 + total).min(pane_right.round()).max(left.round())
    }

    pub(crate) fn drop_frame_cache(&self) {
        *self.grid.frame.borrow_mut() = None;
    }

    // The `Layout:` / `Filter:` bar and the layout options menu are drawn
    // with the view header in views::docks.

    /// Column headers: click sorts (Shift adds a secondary key), drag
    /// reorders, edge drag resizes, edge double-click fits, right-click menu.
    pub(crate) fn grid_header(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        head: Rect,
        table_w: f32,
    ) {
        let cache = self.frame_cache();
        let total = *cache.edges.last().unwrap_or(&0.);
        self.grid.cells.view_width = table_w;
        let table = Rect::from_min_max(
            head.min,
            Pos2::new(head.left() + table_w, ui.max_rect().bottom()),
        );
        // Horizontal scrolling over the table (Shift+wheel or a sideways wheel).
        if ctx.input(|i| i.pointer.hover_pos().is_some_and(|p| table.contains(p))) {
            let delta = ctx.input(|i| i.smooth_scroll_delta);
            let dx = if delta.x != 0. {
                delta.x
            } else if ctx.input(|i| i.modifiers.shift) {
                delta.y
            } else {
                0.
            };
            self.grid.scroll_x -= dx;
        }
        self.grid.scroll_x = self.grid.scroll_x.clamp(0., (total - table_w).max(0.));
        let x0 = (head.left() - self.grid.scroll_x).round();
        let clip = Rect::from_min_max(head.min, Pos2::new(head.left() + table_w, head.bottom()));
        let p = ui.painter().with_clip_rect(clip);
        let sort = self.grid.layout.group_sort.sort.clone();
        let font = FontId::proportional(FONT_SIZE);
        let (top, bottom) = (head.top().round(), head.bottom().round());
        // Raised header: white top and left edges, black line underneath.
        chrome::fill(
            &p,
            Rect::from_min_max(Pos2::new(clip.left(), top), Pos2::new(clip.right(), bottom)),
            chrome::pal::FACE,
        );
        chrome::hline(&p, clip.left(), clip.right(), top, Color32::WHITE);
        chrome::vline(&p, x0, top, bottom, Color32::WHITE);
        let mut sort_click = None;
        let mut moved = None;
        let mut resized = None;
        // Scrolled columns stay clear of the band strips at the left of the
        // rows: the header starts them at the same edge.
        let live = Rect::from_min_max(
            Pos2::new(clip.left() + self.scrolled_strip_gutter(), clip.top()),
            clip.max,
        );
        let pl = p.with_clip_rect(live);
        for (slot, column) in cache.cols.iter().enumerate() {
            let rect = Rect::from_min_max(
                Pos2::new(x0 + cache.edges[slot], head.top()),
                Pos2::new(x0 + cache.edges[slot + 1], head.bottom()),
            );
            if !rect.intersects(live) {
                continue;
            }
            // Captions wrap onto a second line and keep the data's side,
            // except dates, whose captions stay left.
            let mut caption = text_box(rect.left(), rect.right(), top + TEXT_TOP, bottom - 1.);
            // Caption ink sits 3 px after the column's white edge and ends
            // 4 px before its divider.
            caption.min.x += 1.;
            caption.max.x -= 1.;
            let align = if column.kind == Kind::Date {
                ColAlign::Left
            } else {
                column.align
            };
            let cp = pl.with_clip_rect(caption.expand2(Vec2::new(0., 2.)).intersect(live));
            for (n, line) in wrap_caption(ui, &column.title, &font, caption.width().max(8.))
                .into_iter()
                .enumerate()
            {
                let y = caption.top() + n as f32 * CAPTION_PITCH;
                if y + CAPTION_PITCH > bottom + 2. {
                    break;
                }
                let galley = cp.layout_no_wrap(line, font.clone(), INK);
                let x = place(caption, align, galley.size().x);
                cp.galley(Pos2::new(x, y), galley, INK);
            }
            if let Some(key) = sort.iter().find(|s| s.column == column.id) {
                sort_glyph(&pl, rect.right() - 1., bottom - 15., key.descending);
            }
            chrome::vline(&pl, rect.right() - 1., top, bottom, HEADER_LINE);
            chrome::vline(&pl, rect.right(), top, bottom - 1., Color32::WHITE);
            let body = rect.shrink2(Vec2::new(4., 0.)).intersect(live);
            let response = ui
                .interact(
                    body,
                    ui.id().with(("column", &column.id)),
                    Sense::click_and_drag(),
                )
                .on_hover_text(header_hint(column));
            self.action(&format!("grid_header_{}", column.id), &response);
            if response.clicked() {
                sort_click = Some((column.id.clone(), ctx.input(|i| i.modifiers.shift)));
            }
            if response.drag_started() {
                self.grid.column_drag = Some(slot);
            }
            if response.drag_stopped() {
                if let (Some(from), Some(pos)) = (
                    self.grid.column_drag.take(),
                    ctx.input(|i| i.pointer.interact_pos()),
                ) {
                    if live.contains(pos) {
                        let to = (0..cache.cols.len())
                            .find(|&n| pos.x < x0 + cache.edges[n + 1])
                            .unwrap_or(cache.cols.len() - 1);
                        moved = Some((from, to));
                    }
                }
            }
            // The header shares the table's context menu, as in the reference.
            self.table_menu_popup(&response, "header_menu");
            let edge = Rect::from_center_size(rect.right_center(), Vec2::new(7., rect.height()));
            if live.contains(edge.center()) {
                let resize = ui
                    .interact(
                        edge,
                        ui.id().with(("column_resize", &column.id)),
                        Sense::click_and_drag(),
                    )
                    .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
                self.action(&format!("grid_resize_{}", column.id), &resize);
                if resize.double_clicked() {
                    resized = Some((column.id.clone(), self.best_fit_width(&cache, column, ui)));
                } else if resize.dragged() {
                    if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                        resized = Some((column.id.clone(), pos.x - rect.left()));
                    }
                }
            }
        }
        if let Some((id, add)) = sort_click {
            self.header_sort(&id, add);
        }
        if let Some((from, to)) = moved {
            self.move_column(from, to);
        }
        if let Some((id, width)) = resized {
            self.set_column_width(&id, width);
        }
        chrome::hline(&p, clip.left(), clip.right(), bottom - 1., HEADER_LINE);
        // Column edits change the cached geometry for the rows that follow.
        self.drop_frame_cache();
    }

    /// Header click: sort by this column; again flips the direction. With
    /// Shift the column is added as (or flips) a secondary key.
    pub(crate) fn header_sort(&mut self, id: &str, add: bool) {
        let keys = &mut self.grid.layout.group_sort.sort;
        if add {
            match keys.iter_mut().find(|k| k.column == id) {
                Some(k) => k.descending = !k.descending,
                None => keys.push(layout::SortKey {
                    column: id.into(),
                    descending: false,
                }),
            }
        } else if keys.len() == 1 && keys[0].column == id {
            keys[0].descending = !keys[0].descending;
        } else {
            *keys = vec![layout::SortKey {
                column: id.into(),
                descending: false,
            }];
        }
    }

    /// Move a displayed column (indexes into the visible columns).
    pub(crate) fn move_column(&mut self, from: usize, to: usize) {
        let cache = self.frame_cache();
        let (Some(a), Some(b)) = (cache.cols.get(from), cache.cols.get(to)) else {
            return;
        };
        let list = &mut self.grid.layout.columns;
        let (Some(fi), Some(ti)) = (
            list.iter().position(|c| c.id == a.id),
            list.iter().position(|c| c.id == b.id),
        ) else {
            return;
        };
        let item = list.remove(fi);
        list.insert(ti, item);
        self.drop_frame_cache();
    }

    pub(crate) fn set_column_width(&mut self, id: &str, width: f32) {
        if let Some(c) = self.grid.layout.columns.iter_mut().find(|c| c.id == id) {
            c.width = width.clamp(MIN_WIDTH, 1500.).round();
        }
        self.drop_frame_cache();
    }

    /// Drops a column from the layout (the header menu no longer offers
    /// this; the Columns dialog does it).
    #[cfg(test)]
    pub(crate) fn hide_column(&mut self, id: &str) {
        if self.grid.layout.columns.len() > 1 {
            self.grid.layout.columns.retain(|c| c.id != id);
        }
        self.drop_frame_cache();
    }

    fn best_fit_width(&self, cache: &FrameCache, column: &Column, ui: &egui::Ui) -> f32 {
        let font = FontId::proportional(FONT_SIZE);
        let shown = self.shown();
        let widest = (0..shown.model.activities.len())
            .map(|i| {
                columns::display(
                    shown,
                    Some(i),
                    &columns::shown_value(shown, &cache.ix, column, i),
                )
            })
            .chain(std::iter::once(column.title.clone()))
            .map(|t| ui.fonts_mut(|f| f.layout_no_wrap(t, font.clone(), INK).size().x))
            .fold(0., f32::max);
        (widest + 12.).clamp(MIN_WIDTH, 600.)
    }

    /// Strips for `depth` band levels at the left of a row: 4 px of each
    /// level's color (selection blue on the selected row) and a cell line.
    /// Returns where the row's own content starts.
    fn paint_strips(&self, p: &egui::Painter, row: Rect, depth: usize, selected: bool) -> f32 {
        let gs = &self.grid.layout.group_sort;
        for level in 0..depth {
            let x = row.left() + level as f32 * STRIP;
            let fill = if selected {
                SELECT
            } else {
                color(gs.band_style(level).background)
            };
            chrome::fill(
                p,
                Rect::from_min_max(
                    Pos2::new(x, row.top()),
                    Pos2::new(x + STRIP - 1., row.bottom()),
                ),
                fill,
            );
            chrome::vline(p, x + STRIP - 1., row.top(), row.bottom(), LINE);
        }
        row.left() + depth as f32 * STRIP
    }

    /// Width of the widest run of band strips (the deepest band level), or
    /// 0 when the table is not grouped with indents.
    pub(crate) fn strip_gutter(&self) -> f32 {
        if !self.grid_indents() {
            return 0.;
        }
        let depth = self.grid.bands.borrow().iter().map(|b| b.level + 1).max();
        depth.unwrap_or(0) as f32 * STRIP
    }

    /// The strip gutter while the table is scrolled sideways: the columns
    /// scroll under the strips' edge, never over them.
    fn scrolled_strip_gutter(&self) -> f32 {
        if self.grid.scroll_x > 0.5 {
            self.strip_gutter()
        } else {
            0.
        }
    }

    /// Band strips are drawn only when the first grouping level indents.
    pub(crate) fn grid_indents(&self) -> bool {
        self.grid
            .layout
            .group_sort
            .levels
            .first()
            .is_none_or(|l| l.indent)
    }

    /// A band's table part: strips, fill, expand box, caption and totals.
    /// The caption spans the leading text columns; the columns after it keep
    /// their cell lines. A click toggles collapse.
    pub(crate) fn grid_band(
        &mut self,
        painter: &egui::Painter,
        rect: Rect,
        table_right: f32,
        index: usize,
        response: &egui::Response,
    ) {
        let Some(band) = self.grid.bands.borrow().get(index).cloned() else {
            return;
        };
        let cache = self.frame_cache();
        let style = self.grid.layout.group_sort.band_style(band.level);
        let rect = snap(rect);
        let table = Rect::from_min_max(rect.min, Pos2::new(table_right.round(), rect.bottom()));
        let p = painter.with_clip_rect(table.intersect(painter.clip_rect()));
        let depth = if self.grid_indents() { band.level } else { 0 };
        // A selected band is selection blue with white text, strips included.
        let selected = self.grid.cells.selected_band.as_deref() == Some(band.key.as_str());
        let left = self.paint_strips(&p, table, depth, selected);
        let (top, bottom) = (table.top(), table.bottom());
        let fill = band::fill(&style, selected);
        self.grid.cells.band_rects.push((index, table));
        chrome::fill(
            &p,
            Rect::from_min_max(Pos2::new(left, top), table.max),
            fill,
        );
        // The band's own 4 px strip runs on into its rows below.
        chrome::hline(&p, left + STRIP - 1., table.right(), bottom - 1., LINE);
        let x0 = (rect.left() - self.grid.scroll_x).round();
        // Scrolled column lines and totals stay clear of the strips.
        let pc = p.with_clip_rect(
            Rect::from_min_max(Pos2::new(left, top), table.max).intersect(p.clip_rect()),
        );
        let span = cache
            .cols
            .iter()
            .skip(1)
            .position(|c| c.kind != Kind::Text)
            .map_or(cache.cols.len(), |n| n + 1);
        for slot in span.saturating_sub(1)..cache.cols.len() {
            chrome::vline(&pc, x0 + cache.edges[slot + 1] - 1., top, bottom, LINE);
        }
        // The expand box sits 2 px into the band, centered on the cell
        // (rounded down to an even offset; measured 4, 6 and 8 px for 17,
        // 21 and 23 px cells).
        let cell = bottom - 1. - top;
        let offset = ((cell - 9.) / 2.).floor();
        let offset = offset + offset % 2.;
        let boxed = Pos2::new(left + 2., top + offset);
        let collapsed = self.collapsed.contains(&band.key);
        expand_box(&p, boxed, collapsed);
        let text_color = band::ink(&style, selected);
        let span_right = x0 + cache.edges[span] - 1.;
        let caption = Rect::from_min_max(
            Pos2::new(boxed.x + 9., top),
            Pos2::new(span_right.min(table.right()), bottom - 1.),
        );
        // Every band font keeps its cap line 4 px below the row top.
        band::caption(
            &p,
            caption,
            boxed.x + 19. - BAND_INK,
            top,
            &style,
            &band.label,
            selected,
        );
        if self.grid.layout.group_sort.show_group_totals {
            let font = band::total_font();
            for (slot, column) in cache.cols.iter().enumerate().skip(span) {
                let Some(v) =
                    group::band_total_cached(self.shown(), &cache.ix, column, &band.members)
                else {
                    continue;
                };
                let (l, r) = (x0 + cache.edges[slot], x0 + cache.edges[slot + 1]);
                let cell = text_box(l, r, top, bottom - 1.);
                let text = columns::display(self.shown(), None, &v);
                let g = p.layout_no_wrap(text, font.clone(), text_color);
                let x = place(cell, column.align, g.size().x);
                pc.with_clip_rect(
                    cell.expand2(Vec2::new(PAD_LEFT, 0.))
                        .intersect(pc.clip_rect()),
                )
                .galley(Pos2::new(x, band::total_top(top)), g, text_color);
            }
        }
        // Dragged activities show where they would land.
        if self.grid.cells.row_drag.is_some()
            && response
                .ctx
                .input(|i| i.pointer.hover_pos())
                .is_some_and(|pos| table.contains(pos))
        {
            dotted_rect(&p, table.shrink(1.));
        }
        // The expand box toggles the band; elsewhere a click selects it and
        // a double-click toggles it.
        let hit_box = Rect::from_min_size(boxed, Vec2::splat(9.)).expand(3.);
        self.action(&format!("grid_band_{index}"), response);
        self.action_rects.insert(
            format!("grid_band_box_{index}"),
            [
                hit_box.left(),
                hit_box.top(),
                hit_box.right(),
                hit_box.bottom(),
            ],
        );
        let on_box = response
            .interact_pointer_pos()
            .is_some_and(|pos| hit_box.contains(pos));
        let toggle = if response.clicked() && on_box {
            true
        } else {
            if response.clicked() {
                self.select_band(&band.key);
            }
            response.double_clicked()
        };
        if toggle && !self.collapsed.insert(band.key.clone()) {
            self.collapsed.remove(&band.key);
        }
    }

    /// An activity row's table cells under `depth` band levels: strips, cell
    /// lines, text, the focused cell and the in-place editor. The selected
    /// row's blue fill is painted by the caller.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn grid_cells(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        painter: &egui::Painter,
        rect: Rect,
        table_w: f32,
        i: usize,
        depth: usize,
        selected: bool,
        response: &egui::Response,
    ) {
        // Was this row the current activity before the click being handled?
        let current = std::mem::take(&mut self.grid.cells.row_current);
        self.grid.cells.constraint_hint = None;
        let cache = self.frame_cache();
        let rect = snap(rect);
        let table = Rect::from_min_max(
            rect.min,
            Pos2::new(
                self.columns_right(rect.left(), rect.left() + table_w),
                rect.bottom(),
            ),
        );
        let p = painter.with_clip_rect(table.intersect(painter.clip_rect()));
        let grouped = !self.grid.bands.borrow().is_empty();
        let depth = if self.grid_indents() { depth } else { 0 };
        let content = self.paint_strips(&p, table, depth, selected);
        // Columns scroll under the strips' edge: nothing of them is drawn
        // or hit left of `content`.
        let cols = p.with_clip_rect(
            Rect::from_min_max(Pos2::new(content, table.top()), table.max).intersect(p.clip_rect()),
        );
        let opts = self.grid.layout.table.clone();
        if !selected && opts.cell_color != [255, 255, 255] {
            chrome::fill(
                &p,
                Rect::from_min_max(Pos2::new(content, table.top()), table.max),
                color(opts.cell_color),
            );
        }
        if !selected && self.grid.highlighted.borrow().contains(&i) {
            chrome::fill(
                &p,
                Rect::from_min_max(Pos2::new(content, table.top()), table.max),
                HIGHLIGHT,
            );
        }
        chrome::hline(&p, content, table.right(), table.bottom() - 1., LINE);
        let ink = if selected {
            Color32::WHITE
        } else {
            color(opts.font.color)
        };
        let pointer = ctx.input(|v| v.pointer.hover_pos());
        let x0 = (rect.left() - self.grid.scroll_x).round();
        let cell_at = |x: f32| {
            (0..cache.cols.len()).find(|&n| x >= x0 + cache.edges[n] && x < x0 + cache.edges[n + 1])
        };
        if response.clicked() || response.double_clicked() {
            if let Some(slot) = response
                .interact_pointer_pos()
                .filter(|pos| pos.x >= content)
                .and_then(|pos| cell_at(pos.x))
            {
                let column = &cache.cols[slot];
                let focused_before = self
                    .grid
                    .focus_column
                    .as_deref()
                    .filter(|id| cache.cols.iter().any(|c| c.id == *id))
                    .unwrap_or(cache.cols[0].id.as_str())
                    == column.id;
                let plain = ctx.input(|x| x.modifiers.is_none());
                self.grid.focus_column = Some(column.id.clone());
                // A click on the focused cell of the current row edits it.
                let again = response.clicked()
                    && current
                    && plain
                    && focused_before
                    && self.cell_editable(i, column).is_ok();
                if response.double_clicked() || again {
                    self.begin_edit(i, column);
                }
            }
        }
        let focus = self
            .grid
            .focus_column
            .clone()
            .filter(|id| cache.cols.iter().any(|c| &c.id == id))
            .or_else(|| cache.cols.first().map(|c| c.id.clone()));
        for (slot, column) in cache.cols.iter().enumerate() {
            let mut full = Rect::from_min_max(
                Pos2::new(x0 + cache.edges[slot], rect.top()),
                Pos2::new(x0 + cache.edges[slot + 1], rect.bottom()),
            );
            if !full.intersects(table) {
                continue;
            }
            chrome::vline(&cols, full.right() - 1., full.top(), full.bottom(), LINE);
            if slot == 0 {
                // The first column holds the strips and, when grouped, the
                // gutter under the band's expand box; both scroll away with
                // it (the strips themselves stay, drawn above).
                full.min.x += content - rect.left() + if grouped { GUTTER } else { 0. };
            }
            // The cell inside its lines: focus outline and editor area.
            let inner = Rect::from_min_max(full.min, full.max - Vec2::splat(1.));
            let mut cell = text_box(full.left(), full.right(), rect.top(), rect.bottom());
            if slot == 0 && opts.show_icons {
                let a = &self.shown().model.activities[i];
                let at = Rect::from_min_max(Pos2::new(cell.left() + 1., rect.top()), rect.max);
                crate::layout_opts::type_glyph(&cols, at, a, selected);
                cell.min.x += crate::layout_opts::GLYPH_WIDTH;
            }
            let shown = columns::shown_value(self.shown(), &cache.ix, column, i);
            // A check box column shows the box, centered, not Yes or No.
            let boxed = column.source == columns::Source::Field(columns::Field::AutoComputeActuals);
            if boxed {
                let at = Pos2::new(cell.center().x - 6., rect.center().y - 7.);
                let on = shown == columns::Value::Flag(true);
                crate::dialog_kit::paint_check_glyph(&cols, at.round(), on, true);
            }
            let text = if boxed {
                String::new()
            } else {
                columns::display(self.shown(), Some(i), &shown)
            };
            // An indicator field shows its disc before the name.
            if column.editor == Editor::Choice && matches!(column.source, columns::Source::Udf(_)) {
                if let Some((_, color)) = crate::details::record_tabs::indicator_named(&text) {
                    let center = Pos2::new(cell.left() + 7., rect.center().y);
                    crate::details::record_tabs::paint_indicator(&cols, center, color);
                    cell.min.x += 16.;
                }
            }
            // A constrained Start or Finish names its constraint on hover,
            // with or without Hint Help.
            if let columns::Source::Field(
                field @ (columns::Field::Start | columns::Field::Finish),
            ) = column.source
            {
                if pointer.is_some_and(|pos| {
                    full.contains(pos) && table.contains(pos) && pos.x >= content
                }) {
                    let end = if field == columns::Field::Start {
                        super::marks::End::Start
                    } else {
                        super::marks::End::Finish
                    };
                    let shown = self.shown();
                    self.grid.cells.constraint_hint = super::marks::tooltip(
                        shown.model.origin_serial,
                        &shown.model.activities[i],
                        end,
                    );
                }
            }
            let hint = opts.hint_help.then(|| text.clone());
            let wrap = opts
                .optimize_rows
                .filter(|_| column.kind == Kind::Text)
                .map(|lines| (cell.width() + PAD_RIGHT, lines));
            let mut g =
                crate::layout_opts::table_galley(&cols, text.clone(), &opts.font, ink, wrap);
            let room = cell.width() + PAD_RIGHT;
            let clipped = wrap.is_none() && g.size().x > room;
            if clipped {
                // Too wide for the cell: cut it with an ellipsis rather
                // than mid-glyph.
                g = crate::layout_opts::table_galley_elided(&cols, text, &opts.font, ink, room);
            }
            let x = place(cell, column.align, g.size().x);
            let y = if opts.font.is_default() || wrap.is_some() {
                rect.top() + TEXT_TOP
            } else {
                (rect.top() + (rect.height() - 1. - g.size().y) / 2.).round()
            };
            let truncated = clipped || g.elided || g.size().x > room;
            cols.with_clip_rect(
                cell.expand2(Vec2::new(PAD_LEFT, 0.))
                    .intersect(cols.clip_rect()),
            )
            .galley(Pos2::new(x, y), g, ink);
            if let Some(hint) = hint.filter(|_| truncated) {
                if pointer.is_some_and(|pos| {
                    full.contains(pos) && table.contains(pos) && pos.x >= content
                }) {
                    self.layout_opts.hint = Some(hint);
                }
            }
            if selected && self.selected == Some(i) && focus.as_deref() == Some(column.id.as_str())
            {
                focus_outline(&cols, inner);
            }
            if self
                .edit_cell
                .as_ref()
                .is_some_and(|(r, c)| *r == i && *c == column.id)
            {
                self.cell_editor(
                    ui,
                    ctx,
                    inner.intersect(cols.clip_rect()),
                    i,
                    column,
                    &cache,
                );
            }
        }
    }

    fn cell_editor(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        cell: Rect,
        i: usize,
        column: &Column,
        cache: &FrameCache,
    ) {
        let field = sunken_field(ui.painter(), cell);
        if column.editor == Editor::Choice {
            // Choices come from the activity's own project.
            let own = if self.open_set.multi() {
                std::rc::Rc::new(self.grid_index())
            } else {
                cache.ix.clone()
            };
            let choices = columns::choices(&self.project, &own, column, i);
            // The open list has the keyboard: Up and Down move the
            // highlight, Enter takes it (Escape closes, below). The main
            // window's row keys and Delete stay off meanwhile
            // (choice_editor_open).
            let (up, down, enter) = ctx.input(|v| {
                (
                    v.key_pressed(Key::ArrowUp),
                    v.key_pressed(Key::ArrowDown),
                    v.key_pressed(Key::Enter),
                )
            });
            let at = choices
                .iter()
                .position(|(k, _)| k.eq_ignore_ascii_case(&self.edit_text));
            if (up || down) && !choices.is_empty() {
                let next = match (at, down) {
                    (None, _) => 0,
                    (Some(n), true) => (n + 1).min(choices.len() - 1),
                    (Some(n), false) => n.saturating_sub(1),
                };
                self.edit_text = choices[next].0.clone();
            }
            // A focused combo box: the value highlighted inside a dotted
            // focus rectangle, the drop button at the right.
            let button = Rect::from_min_max(
                Pos2::new(field.right() - DROP_BUTTON, field.top()),
                field.max,
            );
            drop_button(ui.painter(), button);
            let value = Rect::from_min_max(
                field.min + Vec2::splat(1.),
                Pos2::new(button.left() - 1., field.bottom() - 1.),
            );
            chrome::fill(ui.painter(), value, SELECT);
            dotted_rect(ui.painter(), value);
            ui.painter().with_clip_rect(value).text(
                Pos2::new(value.left() + 2., cell.top() + TEXT_TOP),
                Align2::LEFT_TOP,
                &self.edit_text,
                FontId::proportional(FONT_SIZE),
                Color32::WHITE,
            );
            let mut picked = None;
            let current = self.edit_text.clone();
            if enter && self.error.is_none() {
                picked = choices
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case(&current))
                    .map(|(k, _)| k.clone());
            }
            let mut rects = vec![];
            let area = egui::Area::new(egui::Id::new(("grid_choice", &column.id)))
                .order(egui::Order::Foreground)
                .fixed_pos(cell.left_bottom())
                .show(ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        // Long choices widen the list instead of wrapping.
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                        ui.set_min_width(cell.width().max(120.));
                        // Sized per column and list length, so a list
                        // never opens at the size of another.
                        egui::ScrollArea::vertical()
                            .id_salt((&column.id, choices.len()))
                            .max_height(220.)
                            .show(ui, |ui| {
                                for (n, (key, label)) in choices.iter().enumerate() {
                                    let r = ui.selectable_label(
                                        key.eq_ignore_ascii_case(&current),
                                        if label.is_empty() { " " } else { label },
                                    );
                                    rects.push((format!("grid_choice_{n}"), r.clone()));
                                    if r.clicked() {
                                        picked = Some(key.clone());
                                    }
                                }
                            });
                    });
                });
            for (name, r) in rects {
                self.action(&name, &r);
            }
            let opening = std::mem::take(&mut self.edit_focus);
            if let Some(key) = picked {
                self.grid.pending_edit = Some((i, column.id.clone(), key));
                self.edit_cell = None;
            } else if ctx.input(|v| v.key_pressed(Key::Escape))
                || !opening
                    && ctx.input(|v| v.pointer.any_pressed())
                    && ctx
                        .input(|v| v.pointer.interact_pos())
                        .is_some_and(|pos| !area.response.rect.contains(pos))
            {
                self.edit_cell = None;
            }
            return;
        }
        let date_editor = column.editor == Editor::Date;
        let button = Rect::from_min_max(
            Pos2::new(field.right() - DROP_BUTTON, field.top()),
            field.max,
        );
        let text_rect = if date_editor {
            Rect::from_min_max(field.min, Pos2::new(button.left(), field.bottom()))
        } else {
            field
        };
        let edit = ui.put(
            text_rect,
            egui::TextEdit::singleline(&mut self.edit_text)
                .frame(false)
                .margin(egui::Margin::symmetric(1, 0))
                .font(FontId::proportional(FONT_SIZE))
                .id(egui::Id::new(("inline_cell", i, &column.id)))
                .horizontal_align(match column.align {
                    ColAlign::Right => Align::Max,
                    ColAlign::Center => Align::Center,
                    ColAlign::Left => Align::Min,
                }),
        );
        // A refused value's message is up: the field waits for it to close
        // (the key that closes it must not commit the value again).
        let message_up = self.error.is_some();
        if self.edit_focus && !message_up {
            if std::mem::take(&mut self.grid.cells.select_all) {
                let mut state = egui::TextEdit::load_state(ctx, edit.id).unwrap_or_default();
                state
                    .cursor
                    .set_char_range(Some(egui::text::CCursorRange::two(
                        egui::text::CCursor::new(0),
                        egui::text::CCursor::new(self.edit_text.chars().count()),
                    )));
                state.store(ctx, edit.id);
            }
            edit.request_focus();
            self.edit_focus = false;
            ctx.request_repaint();
        }
        if message_up {
            if date_editor {
                drop_button(ui.painter(), button);
            }
            return;
        }
        // Date cells: a drop button opening a month calendar under the cell.
        let mut picked = None;
        let mut keep = false;
        if date_editor {
            drop_button(ui.painter(), button);
            let drop = ui.interact(button, egui::Id::new("grid_date_button"), Sense::click());
            self.action("grid_date_button", &drop);
            if drop.clicked() {
                self.grid.cells.calendar = match self.grid.cells.calendar {
                    Some(_) => None,
                    None => Some(self.editor_date(i, column, None).date()),
                };
                self.edit_focus = true;
            }
            let mut popup = None;
            if let Some(month) = self.grid.cells.calendar {
                let current = self.editor_date(i, column, None);
                let (area, chosen) = self.calendar_popup(ctx, cell, month, current.date());
                popup = Some(area);
                picked = chosen.map(|day| {
                    self.editor_date(i, column, Some(day))
                        .format("%Y-%m-%d %H:%M")
                        .to_string()
                });
            }
            let at = ctx.input(|v| v.pointer.interact_pos());
            keep = at.is_some_and(|pos| {
                button.contains(pos) || popup.is_some_and(|r: Rect| r.contains(pos))
            });
        }
        let tab = ctx.input(|v| v.key_pressed(Key::Tab));
        if ctx.input(|v| v.key_pressed(Key::Escape)) {
            if self.grid.cells.calendar.take().is_some() {
                self.edit_focus = true;
            } else {
                self.edit_cell = None;
            }
        } else if let Some(text) = picked {
            self.grid.pending_edit = Some((i, column.id.clone(), text));
            self.grid.cells.calendar = None;
            self.edit_cell = None;
        } else if edit.lost_focus() && keep {
            // The calendar took the click; typing goes on in the field.
            self.edit_focus = true;
        } else if edit.lost_focus() || ctx.input(|v| v.key_pressed(Key::Enter)) {
            self.grid.pending_edit = Some((i, column.id.clone(), self.edit_text.clone()));
            self.grid.cells.calendar = None;
            self.edit_cell = None;
            if tab {
                // Tab commits and moves on to the next cell.
                let back = ctx.input(|v| v.modifiers.shift);
                self.grid.cells.after_commit = Some(if back { -1 } else { 1 });
                self.grid.cells.tab_used = true;
            }
        }
    }

    /// The date and time the date editor of activity `i` holds now: the
    /// typed text, else the data date, at the start of the activity
    /// calendar's working day (its end for a finish-type field, constraint
    /// dates by constraint type). With `day`, that day at the typed time, or
    /// at the start or end of that working day.
    fn editor_date(
        &self,
        i: usize,
        column: &Column,
        day: Option<chrono::NaiveDate>,
    ) -> chrono::NaiveDateTime {
        let origin = self.project.model.origin_serial;
        let finish = match (&column.source, self.project.model.activities.get(i)) {
            (columns::Source::Field(f), Some(a)) => super::edit::finish_type(*f, a),
            _ => false,
        };
        // Typed text without a time keeps the cell's time, as a commit does.
        let current = match columns::value(&self.project, &self.grid_index(), column, i) {
            columns::Value::Date { minute, .. } => Some(minute),
            _ => None,
        };
        let typed = self
            .parse_field_date(
                i,
                finish,
                format::strip_date_marker(&self.edit_text),
                current,
            )
            .ok()
            .flatten();
        let on_day = day.and_then(|d| {
            openxer_xer::parse_relative(origin, &format!("{} 00:00", d.format("%Y-%m-%d")))
                .ok()
                .flatten()
        });
        let minute = match (typed, on_day) {
            (Some(t), Some(d)) => d + t.rem_euclid(DAY),
            (Some(t), None) => t,
            (None, d) => {
                let day = d
                    .unwrap_or_else(|| self.project.model.project.data_date.unwrap_or(0))
                    .div_euclid(DAY);
                day * DAY + self.day_edge(i, finish, day)
            }
        };
        chrono::NaiveDateTime::parse_from_str(&date_string(origin, Some(minute)), "%Y-%m-%d %H:%M")
            .unwrap_or_default()
    }

    /// The date editor's month calendar under `cell`: month arrows and a
    /// grid of days from Sunday. Returns the popup's rect and a clicked day.
    fn calendar_popup(
        &mut self,
        ctx: &egui::Context,
        cell: Rect,
        month: chrono::NaiveDate,
        current: chrono::NaiveDate,
    ) -> (Rect, Option<chrono::NaiveDate>) {
        use chrono::Datelike;
        const DAY_W: f32 = 24.;
        const DAY_H: f32 = 18.;
        let first = month.with_day(1).unwrap_or(month);
        let mut picked = None;
        let mut shift = 0;
        let mut actions: Vec<(String, Rect)> = vec![];
        let area = egui::Area::new(egui::Id::new("grid_calendar"))
            .order(egui::Order::Foreground)
            .fixed_pos(cell.left_bottom())
            .show(ctx, |ui| {
                let size = Vec2::new(7. * DAY_W + 2., 2. * DAY_H + 6. * DAY_H + 2.);
                let (r, _) = ui.allocate_exact_size(size, Sense::hover());
                let p = ui.painter();
                chrome::fill(p, r, Color32::WHITE);
                chrome::hline(p, r.left(), r.right(), r.top(), chrome::pal::SHADOW);
                chrome::hline(p, r.left(), r.right(), r.bottom() - 1., chrome::pal::SHADOW);
                chrome::vline(p, r.left(), r.top(), r.bottom(), chrome::pal::SHADOW);
                chrome::vline(p, r.right() - 1., r.top(), r.bottom(), chrome::pal::SHADOW);
                let font = FontId::proportional(FONT_SIZE);
                let head =
                    Rect::from_min_size(r.min + Vec2::splat(1.), Vec2::new(7. * DAY_W, DAY_H));
                chrome::fill(p, head, chrome::pal::FACE);
                p.text(
                    head.center(),
                    Align2::CENTER_CENTER,
                    first.format("%B %Y").to_string(),
                    font.clone(),
                    INK,
                );
                for (name, step, at) in [
                    ("grid_calendar_prev", -1, head.left() + 2.),
                    ("grid_calendar_next", 1, head.right() - DAY_W + 2.),
                ] {
                    let b = Rect::from_min_size(
                        Pos2::new(at, head.top()),
                        Vec2::new(DAY_W - 4., DAY_H),
                    );
                    let c = b.center();
                    for k in 0..4 {
                        let k = k as f32;
                        // The tallest column is the arrow's base.
                        let x = if step < 0 { c.x + 1. - k } else { c.x - 2. + k };
                        chrome::vline(p, x, c.y - 3. + k, c.y + 4. - k, INK);
                    }
                    if ui.interact(b, ui.id().with(name), Sense::click()).clicked() {
                        shift = step;
                    }
                    actions.push((name.to_string(), b));
                }
                let top = head.bottom();
                for (n, day) in ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"]
                    .iter()
                    .enumerate()
                {
                    p.text(
                        Pos2::new(head.left() + (n as f32 + 0.5) * DAY_W, top + DAY_H / 2.),
                        Align2::CENTER_CENTER,
                        *day,
                        font.clone(),
                        chrome::pal::SHADOW,
                    );
                }
                chrome::hline(
                    p,
                    head.left() + 2.,
                    head.right() - 2.,
                    top + DAY_H - 1.,
                    chrome::pal::LIGHT,
                );
                let lead = first.weekday().num_days_from_sunday() as i64;
                let mut day = first - chrono::Duration::days(lead);
                for row in 0..6 {
                    for col in 0..7 {
                        let b = Rect::from_min_size(
                            Pos2::new(
                                head.left() + col as f32 * DAY_W,
                                top + DAY_H * (row + 1) as f32,
                            ),
                            Vec2::new(DAY_W, DAY_H),
                        );
                        let ink = if day == current {
                            chrome::fill(p, b.shrink(1.), SELECT);
                            Color32::WHITE
                        } else if day.month() == first.month() {
                            INK
                        } else {
                            chrome::pal::SHADOW
                        };
                        p.text(
                            b.center(),
                            Align2::CENTER_CENTER,
                            day.day().to_string(),
                            font.clone(),
                            ink,
                        );
                        let key = format!("grid_calendar_{}", day.format("%Y-%m-%d"));
                        if ui.interact(b, ui.id().with(&key), Sense::click()).clicked() {
                            picked = Some(day);
                        }
                        actions.push((key, b));
                        day += chrono::Duration::days(1);
                    }
                }
            });
        for (name, r) in actions {
            self.action_rects
                .insert(name, [r.left(), r.top(), r.right(), r.bottom()]);
        }
        if shift != 0 {
            self.grid.cells.calendar = first
                .checked_add_months(chrono::Months::new(1))
                .filter(|_| shift > 0)
                .or_else(|| {
                    first
                        .checked_sub_months(chrono::Months::new(1))
                        .filter(|_| shift < 0)
                });
        }
        (area.response.rect, picked)
    }
}
