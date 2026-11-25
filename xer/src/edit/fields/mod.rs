//! The activity field catalog shared by the desktop's activity table and the
//! command line: which fields an activity has (built-ins, one per activity
//! code type and one per activity user defined field), what each holds
//! (`Kind`), how its value is read (`value`) and how typed text is written
//! back through the shared setters (`write`). Global Change, filters and
//! the table's in-place editors all go through it, so a field reads and
//! writes the same wherever it is used.
//!
//! Text is shown and read in a [`Formats`]: the desktop passes the user's
//! display preferences, the command line [`Plain`] (hours, `YYYY-MM-DD
//! HH:MM`, plain numbers). The desktop's table adds its own columns on top
//! (baselines, earned value and the other derived columns, notebook topics,
//! relationship details); those stay in the desktop.
mod catalog;
mod criteria;
mod index;
mod write;

pub use catalog::{
    builtin, catalog, code_types, find, udf_types, Editor, Field, FieldDef, Source, UdfType,
};
pub use criteria::{holds, is_under, under_applies, Op, Test};
pub use index::{value, CodeValue, Index, WbsIndex};
pub use write::{
    assigned_resources, day_edge, edit_for, finish_type, lock_of, parse_field_date, write,
};

use crate::edit::records::format_number;
use crate::ProjectData;
use openxer_engine::model::{Activity, CstrType};

/// What a field holds; decides formatting, parsing, filtering and totals.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Text,
    Date,
    Duration,
    Percent,
    Units,
    Cost,
    Number,
    Flag,
}

/// What a shown date carries after it: nothing, the actual marker ` A`, or
/// the constraint marker. An actual date never also shows the constraint
/// marker.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum DateMark {
    #[default]
    Plain,
    Actual,
    Constrained,
}

/// The marker after a constrained Start or Finish date. It follows a space,
/// as the actual marker ` A` does, so the two read apart.
pub const CONSTRAINT_MARK: char = '*';
/// The double dagger an earlier build used as the constraint marker. Typed
/// and pasted text that still carries it parses.
pub const OLD_CONSTRAINT_MARK: char = '\u{2021}';

/// Date text with its marker: ` A` for an actual, ` *` for a constrained
/// date. Empty text stays empty.
pub fn marked(text: String, mark: DateMark) -> String {
    if text.is_empty() {
        return text;
    }
    match mark {
        DateMark::Plain => text,
        DateMark::Actual => format!("{text} A"),
        DateMark::Constrained => format!("{text} {CONSTRAINT_MARK}"),
    }
}

/// Date text without a trailing display marker: the actual ` A`, the
/// constraint `*`, or the double dagger an earlier build showed (with or
/// without a space before it). Markers are display only and never stored.
pub fn strip_date_marker(text: &str) -> &str {
    let t = text.trim();
    t.strip_suffix(" A")
        .or_else(|| t.strip_suffix(CONSTRAINT_MARK))
        .or_else(|| t.strip_suffix(OLD_CONSTRAINT_MARK))
        .unwrap_or(t)
        .trim_end()
}

/// A typed field value.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Empty,
    Text(String),
    /// Minutes from the project origin; `mark` says whether the shown text
    /// carries the actual or the constraint marker (Start and Finish only).
    Date {
        minute: i64,
        mark: DateMark,
    },
    /// Working minutes.
    Duration(i64),
    Percent(f64),
    /// Hours of work or quantity.
    Units(f64),
    Cost(f64),
    Number(f64),
    Flag(bool),
}

impl Value {
    /// A plain date, or empty.
    pub fn date(minute: Option<i64>) -> Self {
        minute.map_or(Value::Empty, |minute| Value::Date {
            minute,
            mark: DateMark::Plain,
        })
    }
    /// Text, or empty for an empty string.
    pub fn text(s: impl Into<String>) -> Self {
        let s = s.into();
        if s.is_empty() {
            Value::Empty
        } else {
            Value::Text(s)
        }
    }
    /// A number for range comparisons and totals, when the value has one.
    pub fn number(&self) -> Option<f64> {
        match self {
            Value::Date { minute, .. } => Some(*minute as f64),
            Value::Duration(m) => Some(*m as f64),
            Value::Percent(v) | Value::Units(v) | Value::Cost(v) | Value::Number(v) => Some(*v),
            Value::Flag(b) => Some(if *b { 1. } else { 0. }),
            Value::Empty | Value::Text(_) => None,
        }
    }
}

/// Sort order of two values: blanks first, numbers by value, text without case.
pub fn compare(a: &Value, b: &Value) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (a, b) {
        (Value::Empty, Value::Empty) => Ordering::Equal,
        (Value::Empty, _) => Ordering::Less,
        (_, Value::Empty) => Ordering::Greater,
        (Value::Text(x), Value::Text(y)) => x.to_lowercase().cmp(&y.to_lowercase()).then(x.cmp(y)),
        _ => match (a.number(), b.number()) {
            (Some(x), Some(y)) => x.total_cmp(&y),
            _ => format!("{a:?}").cmp(&format!("{b:?}")),
        },
    }
}

/// How durations, dates, units and costs are shown and read. `activity`
/// picks the calendar a duration is counted on (`None`: the project's).
pub trait Formats {
    fn duration(&self, p: &ProjectData, activity: Option<usize>, minutes: i64) -> String;
    fn parse_duration(
        &self,
        p: &ProjectData,
        activity: Option<usize>,
        text: &str,
    ) -> Result<i64, String>;
    /// A date without a marker.
    fn date(&self, origin: i64, minute: i64) -> String;
    /// Read a date; `keep` is the time of day kept when the text has none.
    fn parse_date(&self, origin: i64, text: &str, keep: Option<i64>)
        -> Result<Option<i64>, String>;
    fn units(&self, hours: f64) -> String;
    /// Work units as hours.
    fn parse_units(&self, text: &str) -> Result<f64, String>;
    fn cost(&self, value: f64) -> String;
}

/// The command line's formats: durations in hours (`40h`), dates as
/// `YYYY-MM-DD HH:MM`, units and costs as plain numbers. Typed durations
/// are hours, with or without the `h`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Plain;

impl Formats for Plain {
    fn duration(&self, _: &ProjectData, _: Option<usize>, minutes: i64) -> String {
        format!("{}h", format_number(minutes as f64 / 60.))
    }
    fn parse_duration(&self, _: &ProjectData, _: Option<usize>, text: &str) -> Result<i64, String> {
        let t = text.trim();
        t.trim_end_matches(['h', 'H'])
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .map(|v| (v * 60.).round() as i64)
            .ok_or_else(|| format!("{t} is not a valid duration in hours"))
    }
    fn date(&self, origin: i64, minute: i64) -> String {
        crate::date_string(origin, Some(minute))
    }
    fn parse_date(
        &self,
        origin: i64,
        text: &str,
        _keep: Option<i64>,
    ) -> Result<Option<i64>, String> {
        crate::parse_relative(origin, text)
    }
    fn units(&self, hours: f64) -> String {
        format_number(hours)
    }
    fn parse_units(&self, text: &str) -> Result<f64, String> {
        let t = text.trim();
        t.trim_end_matches(['h', 'H'])
            .replace(',', "")
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .ok_or_else(|| format!("{t:?} is not a number."))
    }
    fn cost(&self, value: f64) -> String {
        format_number(value)
    }
}

/// Text for a value in `f`. Every table cell, band total, filter comparison,
/// find/replace match and Global Change report line goes through here.
/// `activity` picks the calendar for durations.
pub fn display(p: &ProjectData, f: &dyn Formats, activity: Option<usize>, value: &Value) -> String {
    let origin = p.model.origin_serial;
    match value {
        Value::Empty => String::new(),
        Value::Text(s) => s.clone(),
        Value::Date { minute, mark } => marked(f.date(origin, *minute), *mark),
        Value::Duration(minutes) => f.duration(p, activity, *minutes),
        Value::Percent(v) => format!("{}%", format_number((v * 100.).round() / 100.)),
        Value::Units(v) => f.units(*v),
        Value::Cost(v) => f.cost(*v),
        Value::Number(v) => format_number(*v),
        Value::Flag(b) => if *b { "Yes" } else { "No" }.into(),
    }
}

/// Read typed text for a field kind: the inverse of [`display`], also used
/// for filter values. Dates also accept 2026-11-04 08:00 and 04-Nov-26;
/// percents are 0 to 100.
pub fn parse_input(
    p: &ProjectData,
    f: &dyn Formats,
    activity: Option<usize>,
    kind: Kind,
    text: &str,
) -> Result<Value, String> {
    let origin = p.model.origin_serial;
    let t = text.trim();
    if t.is_empty() {
        return Ok(Value::Empty);
    }
    let number = |s: &str| -> Result<f64, String> {
        s.trim()
            .trim_end_matches(['h', 'H', '%'])
            .trim_start_matches(|c: char| !c.is_ascii_digit() && c != '-' && c != '.')
            .replace(',', "")
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .ok_or_else(|| format!("{t:?} is not a number."))
    };
    Ok(match kind {
        Kind::Text => Value::Text(t.into()),
        Kind::Date => {
            let bare = strip_date_marker(t);
            let minute = f
                .parse_date(origin, bare, None)
                .ok()
                .flatten()
                .or_else(|| crate::parse_relative(origin, bare).ok().flatten())
                .or_else(|| {
                    ["%d-%b-%y %H:%M", "%d-%b-%Y %H:%M"]
                        .iter()
                        .find_map(|f| chrono::NaiveDateTime::parse_from_str(bare, f).ok())
                        .or_else(|| {
                            ["%d-%b-%y", "%d-%b-%Y"].iter().find_map(|f| {
                                chrono::NaiveDate::parse_from_str(bare, f)
                                    .ok()
                                    .and_then(|d| d.and_hms_opt(0, 0, 0))
                            })
                        })
                        .and_then(|d| {
                            crate::parse_relative(origin, &d.format("%Y-%m-%d %H:%M").to_string())
                                .ok()
                                .flatten()
                        })
                })
                .ok_or("Use a date like 2026-11-04, 2026-11-04 08:00 or 04-Nov-26.")?;
            Value::Date {
                minute,
                mark: DateMark::Plain,
            }
        }
        Kind::Duration => Value::Duration(f.parse_duration(p, activity, t)?),
        Kind::Percent => Value::Percent(number(t)?),
        Kind::Units => Value::Units(f.parse_units(t).or_else(|_| number(t))?),
        Kind::Cost => Value::Cost(number(t)?),
        Kind::Number => Value::Number(number(t)?),
        Kind::Flag => match t.to_lowercase().as_str() {
            "yes" | "y" | "true" | "1" => Value::Flag(true),
            "no" | "n" | "false" | "0" => Value::Flag(false),
            _ => return Err("Use Yes or No.".into()),
        },
    })
}

/// Text for a value as an in-place editor shows it and reads it back:
/// percents without the sign, dates as `YYYY-MM-DD HH:MM`.
pub fn input_text(p: &ProjectData, f: &dyn Formats, i: usize, v: &Value) -> String {
    match v {
        Value::Empty => String::new(),
        Value::Percent(x) => format_number(*x),
        Value::Date { minute, .. } => crate::date_string(p.model.origin_serial, Some(*minute)),
        other => display(p, f, Some(i), other),
    }
}

/// The end of an activity a constraint applies to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum End {
    Start,
    Finish,
}

/// Which end a constraint type applies to; `None` for no constraint and for
/// As Late As Possible, which carries no date.
pub fn end_of(c: CstrType) -> Option<End> {
    match c {
        CstrType::StartOn
        | CstrType::StartOnBefore
        | CstrType::StartOnAfter
        | CstrType::MandatoryStart => Some(End::Start),
        CstrType::FinishOn
        | CstrType::FinishOnBefore
        | CstrType::FinishOnAfter
        | CstrType::MandatoryFinish => Some(End::Finish),
        CstrType::None | CstrType::Alap => None,
    }
}

/// The activity's constraints that apply to `end`, primary first, each with
/// whether it is the secondary and its date. A dated type without a date
/// is not shown as a constraint.
pub fn constraints_on(a: &Activity, end: End) -> Vec<(bool, CstrType, i64)> {
    [(false, a.cstr, a.cstr_date), (true, a.cstr2, a.cstr_date2)]
        .into_iter()
        .filter(|(_, c, _)| end_of(*c) == Some(end))
        .filter_map(|(secondary, c, d)| d.map(|d| (secondary, c, d)))
        .collect()
}

/// The marker for the shown date at `end`: actual wins, then constrained.
pub fn mark(a: &Activity, end: End) -> DateMark {
    let actual = match end {
        End::Start => a.act_start.is_some(),
        End::Finish => a.act_finish.is_some(),
    };
    if actual {
        DateMark::Actual
    } else if constraints_on(a, end).is_empty() {
        DateMark::Plain
    } else {
        DateMark::Constrained
    }
}

/// The Start field's value: the shown start with its marker.
pub fn start_value(a: &Activity, leveled: bool) -> Value {
    crate::ev::displayed_start(a, leveled).map_or(Value::Empty, |minute| Value::Date {
        minute,
        mark: mark(a, End::Start),
    })
}

/// The Finish field's value: the shown finish with its marker.
pub fn finish_value(a: &Activity, leveled: bool) -> Value {
    crate::ev::displayed_finish(a, leveled).map_or(Value::Empty, |minute| Value::Date {
        minute,
        mark: mark(a, End::Finish),
    })
}

#[cfg(test)]
pub(crate) mod tests;
