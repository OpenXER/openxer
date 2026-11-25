//! Relative date values for filters: a moving anchor (data date, current
//! date, project start or finish, start of this week or month) plus or minus
//! a number of days, weeks or months. They are stored as text such as
//! `DD + 3w` and resolved each time the filter runs, so a lookahead filter
//! follows the data date.
use super::*;
use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime};

/// What a relative date counts from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anchor {
    DataDate,
    CurrentDate,
    ProjectStart,
    ProjectFinish,
    StartOfWeek,
    StartOfMonth,
}

impl Anchor {
    pub const ALL: [Anchor; 6] = [
        Anchor::DataDate,
        Anchor::CurrentDate,
        Anchor::ProjectStart,
        Anchor::ProjectFinish,
        Anchor::StartOfWeek,
        Anchor::StartOfMonth,
    ];
    /// The short form stored in a filter value.
    pub fn code(self) -> &'static str {
        match self {
            Anchor::DataDate => "DD",
            Anchor::CurrentDate => "CD",
            Anchor::ProjectStart => "PS",
            Anchor::ProjectFinish => "PF",
            Anchor::StartOfWeek => "SW",
            Anchor::StartOfMonth => "SM",
        }
    }
    /// The name offered in the Value drop-down.
    pub fn caption(self) -> &'static str {
        match self {
            Anchor::DataDate => "Data Date",
            Anchor::CurrentDate => "Current Date",
            Anchor::ProjectStart => "Project Start",
            Anchor::ProjectFinish => "Project Finish",
            Anchor::StartOfWeek => "Start of This Week",
            Anchor::StartOfMonth => "Start of This Month",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    Days,
    Weeks,
    Months,
}

/// A parsed relative date.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Relative {
    pub anchor: Anchor,
    /// Signed count of `unit`.
    pub offset: i64,
    pub unit: Unit,
}

impl Relative {
    /// The canonical text (`DD`, `DD + 3w`, `CD - 10d`).
    pub fn text(&self) -> String {
        if self.offset == 0 {
            return self.anchor.code().into();
        }
        let unit = match self.unit {
            Unit::Days => 'd',
            Unit::Weeks => 'w',
            Unit::Months => 'm',
        };
        let sign = if self.offset < 0 { '-' } else { '+' };
        format!(
            "{} {sign} {}{unit}",
            self.anchor.code(),
            self.offset.unsigned_abs()
        )
    }
}

/// Read a relative date: an anchor code or name, optionally followed by
/// `+` or `-`, a whole number and a unit (d, w or m; days when left out).
/// Anything else is `None`, so ordinary dates fall through to the date
/// parser.
pub fn parse(text: &str) -> Option<Relative> {
    let t = text.trim();
    let lower = t.to_lowercase();
    let (anchor, rest) = Anchor::ALL.iter().find_map(|a| {
        [a.code().to_lowercase(), a.caption().to_lowercase()]
            .into_iter()
            .chain(match a {
                Anchor::StartOfWeek => Some("start of week".to_string()),
                Anchor::StartOfMonth => Some("start of month".to_string()),
                _ => None,
            })
            .filter(|name| lower.starts_with(name.as_str()))
            .max_by_key(|name| name.len())
            .map(|name| (*a, lower[name.len()..].trim().to_string()))
    })?;
    if rest.is_empty() {
        return Some(Relative {
            anchor,
            offset: 0,
            unit: Unit::Days,
        });
    }
    let (sign, rest) = match rest.chars().next()? {
        '+' => (1, rest[1..].trim()),
        '-' => (-1, rest[1..].trim()),
        _ => return None,
    };
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() || digits.len() > 5 {
        return None;
    }
    let count: i64 = digits.parse().ok()?;
    let unit = match rest[digits.len()..].trim() {
        "" | "d" | "day" | "days" => Unit::Days,
        "w" | "wk" | "week" | "weeks" => Unit::Weeks,
        "m" | "mo" | "month" | "months" => Unit::Months,
        _ => return None,
    };
    Some(Relative {
        anchor,
        offset: sign * count,
        unit,
    })
}

fn epoch() -> NaiveDateTime {
    NaiveDate::from_ymd_opt(1899, 12, 30)
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .expect("valid epoch")
}

fn to_datetime(origin: i64, minute: i64) -> Option<NaiveDateTime> {
    epoch().checked_add_signed(Duration::minutes(
        origin.checked_mul(DAY)?.checked_add(minute)?,
    ))
}

fn to_minute(origin: i64, d: NaiveDateTime) -> i64 {
    (d - epoch()).num_minutes() - origin * DAY
}

/// Now, in the project's minutes.
pub fn now_minute(origin: i64) -> i64 {
    to_minute(origin, chrono::Local::now().naive_local())
}

/// The minute a relative date stands for in project `p` at `now` (project
/// minutes); `None` when the anchor has no date (no data date yet).
pub fn resolve_at(p: &ProjectData, r: &Relative, now: i64) -> Option<i64> {
    let origin = p.model.origin_serial;
    let midnight = |m: i64| m.div_euclid(DAY) * DAY;
    let base = match r.anchor {
        Anchor::DataDate => p.model.project.data_date?,
        Anchor::CurrentDate => now,
        Anchor::ProjectStart => p.model.project.plan_start.or(p.model.project.data_date)?,
        Anchor::ProjectFinish => p.model.project.scd_end.or(p.model.project.plan_end)?,
        Anchor::StartOfWeek => {
            let d = to_datetime(origin, now)?;
            // Weeks start on Sunday, as the timescale's default.
            let back = d.weekday().num_days_from_sunday() as i64;
            midnight(now) - back * DAY
        }
        Anchor::StartOfMonth => {
            let d = to_datetime(origin, now)?;
            midnight(now) - (d.day() as i64 - 1) * DAY
        }
    };
    match r.unit {
        Unit::Days => base.checked_add(r.offset.checked_mul(DAY)?),
        Unit::Weeks => base.checked_add(r.offset.checked_mul(7 * DAY)?),
        Unit::Months => {
            let d = to_datetime(origin, base)?;
            let shifted = if r.offset >= 0 {
                d.checked_add_months(chrono::Months::new(r.offset as u32))?
            } else {
                d.checked_sub_months(chrono::Months::new(r.offset.unsigned_abs() as u32))?
            };
            Some(to_minute(origin, shifted))
        }
    }
}
