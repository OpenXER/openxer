//! Pixel-exact painting helpers for the chart. The reference draws with
//! aliased one-pixel pens, so lines, dashes and small shapes are emitted as
//! whole-pixel rectangles instead of anti-aliased strokes.
use super::settings::LineStyle;
use eframe::egui::{self, Color32, FontId, Galley, Painter, Pos2, Rect};
use std::cell::Cell;
use std::sync::Arc;

thread_local! {
    /// Set while a printout is captured: long dashed lines are then kept as
    /// one `DashRun` shape each instead of one mesh per dash, which on a big
    /// project came to millions of shapes.
    static COMPACT_DASHES: Cell<bool> = const { Cell::new(false) };
}

/// Keeps `COMPACT_DASHES` on while it lives.
pub struct CompactDashes(bool);

/// Paint long dashed lines as `DashRun` shapes until the guard drops.
pub fn compact_dashes() -> CompactDashes {
    #[cfg(test)]
    let on = !PLAIN_DASHES.get();
    #[cfg(not(test))]
    let on = true;
    CompactDashes(COMPACT_DASHES.replace(on))
}

pub fn is_compact_dashes() -> bool {
    COMPACT_DASHES.get()
}

#[cfg(test)]
thread_local! {
    /// Tests: paint every dash as its own mesh, as the screen does.
    pub static PLAIN_DASHES: Cell<bool> = const { Cell::new(false) };
}

impl Drop for CompactDashes {
    fn drop(&mut self) {
        COMPACT_DASHES.set(self.0);
    }
}

/// A dashed line on one pixel row or column, as the rectangles `hline` and
/// `vline` would paint one by one. The print capture expands it again in
/// the same order, so the printed ops are the same.
#[derive(Clone, Debug)]
pub struct DashRun {
    pub vertical: bool,
    /// The pixel column (vertical) or row (horizontal).
    pub at: f32,
    pub width: f32,
    pub style: LineStyle,
    pub origin: f32,
    pub start: f32,
    pub end: f32,
    pub color: Color32,
}

impl DashRun {
    /// Each painted rectangle, in paint order.
    pub fn rects(&self, mut each: impl FnMut(Rect)) {
        dash_runs(self.style, self.origin, self.start, self.end, |a, b| {
            let (x0, y0, x1, y1) = if self.vertical {
                (self.at, a, self.at + self.width, b)
            } else {
                (a, self.at, b, self.at + self.width)
            };
            if x1 > x0 && y1 > y0 {
                each(Rect::from_min_max(Pos2::new(x0, y0), Pos2::new(x1, y1)));
            }
        });
    }

    fn bounds(&self) -> Rect {
        if self.vertical {
            Rect::from_min_max(
                Pos2::new(self.at, self.start),
                Pos2::new(self.at + self.width, self.end),
            )
        } else {
            Rect::from_min_max(
                Pos2::new(self.start, self.at),
                Pos2::new(self.end, self.at + self.width),
            )
        }
    }

    /// Adds the run to `painter` when the capture asked for compact dashes
    /// and the line is dashed; false when the caller paints it itself.
    fn add_compact(self, painter: &Painter) -> bool {
        if !COMPACT_DASHES.get() || self.style.dashes().is_empty() {
            return false;
        }
        painter.add(egui::Shape::Callback(egui::PaintCallback {
            rect: self.bounds(),
            callback: Arc::new(self),
        }));
        true
    }
}

/// The chart's text: Arial at the given point size (96 DPI).
pub fn arial(points: f32) -> FontId {
    FontId::new(points * 96. / 72., egui::FontFamily::Name("Arial".into()))
}

/// Fills the pixel columns `x0..x1` (exclusive) of the pixel rows `y0..y1`.
/// Painted as a plain mesh: egui feathers thin rectangles into lines, which
/// would smear one-pixel shapes over two columns.
pub fn fill(painter: &Painter, x0: f32, y0: f32, x1: f32, y1: f32, color: Color32) {
    if x1 > x0 && y1 > y0 {
        let mut mesh = egui::Mesh::default();
        mesh.add_colored_rect(
            Rect::from_min_max(Pos2::new(x0, y0), Pos2::new(x1, y1)),
            color,
        );
        painter.add(egui::Shape::mesh(mesh));
    }
}

/// Runs of one style: alternating on and off lengths, starting with on.
fn runs(style: LineStyle) -> &'static [f32] {
    style.dashes()
}

/// Calls `on(a, b)` for each drawn run inside `start..end`, with the pattern
/// anchored at `origin` (the reference starts dashes at the chart's left edge
/// and vertical dashes at the body top).
fn dash_runs(style: LineStyle, origin: f32, start: f32, end: f32, mut on: impl FnMut(f32, f32)) {
    let pattern = runs(style);
    if pattern.is_empty() {
        on(start, end);
        return;
    }
    let period: f32 = pattern.iter().sum();
    let mut cursor = origin + ((start - origin) / period).floor() * period;
    while cursor < end {
        let mut at = cursor;
        for (k, length) in pattern.iter().enumerate() {
            if k % 2 == 0 {
                let a = at.max(start);
                let b = (at + length).min(end);
                if b > a {
                    on(a, b);
                }
            }
            at += length;
        }
        cursor += period;
    }
}

/// A horizontal line on the pixel row `y` from `x0` to `x1` (exclusive).
#[allow(clippy::too_many_arguments)]
pub fn hline(
    painter: &Painter,
    x0: f32,
    x1: f32,
    y: f32,
    width: f32,
    color: Color32,
    style: LineStyle,
    origin: f32,
) {
    let run = DashRun {
        vertical: false,
        at: y,
        width,
        style,
        origin,
        start: x0,
        end: x1,
        color,
    };
    if !run.clone().add_compact(painter) {
        dash_runs(style, origin, x0, x1, |a, b| {
            fill(painter, a, y, b, y + width, color)
        });
    }
}

/// A vertical line on the pixel column `x` from `y0` to `y1` (exclusive).
#[allow(clippy::too_many_arguments)]
pub fn vline(
    painter: &Painter,
    x: f32,
    y0: f32,
    y1: f32,
    width: f32,
    color: Color32,
    style: LineStyle,
    origin: f32,
) {
    let run = DashRun {
        vertical: true,
        at: x,
        width,
        style,
        origin,
        start: y0,
        end: y1,
        color,
    };
    if !run.clone().add_compact(painter) {
        dash_runs(style, origin, y0, y1, |a, b| {
            fill(painter, x, a, x + width, b, color)
        });
    }
}

/// One painted pixel row of a small shape: `y`, first and last column
/// (inclusive).
pub type Span = (f32, f32, f32);

/// Paints spans in one color.
pub fn spans(painter: &Painter, spans: &[Span], color: Color32) {
    let mut mesh = egui::Mesh::default();
    spans_into(&mut mesh, spans, color);
    if !mesh.is_empty() {
        painter.add(egui::Shape::mesh(mesh));
    }
}

/// Appends spans in one color into an existing triangle mesh.
pub fn spans_into(mesh: &mut egui::Mesh, spans: &[Span], color: Color32) {
    for &(y, a, b) in spans {
        mesh.add_colored_rect(
            Rect::from_min_max(Pos2::new(a, y), Pos2::new(b + 1., y + 1.)),
            color,
        );
    }
}

/// Paints spans with a one-pixel outline: the first and last pixel of each
/// row, and the whole first and last row, take the outline color.
pub fn outlined_spans(painter: &Painter, spans: &[Span], fill_color: Color32, outline: Color32) {
    for (k, &(y, a, b)) in spans.iter().enumerate() {
        if k == 0 || k + 1 == spans.len() || b - a < 2. {
            fill(painter, a, y, b + 1., y + 1., outline);
        } else {
            fill(painter, a, y, a + 1., y + 1., outline);
            fill(painter, a + 1., y, b, y + 1., fill_color);
            fill(painter, b, y, b + 1., y + 1., outline);
        }
    }
}

/// The milestone diamond: nine rows 1, 3, 5, 7, 9, 7, 5, 3, 1 pixels wide,
/// centered on the column `x`, starting on the pixel row `top`.
pub fn diamond(x: f32, top: f32) -> Vec<Span> {
    (0..9i32)
        .map(|k| {
            let half = 4 - (k - 4i32).abs();
            (top + k as f32, x - half as f32, x + half as f32)
        })
        .collect()
}

/// An axis-aligned pixel path through the pixel centres `points` (column,
/// row), one pixel wide. Dash patterns restart on each segment.
pub fn pixel_path(painter: &Painter, points: &[(f32, f32)], color: Color32, style: LineStyle) {
    for pair in points.windows(2) {
        let ((ax, ay), (bx, by)) = (pair[0], pair[1]);
        if ay == by {
            let (a, b) = if ax <= bx { (ax, bx) } else { (bx, ax) };
            hline(painter, a, b + 1., ay, 1., color, style, ax.min(bx));
        } else {
            let (a, b) = if ay <= by { (ay, by) } else { (by, ay) };
            vline(painter, ax, a, b + 1., 1., color, style, a);
        }
    }
}

/// Appends an axis-aligned pixel path into an existing triangle mesh.
pub fn pixel_path_into(
    mesh: &mut egui::Mesh,
    points: &[(f32, f32)],
    color: Color32,
    style: LineStyle,
) {
    for pair in points.windows(2) {
        let ((ax, ay), (bx, by)) = (pair[0], pair[1]);
        if ay == by {
            let (x0, x1) = if ax <= bx { (ax, bx) } else { (bx, ax) };
            dash_runs(style, x0, x0, x1 + 1., |a, b| {
                if b > a {
                    mesh.add_colored_rect(
                        Rect::from_min_max(Pos2::new(a, ay), Pos2::new(b, ay + 1.)),
                        color,
                    );
                }
            });
        } else {
            let (y0, y1) = if ay <= by { (ay, by) } else { (by, ay) };
            dash_runs(style, y0, y0, y1 + 1., |a, b| {
                if b > a {
                    mesh.add_colored_rect(
                        Rect::from_min_max(Pos2::new(ax, a), Pos2::new(ax + 1., b)),
                        color,
                    );
                }
            });
        }
    }
}

/// A solid arrowhead seven rows tall and four columns long whose tip is the
/// pixel (`x`, `y`), pointing right or left.
pub fn arrowhead(x: f32, y: f32, pointing_right: bool) -> Vec<Span> {
    (-3..=3)
        .map(|d: i32| {
            let length = 3. - d.abs() as f32;
            if pointing_right {
                (y + d as f32, x - length, x)
            } else {
                (y + d as f32, x, x + length)
            }
        })
        .collect()
}

/// Text whose first line's baseline sits on `baseline`, starting at `x`.
pub fn text_at_baseline(
    painter: &Painter,
    x: f32,
    baseline: f32,
    galley: Arc<Galley>,
    color: Color32,
) {
    let ascent = galley
        .rows
        .first()
        .and_then(|row| row.glyphs.first())
        .map_or(galley.size().y * 0.8, |glyph| glyph.pos.y);
    painter.galley(Pos2::new(x, baseline - ascent), galley, color);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashes_start_at_their_origin_and_clip_to_the_range() {
        let mut runs = vec![];
        dash_runs(LineStyle::Dashed, 666., 666., 680., |a, b| {
            runs.push((a, b))
        });
        assert_eq!(
            runs,
            [(666., 669.), (672., 675.), (678., 680.)],
            "3 on, 3 off from the chart's left edge"
        );
        let mut runs = vec![];
        dash_runs(LineStyle::Dashed, 226., 244., 250., |a, b| {
            runs.push((a, b))
        });
        assert_eq!(runs, [(244., 247.)], "phase carries across rows");
        let mut runs = vec![];
        dash_runs(LineStyle::Solid, 0., 5., 9., |a, b| runs.push((a, b)));
        assert_eq!(runs, [(5., 9.)]);
    }

    #[test]
    fn diamond_rows_widen_to_nine_pixels() {
        let rows = diamond(712., 630.);
        let widths: Vec<f32> = rows.iter().map(|(_, a, b)| b - a + 1.).collect();
        assert_eq!(widths, [1., 3., 5., 7., 9., 7., 5., 3., 1.]);
        assert_eq!(rows[4], (634., 708., 716.));
    }
}
