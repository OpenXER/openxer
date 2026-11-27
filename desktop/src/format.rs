//! User display formats: durations, floats, units, dates and costs.
//!
//! Durations and floats are stored in minutes of work time. They are shown in
//! the user's unit of time, converted with the hours per period of the calendar
//! that owns the value (the activity's calendar), falling back to the project
//! calendar and then to the default periods of 8 hours per day, 40 per week,
//! 172 per month and 2000 per year. Parsing accepts the same text back.
//!
//! The active formats live in a thread-local so call sites stay one-liners;
//! the app installs its saved formats at the start of every frame.
use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime, Timelike};
use eframe::egui;
use openxer_engine::{calendar::DAY, options::LagCalendar};
use openxer_xer::ProjectData;
use serde_json::{json, Value};
use std::cell::RefCell;

/// Preference key holding the saved formats as JSON.
pub const PREFERENCE_KEY: &str = "user_preferences_v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeUnit {
    Hour,
    Day,
    Week,
    Month,
    Year,
}
impl TimeUnit {
    pub const ALL: [Self; 5] = [Self::Hour, Self::Day, Self::Week, Self::Month, Self::Year];
    pub fn label(self) -> &'static str {
        match self {
            Self::Hour => "Hour",
            Self::Day => "Day",
            Self::Week => "Week",
            Self::Month => "Month",
            Self::Year => "Year",
        }
    }
    /// The abbreviation printed after a value.
    pub fn suffix(self) -> &'static str {
        match self {
            Self::Hour => "h",
            Self::Day => "d",
            Self::Week => "w",
            Self::Month => "m",
            Self::Year => "y",
        }
    }
    fn code(self) -> &'static str {
        match self {
            Self::Hour => "hour",
            Self::Day => "day",
            Self::Week => "week",
            Self::Month => "month",
            Self::Year => "year",
        }
    }
    fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|u| u.code() == code)
    }
    /// The caption of the sub-unit check box for this unit.
    pub fn sub_unit_label(self) -> &'static str {
        match self {
            Self::Hour => "Minutes",
            Self::Day => "Hours",
            Self::Week => "Days",
            Self::Month => "Weeks",
            Self::Year => "Months",
        }
    }
}

/// Minutes of work in each period, from a calendar's hours per period.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Periods {
    pub day: i64,
    pub week: i64,
    pub month: i64,
    pub year: i64,
}
impl Periods {
    /// Default hours per period when no calendar supplies them: 8, 40, 172, 2000.
    pub const DEFAULT: Self = Self {
        day: 8 * 60,
        week: 40 * 60,
        month: 172 * 60,
        year: 2000 * 60,
    };
    /// A calendar with `hours` per day and default week, month and year hours.
    #[cfg(test)]
    pub fn with_day_hours(hours: f64) -> Self {
        Self {
            day: Self::valid(hours, 24.).unwrap_or(Self::DEFAULT.day),
            ..Self::DEFAULT
        }
    }
    fn valid(hours: f64, max: f64) -> Option<i64> {
        (hours.is_finite() && hours > 0. && hours <= max).then(|| (hours * 60.).round() as i64)
    }
    /// Each field from `hours` (day, week, month, year) when valid, else from `fallback`.
    fn from_hours(hours: [f64; 4], fallback: Self) -> Self {
        Self {
            day: Self::valid(hours[0], 24.).unwrap_or(fallback.day),
            week: Self::valid(hours[1], 168.).unwrap_or(fallback.week),
            month: Self::valid(hours[2], 744.).unwrap_or(fallback.month),
            year: Self::valid(hours[3], 8784.).unwrap_or(fallback.year),
        }
    }
    fn minutes(self, unit: TimeUnit) -> i64 {
        match unit {
            TimeUnit::Hour => 60,
            TimeUnit::Day => self.day,
            TimeUnit::Week => self.week,
            TimeUnit::Month => self.month,
            TimeUnit::Year => self.year,
        }
    }
    /// Minutes of the sub-unit shown after whole `unit`s.
    fn sub_minutes(self, unit: TimeUnit) -> i64 {
        match unit {
            TimeUnit::Hour => 1,
            TimeUnit::Day => 60,
            TimeUnit::Week => self.day,
            TimeUnit::Month => self.week,
            TimeUnit::Year => self.month,
        }
    }
}

/// Periods of the calendar at `index` in the project model.
pub fn calendar_periods(p: &ProjectData, index: usize) -> Periods {
    let project = p
        .model
        .calendars
        .get(p.model.project_calendar)
        .map_or(Periods::DEFAULT, |c| {
            Periods::from_hours(
                crate::calendars::ops::hours(&p.document, &c.id),
                Periods::DEFAULT,
            )
        });
    p.model.calendars.get(index).map_or(project, |c| {
        Periods::from_hours(crate::calendars::ops::hours(&p.document, &c.id), project)
    })
}
/// Periods of the activity at `index` (its own calendar).
pub fn activity_periods(p: &ProjectData, index: usize) -> Periods {
    let cal = p
        .model
        .activities
        .get(index)
        .map_or(p.model.project_calendar, |a| a.cal);
    calendar_periods(p, cal)
}
/// Periods of the project default calendar.
pub fn project_periods(p: &ProjectData) -> Periods {
    calendar_periods(p, p.model.project_calendar)
}
/// Periods for a relationship lag between `pred` and `succ`, following the
/// schedule option that chooses the lag calendar.
pub fn lag_periods(p: &ProjectData, pred: usize, succ: usize) -> Periods {
    match p.model.options.lag_calendar {
        LagCalendar::PredCal => activity_periods(p, pred),
        LagCalendar::SuccCal => activity_periods(p, succ),
        LagCalendar::ProjCal => project_periods(p),
        LagCalendar::Hour24 => Periods {
            day: DAY,
            week: 7 * DAY,
            month: 744 * 60,
            year: 8760 * 60,
        },
    }
}

/// How one kind of time quantity is shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitFormat {
    pub unit: TimeUnit,
    pub sub_unit: bool,
    pub decimals: u8,
    pub show_label: bool,
}
impl UnitFormat {
    pub const MAX_DECIMALS: u8 = 2;

    /// `minutes` of work time as text, using `periods` for the conversion.
    pub fn format(&self, minutes: i64, periods: Periods) -> String {
        let unit = periods.minutes(self.unit).max(1) as f64;
        let negative = minutes < 0;
        let abs = minutes.unsigned_abs() as f64;
        let scale = 10f64.powi(self.decimals as i32);
        let round = |v: f64| (v * scale).round() / scale;
        let fixed = |v: f64| format!("{v:.*}", self.decimals as usize);
        let (text, zero) = if self.sub_unit {
            let sub = periods.sub_minutes(self.unit).max(1) as f64;
            let mut whole = (abs / unit).floor();
            let mut rest = round((abs - whole * unit) / sub);
            if rest * sub >= unit {
                whole += 1.;
                rest = 0.;
            }
            let sub_suffix = if self.unit == TimeUnit::Hour {
                "n"
            } else {
                Self::sub_unit_of(self.unit).suffix()
            };
            let text = match (whole > 0., rest > 0.) {
                (true, true) => {
                    format!("{whole}{} {}{sub_suffix}", self.unit.suffix(), fixed(rest))
                }
                (false, true) => format!("{}{sub_suffix}", fixed(rest)),
                _ => format!("{whole}{}", self.unit.suffix()),
            };
            (text, whole == 0. && rest == 0.)
        } else {
            let value = round(abs / unit);
            let label = if self.show_label {
                self.unit.suffix()
            } else {
                ""
            };
            (format!("{}{label}", fixed(value)), value == 0.)
        };
        if negative && !zero {
            format!("-{text}")
        } else {
            text
        }
    }
    fn sub_unit_of(unit: TimeUnit) -> TimeUnit {
        match unit {
            TimeUnit::Hour | TimeUnit::Day => TimeUnit::Hour,
            TimeUnit::Week => TimeUnit::Day,
            TimeUnit::Month => TimeUnit::Week,
            TimeUnit::Year => TimeUnit::Month,
        }
    }

    /// Parse text such as `5`, `5d`, `-2.5d`, `5d 4h`, `40h` or `1w 2d` into
    /// minutes. A bare number is in this format's unit.
    pub fn parse(&self, text: &str, periods: Periods) -> Result<i64, String> {
        let invalid = || format!("{} is not a valid duration", text.trim());
        let mut rest = text.trim();
        let negative = rest.starts_with('-');
        rest = rest.trim_start_matches(['-', '+']).trim_start();
        if rest.is_empty() {
            return Err(invalid());
        }
        let mut total = 0f64;
        let mut first = true;
        while !rest.is_empty() {
            let number_end = rest
                .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == ','))
                .unwrap_or(rest.len());
            let number: f64 = rest[..number_end]
                .replace(',', "")
                .parse()
                .map_err(|_| invalid())?;
            rest = rest[number_end..].trim_start();
            let label_end = rest
                .find(|c: char| !c.is_ascii_alphabetic())
                .unwrap_or(rest.len());
            let label = rest[..label_end].to_ascii_lowercase();
            rest = rest[label_end..].trim_start();
            let minutes = match label.as_str() {
                "" if first => periods.minutes(self.unit) as f64,
                "" => periods.sub_minutes(self.unit) as f64,
                "n" | "min" | "mins" | "minute" | "minutes" => 1.,
                "h" | "hr" | "hrs" | "hour" | "hours" => 60.,
                "d" | "day" | "days" => periods.day as f64,
                "w" | "wk" | "wks" | "week" | "weeks" => periods.week as f64,
                "m" | "mo" | "mon" | "month" | "months" => periods.month as f64,
                "y" | "yr" | "yrs" | "year" | "years" => periods.year as f64,
                _ => return Err(invalid()),
            };
            total += number * minutes;
            first = false;
        }
        if !total.is_finite() || total > 1e12 {
            return Err(invalid());
        }
        let minutes = total.round() as i64;
        Ok(if negative { -minutes } else { minutes })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateOrder {
    MonthDayYear,
    DayMonthYear,
    YearMonthDay,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeDisplay {
    Hour12,
    Hour24,
    Hidden,
}
pub const SEPARATORS: [char; 4] = ['/', '-', '.', ' '];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DateFormat {
    pub order: DateOrder,
    pub four_digit_year: bool,
    pub month_name: bool,
    pub leading_zeros: bool,
    pub separator: char,
    pub time: TimeDisplay,
    pub show_minutes: bool,
}
impl DateFormat {
    pub fn format(&self, at: NaiveDateTime) -> String {
        let pad = |v: u32| {
            if self.leading_zeros {
                format!("{v:02}")
            } else {
                v.to_string()
            }
        };
        let day = pad(at.day());
        let month = if self.month_name {
            at.format("%b").to_string()
        } else {
            pad(at.month())
        };
        let year = if self.four_digit_year {
            format!("{:04}", at.year())
        } else {
            format!("{:02}", at.year().rem_euclid(100))
        };
        let parts = match self.order {
            DateOrder::MonthDayYear => [month, day, year],
            DateOrder::DayMonthYear => [day, month, year],
            DateOrder::YearMonthDay => [year, month, day],
        };
        let date = parts.join(&self.separator.to_string());
        let minutes = if self.show_minutes {
            format!(":{:02}", at.minute())
        } else {
            String::new()
        };
        match self.time {
            TimeDisplay::Hidden => date,
            TimeDisplay::Hour24 => format!("{date} {:02}{minutes}", at.hour()),
            TimeDisplay::Hour12 => {
                let (pm, hour) = at.hour12();
                format!("{date} {hour}{minutes} {}", if pm { "PM" } else { "AM" })
            }
        }
    }

    /// The date part as an Excel number format (`dd-mmm-yy` for the
    /// default), for workbooks the app writes.
    pub fn excel_pattern(&self) -> String {
        let (d, m) = if self.leading_zeros {
            ("dd", "mm")
        } else {
            ("d", "m")
        };
        let month = if self.month_name { "mmm" } else { m };
        let year = if self.four_digit_year { "yyyy" } else { "yy" };
        let parts = match self.order {
            DateOrder::MonthDayYear => [month, d, year],
            DateOrder::DayMonthYear => [d, month, year],
            DateOrder::YearMonthDay => [year, month, d],
        };
        let sep = match self.separator {
            ' ' => " ".to_string(),
            c => format!("\\{c}"),
        };
        parts.join(&sep)
    }

    /// Parse a date typed in this format (any of the separators, a month
    /// name or number, two or four digit years, an optional time in 24 or 12
    /// hour form, and a trailing marker: actual `A`, constraint `*` or the
    /// double dagger an earlier build showed). When no time is
    /// typed, `keep` supplies it (midnight without it). ISO text is accepted.
    pub fn parse(&self, text: &str, keep: Option<NaiveTime>) -> Option<NaiveDateTime> {
        let text = strip_date_marker(text);
        if let Some(iso) = iso(text) {
            return Some(if text.len() == 10 {
                iso.date().and_time(keep.unwrap_or(NaiveTime::MIN))
            } else {
                iso
            });
        }
        let mut tokens = vec![];
        let mut current = String::new();
        let flush = |current: &mut String, tokens: &mut Vec<String>| {
            if !current.is_empty() {
                tokens.push(std::mem::take(current));
            }
        };
        for c in text.chars() {
            if c.is_whitespace() || matches!(c, '/' | '-' | '.' | ',') {
                flush(&mut current, &mut tokens);
            } else if !current.is_empty()
                && current.chars().last().is_some_and(|l| l.is_ascii_digit()) != c.is_ascii_digit()
                && c != ':'
                && !current.ends_with(':')
            {
                flush(&mut current, &mut tokens);
                current.push(c);
            } else {
                current.push(c);
            }
        }
        flush(&mut current, &mut tokens);
        if tokens.len() < 3 {
            return None;
        }
        let (date_tokens, time_tokens) = tokens.split_at(3);
        let named = date_tokens
            .iter()
            .position(|t| t.chars().all(|c| c.is_ascii_alphabetic()));
        let positions = match self.order {
            DateOrder::MonthDayYear => [1, 0, 2],
            DateOrder::DayMonthYear => [0, 1, 2],
            DateOrder::YearMonthDay => [2, 1, 0],
        };
        let (mut day_at, mut month_at, mut year_at) = (positions[0], positions[1], positions[2]);
        if let Some(named) = named {
            // A month name fixes the month; the numbers keep their relative order.
            let mut others: Vec<usize> = [day_at, year_at].into();
            others.sort_unstable();
            let free: Vec<usize> = (0..3).filter(|&i| i != named).collect();
            let rank = |p: usize| others.iter().position(|&o| o == p).unwrap();
            day_at = free[rank(day_at)];
            year_at = free[rank(year_at)];
            month_at = named;
        }
        let day: u32 = date_tokens[day_at].parse().ok()?;
        let month = month_number(&date_tokens[month_at])?;
        let year_text = &date_tokens[year_at];
        let year: i32 = year_text.parse().ok()?;
        let year = if year_text.len() <= 2 {
            if year < 69 {
                2000 + year
            } else {
                1900 + year
            }
        } else {
            year
        };
        let date = NaiveDate::from_ymd_opt(year, month, day)?;
        let time = if time_tokens.is_empty() {
            keep.unwrap_or(NaiveTime::MIN)
        } else {
            parse_time(&time_tokens.join(" "))?
        };
        Some(date.and_time(time))
    }
}

fn iso(text: &str) -> Option<NaiveDateTime> {
    let bytes = text.as_bytes();
    if bytes.len() < 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M")
        .or_else(|_| NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S"))
        .ok()
        .or_else(|| {
            NaiveDate::parse_from_str(text, "%Y-%m-%d")
                .ok()
                .map(|d| d.and_time(NaiveTime::MIN))
        })
}

fn month_number(token: &str) -> Option<u32> {
    if let Ok(n) = token.parse::<u32>() {
        return (1..=12).contains(&n).then_some(n);
    }
    let lower = token.to_ascii_lowercase();
    if lower.len() < 3 {
        return None;
    }
    const NAMES: [&str; 12] = [
        "january",
        "february",
        "march",
        "april",
        "may",
        "june",
        "july",
        "august",
        "september",
        "october",
        "november",
        "december",
    ];
    NAMES
        .iter()
        .position(|name| name.starts_with(&lower))
        .map(|i| i as u32 + 1)
}

fn parse_time(text: &str) -> Option<NaiveTime> {
    let lower = text.to_ascii_lowercase().replace(' ', "");
    let (body, meridiem) = if let Some(b) = lower.strip_suffix("am") {
        (b, Some(false))
    } else if let Some(b) = lower.strip_suffix("pm") {
        (b, Some(true))
    } else {
        (lower.as_str(), None)
    };
    let (hour, minute) = match body.split_once(':') {
        Some((h, m)) => (h.parse::<u32>().ok()?, m.parse::<u32>().ok()?),
        None => (body.parse::<u32>().ok()?, 0),
    };
    let hour = match meridiem {
        Some(pm) if (1..=12).contains(&hour) => hour % 12 + if pm { 12 } else { 0 },
        Some(_) => return None,
        None => hour,
    };
    NaiveTime::from_hms_opt(hour, minute, 0)
}

/// Units per time: a percentage of one unit per hour, or units per duration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitsPerTime {
    Percentage,
    UnitsPerDuration,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CurrencyFormat {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub decimal_symbol: char,
    pub group_symbol: char,
    pub group_digits: u8,
    /// Positive and negative patterns: `#` is the symbol, `1.1` the number.
    pub positive: String,
    pub negative: String,
    pub show_symbol: bool,
    pub show_decimals: bool,
}
impl Default for CurrencyFormat {
    fn default() -> Self {
        Self {
            name: "Dollar".into(),
            symbol: "$".into(),
            decimals: 2,
            decimal_symbol: '.',
            group_symbol: ',',
            group_digits: 3,
            positive: "#1.1".into(),
            negative: "(#1.1)".into(),
            show_symbol: true,
            show_decimals: true,
        }
    }
}
impl CurrencyFormat {
    pub const MAX_DECIMALS: u8 = 4;
    pub fn format(&self, value: f64) -> String {
        let decimals = if self.show_decimals { self.decimals } else { 0 } as usize;
        let scale = 10f64.powi(decimals as i32);
        let text = format!("{:.*}", decimals, (value.abs() * scale).round() / scale);
        let (whole, fraction) = text.split_once('.').unwrap_or((&text, ""));
        let mut grouped = String::new();
        let group = self.group_digits.max(1) as usize;
        for (i, c) in whole.chars().enumerate() {
            if i > 0 && self.group_symbol != '\0' && (whole.len() - i).is_multiple_of(group) {
                grouped.push(self.group_symbol);
            }
            grouped.push(c);
        }
        if !fraction.is_empty() {
            grouped.push(self.decimal_symbol);
            grouped.push_str(fraction);
        }
        let zero = text.chars().all(|c| c == '0' || c == '.');
        let pattern = if value < 0. && !zero {
            &self.negative
        } else {
            &self.positive
        };
        let symbol = if self.show_symbol { &self.symbol } else { "" };
        pattern
            .replacen('#', symbol, 1)
            .replacen("1.1", &grouped, 1)
            .trim()
            .to_string()
    }

    /// An amount typed or shown in this format: the decimal symbol, digit
    /// grouping and the negative pattern are this format's; the symbol and
    /// spaces may be left out. A minus sign or brackets also mark a
    /// negative amount. Every text `format` shows reads back as its value.
    pub fn parse(&self, text: &str) -> Option<f64> {
        let mut t = text.trim().to_string();
        if t.is_empty() {
            return None;
        }
        if !self.symbol.is_empty() {
            t = t.replace(self.symbol.as_str(), "");
        }
        // The negative pattern's own marks, then a plain minus or brackets.
        let marks: String = self
            .negative
            .replacen('#', "", 1)
            .replacen("1.1", "", 1)
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        let negative = t.contains('-')
            || (t.contains('(') && t.contains(')'))
            || (!marks.is_empty() && marks.chars().all(|m| t.contains(m)) && marks != "+");
        let decimal = self.decimal_symbol;
        let mut number = String::new();
        let mut seen_decimal = false;
        for c in t.chars() {
            if c.is_ascii_digit() {
                number.push(c);
            } else if c == decimal {
                if seen_decimal {
                    return None;
                }
                seen_decimal = true;
                number.push('.');
            } else if c == self.group_symbol
                || c.is_whitespace()
                || c == '\u{a0}'
                || matches!(c, '-' | '(' | ')' | '+')
                || marks.contains(c)
            {
                continue;
            } else {
                return None;
            }
        }
        if number.is_empty() || number == "." {
            return None;
        }
        let value = number.parse::<f64>().ok().filter(|v| v.is_finite())?;
        Some(if negative { -value } else { value })
    }

    /// Currency definitions in the project's CURRTYPE table.
    pub fn from_project(p: &ProjectData) -> Vec<Self> {
        let Some(t) = p.document.table("CURRTYPE") else {
            return vec![];
        };
        let char_of = |s: &str, default: char| s.chars().next().unwrap_or(default);
        t.rows
            .iter()
            .map(|r| {
                let v = |k: &str| t.value(r, k).trim().to_string();
                let pattern = |k: &str, default: &str| {
                    let s = v(k);
                    if s.contains("1.1") {
                        s
                    } else {
                        default.to_string()
                    }
                };
                Self {
                    name: v("curr_type"),
                    symbol: v("curr_symbol"),
                    decimals: v("decimal_digit_cnt")
                        .parse::<u8>()
                        .unwrap_or(2)
                        .min(Self::MAX_DECIMALS),
                    decimal_symbol: char_of(&v("decimal_symbol"), '.'),
                    group_symbol: char_of(&v("digit_group_symbol"), ','),
                    group_digits: v("group_digit_cnt").parse::<u8>().unwrap_or(3).clamp(1, 9),
                    positive: pattern("pos_curr_fmt_type", "#1.1"),
                    negative: pattern("neg_curr_fmt_type", "(#1.1)"),
                    show_symbol: true,
                    show_decimals: true,
                }
            })
            .filter(|c| !c.name.is_empty())
            .collect()
    }
}

/// The view shown when the application starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartupWindow {
    Activities,
    Wbs,
    Projects,
    Resources,
}
impl StartupWindow {
    pub const ALL: [Self; 4] = [Self::Activities, Self::Wbs, Self::Projects, Self::Resources];
    pub fn label(self) -> &'static str {
        match self {
            Self::Activities => "Activities",
            Self::Wbs => "WBS",
            Self::Projects => "Projects",
            Self::Resources => "Resources",
        }
    }
}

/// How activity code columns show an assigned code value (User Preferences,
/// Application, Codes).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CodeDisplay {
    /// The code value itself (`ELEC`).
    #[default]
    Value,
    /// The code value's description (`Electrical`).
    Description,
}

/// All user preferences that change how values are shown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Formats {
    pub units: UnitFormat,
    pub durations: UnitFormat,
    pub units_per_time: UnitsPerTime,
    pub dates: DateFormat,
    pub currency: CurrencyFormat,
    pub startup: StartupWindow,
    pub band_code: bool,
    pub band_name: bool,
    pub codes: CodeDisplay,
    /// Financial Period Data: the first and last day of the periods the
    /// session loads; `None` for both loads every period.
    pub fin_from: Option<NaiveDate>,
    pub fin_to: Option<NaiveDate>,
}
impl Default for Formats {
    /// Whole days (`5`), units in whole hours (`8`), and dates like
    /// `30-Oct-26`, as the reference starts (`crate::defaults`). The legacy
    /// defaults of tests show labels and a 24-hour time (`5d`, `8h`,
    /// `30-Oct-26 06:00`).
    fn default() -> Self {
        let reference = crate::defaults::reference();
        Self {
            units: UnitFormat {
                unit: TimeUnit::Hour,
                sub_unit: false,
                decimals: 0,
                show_label: !reference,
            },
            durations: UnitFormat {
                unit: TimeUnit::Day,
                sub_unit: false,
                decimals: 0,
                show_label: !reference,
            },
            units_per_time: UnitsPerTime::UnitsPerDuration,
            dates: DateFormat {
                order: DateOrder::DayMonthYear,
                four_digit_year: false,
                month_name: true,
                leading_zeros: true,
                separator: '-',
                time: if reference {
                    TimeDisplay::Hidden
                } else {
                    TimeDisplay::Hour24
                },
                show_minutes: true,
            },
            currency: CurrencyFormat::default(),
            startup: StartupWindow::Activities,
            band_code: true,
            band_name: true,
            codes: CodeDisplay::Value,
            fin_from: None,
            fin_to: None,
        }
    }
}

impl Formats {
    /// Hour-based durations with one decimal (`40.0h`), for tests that
    /// expect the older hour display.
    #[cfg(test)]
    pub fn hours() -> Self {
        Self {
            durations: UnitFormat {
                unit: TimeUnit::Hour,
                sub_unit: false,
                decimals: 1,
                show_label: true,
            },
            ..Self::default()
        }
    }

    pub fn duration(&self, minutes: i64, periods: Periods) -> String {
        self.durations.format(minutes, periods)
    }
    pub fn float(&self, minutes: Option<i64>, periods: Periods) -> String {
        minutes.map_or(String::new(), |m| self.duration(m, periods))
    }
    pub fn parse_duration(&self, text: &str, periods: Periods) -> Result<i64, String> {
        self.durations.parse(text, periods)
    }
    /// Work units held in hours, shown in the Units format.
    pub fn units(&self, hours: f64) -> String {
        let minutes = (hours * 60.).round() as i64;
        self.units.format(minutes, Periods::DEFAULT)
    }
    /// Units per hour shown as a percentage or as units per duration.
    pub fn units_per_time(&self, per_hour: f64) -> String {
        match self.units_per_time {
            UnitsPerTime::Percentage => {
                format!(
                    "{}%",
                    crate::records::format_number((per_hour * 100.).round())
                )
            }
            UnitsPerTime::UnitsPerDuration => {
                let per = self.durations.unit;
                let minutes = (per_hour * Periods::DEFAULT.minutes(per) as f64).round() as i64;
                format!(
                    "{}/{}",
                    self.units.format(minutes, Periods::DEFAULT),
                    per.suffix()
                )
            }
        }
    }
    pub fn cost(&self, value: f64) -> String {
        self.currency.format(value)
    }
    /// Model minutes relative to `origin` as text; empty for `None`.
    pub fn date(&self, origin: i64, value: Option<i64>) -> String {
        value
            .and_then(|v| to_datetime(origin, v))
            .map_or(String::new(), |d| self.dates.format(d))
    }
    /// Text to model minutes. Empty text is `None`. Without a typed time the
    /// time of `keep` (model minutes) is used.
    pub fn parse_date(
        &self,
        origin: i64,
        text: &str,
        keep: Option<i64>,
    ) -> Result<Option<i64>, String> {
        if text.trim().is_empty() {
            return Ok(None);
        }
        let keep = keep.and_then(|k| to_datetime(origin, k)).map(|d| d.time());
        self.dates
            .parse(text, keep)
            .map(|d| Some((d - epoch()).num_minutes() - origin * DAY))
            .ok_or_else(|| {
                format!(
                    "{} is not a valid date value. Use the form {}",
                    text.trim(),
                    self.date_example()
                )
            })
    }
    /// The current date format applied to 30 October 2026 13:30.
    pub fn date_example(&self) -> String {
        self.dates.format(
            NaiveDate::from_ymd_opt(2026, 10, 30)
                .unwrap()
                .and_hms_opt(13, 30, 0)
                .unwrap(),
        )
    }

    pub fn to_json(&self) -> String {
        let unit = |u: &UnitFormat| json!({"unit": u.unit.code(), "sub_unit": u.sub_unit, "decimals": u.decimals, "show_label": u.show_label});
        let d = &self.dates;
        let c = &self.currency;
        json!({
            "units": unit(&self.units),
            "durations": unit(&self.durations),
            "units_per_time": match self.units_per_time { UnitsPerTime::Percentage => "percentage", UnitsPerTime::UnitsPerDuration => "units_per_duration" },
            "dates": {
                "order": match d.order { DateOrder::MonthDayYear => "mdy", DateOrder::DayMonthYear => "dmy", DateOrder::YearMonthDay => "ymd" },
                "four_digit_year": d.four_digit_year,
                "month_name": d.month_name,
                "leading_zeros": d.leading_zeros,
                "separator": d.separator.to_string(),
                "time": match d.time { TimeDisplay::Hour12 => "12", TimeDisplay::Hour24 => "24", TimeDisplay::Hidden => "none" },
                "show_minutes": d.show_minutes,
            },
            "currency": {
                "name": c.name, "symbol": c.symbol, "decimals": c.decimals,
                "decimal_symbol": c.decimal_symbol.to_string(), "group_symbol": c.group_symbol.to_string(),
                "group_digits": c.group_digits, "positive": c.positive, "negative": c.negative,
                "show_symbol": c.show_symbol, "show_decimals": c.show_decimals,
            },
            "application": {
                "startup": self.startup.label(),
                "band_code": self.band_code,
                "band_name": self.band_name,
                "codes": match self.codes {
                    CodeDisplay::Value => "value",
                    CodeDisplay::Description => "description",
                },
                "fin_from": self.fin_from.map(|d| d.format("%Y-%m-%d").to_string()),
                "fin_to": self.fin_to.map(|d| d.format("%Y-%m-%d").to_string()),
            },
        })
        .to_string()
    }

    /// Strict reading of saved preferences: any missing, mistyped or out of
    /// range value rejects the whole record.
    pub fn from_json(text: &str) -> Result<Self, String> {
        let root: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let obj = |v: &Value, key: &str| -> Result<Value, String> {
            v.get(key).cloned().ok_or(format!("missing {key}"))
        };
        let boolean = |v: &Value, key: &str| -> Result<bool, String> {
            v.get(key)
                .and_then(Value::as_bool)
                .ok_or(format!("{key} must be true or false"))
        };
        let string = |v: &Value, key: &str| -> Result<String, String> {
            v.get(key)
                .and_then(Value::as_str)
                .map(str::to_string)
                .ok_or(format!("{key} must be text"))
        };
        let small = |v: &Value, key: &str, max: u8| -> Result<u8, String> {
            v.get(key)
                .and_then(Value::as_u64)
                .filter(|n| *n <= max as u64)
                .map(|n| n as u8)
                .ok_or(format!("{key} must be 0 to {max}"))
        };
        let one_char = |v: &Value, key: &str| -> Result<char, String> {
            let s = string(v, key)?;
            let mut chars = s.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => Ok(c),
                _ => Err(format!("{key} must be one character")),
            }
        };
        let unit = |v: &Value| -> Result<UnitFormat, String> {
            Ok(UnitFormat {
                unit: TimeUnit::from_code(&string(v, "unit")?).ok_or("unknown unit")?,
                sub_unit: boolean(v, "sub_unit")?,
                decimals: small(v, "decimals", UnitFormat::MAX_DECIMALS)?,
                show_label: boolean(v, "show_label")?,
            })
        };
        let d = obj(&root, "dates")?;
        let separator = one_char(&d, "separator")?;
        if !SEPARATORS.contains(&separator) {
            return Err("unknown date separator".into());
        }
        let dates = DateFormat {
            order: match string(&d, "order")?.as_str() {
                "mdy" => DateOrder::MonthDayYear,
                "dmy" => DateOrder::DayMonthYear,
                "ymd" => DateOrder::YearMonthDay,
                _ => return Err("unknown date order".into()),
            },
            four_digit_year: boolean(&d, "four_digit_year")?,
            month_name: boolean(&d, "month_name")?,
            leading_zeros: boolean(&d, "leading_zeros")?,
            separator,
            time: match string(&d, "time")?.as_str() {
                "12" => TimeDisplay::Hour12,
                "24" => TimeDisplay::Hour24,
                "none" => TimeDisplay::Hidden,
                _ => return Err("unknown time display".into()),
            },
            show_minutes: boolean(&d, "show_minutes")?,
        };
        let c = obj(&root, "currency")?;
        let symbol = string(&c, "symbol")?;
        let pattern = |key: &str| -> Result<String, String> {
            let s = string(&c, key)?;
            (s.contains("1.1") && s.chars().count() <= 12)
                .then_some(s)
                .ok_or(format!("{key} is not a currency pattern"))
        };
        let currency = CurrencyFormat {
            name: string(&c, "name")?,
            decimals: small(&c, "decimals", CurrencyFormat::MAX_DECIMALS)?,
            decimal_symbol: one_char(&c, "decimal_symbol")?,
            group_symbol: one_char(&c, "group_symbol")?,
            group_digits: small(&c, "group_digits", 9)?.max(1),
            positive: pattern("positive")?,
            negative: pattern("negative")?,
            show_symbol: boolean(&c, "show_symbol")?,
            show_decimals: boolean(&c, "show_decimals")?,
            symbol,
        };
        if currency.symbol.chars().count() > 8 || currency.name.chars().count() > 60 {
            return Err("currency symbol or name is too long".into());
        }
        let a = obj(&root, "application")?;
        let startup = string(&a, "startup")?;
        // Added later than the other settings: older records lack it.
        let codes = match a.get("codes") {
            None => CodeDisplay::Value,
            Some(v) => match v.as_str() {
                Some("value") => CodeDisplay::Value,
                Some("description") => CodeDisplay::Description,
                _ => return Err("codes must be value or description".into()),
            },
        };
        // Added later still: absent or null is "every period".
        let day = |key: &str| -> Result<Option<NaiveDate>, String> {
            match a.get(key) {
                None | Some(Value::Null) => Ok(None),
                Some(v) => v
                    .as_str()
                    .and_then(|t| NaiveDate::parse_from_str(t, "%Y-%m-%d").ok())
                    .map(Some)
                    .ok_or(format!("{key} must be a date")),
            }
        };
        let (fin_from, fin_to) = (day("fin_from")?, day("fin_to")?);
        if fin_from.zip(fin_to).is_some_and(|(a, b)| a > b) {
            return Err("fin_from is after fin_to".into());
        }
        Ok(Self {
            units: unit(&obj(&root, "units")?)?,
            durations: unit(&obj(&root, "durations")?)?,
            units_per_time: match string(&root, "units_per_time")?.as_str() {
                "percentage" => UnitsPerTime::Percentage,
                "units_per_duration" => UnitsPerTime::UnitsPerDuration,
                _ => return Err("unknown units per time format".into()),
            },
            dates,
            currency,
            startup: StartupWindow::ALL
                .into_iter()
                .find(|s| s.label() == startup)
                .ok_or("unknown startup window")?,
            band_code: boolean(&a, "band_code")?,
            band_name: boolean(&a, "band_name")?,
            codes,
            fin_from,
            fin_to,
        })
    }
}

fn epoch() -> NaiveDateTime {
    NaiveDate::from_ymd_opt(1899, 12, 30)
        .unwrap()
        .and_time(NaiveTime::MIN)
}
fn to_datetime(origin: i64, value: i64) -> Option<NaiveDateTime> {
    epoch().checked_add_signed(Duration::minutes(
        origin.checked_mul(DAY)?.checked_add(value)?,
    ))
}

thread_local! {
    static CURRENT: RefCell<Formats> = RefCell::new(Formats::default());
}
/// Make `formats` the ones used by the free functions on this thread.
pub fn install(formats: &Formats) {
    CURRENT.with(|c| {
        if *c.borrow() != *formats {
            *c.borrow_mut() = formats.clone();
        }
    });
}
pub fn current() -> Formats {
    CURRENT.with(|c| c.borrow().clone())
}
fn with<R>(f: impl FnOnce(&Formats) -> R) -> R {
    CURRENT.with(|c| f(&c.borrow()))
}

/// User Preferences > Application > Group and Sorting: whether group band
/// labels may show the ID or code, and the name or description.
pub fn band_parts() -> (bool, bool) {
    with(|f| (f.band_code, f.band_name))
}

/// How activity code columns show code values.
pub fn code_display() -> CodeDisplay {
    with(|f| f.codes)
}
/// A duration in the user's format.
pub fn duration(minutes: i64, periods: Periods) -> String {
    with(|f| f.duration(minutes, periods))
}
/// A float (or any optional duration); empty for `None`.
pub fn float(minutes: Option<i64>, periods: Periods) -> String {
    with(|f| f.float(minutes, periods))
}
pub fn parse_duration(text: &str, periods: Periods) -> Result<i64, String> {
    with(|f| f.parse_duration(text, periods))
}
/// Work units held in hours.
pub fn units(hours: f64) -> String {
    with(|f| f.units(hours))
}
pub fn units_per_time(per_hour: f64) -> String {
    with(|f| f.units_per_time(per_hour))
}
pub fn cost(value: f64) -> String {
    with(|f| f.cost(value))
}
/// An amount typed in the user's currency format (`CurrencyFormat::parse`).
pub fn parse_cost(text: &str) -> Option<f64> {
    with(|f| f.currency.parse(text))
}
pub fn date(origin: i64, value: Option<i64>) -> String {
    with(|f| f.date(origin, value))
}
/// A calendar day in the user's date format, without a time.
pub fn day(date: NaiveDate) -> String {
    with(|f| {
        let mut d = f.dates;
        d.time = TimeDisplay::Hidden;
        d.format(date.and_time(NaiveTime::MIN))
    })
}
/// A time of day in the user's time format: `2:05 PM` with 12-hour times,
/// else `14:05`.
pub fn time_of_day(time: NaiveTime) -> String {
    with(|f| match f.dates.time {
        TimeDisplay::Hour12 => {
            let (pm, hour) = time.hour12();
            format!(
                "{hour}:{:02} {}",
                time.minute(),
                if pm { "PM" } else { "AM" }
            )
        }
        _ => format!("{:02}:{:02}", time.hour(), time.minute()),
    })
}
/// The constraint marker after a constrained Start or Finish date (and the
/// one an earlier build used), what a shown date carries after it, and
/// date text without its marker: the shared field catalog's.
pub use openxer_xer::edit::fields::{strip_date_marker, DateMark};
#[cfg(test)]
pub use openxer_xer::edit::fields::{CONSTRAINT_MARK, OLD_CONSTRAINT_MARK};

/// A date with its marker: ` A` for an actual, ` *` for a constrained
/// date. Empty dates stay empty.
pub fn date_marked(origin: i64, value: Option<i64>, mark: DateMark) -> String {
    openxer_xer::edit::fields::marked(date(origin, value), mark)
}

/// A date with the actual marker ` A` when `actual`.
pub fn date_actual(origin: i64, value: Option<i64>, actual: bool) -> String {
    let mark = if actual {
        DateMark::Actual
    } else {
        DateMark::Plain
    };
    date_marked(origin, value, mark)
}

pub fn parse_date(origin: i64, text: &str, keep: Option<i64>) -> Result<Option<i64>, String> {
    with(|f| f.parse_date(origin, text, keep))
}
/// A calendar day typed in the user's date format; empty text is `None`.
pub fn parse_day(text: &str) -> Result<Option<NaiveDate>, String> {
    Ok(parse_date(0, text, None)?.map(|m| (epoch() + Duration::minutes(m)).date()))
}
/// A sunken edit box over a duration held in minutes, shown and typed in the
/// user's format. Commits on Enter or focus loss.
pub fn duration_editor(
    ui: &mut egui::Ui,
    enabled: bool,
    minutes: &mut i64,
    periods: Periods,
    range: std::ops::RangeInclusive<i64>,
) -> egui::Response {
    let formats = current();
    let shown = formats.clone();
    let width = ui.available_width().clamp(24., 96.);
    let min_m = *range.start();
    let max_m = *range.end();
    let (mut response, committed) = crate::dialog_kit::flow_parsed_edit(
        ui,
        ui.next_auto_id(),
        *minutes as f64,
        width,
        move |v| shown.duration(v.round() as i64, periods),
        move |text| {
            formats
                .parse_duration(text, periods)
                .ok()
                .map(|m| m.clamp(min_m, max_m) as f64)
        },
        enabled,
    );
    if let Some(v) = committed {
        let clamped = (v.round() as i64).clamp(min_m, max_m);
        if clamped != *minutes {
            *minutes = clamped;
            response.mark_changed();
        }
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_time_of_day_follows_the_time_format() {
        let before = current();
        let t = NaiveTime::from_hms_opt(14, 5, 0).unwrap();
        let mut f = Formats::default();
        f.dates.time = TimeDisplay::Hour12;
        install(&f);
        assert_eq!(time_of_day(t), "2:05 PM");
        f.dates.time = TimeDisplay::Hour24;
        install(&f);
        assert_eq!(time_of_day(t), "14:05");
        install(&before);
    }

    fn days(decimals: u8) -> Formats {
        let mut f = Formats::default();
        f.durations.decimals = decimals;
        f
    }

    #[test]
    fn durations_use_the_calendar_hours_per_day() {
        let f = Formats::default();
        let forty_hours = 40 * 60;
        let cases = [(8., "5d"), (10., "4d"), (23., "2d"), (24., "2d")];
        for (hours, expected) in cases {
            assert_eq!(
                f.duration(forty_hours, Periods::with_day_hours(hours)),
                expected,
                "{hours} h/day"
            );
        }
        let one = days(1);
        assert_eq!(
            one.duration(forty_hours, Periods::with_day_hours(23.)),
            "1.7d"
        );
        assert_eq!(
            one.duration(forty_hours, Periods::with_day_hours(24.)),
            "1.7d"
        );
        assert_eq!(
            days(2).duration(forty_hours, Periods::with_day_hours(23.)),
            "1.74d"
        );
        assert_eq!(f.duration(0, Periods::with_day_hours(10.)), "0d");
        // Half a day rounds away from zero.
        assert_eq!(f.duration(12 * 60, Periods::with_day_hours(8.)), "2d");
    }

    #[test]
    fn negative_float_and_zero() {
        let f = Formats::default();
        let p = Periods::with_day_hours(8.);
        assert_eq!(f.float(Some(-16 * 60), p), "-2d");
        assert_eq!(f.float(Some(-60), p), "0d", "rounds to zero without a sign");
        assert_eq!(
            days(1).float(Some(-12 * 60), Periods::with_day_hours(10.)),
            "-1.2d"
        );
        assert_eq!(f.float(None, p), "");
        assert_eq!(f.parse_duration("-2", p), Ok(-16 * 60));
        assert_eq!(
            f.parse_duration("-1.5d", Periods::with_day_hours(10.)),
            Ok(-15 * 60)
        );
    }

    #[test]
    fn other_units_labels_and_sub_units() {
        let p = Periods::with_day_hours(10.);
        let mut f = Formats::hours();
        assert_eq!(f.duration(150, p), "2.5h");
        f.durations = UnitFormat {
            unit: TimeUnit::Day,
            sub_unit: true,
            decimals: 0,
            show_label: true,
        };
        assert_eq!(f.duration(54 * 60, p), "5d 4h");
        assert_eq!(f.duration(4 * 60, p), "4h");
        assert_eq!(f.duration(50 * 60, p), "5d");
        assert_eq!(f.duration(-54 * 60, p), "-5d 4h");
        f.durations.sub_unit = false;
        f.durations.show_label = false;
        assert_eq!(f.duration(50 * 60, p), "5");
        f.durations.unit = TimeUnit::Week;
        f.durations.show_label = true;
        f.durations.decimals = 1;
        assert_eq!(f.duration(80 * 60, Periods::DEFAULT), "2.0w");
        f.durations.unit = TimeUnit::Month;
        assert_eq!(f.duration(172 * 60, Periods::DEFAULT), "1.0m");
        f.durations.unit = TimeUnit::Year;
        assert_eq!(f.duration(1000 * 60, Periods::DEFAULT), "0.5y");
        let units = Formats::default();
        assert_eq!(units.units(8.), "8h");
        assert_eq!(units.units_per_time(1.), "8h/d");
        let pct = Formats {
            units_per_time: UnitsPerTime::Percentage,
            ..Formats::default()
        };
        assert_eq!(pct.units_per_time(0.5), "50%");
    }

    #[test]
    fn parsing_durations_round_trips_through_each_calendar() {
        let f = Formats::default();
        for hours in [8., 10., 23., 24.] {
            let p = Periods::with_day_hours(hours);
            let per_day = (hours * 60.) as i64;
            assert_eq!(f.parse_duration("5", p), Ok(5 * per_day), "{hours}");
            assert_eq!(f.parse_duration("5d", p), Ok(5 * per_day));
            assert_eq!(f.parse_duration(" 5 d ", p), Ok(5 * per_day));
            assert_eq!(f.parse_duration("5d 4h", p), Ok(5 * per_day + 240));
            assert_eq!(f.parse_duration("40h", p), Ok(2400));
            assert_eq!(
                f.parse_duration(&f.duration(7 * per_day, p), p),
                Ok(7 * per_day)
            );
        }
        let p = Periods::DEFAULT;
        assert_eq!(f.parse_duration("1w", p), Ok(2400));
        assert_eq!(f.parse_duration("1m", p), Ok(172 * 60));
        assert_eq!(f.parse_duration("30min", p), Ok(30));
        assert!(f.parse_duration("abc", p).is_err());
        assert!(f.parse_duration("5x", p).is_err());
        assert!(f.parse_duration("", p).is_err());
        assert_eq!(Formats::hours().parse_duration("12.5", p), Ok(750));
    }

    fn at(y: i32, m: u32, d: u32, h: u32, min: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(h, min, 0)
            .unwrap()
    }

    #[test]
    fn dates_across_formats() {
        let moment = at(2026, 3, 5, 13, 7);
        let mut d = Formats::default().dates;
        assert_eq!(d.format(moment), "05-Mar-26 13:07");
        d.time = TimeDisplay::Hidden;
        assert_eq!(d.format(moment), "05-Mar-26");
        d.order = DateOrder::MonthDayYear;
        d.month_name = false;
        d.separator = '/';
        d.four_digit_year = true;
        d.time = TimeDisplay::Hour12;
        assert_eq!(d.format(moment), "03/05/2026 1:07 PM");
        d.leading_zeros = false;
        d.show_minutes = false;
        assert_eq!(d.format(moment), "3/5/2026 1 PM");
        d.order = DateOrder::YearMonthDay;
        d.separator = '.';
        d.time = TimeDisplay::Hour24;
        d.show_minutes = true;
        d.leading_zeros = true;
        assert_eq!(d.format(moment), "2026.03.05 13:07");
        assert_eq!(d.format(at(2026, 3, 5, 0, 0)), "2026.03.05 00:00");
        let mut midnight = d;
        midnight.time = TimeDisplay::Hour12;
        assert_eq!(
            midnight.format(at(2026, 3, 5, 0, 30)),
            "2026.03.05 12:30 AM"
        );
    }

    #[test]
    fn dates_parse_in_every_format_and_keep_time() {
        let moment = at(2026, 3, 5, 13, 7);
        for order in [
            DateOrder::MonthDayYear,
            DateOrder::DayMonthYear,
            DateOrder::YearMonthDay,
        ] {
            for month_name in [true, false] {
                for four in [true, false] {
                    for separator in SEPARATORS {
                        for (time, minutes) in [
                            (TimeDisplay::Hour24, true),
                            (TimeDisplay::Hour12, true),
                            (TimeDisplay::Hour12, false),
                        ] {
                            let d = DateFormat {
                                order,
                                four_digit_year: four,
                                month_name,
                                leading_zeros: true,
                                separator,
                                time,
                                show_minutes: minutes,
                            };
                            let expected = if minutes {
                                moment
                            } else {
                                at(2026, 3, 5, 13, 0)
                            };
                            let text = d.format(moment);
                            assert_eq!(d.parse(&text, None), Some(expected), "{text}");
                        }
                    }
                }
            }
        }
        let d = Formats::default().dates;
        let keep = NaiveTime::from_hms_opt(17, 0, 0);
        assert_eq!(d.parse("5-mar-2026", keep), Some(at(2026, 3, 5, 17, 0)));
        assert_eq!(
            d.parse("05-Mar-26 08:00 A", None),
            Some(at(2026, 3, 5, 8, 0))
        );
        assert_eq!(
            d.parse("2026-03-05 08:30", None),
            Some(at(2026, 3, 5, 8, 30))
        );
        assert_eq!(
            d.parse("05 March 26 1:30 pm", None),
            Some(at(2026, 3, 5, 13, 30))
        );
        assert_eq!(d.parse("31-Feb-26", None), None);
        assert_eq!(d.parse("05-Mar", None), None);
        assert_eq!(d.parse("05-Mar-26 25:00", None), None);
        let f = Formats::default();
        let origin = 46000;
        let minutes = f
            .parse_date(origin, "05-Mar-26 13:07", None)
            .unwrap()
            .unwrap();
        assert_eq!(f.date(origin, Some(minutes)), "05-Mar-26 13:07");
        assert_eq!(f.parse_date(origin, "  ", None), Ok(None));
        assert!(f
            .parse_date(origin, "someday", None)
            .unwrap_err()
            .contains("30-Oct-26 13:30"));
    }

    #[test]
    fn currency_symbol_decimals_and_grouping() {
        let c = CurrencyFormat::default();
        assert_eq!(c.format(1234567.5), "$1,234,567.50");
        assert_eq!(c.format(-12.345), "($12.35)");
        assert_eq!(c.format(-0.001), "$0.00");
        let mut euro = CurrencyFormat {
            symbol: "€".into(),
            decimal_symbol: ',',
            group_symbol: '.',
            positive: "1.1 #".into(),
            negative: "-1.1 #".into(),
            ..CurrencyFormat::default()
        };
        assert_eq!(euro.format(1234.5), "1.234,50 €");
        assert_eq!(euro.format(-1234.5), "-1.234,50 €");
        euro.show_decimals = false;
        euro.show_symbol = false;
        assert_eq!(euro.format(1234.5), "1.235");
    }

    #[test]
    fn preferences_round_trip_and_reject_invalid_data() {
        let mut f = Formats::default();
        f.durations.unit = TimeUnit::Week;
        f.durations.decimals = 2;
        f.dates.order = DateOrder::YearMonthDay;
        f.dates.separator = '/';
        f.dates.time = TimeDisplay::Hour12;
        f.currency.symbol = "€".into();
        f.startup = StartupWindow::Projects;
        f.band_code = false;
        f.fin_from = NaiveDate::from_ymd_opt(2026, 1, 1);
        f.fin_to = NaiveDate::from_ymd_opt(2026, 12, 31);
        assert_eq!(Formats::from_json(&f.to_json()), Ok(f.clone()));
        let mut older: Value = serde_json::from_str(&f.to_json()).unwrap();
        older["application"]
            .as_object_mut()
            .unwrap()
            .remove("fin_from");
        older["application"]
            .as_object_mut()
            .unwrap()
            .remove("fin_to");
        let older = Formats::from_json(&older.to_string()).unwrap();
        assert_eq!((older.fin_from, older.fin_to), (None, None));
        let good: Value = serde_json::from_str(&f.to_json()).unwrap();
        let broken = |path: &[&str], value: Value| {
            let mut v = good.clone();
            let mut node = &mut v;
            for key in &path[..path.len() - 1] {
                node = node.get_mut(*key).unwrap();
            }
            node[path[path.len() - 1]] = value;
            Formats::from_json(&v.to_string())
        };
        assert!(broken(&["durations", "decimals"], json!(9)).is_err());
        assert!(broken(&["durations", "unit"], json!("fortnight")).is_err());
        assert!(broken(&["dates", "separator"], json!("::")).is_err());
        assert!(broken(&["dates", "separator"], json!("x")).is_err());
        assert!(broken(&["dates", "time"], json!(24)).is_err());
        assert!(broken(&["dates", "month_name"], json!("yes")).is_err());
        assert!(broken(&["currency", "negative"], json!("-#")).is_err());
        assert!(broken(&["currency", "symbol"], json!("123456789")).is_err());
        assert!(broken(&["application", "startup"], json!("Gantt")).is_err());
        assert!(broken(&["application", "fin_from"], json!("soon")).is_err());
        assert!(broken(&["application", "fin_from"], json!("2027-01-01")).is_err());
        assert!(Formats::from_json("not json").is_err());
        assert!(Formats::from_json("{}").is_err());
    }

    #[test]
    fn every_shown_amount_reads_back_in_its_currency_format() {
        let dollar = CurrencyFormat::default();
        let euro = CurrencyFormat {
            name: "Euro".into(),
            symbol: "EUR".into(),
            decimal_symbol: ',',
            group_symbol: '.',
            positive: "1.1 #".into(),
            negative: "-1.1 #".into(),
            ..CurrencyFormat::default()
        };
        let minus_after_symbol = CurrencyFormat {
            negative: "#-1.1".into(),
            ..CurrencyFormat::default()
        };
        let swiss = CurrencyFormat {
            symbol: "CHF".into(),
            group_symbol: '\'',
            decimals: 3,
            negative: "# 1.1-".into(),
            ..CurrencyFormat::default()
        };
        let whole = CurrencyFormat {
            decimals: 0,
            group_digits: 4,
            ..CurrencyFormat::default()
        };
        for f in [dollar, euro, minus_after_symbol, swiss, whole] {
            for v in [
                0.,
                0.5,
                1.,
                -1.,
                12.34,
                -12.34,
                85.5,
                999.999,
                1234.5678,
                -25_000.,
                1_000_000.01,
                -98_765_432.1,
            ] {
                let shown = f.format(v);
                let back = f
                    .parse(&shown)
                    .unwrap_or_else(|| panic!("{} reads {shown:?}", f.name));
                assert_eq!(f.format(back), shown, "{}: {v}", f.name);
                let decimals = f.decimals as i32;
                let rounded = (v * 10f64.powi(decimals)).round() / 10f64.powi(decimals);
                assert!(
                    (back - rounded).abs() < 1e-9,
                    "{}: {shown:?} read as {back}",
                    f.name
                );
            }
        }
        // The audit's cases: decimal comma, and a minus after the symbol.
        let euro = CurrencyFormat {
            decimal_symbol: ',',
            group_symbol: '.',
            ..CurrencyFormat::default()
        };
        assert_eq!(euro.parse("25.000,00"), Some(25_000.));
        assert_eq!(euro.parse("85,50"), Some(85.5));
        assert_eq!(CurrencyFormat::default().parse("$-500.00"), Some(-500.));
        assert_eq!(CurrencyFormat::default().parse("($500.00)"), Some(-500.));
        assert_eq!(CurrencyFormat::default().parse("1.2.3"), None);
        assert_eq!(CurrencyFormat::default().parse("abc"), None);
        assert_eq!(CurrencyFormat::default().parse(""), None);
    }
}
