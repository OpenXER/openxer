//! Group Interval: grouping bands that span a period of dates or a step of
//! numbers instead of one exact value.
use super::columns::{self, Kind, Value};
use super::*;
use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime};

/// The date intervals, as stored and as the Group Interval cell offers them.
pub const DATE_INTERVALS: [(&str, &str); 5] = [
    ("day", "Day"),
    ("week", "Week"),
    ("month", "Month"),
    ("quarter", "Quarter"),
    ("year", "Year"),
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Interval {
    /// Exact values (dates by day).
    None,
    Day,
    Week,
    Month,
    Quarter,
    Year,
    /// A positive step in the column's own value units.
    Step(f64),
}

/// Read a level's stored interval for a column kind; anything unusable
/// groups by exact value.
pub fn parse(p: &ProjectData, kind: Kind, text: &str) -> Interval {
    let t = text.trim();
    if t.is_empty() {
        return Interval::None;
    }
    match kind {
        Kind::Date => match t.to_lowercase().as_str() {
            "day" => Interval::Day,
            "week" => Interval::Week,
            "month" => Interval::Month,
            "quarter" => Interval::Quarter,
            "year" => Interval::Year,
            _ => Interval::None,
        },
        Kind::Duration | Kind::Percent | Kind::Units | Kind::Cost | Kind::Number => {
            columns::parse_input(p, None, kind, t)
                .ok()
                .and_then(|v| v.number())
                .filter(|s| s.is_finite() && *s > 0.)
                .map_or(Interval::None, Interval::Step)
        }
        Kind::Text | Kind::Flag => Interval::None,
    }
}

/// Is `text` a usable interval for `kind` (empty is)?
pub fn check(p: &ProjectData, kind: Kind, text: &str) -> Result<(), String> {
    if text.trim().is_empty() || parse(p, kind, text) != Interval::None {
        return Ok(());
    }
    Err(match kind {
        Kind::Date => "Use Day, Week, Month, Quarter or Year.".into(),
        Kind::Text | Kind::Flag => "Only date and number columns take a group interval.".into(),
        _ => format!("{text:?} is not a positive step."),
    })
}

fn epoch() -> NaiveDateTime {
    NaiveDate::from_ymd_opt(1899, 12, 30)
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .expect("valid epoch")
}

fn to_date(origin: i64, minute: i64) -> Option<NaiveDate> {
    epoch()
        .checked_add_signed(Duration::minutes(
            origin.checked_mul(DAY)?.checked_add(minute)?,
        ))
        .map(|d| d.date())
}

fn to_minute(origin: i64, d: NaiveDate) -> Option<i64> {
    Some((d.and_hms_opt(0, 0, 0)? - epoch()).num_minutes() - origin * DAY)
}

/// The band a value falls in: (the band's start value, its caption). The
/// start value orders the bands.
pub fn bucket(
    p: &ProjectData,
    value: &Value,
    interval: Interval,
    activity: Option<usize>,
) -> (Value, String) {
    let origin = p.model.origin_serial;
    if let Value::Date { minute, .. } = *value {
        let day = minute.div_euclid(DAY) * DAY;
        let date = to_date(origin, day);
        let start = date.and_then(|d| match interval {
            Interval::Week => {
                // Weeks start on Sunday, as the timescale's default.
                let back = d.weekday().num_days_from_sunday() as i64;
                Some(d - Duration::days(back))
            }
            Interval::Month => d.with_day(1),
            Interval::Quarter => NaiveDate::from_ymd_opt(d.year(), (d.month0() / 3) * 3 + 1, 1),
            Interval::Year => NaiveDate::from_ymd_opt(d.year(), 1, 1),
            _ => Some(d),
        });
        let grouped = Value::Date {
            minute: start.and_then(|s| to_minute(origin, s)).unwrap_or(day),
            mark: crate::format::DateMark::Plain,
        };
        let label = match (interval, start) {
            (Interval::Week, Some(s)) => format!("Week of {}", s.format("%d-%b-%y")),
            (Interval::Month, Some(s)) => s.format("%B %Y").to_string(),
            (Interval::Quarter, Some(s)) => format!("Q{} {}", s.month0() / 3 + 1, s.year()),
            (Interval::Year, Some(s)) => s.year().to_string(),
            _ => columns::display(p, activity, &grouped),
        };
        return (grouped, label);
    }
    if let (Interval::Step(step), Some(n)) = (interval, value.number()) {
        let low = (n / step).floor() * step;
        let make = |v: f64| match value {
            Value::Duration(_) => Value::Duration(v.round() as i64),
            Value::Percent(_) => Value::Percent(v),
            Value::Units(_) => Value::Units(v),
            Value::Cost(_) => Value::Cost(v),
            _ => Value::Number(v),
        };
        let grouped = make(low);
        let label = format!(
            "{} - {}",
            columns::display(p, None, &grouped),
            columns::display(p, None, &make(low + step))
        );
        return (grouped, label);
    }
    (value.clone(), columns::display(p, activity, value))
}
