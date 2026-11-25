//! Calendar-aware timescale math, independent of painting and application state.
//!
//! Coordinates: the scale is pixels per week and the displacement is truncated
//! toward zero before the chart's left edge is added. Cell boundaries use
//! actual Gregorian dates.
//!
//! Labels do not roll their format from cell to cell: every cell in a band carries the
//! band's own format, chosen by the band's role. An upper band (one with a finer
//! band below it) names its period ("Qtr 1, 2026", "June 2026", "Sep 20"); the
//! lowest band names the cell briefly ("Jan", "07", "Sun"). Each cell offers its
//! labels from longest to shortest and the painter picks the first that fits.
//! Lower cells are never split at upper boundaries, so a week that straddles a
//! month stays one cell.

use chrono::{Datelike, Duration, Months, NaiveDate, NaiveDateTime, Timelike};

const MINUTES_PER_DAY: f64 = 1440.0;
const MINUTES_PER_WEEK: f64 = 7.0 * MINUTES_PER_DAY;
/// A bounded sweep protects rendering when a manually selected fine band is
/// paired with an extremely distant zoom. Automatic bands stay well below this.
pub const MAX_CELLS: usize = 8192;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimeScale {
    /// Minute offset relative to the model origin, including fractions while zooming.
    pub first_minute: f64,
    pub pixels_per_week: f64,
    pub left: f32,
}

fn valid_scale(scale: f64) -> f64 {
    if scale.is_finite() {
        scale.max(1.0)
    } else {
        1.0
    }
}

impl TimeScale {
    pub fn new(first_minute: f64, pixels_per_week: f64, left: f32) -> Self {
        Self {
            first_minute: if first_minute.is_finite() {
                first_minute
            } else {
                0.0
            },
            pixels_per_week: valid_scale(pixels_per_week),
            left: if left.is_finite() { left } else { 0.0 },
        }
    }

    /// Truncate the displacement before adding the chart's left edge, matching the reference.
    /// This deliberately differs from flooring negative displacements.
    pub fn x(&self, minute: f64) -> f32 {
        self.left
            + ((minute - self.first_minute) * valid_scale(self.pixels_per_week) / MINUTES_PER_WEEK)
                .trunc() as f32
    }

    /// Continuous inverse; integer-pixel rendering has less than one pixel error.
    pub fn x_date(&self, x: f32) -> f64 {
        self.first_minute
            + (f64::from(x) - f64::from(self.left)) * MINUTES_PER_WEEK
                / valid_scale(self.pixels_per_week)
    }

    /// Change scale while keeping the date beneath the pointer at the same x.
    pub fn zoom_at(&mut self, pixels_per_week: f64, anchor_x: f32) {
        if !anchor_x.is_finite() || !pixels_per_week.is_finite() {
            return;
        }
        let anchor_date = self.x_date(anchor_x);
        self.pixels_per_week = valid_scale(pixels_per_week);
        self.first_minute = anchor_date
            - (f64::from(anchor_x) - f64::from(self.left)) * MINUTES_PER_WEEK
                / self.pixels_per_week;
    }

    /// Positive drag moves the chart contents right, revealing earlier dates.
    pub fn pan_pixels(&mut self, delta: f32) {
        if delta.is_finite() {
            self.first_minute -=
                f64::from(delta) * MINUTES_PER_WEEK / valid_scale(self.pixels_per_week);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Unit {
    Year,
    Quarter,
    Month,
    Week,
    Day,
    Hour,
}

impl Unit {
    pub const fn all() -> [Self; 6] {
        [
            Self::Year,
            Self::Quarter,
            Self::Month,
            Self::Week,
            Self::Day,
            Self::Hour,
        ]
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Year => "Year",
            Self::Quarter => "Quarter",
            Self::Month => "Month",
            Self::Week => "Week",
            Self::Day => "Day",
            Self::Hour => "Hour",
        }
    }

    /// Approximate duration used only for the source-derived zoom ladder.
    pub const fn preset_days(self) -> f64 {
        match self {
            Self::Year => 360.0,
            Self::Quarter => 90.0,
            Self::Month => 30.0,
            Self::Week => 7.0,
            Self::Day => 1.0,
            Self::Hour => 1.0 / 24.0,
        }
    }

    pub fn preset_scale(self, target_pixels: f64) -> f64 {
        valid_scale(7.0 * target_pixels / self.preset_days())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZoomPreset {
    pub label: &'static str,
    pub unit: Unit,
    pub major: Unit,
    pub minor: Unit,
    pub pixels_per_week: f64,
}

/// The measured zoom ladder. Zoom Out from the reference view stepped through
/// 100, 28 and 13 pixels per week and Zoom In came back through 28 and 100 to
/// 200: Quarter/Month at 13, Month/Week at 28, Week/Day at 100 and 200. The
/// Year and Hour rungs extend the ladder by the same factor and are not
/// measured.
pub const ZOOM_LADDER: [f64; 6] = [5.0, 13.0, 28.0, 100.0, 200.0, 4032.0];

/// View > Zoom presets, one per ladder rung.
pub const ZOOM_PRESETS: [ZoomPreset; 6] = [
    ZoomPreset {
        label: "Year",
        unit: Unit::Year,
        major: Unit::Year,
        minor: Unit::Quarter,
        pixels_per_week: ZOOM_LADDER[0],
    },
    ZoomPreset {
        label: "Quarter",
        unit: Unit::Quarter,
        major: Unit::Quarter,
        minor: Unit::Month,
        pixels_per_week: ZOOM_LADDER[1],
    },
    ZoomPreset {
        label: "Month",
        unit: Unit::Month,
        major: Unit::Month,
        minor: Unit::Week,
        pixels_per_week: ZOOM_LADDER[2],
    },
    ZoomPreset {
        label: "Week",
        unit: Unit::Week,
        major: Unit::Week,
        minor: Unit::Day,
        pixels_per_week: ZOOM_LADDER[3],
    },
    ZoomPreset {
        label: "Day",
        unit: Unit::Day,
        major: Unit::Week,
        minor: Unit::Day,
        pixels_per_week: ZOOM_LADDER[4],
    },
    ZoomPreset {
        label: "Hour",
        unit: Unit::Hour,
        major: Unit::Day,
        minor: Unit::Hour,
        pixels_per_week: ZOOM_LADDER[5],
    },
];

/// The next rung of the zoom ladder: the largest rung below the scale when
/// zooming out, the smallest above it when zooming in (Zoom Out from the
/// reference's 101.5 landed on 100). Past either end the scale halves or
/// doubles.
pub fn zoom_step(pixels_per_week: f64, zoom_in: bool) -> f64 {
    let scale = valid_scale(pixels_per_week);
    let next = if zoom_in {
        ZOOM_LADDER
            .iter()
            .copied()
            .find(|&rung| rung > scale + 1e-9)
    } else {
        ZOOM_LADDER
            .iter()
            .rev()
            .copied()
            .find(|&rung| rung < scale - 1e-9)
    };
    valid_scale(next.unwrap_or(if zoom_in { scale * 2.0 } else { scale / 2.0 }))
}

/// Bands for automatic (zoom-driven) timescales. The thresholds sit between
/// the measured ladder rungs so each rung shows the bands the reference showed.
pub fn bands_for_scale(pixels_per_week: f64) -> (Unit, Unit) {
    let scale = valid_scale(pixels_per_week);
    if scale >= 1344.0 {
        (Unit::Day, Unit::Hour)
    } else if scale >= 77.0 {
        // Eleven pixels a day: enough for every weekday letter.
        (Unit::Week, Unit::Day)
    } else if scale >= 20.0 {
        (Unit::Month, Unit::Week)
    } else if scale >= 8.0 {
        (Unit::Quarter, Unit::Month)
    } else {
        (Unit::Year, Unit::Quarter)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cell {
    /// Full calendar boundary; the painter clips it to the visible rectangle.
    pub start_minute: i64,
    pub end_minute: i64,
    /// Candidate labels from longest to shortest; the painter uses the first
    /// that fits the cell. Never empty.
    pub labels: Vec<String>,
    /// The longest label, kept for painters that do not fit labels.
    pub label: String,
    /// The shortest label.
    pub compact_label: String,
    /// Stable position of the cell in its unit's own count (hours since the
    /// origin, days since a week start, and so on). Label steps keep every
    /// n-th cell by this number, so labels do not move while scrolling.
    pub index: i64,
}

fn origin_datetime(origin_serial: i64) -> Option<NaiveDateTime> {
    NaiveDate::from_ymd_opt(1899, 12, 30)?
        .checked_add_signed(Duration::try_days(origin_serial)?)?
        .and_hms_opt(0, 0, 0)
}

fn datetime_at(origin: NaiveDateTime, minute: f64) -> Option<NaiveDateTime> {
    if !minute.is_finite() || minute < i64::MIN as f64 || minute >= i64::MAX as f64 {
        return None;
    }
    origin.checked_add_signed(Duration::try_minutes(minute.floor() as i64)?)
}

/// Display options for cell boundaries and labels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LabelOptions {
    /// Weeks begin on Sunday instead of Monday.
    pub week_starts_sunday: bool,
    /// Day cells in the lowest band show weekday names or letters
    /// (Sun or S) instead of the day of the month.
    pub day_letters: bool,
    /// Week cells show the week of the year instead of the start date.
    pub week_numbers: bool,
    /// First month of the year and quarter cycle, 1 for calendar years.
    pub fiscal_start_month: u32,
    /// Ordinal labels count cells from the cell containing this minute.
    pub ordinal_origin: Option<i64>,
    /// Hour cells read 06:00 (true) or 6 AM (false).
    pub hour_24: bool,
}

impl Default for LabelOptions {
    /// The reference timescale: Sunday weeks, weekday letters.
    fn default() -> Self {
        Self {
            week_starts_sunday: true,
            day_letters: true,
            week_numbers: false,
            fiscal_start_month: 1,
            ordinal_origin: None,
            hour_24: true,
        }
    }
}

impl LabelOptions {
    fn fiscal_month0(&self) -> u32 {
        self.fiscal_start_month.clamp(1, 12) - 1
    }
}

/// First day of the fiscal-aligned cycle (12 or 3 months) containing `day`.
fn fiscal_cycle_start(day: NaiveDate, months: u32, options: &LabelOptions) -> Option<NaiveDate> {
    let index = day.year() as i64 * 12 + day.month0() as i64 - options.fiscal_month0() as i64;
    let start = index.div_euclid(months as i64) * months as i64 + options.fiscal_month0() as i64;
    NaiveDate::from_ymd_opt(
        start.div_euclid(12) as i32,
        start.rem_euclid(12) as u32 + 1,
        1,
    )
}

fn floor_unit_with(
    date: NaiveDateTime,
    unit: Unit,
    options: &LabelOptions,
) -> Option<NaiveDateTime> {
    let day = date.date();
    match unit {
        Unit::Year => fiscal_cycle_start(day, 12, options)?.and_hms_opt(0, 0, 0),
        Unit::Quarter => fiscal_cycle_start(day, 3, options)?.and_hms_opt(0, 0, 0),
        Unit::Month => NaiveDate::from_ymd_opt(day.year(), day.month(), 1)?.and_hms_opt(0, 0, 0),
        Unit::Week => {
            let back = if options.week_starts_sunday {
                day.weekday().num_days_from_sunday()
            } else {
                day.weekday().num_days_from_monday()
            };
            day.checked_sub_signed(Duration::days(back.into()))?
                .and_hms_opt(0, 0, 0)
        }
        Unit::Day => day.and_hms_opt(0, 0, 0),
        Unit::Hour => day.and_hms_opt(date.hour(), 0, 0),
    }
}

/// Week of the year: ISO weeks for Monday starts; for Sunday starts the week
/// containing January 1 is week 1.
fn week_number(date: NaiveDateTime, options: &LabelOptions) -> u32 {
    if !options.week_starts_sunday {
        return date.iso_week().week();
    }
    let day = date.date();
    let jan1 = NaiveDate::from_ymd_opt(day.year(), 1, 1).unwrap_or(day);
    (day.ordinal0() + jan1.weekday().num_days_from_sunday()) / 7 + 1
}

/// Cell count between the cell containing `origin` and `date`, 1-based.
fn ordinal_index(date: NaiveDateTime, origin: NaiveDateTime, unit: Unit) -> i64 {
    let months = |d: NaiveDateTime| d.year() as i64 * 12 + d.month0() as i64;
    let n = match unit {
        Unit::Year => (date.year() - origin.year()) as i64,
        Unit::Quarter => (months(date) - months(origin)).div_euclid(3),
        Unit::Month => months(date) - months(origin),
        Unit::Week => (date.date() - origin.date()).num_days().div_euclid(7),
        Unit::Day => (date.date() - origin.date()).num_days(),
        Unit::Hour => (date - origin).num_hours(),
    };
    if n >= 0 {
        n + 1
    } else {
        n
    }
}

fn next_unit(start: NaiveDateTime, unit: Unit) -> Option<NaiveDateTime> {
    match unit {
        Unit::Year => start.checked_add_months(Months::new(12)),
        Unit::Quarter => start.checked_add_months(Months::new(3)),
        Unit::Month => start.checked_add_months(Months::new(1)),
        Unit::Week => start.checked_add_signed(Duration::days(7)),
        Unit::Day => start.checked_add_signed(Duration::days(1)),
        Unit::Hour => start.checked_add_signed(Duration::hours(1)),
    }
}

/// Weekday names as the reference timescale prints them (Thursday is "Thr").
const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thr", "Fri", "Sat"];
const WEEKDAYS_FULL: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];

/// The cell's position in its unit's own count, see `Cell::index`.
fn step_index(
    start: NaiveDateTime,
    origin: NaiveDateTime,
    unit: Unit,
    options: &LabelOptions,
) -> i64 {
    // Day zero of the count is a week start: 0001-01-01 is a Monday.
    let day =
        i64::from(start.date().num_days_from_ce()) - if options.week_starts_sunday { 0 } else { 1 };
    match unit {
        Unit::Year => i64::from(start.year()),
        Unit::Quarter => i64::from(start.year()) * 4 + i64::from(start.month0() / 3),
        Unit::Month => i64::from(start.year()) * 12 + i64::from(start.month0()),
        Unit::Week => day.div_euclid(7),
        Unit::Day => day,
        Unit::Hour => start.signed_duration_since(origin).num_hours(),
    }
}

/// Label steps tried in order when a band's cells are too narrow for a label
/// in every cell: every n-th cell carries one.
const fn label_steps(unit: Unit) -> &'static [i64] {
    match unit {
        Unit::Hour => &[1, 2, 3, 4, 6, 12],
        Unit::Day => &[1, 2, 3, 7, 14],
        Unit::Week => &[1, 2, 3, 4, 6, 13, 26, 52],
        Unit::Month => &[1, 2, 3, 4, 6, 12],
        Unit::Quarter => &[1, 2, 4, 8, 20],
        Unit::Year => &[1, 2, 5, 10, 20, 50, 100],
    }
}

fn hour_labels(hour: u32, hour_24: bool) -> Vec<String> {
    if hour_24 {
        return vec![format!("{hour:02}:00"), format!("{hour:02}")];
    }
    let clock = (hour + 11) % 12 + 1;
    let (long, short) = if hour < 12 { ("AM", "a") } else { ("PM", "p") };
    vec![format!("{clock} {long}"), format!("{clock}{short}")]
}

/// Candidate labels for a cell starting at `date`, longest first. `upper` is
/// true when a finer band is painted below this one.
fn labels_for(date: NaiveDateTime, unit: Unit, upper: bool, options: &LabelOptions) -> Vec<String> {
    let fiscal = options.fiscal_month0() != 0;
    let year = date.year();
    let quarter = (date.month0() + 12 - options.fiscal_month0()) % 12 / 3 + 1;
    let quarter_year = if fiscal { year + 1 } else { year };
    match unit {
        Unit::Year if fiscal => vec![
            format!("FY{}", year + 1),
            format!("FY{:02}", (year + 1).rem_euclid(100)),
        ],
        Unit::Year => vec![date.format("%Y").to_string(), date.format("%y").to_string()],
        Unit::Quarter if upper => vec![
            format!("Qtr {quarter}, {quarter_year}"),
            format!("Qtr {quarter}"),
            format!("Q{quarter}"),
        ],
        Unit::Quarter => vec![format!("Qtr {quarter}"), format!("Q{quarter}")],
        Unit::Month if upper => vec![
            date.format("%B %Y").to_string(),
            date.format("%b %Y").to_string(),
            date.format("%b").to_string(),
            date.format("%b").to_string()[..1].to_string(),
        ],
        Unit::Month => vec![
            date.format("%b").to_string(),
            date.format("%b").to_string()[..1].to_string(),
        ],
        Unit::Week if options.week_numbers => vec![week_number(date, options).to_string()],
        Unit::Week if upper => vec![
            date.format("%b %d").to_string(),
            date.format("%d").to_string(),
        ],
        Unit::Week => vec![date.format("%d").to_string()],
        Unit::Day if upper => {
            let date_text = date.format("%b %-d").to_string();
            if options.day_letters {
                let day = date.weekday().num_days_from_sunday() as usize;
                let (full, name) = (WEEKDAYS_FULL[day], WEEKDAYS[day]);
                vec![
                    format!("{full} {date_text}"),
                    format!("{name} {date_text}"),
                    format!("{name} {}", date.day()),
                    date_text,
                    date.day().to_string(),
                ]
            } else {
                vec![date_text, date.day().to_string()]
            }
        }
        Unit::Day if options.day_letters => {
            let day = date.weekday().num_days_from_sunday() as usize;
            let (full, name) = (WEEKDAYS_FULL[day], WEEKDAYS[day]);
            let number = date.day();
            vec![
                format!("{full} {number}"),
                format!("{name} {number} {}", date.format("%b")),
                format!("{name} {number}"),
                format!("{} {number}", &name[..2]),
                name.to_string(),
                name[..2].to_string(),
                name[..1].to_string(),
            ]
        }
        Unit::Day => vec![date.format("%d").to_string()],
        Unit::Hour => hour_labels(date.hour(), options.hour_24),
    }
}

/// One-based index of the `unit` cell containing `minute`, counted from the
/// cell containing `anchor_minute` (negative before it, never zero).
pub fn unit_ordinal(
    unit: Unit,
    origin_serial: i64,
    anchor_minute: i64,
    minute: i64,
    options: &LabelOptions,
) -> Option<i64> {
    let origin = origin_datetime(origin_serial)?;
    let anchor = floor_unit_with(datetime_at(origin, anchor_minute as f64)?, unit, options)?;
    let date = floor_unit_with(datetime_at(origin, minute as f64)?, unit, options)?;
    Some(ordinal_index(date, anchor, unit))
}

/// Sweep actual unit boundaries that intersect `[first_minute, last_minute)`
/// with the default label options. A band without a `parent` is labelled as
/// an upper band; a band with one is labelled as the lowest band. Invalid
/// ranges return no cells. Extremely large manual sweeps stop at MAX_CELLS.
pub fn cells(
    unit: Unit,
    parent: Option<Unit>,
    origin_serial: i64,
    first_minute: f64,
    last_minute: f64,
) -> Vec<Cell> {
    band_cells(
        unit,
        parent.is_none(),
        origin_serial,
        first_minute,
        last_minute,
        &LabelOptions::default(),
    )
}

/// The general sweep. `upper` selects the upper-band formats; week start,
/// fiscal years, weekday names, week numbers and ordinals come from `options`.
pub fn band_cells(
    unit: Unit,
    upper: bool,
    origin_serial: i64,
    first_minute: f64,
    last_minute: f64,
    options: &LabelOptions,
) -> Vec<Cell> {
    let mut result = Vec::new();
    if !first_minute.is_finite() || !last_minute.is_finite() || first_minute >= last_minute {
        return result;
    }
    let Some(origin) = origin_datetime(origin_serial) else {
        return result;
    };
    let Some(first) = datetime_at(origin, first_minute) else {
        return result;
    };
    let Some(mut start) = floor_unit_with(first, unit, options) else {
        return result;
    };
    let ordinal = options
        .ordinal_origin
        .and_then(|minute| datetime_at(origin, minute as f64))
        .and_then(|date| floor_unit_with(date, unit, options));
    for _ in 0..MAX_CELLS {
        let start_minute = start.signed_duration_since(origin).num_minutes();
        if start_minute as f64 >= last_minute {
            break;
        }
        let Some(end) = next_unit(start, unit) else {
            break;
        };
        let end_minute = end.signed_duration_since(origin).num_minutes();
        let labels = match ordinal {
            Some(o) => vec![ordinal_index(start, o, unit).to_string()],
            None => labels_for(start, unit, upper, options),
        };
        result.push(Cell {
            start_minute,
            end_minute,
            label: labels[0].clone(),
            compact_label: labels[labels.len() - 1].clone(),
            labels,
            index: step_index(start, origin, unit, options),
        });
        if end <= start {
            break;
        }
        start = end;
    }
    result
}

/// A label chosen for one cell: its text and where its left edge sits.
#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    pub text: String,
    pub left: f32,
    pub width: f32,
}

/// The labels `place_labels` chose, with the step that was needed.
#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    /// One entry per cell: its label, or none when it has no room or no turn.
    pub labels: Vec<Option<Placed>>,
    /// Every `step`-th cell carries a label.
    pub step: i64,
}

impl Placement {
    /// False for the cells a stepped band skips. Painters draw their tick
    /// lines short, so a line never runs through a neighbour's label.
    pub fn is_major(&self, cell: &Cell) -> bool {
        cell.index.rem_euclid(self.step) == 0
    }
}

/// Height of the tick line at a skipped cell of a stepped band.
pub const MINOR_TICK: f32 = 4.0;

/// Room kept around a label: the cell's tick line plus a pixel each side.
/// Hour numbers run together without more.
const fn label_padding(unit: Unit) -> f32 {
    match unit {
        Unit::Hour => 5.0,
        _ => 3.0,
    }
}

/// Chooses a label for each cell of one band so that every label drawn fits
/// whole. `spans` holds each cell's left and right pixel edge, `visible` the
/// pixel range the band is shown in, and `measure` gives a label's width.
///
/// Tries every step of the unit's ladder (every cell, then every second,
/// third ... cell), and at each step every label length from longest to
/// shortest, and keeps the first combination in which all labels fit. One
/// length is used for the whole band, so it never mixes "Sun" with "S". Labels
/// of every cell are centred in it; labels of a stepped band sit just right of
/// their tick line and must lie inside `visible`, so nothing is drawn cut off.
/// When even the sparsest step cannot fit, only the cells that do get one.
pub fn place_labels(
    unit: Unit,
    cells: &[Cell],
    spans: &[(f32, f32)],
    visible: (f32, f32),
    measure: &mut dyn FnMut(&str) -> f32,
) -> Placement {
    let mut result = vec![None; cells.len()];
    if cells.is_empty() || cells.len() != spans.len() {
        return Placement {
            labels: result,
            step: 1,
        };
    }
    let pad = label_padding(unit);
    let levels = cells.iter().map(|c| c.labels.len()).max().unwrap_or(1);
    let mut widths: std::collections::HashMap<String, f32> = std::collections::HashMap::new();
    let mut width_of = |text: &str| {
        *widths
            .entry(text.to_owned())
            .or_insert_with(|| measure(text))
    };
    let pick = |cell: &Cell, level: usize| cell.labels[level.min(cell.labels.len() - 1)].clone();
    let steps = label_steps(unit);
    // Cells that carry a label at `step`, with the room each has.
    let place = |step: i64, level: usize, width_of: &mut dyn FnMut(&str) -> f32| {
        let kept: Vec<usize> = (0..cells.len())
            .filter(|&i| cells[i].index.rem_euclid(step) == 0)
            .collect();
        let mut out: Vec<(usize, Placed, bool, bool)> = vec![];
        for (n, &i) in kept.iter().enumerate() {
            let (x, end) = spans[i];
            let text = pick(&cells[i], level);
            let width = width_of(&text);
            let (left, room, whole) = if step == 1 {
                (((x + 1. + end - width) / 2.).round(), end - x - pad, false)
            } else {
                let next = kept
                    .get(n + 1)
                    .map_or(x + (end - x) * step as f32, |&j| spans[j].0);
                let left = (x + 3.).round();
                (left, next - x - pad - 3., true)
            };
            let inside = left >= visible.0 && left + width <= visible.1;
            let fits = width <= room;
            let shown = fits && (!whole && unit != Unit::Hour || inside);
            out.push((i, Placed { text, left, width }, fits, shown));
        }
        out
    };
    let mut best = None;
    'search: for &step in steps {
        for level in 0..levels {
            let placed = place(step, level, &mut width_of);
            if placed.iter().all(|p| p.2) {
                best = Some((step, placed));
                break 'search;
            }
        }
    }
    let (step, placed) = best.unwrap_or_else(|| {
        let step = steps[steps.len() - 1];
        (step, place(step, levels - 1, &mut width_of))
    });
    for (i, label, _, shown) in placed {
        if shown {
            result[i] = Some(label);
        }
    }
    Placement {
        labels: result,
        step,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn serial(year: i32, month: u32, day: u32) -> i64 {
        NaiveDate::from_ymd_opt(year, month, day)
            .unwrap()
            .signed_duration_since(NaiveDate::from_ymd_opt(1899, 12, 30).unwrap())
            .num_days()
    }

    #[test]
    fn source_mapping_truncates_displacement_toward_zero_before_left_edge() {
        let scale = TimeScale::new(100.0, 70.0, 20.5);
        assert_eq!(scale.x(100.0 + 1440.0), 30.5);
        assert_eq!(scale.x(100.0 - 1440.0), 10.5);
        assert_eq!(scale.x(99.0), 20.5);
        assert_eq!(scale.x(100.0 - 216.0), 19.5); // -1.5 pixels -> -1, not -2
        assert_eq!(scale.x(100.0 + 216.0), 21.5);
        assert_eq!(scale.x_date(30.5), 1540.0);
    }

    #[test]
    fn inverse_error_is_bounded_by_one_pixel_on_both_sides() {
        for ppw in [1.0, 17.5, 112.0, 13440.0] {
            let scale = TimeScale::new(-1234.75, ppw, 256.0);
            for minute in [-10000.0, -1234.75, -12.5, 0.0, 123456.125] {
                let error = (scale.x_date(scale.x(minute)) - minute).abs();
                assert!(
                    error < MINUTES_PER_WEEK / ppw + 1e-8,
                    "{ppw} {minute} {error}"
                );
            }
        }
    }

    #[test]
    fn scale_clamps_and_zoom_preserves_anchor_including_negative_dates() {
        assert_eq!(TimeScale::new(0.0, -10.0, 0.0).pixels_per_week, 1.0);
        assert_eq!(TimeScale::new(0.0, f64::NAN, 0.0).pixels_per_week, 1.0);
        let mut scale = TimeScale::new(-2500.5, 112.0, 300.0);
        let before = scale.x_date(428.0);
        scale.zoom_at(13440.0, 428.0);
        assert!((scale.x_date(428.0) - before).abs() < 1e-9);
        scale.zoom_at(0.0, 428.0);
        assert_eq!(scale.pixels_per_week, 1.0);
        assert!((scale.x_date(428.0) - before).abs() < 1e-8);
        let first = scale.first_minute;
        scale.pan_pixels(10.0);
        assert_eq!(scale.first_minute, first - 10.0 * MINUTES_PER_WEEK);
    }

    fn labels(cells: &[Cell]) -> Vec<&str> {
        cells.iter().map(|c| c.label.as_str()).collect()
    }

    #[test]
    fn zoom_ladder_steps_like_the_reference_and_picks_its_bands() {
        assert!(ZOOM_LADDER.windows(2).all(|p| p[0] < p[1]));
        for (preset, rung) in ZOOM_PRESETS.iter().zip(ZOOM_LADDER) {
            assert_eq!(preset.pixels_per_week, rung);
            assert_eq!(bands_for_scale(rung), (preset.major, preset.minor));
        }
        // Reference: 101.5 out to 100, 28, 13; then in to 28, 100, 200.
        assert_eq!(zoom_step(101.5, false), 100.0);
        assert_eq!(zoom_step(100.0, false), 28.0);
        assert_eq!(zoom_step(28.0, false), 13.0);
        assert_eq!(zoom_step(13.0, true), 28.0);
        assert_eq!(zoom_step(28.0, true), 100.0);
        assert_eq!(zoom_step(100.0, true), 200.0);
        assert_eq!(
            zoom_step(1.5, false),
            1.0,
            "past the ladder halves, clamped"
        );
        assert_eq!(zoom_step(8064.0, true), 16128.0);
        assert_eq!(bands_for_scale(101.5), (Unit::Week, Unit::Day));
        assert_eq!(bands_for_scale(28.0), (Unit::Month, Unit::Week));
        assert_eq!(bands_for_scale(13.0), (Unit::Quarter, Unit::Month));
        assert_eq!(bands_for_scale(1.0), (Unit::Year, Unit::Quarter));
        assert_eq!(bands_for_scale(13440.0), (Unit::Day, Unit::Hour));
    }

    #[test]
    fn months_use_real_lengths_and_band_role_formats() {
        let options = LabelOptions::default();
        let upper = band_cells(
            Unit::Month,
            true,
            serial(2024, 1, 1),
            15.0 * 1440.0,
            75.0 * 1440.0,
            &options,
        );
        assert_eq!(
            upper
                .iter()
                .map(|c| (c.end_minute - c.start_minute) / 1440)
                .collect::<Vec<_>>(),
            [31, 29, 31]
        );
        assert_eq!(
            labels(&upper),
            ["January 2024", "February 2024", "March 2024"]
        );
        assert_eq!(upper[0].labels, ["January 2024", "Jan 2024", "Jan", "J"]);
        let lower = band_cells(
            Unit::Month,
            false,
            serial(2025, 12, 1),
            0.0,
            65.0 * 1440.0,
            &options,
        );
        assert_eq!(labels(&lower), ["Dec", "Jan", "Feb"]);
        assert_eq!(lower[1].compact_label, "J");
    }

    #[test]
    fn quarters_and_years_use_actual_gregorian_boundaries() {
        let quarters = band_cells(
            Unit::Quarter,
            true,
            serial(2026, 1, 1),
            0.0,
            366.0 * 1440.0,
            &LabelOptions::default(),
        );
        assert_eq!(
            quarters
                .iter()
                .map(|c| (c.end_minute - c.start_minute) / 1440)
                .collect::<Vec<_>>(),
            [90, 91, 92, 92, 90]
        );
        assert_eq!(
            labels(&quarters),
            [
                "Qtr 1, 2026",
                "Qtr 2, 2026",
                "Qtr 3, 2026",
                "Qtr 4, 2026",
                "Qtr 1, 2027"
            ]
        );
        let lower = cells(
            Unit::Quarter,
            Some(Unit::Year),
            serial(2026, 1, 1),
            0.0,
            1.0,
        );
        assert_eq!(lower[0].labels, ["Qtr 1", "Q1"]);
        let years = cells(Unit::Year, None, serial(2024, 1, 1), 0.0, 367.0 * 1440.0);
        assert_eq!(
            years
                .iter()
                .map(|c| (c.end_minute - c.start_minute) / 1440)
                .collect::<Vec<_>>(),
            [366, 365]
        );
        assert_eq!(years[1].label, "2025");
    }

    #[test]
    fn reference_week_day_and_month_week_labels() {
        // 2026-09-20 is a Sunday: the reference's first full week.
        let origin = serial(2026, 9, 18);
        let options = LabelOptions::default();
        let weeks = band_cells(Unit::Week, true, origin, 0.0, 16.0 * 1440.0, &options);
        assert_eq!(labels(&weeks), ["Sep 13", "Sep 20", "Sep 27"]);
        assert_eq!(weeks[1].start_minute, 2 * 1440, "Sunday weeks");
        let days = band_cells(
            Unit::Day,
            false,
            origin,
            2.0 * 1440.0,
            9.0 * 1440.0,
            &options,
        );
        assert_eq!(
            days.iter().map(|c| c.labels.clone()).collect::<Vec<_>>(),
            [
                vec![
                    "Sunday 20",
                    "Sun 20 Sep",
                    "Sun 20",
                    "Su 20",
                    "Sun",
                    "Su",
                    "S"
                ],
                vec![
                    "Monday 21",
                    "Mon 21 Sep",
                    "Mon 21",
                    "Mo 21",
                    "Mon",
                    "Mo",
                    "M"
                ],
                vec![
                    "Tuesday 22",
                    "Tue 22 Sep",
                    "Tue 22",
                    "Tu 22",
                    "Tue",
                    "Tu",
                    "T"
                ],
                vec![
                    "Wednesday 23",
                    "Wed 23 Sep",
                    "Wed 23",
                    "We 23",
                    "Wed",
                    "We",
                    "W"
                ],
                vec![
                    "Thursday 24",
                    "Thr 24 Sep",
                    "Thr 24",
                    "Th 24",
                    "Thr",
                    "Th",
                    "T"
                ],
                vec![
                    "Friday 25",
                    "Fri 25 Sep",
                    "Fri 25",
                    "Fr 25",
                    "Fri",
                    "Fr",
                    "F"
                ],
                vec![
                    "Saturday 26",
                    "Sat 26 Sep",
                    "Sat 26",
                    "Sa 26",
                    "Sat",
                    "Sa",
                    "S"
                ]
            ]
        );
        // Month/Week: weeks print their start day and are never split at a
        // month boundary (the week of Jun 28 runs into July).
        let june = serial(2026, 5, 31);
        let weeks = band_cells(Unit::Week, false, june, 0.0, 35.0 * 1440.0, &options);
        assert_eq!(labels(&weeks), ["31", "07", "14", "21", "28"]);
        assert!(weeks.iter().all(|c| c.end_minute - c.start_minute == 10080));
        let months = band_cells(Unit::Month, true, june, 0.0, 35.0 * 1440.0, &options);
        assert_eq!(labels(&months), ["May 2026", "June 2026", "July 2026"]);
        assert_eq!(months[1].start_minute, 1440, "months start on the 1st");
    }

    #[test]
    fn label_options_cover_monday_weeks_numbers_fiscal_years_and_ordinals() {
        let monday = LabelOptions {
            week_starts_sunday: false,
            ..Default::default()
        };
        let weeks = band_cells(
            Unit::Week,
            true,
            serial(2021, 1, 1),
            -1440.0,
            11.0 * 1440.0,
            &monday,
        );
        assert_eq!(
            weeks
                .iter()
                .map(|c| c.start_minute / 1440)
                .collect::<Vec<_>>(),
            [-4, 3, 10]
        );
        assert_eq!(labels(&weeks), ["Dec 28", "Jan 04", "Jan 11"]);
        let numbers = LabelOptions {
            day_letters: false,
            ..Default::default()
        };
        let days = band_cells(Unit::Day, false, serial(2026, 9, 20), 0.0, 1.0, &numbers);
        assert_eq!(days[0].labels, ["20"]);

        let numbered = LabelOptions {
            week_numbers: true,
            ..Default::default()
        };
        let jan = serial(2026, 1, 1); // Thursday: week 1 runs Dec 28 to Jan 3.
        let weeks = band_cells(Unit::Week, false, jan, 0.0, 4.0 * 1440.0, &numbered);
        assert_eq!(weeks[1].label, "2");

        let fiscal = LabelOptions {
            fiscal_start_month: 10,
            ..Default::default()
        };
        let october = serial(2026, 10, 1);
        let years = band_cells(Unit::Year, true, october, 0.0, 400.0 * 1440.0, &fiscal);
        assert_eq!(years[0].start_minute, 0, "Fiscal years start in October");
        assert_eq!(years[0].label, "FY2027");
        let quarters = band_cells(Unit::Quarter, false, october, 0.0, 100.0 * 1440.0, &fiscal);
        assert_eq!(quarters[0].label, "Qtr 1");
        assert_eq!(quarters[1].label, "Qtr 2");

        let ordinal = LabelOptions {
            ordinal_origin: Some(31 * 1440),
            ..Default::default()
        };
        let months = band_cells(Unit::Month, true, october, 0.0, 100.0 * 1440.0, &ordinal);
        assert_eq!(labels(&months), ["-1", "1", "2", "3"]);
    }

    #[test]
    fn negative_minutes_and_hour_cells_cross_midnight() {
        let origin = serial(2026, 11, 4);
        let hours = cells(Unit::Hour, Some(Unit::Day), origin, -30.0, 120.0);
        assert_eq!(
            hours
                .iter()
                .map(|c| (c.start_minute, c.end_minute))
                .collect::<Vec<_>>(),
            [(-60, 0), (0, 60), (60, 120)]
        );
        assert_eq!(labels(&hours), ["23:00", "00:00", "01:00"]);
        assert_eq!(hours[0].labels, ["23:00", "23"]);
        let twelve = LabelOptions {
            hour_24: false,
            ..Default::default()
        };
        let hours = band_cells(Unit::Hour, false, origin, 11.0 * 60.0, 14.0 * 60.0, &twelve);
        assert_eq!(labels(&hours), ["11 AM", "12 PM", "1 PM"]);
        assert_eq!(hours[1].labels, ["12 PM", "12p"]);
        let midnight = band_cells(Unit::Hour, false, origin, -30.0, 30.0, &twelve);
        assert_eq!(midnight[0].labels, ["11 PM", "11p"]);
        assert_eq!(midnight[1].labels, ["12 AM", "12a"]);
        // The band above hours names the day with its weekday.
        let days = cells(Unit::Day, None, serial(2026, 10, 31), 0.0, 3.0 * 1440.0);
        assert_eq!(
            labels(&days),
            ["Saturday Oct 31", "Sunday Nov 1", "Monday Nov 2"]
        );
        assert_eq!(
            days[0].labels,
            ["Saturday Oct 31", "Sat Oct 31", "Sat 31", "Oct 31", "31"]
        );
    }

    #[test]
    fn empty_invalid_and_large_sweeps_are_bounded() {
        assert!(cells(Unit::Day, None, 0, 1.0, 1.0).is_empty());
        assert!(cells(Unit::Day, None, 0, f64::NAN, 10.0).is_empty());
        assert!(cells(Unit::Day, None, i64::MAX, 0.0, 10.0).is_empty());
        assert!(cells(Unit::Day, None, 0, f64::INFINITY, f64::INFINITY).is_empty());
        assert_eq!(cells(Unit::Hour, None, 0, 0.0, 1e9).len(), MAX_CELLS);
    }

    #[test]
    fn automatic_zoom_bands_keep_calendar_sweeps_small() {
        for scale in [
            1.0, 2.0, 8.0, 8.4, 18.0, 29.9, 30.0, 80.0, 125.9, 126.0, 560.0, 4032.0, 13440.0,
        ] {
            let (upper, lower) = bands_for_scale(scale);
            let axis = TimeScale::new(0.0, scale, 0.0);
            let end = axis.x_date(1920.0);
            let upper_cells = cells(upper, None, serial(2024, 1, 1), 0.0, end);
            let lower_cells = cells(lower, Some(upper), serial(2024, 1, 1), 0.0, end);
            assert!(!upper_cells.is_empty() && !lower_cells.is_empty());
            assert!(
                upper_cells.len() < 240 && lower_cells.len() < 240,
                "scale {scale}: {} upper, {} lower",
                upper_cells.len(),
                lower_cells.len()
            );
        }
    }

    /// A stand-in font: every character is six pixels wide.
    fn six_per_character(text: &str) -> f32 {
        6.0 * text.chars().count() as f32
    }

    fn placed_band(
        unit: Unit,
        upper: bool,
        options: &LabelOptions,
        pixels_per_week: f64,
        width: f32,
    ) -> (Vec<(f32, f32)>, Vec<Option<Placed>>) {
        let origin = serial(2026, 9, 14);
        let axis = TimeScale::new(0.37 * MINUTES_PER_DAY, pixels_per_week, 0.0);
        let list = band_cells(
            unit,
            upper,
            origin,
            axis.first_minute,
            axis.x_date(width),
            options,
        );
        let spans: Vec<(f32, f32)> = list
            .iter()
            .map(|c| (axis.x(c.start_minute as f64), axis.x(c.end_minute as f64)))
            .collect();
        let placed = place_labels(unit, &list, &spans, (1.0, width), &mut |t| {
            six_per_character(t)
        })
        .labels;
        (spans, placed)
    }

    #[test]
    fn labels_never_overflow_their_cells_or_overlap_at_any_zoom_and_width() {
        let twelve = LabelOptions {
            hour_24: false,
            ..Default::default()
        };
        let monday = LabelOptions {
            week_starts_sunday: false,
            ..Default::default()
        };
        let numbered = LabelOptions {
            day_letters: false,
            week_numbers: true,
            ..Default::default()
        };
        let mut shown = 0;
        for options in [LabelOptions::default(), twelve, monday, numbered] {
            for scale in [
                3.0, 5.0, 13.0, 28.0, 60.0, 100.0, 200.0, 460.0, 900.0, 1344.0, 2016.0, 4032.0,
                13440.0,
            ] {
                for width in [320.0, 700.0, 1900.0] {
                    for unit in Unit::all() {
                        for upper in [true, false] {
                            let (spans, placed) = placed_band(unit, upper, &options, scale, width);
                            let mut right_edge = f32::MIN;
                            for (i, label) in placed.iter().enumerate() {
                                let Some(label) = label else { continue };
                                let (x, end) = spans[i];
                                shown += 1;
                                assert_eq!(label.width, six_per_character(&label.text));
                                // Labels of neighbouring cells mean every cell has
                                // one: each must fit the cell it belongs to.
                                if placed.get(i + 1).is_some_and(Option::is_some) {
                                    assert!(
                                        label.left >= x && label.left + label.width <= end,
                                        "{unit:?} at {scale}: {} outside {x}..{end}",
                                        label.text
                                    );
                                }
                                assert!(
                                    label.left >= right_edge,
                                    "{unit:?} at {scale}: {} overlaps the label before it",
                                    label.text
                                );
                                right_edge = label.left + label.width;
                            }
                        }
                    }
                }
            }
        }
        assert!(shown > 1000, "labels were placed: {shown}");
    }

    #[test]
    fn hour_labels_step_through_the_ladder_as_the_cells_narrow() {
        let options = LabelOptions::default();
        let text = |scale: f64| {
            let (_, placed) = placed_band(Unit::Hour, false, &options, scale, 1200.0);
            let mut steps: Vec<i64> = vec![];
            let origin = serial(2026, 9, 14);
            let axis = TimeScale::new(0.37 * MINUTES_PER_DAY, scale, 0.0);
            let list = band_cells(
                Unit::Hour,
                false,
                origin,
                axis.first_minute,
                axis.x_date(1200.0),
                &options,
            );
            for (cell, label) in list.iter().zip(&placed) {
                if label.is_some() {
                    steps.push(cell.index.rem_euclid(24));
                }
            }
            (steps, placed.iter().flatten().next().cloned())
        };
        // 40 px an hour: "06:00" fits in every cell (30 px with the stand-in font).
        let (hours, first) = text(40.0 * 168.0);
        assert!(hours.len() > 20);
        assert!(first.unwrap().text.contains(':'));
        // 12 px an hour: bare hour numbers, every second hour.
        let (hours, first) = text(12.0 * 168.0);
        assert!(hours.iter().all(|h| h % 2 == 0), "{hours:?}");
        assert!(!first.unwrap().text.contains(':'));
        // 7 px an hour, the reported case: every third hour.
        let (hours, _) = text(7.0 * 168.0);
        assert!(hours.iter().all(|h| h % 3 == 0), "{hours:?}");
        assert!(hours.contains(&0) && hours.contains(&21));
        // 2.5 px an hour: every twelfth hour at most.
        let (hours, _) = text(2.5 * 168.0);
        assert!(hours.iter().all(|h| h % 6 == 0), "{hours:?}");
    }

    #[test]
    fn stepped_labels_keep_their_cells_while_scrolling() {
        let options = LabelOptions::default();
        let origin = serial(2026, 9, 14);
        let scale = 7.0 * 168.0;
        let mut seen: Vec<Vec<i64>> = vec![];
        for shift in [0.0, 20.0, 137.0, 3000.0] {
            let axis = TimeScale::new(shift, scale, 0.0);
            let list = band_cells(
                Unit::Hour,
                false,
                origin,
                axis.first_minute,
                axis.x_date(900.0),
                &options,
            );
            let spans: Vec<(f32, f32)> = list
                .iter()
                .map(|c| (axis.x(c.start_minute as f64), axis.x(c.end_minute as f64)))
                .collect();
            let placed = place_labels(Unit::Hour, &list, &spans, (1.0, 900.0), &mut |t| {
                six_per_character(t)
            })
            .labels;
            seen.push(
                list.iter()
                    .zip(&placed)
                    .filter(|(_, p)| p.is_some())
                    .map(|(c, _)| c.index.rem_euclid(24))
                    .collect(),
            );
        }
        for hours in &seen {
            assert!(hours.iter().all(|h| h % 3 == 0), "{hours:?}");
        }
    }

    #[test]
    fn a_stepped_label_is_not_drawn_cut_by_the_edge_of_the_band() {
        let options = LabelOptions::default();
        let (spans, placed) = placed_band(Unit::Hour, false, &options, 7.0 * 168.0, 300.0);
        for (&(x, _), label) in spans.iter().zip(&placed) {
            if let Some(label) = label {
                assert!(
                    label.left >= 1.0 && label.left + label.width <= 300.0,
                    "{x}"
                );
            }
        }
        // The first cell starts before the band, so its label is left out.
        assert!(spans[0].0 < 1.0 || placed[0].is_some());
    }

    #[test]
    fn weekday_names_shorten_with_the_cell_width() {
        let options = LabelOptions::default();
        let names = |scale: f64| {
            let (_, placed) = placed_band(Unit::Day, false, &options, scale, 900.0);
            let texts: Vec<String> = placed.iter().flatten().map(|p| p.text.clone()).collect();
            texts
        };
        // 120 px a day: the full weekday and the day of the month.
        let wide = names(120.0 * 7.0);
        assert!(wide.iter().any(|t| t.starts_with("Wednesday ")), "{wide:?}");
        // 40 px a day: three letters and the date.
        let medium = names(40.0 * 7.0);
        assert!(medium.iter().any(|t| t == "Wed 16"), "{medium:?}");
        // 20 px a day (reference zoom is 14): the three-letter name.
        let narrow = names(26.0 * 7.0);
        assert!(narrow.iter().any(|t| t == "Wed"), "{narrow:?}");
        // 14.3 px a day: the reference letters.
        let letters = names(100.0);
        assert!(
            letters.iter().all(|t| t.chars().count() == 1),
            "{letters:?}"
        );
        // Too narrow for any letter in every cell: weekdays are skipped.
        let sparse = names(5.0 * 7.0);
        assert!(sparse.len() < 900 / 5 / 2, "{}", sparse.len());
    }
}
