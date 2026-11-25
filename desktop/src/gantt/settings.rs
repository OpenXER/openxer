//! Serializable Gantt chart settings: the bar list, timescale format, chart
//! options and curtains. One `GanttSettings` value describes everything the
//! Bars, Timescale and Bar Chart Options dialogs edit, so a named layout can
//! store it as a single JSON object.
//!
//! The default bar list keeps the standard set of bars (their order, shapes,
//! filters and labels); its colors are OpenXER's own (`crate::theme`).
use serde::{Deserialize, Serialize};
use std::fmt;

pub const SETTINGS_VERSION: u32 = 1;
pub const MAX_BARS: usize = 64;
pub const MAX_LABELS: usize = 8;
pub const MAX_CURTAINS: usize = 64;
/// Text attachments a layout keeps, and the longest text of one.
pub const MAX_TEXTS: usize = 256;
pub const MAX_TEXT_CHARS: usize = 2000;
pub const MAX_BAND_KEY: usize = 500;
pub const MAX_BAR_ROW: u8 = 3;

/// An opaque RGB color stored as `#RRGGBB`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub const BLACK: Rgb = Rgb(0, 0, 0);
    pub const WHITE: Rgb = Rgb(255, 255, 255);
    pub const BLUE: Rgb = Rgb(0, 0, 255);
    pub const RED: Rgb = Rgb(255, 0, 0);
    pub const YELLOW: Rgb = Rgb(255, 255, 0);
    pub const SIGHT: Rgb = Rgb::hex(crate::theme::SIGHT_LINE);
    /// A color from `0xRRGGBB`.
    pub const fn hex(value: u32) -> Rgb {
        Rgb((value >> 16) as u8, (value >> 8) as u8, value as u8)
    }
    pub fn color(self) -> eframe::egui::Color32 {
        eframe::egui::Color32::from_rgb(self.0, self.1, self.2)
    }
    pub fn from_color(color: eframe::egui::Color32) -> Self {
        Rgb(color.r(), color.g(), color.b())
    }
}
impl fmt::Display for Rgb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02X}{:02X}{:02X}", self.0, self.1, self.2)
    }
}
impl From<Rgb> for String {
    fn from(value: Rgb) -> Self {
        value.to_string()
    }
}
impl TryFrom<String> for Rgb {
    type Error = String;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        let hex = value
            .strip_prefix('#')
            .filter(|h| h.len() == 6 && h.chars().all(|c| c.is_ascii_hexdigit()))
            .ok_or_else(|| format!("Invalid color {value:?}; expected #RRGGBB"))?;
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|e| e.to_string());
        Ok(Rgb(byte(0)?, byte(2)?, byte(4)?))
    }
}

/// Which pair of dates a bar spans (the Bars dialog "Timescale" column).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[allow(clippy::enum_variant_names)] // Names mirror the Timescale column choices.
pub enum BarSource {
    CurrentBar,
    PercentCompleteBar,
    PlanBar,
    ActualBar,
    RemainBar,
    EarlyBar,
    LateBar,
    FloatBar,
    NegFloatBar,
    ProjectBaselineBar,
    PrimaryBaselineBar,
    SecondaryBaselineBar,
    TertiaryBaselineBar,
}
impl BarSource {
    pub const ALL: [BarSource; 13] = [
        Self::CurrentBar,
        Self::PercentCompleteBar,
        Self::PlanBar,
        Self::ActualBar,
        Self::RemainBar,
        Self::EarlyBar,
        Self::LateBar,
        Self::FloatBar,
        Self::NegFloatBar,
        Self::ProjectBaselineBar,
        Self::PrimaryBaselineBar,
        Self::SecondaryBaselineBar,
        Self::TertiaryBaselineBar,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::CurrentBar => "Current Bar",
            Self::PercentCompleteBar => "% Complete Bar",
            Self::PlanBar => "Plan Bar",
            Self::ActualBar => "Actual Bar",
            Self::RemainBar => "Remain Bar",
            Self::EarlyBar => "Early Bar",
            Self::LateBar => "Late Bar",
            Self::FloatBar => "Float Bar",
            Self::NegFloatBar => "Neg Float Bar",
            Self::ProjectBaselineBar => "Project Baseline Bar",
            Self::PrimaryBaselineBar => "Primary Baseline Bar",
            Self::SecondaryBaselineBar => "Secondary Baseline Bar",
            Self::TertiaryBaselineBar => "Tertiary Baseline Bar",
        }
    }
    /// Bars drawn from an assigned baseline's dates.
    pub fn is_baseline(self) -> bool {
        matches!(
            self,
            Self::ProjectBaselineBar
                | Self::PrimaryBaselineBar
                | Self::SecondaryBaselineBar
                | Self::TertiaryBaselineBar
        )
    }
    /// Field names used by the necking gates in `gantt_geometry`.
    pub fn source_fields(self) -> (&'static str, &'static str) {
        match self {
            Self::CurrentBar => ("start_date", "end_date"),
            Self::RemainBar => ("restart_date", "reend_date"),
            Self::ActualBar => ("act_start_date", "act_progress_date"),
            Self::EarlyBar => ("early_start_date", "early_end_date"),
            Self::LateBar => ("late_start_date", "late_end_date"),
            Self::PlanBar => ("target_start_date", "target_end_date"),
            Self::PercentCompleteBar => ("start_date", "end_date"),
            Self::FloatBar | Self::NegFloatBar => ("end_date", "late_end_date"),
            _ => ("base_start_date", "base_end_date"),
        }
    }
}

/// One condition of a bar filter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FilterKind {
    AllActivities,
    Normal,
    LevelOfEffort,
    Milestone,
    Summary,
    Critical,
    NonCritical,
    NegativeFloat,
    HasStartConstraint,
    HasFinishConstraint,
    NotStarted,
    InProgress,
    Completed,
}
impl FilterKind {
    pub const ALL: [FilterKind; 13] = [
        Self::AllActivities,
        Self::Normal,
        Self::LevelOfEffort,
        Self::Milestone,
        Self::Summary,
        Self::Critical,
        Self::NonCritical,
        Self::NegativeFloat,
        Self::HasStartConstraint,
        Self::HasFinishConstraint,
        Self::NotStarted,
        Self::InProgress,
        Self::Completed,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::AllActivities => "All Activities",
            Self::Normal => "Normal",
            Self::LevelOfEffort => "Level of Effort",
            Self::Milestone => "Milestone",
            Self::Summary => "Summary",
            Self::Critical => "Critical",
            Self::NonCritical => "Non-critical",
            Self::NegativeFloat => "Negative Float",
            Self::HasStartConstraint => "Has Start Constraint",
            Self::HasFinishConstraint => "Has Finish Constraint",
            Self::NotStarted => "Not Started",
            Self::InProgress => "In Progress",
            Self::Completed => "Completed",
        }
    }
}

/// A bar applies to activities matching all (or any) of its filter kinds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BarFilter {
    pub kinds: Vec<FilterKind>,
    pub match_all: bool,
}
impl BarFilter {
    pub fn all(kinds: &[FilterKind]) -> Self {
        Self {
            kinds: kinds.to_vec(),
            match_all: true,
        }
    }
    pub fn any(kinds: &[FilterKind]) -> Self {
        Self {
            kinds: kinds.to_vec(),
            match_all: false,
        }
    }
    pub fn label(&self) -> String {
        let joiner = if self.match_all { " and " } else { " or " };
        self.kinds
            .iter()
            .map(|k| k.label())
            .collect::<Vec<_>>()
            .join(joiner)
    }
}

/// Middle bar shape: where the body sits inside the 9-pixel bar box.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BodyShape {
    None,
    Full,
    UpperHalf,
    LowerHalf,
    UpperThin,
    CenterThin,
    LowerThin,
    /// Five pixels centred in the bar box (the reference LOE bars).
    CenterHalf,
}
impl BodyShape {
    pub const ALL: [BodyShape; 8] = [
        Self::None,
        Self::Full,
        Self::UpperHalf,
        Self::CenterHalf,
        Self::LowerHalf,
        Self::UpperThin,
        Self::CenterThin,
        Self::LowerThin,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Full => "Full",
            Self::UpperHalf => "Upper half",
            Self::LowerHalf => "Lower half",
            Self::CenterHalf => "Center half",
            Self::UpperThin => "Upper thin",
            Self::CenterThin => "Center thin",
            Self::LowerThin => "Lower thin",
        }
    }
}

/// Start and end point shapes, drawn centered on the bar's date.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EndShape {
    None,
    Diamond,
    TriangleDown,
    TriangleUp,
    PointLeft,
    PointRight,
    Line,
    Flag,
    SummaryCap,
}
impl EndShape {
    pub const ALL: [EndShape; 9] = [
        Self::None,
        Self::Diamond,
        Self::TriangleDown,
        Self::TriangleUp,
        Self::PointLeft,
        Self::PointRight,
        Self::Line,
        Self::Flag,
        Self::SummaryCap,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Diamond => "Diamond",
            Self::TriangleDown => "Triangle down",
            Self::TriangleUp => "Triangle up",
            Self::PointLeft => "Point left",
            Self::PointRight => "Point right",
            Self::Line => "Line",
            Self::Flag => "Flag",
            Self::SummaryCap => "Summary cap",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Pattern {
    Solid,
    Hollow,
    Horizontal,
    Vertical,
    ForwardDiagonal,
    BackwardDiagonal,
    Cross,
    DiagonalCross,
}
impl Pattern {
    pub const ALL: [Pattern; 8] = [
        Self::Solid,
        Self::Hollow,
        Self::Horizontal,
        Self::Vertical,
        Self::ForwardDiagonal,
        Self::BackwardDiagonal,
        Self::Cross,
        Self::DiagonalCross,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Solid => "Solid",
            Self::Hollow => "Hollow",
            Self::Horizontal => "Horizontal",
            Self::Vertical => "Vertical",
            Self::ForwardDiagonal => "Forward diagonal",
            Self::BackwardDiagonal => "Backward diagonal",
            Self::Cross => "Cross",
            Self::DiagonalCross => "Diagonal cross",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EndStyle {
    pub shape: EndShape,
    pub color: Rgb,
    pub pattern: Pattern,
}
impl EndStyle {
    /// An unused end: no shape, white as on the reference bar list.
    pub fn none() -> Self {
        Self {
            shape: EndShape::None,
            color: Rgb::WHITE,
            pattern: Pattern::Solid,
        }
    }
    pub fn new(shape: EndShape, color: Rgb) -> Self {
        Self {
            shape,
            color,
            pattern: Pattern::Solid,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MiddleStyle {
    pub shape: BodyShape,
    /// One-pixel black border around the body.
    pub outline: bool,
    pub color: Rgb,
    pub pattern: Pattern,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LabelPosition {
    Left,
    Right,
    Top,
    Bottom,
    Center,
}
impl LabelPosition {
    pub const ALL: [LabelPosition; 5] = [
        Self::Left,
        Self::Right,
        Self::Top,
        Self::Bottom,
        Self::Center,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Left => "Left",
            Self::Right => "Right",
            Self::Top => "Top",
            Self::Bottom => "Bottom",
            Self::Center => "Center",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LabelField {
    ActivityId,
    ActivityName,
    Start,
    Finish,
    OriginalDuration,
    RemainingDuration,
    TotalFloat,
    FreeFloat,
    PercentComplete,
    ActivityStatus,
    ActivityType,
    Calendar,
    Wbs,
}
impl LabelField {
    pub const ALL: [LabelField; 13] = [
        Self::ActivityId,
        Self::ActivityName,
        Self::Start,
        Self::Finish,
        Self::OriginalDuration,
        Self::RemainingDuration,
        Self::TotalFloat,
        Self::FreeFloat,
        Self::PercentComplete,
        Self::ActivityStatus,
        Self::ActivityType,
        Self::Calendar,
        Self::Wbs,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::ActivityId => "Activity ID",
            Self::ActivityName => "Activity Name",
            Self::Start => "Start",
            Self::Finish => "Finish",
            Self::OriginalDuration => "Original Duration",
            Self::RemainingDuration => "Remaining Duration",
            Self::TotalFloat => "Total Float",
            Self::FreeFloat => "Free Float",
            Self::PercentComplete => "Activity % Complete",
            Self::ActivityStatus => "Activity Status",
            Self::ActivityType => "Activity Type",
            Self::Calendar => "Calendar",
            Self::Wbs => "WBS",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BarLabel {
    pub position: LabelPosition,
    pub field: LabelField,
}

/// One row of the Bars dialog.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BarDef {
    pub display: bool,
    pub name: String,
    pub timescale: BarSource,
    pub filter: BarFilter,
    pub start: EndStyle,
    pub middle: MiddleStyle,
    pub end: EndStyle,
    /// Bar row within the activity row, 1 to `MAX_BAR_ROW`.
    pub row: u8,
    pub labels: Vec<BarLabel>,
    pub show_when_collapsed: bool,
    pub show_for_grouping_bands: bool,
    pub neck_calendar_nonwork: bool,
    pub neck_activity_nonwork: bool,
}

impl BarDef {
    fn new(name: &str, display: bool, timescale: BarSource, filter: BarFilter) -> Self {
        Self {
            display,
            name: name.into(),
            timescale,
            filter,
            start: EndStyle::none(),
            middle: MiddleStyle {
                shape: BodyShape::Full,
                outline: true,
                color: Rgb::hex(crate::theme::BLUE),
                pattern: Pattern::Solid,
            },
            end: EndStyle::none(),
            row: 1,
            labels: vec![],
            // Off in the reference; the legacy defaults of tests keep it on.
            show_when_collapsed: !crate::defaults::reference(),
            show_for_grouping_bands: false,
            neck_calendar_nonwork: false,
            neck_activity_nonwork: false,
        }
    }
    fn body(mut self, shape: BodyShape, color: Rgb, outline: bool) -> Self {
        self.middle.shape = shape;
        self.middle.color = color;
        self.middle.outline = outline;
        self
    }
    fn caps(mut self, start: EndStyle, end: EndStyle) -> Self {
        self.start = start;
        self.end = end;
        self
    }
    fn label(mut self, position: LabelPosition, field: LabelField) -> Self {
        self.labels.push(BarLabel { position, field });
        self
    }
    /// A new bar as Add creates it: a displayed, outlined Current Bar.
    pub fn added(index: usize) -> Self {
        Self::new(
            &format!("New Bar {index}"),
            true,
            BarSource::CurrentBar,
            BarFilter::all(&[FilterKind::Normal]),
        )
    }
}

/// The colors of the default bar list. `Palette::theme()` is what new
/// charts use; `crate::theme::legacy` holds the earlier set so settings that
/// still hold it can be recognized and upgraded.
#[derive(Clone, Copy)]
pub struct BarPalette {
    pub remaining_loe: Rgb,
    pub actual_loe: Rgb,
    pub baseline_project: Rgb,
    pub baseline_primary: Rgb,
    pub baseline_secondary: Rgb,
    pub baseline_tertiary: Rgb,
    pub actual: Rgb,
    pub remaining: Rgb,
    pub critical: Rgb,
    pub constraint: Rgb,
    pub baseline_milestone: Rgb,
    pub milestone: Rgb,
    pub percent: Rgb,
    pub summary: Rgb,
    pub float: Rgb,
    pub negative_float: Rgb,
}

impl BarPalette {
    pub const fn theme() -> Self {
        use crate::theme as t;
        Self {
            remaining_loe: Rgb::hex(t::LOE_REMAINING),
            actual_loe: Rgb::hex(t::LOE_ACTUAL),
            baseline_project: Rgb::hex(t::BASELINE),
            baseline_primary: Rgb::hex(t::BASELINE),
            baseline_secondary: Rgb::hex(t::BASELINE_LIGHT),
            baseline_tertiary: Rgb::hex(t::BASELINE_PALE),
            actual: Rgb::hex(t::TEAL),
            remaining: Rgb::hex(t::BLUE),
            critical: Rgb::hex(t::CRITICAL),
            constraint: Rgb::hex(t::NAVY),
            baseline_milestone: Rgb::hex(t::BASELINE),
            milestone: Rgb::hex(t::NAVY),
            percent: Rgb::hex(t::NAVY),
            summary: Rgb::hex(t::NAVY),
            float: Rgb::hex(t::SLATE),
            negative_float: Rgb::hex(t::AMBER),
        }
    }
}

/// The default bar list: the standard bars, in order, in the theme's colors.
pub fn default_bars() -> Vec<BarDef> {
    bars_with(&BarPalette::theme())
}

/// The standard bar list in the given colors.
pub fn bars_with(c: &BarPalette) -> Vec<BarDef> {
    use BarSource as S;
    use FilterKind as F;
    let normal = || BarFilter::all(&[F::Normal]);
    let baseline = |name: &str, source: S, fill: Rgb| {
        BarDef::new(name, false, source, normal()).body(BodyShape::LowerThin, fill, true)
    };
    vec![
        BarDef::new(
            "Remaining Level of Effort",
            true,
            S::RemainBar,
            BarFilter::all(&[F::LevelOfEffort]),
        )
        .body(BodyShape::CenterHalf, c.remaining_loe, false),
        BarDef::new(
            "Actual Level of Effort",
            true,
            S::ActualBar,
            BarFilter::all(&[F::LevelOfEffort]),
        )
        .body(BodyShape::CenterHalf, c.actual_loe, false),
        baseline(
            "Project Baseline Bar",
            S::ProjectBaselineBar,
            c.baseline_project,
        ),
        baseline(
            "Primary Baseline Bar",
            S::PrimaryBaselineBar,
            c.baseline_primary,
        ),
        baseline(
            "Secondary Baseline Bar",
            S::SecondaryBaselineBar,
            c.baseline_secondary,
        ),
        baseline(
            "Tertiary Baseline Bar",
            S::TertiaryBaselineBar,
            c.baseline_tertiary,
        ),
        BarDef::new("Actual Work", true, S::ActualBar, normal()).body(
            BodyShape::Full,
            c.actual,
            true,
        ),
        BarDef::new(
            "Remaining Work",
            true,
            S::RemainBar,
            BarFilter::all(&[F::Normal, F::NonCritical]),
        )
        .body(BodyShape::Full, c.remaining, true),
        BarDef::new(
            "Critical Remaining Work",
            true,
            S::RemainBar,
            BarFilter::all(&[F::Normal, F::Critical]),
        )
        .body(BodyShape::Full, c.critical, true),
        BarDef::new(
            "Start Constraint",
            false,
            S::RemainBar,
            BarFilter::all(&[F::HasStartConstraint]),
        )
        .body(BodyShape::None, c.constraint, false)
        .caps(
            EndStyle::new(EndShape::Flag, c.constraint),
            EndStyle::none(),
        ),
        BarDef::new(
            "Finish Constraint",
            false,
            S::RemainBar,
            BarFilter::all(&[F::HasFinishConstraint]),
        )
        .body(BodyShape::None, c.constraint, false)
        .caps(
            EndStyle::none(),
            EndStyle::new(EndShape::Flag, c.constraint),
        ),
        BarDef::new(
            "Baseline Milestone",
            false,
            S::PrimaryBaselineBar,
            BarFilter::all(&[F::Milestone]),
        )
        .body(BodyShape::None, c.baseline_milestone, false)
        .caps(
            EndStyle::new(EndShape::Diamond, c.baseline_milestone),
            EndStyle::none(),
        ),
        BarDef::new(
            "Current Bar Labels",
            true,
            S::CurrentBar,
            BarFilter::any(&[F::Normal, F::LevelOfEffort]),
        )
        .body(BodyShape::None, Rgb::BLACK, false)
        .label(LabelPosition::Right, LabelField::ActivityName),
        BarDef::new(
            "Milestone",
            true,
            S::CurrentBar,
            BarFilter::all(&[F::Milestone]),
        )
        .body(BodyShape::None, c.milestone, false)
        .caps(
            EndStyle::new(EndShape::Diamond, c.milestone),
            EndStyle::none(),
        )
        .label(LabelPosition::Right, LabelField::ActivityName),
        BarDef::new("% Complete", false, S::PercentCompleteBar, normal()).body(
            BodyShape::UpperThin,
            c.percent,
            false,
        ),
        {
            let mut summary = BarDef::new(
                "Summary",
                true,
                S::CurrentBar,
                BarFilter::all(&[F::Summary]),
            )
            .body(BodyShape::UpperHalf, c.summary, false)
            .caps(
                EndStyle::new(EndShape::SummaryCap, c.summary),
                EndStyle::new(EndShape::SummaryCap, c.summary),
            )
            .label(LabelPosition::Right, LabelField::Finish)
            .label(LabelPosition::Right, LabelField::ActivityName);
            summary.show_for_grouping_bands = true;
            summary
        },
        BarDef::new(
            "Float Bar",
            false,
            S::FloatBar,
            BarFilter::all(&[F::AllActivities]),
        )
        .body(BodyShape::LowerThin, c.float, false),
        BarDef::new(
            "Neg Float Bar",
            false,
            S::NegFloatBar,
            BarFilter::all(&[F::NegativeFloat]),
        )
        .body(BodyShape::LowerThin, c.negative_float, true),
    ]
}

/// Timescale Date Interval choices. `Automatic` follows the zoom level.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DateInterval {
    Automatic,
    YearQuarter,
    YearMonth,
    QuarterMonth,
    MonthWeek,
    WeekDay1,
    WeekDay2,
    DayShift,
    DayHour,
}
impl DateInterval {
    pub const ALL: [DateInterval; 9] = [
        Self::Automatic,
        Self::YearQuarter,
        Self::YearMonth,
        Self::QuarterMonth,
        Self::MonthWeek,
        Self::WeekDay1,
        Self::WeekDay2,
        Self::DayShift,
        Self::DayHour,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Automatic => "Automatic (follows zoom)",
            Self::YearQuarter => "Year/Quarter",
            Self::YearMonth => "Year/Month",
            Self::QuarterMonth => "Quarter/Month",
            Self::MonthWeek => "Month/Week",
            Self::WeekDay1 => "Week/Day 1",
            Self::WeekDay2 => "Week/Day 2",
            Self::DayShift => "Day/Shift",
            Self::DayHour => "Day/Hour",
        }
    }
    /// Shift cells need shift calendars, which are not modeled.
    pub fn available(self) -> bool {
        self != Self::DayShift
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DateType {
    Calendar,
    Fiscal,
    WeekOfYear,
}
impl DateType {
    pub const ALL: [DateType; 3] = [Self::Calendar, Self::Fiscal, Self::WeekOfYear];
    pub fn label(self) -> &'static str {
        match self {
            Self::Calendar => "Calendar",
            Self::Fiscal => "Fiscal",
            Self::WeekOfYear => "Week of Year",
        }
    }
}

/// Where a date field is anchored: PS (project start), DD (data date) or a
/// fixed date, plus a day offset.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Anchor {
    ProjectStart,
    DataDate,
    /// Minutes from the model origin.
    Date(i64),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnchoredDate {
    pub anchor: Anchor,
    pub offset_days: i32,
}
impl AnchoredDate {
    pub fn project_start() -> Self {
        Self {
            anchor: Anchor::ProjectStart,
            offset_days: 0,
        }
    }
    pub fn resolve(self, project_start: i64, data_date: i64) -> i64 {
        let base = match self.anchor {
            Anchor::ProjectStart => project_start,
            Anchor::DataDate => data_date,
            Anchor::Date(minute) => minute,
        };
        base.saturating_add(i64::from(self.offset_days) * openxer_engine::calendar::DAY)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrdinalUnit {
    Day,
    Week,
    Month,
    Quarter,
    Year,
}
impl OrdinalUnit {
    pub const ALL: [OrdinalUnit; 5] = [
        Self::Day,
        Self::Week,
        Self::Month,
        Self::Quarter,
        Self::Year,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Day => "Day",
            Self::Week => "Week",
            Self::Month => "Month",
            Self::Quarter => "Quarter",
            Self::Year => "Year",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Weekday {
    Sunday,
    Monday,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TimescaleSettings {
    pub three_lines: bool,
    pub start: AnchoredDate,
    /// Label size in points (8 pt is the reference default).
    pub font_points: f32,
    pub font_bold: bool,
    pub font_color: Rgb,
    pub interval: DateInterval,
    pub date_type: DateType,
    /// First month of the fiscal year, 1 to 12.
    pub fiscal_start_month: u32,
    pub week_start: Weekday,
    pub ordinal: bool,
    pub ordinal_start: AnchoredDate,
    pub ordinal_unit: OrdinalUnit,
}
impl Default for TimescaleSettings {
    fn default() -> Self {
        Self {
            three_lines: false,
            start: AnchoredDate::project_start(),
            font_points: 8.,
            font_bold: false,
            font_color: Rgb::BLACK,
            // Quarter over Month, as the reference starts; the legacy
            // defaults of tests follow the zoom (`crate::defaults`).
            interval: if crate::defaults::reference() {
                DateInterval::QuarterMonth
            } else {
                DateInterval::Automatic
            },
            date_type: DateType::Calendar,
            fiscal_start_month: 1,
            week_start: Weekday::Sunday,
            ordinal: false,
            ordinal_start: AnchoredDate::project_start(),
            ordinal_unit: OrdinalUnit::Month,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LineStyle {
    Solid,
    Dashed,
    Dotted,
    DashDot,
}
impl LineStyle {
    pub const ALL: [LineStyle; 4] = [Self::Solid, Self::Dashed, Self::Dotted, Self::DashDot];
    pub fn label(self) -> &'static str {
        match self {
            Self::Solid => "Solid",
            Self::Dashed => "Dashed",
            Self::Dotted => "Dotted",
            Self::DashDot => "Dash dot",
        }
    }
    /// On/off run lengths in pixels; empty means solid.
    pub fn dashes(self) -> &'static [f32] {
        match self {
            Self::Solid => &[],
            Self::Dashed => &[3., 3.],
            Self::Dotted => &[1., 2.],
            Self::DashDot => &[6., 2., 1., 2.],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SightUnit {
    Day,
    Week,
    Month,
    Quarter,
    Year,
}
impl SightUnit {
    pub const ALL: [SightUnit; 5] = [
        Self::Day,
        Self::Week,
        Self::Month,
        Self::Quarter,
        Self::Year,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Day => "Day",
            Self::Week => "Week",
            Self::Month => "Month",
            Self::Quarter => "Quarter",
            Self::Year => "Year",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct HorizontalLines {
    pub show: bool,
    pub every_rows: u16,
    pub style: LineStyle,
    pub color: Rgb,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct VerticalLines {
    pub show: bool,
    pub every: u16,
    pub unit: SightUnit,
    pub start: AnchoredDate,
    pub style: LineStyle,
    pub color: Rgb,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SightLines {
    pub horizontal_major: HorizontalLines,
    pub horizontal_minor: HorizontalLines,
    pub vertical_major: VerticalLines,
    pub vertical_minor: VerticalLines,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct DataDateLine {
    pub style: LineStyle,
    pub size: u8,
    pub color: Rgb,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProgressBasis {
    PercentComplete,
    RemainingDuration,
}
impl ProgressBasis {
    pub fn label(self) -> &'static str {
        match self {
            Self::PercentComplete => "Percent Complete",
            Self::RemainingDuration => "Remaining Duration",
        }
    }
}
/// How the progress line finds each activity's point.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProgressMode {
    /// Shift from the data date by how far the current date is from the
    /// baseline date (behind schedule bends left).
    Difference,
    /// The progress point along an in-progress activity's bar.
    #[default]
    Points,
}

/// Which date the difference mode compares.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProgressDate {
    Start,
    #[default]
    Finish,
}
impl ProgressDate {
    pub fn label(self) -> &'static str {
        match self {
            Self::Start => "Start Date",
            Self::Finish => "Finish Date",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProgressLine {
    pub show: bool,
    pub size: u8,
    pub color: Rgb,
    pub basis: ProgressBasis,
    #[serde(default)]
    pub mode: ProgressMode,
    /// The baseline the difference mode compares against.
    #[serde(default)]
    pub baseline: crate::baselines::Slot,
    #[serde(default)]
    pub date: ProgressDate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CollapseMode {
    GroupingBands,
    IndividualBars,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct RelationshipColors {
    pub driving: Rgb,
    pub non_driving: Rgb,
    pub critical: Rgb,
    pub non_driving_dotted: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Nonwork {
    pub shade: bool,
    pub hatch: bool,
    pub color: Rgb,
}

/// The chart's text font (Chart Font and Colors): bar labels.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChartFont {
    pub face: crate::chrome::Face,
    pub points: f32,
    pub bold: bool,
    pub color: Rgb,
}
impl Default for ChartFont {
    /// Arial 8 pt black, the measured bar label font.
    fn default() -> Self {
        Self {
            face: crate::chrome::Face::Arial,
            points: 8.,
            bold: false,
            color: Rgb::BLACK,
        }
    }
}

fn white() -> Rgb {
    Rgb::WHITE
}

/// Bar Chart Options.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChartOptions {
    pub show_relationships: bool,
    pub relationship_colors: RelationshipColors,
    pub show_legend: bool,
    pub limit_label_chars: bool,
    pub label_chars: u16,
    pub notebook_width: u16,
    pub notebook_height: u16,
    pub collapse: CollapseMode,
    pub collapsed_relationships: bool,
    pub allow_label_overlap: bool,
    pub data_date: DataDateLine,
    pub sight_lines: SightLines,
    pub progress_line: ProgressLine,
    pub nonwork: Nonwork,
    /// Chart Font and Colors: label font and the chart background.
    #[serde(default)]
    pub font: ChartFont,
    #[serde(default = "white")]
    pub background: Rgb,
    /// Progress Spotlight: highlight the activities due in the next
    /// update period.
    #[serde(default)]
    pub spotlight: bool,
}
impl Default for ChartOptions {
    fn default() -> Self {
        let vertical = |show, every, unit, style| VerticalLines {
            show,
            every,
            unit,
            start: AnchoredDate::project_start(),
            style,
            color: Rgb::SIGHT,
        };
        Self {
            show_relationships: true,
            relationship_colors: RelationshipColors {
                driving: Rgb::BLACK,
                non_driving: Rgb::BLACK,
                critical: Rgb::hex(crate::theme::CRITICAL),
                non_driving_dotted: true,
            },
            show_legend: false,
            limit_label_chars: false,
            label_chars: 10,
            notebook_width: 100,
            notebook_height: 50,
            collapse: CollapseMode::GroupingBands,
            collapsed_relationships: false,
            allow_label_overlap: false,
            data_date: DataDateLine {
                style: LineStyle::Solid,
                size: 1,
                color: Rgb::hex(crate::theme::DATA_DATE),
            },
            sight_lines: SightLines {
                horizontal_major: HorizontalLines {
                    show: false,
                    every_rows: 1,
                    style: LineStyle::Solid,
                    color: Rgb::SIGHT,
                },
                horizontal_minor: HorizontalLines {
                    show: true,
                    every_rows: 5,
                    style: LineStyle::Dashed,
                    color: Rgb::SIGHT,
                },
                vertical_major: vertical(false, 1, SightUnit::Quarter, LineStyle::Solid),
                vertical_minor: vertical(true, 1, SightUnit::Month, LineStyle::Dashed),
            },
            progress_line: ProgressLine {
                show: false,
                size: 2,
                color: Rgb::hex(crate::theme::PROGRESS),
                basis: ProgressBasis::PercentComplete,
                mode: ProgressMode::Points,
                baseline: crate::baselines::Slot::Project,
                date: ProgressDate::Finish,
            },
            nonwork: Nonwork {
                shade: true,
                hatch: false,
                color: Rgb::from_color(crate::gantt_geometry::NONWORK),
            },
            font: ChartFont::default(),
            background: Rgb::WHITE,
            spotlight: false,
        }
    }
}

/// A shaded date range drawn over the chart body.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Curtain {
    pub display: bool,
    /// Minutes from the model origin.
    pub start: i64,
    pub finish: i64,
    pub color: Rgb,
    pub pattern: Pattern,
}

/// The font of a text attachment: face, size, style and color.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextFont {
    pub face: crate::chrome::Face,
    pub points: f32,
    pub bold: bool,
    pub italic: bool,
    pub color: Rgb,
}
impl Default for TextFont {
    /// Arial 8 pt black, as the first text attachments drew.
    fn default() -> Self {
        Self {
            face: crate::chrome::Face::Arial,
            points: 8.,
            bold: false,
            italic: false,
            color: Rgb::BLACK,
        }
    }
}

/// A text box on the chart (Attachments > Text), anchored to a date and to
/// the row of an activity or of a grouping band. It is kept with the layout
/// and printed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextAttachment {
    pub text: String,
    /// Minutes from the model origin at the box's left edge.
    pub date: i64,
    /// The activity whose row holds the box's top (when `band` is none).
    pub task_id: i64,
    /// The grouping band whose row holds the box's top, by the band's key.
    /// Attachments saved before band anchors existed have none.
    #[serde(default)]
    pub band: Option<String>,
    /// Attachments saved before fonts existed draw in the default font.
    #[serde(default)]
    pub font: TextFont,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GanttSettings {
    pub version: u32,
    pub bars: Vec<BarDef>,
    pub timescale: TimescaleSettings,
    pub options: ChartOptions,
    pub curtains: Vec<Curtain>,
    /// Settings saved before text attachments existed have none.
    #[serde(default)]
    pub texts: Vec<TextAttachment>,
}
impl Default for GanttSettings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            bars: default_bars(),
            timescale: TimescaleSettings::default(),
            options: ChartOptions::default(),
            curtains: vec![],
            texts: vec![],
        }
    }
}

impl GanttSettings {
    /// Rejects settings the chart cannot draw sensibly. Called before saving
    /// and after loading, so a damaged preference never reaches the painter.
    pub fn validate(&self) -> Result<(), String> {
        if self.version != SETTINGS_VERSION {
            return Err(format!(
                "Unsupported Gantt settings version {}",
                self.version
            ));
        }
        if self.bars.len() > MAX_BARS {
            return Err(format!("At most {MAX_BARS} bars are allowed"));
        }
        for (index, bar) in self.bars.iter().enumerate() {
            let at = format!("Bar {} ({:?})", index + 1, bar.name);
            if bar.name.trim().is_empty() {
                return Err(format!("Bar {} needs a name", index + 1));
            }
            if bar.name.chars().count() > 100 {
                return Err(format!("{at}: the name is longer than 100 characters"));
            }
            if !(1..=MAX_BAR_ROW).contains(&bar.row) {
                return Err(format!("{at}: row must be from 1 to {MAX_BAR_ROW}"));
            }
            if bar.filter.kinds.is_empty() {
                return Err(format!("{at}: choose at least one filter"));
            }
            if bar.labels.len() > MAX_LABELS {
                return Err(format!("{at}: at most {MAX_LABELS} labels"));
            }
        }
        let t = &self.timescale;
        if !(t.font_points.is_finite() && (6.0..=16.0).contains(&t.font_points)) {
            return Err("Timescale font size must be from 6 to 16 points".into());
        }
        if !(1..=12).contains(&t.fiscal_start_month) {
            return Err("Fiscal year start month must be from 1 to 12".into());
        }
        if !t.interval.available() {
            return Err(format!("{} needs shift calendars", t.interval.label()));
        }
        for date in [t.start, t.ordinal_start] {
            if date.offset_days.unsigned_abs() > 36_600 {
                return Err("Date offsets must be within 100 years".into());
            }
        }
        let o = &self.options;
        if !(1..=999).contains(&o.label_chars) {
            return Err("Label character limit must be from 1 to 999".into());
        }
        if !(10..=999).contains(&o.notebook_width) || !(10..=999).contains(&o.notebook_height) {
            return Err("Notebook item size must be from 10 to 999 pixels".into());
        }
        if !(o.font.points.is_finite() && (6.0..=20.0).contains(&o.font.points)) {
            return Err("Chart font size must be from 6 to 20 points".into());
        }
        if !(1..=5).contains(&o.data_date.size) || !(1..=5).contains(&o.progress_line.size) {
            return Err("Line sizes must be from 1 to 5 pixels".into());
        }
        let s = &o.sight_lines;
        for lines in [s.horizontal_major, s.horizontal_minor] {
            if !(1..=999).contains(&lines.every_rows) {
                return Err("Horizontal sight lines must repeat every 1 to 999 rows".into());
            }
        }
        for lines in [s.vertical_major, s.vertical_minor] {
            if !(1..=999).contains(&lines.every) {
                return Err("Vertical sight lines must repeat every 1 to 999 units".into());
            }
        }
        if self.curtains.len() > MAX_CURTAINS {
            return Err(format!("At most {MAX_CURTAINS} curtains are allowed"));
        }
        for (index, curtain) in self.curtains.iter().enumerate() {
            if curtain.start >= curtain.finish {
                return Err(format!(
                    "Curtain {}: the finish must be after the start",
                    index + 1
                ));
            }
        }
        if self.texts.len() > MAX_TEXTS {
            return Err(format!("At most {MAX_TEXTS} text attachments are allowed"));
        }
        for (index, text) in self.texts.iter().enumerate() {
            if text.text.trim().is_empty() {
                return Err(format!("Text attachment {} has no text", index + 1));
            }
            if text.text.chars().count() > MAX_TEXT_CHARS {
                return Err(format!(
                    "Text attachment {} is longer than {MAX_TEXT_CHARS} characters",
                    index + 1
                ));
            }
            if text.band.as_ref().is_some_and(|b| b.len() > MAX_BAND_KEY) {
                return Err(format!("Text attachment {} has a bad band", index + 1));
            }
            if !(text.font.points.is_finite() && (6.0..=20.0).contains(&text.font.points)) {
                return Err(format!(
                    "Text attachment {}: font size must be from 6 to 20 points",
                    index + 1
                ));
            }
        }
        Ok(())
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    pub fn from_json(json: &str) -> Result<Self, String> {
        let settings: Self = serde_json::from_str(json).map_err(|e| e.to_string())?;
        settings.validate()?;
        Ok(settings)
    }

    /// The number of bar rows a displayed activity row needs.
    pub fn bar_rows(&self) -> u8 {
        self.bars
            .iter()
            .filter(|b| b.display)
            .map(|b| b.row)
            .max()
            .unwrap_or(1)
            .clamp(1, MAX_BAR_ROW)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_follow_the_standard_bar_list_and_validate() {
        let settings = GanttSettings::default();
        settings.validate().unwrap();
        let names: Vec<_> = settings.bars.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names.len(), 18);
        assert_eq!(names[0], "Remaining Level of Effort");
        assert_eq!(names[17], "Neg Float Bar");
        let displayed: Vec<_> = settings
            .bars
            .iter()
            .filter(|b| b.display)
            .map(|b| b.name.as_str())
            .collect();
        assert_eq!(
            displayed,
            [
                "Remaining Level of Effort",
                "Actual Level of Effort",
                "Actual Work",
                "Remaining Work",
                "Critical Remaining Work",
                "Current Bar Labels",
                "Milestone",
                "Summary"
            ]
        );
        let remaining = &settings.bars[7];
        assert_eq!(remaining.filter.label(), "Normal and Non-critical");
        assert_eq!(remaining.middle.color, Rgb::hex(crate::theme::BLUE));
        assert_eq!(settings.bars[6].middle.color, Rgb::hex(crate::theme::TEAL));
        assert_eq!(
            settings.bars[8].middle.color,
            Rgb::hex(crate::theme::CRITICAL)
        );
        assert_eq!(
            settings.bars[12].filter.label(),
            "Normal or Level of Effort"
        );
        assert_eq!(settings.bar_rows(), 1);
    }

    #[test]
    fn json_round_trips_and_invalid_data_is_rejected() {
        let mut settings = GanttSettings::default();
        settings.bars[2].display = true;
        settings.bars[2].row = 2;
        settings.curtains.push(Curtain {
            display: true,
            start: 100,
            finish: 200,
            color: Rgb(1, 2, 3),
            pattern: Pattern::Cross,
        });
        settings.texts.push(TextAttachment {
            text: "Shutdown\nwindow".into(),
            date: 300,
            task_id: 1010,
            band: None,
            font: TextFont {
                face: crate::chrome::Face::Tahoma,
                points: 10.,
                bold: true,
                italic: true,
                color: Rgb(9, 8, 7),
            },
        });
        settings.texts.push(TextAttachment {
            text: "Band note".into(),
            date: 400,
            task_id: 0,
            band: Some("wbs=A|".into()),
            font: TextFont::default(),
        });
        let json = settings.to_json();
        assert!(json.contains("\"#010203\""));
        let back = GanttSettings::from_json(&json).unwrap();
        assert_eq!(back, settings);
        assert_eq!(back.bar_rows(), 2);

        let mut bad = settings.clone();
        bad.bars[0].row = 4;
        assert!(GanttSettings::from_json(&bad.to_json()).is_err());
        let mut bad = settings.clone();
        bad.curtains[0].finish = 50;
        assert!(GanttSettings::from_json(&bad.to_json()).is_err());
        let mut bad = settings.clone();
        bad.bars[1].name = "  ".into();
        assert!(bad.validate().is_err());
        let mut bad = settings.clone();
        bad.timescale.interval = DateInterval::DayShift;
        assert!(bad.validate().is_err());
        assert!(GanttSettings::from_json(&json.replace("#010203", "#01020")).is_err());
        let mut bad = settings.clone();
        bad.texts[0].text = " ".into();
        assert!(bad.validate().is_err());
        let mut bad = settings.clone();
        bad.texts[0].font.points = 3.;
        assert!(bad.validate().is_err());
        // Text attachments saved before bands and fonts load with the
        // default font on an activity row.
        let mut older: serde_json::Value = serde_json::from_str(&json).unwrap();
        for t in older["texts"].as_array_mut().unwrap() {
            let t = t.as_object_mut().unwrap();
            t.remove("band");
            t.remove("font");
        }
        let back = GanttSettings::from_json(&older.to_string()).unwrap();
        assert_eq!(back.texts[0].band, None);
        assert_eq!(back.texts[0].font, TextFont::default());
        // Settings saved before text attachments load with none.
        let mut old: serde_json::Value = serde_json::from_str(&json).unwrap();
        old.as_object_mut().unwrap().remove("texts");
        let back = GanttSettings::from_json(&old.to_string()).unwrap();
        assert!(back.texts.is_empty());
        assert!(GanttSettings::from_json("{\"version\":1}").is_err());
        assert!(GanttSettings::from_json("not json").is_err());
    }
}
