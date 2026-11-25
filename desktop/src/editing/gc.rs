//! Global Change in the app. The definitions, their evaluation, the report
//! lines and the commit are the shared ones
//! (`openxer_xer::edit::global_change`), which the command line and MCP run
//! too; this module gives them the app's side: the activity table's columns
//! and display formats, its filter evaluation for the If rows and the
//! layout filter's scope, and writes through the in-place editing rules so
//! a global change accepts exactly what a typed cell accepts.
use crate::grid::columns::{self, Column, Editor, Index, Kind, Source, Value};
use crate::grid::filters::{self, Criterion, FilterSelection, Op, PreparedSelection, UserFilter};
use crate::App;
use openxer_xer::edit::global_change::{FieldRef, Host};
use openxer_xer::ProjectData;

pub use openxer_xer::edit::global_change::{
    check, from_json, pasted_name, to_json, Assignment, Change, Condition, GlobalChange, OPERATORS,
};

/// Columns a Then or Else row can set: every editable column, user defined
/// fields included (16.092).
pub fn assignable(catalog: &[Column]) -> Vec<&Column> {
    catalog
        .iter()
        .filter(|c| c.editor != Editor::ReadOnly)
        .filter(|c| !matches!(c.source, Source::Notebook(_)))
        .collect()
}

/// Whether an If row on `column` offers "is under": WBS and the activity
/// code fields form a hierarchy.
pub fn offers_under(column: &Column) -> bool {
    column
        .source
        .shared()
        .is_some_and(openxer_xer::edit::fields::under_applies)
}

/// The "Is" choices an If row on `column` offers.
pub fn ops_for(column: Option<&Column>) -> Vec<Op> {
    let mut ops = Op::ALL.to_vec();
    if column.is_some_and(offers_under) {
        ops.push(Op::IsUnder);
    }
    ops
}

/// Which activities a global change acts on, one flag per activity.
/// `filter` is the layout filter of the Activities view the change was
/// started from (`None` from any other view). Only the activities that
/// filter shows are in scope, tested with the table's own filter
/// evaluation; If rows then pick among those. With no filter, All
/// Activities, or a filter that only highlights, every activity is in
/// scope.
pub fn scope(
    p: &ProjectData,
    ix: &Index,
    catalog: &[Column],
    filter: Option<(&FilterSelection, &[UserFilter])>,
) -> Vec<bool> {
    let n = p.model.activities.len();
    match filter {
        Some((selection, users)) if selection.active() && !selection.highlight => {
            let shown = PreparedSelection::new(p, catalog, selection, users);
            (0..n).map(|i| shown.matches(p, ix, i)).collect()
        }
        _ => vec![true; n],
    }
}

/// A column a statement names: by title (ignoring case) or by id.
fn find<'a>(catalog: &'a [Column], text: &str) -> Option<&'a Column> {
    let t = text.trim();
    catalog
        .iter()
        .find(|c| c.title.eq_ignore_ascii_case(t) || c.id == t)
}

fn field_ref(c: &Column) -> FieldRef {
    FieldRef {
        id: c.id.clone(),
        title: c.title.clone(),
        kind: c.kind,
    }
}

/// Does If row `c` hold for activity `i`, by the table's filter rules?
fn row_holds(p: &ProjectData, ix: &Index, catalog: &[Column], c: &Condition, i: usize) -> bool {
    let criterion = Criterion {
        column: c.column.clone(),
        op: Op::from_key(&c.op).unwrap_or(Op::Equals),
        value: c.value.clone(),
        high: c.high.clone(),
        level: 0,
    };
    filters::criterion_matches(p, ix, catalog, &criterion, i)
}

/// The project as the table shows it, read only: for working out values
/// without writing them.
#[cfg(test)]
pub struct View<'a> {
    pub p: &'a ProjectData,
    pub ix: &'a Index,
    pub catalog: &'a [Column],
}

#[cfg(test)]
impl Host for View<'_> {
    fn project(&self) -> &ProjectData {
        self.p
    }
    fn field(&self, text: &str) -> Option<FieldRef> {
        find(self.catalog, text).map(field_ref)
    }
    fn value(&self, i: usize, id: &str) -> Value {
        columns::find(self.catalog, id)
            .map_or(Value::Empty, |c| columns::value(self.p, self.ix, c, i))
    }
    fn display(&self, i: usize, value: &Value) -> String {
        columns::display(self.p, Some(i), value)
    }
    fn parse(&self, i: usize, kind: Kind, text: &str) -> Result<Value, String> {
        columns::parse_input(self.p, Some(i), kind, text)
    }
    fn input_text(&self, i: usize, value: &Value) -> String {
        input_text(self.p, i, value)
    }
    fn holds(&self, i: usize, c: &Condition) -> bool {
        row_holds(self.p, self.ix, self.catalog, c, i)
    }
    fn write(&mut self, _: usize, id: &str, _: &str) -> Result<bool, String> {
        Err(format!("{id} is not written here."))
    }
}

/// The open project in the app: values as the table shows them, writes
/// through the in-place editing rules (`App::try_apply_cell`).
pub(crate) struct AppHost<'a> {
    pub app: &'a mut App,
    pub catalog: &'a [Column],
    ix: Index,
}

impl<'a> AppHost<'a> {
    pub(crate) fn new(app: &'a mut App, catalog: &'a [Column]) -> Self {
        let ix = app.grid_index();
        Self { app, catalog, ix }
    }
}

impl Host for AppHost<'_> {
    fn project(&self) -> &ProjectData {
        &self.app.project
    }
    fn field(&self, text: &str) -> Option<FieldRef> {
        find(self.catalog, text).map(field_ref)
    }
    fn value(&self, i: usize, id: &str) -> Value {
        columns::find(self.catalog, id).map_or(Value::Empty, |c| {
            columns::value(&self.app.project, &self.ix, c, i)
        })
    }
    fn display(&self, i: usize, value: &Value) -> String {
        columns::display(&self.app.project, Some(i), value)
    }
    fn parse(&self, i: usize, kind: Kind, text: &str) -> Result<Value, String> {
        columns::parse_input(&self.app.project, Some(i), kind, text)
    }
    fn input_text(&self, i: usize, value: &Value) -> String {
        input_text(&self.app.project, i, value)
    }
    fn holds(&self, i: usize, c: &Condition) -> bool {
        row_holds(&self.app.project, &self.ix, self.catalog, c, i)
    }
    fn write(&mut self, i: usize, id: &str, text: &str) -> Result<bool, String> {
        let result = self.app.try_apply_cell(i, id, text);
        // The lookups describe the project as it was.
        self.ix = self.app.grid_index();
        result
    }
}

/// Text for a value as the in-place editor shows it (and parses it back).
pub fn input_text(p: &ProjectData, i: usize, v: &Value) -> String {
    openxer_xer::edit::fields::input_text(p, &columns::UserFormats, i, v)
}

/// The new value of a Then or Else row for activity `i`, as editor text for
/// the destination column.
#[cfg(test)]
pub fn compute(
    p: &ProjectData,
    ix: &Index,
    catalog: &[Column],
    a: &Assignment,
    i: usize,
) -> Result<String, String> {
    openxer_xer::edit::global_change::compute(&View { p, ix, catalog }, a, i)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The table's columns for the fields the shared catalog reads and
    /// writes carry the shared field's id, title, kind and editor, so the
    /// app, the command line and MCP name and treat each field the same.
    #[test]
    fn the_table_columns_match_the_shared_catalog() {
        let shared = openxer_xer::edit::fields::builtin();
        let mut matched = 0;
        for c in columns::builtin() {
            let Source::Field(f) = c.source else {
                continue;
            };
            let Some(f) = f.shared() else {
                continue;
            };
            let s = shared
                .iter()
                .find(|s| s.source == openxer_xer::edit::fields::Source::Field(f))
                .unwrap_or_else(|| panic!("{} is in the shared catalog", c.id));
            assert_eq!(
                (s.id.as_str(), s.title.as_str(), s.kind, s.editor),
                (c.id.as_str(), c.title.as_str(), c.kind, c.editor),
                "{}",
                c.id
            );
            matched += 1;
        }
        assert_eq!(matched, shared.len(), "every shared field is a column");
    }

    #[test]
    fn a_pasted_copy_takes_the_next_free_number() {
        let taken = ["Increase Durations", "Increase Durations1"];
        let is_taken = |n: &str| taken.iter().any(|t| t.eq_ignore_ascii_case(n));
        assert_eq!(pasted_name("Shift", is_taken), "Shift1");
        assert_eq!(
            pasted_name("Increase Durations", is_taken),
            "Increase Durations2"
        );
    }
}
