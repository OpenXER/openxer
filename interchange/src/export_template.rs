//! Saved spreadsheet export templates: a subject area, the columns, a filter
//! and a sort, kept as JSON in the workspace preferences. A template names
//! columns by their stable keys, so the desktop's Export wizard and
//! `openxer-cli export-xlsx --template` build the same workbook from it.
use crate::columns::{value, Column, DurationUnit, Kind, Lookup, Value};
use crate::xlsx_export::ExportOptions;
use chrono::NaiveDate;
use openxer_xer::ProjectData;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

/// Workspace preference holding the saved templates (a JSON array).
pub const PREFERENCE_KEY: &str = "export_templates_v1";
/// The only subject area the modern workbook has.
pub const SUBJECT_ACTIVITIES: &str = "Activities";
pub const MAX_COLUMNS: usize = 200;
const MAX_CONDITIONS: usize = 20;
const MAX_SORT_KEYS: usize = 10;
const MAX_NAME: usize = 60;
/// Date formats the workbook can show (Excel number formats).
pub const DATE_FORMATS: [&str; 5] = [
    "dd-mmm-yy hh:mm",
    "dd-mmm-yy",
    "yyyy-mm-dd hh:mm",
    "mm/dd/yyyy hh:mm",
    "mm/dd/yyyy",
];

/// How a filter condition compares a column's value with its text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Op {
    Is,
    IsNot,
    Contains,
    DoesNotContain,
    GreaterThan,
    AtLeast,
    LessThan,
    AtMost,
    IsEmpty,
    IsNotEmpty,
}
impl Op {
    pub const ALL: [Op; 10] = [
        Op::Is,
        Op::IsNot,
        Op::Contains,
        Op::DoesNotContain,
        Op::GreaterThan,
        Op::AtLeast,
        Op::LessThan,
        Op::AtMost,
        Op::IsEmpty,
        Op::IsNotEmpty,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Op::Is => "is",
            Op::IsNot => "is not",
            Op::Contains => "contains",
            Op::DoesNotContain => "does not contain",
            Op::GreaterThan => "is greater than",
            Op::AtLeast => "is greater than or equal to",
            Op::LessThan => "is less than",
            Op::AtMost => "is less than or equal to",
            Op::IsEmpty => "is empty",
            Op::IsNotEmpty => "is not empty",
        }
    }
    fn takes_value(self) -> bool {
        !matches!(self, Op::IsEmpty | Op::IsNotEmpty)
    }
    fn ordered(self) -> bool {
        matches!(
            self,
            Op::GreaterThan | Op::AtLeast | Op::LessThan | Op::AtMost
        )
    }
}

/// One filter line: the column's value compared with `value`. Dates compare
/// as `YYYY-MM-DD` days, durations in the template's unit, text ignoring case.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Condition {
    pub column: String,
    pub op: Op,
    #[serde(default)]
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SortKey {
    pub column: String,
    #[serde(default)]
    pub descending: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportTemplate {
    pub name: String,
    pub subject: String,
    /// Column keys (`Column::key`), in sheet order.
    pub columns: Vec<String>,
    pub unit: DurationUnit,
    pub date_format: String,
    pub group_by_wbs: bool,
    /// Rows must meet every condition (`true`) or any of them.
    #[serde(default)]
    pub match_all: bool,
    #[serde(default)]
    pub filter: Vec<Condition>,
    #[serde(default)]
    pub sort: Vec<SortKey>,
}

impl ExportTemplate {
    /// A template of `columns` with the workbook's usual options.
    pub fn new(name: &str, columns: &[Column]) -> Self {
        Self {
            name: name.trim().to_string(),
            subject: SUBJECT_ACTIVITIES.into(),
            columns: columns.iter().map(|c| c.key()).collect(),
            unit: DurationUnit::Days,
            date_format: DATE_FORMATS[0].into(),
            group_by_wbs: true,
            match_all: true,
            filter: vec![],
            sort: vec![],
        }
    }

    /// Why this template cannot be used, if it cannot. Checks the template
    /// alone; `options` also checks it against a project.
    pub fn validate(&self) -> Result<(), String> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err("A template needs a name.".into());
        }
        if name.chars().count() > MAX_NAME || name.chars().any(char::is_control) {
            return Err(format!(
                "Template name {name:?} must be up to {MAX_NAME} characters without line breaks."
            ));
        }
        let label = |what: &str| format!("Template {name:?}: {what}");
        if self.subject != SUBJECT_ACTIVITIES {
            return Err(label(&format!("unknown subject area {:?}.", self.subject)));
        }
        if self.columns.is_empty() {
            return Err(label("choose at least one column."));
        }
        if self.columns.len() > MAX_COLUMNS {
            return Err(label(&format!("more than {MAX_COLUMNS} columns.")));
        }
        let mut seen = std::collections::HashSet::new();
        for key in &self.columns {
            if Column::from_key(key).is_none() {
                return Err(label(&format!("unknown column {key:?}.")));
            }
            if !seen.insert(key) {
                return Err(label(&format!("column {key:?} appears twice.")));
            }
        }
        if !DATE_FORMATS.contains(&self.date_format.as_str()) {
            return Err(label(&format!(
                "unknown date format {:?}.",
                self.date_format
            )));
        }
        if self.filter.len() > MAX_CONDITIONS {
            return Err(label(&format!("more than {MAX_CONDITIONS} filter lines.")));
        }
        for c in &self.filter {
            if Column::from_key(&c.column).is_none() {
                return Err(label(&format!("filter on unknown column {:?}.", c.column)));
            }
            if c.op.takes_value() && c.value.trim().is_empty() {
                return Err(label(&format!(
                    "the filter on {:?} needs a value to compare with.",
                    c.column
                )));
            }
            if c.value.chars().any(char::is_control) {
                return Err(label(
                    "a filter value has a line break or control character.",
                ));
            }
            let column = Column::from_key(&c.column).expect("checked");
            if c.op.takes_value() {
                match column_kind(column) {
                    Some(Kind::Date) if parse_day(&c.value).is_none() => {
                        return Err(label(&format!(
                            "{:?} is not a date; use YYYY-MM-DD.",
                            c.value
                        )))
                    }
                    Some(Kind::Duration | Kind::Number | Kind::Percent)
                        if c.value.trim().parse::<f64>().is_err() =>
                    {
                        return Err(label(&format!("{:?} is not a number.", c.value)))
                    }
                    Some(Kind::Text | Kind::Bool) if c.op.ordered() => {
                        return Err(label(&format!(
                            "{:?} cannot be compared with {:?}.",
                            c.column,
                            c.op.label()
                        )))
                    }
                    _ => {}
                }
            }
        }
        if self.sort.len() > MAX_SORT_KEYS {
            return Err(label(&format!("more than {MAX_SORT_KEYS} sort keys.")));
        }
        for s in &self.sort {
            if Column::from_key(&s.column).is_none() {
                return Err(label(&format!("sort on unknown column {:?}.", s.column)));
            }
        }
        Ok(())
    }

    /// The export options for `p`: the template's columns, and the rows that
    /// pass its filter in its sort order.
    pub fn options(&self, p: &ProjectData) -> Result<ExportOptions, String> {
        self.validate()?;
        let lookup = Lookup::new(p);
        let catalog = lookup.catalog();
        let resolve = |key: &str| -> Result<Column, String> {
            Column::from_key(key)
                .filter(|c| catalog.contains(c))
                .ok_or_else(|| {
                    format!(
                        "Template {:?}: the column {key:?} is not in this project.",
                        self.name
                    )
                })
        };
        let columns = self
            .columns
            .iter()
            .map(|k| resolve(k))
            .collect::<Result<Vec<_>, _>>()?;
        let mut rows: Vec<usize> = (0..p.model.activities.len()).collect();
        if !self.filter.is_empty() {
            let conditions = self
                .filter
                .iter()
                .map(|c| resolve(&c.column).map(|column| (column, c)))
                .collect::<Result<Vec<_>, _>>()?;
            rows.retain(|&i| {
                let mut passes = conditions.iter().map(|(column, c)| {
                    self.holds(c, &value(p, &lookup, i, *column), lookup.hours_per_day)
                });
                if self.match_all {
                    passes.all(|x| x)
                } else {
                    passes.any(|x| x)
                }
            });
        }
        if !self.sort.is_empty() {
            let keys = self
                .sort
                .iter()
                .map(|s| resolve(&s.column).map(|column| (column, s.descending)))
                .collect::<Result<Vec<_>, _>>()?;
            rows.sort_by(|&a, &b| {
                for (column, descending) in &keys {
                    let (x, y) = (value(p, &lookup, a, *column), value(p, &lookup, b, *column));
                    let order = compare(&x, &y, self.unit, lookup.hours_per_day);
                    let order = if *descending { order.reverse() } else { order };
                    if order != Ordering::Equal {
                        return order;
                    }
                }
                Ordering::Equal
            });
        }
        Ok(ExportOptions {
            columns,
            unit: self.unit,
            date_format: self.date_format.clone(),
            group_by_wbs: self.group_by_wbs,
            rows: Some(rows),
            extra: vec![],
        })
    }

    fn holds(&self, c: &Condition, v: &Value, hours_per_day: f64) -> bool {
        let empty = match v {
            Value::Empty => true,
            Value::Text(t) => t.trim().is_empty(),
            _ => false,
        };
        match c.op {
            Op::IsEmpty => return empty,
            Op::IsNotEmpty => return !empty,
            _ => {}
        }
        let wanted = c.value.trim();
        let order = match v {
            Value::Empty => return matches!(c.op, Op::IsNot | Op::DoesNotContain),
            Value::Text(t) => {
                let (t, w) = (t.to_lowercase(), wanted.to_lowercase());
                return match c.op {
                    Op::Is => t == w,
                    Op::IsNot => t != w,
                    Op::Contains => t.contains(&w),
                    Op::DoesNotContain => !t.contains(&w),
                    _ => false,
                };
            }
            Value::Bool(b) => {
                let w = matches!(wanted.to_lowercase().as_str(), "yes" | "true" | "y");
                return match c.op {
                    Op::Is => *b == w,
                    Op::IsNot => *b != w,
                    _ => false,
                };
            }
            Value::Date(m) => {
                let day = m.div_euclid(openxer_engine::calendar::DAY);
                match parse_day(wanted) {
                    Some(w) => day.cmp(&w),
                    None => return false,
                }
            }
            Value::Duration(m) => {
                let shown = self.unit.from_minutes(*m, hours_per_day);
                match wanted.parse::<f64>() {
                    Ok(w) => cmp_f64(shown, w),
                    Err(_) => return false,
                }
            }
            Value::Number(n) => match wanted.parse::<f64>() {
                Ok(w) => cmp_f64(*n, w),
                Err(_) => return false,
            },
        };
        match c.op {
            Op::Is | Op::Contains => order == Ordering::Equal,
            Op::IsNot | Op::DoesNotContain => order != Ordering::Equal,
            Op::GreaterThan => order == Ordering::Greater,
            Op::AtLeast => order != Ordering::Less,
            Op::LessThan => order == Ordering::Less,
            Op::AtMost => order != Ordering::Greater,
            Op::IsEmpty | Op::IsNotEmpty => unreachable!("handled above"),
        }
    }
}

fn column_kind(column: Column) -> Option<Kind> {
    match column {
        // A UDF's kind depends on the project; accept any comparison and let
        // a value that does not fit simply match nothing.
        Column::Udf(_) | Column::Extra(_) => None,
        c => Some(c.kind(&Lookup::empty())),
    }
}

fn cmp_f64(a: f64, b: f64) -> Ordering {
    if (a - b).abs() < 1e-9 {
        Ordering::Equal
    } else if a < b {
        Ordering::Less
    } else {
        Ordering::Greater
    }
}

/// `YYYY-MM-DD` as a day count on the workbook's serial scale.
fn parse_day(text: &str) -> Option<i64> {
    let d = NaiveDate::parse_from_str(text.trim(), "%Y-%m-%d").ok()?;
    let epoch = NaiveDate::from_ymd_opt(1899, 12, 30)?;
    Some((d - epoch).num_days())
}

/// Order of two cell values: empty first, then by kind.
fn compare(a: &Value, b: &Value, unit: DurationUnit, hours_per_day: f64) -> Ordering {
    let number = |v: &Value| -> Option<f64> {
        match v {
            Value::Number(n) => Some(*n),
            Value::Date(m) => Some(*m as f64),
            Value::Duration(m) => Some(unit.from_minutes(*m, hours_per_day)),
            Value::Bool(b) => Some(f64::from(u8::from(*b))),
            _ => None,
        }
    };
    match (a, b) {
        (Value::Empty, Value::Empty) => Ordering::Equal,
        (Value::Empty, _) => Ordering::Less,
        (_, Value::Empty) => Ordering::Greater,
        (Value::Text(x), Value::Text(y)) => x.to_lowercase().cmp(&y.to_lowercase()),
        _ => match (number(a), number(b)) {
            (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(Ordering::Equal),
            _ => Ordering::Equal,
        },
    }
}

/// The templates in a saved list, and why any entry was left out. A list that
/// is not valid JSON, an entry with unknown or missing fields, a template that
/// fails `validate` and a second template with a name already taken are all
/// rejected; the rest load.
pub fn parse_list(text: &str) -> (Vec<ExportTemplate>, Vec<String>) {
    let mut rejected = vec![];
    if text.trim().is_empty() {
        return (vec![], rejected);
    }
    let entries: Vec<serde_json::Value> = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(e) => {
            rejected.push(format!("The saved export templates cannot be read: {e}."));
            return (vec![], rejected);
        }
    };
    let mut out: Vec<ExportTemplate> = vec![];
    for (k, entry) in entries.into_iter().enumerate() {
        let template: ExportTemplate = match serde_json::from_value(entry) {
            Ok(t) => t,
            Err(e) => {
                rejected.push(format!(
                    "Saved export template {} is not valid: {e}.",
                    k + 1
                ));
                continue;
            }
        };
        if let Err(e) = template.validate() {
            rejected.push(e);
            continue;
        }
        if out
            .iter()
            .any(|t| t.name.trim().eq_ignore_ascii_case(template.name.trim()))
        {
            rejected.push(format!(
                "Template {:?} is saved twice; the first is kept.",
                template.name
            ));
            continue;
        }
        out.push(template);
    }
    (out, rejected)
}

/// The list as it is stored.
pub fn list_json(templates: &[ExportTemplate]) -> String {
    serde_json::to_string(templates).expect("templates serialize")
}

/// The template called `name`, ignoring case.
pub fn find<'a>(templates: &'a [ExportTemplate], name: &str) -> Option<&'a ExportTemplate> {
    templates
        .iter()
        .find(|t| t.name.trim().eq_ignore_ascii_case(name.trim()))
}

/// Parse one template from JSON (a template file) and validate it.
pub fn parse_one(text: &str) -> Result<ExportTemplate, String> {
    let t: ExportTemplate =
        serde_json::from_str(text).map_err(|e| format!("The template is not valid: {e}."))?;
    t.validate()?;
    Ok(t)
}
