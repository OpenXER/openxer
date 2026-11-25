//! Right-hand panes for the view tables: a summary Gantt chart (Projects and
//! WBS views) on a month over week timescale, and the two-tier timescale
//! header the Resource Assignments window's spreadsheet
//! (`panes::sheet_pane`) draws too. They reuse the application's timescale
//! sweep (`timescale::cells_with_options`) for unit boundaries and draw
//! their own labels in the views' formats.
use super::super::chrome::{CHROME, FONT_SIZE, INK};
use super::super::timescale::{self, LabelOptions, TimeScale, Unit};
use super::grid::{self, Pane, LINE};
use crate::theme;
use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Stroke};
use openxer_engine::calendar::DAY;

pub const EPS_BAR: Color32 = theme::color(0x7F97B2);
pub const PROJECT_BAR: Color32 = theme::color(theme::BLUE);
pub const WBS_BAR: Color32 = theme::color(theme::NAVY);
/// The actual part of a project or folder bar.
pub const ACTUAL_BAR: Color32 = theme::color(theme::BLUE_DARK);
pub const BASELINE_BAR: Color32 = theme::color(theme::BASELINE);
pub const DATA_DATE_LINE: Color32 = theme::color(theme::DATA_DATE);
/// Default pitch of the views' month over week timescale (4 px a day).
pub const WEEK_PITCH: f64 = 28.;
/// Upper timescale tier height; the lower tier fills the rest of the header.
const TIER: f32 = 16.;

pub fn datetime(origin_serial: i64, minute: i64) -> Option<NaiveDateTime> {
    NaiveDate::from_ymd_opt(1899, 12, 30)?
        .checked_add_signed(Duration::try_days(origin_serial)?)?
        .and_hms_opt(0, 0, 0)?
        .checked_add_signed(Duration::try_minutes(minute)?)
}

/// Draws a two-tier timescale header in the Gantt's style: `upper` over
/// `lower`, each cell labelled with the timescale's own formats (the upper
/// band names its period, the lower one the cell), stepped where cells are
/// too narrow, on the `#F0F0F0` face with black lines.
pub fn timescale_header(
    painter: &egui::Painter,
    rect: Rect,
    axis: &TimeScale,
    origin: i64,
    options: &LabelOptions,
    upper: Unit,
    lower: Unit,
) {
    painter.rect_filled(rect, 0., CHROME);
    let last = axis.x_date(rect.right());
    let first = axis.x_date(rect.left());
    let tiers = [
        (
            Rect::from_min_max(rect.min, Pos2::new(rect.right(), rect.top() + TIER)),
            upper,
            true,
        ),
        (
            Rect::from_min_max(Pos2::new(rect.left(), rect.top() + TIER + 1.), rect.max),
            lower,
            false,
        ),
    ];
    let pen = Stroke::new(1_f32, INK);
    let font = FontId::proportional(FONT_SIZE);
    for (tier, unit, is_upper) in tiers {
        let p = painter.with_clip_rect(tier.intersect(painter.clip_rect()));
        let cells = timescale::band_cells(unit, is_upper, origin, first, last, options);
        crate::timeline::paint_band(&p, tier, axis, unit, &cells, &font, INK);
    }
    painter.hline(rect.x_range(), rect.top() + TIER + 0.5, pen);
    painter.hline(rect.x_range(), rect.bottom() - 0.5, pen);
    painter.vline(rect.left() + 0.5, rect.y_range(), pen);
}

/// Restore a saved chart zoom when it is a usable one.
pub fn restore_zoom(value: &serde_json::Value, zoom: &mut f64) {
    if let Some(z) = value
        .as_f64()
        .filter(|z| z.is_finite() && (SUMMARY_ZOOM.0..=SUMMARY_ZOOM.1).contains(z))
    {
        *zoom = z;
    }
}

/// Zoom range of the WBS and Projects charts, in pixels a week: from a few
/// years to a few days across the chart.
pub const SUMMARY_ZOOM: (f64, f64) = (1., 4032.);

/// A dashed line, 3 px on and 3 px off.
fn dashed(painter: &egui::Painter, a: Pos2, b: Pos2, color: Color32) {
    let horizontal = (a.y - b.y).abs() < 0.5;
    let length = if horizontal { b.x - a.x } else { b.y - a.y };
    let mut t = 0.;
    while t < length {
        let e = (t + 3.).min(length);
        let r = if horizontal {
            Rect::from_min_max(Pos2::new(a.x + t, a.y), Pos2::new(a.x + e, a.y + 1.))
        } else {
            Rect::from_min_max(Pos2::new(a.x, a.y + t), Pos2::new(a.x + 1., a.y + e))
        };
        painter.rect_filled(r, 0., color);
        t += 6.;
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BarKind {
    /// EPS rollup: thin light yellow bar with an end cap.
    Eps,
    #[default]
    /// A project in the Projects view: 9 px green bar.
    Project,
    /// The project row at the top of the WBS tree: thin green bar with an end cap.
    ProjectNode,
    /// A WBS element: 9 px blue bar with start and finish labels.
    Wbs,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bar {
    pub start: i64,
    pub finish: i64,
    pub kind: BarKind,
    pub start_label: Option<String>,
    pub finish_label: String,
    /// The actual part (actual start to actual finish or the data date),
    /// drawn darker over the bar (Projects view).
    pub actual: Option<(i64, i64)>,
    /// A data date mark across the row (Projects view).
    pub data_date: Option<i64>,
    /// A thin baseline bar under the bar (Projects view).
    pub baseline: Option<(i64, i64)>,
}

/// The Projects and WBS Gantt pane. `bars` is indexed by table row.
pub struct SummaryGantt {
    pub axis: TimeScale,
    /// The two timescale bands: month and week unless zoomed (Tracking).
    pub bands: (Unit, Unit),
    pub origin: i64,
    pub options: LabelOptions,
    pub bars: Vec<Option<Bar>>,
    /// Bar outlines drawn this frame, for tests: (row, rect).
    pub drawn: Vec<(usize, Rect)>,
    /// Baseline bars drawn this frame, for tests: (row, rect).
    pub baselines: Vec<(usize, Rect)>,
    /// The bands follow the zoom (`timescale::bands_for_scale`).
    pub auto_bands: bool,
    /// Set to scroll and zoom the chart like the Gantt; None draws it fixed
    /// (printing).
    pub timeline: Option<crate::timeline::Spec>,
    /// Where the chart was drawn, for the zoom commands.
    pub place: crate::timeline::Place,
}

impl SummaryGantt {
    /// A 4 px a day month/week chart starting a little before `first`.
    pub fn new(origin: i64, first: Option<i64>, options: LabelOptions, left: f32) -> Self {
        // Start on the week boundary before the first date, so the first
        // week cell is whole.
        let first_day = first.unwrap_or(0).div_euclid(DAY) - 1;
        let weekday = datetime(origin, first_day * DAY).map_or(0, |d| {
            if options.week_starts_sunday {
                d.weekday().num_days_from_sunday()
            } else {
                d.weekday().num_days_from_monday()
            }
        });
        let start = (first_day - weekday as i64) * DAY;
        Self {
            axis: TimeScale::new(start as f64, WEEK_PITCH, left),
            bands: (Unit::Month, Unit::Week),
            origin,
            options,
            bars: vec![],
            drawn: vec![],
            baselines: vec![],
            auto_bands: false,
            timeline: None,
            place: Default::default(),
        }
    }

    /// The scroll and zoom spec for bars covering `extent`.
    pub fn spec(&self, extent: (i64, i64)) -> crate::timeline::Spec {
        crate::timeline::Spec {
            extent,
            limits: SUMMARY_ZOOM,
            origin_serial: self.origin,
            unit: self.bands.1,
            options: self.options,
        }
    }

    /// Scroll and zoom the chart from a view's saved timeline: `first`
    /// (None: the default start) and `zoom`. The bands follow the zoom.
    /// Returns the default first minute.
    pub fn follow(&mut self, first: Option<f64>, zoom: f64) -> f64 {
        let default = self.axis.first_minute;
        self.axis.first_minute = first.unwrap_or(default);
        self.axis.pixels_per_week = zoom.clamp(SUMMARY_ZOOM.0, SUMMARY_ZOOM.1);
        self.auto_bands = true;
        self.bands = timescale::bands_for_scale(self.axis.pixels_per_week);
        let extent = self
            .extent()
            .unwrap_or((default as i64, default as i64 + 60 * DAY));
        self.timeline = Some(self.spec(extent));
        default
    }

    /// What a view keeps after the chart was drawn: its first minute (None
    /// while it is still at the default), zoom and place.
    pub fn kept(
        &self,
        first: Option<f64>,
        default: f64,
    ) -> (Option<f64>, f64, crate::timeline::Place) {
        let moved = (self.axis.first_minute - default).abs() > 1e-6;
        (
            if moved || first.is_some() {
                Some(self.axis.first_minute)
            } else {
                None
            },
            self.axis.pixels_per_week,
            self.place,
        )
    }

    /// Printing: start the chart enough whole weeks earlier that the start
    /// date labels drawn left of the first bars are not cut at its left
    /// edge. Labels are measured with `painter` at the chart's font.
    pub fn room_for_start_labels(&mut self, painter: &egui::Painter) {
        let font = FontId::proportional(FONT_SIZE);
        let left = self.axis.x(self.axis.first_minute);
        let need = self
            .bars
            .iter()
            .flatten()
            .filter_map(|b| {
                let label = b.start_label.as_ref()?;
                let width = painter
                    .layout_no_wrap(label.clone(), font.clone(), INK)
                    .size()
                    .x;
                // The label ends 10 px before the bar; keep 4 px of margin.
                Some(width + 14. - (self.axis.x(b.start as f64) - left))
            })
            .fold(0_f32, f32::max);
        if need > 0. {
            let weeks = (need / self.axis.pixels_per_week as f32).ceil() as f64;
            self.axis.first_minute -= weeks * 7. * DAY as f64;
        }
    }

    /// The first and last minute of the bars.
    pub fn extent(&self) -> Option<(i64, i64)> {
        let bars = self.bars.iter().flatten();
        let start = bars.clone().map(|b| b.start).min()?;
        let finish = bars.map(|b| b.finish).max()?;
        Some((start, finish.max(start)))
    }
}

fn outlined(painter: &egui::Painter, r: Rect, fill: Color32) {
    painter.rect_filled(r, 0., INK);
    painter.rect_filled(r.shrink(1.), 0., fill);
}

impl Pane for SummaryGantt {
    fn header(&mut self, ui: &mut egui::Ui, rect: Rect) {
        self.axis.left = rect.left() + 1.;
        if self.auto_bands {
            self.bands = timescale::bands_for_scale(self.axis.pixels_per_week);
        }
        timescale_header(
            ui.painter(),
            rect,
            &self.axis,
            self.origin,
            &self.options,
            self.bands.0,
            self.bands.1,
        );
    }
    fn timeline(&mut self) -> Option<grid::PaneTimeline<'_>> {
        let spec = self.timeline?;
        Some(grid::PaneTimeline {
            axis: &mut self.axis,
            spec,
            inset: 1.,
            pan_height: TIER + 1.,
            place: &mut self.place,
        })
    }
    fn body(&mut self, painter: &egui::Painter, rect: Rect, rows: &[(usize, Rect)]) {
        painter.rect_filled(rect, 0., Color32::WHITE);
        let first = self.axis.x_date(rect.left());
        let last = self.axis.x_date(rect.right());
        for cell in
            timescale::band_cells(self.bands.0, true, self.origin, first, last, &self.options)
        {
            let x = self.axis.x(cell.start_minute as f64).floor();
            if x > rect.left() + 1. {
                dashed(
                    painter,
                    Pos2::new(x, rect.top()),
                    Pos2::new(x, rect.bottom()),
                    LINE,
                );
            }
        }
        // Horizontal sight lines every five rows.
        for (index, row) in rows {
            if (index + 1) % 5 == 0 {
                dashed(
                    painter,
                    Pos2::new(rect.left(), row.bottom() - 1.),
                    Pos2::new(rect.right(), row.bottom() - 1.),
                    LINE,
                );
            }
        }
    }
    fn row(&mut self, painter: &egui::Painter, rect: Rect, index: usize, _selected: bool) {
        let Some(Some(bar)) = self.bars.get(index) else {
            return;
        };
        let x0 = self.axis.x(bar.start as f64).floor();
        let x1 = self.axis.x(bar.finish as f64).floor().max(x0 + 2.);
        // 3 below the top of a plain row; a taller (band) row centres the
        // bar in its extra height.
        let top = rect.top() + 3. + ((rect.height() - grid::ROW_HEIGHT).max(0.) / 2.).round();
        let font = FontId::proportional(FONT_SIZE);
        let (body, label_x) = match bar.kind {
            BarKind::Eps | BarKind::ProjectNode => {
                let body = Rect::from_min_max(Pos2::new(x0, top), Pos2::new(x1, top + 5.));
                let fill = if bar.kind == BarKind::Eps {
                    EPS_BAR
                } else {
                    PROJECT_BAR
                };
                outlined(painter, body, fill);
                // Down-pointing end cap, drawn as an outline.
                let cap = [
                    Pos2::new(x1 - 4., top),
                    Pos2::new(x1 + 4., top),
                    Pos2::new(x1, top + 7.),
                ];
                painter.add(egui::Shape::convex_polygon(
                    cap.to_vec(),
                    Color32::WHITE,
                    Stroke::new(1_f32, INK),
                ));
                (body, x1 + 9.)
            }
            BarKind::Project | BarKind::Wbs => {
                let body = Rect::from_min_max(Pos2::new(x0, top), Pos2::new(x1, top + 9.));
                outlined(
                    painter,
                    body,
                    if bar.kind == BarKind::Project {
                        PROJECT_BAR
                    } else {
                        WBS_BAR
                    },
                );
                (body, x1 + 10.)
            }
        };
        if let Some((a, b)) = bar.actual {
            let xa = self.axis.x(a as f64).floor().max(x0);
            let xb = self.axis.x(b as f64).floor().min(x1);
            if xb > xa + 1. {
                let r = Rect::from_min_max(Pos2::new(xa, body.top()), Pos2::new(xb, body.bottom()));
                outlined(painter, r, ACTUAL_BAR);
            }
        }
        if let Some((a, b)) = bar.baseline {
            let xa = self.axis.x(a as f64).floor();
            let xb = self.axis.x(b as f64).floor().max(xa + 2.);
            let top = body.bottom() + 2.;
            let r = Rect::from_min_max(Pos2::new(xa, top), Pos2::new(xb, top + 3.));
            painter.rect_filled(r, 0., BASELINE_BAR);
            self.baselines.push((index, r));
        }
        if let Some(d) = bar.data_date {
            let x = self.axis.x(d as f64).floor() + 0.5;
            painter.vline(x, rect.y_range(), Stroke::new(1_f32, DATA_DATE_LINE));
        }
        self.drawn.push((index, body));
        let y = body.center().y.round();
        painter.text(
            Pos2::new(label_x, y),
            Align2::LEFT_CENTER,
            &bar.finish_label,
            font.clone(),
            INK,
        );
        if let Some(label) = &bar.start_label {
            // Drawn only where it fits left of the bar: a label cut at the
            // chart's left edge reads as a wrong date.
            let galley = painter.layout_no_wrap(label.clone(), font, INK);
            let left = x0 - 10. - galley.size().x;
            if left >= rect.left() {
                painter.galley(Pos2::new(left, y - galley.size().y / 2.), galley, INK);
            }
        }
    }
}
