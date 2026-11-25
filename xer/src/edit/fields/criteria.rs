//! Comparing a field's value with a criterion: the "Is" choices of a
//! filter row or a Global Change If row, and "is under" for the fields
//! that form a hierarchy (WBS and activity codes).
use super::{compare, Field, Index, Kind, Source, Value};
use crate::ProjectData;
use openxer_engine::calendar::DAY;
use std::cmp::Ordering;

/// The "Is" choices of a criterion row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Equals,
    NotEqual,
    Contains,
    NotContains,
    StartsWith,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
    Within,
    NotWithin,
    /// The value or one below it in the field's hierarchy (WBS and
    /// activity codes; see [`under_applies`]). Not in [`Op::ALL`]: only the
    /// fields that form a hierarchy offer it.
    IsUnder,
}

impl Op {
    /// The choices every field offers.
    pub const ALL: [Op; 11] = [
        Op::Equals,
        Op::NotEqual,
        Op::Contains,
        Op::NotContains,
        Op::StartsWith,
        Op::Less,
        Op::LessOrEqual,
        Op::Greater,
        Op::GreaterOrEqual,
        Op::Within,
        Op::NotWithin,
    ];
    pub fn caption(self) -> &'static str {
        match self {
            Op::Equals => "equals",
            Op::NotEqual => "is not equal to",
            Op::Contains => "contains",
            Op::NotContains => "does not contain",
            Op::StartsWith => "starts with",
            Op::Less => "is less than",
            Op::LessOrEqual => "is less than or equals",
            Op::Greater => "is greater than",
            Op::GreaterOrEqual => "is greater than or equals",
            Op::Within => "is within range of",
            Op::NotWithin => "is not within range of",
            Op::IsUnder => "is under",
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Op::Equals => "eq",
            Op::NotEqual => "ne",
            Op::Contains => "contains",
            Op::NotContains => "not_contains",
            Op::StartsWith => "starts_with",
            Op::Less => "lt",
            Op::LessOrEqual => "le",
            Op::Greater => "gt",
            Op::GreaterOrEqual => "ge",
            Op::Within => "within",
            Op::NotWithin => "not_within",
            Op::IsUnder => "under",
        }
    }
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .chain([Op::IsUnder])
            .find(|o| o.key() == key)
    }
    pub fn uses_high(self) -> bool {
        matches!(self, Op::Within | Op::NotWithin)
    }
}

/// A criterion row ready to compare: its choice, the kind of the field it
/// tests, its value trimmed and lowercased, and its high value as typed.
#[derive(Clone, Copy, Debug)]
pub struct Test<'a> {
    pub op: Op,
    pub kind: Kind,
    pub wanted: &'a str,
    pub high: &'a str,
}

/// Does a criterion hold for `value`? `text` gives the value as shown,
/// lowercased; `target` reads the criterion's value (`false`) or high
/// value (`true`) as a value of the field's kind; `under` answers "is
/// under". Text compares as shown, without case; other kinds by value,
/// dates by day, so "equals 2026-11-04" holds at any time that day.
pub fn holds(
    t: &Test,
    value: &Value,
    text: &dyn Fn() -> String,
    target: &dyn Fn(bool) -> Option<Value>,
    under: &dyn Fn() -> bool,
) -> bool {
    let (op, kind, wanted, high_text) = (t.op, t.kind, t.wanted, t.high);
    let text_cell = std::cell::OnceCell::new();
    let text = || -> &String { text_cell.get_or_init(text) };
    let day = |v: &Value| match v {
        Value::Date { minute, .. } => Value::Number(minute.div_euclid(DAY) as f64),
        other => other.clone(),
    };
    let cmp = |high: bool| -> Option<Ordering> {
        let target = target(high)?;
        if kind == Kind::Text {
            let other = if high { high_text } else { wanted };
            return Some(text().cmp(&other.trim().to_lowercase()));
        }
        if matches!(value, Value::Empty) || matches!(target, Value::Empty) {
            return None;
        }
        Some(compare(&day(value), &day(&target)))
    };
    let equals = || match kind {
        Kind::Text => *text() == wanted,
        _ if wanted.is_empty() => matches!(value, Value::Empty),
        _ => cmp(false) == Some(Ordering::Equal) || *text() == wanted,
    };
    use Ordering::*;
    match op {
        Op::Equals => equals(),
        Op::NotEqual => !equals(),
        Op::Contains => text().contains(wanted),
        Op::NotContains => !text().contains(wanted),
        Op::StartsWith => text().starts_with(wanted),
        Op::Less => cmp(false) == Some(Less),
        Op::LessOrEqual => matches!(cmp(false), Some(Less | Equal)),
        Op::Greater => cmp(false) == Some(Greater),
        Op::GreaterOrEqual => matches!(cmp(false), Some(Greater | Equal)),
        Op::Within | Op::NotWithin => {
            let inside = matches!(cmp(false), Some(Greater | Equal))
                && matches!(cmp(true), Some(Less | Equal));
            inside == (op == Op::Within)
        }
        Op::IsUnder => under(),
    }
}

/// Does "is under" apply to fields from `source`? WBS and activity codes
/// form a hierarchy; nothing else does.
pub fn under_applies(source: Source) -> bool {
    matches!(source, Source::Field(Field::Wbs) | Source::Code(_))
}

/// Is activity `i`'s value of `source` the element named `wanted`, or below
/// it? A WBS element is named by its full code (as the WBS field shows it)
/// or its own short code; a code value by its short name. Case is ignored.
/// False for a field that is not a hierarchy.
pub fn is_under(p: &ProjectData, ix: &Index, source: Source, i: usize, wanted: &str) -> bool {
    let wanted = wanted.trim();
    if wanted.is_empty() {
        return false;
    }
    let Some(a) = p.model.activities.get(i) else {
        return false;
    };
    match source {
        Source::Field(Field::Wbs) => {
            let Some(mut at) = p.details.get(&a.task_id).map(|d| d.wbs) else {
                return false;
            };
            let w = ix.wbs(p);
            // Up the tree, at most once per element.
            for _ in 0..=w.tree.parent.len() {
                let full = w
                    .codes
                    .get(&at)
                    .is_some_and(|c| c.eq_ignore_ascii_case(wanted));
                let short = w
                    .short_codes
                    .get(&at)
                    .is_some_and(|c| c.eq_ignore_ascii_case(wanted));
                if full || short {
                    return true;
                }
                match w.tree.parent.get(&at).copied().flatten() {
                    Some(parent) => at = parent,
                    None => return false,
                }
            }
            false
        }
        Source::Code(type_id) => {
            let Some(mut at) = ix.code_id(p, a.task_id, type_id) else {
                return false;
            };
            let values = ix.code_values(p);
            for _ in 0..=values.len() {
                let Some(v) = values.get(&at) else {
                    return false;
                };
                if v.short_name.eq_ignore_ascii_case(wanted) {
                    return true;
                }
                if v.parent == 0 || v.parent == at {
                    return false;
                }
                at = v.parent;
            }
            false
        }
        _ => false,
    }
}
