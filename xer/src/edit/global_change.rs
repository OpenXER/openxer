//! Global Change: If conditions over any activity field, Then statements for
//! the activities that meet them and Else statements for the others. The
//! desktop's Global Change dialog and report, the command line and MCP all
//! use this one model and evaluation; each front end supplies a [`Host`]
//! that reads, shows and writes fields its way (the desktop with the user's
//! display formats, undo and its extra table columns; the command line with
//! the shared field catalog and [`crate::edit::fields::Plain`] formats).
//!
//! A run never changes the project by itself: [`preview`] tries every
//! change through the host (as a typed cell would be written) so later
//! statements see earlier ones, and the caller puts the project back.
//! [`commit`] then writes the lines the user kept, which the caller makes
//! one undo step.
use crate::edit::fields::{self, FieldDef, Formats, Index, Kind, Op, Plain, Test, Value};
use crate::ProjectData;
use openxer_engine::calendar::DAY;
use serde::{Deserialize, Serialize};

/// The workspace preferences row that holds the saved definitions (a
/// JSON list, see [`to_json`]); the app writes it, the command line reads it.
pub const PREFERENCE_KEY: &str = "global_changes_v1";

/// The Then and Else operators: none (plain `=`), add, subtract, multiply,
/// divide and text concatenation.
pub const OPERATORS: [&str; 6] = ["", "+", "-", "x", "/", "&"];

/// One If row: `Parameter Is Value [High Value]`, joined to the row before
/// it by And (`and` true) or Or. And binds before Or. A definition made in
/// the dialog joins every row the same way, by its All or Any heading
/// ([`GlobalChange::heading`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Condition {
    pub and: bool,
    pub column: String,
    pub op: String,
    pub value: String,
    pub high: String,
}

impl Default for Condition {
    fn default() -> Self {
        Self {
            and: true,
            column: String::new(),
            op: Op::Equals.key().into(),
            value: String::new(),
            high: String::new(),
        }
    }
}

/// One Then or Else row: `Parameter = Value [Operator Value]`. A value is a
/// literal, a quoted text (spaces kept) or the title of another field.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Assignment {
    pub column: String,
    pub value: String,
    pub operator: String,
    pub value2: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GlobalChange {
    pub name: String,
    pub conditions: Vec<Condition>,
    pub then: Vec<Assignment>,
    pub otherwise: Vec<Assignment>,
}

impl GlobalChange {
    /// The If heading: `Some(true)` for "All of the following",
    /// `Some(false)` for "Any of the following", `None` when an older
    /// definition joins its rows both ways. With no rows, All.
    pub fn heading(&self) -> Option<bool> {
        match self.conditions.as_slice() {
            [] => Some(true),
            [one] => Some(one.and),
            [_, rest @ ..] => {
                let all = rest[0].and;
                rest.iter().all(|c| c.and == all).then_some(all)
            }
        }
    }
    /// Join every If row by `all` (All of the following) or Any.
    pub fn set_heading(&mut self, all: bool) {
        for c in &mut self.conditions {
            c.and = all;
        }
    }
}

pub fn to_json(list: &[GlobalChange]) -> String {
    serde_json::to_string(list).unwrap_or_else(|_| "[]".into())
}

/// Read stored definitions; the whole list is rejected when one is invalid.
pub fn from_json(text: &str) -> Result<Vec<GlobalChange>, String> {
    let list: Vec<GlobalChange> = serde_json::from_str(text).map_err(|e| e.to_string())?;
    for g in &list {
        check(g)?;
    }
    Ok(list)
}

/// A definition given as JSON by a script or an agent: the stored form,
/// where an If row's `op` may also be its caption ("is under") and a
/// top-level `"match": "all"` or `"any"` sets the If heading.
pub fn from_value(mut value: serde_json::Value) -> Result<GlobalChange, String> {
    let heading = match value.as_object_mut().and_then(|m| m.remove("match")) {
        None => None,
        Some(serde_json::Value::String(s)) if s.eq_ignore_ascii_case("all") => Some(true),
        Some(serde_json::Value::String(s)) if s.eq_ignore_ascii_case("any") => Some(false),
        Some(other) => return Err(format!("match is \"all\" or \"any\", not {other}")),
    };
    let mut g: GlobalChange = serde_json::from_value(value).map_err(|e| e.to_string())?;
    for c in &mut g.conditions {
        if Op::from_key(&c.op).is_none() {
            if let Some(op) = Op::ALL
                .into_iter()
                .chain([Op::IsUnder])
                .find(|o| o.caption().eq_ignore_ascii_case(c.op.trim()))
            {
                c.op = op.key().into();
            }
        }
    }
    if let Some(all) = heading {
        g.set_heading(all);
    }
    if g.name.trim().is_empty() {
        g.name = "(Global Change)".into();
    }
    check(&g)?;
    Ok(g)
}

/// Structural checks: a name, known operators and comparison keys.
pub fn check(g: &GlobalChange) -> Result<(), String> {
    if g.name.trim().is_empty() {
        return Err("Enter a global change name.".into());
    }
    if let Some(c) = g.conditions.iter().find(|c| Op::from_key(&c.op).is_none()) {
        return Err(format!("{:?} is not a comparison.", c.op));
    }
    for a in g.then.iter().chain(&g.otherwise) {
        if !OPERATORS.contains(&a.operator.as_str()) {
            return Err(format!("{:?} is not an operator.", a.operator));
        }
        if a.column.trim().is_empty() {
            return Err("Destination parameter not specified.".into());
        }
        if !a.operator.is_empty() && a.value2.trim().is_empty() {
            return Err("An operator needs a value after it.".into());
        }
    }
    Ok(())
}

/// The name a pasted copy of `source` takes: the source name with the
/// first number from 1 up that gives a name not `taken` ("Name1", then
/// "Name2"), as observed in the reference product.
pub fn pasted_name(source: &str, taken: impl Fn(&str) -> bool) -> String {
    (1u32..)
        .map(|k| format!("{source}{k}"))
        .find(|n| !taken(n))
        .unwrap_or_default()
}

/// One line of the Global Change Report.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Change {
    pub task_id: i64,
    /// Project ID and WBS Code of the activity, as the table shows them.
    pub project: String,
    pub wbs: String,
    pub activity: String,
    pub name: String,
    pub column: String,
    pub title: String,
    pub old: String,
    /// Editor text that sets the new value.
    pub new_text: String,
    /// The new value as the table shows it.
    pub new: String,
    pub error: Option<String>,
    pub apply: bool,
}

/// A field a statement names: its id, title and kind.
#[derive(Clone, Debug, PartialEq)]
pub struct FieldRef {
    pub id: String,
    pub title: String,
    pub kind: Kind,
}

/// What a front end gives a Global Change run: its fields, how values are
/// shown and typed, how an If row is tested and how a value is written.
pub trait Host {
    fn project(&self) -> &ProjectData;
    /// The field `text` names: its id, or its title ignoring case.
    fn field(&self, text: &str) -> Option<FieldRef>;
    fn value(&self, i: usize, id: &str) -> Value;
    fn display(&self, i: usize, value: &Value) -> String;
    fn parse(&self, i: usize, kind: Kind, text: &str) -> Result<Value, String>;
    /// Text that an in-place editor would show for `value` and read back.
    fn input_text(&self, i: usize, value: &Value) -> String;
    /// Does If row `c` hold for activity `i`?
    fn holds(&self, i: usize, c: &Condition) -> bool;
    /// Write typed text into field `id` of activity `i`, as a typed cell
    /// is written. Returns whether anything changed; on `Err` nothing did.
    fn write(&mut self, i: usize, id: &str, text: &str) -> Result<bool, String>;
}

/// Do activity `i`'s values meet the If rows? No rows means every activity.
pub fn matches(host: &dyn Host, g: &GlobalChange, i: usize) -> bool {
    let rows: Vec<&Condition> = g
        .conditions
        .iter()
        .filter(|c| !c.column.is_empty())
        .collect();
    if rows.is_empty() {
        return true;
    }
    // Or of And-groups.
    let mut any = false;
    let mut group = true;
    for (k, c) in rows.iter().enumerate() {
        if k > 0 && !c.and {
            any |= group;
            group = true;
        }
        group &= host.holds(i, c);
    }
    any || group
}

/// An operand: another field's value for the activity, a quoted text
/// (spaces kept), or a literal read as `kind`.
fn operand(host: &dyn Host, i: usize, text: &str, kind: Kind) -> Result<Value, String> {
    let t = text.trim();
    if t.len() >= 2 && t.starts_with('"') && t.ends_with('"') {
        return Ok(Value::Text(t[1..t.len() - 1].to_string()));
    }
    match host.field(text) {
        Some(f) => Ok(host.value(i, &f.id)),
        None => host.parse(i, kind, text),
    }
}

fn number(v: &Value, what: &str) -> Result<f64, String> {
    v.number().ok_or_else(|| format!("{what} is not a number."))
}

/// A number in the same kind of value as `like`.
fn scaled(like: &Value, x: f64) -> Value {
    match like {
        Value::Percent(_) => Value::Percent(x),
        Value::Units(_) => Value::Units(x),
        Value::Cost(_) => Value::Cost(x),
        _ => Value::Number(x),
    }
}

/// The new value of a Then or Else row for activity `i`, as editor text for
/// the destination field.
pub fn compute(host: &dyn Host, a: &Assignment, i: usize) -> Result<String, String> {
    let dest = host
        .field(&a.column)
        .ok_or("Destination parameter not specified.")?;
    let first = operand(host, i, &a.value, dest.kind)?;
    let text = |v: &Value| host.display(i, v);
    let result = match a.operator.as_str() {
        "" => first,
        "&" => {
            let second = operand(host, i, &a.value2, Kind::Text)?;
            Value::Text(format!("{}{}", text(&first), text(&second)))
        }
        op @ ("+" | "-") => {
            let sign = if op == "+" { 1. } else { -1. };
            match first {
                Value::Empty => return Err(format!("{} has no value.", a.value.trim())),
                Value::Date { minute, .. } => {
                    // A number added to a date counts calendar days.
                    let second = operand(host, i, &a.value2, Kind::Number)?;
                    let minutes = match second {
                        Value::Duration(m) => m as f64,
                        other => number(&other, a.value2.trim())? * DAY as f64,
                    };
                    Value::Date {
                        minute: minute + (sign * minutes).round() as i64,
                        mark: fields::DateMark::Plain,
                    }
                }
                Value::Duration(m) => {
                    let second = operand(host, i, &a.value2, Kind::Duration)?;
                    let add = match second {
                        Value::Duration(x) => x as f64,
                        other => number(&other, a.value2.trim())?,
                    };
                    Value::Duration((m as f64 + sign * add).round() as i64)
                }
                Value::Text(s) if op == "+" => {
                    let second = operand(host, i, &a.value2, Kind::Text)?;
                    Value::Text(format!("{s}{}", text(&second)))
                }
                Value::Text(_) | Value::Flag(_) => {
                    return Err(
                        "Source parameter/value is not compatible with the operator.".into(),
                    )
                }
                other => {
                    let x = number(&other, a.value.trim())?;
                    let y = number(&operand(host, i, &a.value2, Kind::Number)?, a.value2.trim())?;
                    scaled(&other, x + sign * y)
                }
            }
        }
        op @ ("x" | "/") => {
            let y = number(&operand(host, i, &a.value2, Kind::Number)?, a.value2.trim())?;
            if op == "/" && y == 0. {
                return Err("Division by zero is not allowed.".into());
            }
            let factor = if op == "x" { y } else { 1. / y };
            match first {
                Value::Empty => return Err(format!("{} has no value.", a.value.trim())),
                Value::Duration(m) => Value::Duration((m as f64 * factor).round() as i64),
                Value::Date { .. } | Value::Text(_) | Value::Flag(_) => {
                    return Err(
                        "Source parameter/value is not compatible with the operator.".into(),
                    )
                }
                other => {
                    let x = number(&other, a.value.trim())?;
                    scaled(&other, x * factor)
                }
            }
        }
        other => return Err(format!("{other:?} is not an operator.")),
    };
    if !result.number().is_none_or(f64::is_finite) {
        return Err("The result is not a number.".into());
    }
    Ok(host.input_text(i, &result))
}

/// Work out global change `g` for the activities `in_scope` (one flag per
/// activity) as report lines. Each change is written through the host so
/// later statements see earlier ones, and its new value is read back; the
/// caller puts the project back afterwards. A line whose value does not
/// change is left out; one the host refuses carries the reason.
pub fn preview(host: &mut dyn Host, g: &GlobalChange, in_scope: &[bool]) -> Vec<Change> {
    let mut changes = vec![];
    let count = host.project().model.activities.len();
    for i in (0..count).filter(|&i| in_scope.get(i).copied().unwrap_or(false)) {
        let (task_id, code, name) = {
            let p = host.project();
            let a = &p.model.activities[i];
            (a.task_id, a.task_code.clone(), p.name(i).to_string())
        };
        // Project ID and WBS Code as the table shows them, read before any
        // change is tried.
        let shown = |host: &dyn Host, id: &str| {
            host.field(id)
                .map_or_else(String::new, |f| host.display(i, &host.value(i, &f.id)))
        };
        let (project, wbs) = (shown(host, "project_id"), shown(host, "wbs"));
        let branch = if matches(host, g, i) {
            g.then.clone()
        } else {
            g.otherwise.clone()
        };
        for a in &branch {
            let Some(field) = host.field(&a.column) else {
                continue;
            };
            let show = |host: &dyn Host| host.display(i, &host.value(i, &field.id));
            let old = show(host);
            let (new_text, new, error) = match compute(host, a, i) {
                Ok(text) => match host.write(i, &field.id, &text) {
                    Ok(_) => {
                        let new = show(host);
                        (text, new, None)
                    }
                    Err(e) => (text.clone(), text, Some(e)),
                },
                Err(e) => (String::new(), String::new(), Some(e)),
            };
            if error.is_none() && new == old {
                continue;
            }
            changes.push(Change {
                task_id,
                project: project.clone(),
                wbs: wbs.clone(),
                activity: code.clone(),
                name: name.clone(),
                column: field.id.clone(),
                title: field.title.clone(),
                old,
                new_text,
                new,
                apply: error.is_none(),
                error,
            });
        }
    }
    changes
}

/// Write the report lines that are ticked and carry no error. Returns how
/// many were written and how many could not be.
pub fn commit(host: &mut dyn Host, changes: &[Change]) -> (usize, usize) {
    let mut applied = 0;
    let mut failed = 0;
    for c in changes.iter().filter(|c| c.apply && c.error.is_none()) {
        let Some(i) = host
            .project()
            .model
            .activities
            .iter()
            .position(|a| a.task_id == c.task_id)
        else {
            failed += 1;
            continue;
        };
        match host.write(i, &c.column, &c.new_text) {
            Ok(_) => applied += 1,
            Err(_) => failed += 1,
        }
    }
    (applied, failed)
}

/// The command line's host: the shared field catalog over a project, in
/// [`Plain`] formats, writing through the shared setters.
pub struct ProjectHost<'a> {
    pub project: &'a mut ProjectData,
    catalog: Vec<FieldDef>,
    index: Index,
}

impl<'a> ProjectHost<'a> {
    pub fn new(project: &'a mut ProjectData) -> Self {
        let catalog = fields::catalog(project);
        let index = Index::for_project(project);
        Self {
            project,
            catalog,
            index,
        }
    }
    /// The fields of the project's catalog.
    pub fn catalog(&self) -> &[FieldDef] {
        &self.catalog
    }
    fn def(&self, id: &str) -> Option<&FieldDef> {
        self.catalog.iter().find(|d| d.id == id)
    }
}

/// The Project ID field the report shows; the command line's catalog has
/// no table column for it.
const PROJECT_ID: &str = "project_id";

impl Host for ProjectHost<'_> {
    fn project(&self) -> &ProjectData {
        &*self.project
    }
    fn field(&self, text: &str) -> Option<FieldRef> {
        if text.trim() == PROJECT_ID {
            return Some(FieldRef {
                id: PROJECT_ID.into(),
                title: "Project ID".into(),
                kind: Kind::Text,
            });
        }
        fields::find(&self.catalog, text).map(|d| FieldRef {
            id: d.id.clone(),
            title: d.title.clone(),
            kind: d.kind,
        })
    }
    fn value(&self, i: usize, id: &str) -> Value {
        if id == PROJECT_ID {
            return Value::text(self.project.model.project.name.clone());
        }
        self.def(id).map_or(Value::Empty, |d| {
            fields::value(&*self.project, &self.index, d.source, d.kind, i)
        })
    }
    fn display(&self, i: usize, value: &Value) -> String {
        fields::display(&*self.project, &Plain, Some(i), value)
    }
    fn parse(&self, i: usize, kind: Kind, text: &str) -> Result<Value, String> {
        fields::parse_input(&*self.project, &Plain, Some(i), kind, text)
    }
    fn input_text(&self, i: usize, value: &Value) -> String {
        fields::input_text(&*self.project, &Plain, i, value)
    }
    fn holds(&self, i: usize, c: &Condition) -> bool {
        let Some(d) = fields::find(&self.catalog, &c.column) else {
            return true;
        };
        condition_holds(&*self.project, &self.index, &Plain, d, c, i)
    }
    fn write(&mut self, i: usize, id: &str, text: &str) -> Result<bool, String> {
        let def = self
            .def(id)
            .cloned()
            .ok_or_else(|| format!("No field {id}."))?;
        let changed = fields::write(
            self.project,
            &Plain,
            &def,
            i,
            text,
            crate::edit::activity::hours_text,
        )?;
        // The lookups describe the project as it was.
        self.index = Index::for_project(&*self.project);
        Ok(changed)
    }
}

/// Does If row `c` hold for activity `i`'s value of field `d`, read and
/// typed in `f`?
pub fn condition_holds(
    p: &ProjectData,
    ix: &Index,
    f: &dyn Formats,
    d: &FieldDef,
    c: &Condition,
    i: usize,
) -> bool {
    let op = Op::from_key(&c.op).unwrap_or(Op::Equals);
    let value = fields::value(p, ix, d.source, d.kind, i);
    let wanted = c.value.trim().to_lowercase();
    let test = Test {
        op,
        kind: d.kind,
        wanted: &wanted,
        high: &c.high,
    };
    fields::holds(
        &test,
        &value,
        &|| fields::display(p, f, Some(i), &value).to_lowercase(),
        &|high| {
            let s = if high { &c.high } else { &c.value };
            fields::parse_input(p, f, Some(i), d.kind, s).ok()
        },
        &|| fields::is_under(p, ix, d.source, i, &c.value),
    )
}

/// Fields a Then or Else row can set: every editable field, user defined
/// fields included.
pub fn assignable(catalog: &[FieldDef]) -> Vec<&FieldDef> {
    catalog
        .iter()
        .filter(|c| c.editor != fields::Editor::ReadOnly)
        .collect()
}

#[cfg(test)]
mod tests;
