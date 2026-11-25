//! Scheduling options. Defaults are those of a new project.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LagCalendar {
    PredCal,
    SuccCal,
    Hour24,
    /// The project's default calendar (saved probe 503; REL-2).
    ProjCal,
}
impl LagCalendar {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PredCal => "PredCal",
            Self::SuccCal => "SuccCal",
            Self::Hour24 => "24Hour",
            Self::ProjCal => "ProjCal",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FloatType {
    Start,
    Finish,
    Min,
}
impl FloatType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Start => "FT_SS",
            Self::Finish => "FT_FF",
            Self::Min => "FT_Min",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutOfSequence {
    None,
    RetainedLogic,
    ProgressOverride,
    Both,
}
impl OutOfSequence {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "SD_None",
            Self::RetainedLogic => "SD_RetainedLogic",
            Self::ProgressOverride => "SD_ProgressOverride",
            Self::Both => "SD_Both",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduleOptions {
    /// Retained Logic scheduling mode (both modes off is Actual Dates).
    pub retained_logic: bool,
    /// Progress Override scheduling mode.
    pub progress_override: bool,
    /// Count start lags from the early start (FLT-2).
    pub lag_from_early_start: bool,
    /// Use Expected Finish dates (EXP-1).
    pub use_expected_finish: bool,
    /// Level resources as part of scheduling.
    pub do_level: bool,
    /// Stored and written back with the project; no scheduling rule reads it.
    pub use_remaining_dates: bool,
    /// Calendar that counts relationship lags (REL-2).
    pub lag_calendar: LagCalendar,
    /// Which float the total float reports (FLT-6).
    pub float_type: FloatType,
    /// Anything but `None` honors early starts and late finishes that
    /// relationships to and from other projects impose (FWD-2, BWD-4).
    pub out_of_sequence: OutOfSequence,
    /// Open ends take their early finish as their late finish (BWD-4).
    pub open_end_uses_early: bool,
    pub use_must_finish: bool,
    pub multiple_float_paths: bool,
    pub float_paths_use_total: bool,
    pub max_float_paths: usize,
    pub float_paths_end_activity: Option<i64>,
    /// Global administrator setting, not an XER project option: finish a
    /// negative-lag FF or nonpositive-lag SF candidate at a day's first work
    /// start on the previous working finish (ADM-1).
    pub finish_on_valid_time: bool,
}
impl Default for ScheduleOptions {
    fn default() -> Self {
        Self {
            retained_logic: false,
            progress_override: false,
            lag_from_early_start: false,
            use_expected_finish: false,
            do_level: false,
            use_remaining_dates: false,
            lag_calendar: LagCalendar::PredCal,
            float_type: FloatType::Min,
            out_of_sequence: OutOfSequence::None,
            open_end_uses_early: true,
            use_must_finish: true,
            multiple_float_paths: false,
            float_paths_use_total: true,
            max_float_paths: 10,
            float_paths_end_activity: None,
            finish_on_valid_time: false,
        }
    }
}
impl ScheduleOptions {
    /// True in Actual Dates mode: neither Retained Logic nor Progress Override.
    pub fn neither_oos(&self) -> bool {
        !self.retained_logic && !self.progress_override
    }
}
