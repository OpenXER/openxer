//! Gantt chart drawing and interaction for the Activities view. The activity
//! grid owns the rows; this module paints the chart half of each row, the
//! timescale header, the overlays and the relationship gesture. What is drawn
//! comes from `GanttSettings` (bars, timescale, chart options, curtains).
pub mod bars;
mod dialogs;
#[cfg(test)]
mod edit_tests;
pub(crate) mod paint;
pub mod settings;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod text_tests;

use super::*;
use crate::format::TimeDisplay;
use crate::grid::marks::End;
use crate::grid::Row;
use bars::{BarDraw, BarSubject};
use gantt_geometry::{CalendarSpans, TimeSpan};
use settings::{
    AnchoredDate, BarDef, CollapseMode, DateInterval, DateType, GanttSettings, LabelField,
    LabelPosition, LineStyle, SightUnit, Weekday,
};
use timescale::{LabelOptions, TimeScale, Unit};

pub(super) use dialogs::GanttUi;

const GANTT_KEY: &str = "gantt_settings_v1";
/// Bar label origin after the finish date's pixel column (measured: the ink
/// of a capital I starts ten pixels after the bar's last column).
const LABEL_GAP: f32 = 9.;
/// Summary labels start one pixel further out (measured against the same
/// glyphs).
const SUMMARY_LABEL_GAP: f32 = 10.;
/// Bar label baseline below the row top (measured: cap tops on the bar box's
/// top row, three pixels down).
const LABEL_BASELINE: f32 = 11.;
/// Progress Spotlight shading from the data date to the spotlight's end.
const SPOTLIGHT: Color32 = Color32::from_rgba_premultiplied(0x30, 0x30, 0x00, 0x30);

#[derive(Clone, Copy)]
pub(super) struct LinkDrag {
    activity: usize,
    edge: Edge,
}
/// The part of a bar a drag holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BarPart {
    /// The body: the whole bar moves, the duration stays.
    Body,
    /// The start end: the start moves, the finish stays.
    Start,
    /// The finish end: the finish moves, the start stays.
    Finish,
}

/// A bar being dragged: the body moves the start, either end changes the
/// duration. The ends snap to whole days; the body keeps the time of day it
/// is dropped at (to the hour). Nothing changes until release.
#[derive(Clone, Copy, Debug)]
pub(crate) struct BarDrag {
    pub(crate) id: egui::Id,
    pub(crate) activity: usize,
    pub(crate) part: BarPart,
    /// Displayed start and finish when the drag began.
    pub(crate) start: i64,
    pub(crate) finish: i64,
    /// The drag so far on the time axis, in minutes rounded to the hour.
    pub(crate) minutes: i64,
}

/// The step a dropped bar body keeps: the hour.
const DROP_STEP: i64 = 60;

/// What a bar drag of `shift` minutes on the time axis leaves: the new
/// start, finish and duration (minutes) of a not-started activity on `cal`.
/// Moving keeps the remaining duration and starts at the first work time on
/// or after the shifted start, keeping its time of day; the ends take the
/// shift to the nearest whole day. Stretching the finish rounds the work
/// between the start and the shifted finish to whole days of
/// `day_minutes`, at least one; stretching the start starts at the first
/// work time on or after the shifted start (never later than a day before
/// the finish) and rounds the work from there to the finish the same way.
pub(crate) fn drag_dates(
    cal: Option<&openxer_engine::calendar::CalendarAdapter>,
    part: BarPart,
    (start, finish, rem): (i64, i64, i64),
    day_minutes: i64,
    shift: i64,
) -> (i64, i64, i64) {
    let add = |t: i64, m: i64| cal.map_or(t + m, |c| c.add(t, m));
    let day = day_minutes.max(1);
    let days = (shift as f64 / DAY as f64).round() as i64;
    let whole_days = |from: i64, to: i64| {
        let work = cal.map_or(to - from, |c| c.between(from, to));
        ((work as f64) / day as f64).round().max(1.) as i64 * day
    };
    match part {
        BarPart::Finish => {
            let duration = whole_days(start, finish + days * DAY);
            (start, add(start, duration), duration)
        }
        BarPart::Start => {
            let shifted = (start + days * DAY).min(finish - DAY);
            let begin = cal.map_or(shifted, |c| c.snap_start(shifted));
            let duration = whole_days(begin, finish);
            (begin, add(begin, duration), duration)
        }
        BarPart::Body => {
            let shifted = start + shift;
            let begin = cal.map_or(shifted, |c| c.snap_start(shifted));
            (begin, add(begin, rem), rem)
        }
    }
}

/// Size of the relationship zone at each bar end: wider and taller than
/// the bar, so a ring of it surrounds the stretch zone.
const LINK_ZONE: Vec2 = Vec2::new(16., 15.);
/// Width of the stretch zone at each bar end, inside the relationship zone.
const STRETCH_WIDTH: f32 = 6.;
/// Shortest bar (pixels between its ends) whose finish end stretches: the
/// finish stretch zone then stays clear of the start end's relationship
/// zone.
const FINISH_STRETCH_MIN: f32 = 12.;
/// Shortest bar whose start end stretches: the body then still keeps 8 px
/// to move the bar by. Shorter bars drop the start stretch zone first, then
/// the body, then the finish stretch zone.
const START_STRETCH_MIN: f32 = 24.;

/// The pointer zones of one bar, like a bullseye at each end: a stretch
/// zone (inner, the bar's own height) inside a relationship zone (outer).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct EndZones {
    pub(super) start_link: Rect,
    pub(super) finish_link: Rect,
    pub(super) start_stretch: Option<Rect>,
    pub(super) finish_stretch: Option<Rect>,
    pub(super) body: Option<Rect>,
}

/// Where a bar's ends at `start` and `finish` take the pointer, in a row
/// whose top is `row_top`. `movable` is a bar that can be dragged at all;
/// `stretchable` says per end whether that end may stretch (an end pulled in
/// to the chart's edge is not the real end). A milestone has no stretch
/// zones and keeps a 2 px body at its middle.
pub(super) fn end_zones(
    start: Pos2,
    finish: Pos2,
    row_top: f32,
    movable: bool,
    milestone: bool,
    stretchable: (bool, bool),
) -> EndZones {
    let middle = (start.x + finish.x) / 2.;
    let mut start_link = Rect::from_center_size(start, LINK_ZONE);
    let mut finish_link = Rect::from_center_size(finish, LINK_ZONE);
    // On a short bar the two relationship zones meet halfway, not overlap.
    start_link.max.x = start_link.max.x.min(middle);
    finish_link.min.x = finish_link.min.x.max(middle);
    let width = finish.x - start.x;
    let (bar_top, bar_bottom) = (
        row_top + bars::BOX_INSET,
        row_top + bars::BOX_INSET + bars::BOX_HEIGHT,
    );
    let stretch = |at: Pos2| {
        Rect::from_min_max(
            Pos2::new(at.x - STRETCH_WIDTH / 2., bar_top),
            Pos2::new(at.x + STRETCH_WIDTH / 2., bar_bottom),
        )
    };
    let ends = movable && !milestone;
    let (top, bottom) = (row_top + 1., bar_bottom + 2.);
    let body = if !movable {
        None
    } else if milestone {
        Some(Rect::from_min_max(
            Pos2::new(middle - 1., top),
            Pos2::new(middle + 1., bottom),
        ))
    } else {
        let body = Rect::from_min_max(
            Pos2::new(start_link.right(), top),
            Pos2::new(finish_link.left(), bottom),
        );
        (body.width() >= 2.).then_some(body)
    };
    EndZones {
        start_link,
        finish_link,
        start_stretch: (ends && stretchable.0 && width >= START_STRETCH_MIN)
            .then(|| stretch(start)),
        finish_stretch: (ends && stretchable.1 && width >= FINISH_STRETCH_MIN)
            .then(|| stretch(finish)),
        body,
    }
}

#[derive(Clone, Copy)]
pub(super) struct BarGeometry {
    pub(super) activity: usize,
    /// Read by the tests and by a debug build's `--capture-dir` output.
    #[cfg_attr(not(debug_assertions), allow(dead_code))]
    pub(super) task_id: i64,
    pub(super) start: Pos2,
    pub(super) finish: Pos2,
    pub(super) row: Rect,
    /// The relationship zones at each end (also the drop targets).
    pub(super) start_hit: Rect,
    pub(super) finish_hit: Rect,
}

#[cfg(test)]
impl BarGeometry {
    /// A point in the ring of an end's relationship zone, just above the
    /// bar (and so above any stretch zone): where a press starts a
    /// relationship on any bar.
    pub(super) fn link_grip(&self, edge: Edge) -> Pos2 {
        let (at, hit) = match edge {
            Edge::Start => (self.start, self.start_hit),
            Edge::Finish => (self.finish, self.finish_hit),
        };
        Pos2::new(at.x, hit.top() + 1.5)
    }
}

/// What a text attachment hangs from: an activity's row or a grouping
/// band's row (by the band's key, which stays the same between builds).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum RowAnchor {
    Activity(i64),
    Band(String),
}

/// Per-frame chart state shared by the header, every row and the overlays.
pub(super) struct GanttFrame {
    pub(super) chart: Rect,
    pub(super) body: Rect,
    pub(super) axis: TimeScale,
    pub(super) calendars: std::rc::Rc<BTreeMap<usize, CalendarSpans>>,
    pub(super) shade_cells: std::rc::Rc<BTreeMap<usize, Vec<TimeSpan>>>,
    /// Vertical sight lines: x and whether the line is a major line.
    pub(super) sight_x: Vec<(f32, bool)>,
    /// The table's column lookups for this frame: bars, labels and lines
    /// read percent complete and critical exactly as the columns do.
    pub(super) ix: std::rc::Rc<crate::grid::columns::Index>,
    /// Bar labels of the rows painted this frame, painted by the overlay
    /// over the relationship lines (with a halo in the chart's background
    /// color) so a line never runs through their text.
    pub(super) labels: std::cell::RefCell<Vec<DeferredLabel>>,
}

/// A bar label waiting for the overlay: where it goes and its text.
pub(super) struct DeferredLabel {
    x: f32,
    baseline: f32,
    galley: std::sync::Arc<egui::Galley>,
    ink: Color32,
}

impl GanttFrame {
    /// A row's chart half on the chart's whole-pixel columns.
    pub(super) fn row_rect(&self, row: Rect) -> Rect {
        Rect::from_min_max(
            Pos2::new(self.chart.left(), row.top().round()),
            Pos2::new(self.chart.right(), row.bottom().round()),
        )
    }
}

/// A timescale band: its unit and the label options it uses.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Band {
    pub unit: Unit,
    pub options: LabelOptions,
}

pub(super) fn interval_units(interval: DateInterval) -> Option<(Unit, Unit)> {
    Some(match interval {
        DateInterval::Automatic | DateInterval::DayShift => return None,
        DateInterval::YearQuarter => (Unit::Year, Unit::Quarter),
        DateInterval::YearMonth => (Unit::Year, Unit::Month),
        DateInterval::QuarterMonth => (Unit::Quarter, Unit::Month),
        DateInterval::MonthWeek => (Unit::Month, Unit::Week),
        DateInterval::WeekDay1 | DateInterval::WeekDay2 => (Unit::Week, Unit::Day),
        DateInterval::DayHour => (Unit::Day, Unit::Hour),
    })
}

/// Pixels per week chosen when a date interval is picked. Week/Day uses the
/// measured 14.5 px day; the others give each lower cell a comparable width.
pub(super) fn interval_scale(interval: DateInterval) -> f64 {
    match interval {
        DateInterval::YearQuarter => Unit::Quarter.preset_scale(60.),
        DateInterval::YearMonth => Unit::Month.preset_scale(40.),
        DateInterval::QuarterMonth => Unit::Month.preset_scale(60.),
        DateInterval::MonthWeek => Unit::Week.preset_scale(50.),
        DateInterval::WeekDay1 | DateInterval::WeekDay2 => Unit::Day.preset_scale(14.5),
        DateInterval::DayHour => Unit::Hour.preset_scale(24.),
        DateInterval::Automatic | DateInterval::DayShift => Unit::Week.preset_scale(80.),
    }
}

fn sight_unit(unit: SightUnit) -> Unit {
    match unit {
        SightUnit::Day => Unit::Day,
        SightUnit::Week => Unit::Week,
        SightUnit::Month => Unit::Month,
        SightUnit::Quarter => Unit::Quarter,
        SightUnit::Year => Unit::Year,
    }
}

/// The units painted top to bottom for a two- or three-line timescale.
pub(super) fn band_units(upper: Unit, lower: Unit, three_lines: bool) -> Vec<Unit> {
    if !three_lines {
        return vec![upper, lower];
    }
    match upper {
        Unit::Year => match lower {
            Unit::Quarter | Unit::Month => vec![Unit::Year, Unit::Quarter, Unit::Month],
            _ => vec![Unit::Year, Unit::Month, lower],
        },
        Unit::Quarter | Unit::Month => vec![Unit::Year, upper, lower],
        Unit::Week | Unit::Day => vec![Unit::Month, upper, lower],
        Unit::Hour => vec![Unit::Day, upper, lower],
    }
}

impl App {
    /// Measured: two bands of 16 and 17 pixels, each closed by a black line,
    /// 35 pixels in all (the table header's height). Three lines keep the
    /// 17-pixel band pitch.
    pub(super) fn gantt_header_height(&self) -> f32 {
        if self.gantt.timescale.three_lines {
            52.
        } else {
            35.
        }
    }

    /// Activity rows grow when a displayed bar uses a second or third bar row.
    pub(super) fn gantt_row_height(&self) -> f32 {
        self.view.row_height + f32::from(self.gantt.bar_rows() - 1) * bars::SLOT_PITCH
    }

    fn project_start(&self) -> i64 {
        let leveled = self.shown().model.options.do_level;
        self.shown()
            .model
            .activities
            .iter()
            .filter_map(|a| displayed_start(a, leveled))
            .min()
            .or(self.shown().model.project.data_date)
            .unwrap_or(0)
    }

    pub(super) fn resolve_date(&self, date: AnchoredDate) -> i64 {
        let start = self.project_start();
        date.resolve(start, self.shown().model.project.data_date.unwrap_or(start))
    }

    pub(super) fn label_options(&self) -> LabelOptions {
        let t = &self.gantt.timescale;
        LabelOptions {
            week_starts_sunday: t.week_start == Weekday::Sunday,
            // Week/Day 1 numbers the days; every other choice names them.
            day_letters: self.view.automatic_units || t.interval != DateInterval::WeekDay1,
            week_numbers: t.date_type == DateType::WeekOfYear,
            fiscal_start_month: if t.date_type == DateType::Fiscal {
                t.fiscal_start_month
            } else {
                1
            },
            ordinal_origin: None,
            hour_24: self.formats.dates.time != TimeDisplay::Hour12,
        }
    }

    /// Timescale bands from top to bottom, honoring three lines and ordinal dates.
    pub(super) fn timescale_bands(&self) -> Vec<Band> {
        let options = self.label_options();
        let t = &self.gantt.timescale;
        let mut bands: Vec<Band> = band_units(self.view.upper, self.view.lower, t.three_lines)
            .into_iter()
            .map(|unit| Band { unit, options })
            .collect();
        if t.ordinal {
            let unit = sight_unit(match t.ordinal_unit {
                settings::OrdinalUnit::Day => SightUnit::Day,
                settings::OrdinalUnit::Week => SightUnit::Week,
                settings::OrdinalUnit::Month => SightUnit::Month,
                settings::OrdinalUnit::Quarter => SightUnit::Quarter,
                settings::OrdinalUnit::Year => SightUnit::Year,
            });
            bands[0] = Band {
                unit,
                options: LabelOptions {
                    ordinal_origin: Some(self.resolve_date(t.ordinal_start)),
                    ..options
                },
            };
        }
        bands
    }

    fn nonwork_color(&self) -> Color32 {
        self.gantt.options.nonwork.color.color()
    }

    fn paint_nonwork(&self, painter: &egui::Painter, rect: Rect) {
        let rect = rect.intersect(painter.clip_rect());
        if !rect.is_positive() {
            return;
        }
        painter.rect_filled(rect, 0., self.nonwork_color());
        if self.gantt.options.nonwork.hatch {
            bars::fill_rect(
                painter,
                rect,
                Color32::from_gray(192),
                settings::Pattern::ForwardDiagonal,
            );
        }
    }

    /// Shading, curtains and vertical sight lines behind the bars of a strip.
    /// The shading follows the project calendar on every row, as the
    /// reference shades weekends even on seven-day activities.
    fn paint_backdrop(&self, painter: &egui::Painter, gantt: &GanttFrame, strip: Rect) {
        let axis = &gantt.axis;
        let cal = self.shown().model.project_calendar;
        for span in gantt.shade_cells.get(&cal).into_iter().flatten() {
            self.paint_nonwork(
                painter,
                Rect::from_min_max(
                    Pos2::new(axis.x(span.start as f64), strip.top()),
                    Pos2::new(axis.x(span.end as f64), strip.bottom()),
                ),
            );
        }
        if let Some((dd, end)) = self.spotlight_window() {
            let rect = Rect::from_min_max(
                Pos2::new(axis.x(dd as f64), strip.top()),
                Pos2::new(axis.x(end as f64), strip.bottom()),
            )
            .intersect(strip);
            if rect.is_positive() {
                painter.rect_filled(rect, 0., SPOTLIGHT);
            }
        }
        for curtain in self.gantt.curtains.iter().filter(|c| c.display) {
            let rect = Rect::from_min_max(
                Pos2::new(axis.x(curtain.start as f64), strip.top()),
                Pos2::new(axis.x(curtain.finish as f64), strip.bottom()),
            )
            .intersect(strip);
            if rect.is_positive() {
                bars::fill_rect(painter, rect, curtain.color.color(), curtain.pattern);
            }
        }
        let sight = &self.gantt.options.sight_lines;
        for &(x, major) in &gantt.sight_x {
            let lines = if major {
                sight.vertical_major
            } else {
                sight.vertical_minor
            };
            // Dashes run from the body top, so rows of any height line up.
            paint::vline(
                painter,
                x,
                strip.top(),
                strip.bottom(),
                1.,
                lines.color.color(),
                lines.style,
                gantt.body.top(),
            );
        }
    }

    fn vertical_sight_lines(&self, axis: &TimeScale, first: f64, last: f64) -> Vec<(f32, bool)> {
        let origin = self.shown().model.origin_serial;
        let options = self.label_options();
        let sight = &self.gantt.options.sight_lines;
        let mut result: Vec<(f32, bool)> = vec![];
        for (lines, major) in [(sight.vertical_major, true), (sight.vertical_minor, false)] {
            if !lines.show {
                continue;
            }
            let unit = sight_unit(lines.unit);
            let anchor = self.resolve_date(lines.start);
            for cell in timescale::band_cells(unit, true, origin, first, last, &options) {
                let Some(n) =
                    timescale::unit_ordinal(unit, origin, anchor, cell.start_minute, &options)
                else {
                    continue;
                };
                let k = if n > 0 { n - 1 } else { n };
                let x = axis.x(cell.start_minute as f64);
                // Minor lines are skipped where a major line is already drawn.
                if k.rem_euclid(i64::from(lines.every)) == 0
                    && !result.iter().any(|&(other, _)| other == x)
                {
                    result.push((x, major));
                }
            }
        }
        result
    }

    /// Band rectangles from top to bottom. Each band ends in its black line;
    /// the bottom band absorbs the division remainder, as it is painted first.
    pub(super) fn band_rects(chart: Rect, count: usize) -> Vec<Rect> {
        let count = count.max(1);
        let band = (chart.height() / count as f32).floor();
        let last = chart.height() - band * (count - 1) as f32;
        (0..count)
            .map(|i| {
                let top = chart.top() + band * i as f32;
                let height = if i + 1 == count { last } else { band };
                Rect::from_min_size(
                    Pos2::new(chart.left(), top),
                    Vec2::new(chart.width(), height),
                )
            })
            .collect()
    }

    /// Paints the timescale bands over `chart` and the body backdrop below
    /// it, and handles the header pan and zoom drags.
    pub(super) fn gantt_header(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        chart: Rect,
        _rows: &[Row],
    ) -> GanttFrame {
        // The table's width is fractional; the chart starts on a whole pixel
        // so every line and bar lands on pixel columns.
        let chart = Rect::from_min_max(
            Pos2::new(chart.left().round(), chart.top().round()),
            Pos2::new(chart.right().round(), chart.bottom().round()),
        );
        self.gantt_left = chart.left();
        let p = ui.painter().clone();
        let bands = self.timescale_bands();
        let count = bands.len();
        let band_rects = Self::band_rects(chart, count);
        let top = band_rects[..count - 1]
            .iter()
            .fold(Rect::NOTHING, |acc, r| acc.union(*r));
        let bottom = band_rects[count - 1];
        let (pan, zoom) = crate::timeline::header(ui, ui.id(), top, bottom);
        self.action("timescale_pan", &pan);
        self.action("timescale_zoom", &zoom);
        let span = crate::timeline::header_drags(
            ctx,
            self.main_span(),
            &pan,
            &zoom,
            chart.left(),
            self.main_limits(),
        );
        if span != self.main_span() {
            self.set_main_span(span);
        }
        if pan.is_pointer_button_down_on() || zoom.is_pointer_button_down_on() {
            self.touch_gantt_timeline();
        }
        for response in [&pan, &zoom] {
            if response.secondary_clicked() {
                if let Some(pos) = response.interact_pointer_pos() {
                    self.open_gantt_menu(pos);
                }
            }
        }
        let ppd = self.pixels_per_day;
        let origin = self.shown().model.origin_serial;
        let axis = TimeScale::new(self.timeline_origin as f64, ppd as f64 * 7., chart.left());
        let last = axis.x_date(chart.right());
        let cells: Vec<Vec<timescale::Cell>> = bands
            .iter()
            .enumerate()
            .map(|(i, band)| {
                timescale::band_cells(
                    band.unit,
                    i + 1 < count,
                    origin,
                    axis.first_minute,
                    last,
                    &band.options,
                )
            })
            .collect();
        let lower_cells = &cells[count - 1];
        let intervals: Vec<_> = lower_cells
            .iter()
            .map(|cell| (cell.start_minute, cell.end_minute))
            .collect();
        let cal_start = intervals.first().map_or(self.timeline_origin, |c| c.0);
        let cal_finish = intervals.last().map_or(last.ceil() as i64, |c| c.1);
        let shade = self.gantt.options.nonwork.shade;
        let lowest_hour = bands[count - 1].unit == Unit::Hour;
        let serial = self.shown_serial();
        let cal_key = serial.map(|serial| dialogs::GanttCalKey {
            serial,
            cal_start,
            cal_finish,
            lowest_hour,
            shade,
        });
        let (calendars, shade_cells) = match (cal_key, &self.gantt_ui.cal_cache) {
            (Some(key), Some(c)) if c.key == key => (c.calendars.clone(), c.shade_cells.clone()),
            _ => {
                let model = &self.shown().model;
                let mut used = vec![false; model.calendars.len()];
                if let Some(slot) = used.get_mut(model.project_calendar) {
                    *slot = true;
                }
                for a in &model.activities {
                    if let Some(slot) = used.get_mut(a.cal) {
                        *slot = true;
                    }
                }
                let calendars: BTreeMap<_, _> = used
                    .into_iter()
                    .enumerate()
                    .filter(|(_, keep)| *keep)
                    .map(|(i, _)| {
                        (
                            i,
                            CalendarSpans::new(&model.calendars[i].cal, cal_start, cal_finish),
                        )
                    })
                    .collect();
                let mut shade_map = BTreeMap::new();
                if shade {
                    if let Some(spans) = calendars.get(&model.project_calendar) {
                        shade_map.insert(
                            model.project_calendar,
                            spans.nonwork_cells(&intervals, lowest_hour),
                        );
                    }
                }
                let calendars = std::rc::Rc::new(calendars);
                let shade_cells = std::rc::Rc::new(shade_map);
                if let Some(key) = cal_key {
                    self.gantt_ui.cal_cache = Some(dialogs::GanttCalCache {
                        key,
                        calendars: calendars.clone(),
                        shade_cells: shade_cells.clone(),
                    });
                }
                (calendars, shade_cells)
            }
        };
        let t = &self.gantt.timescale;
        let font = if t.font_bold {
            FontId::new(t.font_points * 96. / 72., chrome::bold_font().family)
        } else {
            paint::arial(t.font_points)
        };
        let ink = t.font_color.color();
        let header = p.with_clip_rect(chart);
        for (band_index, (band, cells)) in band_rects.iter().zip(&cells).enumerate() {
            let band = *band;
            // The band's own fill ends above its closing line.
            let fill_bottom = band.bottom() - 1.;
            let painter = p.with_clip_rect(band);
            paint::fill(
                &painter,
                band.left(),
                band.top(),
                band.right(),
                fill_bottom,
                CHROME,
            );
            let spans: Vec<(f32, f32)> = cells
                .iter()
                .map(|cell| {
                    (
                        axis.x(cell.start_minute as f64),
                        axis.x(cell.end_minute as f64),
                    )
                })
                .collect();
            let unit = bands[band_index].unit;
            let placed = timescale::place_labels(
                unit,
                cells,
                &spans,
                (band.left() + 1., band.right()),
                &mut |text| {
                    painter
                        .layout_no_wrap(text.to_owned(), font.clone(), ink)
                        .size()
                        .x
                },
            );
            for ((cell, &(x, _)), label) in cells.iter().zip(&spans).zip(placed.labels.iter()) {
                // Skipped cells of a stepped band get a short tick, so no
                // line runs through a neighbour's label.
                let tick_top = if placed.is_major(cell) {
                    band.top()
                } else {
                    (fill_bottom - timescale::MINOR_TICK).max(band.top())
                };
                paint::fill(&painter, x, tick_top, x + 1., fill_bottom, INK);
                let Some(label) = label else { continue };
                let galley = painter.layout_no_wrap(label.text.clone(), font.clone(), ink);
                // Measured: the baseline sits five pixels above the fill's end.
                paint::text_at_baseline(&painter, label.left, fill_bottom - 5., galley, ink);
            }
            paint::fill(
                &painter,
                band.left(),
                fill_bottom,
                band.right(),
                band.bottom(),
                INK,
            );
        }
        // The header's left edge carries a black line.
        paint::fill(
            &header,
            chart.left(),
            chart.top(),
            chart.left() + 1.,
            chart.bottom(),
            INK,
        );
        let body = Rect::from_min_max(
            Pos2::new(chart.left(), chart.bottom()),
            Pos2::new(chart.right(), ui.max_rect().bottom()),
        );
        let sight_x = self.vertical_sight_lines(&axis, axis.first_minute, last);
        let frame = GanttFrame {
            chart,
            body,
            axis,
            calendars,
            shade_cells,
            sight_x,
            ix: self.shared_index(),
            labels: Default::default(),
        };
        let backdrop = ui.painter().with_clip_rect(body);
        paint::fill(
            &backdrop,
            body.left(),
            body.top(),
            body.right(),
            body.bottom(),
            self.gantt.options.background.color(),
        );
        self.paint_backdrop(&backdrop, &frame, body);
        frame
    }

    /// Dashed sight lines on the row's last pixel row (its line slot). Rows
    /// are counted from the top of the list, bands included.
    fn horizontal_sight_line(
        &self,
        painter: &egui::Painter,
        gantt: &GanttFrame,
        row_index: usize,
        rect: Rect,
        group: bool,
    ) {
        let sight = &self.gantt.options.sight_lines;
        let origin = gantt.chart.left();
        if sight.horizontal_major.show && group {
            let major = sight.horizontal_major;
            paint::hline(
                painter,
                rect.left(),
                rect.right(),
                rect.top(),
                1.,
                major.color.color(),
                major.style,
                origin,
            );
        }
        let minor = sight.horizontal_minor;
        if minor.show && (row_index + 1).is_multiple_of(usize::from(minor.every_rows.max(1))) {
            paint::hline(
                painter,
                rect.left(),
                rect.right(),
                rect.bottom() - 1.,
                1.,
                minor.color.color(),
                minor.style,
                origin,
            );
        }
    }

    /// The facts the bar filters and date sources read about activity `i`
    /// (`a`): its Activity % by its percent complete type and whether it is
    /// critical by the project's definition, as the table columns show them.
    pub(crate) fn subject<'a>(
        &self,
        ix: &crate::grid::columns::Index,
        i: usize,
        a: &'a Activity,
    ) -> BarSubject<'a> {
        BarSubject {
            activity: a,
            percent: crate::grid::columns::activity_percent(self.shown(), ix, i),
            critical: crate::grid::columns::is_critical(a, ix.critical),
            leveled: self.shown().model.options.do_level,
            data_date: self.shown().model.project.data_date,
            baselines: if self
                .gantt
                .bars
                .iter()
                .any(|b| b.display && b.timescale.is_baseline())
            {
                self.baseline_compare().spans(self.shown(), a)
            } else {
                [None; 4]
            },
        }
    }

    /// A bar label's text: the same text the table column for that field
    /// shows (durations and float in the user's duration format, Activity %
    /// by the activity's percent complete type).
    pub(crate) fn label_text(
        &self,
        ix: &crate::grid::columns::Index,
        field: LabelField,
        a: &Activity,
        index: usize,
    ) -> String {
        use crate::grid::columns::{self, Field};
        let origin = self.shown().model.origin_serial;
        let leveled = self.shown().model.options.do_level;
        let shown = |f: Field| {
            let p = self.shown();
            columns::display(p, Some(index), &columns::field(p, ix, index, f))
        };
        let detail = self.shown().details.get(&a.task_id);
        match field {
            LabelField::ActivityId => a.task_code.clone(),
            LabelField::ActivityName => self.shown().name(index).to_owned(),
            LabelField::Start => bar_label_date(origin, displayed_start(a, leveled), a, End::Start),
            LabelField::Finish => {
                bar_label_date(origin, displayed_finish(a, leveled), a, End::Finish)
            }
            LabelField::OriginalDuration => shown(Field::OriginalDuration),
            LabelField::RemainingDuration => shown(Field::RemainingDuration),
            LabelField::TotalFloat => shown(Field::TotalFloat),
            LabelField::FreeFloat => shown(Field::FreeFloat),
            LabelField::PercentComplete => shown(Field::ActivityPercent),
            LabelField::ActivityStatus => status_label(a.status).into(),
            LabelField::ActivityType => type_label(a.task_type).into(),
            LabelField::Calendar => self
                .shown()
                .calendar_names
                .get(a.cal)
                .cloned()
                .unwrap_or_default(),
            LabelField::Wbs => detail.map_or(String::new(), |d| self.shown().wbs_label(d.wbs)),
        }
    }

    /// Paints label text around a drawn bar. `texts` holds each label's
    /// position and resolved text.
    #[allow(clippy::too_many_arguments)]
    fn paint_labels(
        &self,
        gantt: &GanttFrame,
        painter: &egui::Painter,
        def: &BarDef,
        draw: &BarDraw,
        x0: f32,
        x1: f32,
        slot: Rect,
        texts: &[(LabelPosition, String)],
    ) {
        let options = &self.gantt.options;
        let font = options
            .font
            .face
            .font(options.font.points, options.font.bold);
        let ink = options.font.color.color();
        let summary = def.start.shape == settings::EndShape::SummaryCap
            || def.end.shape == settings::EndShape::SummaryCap;
        for position in LabelPosition::ALL {
            let mut parts: Vec<String> = texts
                .iter()
                .filter(|(p, text)| *p == position && !text.is_empty())
                .map(|(_, text)| text.clone())
                .collect();
            if parts.is_empty() {
                continue;
            }
            if options.limit_label_chars {
                let limit = usize::from(options.label_chars);
                for part in &mut parts {
                    *part = part.chars().take(limit).collect();
                }
            }
            let text = parts.join(", ");
            let bounds = if draw.bounds.is_positive() {
                draw.bounds
            } else {
                Rect::from_min_max(
                    Pos2::new(x0, slot.top() + bars::BOX_INSET),
                    Pos2::new(x1, slot.top() + bars::BOX_INSET + bars::BOX_HEIGHT),
                )
            };
            let galley = painter.layout_no_wrap(text, font.clone(), ink);
            let width = galley.size().x;
            // Measured: labels sit on the row's baseline (cap tops level with
            // the bar box) and start a fixed distance after the finish date,
            // whatever the bar's shape.
            let baseline = slot.top() + LABEL_BASELINE;
            let line = galley.size().y;
            let gap = if summary {
                SUMMARY_LABEL_GAP
            } else {
                LABEL_GAP
            };
            let (x, baseline) = match position {
                LabelPosition::Right => (x1 + gap, baseline),
                LabelPosition::Left => (x0 - gap - width, baseline),
                LabelPosition::Top => (((x0 + x1 - width) / 2.).round(), bounds.top() - 2.),
                LabelPosition::Bottom => (
                    ((x0 + x1 - width) / 2.).round(),
                    bounds.bottom() + line - 1.,
                ),
                LabelPosition::Center => (((x0 + x1 - width) / 2.).round(), baseline),
            };
            gantt.labels.borrow_mut().push(DeferredLabel {
                x: x.round(),
                baseline,
                galley,
                ink,
            });
        }
    }

    /// Paint the bar labels the rows left for the overlay: each on a halo of
    /// the chart's background, above the relationship lines.
    fn paint_deferred_labels(&self, gantt: &GanttFrame, painter: &egui::Painter) {
        let halo = self.gantt.options.background.color();
        for label in gantt.labels.borrow_mut().drain(..) {
            let ascent = label
                .galley
                .rows
                .first()
                .and_then(|row| row.glyphs.first())
                .map_or(label.galley.size().y * 0.8, |glyph| glyph.pos.y);
            let ink_rect = Rect::from_min_size(
                Pos2::new(label.x, label.baseline - ascent),
                label.galley.size(),
            );
            painter.rect_filled(ink_rect.expand(1.), 0., halo);
            paint::text_at_baseline(painter, label.x, label.baseline, label.galley, label.ink);
        }
    }

    /// Draws every displayed bar that applies to `a` into the row slots below
    /// `row_top`; returns the union of the drawn bounds.
    #[allow(clippy::too_many_arguments)]
    fn draw_activity_bars(
        &self,
        painter: &egui::Painter,
        gantt: &GanttFrame,
        row_top: f32,
        i: usize,
        a: &Activity,
        filter: impl Fn(&BarDef) -> bool,
        lane: Option<(f32, f32)>,
        with_labels: bool,
    ) -> Rect {
        let axis = &gantt.axis;
        let subject = self.subject(&gantt.ix, i, a);
        let mut bounds = Rect::NOTHING;
        let mut labels = vec![];
        for def in self.gantt.bars.iter().filter(|d| d.display && filter(d)) {
            if !bars::bar_applies(def, &subject) {
                continue;
            }
            let Some((start, finish)) = bars::bar_span(def.timescale, &subject) else {
                continue;
            };
            let x0 = axis.x(start as f64);
            let x1 = axis.x(finish as f64);
            let slot_top = row_top + f32::from(def.row - 1) * bars::SLOT_PITCH;
            let necks = if def.neck_calendar_nonwork {
                gantt
                    .calendars
                    .get(&a.cal)
                    .map(|c| c.pixel_necks(start, finish, |m| axis.x(m as f64)))
                    .unwrap_or_default()
            } else {
                vec![]
            };
            let mut draw = bars::layout_bar(def, gantt.chart.left(), slot_top, x0, x1, &necks);
            if let Some((top, height)) = lane {
                draw = squeeze(draw, row_top, top, height);
            }
            bars::paint(painter, &draw);
            bounds = bounds.union(draw.bounds);
            if with_labels && !def.labels.is_empty() {
                let slot = Rect::from_min_max(
                    Pos2::new(gantt.chart.left(), slot_top),
                    Pos2::new(gantt.chart.right(), slot_top + 17.),
                );
                let texts: Vec<_> = def
                    .labels
                    .iter()
                    .map(|l| (l.position, self.label_text(&gantt.ix, l.field, a, i)))
                    .collect();
                labels.push((def, draw, x0, x1, slot, texts));
            }
        }
        for (def, draw, x0, x1, slot, texts) in labels {
            self.paint_labels(gantt, painter, def, &draw, x0, x1, slot, &texts);
        }
        bounds
    }

    /// The chart half of a grouping band row: the rolled-up grouping band
    /// bars and, when the band is collapsed, its activities' bars.
    pub(super) fn gantt_group_row(
        &mut self,
        painter: &egui::Painter,
        gantt: &GanttFrame,
        chart_row: Rect,
        row_index: usize,
        band: usize,
    ) {
        let chart_row = gantt.row_rect(chart_row);
        let key = self.grid.bands.borrow().get(band).map(|b| b.key.clone());
        if let Some(key) = key {
            self.row_geometry.push((RowAnchor::Band(key), chart_row));
        }
        // Band rows are white in the chart, with the same backdrop as
        // activity rows; the band color stays in the table.
        let painter = painter.with_clip_rect(chart_row);
        paint::fill(
            &painter,
            chart_row.left(),
            chart_row.top(),
            chart_row.right(),
            chart_row.bottom(),
            self.gantt.options.background.color(),
        );
        self.paint_backdrop(&painter, gantt, chart_row);
        self.horizontal_sight_line(&painter, gantt, row_index, chart_row, true);
        let leveled = self.shown().model.options.do_level;
        let members = self.band_members_shown(band);
        let acts = || members.iter().map(|&i| &self.shown().model.activities[i]);
        let start = acts().filter_map(|a| displayed_start(a, leveled)).min();
        let finish = acts().filter_map(|a| displayed_finish(a, leveled)).max();
        let axis = &gantt.axis;
        if let (Some(start), Some(finish)) = (start, finish) {
            let complete = acts().all(|a| a.is_completed());
            let origin = self.shown().model.origin_serial;
            for def in self
                .gantt
                .bars
                .iter()
                .filter(|d| d.display && d.show_for_grouping_bands)
            {
                let x0 = axis.x(start as f64);
                let x1 = axis.x(finish as f64);
                let draw = bars::layout_bar(def, gantt.chart.left(), chart_row.top(), x0, x1, &[]);
                bars::paint(&painter, &draw);
                let texts: Vec<_> = def
                    .labels
                    .iter()
                    .map(|l| {
                        let text = match l.field {
                            LabelField::ActivityName | LabelField::Wbs => self.band_label(band),
                            LabelField::Start => format::date(origin, Some(start)),
                            LabelField::Finish => {
                                let date = format::date(origin, Some(finish));
                                if complete {
                                    format!("{date} A")
                                } else {
                                    date
                                }
                            }
                            _ => String::new(),
                        };
                        (l.position, text)
                    })
                    .collect();
                self.paint_labels(gantt, &painter, def, &draw, x0, x1, chart_row, &texts);
            }
        }
        if !self.band_collapsed(band) {
            return;
        }
        // Collapsed band: draw member bars into the band row.
        let individual = self.gantt.options.collapse == CollapseMode::IndividualBars;
        let mut lanes: Vec<i64> = vec![];
        let mut placed = vec![];
        for &i in &members {
            let a = &self.shown().model.activities[i];
            let (Some(s), Some(f)) = (displayed_start(a, leveled), displayed_finish(a, leveled))
            else {
                continue;
            };
            let lane = if individual {
                match lanes.iter().position(|&end| end <= s) {
                    Some(k) => {
                        lanes[k] = f;
                        k
                    }
                    None => {
                        lanes.push(f);
                        lanes.len() - 1
                    }
                }
            } else {
                0
            };
            placed.push((i, lane));
        }
        let lane_count = lanes.len().max(1) as f32;
        let mut collapsed = vec![];
        for (i, lane) in placed {
            let a = self.shown().model.activities[i].clone();
            let lane_box = individual.then(|| {
                let height = ((chart_row.height() - 4.) / lane_count).max(2.);
                (chart_row.top() + 2. + lane as f32 * height, height)
            });
            let bounds = self.draw_activity_bars(
                &painter,
                gantt,
                chart_row.top(),
                i,
                &a,
                |d| d.show_when_collapsed && !d.show_for_grouping_bands,
                lane_box,
                false,
            );
            if bounds.is_positive() {
                let y = bounds.center().y;
                collapsed.push(BarGeometry {
                    activity: i,
                    task_id: a.task_id,
                    start: Pos2::new(bounds.left(), y),
                    finish: Pos2::new(bounds.right(), y),
                    row: chart_row,
                    start_hit: Rect::NOTHING,
                    finish_hit: Rect::NOTHING,
                });
            }
        }
        self.gantt_ui.collapsed_geometry.extend(collapsed);
    }

    /// The chart half of an activity row: backdrop, configured bars and
    /// labels, and the relationship drag handles.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn gantt_activity_row(
        &mut self,
        ui: &mut egui::Ui,
        painter: &egui::Painter,
        gantt: &GanttFrame,
        rect: Rect,
        chart_row: Rect,
        row_index: usize,
        i: usize,
        a: &Activity,
        // The reference does not mark the selected activity in the chart.
        _selected: bool,
    ) {
        let axis = &gantt.axis;
        let leveled = self.shown().model.options.do_level;
        let chart_row = gantt.row_rect(chart_row);
        self.row_geometry
            .push((RowAnchor::Activity(a.task_id), chart_row));
        let chart_p = painter.with_clip_rect(chart_row);
        paint::fill(
            &chart_p,
            chart_row.left(),
            chart_row.top(),
            chart_row.right(),
            chart_row.bottom(),
            self.gantt.options.background.color(),
        );
        self.paint_backdrop(&chart_p, gantt, chart_row);
        self.horizontal_sight_line(&chart_p, gantt, row_index, chart_row, false);
        self.draw_activity_bars(&chart_p, gantt, rect.top(), i, a, |_| true, None, true);
        let (Some(start), Some(finish)) =
            (displayed_start(a, leveled), displayed_finish(a, leveled))
        else {
            return;
        };
        let (start, finish) = match a.task_type {
            TaskType::Mile => (start, start),
            TaskType::FinMile => (finish, finish),
            _ => (start, finish),
        };
        let x = axis.x(start as f64);
        let end = axis.x(finish as f64);
        let y = rect.top() + bars::BOX_INSET + bars::BOX_HEIGHT / 2.;
        if end >= chart_row.left() && x <= chart_row.right() {
            let center = (x + end) / 2.;
            let mut left = x;
            let mut right = end;
            if right - left < 10. {
                left = center - 5.;
                right = center + 5.;
            }
            let start_x = left.clamp(chart_row.left() + 5., chart_row.right() - 5.);
            let finish_x = right.clamp(chart_row.left() + 5., chart_row.right() - 5.);
            let start_pos = Pos2::new(start_x, y);
            let finish_pos = Pos2::new(finish_x, y);
            let zones = end_zones(
                start_pos,
                finish_pos,
                rect.top(),
                self.bar_movable(i, a),
                a.task_type == TaskType::Mile,
                (start_x == left, finish_x == right),
            );
            self.bar_geometry.push(BarGeometry {
                activity: i,
                task_id: a.task_id,
                start: start_pos,
                finish: finish_pos,
                row: rect,
                start_hit: zones.start_link,
                finish_hit: zones.finish_link,
            });
            // The relationship zones first, so the body and stretch zones
            // registered after them are on top where they overlap.
            for (edge, hit, at) in [
                (Edge::Start, zones.start_link, start_pos),
                (Edge::Finish, zones.finish_link, finish_pos),
            ] {
                let handle = ui.interact(
                    hit,
                    ui.id().with(("bar_edge", a.task_id, edge == Edge::Finish)),
                    Sense::drag(),
                );
                // The pointer is the Relationship Lines icon here; while a
                // relationship is dragged, `relationship_gesture` shows it.
                if handle.hovered() && self.link_drag.is_none() {
                    chrome::paint_pointer_icon(ui.ctx(), chrome::Icon::Links);
                }
                if handle.hovered() || self.link_drag.is_some_and(|d| d.activity == i) {
                    let mark = Rect::from_center_size(at, Vec2::new(4., 7.));
                    chart_p.rect_filled(mark, 0., Color32::WHITE);
                    chart_p.rect_stroke(
                        mark,
                        0.,
                        Stroke::new(1_f32, INK),
                        egui::StrokeKind::Inside,
                    );
                }
                if handle.drag_started() {
                    self.link_drag = Some(LinkDrag { activity: i, edge });
                    if self.active_rows().is_none_or(|n| i < n) {
                        self.selected = Some(i);
                    }
                    self.status = "Drag to a successor's start or finish. Escape cancels.".into();
                }
            }
            self.bar_drag_zones(ui, gantt, rect, i, a, (start, finish), &zones);
        }
    }

    /// Whether activity `i`'s bar can be moved or stretched by dragging: a
    /// not-started activity of this project that is not a level of effort,
    /// WBS summary or finish milestone, outside the open-set composite.
    fn bar_movable(&self, i: usize, a: &Activity) -> bool {
        a.status == StatusType::NotStarted
            && !matches!(
                a.task_type,
                TaskType::Loe | TaskType::Wbs | TaskType::FinMile
            )
            && self.open_set.composite.is_none()
            && i < self.project.model.activities.len()
    }

    /// Drag zones of a not-started activity's bar (see [`end_zones`]): the
    /// body moves the start; the stretch zone at the middle of each end's
    /// relationship zone moves that end, the finish end keeping the start
    /// and the start end keeping the finish. A live hint names the dates;
    /// release applies them through the activity setters as one undo step.
    /// A moved bar asks about a Start On or After hold as a typed Start does;
    /// a stretched start end is held without asking. Registered after the
    /// relationship zones, so they win where
    /// they overlap. Started and completed bars have no such zones.
    #[allow(clippy::too_many_arguments)]
    fn bar_drag_zones(
        &mut self,
        ui: &mut egui::Ui,
        gantt: &GanttFrame,
        rect: Rect,
        i: usize,
        a: &Activity,
        (start, finish): (i64, i64),
        zones: &EndZones,
    ) {
        let parts = [
            (BarPart::Body, zones.body),
            (BarPart::Start, zones.start_stretch),
            (BarPart::Finish, zones.finish_stretch),
        ];
        for (part, zone) in parts {
            let Some(zone) = zone else {
                continue;
            };
            let id = ui.id().with(("bar_drag", a.task_id, part as u8));
            let response =
                ui.interact(zone, id, Sense::drag())
                    .on_hover_cursor(if part == BarPart::Body {
                        egui::CursorIcon::Grab
                    } else {
                        egui::CursorIcon::ResizeHorizontal
                    });
            self.action(
                &format!(
                    "bar_{}_{}",
                    match part {
                        BarPart::Body => "body",
                        BarPart::Start => "start",
                        BarPart::Finish => "finish",
                    },
                    a.task_id
                ),
                &response,
            );
            if response.drag_started() {
                self.gantt_ui.bar_drag = Some(BarDrag {
                    id,
                    activity: i,
                    part,
                    start,
                    finish,
                    minutes: 0,
                });
                if self.active_rows().is_none_or(|n| i < n) {
                    self.selected = Some(i);
                }
            }
            let Some(mut drag) = self.gantt_ui.bar_drag.filter(|d| d.id == id) else {
                continue;
            };
            if response.dragged() {
                let travel = ui.ctx().input(|input| {
                    input
                        .pointer
                        .interact_pos()
                        .zip(input.pointer.press_origin())
                        .map_or(0., |(pos, origin)| pos.x - origin.x)
                });
                let minutes = f64::from(travel / self.pixels_per_day.max(0.01)) * DAY as f64;
                drag.minutes = (minutes / DROP_STEP as f64).round() as i64 * DROP_STEP;
            }
            let (new_start, new_finish, duration) = self.bar_drag_dates(&drag);
            if response.dragged() {
                self.gantt_ui.bar_drag = Some(drag);
                // The bar's new place, outlined, and the dates in a hint.
                let axis = &gantt.axis;
                let ghost = Rect::from_min_max(
                    Pos2::new(axis.x(new_start as f64), rect.top() + bars::BOX_INSET),
                    Pos2::new(
                        axis.x(new_finish as f64).max(axis.x(new_start as f64) + 2.),
                        rect.top() + bars::BOX_INSET + bars::BOX_HEIGHT,
                    ),
                );
                ui.painter().with_clip_rect(gantt.body).rect_stroke(
                    ghost,
                    0.,
                    Stroke::new(1_f32, INK),
                    egui::StrokeKind::Inside,
                );
                let hint = self.bar_drag_hint(&drag, new_start, new_finish, duration);
                if let Some(pos) = ui.ctx().input(|i| i.pointer.interact_pos()) {
                    egui::Area::new(egui::Id::new("bar_drag_hint"))
                        .order(egui::Order::Tooltip)
                        .fixed_pos(pos + Vec2::new(12., 12.))
                        .interactable(false)
                        .show(ui.ctx(), |ui| {
                            egui::Frame::popup(ui.style()).show(ui, |ui| {
                                ui.label(&hint);
                            });
                        });
                }
                self.status = hint;
            }
            if response.drag_stopped() {
                self.gantt_ui.bar_drag = None;
                if (new_start, new_finish) != (drag.start, drag.finish) {
                    self.finish_bar_drag(&drag, new_start, duration);
                }
            }
        }
    }

    /// The dates a bar drag would set, on the activity's calendar.
    pub(crate) fn bar_drag_dates(&self, drag: &BarDrag) -> (i64, i64, i64) {
        let a = &self.project.model.activities[drag.activity];
        let periods = format::activity_periods(&self.project, drag.activity);
        drag_dates(
            self.project.model.calendars.get(a.cal),
            drag.part,
            (drag.start, drag.finish, a.rem),
            periods.day,
            drag.minutes,
        )
    }

    fn bar_drag_hint(&self, drag: &BarDrag, start: i64, finish: i64, duration: i64) -> String {
        let origin = self.project.model.origin_serial;
        let periods = format::activity_periods(&self.project, drag.activity);
        match drag.part {
            BarPart::Finish => format!(
                "Finish: {}   Original Duration: {}",
                format::date(origin, Some(finish)),
                format::duration(duration, periods)
            ),
            BarPart::Start => format!(
                "Start: {}   Original Duration: {}",
                format::date(origin, Some(start)),
                format::duration(duration, periods)
            ),
            BarPart::Body => format!(
                "Start: {}   Finish: {}",
                format::date(origin, Some(start)),
                format::date(origin, Some(finish))
            ),
        }
    }

    /// Release of a bar drag, one undo step. The body is a new Start, as if
    /// typed: the same question (or saved answer) of `start_hold`, Yes
    /// holding it with a Start On or After constraint and No moving it
    /// without one. The finish end is one duration change, and the start end
    /// a held start move followed by a duration change in the same frame
    /// (the frame's changes are recorded as one undo step). If the second
    /// setter refuses, the first is put back, so the project is unchanged.
    fn finish_bar_drag(&mut self, drag: &BarDrag, start: i64, duration: i64) {
        use details::setters::Edit;
        let code = self.project.model.activities[drag.activity]
            .task_code
            .clone();
        let applied = match drag.part {
            BarPart::Body => self.type_start(drag.activity, start),
            BarPart::Finish => self.edit_activity(drag.activity, Edit::BarDuration(duration)),
            BarPart::Start if start == drag.start => Ok(false),
            BarPart::Start => {
                let before = self.project.clone();
                self.field_lock(drag.activity, crate::grid::columns::Field::OriginalDuration)
                    .and_then(|()| self.edit_activity(drag.activity, Edit::MoveStart(start)))
                    .and_then(|moved| {
                        match self.edit_activity(drag.activity, Edit::BarDuration(duration)) {
                            Ok(stretched) => Ok(moved || stretched),
                            Err(error) => {
                                self.project = before;
                                Err(error)
                            }
                        }
                    })
            }
        };
        let a = &self.project.model.activities[drag.activity];
        let held = if drag.part != BarPart::Finish
            && a.cstr == CstrType::StartOnAfter
            && a.cstr_date == Some(start)
        {
            "   (Start On or After constraint set)"
        } else {
            ""
        };
        self.status = match applied {
            Ok(true) => format!(
                "{code}: {}{held}",
                self.bar_drag_hint(
                    drag,
                    start,
                    self.project.model.activities[drag.activity]
                        .early_finish
                        .unwrap_or(start),
                    duration
                )
            ),
            Ok(false) => String::new(),
            Err(error) => error,
        };
    }

    /// Text attachments drawn on top of the chart: a white box with a thin
    /// frame at the anchored activity's row and date, dragged to move it.
    /// Returns each drawn box for hit testing.
    fn paint_text_attachments(
        &mut self,
        ui: &mut egui::Ui,
        gantt: &GanttFrame,
        clip: Rect,
    ) -> Vec<(usize, Rect)> {
        let painter = ui.painter().with_clip_rect(clip);
        let mut drawn = vec![];
        for k in 0..self.gantt.texts.len() {
            let t = &self.gantt.texts[k];
            let anchor = match &t.band {
                Some(key) => RowAnchor::Band(key.clone()),
                None => RowAnchor::Activity(t.task_id),
            };
            let Some(row) = self
                .row_geometry
                .iter()
                .find(|(a, _)| *a == anchor)
                .map(|(_, r)| *r)
            else {
                continue;
            };
            let f = t.font;
            let mut job = egui::text::LayoutJob::single_section(
                t.text.clone(),
                egui::TextFormat {
                    font_id: f.face.font(f.points, f.bold),
                    color: f.color.color(),
                    italics: f.italic,
                    ..Default::default()
                },
            );
            job.wrap.max_width = f32::INFINITY;
            let galley = painter.layout_job(job);
            let ink = f.color.color();
            let x = gantt.axis.x(t.date as f64).round();
            let mut rect = Rect::from_min_size(
                Pos2::new(x, row.top() + 1.),
                galley.size().ceil() + Vec2::new(6., 4.),
            );
            let id = ui.id().with(("text_attachment", k));
            let response = ui.interact(rect.intersect(clip), id, Sense::click_and_drag());
            self.action(&format!("text_attachment_{k}"), &response);
            if response.dragged() {
                let delta =
                    self.gantt_ui.text_drag.map_or(Vec2::ZERO, |(_, d)| d) + response.drag_delta();
                self.gantt_ui.text_drag = Some((k, delta));
            }
            if let Some((_, delta)) = self.gantt_ui.text_drag.filter(|(i, _)| *i == k) {
                rect = rect.translate(delta);
                if response.drag_stopped() {
                    self.gantt_ui.text_drag = None;
                    let date = gantt.axis.x_date(rect.left()).round() as i64;
                    let anchor = self.row_anchor_at(rect.top() + 2.);
                    let t = &mut self.gantt.texts[k];
                    t.date = date.div_euclid(60) * 60;
                    match anchor {
                        Some(RowAnchor::Activity(task)) => {
                            t.task_id = task;
                            t.band = None;
                        }
                        Some(RowAnchor::Band(key)) => t.band = Some(key),
                        None => {}
                    }
                }
            }
            painter.rect_filled(rect, 0., Color32::WHITE);
            painter.rect_stroke(rect, 0., Stroke::new(1_f32, ink), egui::StrokeKind::Inside);
            painter.galley(rect.min + Vec2::new(3., 2.), galley, ink);
            drawn.push((k, rect));
        }
        drawn
    }

    /// After Attachments > Text: a click on the chart opens the new text
    /// box's dialog at that date and row.
    fn place_text_on_click(&mut self, ui: &egui::Ui, axis: &TimeScale, clip: Rect) {
        let Some(since) = self.gantt_ui.placing_text else {
            return;
        };
        let ctx = ui.ctx();
        // Only a pointer over the chart itself, not a menu or dialog above it.
        let at = ctx
            .input(|i| i.pointer.hover_pos())
            .filter(|p| clip.contains(*p) && ctx.layer_id_at(*p) == Some(ui.layer_id()));
        if at.is_some() {
            ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        if self.frame_number <= since || !ctx.input(|i| i.pointer.primary_clicked()) {
            return;
        }
        if let Some(pos) = at {
            if let Some(anchor) = self.row_anchor_at(pos.y) {
                let minute = axis.x_date(pos.x).round() as i64;
                self.place_text_attachment(minute, anchor);
            }
        }
    }

    /// Driving flags for every relationship, recomputed only when dates or
    /// links change.
    fn relationship_driving(&mut self) -> std::rc::Rc<Vec<Option<bool>>> {
        let key = if let Some(serial) = self.shown_serial() {
            dialogs::DrivingKey::Serial(serial)
        } else {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            for a in &self.shown().model.activities {
                (
                    a.early_start,
                    a.early_finish,
                    a.total_float,
                    a.act_start,
                    a.act_finish,
                )
                    .hash(&mut hasher);
            }
            for l in &self.shown().model.links {
                (l.pred, l.succ, l.link_type.as_str(), l.lag).hash(&mut hasher);
            }
            dialogs::DrivingKey::Hash(hasher.finish())
        };
        if self.gantt_ui.driving_key != Some(key) {
            let model = match self.open_set.composite.as_mut() {
                Some(c) => &mut c.project.model,
                None => &mut self.project.model,
            };
            let scheduler = openxer_engine::scheduler::Scheduler::new(model);
            self.gantt_ui.driving = std::rc::Rc::new(scheduler.relationship_driving_all());
            self.gantt_ui.driving_key = Some(key);
        }
        self.gantt_ui.driving.clone()
    }

    fn progress_point(&self, gantt: &GanttFrame, i: usize, a: &Activity, data_date: i64) -> i64 {
        let leveled = self.shown().model.options.do_level;
        let line = self.gantt.options.progress_line;
        if line.mode == settings::ProgressMode::Difference {
            if a.is_completed() {
                return data_date;
            }
            let Some(bl) = self
                .baseline_compare()
                .dates(self.shown(), a, line.baseline)
            else {
                return data_date;
            };
            let (current, baseline) = match line.date {
                settings::ProgressDate::Start => (displayed_start(a, leveled), bl.start),
                settings::ProgressDate::Finish => (displayed_finish(a, leveled), bl.finish),
            };
            return match (current, baseline) {
                (Some(c), Some(b)) => data_date + (b - c),
                _ => data_date,
            };
        }
        if a.status != openxer_engine::model::StatusType::InProgress {
            return data_date;
        }
        let (Some(start), Some(finish)) =
            (displayed_start(a, leveled), displayed_finish(a, leveled))
        else {
            return data_date;
        };
        match self.gantt.options.progress_line.basis {
            settings::ProgressBasis::PercentComplete => {
                let pct = self.subject(&gantt.ix, i, a).percent.clamp(0., 100.) / 100.;
                start + ((finish - start) as f64 * pct).round() as i64
            }
            settings::ProgressBasis::RemainingDuration => (finish - a.rem).max(start),
        }
    }

    fn paint_legend(&self, painter: &egui::Painter, clip: Rect) {
        let shown: Vec<_> = self
            .gantt
            .bars
            .iter()
            .filter(|b| {
                b.display
                    && (b.middle.shape != settings::BodyShape::None
                        || b.start.shape != settings::EndShape::None)
            })
            .collect();
        if shown.is_empty() {
            return;
        }
        let height = shown.len() as f32 * 16. + 8.;
        let rect = Rect::from_min_size(
            Pos2::new(clip.left() + 8., clip.bottom() - height - 8.),
            Vec2::new(200., height),
        );
        painter.rect_filled(rect, 0., Color32::WHITE);
        painter.rect_stroke(rect, 0., Stroke::new(1_f32, INK), egui::StrokeKind::Inside);
        for (k, def) in shown.iter().enumerate() {
            let top = rect.top() + 4. + k as f32 * 16. - 2.;
            let draw = bars::layout_bar(
                def,
                rect.left(),
                top,
                rect.left() + 10.,
                rect.left() + 50.,
                &[],
            );
            bars::paint(painter, &draw);
            painter.text(
                Pos2::new(rect.left() + 62., top + 7.5),
                Align2::LEFT_CENTER,
                &def.name,
                FontId::proportional(FONT_SIZE),
                INK,
            );
        }
    }

    /// Data date line, progress line, relationship lines, legend and chart
    /// pointer handling, drawn after all rows so they sit on top.
    pub(super) fn gantt_overlay(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        gantt: &GanttFrame,
        clip: Rect,
        // Every table row in order, when the rows are the scrolled table's
        // (`grid.row_offsets` and `grid.body_top` place them): relationship
        // lines then also reach bars scrolled or panned out of view.
        rows: Option<&std::rc::Rc<Vec<Row>>>,
    ) {
        let axis = &gantt.axis;
        let ppd = self.pixels_per_day;
        let clip = clip.intersect(gantt.body);
        let painter = ui.painter().with_clip_rect(clip);
        let options = self.gantt.options;
        if let Some(dd) = self.shown().model.project.data_date {
            let column = axis.x(dd as f64);
            let x = column + 0.5;
            paint::vline(
                &painter,
                column,
                clip.top(),
                clip.bottom(),
                f32::from(options.data_date.size.max(1)),
                options.data_date.color.color(),
                options.data_date.style,
                gantt.body.top(),
            );
            if options.progress_line.show {
                let mut rows: Vec<_> = self
                    .bar_geometry
                    .iter()
                    .map(|b| (b.row, b.activity))
                    .collect();
                rows.sort_by(|a, b| a.0.top().total_cmp(&b.0.top()));
                let mut points = vec![];
                for (row, i) in rows {
                    let point =
                        self.progress_point(gantt, i, &self.shown().model.activities[i], dd);
                    let px = axis.x(point as f64);
                    points.push(Pos2::new(x, row.top()));
                    points.push(Pos2::new(
                        px,
                        row.top() + bars::BOX_INSET + bars::BOX_HEIGHT / 2.,
                    ));
                    points.push(Pos2::new(x, row.bottom()));
                }
                if points.len() > 1 {
                    painter.add(egui::Shape::line(
                        points,
                        Stroke::new(
                            f32::from(options.progress_line.size),
                            options.progress_line.color.color(),
                        ),
                    ));
                }
            }
        }
        // Progress Spotlight: dragging the data date line (or the
        // spotlight's edge) moves the spotlight's end.
        if let Some((dd, end)) = self.spotlight_window() {
            paint::vline(
                &painter,
                axis.x(end as f64),
                clip.top(),
                clip.bottom(),
                1.,
                options.data_date.color.color(),
                LineStyle::Dashed,
                gantt.body.top(),
            );
            for (name, minute) in [("spotlight_data_date", dd), ("spotlight_end", end)] {
                let x = axis.x(minute as f64);
                let handle = Rect::from_min_max(
                    Pos2::new(x - 3., clip.top()),
                    Pos2::new(x + 4., clip.bottom()),
                );
                let response = ui
                    .interact(handle, ui.id().with(name), Sense::drag())
                    .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
                self.action(name, &response);
                if response.dragged() {
                    if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                        self.drag_spotlight_to(axis.x_date(pos.x).round() as i64);
                    }
                }
            }
        }
        if options.show_relationships {
            let driving = self.relationship_driving();
            let colors = options.relationship_colors;
            let mut geometry = self.bar_geometry.clone();
            if options.collapsed_relationships {
                geometry.extend(self.gantt_ui.collapsed_geometry.iter().copied());
            }
            let mut drawn = vec![];
            // The first bar drawn for each activity.
            let mut first_bar: std::collections::HashMap<usize, usize> =
                std::collections::HashMap::with_capacity(geometry.len());
            for (at, b) in geometry.iter().enumerate() {
                first_bar.entry(b.activity).or_insert(at);
            }
            let row_of = rows.map(|r| self.shown_row_of(r));
            let serial = self.shown_serial();
            let candidates = self.visible_link_indices(
                rows,
                serial,
                row_of.as_deref().map(Vec::as_slice),
                clip,
                options.collapsed_relationships,
            );
            let unpainted = |activity: usize| -> Option<BarGeometry> {
                let row_of = row_of.as_ref()?;
                let k = (*row_of.get(activity)?)?;
                let top = self.grid.body_top + self.grid.row_offsets.get(k)?;
                let height = self.grid.row_offsets.get(k + 1)? - self.grid.row_offsets[k];
                offscreen_geometry(self.shown(), axis, activity, top, height, clip)
            };
            let batch_mesh = rows.is_some() && !paint::is_compact_dashes();
            let mut rel_mesh = batch_mesh.then(egui::Mesh::default);
            let paint_link = |index: usize,
                              link: &openxer_engine::model::Link,
                              drawn: &mut Vec<(usize, Vec<Pos2>)>,
                              rel_mesh: &mut Option<egui::Mesh>| {
                let pred = first_bar
                    .get(&link.pred)
                    .map(|&at| geometry[at])
                    .or_else(|| unpainted(link.pred));
                let succ = first_bar
                    .get(&link.succ)
                    .map(|&at| geometry[at])
                    .or_else(|| unpainted(link.succ));
                // Lines whose ends are both out of view are drawn only when
                // they cross the chart.
                let crosses = |a: &BarGeometry, b: &BarGeometry| {
                    let span = Rect::from_two_pos(a.start, a.finish)
                        .union(Rect::from_two_pos(b.start, b.finish))
                        .union(a.row)
                        .union(b.row)
                        .expand(20.);
                    span.intersects(clip)
                };
                let ends = pred.zip(succ).filter(|(p, s)| crosses(p, s));
                if let Some((pred, succ)) =
                    ends.map(|(p, s)| (keep_near(p, clip), keep_near(s, clip)))
                {
                    let (pred, succ) = (&pred, &succ);
                    let from_start = matches!(link.link_type, LinkType::SS | LinkType::SF);
                    let to_start = matches!(link.link_type, LinkType::FS | LinkType::SS);
                    let from = if from_start { pred.start } else { pred.finish };
                    let to = if to_start { succ.start } else { succ.finish };
                    let path = link_path(from, from_start, to, to_start, succ.row.top());
                    let is_driving = driving.get(index).copied().flatten().unwrap_or(false);
                    let models = &self.shown().model.activities;
                    let definition = gantt.ix.critical;
                    let critical = is_driving
                        && crate::grid::columns::is_critical(&models[link.pred], definition)
                        && crate::grid::columns::is_critical(&models[link.succ], definition);
                    let color = if critical {
                        colors.critical
                    } else if is_driving {
                        colors.driving
                    } else {
                        colors.non_driving
                    }
                    .color();
                    // The right-clicked line is drawn selected (solid, in the
                    // selection color) while its menu or dialog is up.
                    let picked = self.gantt_ui.picked_link == Some(index);
                    let color = if picked { crate::chrome::SELECT } else { color };
                    let style = if !picked && !is_driving && colors.non_driving_dotted {
                        LineStyle::Dotted
                    } else {
                        LineStyle::Solid
                    };
                    let is_loop = self
                        .loops
                        .view
                        .as_ref()
                        .is_some_and(|v| v.links.contains(&(link.pred, link.succ)));
                    if let Some(mesh) = rel_mesh.as_mut() {
                        paint::pixel_path_into(mesh, &path, color, style);
                        if is_loop {
                            let heavy: Vec<(f32, f32)> =
                                path.iter().map(|&(x, y)| (x + 1., y + 1.)).collect();
                            let red = Color32::from_rgb(200, 0, 0);
                            paint::pixel_path_into(mesh, &path, red, LineStyle::Solid);
                            paint::pixel_path_into(mesh, &heavy, red, LineStyle::Solid);
                        }
                        if let Some(&(tip_x, tip_y)) = path.last() {
                            paint::spans_into(
                                mesh,
                                &paint::arrowhead(tip_x, tip_y, to_start),
                                color,
                            );
                        }
                    } else {
                        paint::pixel_path(&painter, &path, color, style);
                        if is_loop {
                            let heavy: Vec<(f32, f32)> =
                                path.iter().map(|&(x, y)| (x + 1., y + 1.)).collect();
                            let red = Color32::from_rgb(200, 0, 0);
                            paint::pixel_path(&painter, &path, red, LineStyle::Solid);
                            paint::pixel_path(&painter, &heavy, red, LineStyle::Solid);
                        }
                        if let Some(&(tip_x, tip_y)) = path.last() {
                            paint::spans(
                                &painter,
                                &paint::arrowhead(tip_x, tip_y, to_start),
                                color,
                            );
                        }
                    }
                    drawn.push((index, link_points(&path)));
                }
            };
            let links = &self.shown().model.links;
            match candidates {
                Some(indices) => {
                    for index in indices {
                        paint_link(index, &links[index], &mut drawn, &mut rel_mesh);
                    }
                }
                None => {
                    for (index, link) in links.iter().enumerate() {
                        paint_link(index, link, &mut drawn, &mut rel_mesh);
                    }
                }
            }
            if let Some(mesh) = rel_mesh {
                if !mesh.is_empty() {
                    painter.add(egui::Shape::mesh(mesh));
                }
            }
            self.link_geometry.extend(drawn);
        }
        self.paint_deferred_labels(gantt, &painter);
        let texts = self.paint_text_attachments(ui, gantt, clip);
        self.place_text_on_click(ui, axis, clip);
        if options.show_legend {
            self.paint_legend(&painter, clip);
        }
        // A double-click on a relationship line opens Edit Relationship; a
        // right-click on one selects it and opens the chart menu, and Edit
        // Relationship follows when that menu closes, as observed in the
        // reference. A right-click elsewhere (a bar, a summary bar, a
        // milestone, the empty chart) opens the chart menu.
        let secondary = ctx.input(|i| i.pointer.button_clicked(egui::PointerButton::Secondary));
        let double = ctx.input(|i| {
            i.pointer
                .button_double_clicked(egui::PointerButton::Primary)
        });
        if secondary || double {
            if let Some(pos) = ctx
                .input(|i| i.pointer.interact_pos())
                .filter(|pos| clip.contains(*pos))
            {
                if let Some(&(text, _)) = texts.iter().rev().find(|(_, r)| r.contains(pos)) {
                    self.edit_text_attachment(text);
                } else if let Some(index) = pick_link(pos, &self.link_geometry) {
                    // Relationships of other open projects (and between
                    // projects) are edited in the Predecessors and
                    // Successors grids.
                    if index < self.project.model.links.len() {
                        if secondary {
                            self.open_link_menu(pos, index);
                        } else {
                            self.edit_link(index);
                        }
                    }
                } else if secondary {
                    self.open_gantt_menu(pos);
                }
            }
        }
        self.relationship_gesture(ctx, ui, clip);
        if ctx.input(|i| i.pointer.hover_pos().is_some_and(|pos| clip.contains(pos))) {
            let scroll = ctx.input(|i| i.smooth_scroll_delta);
            if let Some((factor, anchor)) = crate::timeline::wheel_zoom(ctx, clip) {
                self.zoom_at(self.pixels_per_day as f64 * 7. * factor as f64, anchor);
            } else if scroll.x != 0. {
                self.timeline_origin -= (scroll.x / ppd * DAY as f32).round() as i64;
            }
            if scroll != Vec2::ZERO
                || ctx.input(|i| i.pointer.any_pressed() || i.zoom_delta() != 1.)
            {
                self.touch_gantt_timeline();
            }
        }
        self.gantt_menu(ctx, axis);
        // The selected line goes back to its own color once its menu and
        // Edit Relationship are closed.
        if self.gantt_ui.menu.is_none() && self.link_editor.is_none() {
            self.gantt_ui.picked_link = None;
        }
    }

    fn visible_link_indices(
        &mut self,
        rows: Option<&std::rc::Rc<Vec<Row>>>,
        serial: Option<u64>,
        row_of: Option<&[Option<usize>]>,
        clip: Rect,
        collapsed: bool,
    ) -> Option<Vec<usize>> {
        let (rows_rc, serial, row_of) = (rows?, serial?, row_of?);
        if rows_rc.is_empty() {
            return Some(vec![]);
        }
        if self.grid.row_offsets.len() != rows_rc.len() + 1 {
            return None;
        }
        let need_rebuild = !matches!(
            &self.gantt_ui.link_index,
            Some(c) if c.serial == serial && std::rc::Rc::ptr_eq(&c.rows, rows_rc)
        );
        if need_rebuild {
            let model = &self.shown().model;
            let built = build_link_index(
                serial,
                rows_rc,
                model.activities.len(),
                &model.links,
                row_of,
            );
            self.gantt_ui.link_index = Some(built);
        }
        let idx = self.gantt_ui.link_index.as_ref()?;
        let collapsed_bars = collapsed.then_some(self.gantt_ui.collapsed_geometry.as_slice());
        Some(collect_visible_links(
            idx,
            &self.grid.row_offsets,
            self.grid.body_top,
            clip,
            collapsed_bars,
        ))
    }

    fn relationship_gesture(&mut self, ctx: &egui::Context, ui: &mut egui::Ui, clip: Rect) {
        let Some(drag) = self.link_drag else {
            return;
        };
        let pointer = ctx.input(|i| i.pointer.interact_pos());
        let target = pointer.and_then(|pos| {
            self.bar_geometry
                .iter()
                .flat_map(|bar| {
                    [
                        (bar.activity, Edge::Start, bar.start_hit, bar.start),
                        (bar.activity, Edge::Finish, bar.finish_hit, bar.finish),
                    ]
                })
                .filter(|(_, _, hit, _)| hit.expand(3.).contains(pos))
                // Where the zones of a short bar's two ends (or of
                // neighbouring bars) both reach, the nearer end.
                .min_by(|a, b| a.3.distance(pos).total_cmp(&b.3.distance(pos)))
                .map(|(i, e, _, _)| (i, e))
        });
        if let (Some(from), Some(pos)) = (
            self.bar_geometry
                .iter()
                .find(|bar| bar.activity == drag.activity),
            pointer,
        ) {
            let begin = if drag.edge == Edge::Start {
                from.start
            } else {
                from.finish
            };
            let valid = target.is_some_and(|(i, edge)| {
                relationships::can_connect(&self.shown().model, drag.activity, i).is_ok()
                    && !self.shown().model.links.iter().any(|l| {
                        l.pred == drag.activity
                            && l.succ == i
                            && l.link_type == relationships::kind(drag.edge, edge)
                    })
            });
            ui.painter().with_clip_rect(clip).line_segment(
                [begin, pos],
                Stroke::new(1.5_f32, if valid { BLUE } else { CRITICAL }),
            );
            let hint = if let Some((i, edge)) = target {
                format!(
                    "{} → {}   {}",
                    self.shown().model.activities[drag.activity].task_code,
                    self.shown().model.activities[i].task_code,
                    &relationships::kind(drag.edge, edge).as_str()[3..]
                )
            } else {
                "Drag to a successor's start or finish · Esc to cancel".into()
            };
            chrome::paint_pointer_icon(ctx, chrome::Icon::Links);
            // Right of the pointer icon, not over it.
            egui::Area::new(egui::Id::new("relationship_hint"))
                .order(egui::Order::Tooltip)
                .fixed_pos(pos + chrome::POINTER_ICON_OFFSET + Vec2::new(19., 7.))
                .interactable(false)
                .show(ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.label(hint);
                    });
                });
        }
        if ctx.input(|i| i.pointer.button_released(egui::PointerButton::Primary)) {
            self.link_drag = None;
            if let Some((successor, edge)) = target {
                let kind = relationships::kind(drag.edge, edge);
                let names = (
                    self.shown().model.activities[drag.activity]
                        .task_code
                        .clone(),
                    self.shown().model.activities[successor].task_code.clone(),
                );
                if let Some(result) = self.cross_project_drop(drag.activity, successor, kind) {
                    self.status = match result {
                        Ok(()) => format!(
                            "Created {} → {} ({}) between projects",
                            names.0,
                            names.1,
                            &kind.as_str()[3..]
                        ),
                        Err(error) => format!("Relationship not created: {error}"),
                    };
                    return;
                }
                self.unpin_serial();
                match relationships::add(&mut self.project.model, drag.activity, successor, kind, 0)
                {
                    Ok(()) => {
                        self.dirty = true;
                        self.check_relationship_sequence(self.project.model.links.len() - 1, None);
                        self.status = format!(
                            "Created {} → {} ({})",
                            self.shown().model.activities[drag.activity].task_code,
                            self.shown().model.activities[successor].task_code,
                            &kind.as_str()[3..]
                        );
                    }
                    Err(error) => self.status = format!("Relationship not created: {error}"),
                }
            } else {
                self.status = "Relationship cancelled: release on an activity endpoint.".into();
            }
        }
    }

    /// The pointer used the Gantt being drawn: the zoom commands act on it.
    pub(crate) fn touch_gantt_timeline(&mut self) {
        self.panes.timeline_focus = if self.panes.grid.drawing {
            crate::timeline::Focus::BottomGantt
        } else {
            crate::timeline::Focus::Activities
        };
    }

    /// One step along the measured zoom ladder, keeping the chart's centre
    /// date in place (Zoom In and Zoom Out).
    pub(super) fn zoom_ladder(&mut self, zoom_in: bool) {
        let scale = timescale::zoom_step(f64::from(self.pixels_per_day) * 7., zoom_in);
        self.zoom_at(scale, self.gantt_left + self.chart_width / 2.);
    }

    /// Applies new Gantt settings: the date interval drives the view's bands.
    pub(super) fn apply_gantt(&mut self, settings: GanttSettings) -> Result<(), String> {
        settings.validate()?;
        let interval_changed = settings.timescale.interval != self.gantt.timescale.interval;
        self.gantt = settings;
        match interval_units(self.gantt.timescale.interval) {
            Some((upper, lower)) => {
                self.view.automatic_units = false;
                self.view.upper = upper;
                self.view.lower = lower;
                self.pixels_per_day = self.pixels_per_day.max((self.minimum_scale() / 7.) as f32);
                if interval_changed {
                    // A new date interval also sets a readable zoom for its lower band.
                    self.zoom_at(
                        interval_scale(self.gantt.timescale.interval),
                        self.gantt_left + self.chart_width / 2.,
                    );
                }
            }
            None if interval_changed => {
                self.view.automatic_units = true;
                self.update_timescale_units();
            }
            None => {}
        }
        self.save_gantt();
        Ok(())
    }

    pub(super) fn save_gantt(&mut self) {
        let json = self.gantt.to_json();
        match self.store.set_preference(GANTT_KEY, &json) {
            Ok(()) => self.gantt_ui.snapshot = json,
            Err(error) => self.error(format!("Cannot save Gantt settings: {error}")),
        }
    }

    /// Loads saved settings. Invalid data is rejected and the defaults stay.
    /// Without saved settings, the earlier view switches are carried over.
    pub(super) fn restore_gantt(&mut self) {
        let view_stored = matches!(self.store.preference("activities_view_v1"), Ok(Some(_)));
        let mut gantt_stored = true;
        match self.store.preference(GANTT_KEY) {
            Ok(Some(json)) => match GanttSettings::from_json(&json) {
                Ok(mut settings) => {
                    crate::theme::legacy::upgrade_gantt(&mut settings);
                    self.gantt = settings;
                }
                Err(error) => {
                    self.gantt = GanttSettings::default();
                    if view_stored {
                        // The stored view keeps the bands it had.
                        self.gantt.timescale.interval = DateInterval::Automatic;
                    }
                    self.status =
                        format!("Saved Gantt settings were rejected ({error}); defaults restored.");
                }
            },
            _ => {
                gantt_stored = false;
                if view_stored {
                    // A workspace from before the Gantt settings were kept:
                    // its date interval followed its stored view, as it did.
                    self.gantt.timescale.interval = DateInterval::Automatic;
                }
                if let Ok(Some(json)) = self.store.preference("activities_view_v1") {
                    if let Ok(old) = serde_json::from_str::<serde_json::Value>(&json) {
                        let flag = |key: &str| old[key].as_bool();
                        let options = &mut self.gantt.options;
                        if let Some(v) = flag("shade_nonwork") {
                            options.nonwork.shade = v;
                        }
                        if let Some(v) = flag("hatch_nonwork") {
                            options.nonwork.hatch = v;
                        }
                        if let Some(v) = flag("show_links") {
                            options.show_relationships = v;
                        }
                        if let Some(v) = flag("show_float") {
                            for bar in &mut self.gantt.bars {
                                if bar.timescale == settings::BarSource::FloatBar {
                                    bar.display = v;
                                }
                            }
                        }
                    }
                }
            }
        }
        let fresh_bands = (!gantt_stored && !view_stored)
            .then(|| interval_units(self.gantt.timescale.interval))
            .flatten();
        if let Some((upper, lower)) = fresh_bands {
            // A new workspace: the default date interval sets the bands and
            // a readable zoom for its lower band, as choosing it would.
            self.view.automatic_units = false;
            self.view.upper = upper;
            self.view.lower = lower;
            self.pixels_per_day = (interval_scale(self.gantt.timescale.interval) / 7.) as f32;
        } else if !self.view.automatic_units {
            if let Some(interval) = DateInterval::ALL
                .into_iter()
                .find(|&i| interval_units(i) == Some((self.view.upper, self.view.lower)))
                .filter(|_| self.gantt.timescale.interval == DateInterval::Automatic)
            {
                self.gantt.timescale.interval = interval;
            }
        } else {
            self.gantt.timescale.interval = DateInterval::Automatic;
        }
        self.gantt_ui.snapshot = self.gantt.to_json();
    }

    /// Saves settings changed outside the dialogs (menus, toolbar).
    pub(super) fn persist_gantt_when_idle(&mut self) {
        let json = self.gantt.to_json();
        if json != self.gantt_ui.snapshot && self.gantt.validate().is_ok() {
            self.save_gantt();
        }
        self.gantt_ui.collapsed_geometry.clear();
    }
}

/// How far from a drawn relationship line a click still picks it, in pixels.
pub(super) const LINK_HIT: f32 = 4.;

/// Pixel-centre points of a relationship route, as hit testing sees it.
pub(super) fn link_points(path: &[(f32, f32)]) -> Vec<Pos2> {
    path.iter()
        .map(|&(x, y)| Pos2::new(x + 0.5, y + 0.5))
        .collect()
}

/// The relationship under `pos`: the drawn line with the nearest segment
/// within `LINK_HIT` pixels. Lines later in `lines` are drawn on top, so
/// they win a tie.
pub(super) fn pick_link(pos: Pos2, lines: &[(usize, Vec<Pos2>)]) -> Option<usize> {
    let mut best: Option<(usize, f32)> = None;
    for (index, points) in lines {
        let distance = points
            .windows(2)
            .map(|pair| segment_distance(pos, pair[0], pair[1]))
            .fold(f32::INFINITY, f32::min);
        if distance <= LINK_HIT && best.is_none_or(|(_, b)| distance <= b) {
            best = Some((*index, distance));
        }
    }
    best.map(|(index, _)| index)
}

/// The relationship-type picture of Edit Relationship, drawn with the
/// chart's own pieces: two bars in the default remaining-work blue and
/// the relationship route between the ends the type ties, with its arrow.
pub(crate) fn relationship_picture(painter: &egui::Painter, area: Rect, kind: LinkType) {
    let painter = painter.with_clip_rect(area);
    let o = area.min;
    // Bar spans (x from the area's left) chosen so every route runs
    // directly, inside the area.
    let ((p0, p1), (s0, s1)) = match kind {
        LinkType::FS | LinkType::None => ((4., 22.), (38., 60.)),
        LinkType::SS => ((14., 44.), (22., 60.)),
        LinkType::FF => ((4., 46.), (18., 40.)),
        LinkType::SF => ((32., 60.), (4., 18.)),
    };
    let (pred_top, succ_top) = (o.y + 12., o.y + 40.);
    let green = crate::theme::color(crate::theme::BLUE);
    let dark = Color32::BLACK;
    for (left, right, top) in [(p0, p1, pred_top), (s0, s1, succ_top)] {
        let rect = Rect::from_min_max(
            Pos2::new(o.x + left, top),
            Pos2::new(o.x + right, top + bars::BOX_HEIGHT),
        );
        painter.rect_filled(rect, 0., green);
        painter.rect_stroke(rect, 0., Stroke::new(1_f32, dark), egui::StrokeKind::Inside);
    }
    let from_start = matches!(kind, LinkType::SS | LinkType::SF);
    let to_start = matches!(kind, LinkType::FS | LinkType::SS);
    let mid = bars::BOX_HEIGHT / 2.;
    let from = Pos2::new(o.x + if from_start { p0 } else { p1 - 1. }, pred_top + mid);
    let to = Pos2::new(o.x + if to_start { s0 } else { s1 - 1. }, succ_top + mid);
    let path = link_path(from, from_start, to, to_start, succ_top - bars::BOX_INSET);
    paint::pixel_path(&painter, &path, INK, LineStyle::Solid);
    if let Some(&(x, y)) = path.last() {
        paint::spans(&painter, &paint::arrowhead(x, y, to_start), INK);
    }
}

/// The pixel route of a relationship line, from the column beside the
/// predecessor's end to the arrow tip beside the successor's end. Lines leave
/// and enter horizontally on the bar box's middle row; when the successor end
/// lies behind the elbow the line drops to the line slot above the
/// successor's row, runs back and comes in from the side.
fn link_path(
    from: Pos2,
    from_start: bool,
    to: Pos2,
    to_start: bool,
    succ_top: f32,
) -> Vec<(f32, f32)> {
    const ELBOW: f32 = 6.;
    let fy = from.y.floor();
    let ty = to.y.floor();
    // Bars cover their finish column, so a finish side starts one further.
    let fx = if from_start { from.x - 1. } else { from.x + 1. };
    let tip = if to_start { to.x - 1. } else { to.x + 1. };
    let elbow = if from_start { fx - ELBOW } else { fx + ELBOW };
    let approach = if to_start { tip - ELBOW } else { tip + ELBOW };
    let direct = if to_start {
        approach >= elbow
    } else {
        approach <= elbow
    };
    if direct || fy == ty {
        vec![(fx, fy), (elbow, fy), (elbow, ty), (tip, ty)]
    } else {
        let lane = if ty > fy {
            succ_top - 1.
        } else {
            succ_top + 17.
        };
        vec![
            (fx, fy),
            (elbow, fy),
            (elbow, lane),
            (approach, lane),
            (approach, ty),
            (tip, ty),
        ]
    }
}

/// Where a relationship line meets activity `i` when its bar is not painted
/// this frame (scrolled above or below the view, or panned off the chart):
/// its row's bar line at the activity's dates on the time axis.
fn offscreen_geometry(
    p: &ProjectData,
    axis: &TimeScale,
    i: usize,
    row_top: f32,
    row_height: f32,
    clip: Rect,
) -> Option<BarGeometry> {
    let a = p.model.activities.get(i)?;
    let leveled = p.model.options.do_level;
    let (start, finish) = (displayed_start(a, leveled)?, displayed_finish(a, leveled)?);
    let (start, finish) = match a.task_type {
        TaskType::Mile => (start, start),
        TaskType::FinMile => (finish, finish),
        _ => (start, finish),
    };
    let y = row_top + bars::BOX_INSET + bars::BOX_HEIGHT / 2.;
    let x = |m: i64| axis.x(m as f64);
    Some(BarGeometry {
        activity: i,
        task_id: a.task_id,
        start: Pos2::new(x(start), y),
        finish: Pos2::new(x(finish), y),
        row: Rect::from_min_size(
            Pos2::new(clip.left(), row_top),
            Vec2::new(clip.width(), row_height),
        ),
        start_hit: Rect::NOTHING,
        finish_hit: Rect::NOTHING,
    })
}

/// A line end kept just outside `clip`: a line to a bar far out of view
/// runs off the chart's edge and is clipped there, without painting
/// thousands of hidden pixels. Ends in view are unchanged.
fn keep_near(mut b: BarGeometry, clip: Rect) -> BarGeometry {
    let near = clip.expand(40.);
    let keep = |p: Pos2| {
        Pos2::new(
            p.x.clamp(near.left(), near.right()),
            p.y.clamp(near.top(), near.bottom()),
        )
    };
    b.start = keep(b.start);
    b.finish = keep(b.finish);
    let top = b.row.top().clamp(near.top(), near.bottom());
    b.row = Rect::from_min_size(Pos2::new(b.row.left(), top), b.row.size());
    b
}

/// Scales a drawn bar into a lane of a collapsed band.
fn squeeze(draw: BarDraw, row_top: f32, lane_top: f32, lane_height: f32) -> BarDraw {
    let box_top = row_top + bars::BOX_INSET;
    let scale = lane_height / bars::BOX_HEIGHT;
    let map = |y: f32| lane_top + (y - box_top) * scale;
    let pieces = draw
        .pieces
        .into_iter()
        .map(|piece| match piece {
            bars::Piece::Rect {
                rect,
                color,
                pattern,
                stroke,
            } => bars::Piece::Rect {
                rect: Rect::from_min_max(
                    Pos2::new(rect.left(), map(rect.top())),
                    Pos2::new(rect.right(), map(rect.bottom()).max(map(rect.top()) + 1.)),
                ),
                color,
                pattern,
                stroke,
            },
            bars::Piece::Spans {
                spans,
                color,
                outline,
            } => {
                let mut rows: Vec<paint::Span> = vec![];
                for (y, a, b) in spans {
                    let y = map(y).floor();
                    match rows.last_mut().filter(|row| row.0 == y) {
                        Some(row) => {
                            row.1 = row.1.min(a);
                            row.2 = row.2.max(b);
                        }
                        None => rows.push((y, a, b)),
                    }
                }
                bars::Piece::Spans {
                    spans: rows,
                    color,
                    outline,
                }
            }
        })
        .collect::<Vec<_>>();
    let bounds = pieces
        .iter()
        .fold(Rect::NOTHING, |acc, piece| acc.union(piece.bounds()));
    BarDraw { pieces, bounds }
}

/// A Start or Finish bar label: the date with the table's ` *` when a
/// constraint applies to that end and the date is not an actual. Bar labels
/// show no actual marker, as before.
pub(crate) fn bar_label_date(origin: i64, value: Option<i64>, a: &Activity, end: End) -> String {
    let mark = match crate::grid::marks::mark(a, end) {
        format::DateMark::Constrained => format::DateMark::Constrained,
        _ => format::DateMark::Plain,
    };
    format::date_marked(origin, value, mark)
}

const LINK_BLOCK_ROWS: usize = 64;
const LINK_MAX_SPAN_BLOCKS: usize = 8;

fn build_link_index(
    serial: u64,
    rows: &std::rc::Rc<Vec<Row>>,
    act_len: usize,
    links: &[openxer_engine::model::Link],
    row_of: &[Option<usize>],
) -> dialogs::LinkIndexCache {
    let n_blocks = rows.len().div_ceil(LINK_BLOCK_ROWS);
    let mut blocks = vec![vec![]; n_blocks];
    let mut long_links: Vec<(u32, u32, u32)> = vec![];
    let mut deg = vec![0u32; act_len];
    for l in links {
        if l.pred < act_len {
            deg[l.pred] += 1;
        }
        if l.succ < act_len {
            deg[l.succ] += 1;
        }
    }
    let mut act_offsets = Vec::with_capacity(act_len + 1);
    let mut total = 0u32;
    act_offsets.push(0);
    for &d in &deg {
        total += d;
        act_offsets.push(total);
    }
    let mut cursor = act_offsets[..act_len].to_vec();
    let mut act_links = vec![0u32; total as usize];
    for (idx, l) in links.iter().enumerate() {
        let u = idx as u32;
        if l.pred < act_len {
            let pos = cursor[l.pred] as usize;
            act_links[pos] = u;
            cursor[l.pred] += 1;
        }
        if l.succ < act_len {
            let pos = cursor[l.succ] as usize;
            act_links[pos] = u;
            cursor[l.succ] += 1;
        }
        let Some(rp) = row_of.get(l.pred).copied().flatten() else {
            continue;
        };
        let Some(rs) = row_of.get(l.succ).copied().flatten() else {
            continue;
        };
        let lo = rp.min(rs);
        let hi = rp.max(rs);
        let b0 = lo / LINK_BLOCK_ROWS;
        let b1 = hi / LINK_BLOCK_ROWS;
        if b1 - b0 <= LINK_MAX_SPAN_BLOCKS {
            for block in &mut blocks[b0..=b1] {
                block.push(idx);
            }
        } else {
            long_links.push((lo as u32, hi as u32, u));
        }
    }
    long_links.sort_unstable_by_key(|e| e.0);
    let mut long_max_end = Vec::with_capacity(long_links.len());
    let mut max_end = 0u32;
    for &(_, hi, _) in &long_links {
        max_end = max_end.max(hi);
        long_max_end.push(max_end);
    }
    dialogs::LinkIndexCache {
        serial,
        rows: rows.clone(),
        blocks,
        long_links,
        long_max_end,
        act_offsets,
        act_links,
    }
}

fn collect_visible_links(
    idx: &dialogs::LinkIndexCache,
    offsets: &[f32],
    body_top: f32,
    clip: Rect,
    collapsed_bars: Option<&[BarGeometry]>,
) -> Vec<usize> {
    let last_row = offsets.len().saturating_sub(2);
    let y_lo = clip.top() - 20. - body_top;
    let y_hi = clip.bottom() + 20. - body_top;
    let r_lo = offsets
        .partition_point(|&y| y < y_lo)
        .saturating_sub(1)
        .min(last_row);
    let r_hi = offsets.partition_point(|&y| y <= y_hi).min(last_row);
    let mut candidates = vec![];
    if !idx.blocks.is_empty() && r_lo <= r_hi {
        let b0 = r_lo / LINK_BLOCK_ROWS;
        let b1 = (r_hi / LINK_BLOCK_ROWS).min(idx.blocks.len() - 1);
        for block in &idx.blocks[b0..=b1] {
            candidates.extend_from_slice(block);
        }
    }
    let hi = idx.long_links.partition_point(|e| e.0 <= r_hi as u32);
    let mut k = hi;
    while k > 0 {
        k -= 1;
        if idx.long_max_end[k] < r_lo as u32 {
            break;
        }
        let (_, end_row, link_idx) = idx.long_links[k];
        if end_row >= r_lo as u32 {
            candidates.push(link_idx as usize);
        }
    }
    if let Some(bars) = collapsed_bars {
        let acts = idx.act_offsets.len().saturating_sub(1);
        for b in bars {
            if b.activity < acts {
                let start = idx.act_offsets[b.activity] as usize;
                let end = idx.act_offsets[b.activity + 1] as usize;
                candidates.extend(idx.act_links[start..end].iter().map(|&u| u as usize));
            }
        }
    }
    candidates.sort_unstable();
    candidates.dedup();
    candidates
}
