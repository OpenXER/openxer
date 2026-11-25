//! The histogram and cumulative curve painter of the usage profiles, shared
//! with the S-Curve report so both draw the same chart: Y axis labels with
//! dashed gridlines, a picked period, the bars (one slot per series in each
//! period, each slot a stack), the limit as a stepped line, the cumulative
//! curves on their own scale labelled at the right edge, markers on that
//! scale, the data date line, and the legend box.
//!
//! The painters take only what they draw: the usage panes add their header
//! button, nonwork shading, the timescale and the clicks around them.
use super::usage::{LINE, PICKED};
use crate::chrome::{self, BLUE, INK};
use crate::timescale::TimeScale;
use crate::views::resources as rs;
use eframe::egui::{self, Align2, Color32, Painter, Pos2, Rect, Stroke, Vec2};

/// Room above the tallest bar for the legend.
const HEADROOM: f32 = 24.;

/// One bar segment: value range and colors (fill, outline).
#[derive(Clone, Copy, Debug)]
pub struct Segment {
    pub from: f64,
    pub to: f64,
    pub colors: (Color32, Color32),
}

/// A cumulative curve over the bars: the running total at the end of each
/// period, drawn on its own scale at the right of the chart.
#[derive(Clone, Debug, PartialEq)]
pub struct Curve {
    /// Running totals, one per period.
    pub totals: Vec<f64>,
    pub color: Color32,
    /// Drawn dashed, to tell a series from another of the same color.
    pub dashed: bool,
}

/// A point on the curves' scale: a diamond at model minute `at`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Marker {
    pub at: i64,
    pub value: f64,
    pub color: Color32,
}

/// What [`paint_plot`] draws.
pub struct Plot<'a> {
    pub axis: &'a TimeScale,
    pub periods: &'a [(i64, i64)],
    pub bars: &'a [Vec<Vec<Segment>>],
    pub limit: Option<&'a [f64]>,
    pub curves: &'a [Curve],
    pub markers: &'a [Marker],
    /// A period shaded behind the bars.
    pub picked: Option<(i64, i64)>,
    pub data_date: Option<i64>,
    /// The text of a value on either scale.
    pub quantity: &'a dyn Fn(f64) -> String,
    pub style: Style,
}

/// How the plot is drawn (the profiles' Graph options).
#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    /// Horizontal lines at the left scale's steps: their color, dotted or
    /// solid; `None` for none.
    pub grid: Option<(Color32, bool)>,
    /// Bars drawn with a top and a side, as blocks.
    pub bars_3d: bool,
    /// Each bar's value written over it.
    pub values: bool,
    /// A caption over the left scale (the average's unit of measure).
    pub unit: String,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            grid: Some((LINE, true)),
            bars_3d: false,
            values: false,
            unit: String::new(),
        }
    }
}

/// The bars of a plot: per period, the slots, each a stack of segments.
pub type Bars = Vec<Vec<Vec<Segment>>>;

/// Bars and limit with each period's values divided by that period's
/// divisor (the profiles' average).
pub fn divided(
    bars: &[Vec<Vec<Segment>>],
    limit: Option<&[f64]>,
    divisors: &[f64],
) -> (Bars, Option<Vec<f64>>) {
    let by = |n: usize| divisors.get(n).copied().filter(|d| *d > 0.).unwrap_or(1.);
    let bars = bars
        .iter()
        .enumerate()
        .map(|(n, slots)| {
            slots
                .iter()
                .map(|stack| {
                    stack
                        .iter()
                        .map(|s| Segment {
                            from: s.from / by(n),
                            to: s.to / by(n),
                            colors: s.colors,
                        })
                        .collect()
                })
                .collect()
        })
        .collect();
    let limit = limit.map(|l| l.iter().enumerate().map(|(n, v)| v / by(n)).collect());
    (bars, limit)
}

/// A shade of `c`: lighter above 1, darker below.
fn shade(c: Color32, f: f32) -> Color32 {
    let ch = |v: u8| {
        let v = v as f32;
        if f >= 1. {
            v + (255. - v) * (f - 1.)
        } else {
            v * f
        }
        .clamp(0., 255.) as u8
    };
    Color32::from_rgb(ch(c.r()), ch(c.g()), ch(c.b()))
}

/// One legend entry: label, colors, and whether it stands for a line.
pub type LegendEntry<'a> = (&'a str, (Color32, Color32), bool);

/// The plot inside `body` (the white area right of the Y axis labels); the
/// labels are drawn with `p`, which must reach left of `body`.
pub fn paint_plot(p: &Painter, body: Rect, c: &Plot) {
    let bp = p.with_clip_rect(body);
    let axis = c.axis;
    let peak = c
        .bars
        .iter()
        .flat_map(|slots| slots.iter().flatten().map(|s| s.to))
        .chain(c.limit.unwrap_or(&[]).iter().copied())
        // A resource with no rate row has no limit (infinite): it must
        // not set the scale, or the ticks would never end.
        .filter(|v| v.is_finite())
        .fold(0., f64::max);
    let step = rs::nice_step(peak);
    let top = (peak / step).ceil().max(1.) * step;
    let plot = Rect::from_min_max(Pos2::new(body.left(), body.top() + HEADROOM), body.max);
    let y = |v: f64| (plot.bottom() - (v / top) as f32 * plot.height()).round();
    let mut tick = step;
    while tick <= top + step * 0.01 {
        let at = y(tick);
        match c.style.grid {
            Some((color, true)) => {
                let mut x = body.left();
                while x < body.right() {
                    chrome::hline(&bp, x, (x + 2.).min(body.right()), at, color);
                    x += 4.;
                }
            }
            Some((color, false)) => chrome::hline(&bp, body.left(), body.right(), at, color),
            None => {}
        }
        p.text(
            Pos2::new(body.left() - 4., at),
            Align2::RIGHT_CENTER,
            (c.quantity)(tick),
            chrome::table_font(),
            INK,
        );
        tick += step;
    }
    if !c.style.unit.is_empty() {
        p.text(
            Pos2::new(body.left() - 4., body.top() + 2.),
            Align2::RIGHT_TOP,
            &c.style.unit,
            chrome::table_font(),
            INK,
        );
    }
    if let Some((a, b)) = c.picked {
        let x0 = axis.x(a as f64).round();
        let x1 = axis.x(b as f64).round();
        bp.rect_filled(
            Rect::from_min_max(Pos2::new(x0, body.top()), Pos2::new(x1, body.bottom())),
            0.,
            PICKED,
        );
    }
    for (n, &(start, end)) in c.periods.iter().enumerate() {
        let Some(slots) = c.bars.get(n) else {
            continue;
        };
        let x0 = axis.x(start as f64).round() + 1.;
        let x1 = axis.x(end as f64).round() - 1.;
        let count = slots.len().max(1) as f32;
        let w = ((x1 - x0) / count).floor();
        if w < 1. {
            continue;
        }
        // 3D: each bar is narrower by its depth, with a lighter top and a
        // darker side drawn back and up.
        let depth = if c.style.bars_3d {
            (w * 0.3).clamp(0., 6.).floor()
        } else {
            0.
        };
        for (k, stack) in slots.iter().enumerate() {
            let sx = x0 + k as f32 * w;
            let face = w - depth;
            for s in stack {
                if s.to - s.from <= 1e-9 {
                    continue;
                }
                let r = Rect::from_min_max(Pos2::new(sx, y(s.to)), Pos2::new(sx + face, y(s.from)));
                bp.rect_filled(r, 0., s.colors.0);
                if depth >= 1. {
                    let d = Vec2::new(depth, -depth);
                    bp.add(egui::Shape::convex_polygon(
                        vec![
                            r.left_top(),
                            r.left_top() + d,
                            r.right_top() + d,
                            r.right_top(),
                        ],
                        shade(s.colors.0, 1.35),
                        Stroke::new(1_f32, s.colors.1),
                    ));
                    bp.add(egui::Shape::convex_polygon(
                        vec![
                            r.right_top(),
                            r.right_top() + d,
                            r.right_bottom() + d,
                            r.right_bottom(),
                        ],
                        shade(s.colors.0, 0.7),
                        Stroke::new(1_f32, s.colors.1),
                    ));
                }
                if face >= 3. {
                    bp.rect_stroke(
                        r,
                        0.,
                        Stroke::new(1_f32, s.colors.1),
                        egui::StrokeKind::Inside,
                    );
                }
            }
            // Show Values: the top of the bar, written over it.
            let peak = stack.iter().map(|s| s.to).fold(0., f64::max);
            if c.style.values && peak > 1e-9 {
                bp.text(
                    Pos2::new(sx + face / 2., y(peak) - depth - 1.),
                    Align2::CENTER_BOTTOM,
                    (c.quantity)(peak),
                    chrome::table_font(),
                    INK,
                );
            }
        }
        // No line where there is no limit.
        if let Some(limit) = c.limit.filter(|l| l[n].is_finite()) {
            let ly = y(limit[n]);
            let px0 = axis.x(start as f64).round();
            let px1 = axis.x(end as f64).round();
            chrome::hline(&bp, px0, px1, ly, INK);
            if n + 1 < limit.len() && limit[n + 1].is_finite() {
                let next = y(limit[n + 1]);
                chrome::vline(&bp, px1, ly.min(next), ly.max(next) + 1., INK);
            }
        }
    }
    // Cumulative: a running-total curve per series over the bars, on its
    // own scale labelled at the right edge.
    let peak_total = c
        .curves
        .iter()
        .flat_map(|c| c.totals.iter().copied())
        .chain(c.markers.iter().map(|m| m.value))
        .filter(|v| v.is_finite())
        .fold(0., f64::max);
    if !c.curves.is_empty() {
        let step = rs::nice_step(peak_total);
        let top = (peak_total / step).ceil().max(1.) * step;
        let y2 = |v: f64| (plot.bottom() - (v / top) as f32 * plot.height()).round();
        let mut tick = step;
        while tick <= top + step * 0.01 {
            let at = y2(tick);
            chrome::hline(&bp, body.right() - 4., body.right(), at, INK);
            p.text(
                Pos2::new(body.right() - 7., at),
                Align2::RIGHT_CENTER,
                (c.quantity)(tick),
                chrome::table_font(),
                INK,
            );
            tick += step;
        }
        for curve in c.curves {
            let first = c
                .periods
                .first()
                .map_or(0., |(a, _)| axis.x(*a as f64).round());
            let mut points = vec![Pos2::new(first + 0.5, y2(0.) + 0.5)];
            for (&(_, end), &total) in c.periods.iter().zip(&curve.totals) {
                points.push(Pos2::new(axis.x(end as f64).round() + 0.5, y2(total) + 0.5));
            }
            let stroke = Stroke::new(2_f32, curve.color);
            if curve.dashed {
                bp.extend(egui::Shape::dashed_line(&points, stroke, 5., 3.));
            } else {
                bp.add(egui::Shape::line(points, stroke));
            }
        }
        for m in c.markers {
            let center = Pos2::new(axis.x(m.at as f64).round() + 0.5, y2(m.value) + 0.5);
            let r = 5.;
            bp.add(egui::Shape::convex_polygon(
                vec![
                    center + Vec2::new(0., -r),
                    center + Vec2::new(r, 0.),
                    center + Vec2::new(0., r),
                    center + Vec2::new(-r, 0.),
                ],
                m.color,
                Stroke::new(1_f32, INK),
            ));
        }
    }
    if let Some(dd) = c.data_date {
        let x = axis.x(dd as f64).round();
        chrome::vline(&bp, x, body.top(), body.bottom(), BLUE);
    }
}

/// The legend: a white box with a black border, its top left at `at`, with
/// an optional bold title and a swatch (or a line) per entry.
pub fn paint_legend(p: &Painter, at: Pos2, title: &str, legend: &[LegendEntry]) {
    let font = chrome::table_font();
    let title_h = if title.is_empty() { 0. } else { 20. };
    let width = legend
        .iter()
        .map(|(label, _, _)| {
            p.layout_no_wrap((*label).to_string(), font.clone(), INK)
                .size()
                .x
        })
        .fold(0., f32::max)
        .max(if title.is_empty() {
            0.
        } else {
            p.layout_no_wrap(title.to_string(), chrome::bold_font(), INK)
                .size()
                .x
                - 10.
        })
        + 30.;
    let boxed = Rect::from_min_size(
        at,
        Vec2::new(width, title_h + 20. * legend.len() as f32 + 6.),
    );
    p.rect_filled(boxed, 0., Color32::WHITE);
    p.rect_stroke(boxed, 0., Stroke::new(1_f32, INK), egui::StrokeKind::Inside);
    if !title.is_empty() {
        p.text(
            Pos2::new(boxed.center().x, boxed.top() + 12.),
            Align2::CENTER_CENTER,
            title,
            chrome::bold_font(),
            INK,
        );
    }
    for (k, (label, colors, line)) in legend.iter().enumerate() {
        let cy = boxed.top() + title_h + 13. + 20. * k as f32;
        let swatch = Rect::from_min_size(Pos2::new(boxed.left() + 6., cy - 5.), Vec2::splat(10.));
        p.rect_filled(swatch, 0., colors.0);
        if !*line {
            p.rect_stroke(
                swatch,
                0.,
                Stroke::new(1_f32, colors.1),
                egui::StrokeKind::Inside,
            );
        }
        p.text(
            Pos2::new(boxed.left() + 20., cy),
            Align2::LEFT_CENTER,
            *label,
            font.clone(),
            INK,
        );
    }
}

#[cfg(test)]
mod tests {
    /// The cumulative curves' second scale (`y2`) is drawn here only: a
    /// chart with running totals calls `paint_plot` instead of painting
    /// its own.
    #[test]
    fn one_histogram_painter() {
        fn walk(dir: &std::path::Path, out: &mut Vec<String>) {
            for entry in std::fs::read_dir(dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, out);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    let name = path.file_name().unwrap().to_string_lossy().to_string();
                    let code = std::fs::read_to_string(&path).unwrap_or_default();
                    if name != "histogram.rs"
                        && !name.ends_with("_tests.rs")
                        && (code.contains("peak_total") || code.contains("let y2 ="))
                    {
                        out.push(path.display().to_string());
                    }
                }
            }
        }
        let mut found = vec![];
        walk(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut found,
        );
        assert!(found.is_empty(), "a second histogram painter: {found:?}");
    }
}
