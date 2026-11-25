//! The User Defined columns of the view tables besides the Activities
//! window (WBS, Projects, Resources, Resource Assignments and the record
//! views): one catalog of a subject area's user defined fields, how their
//! values read in a cell, how a typed or picked cell value is read back,
//! and the one setter every cell edit (in place, Fill Down, Replace) goes
//! through. The values themselves are written by the shared edit layer
//! (`openxer_xer::edit::udf::set_value`), as the CLI and MCP write them.
use super::*;
use crate::records::num;
use openxer_xer::edit::udf::{self as shared, UdfKind, UdfType, UdfValue};
use std::collections::HashMap;

/// The group the Columns choosers list these columns under.
pub const GROUP: &str = "User Defined";
/// Width of a new User Defined column.
pub const WIDTH: f32 = 100.;
/// Field id prefix: `udf_<udf_type_id>`.
const PREFIX: &str = "udf_";

/// Subject areas of the view tables: the view and the table its fields
/// belong to (the UDFTYPE `table_name`).
pub const AREAS: [(&str, &str); 8] = [
    ("wbs", "PROJWBS"),
    ("projects", "PROJECT"),
    ("resources", "RSRC"),
    ("assignments", "TASKRSRC"),
    ("expenses", "PROJCOST"),
    ("issues", "PROJISSU"),
    ("risks", "PROJRISK"),
    ("documents", "DOCUMENT"),
];

/// The subject area of view table `view`, if it has one.
pub fn area_of(view: &str) -> Option<&'static str> {
    AREAS.iter().find(|(v, _)| *v == view).map(|(_, t)| *t)
}

/// The field id of a user defined column.
pub fn id_of(type_id: i64) -> String {
    format!("{PREFIX}{type_id}")
}

/// The field a column id names.
pub fn type_of(id: &str) -> Option<i64> {
    id.strip_prefix(PREFIX)?.parse().ok()
}

/// The fields of subject area `table`, in the dictionary's order.
pub fn fields(doc: &openxer_xer::Document, table: &str) -> Vec<UdfType> {
    shared::udf_types(doc, table)
}

/// Whether the column's values are numbers (right aligned).
pub fn is_number(ty: &UdfType) -> bool {
    ty.kind() == UdfKind::Number
}

/// Whether the column holds indicators (drawn with their marks).
pub fn is_indicator(ty: &UdfType) -> bool {
    ty.kind() == UdfKind::Indicator
}

/// A list of `items` after an empty choice, which clears the cell.
fn choice(items: impl Iterator<Item = String>) -> grid::EditKind {
    grid::EditKind::Choice(std::iter::once(String::new()).chain(items).collect())
}

/// How a cell of the field is edited in place.
pub fn editor(doc: &openxer_xer::Document, ty: &UdfType) -> Option<grid::EditKind> {
    match ty.kind() {
        UdfKind::Text | UdfKind::Number | UdfKind::Date => Some(grid::EditKind::Text),
        UdfKind::Indicator => {
            let names = shared::INDICATOR_NAMES.map(String::from);
            Some(choice(names.into_iter()))
        }
        UdfKind::ReadOnly if shared::is_code(&ty.logical) => {
            let codes = shared::udf_codes(doc, ty.type_id);
            Some(choice(codes.into_iter().map(|(_, short, _)| short)))
        }
        UdfKind::ReadOnly => None,
    }
}

/// The values of one document, looked up by field and record.
pub struct Values<'a> {
    doc: &'a openxer_xer::Document,
    origin: i64,
    rows: HashMap<(i64, i64), usize>,
}

/// What a cell sorts on: numbers and dates as numbers, the rest as text;
/// blanks last.
#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub enum SortKey {
    Number(f64),
    Text(String),
}

impl<'a> Values<'a> {
    pub fn new(doc: &'a openxer_xer::Document, origin: i64) -> Self {
        let rows = doc.table("UDFVALUE").map_or_else(HashMap::new, |t| {
            t.rows
                .iter()
                .enumerate()
                .map(|(i, r)| {
                    (
                        (num(t.value(r, "udf_type_id")), num(t.value(r, "fk_id"))),
                        i,
                    )
                })
                .collect()
        });
        Self { doc, origin, rows }
    }

    fn get(&self, type_id: i64, fk: i64, column: &str) -> String {
        self.rows
            .get(&(type_id, fk))
            .map(|&r| crate::records::get(self.doc, "UDFVALUE", r, column))
            .unwrap_or_default()
    }

    /// The value of field `ty` on record `fk` as a cell shows it.
    pub fn text(&self, ty: &UdfType, fk: i64) -> String {
        text_with(self.doc, self.origin, ty, |c| self.get(ty.type_id, fk, c))
    }

    /// What the cell sorts on (`None`: blank).
    pub fn sort_key(&self, ty: &UdfType, fk: i64) -> Option<SortKey> {
        sort_with(self.doc, self.origin, ty, |c| self.get(ty.type_id, fk, c))
    }
}

/// A value of field `ty` as a cell shows it, its stored columns read by
/// `get` (`doc` holds the field's code list).
fn text_with(
    doc: &openxer_xer::Document,
    origin: i64,
    ty: &UdfType,
    get: impl Fn(&str) -> String,
) -> String {
    match ty.kind() {
        UdfKind::Number => match get("udf_number").trim().parse() {
            Ok(v) if ty.logical == "FT_MONEY" => format::cost(v),
            Ok(v) => crate::records::format_number(v),
            Err(_) => String::new(),
        },
        UdfKind::Date => format::date(origin, date_with(origin, &get)),
        UdfKind::ReadOnly if shared::is_code(&ty.logical) => {
            let code = num(&get("udf_code_id"));
            if code == 0 {
                String::new()
            } else {
                crate::records::lookup(doc, "UDFCODE", "udf_code_id", code, "short_name")
            }
        }
        _ => get("udf_text"),
    }
}

fn date_with(origin: i64, get: &impl Fn(&str) -> String) -> Option<i64> {
    openxer_xer::parse_relative(origin, &get("udf_date"))
        .ok()
        .flatten()
}

/// What a value of field `ty` sorts on, its stored columns read by `get`.
fn sort_with(
    doc: &openxer_xer::Document,
    origin: i64,
    ty: &UdfType,
    get: impl Fn(&str) -> String,
) -> Option<SortKey> {
    match ty.kind() {
        UdfKind::Number => get("udf_number").trim().parse().ok().map(SortKey::Number),
        UdfKind::Date => date_with(origin, &get).map(|d| SortKey::Number(d as f64)),
        _ => Some(text_with(doc, origin, ty, get).to_lowercase())
            .filter(|t| !t.is_empty())
            .map(SortKey::Text),
    }
}

/// The stored values of one record, by field: kept with a summary of a
/// record whose document is not open (the Projects view's closed
/// projects), and read with the open document's field definitions.
pub type Raw = std::collections::BTreeMap<i64, std::collections::BTreeMap<String, String>>;

/// Every stored value whose record is `fk`, by field. Records of other
/// subject areas may share the key; a field names its own area.
pub fn raw_of(doc: &openxer_xer::Document, fk: i64) -> Raw {
    let Some(t) = doc.table("UDFVALUE") else {
        return Raw::new();
    };
    t.rows
        .iter()
        .filter(|r| num(t.value(r, "fk_id")) == fk)
        .map(|r| {
            let columns = ["udf_text", "udf_number", "udf_date", "udf_code_id"]
                .iter()
                .map(|c| (c.to_string(), t.value(r, c).to_string()))
                .collect();
            (num(t.value(r, "udf_type_id")), columns)
        })
        .collect()
}

fn raw_get(raw: &Raw, type_id: i64, column: &str) -> String {
    raw.get(&type_id)
        .and_then(|c| c.get(column))
        .cloned()
        .unwrap_or_default()
}

/// A stored value of field `ty` as a cell shows it (`raw_of`).
pub fn raw_text(doc: &openxer_xer::Document, origin: i64, ty: &UdfType, raw: &Raw) -> String {
    text_with(doc, origin, ty, |c| raw_get(raw, ty.type_id, c))
}

/// What a stored value of field `ty` sorts on (`raw_of`).
pub fn raw_sort(
    doc: &openxer_xer::Document,
    origin: i64,
    ty: &UdfType,
    raw: &Raw,
) -> Option<SortKey> {
    sort_with(doc, origin, ty, |c| raw_get(raw, ty.type_id, c))
}

/// The field with id `type_id`, when it is one.
fn type_named(doc: &openxer_xer::Document, type_id: i64) -> Option<UdfType> {
    shared::udf_type(doc, type_id).map(|x| x.1)
}

/// Values of one record and field only, without indexing the table.
fn one(doc: &openxer_xer::Document, origin: i64, type_id: i64, fk: i64) -> Values<'_> {
    let rows = shared::value_row(doc, type_id, fk)
        .map(|r| ((type_id, fk), r))
        .into_iter()
        .collect();
    Values { doc, origin, rows }
}

/// The value of field `type_id` on record `fk` as a cell shows it.
pub fn value_text(doc: &openxer_xer::Document, origin: i64, type_id: i64, fk: i64) -> String {
    match type_named(doc, type_id) {
        Some(ty) => one(doc, origin, type_id, fk).text(&ty, fk),
        None => String::new(),
    }
}

/// What the value of field `type_id` on record `fk` sorts on.
pub fn value_sort(
    doc: &openxer_xer::Document,
    origin: i64,
    type_id: i64,
    fk: i64,
) -> Option<SortKey> {
    let ty = type_named(doc, type_id)?;
    one(doc, origin, type_id, fk).sort_key(&ty, fk)
}

/// Order two sort keys with blanks last, ascending or descending.
pub fn compare(a: Option<&SortKey>, b: Option<&SortKey>, descending: bool) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (a, b) {
        (Some(x), Some(y)) => {
            let o = x.partial_cmp(y).unwrap_or(Ordering::Equal);
            if descending {
                o.reverse()
            } else {
                o
            }
        }
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

/// Read a typed or picked cell text as a value of field `ty`; `keep` is
/// the record's current date (a date typed without a time keeps its time).
pub fn parse(
    doc: &openxer_xer::Document,
    origin: i64,
    ty: &UdfType,
    text: &str,
    keep: Option<i64>,
) -> Result<UdfValue, String> {
    let text = text.trim();
    Ok(match ty.kind() {
        UdfKind::Text | UdfKind::Indicator => UdfValue::Text(text.to_string()),
        UdfKind::Number if text.is_empty() => UdfValue::Number(None),
        UdfKind::Number => {
            let plain = text.replace(',', "").parse::<f64>().ok();
            let v = plain
                .or_else(|| format::parse_cost(text))
                .ok_or_else(|| format!("{} must be a number.", ty.label))?;
            UdfValue::Number(Some(v))
        }
        UdfKind::Date => UdfValue::Date(format::parse_date(origin, text, keep)?),
        UdfKind::ReadOnly if shared::is_code(&ty.logical) => {
            if text.is_empty() {
                UdfValue::Code(None)
            } else {
                let code = shared::udf_codes(doc, ty.type_id)
                    .into_iter()
                    .find(|(_, short, name)| {
                        short.eq_ignore_ascii_case(text) || name.eq_ignore_ascii_case(text)
                    })
                    .ok_or_else(|| format!("{} has no code {text}.", ty.label))?;
                UdfValue::Code(Some(code.0))
            }
        }
        UdfKind::ReadOnly => return Err("This user defined field is not editable here.".into()),
    })
}

/// What a view table needs to show its User Defined columns: the columns
/// (`extra`, in field order after the table's own `own` fields), their
/// fields and titles.
pub struct Shown {
    pub own: usize,
    pub types: Vec<Option<UdfType>>,
    pub titles: Vec<String>,
}

impl Shown {
    /// The User Defined columns of `columns` (its extra fields).
    pub fn of(doc: &openxer_xer::Document, columns: &grid::Columns) -> Self {
        let types: Vec<Option<UdfType>> = columns
            .extra
            .iter()
            .map(|id| type_of(id).and_then(|t| type_named(doc, t)))
            .collect();
        Self {
            own: columns.own(),
            titles: types
                .iter()
                .map(|t| t.as_ref().map_or_else(String::new, |t| t.label.clone()))
                .collect(),
            types,
        }
    }

    /// The field of column `field`, when it is a User Defined one.
    pub fn field(&self, field: usize) -> Option<&UdfType> {
        self.types.get(field.checked_sub(self.own)?)?.as_ref()
    }

    /// The right-aligned (number) fields.
    pub fn right(&self) -> Vec<usize> {
        self.fields_where(is_number)
    }

    /// The indicator fields.
    pub fn indicators(&self) -> Vec<usize> {
        self.fields_where(is_indicator)
    }

    fn fields_where(&self, test: fn(&UdfType) -> bool) -> Vec<usize> {
        self.types
            .iter()
            .enumerate()
            .filter(|(_, t)| t.as_ref().is_some_and(test))
            .map(|(k, _)| self.own + k)
            .collect()
    }

    /// Add the User Defined cells to `rows`, whose records are `keys`
    /// (`None`: a grouping band or another row that holds no record of the
    /// area; a WBS element is a band row and a record).
    pub fn fill(
        &self,
        doc: &openxer_xer::Document,
        origin: i64,
        rows: &mut [grid::Row],
        keys: &[Option<i64>],
    ) {
        if self.types.is_empty() {
            return;
        }
        let values = Values::new(doc, origin);
        for (row, key) in rows.iter_mut().zip(keys) {
            row.cells.resize(self.own, String::new());
            for ty in &self.types {
                let text = match (ty, key) {
                    (Some(ty), Some(fk)) => values.text(ty, *fk),
                    _ => String::new(),
                };
                row.cells.push(text);
            }
        }
    }
}

/// The User Defined columns a view table can show, for its Columns
/// chooser: (id, title).
pub fn choices(doc: &openxer_xer::Document, view: &str) -> Vec<(String, String)> {
    let Some(table) = area_of(view) else {
        return vec![];
    };
    fields(doc, table)
        .into_iter()
        .map(|ty| (id_of(ty.type_id), ty.label))
        .collect()
}

impl App {
    /// The cell editor open in view table `table`, taken for its draw.
    pub(crate) fn take_cell_edit(&mut self, table: &'static str) -> Option<grid::CellEdit> {
        let (owner, edit) = &mut self.views.cell_edit;
        if *owner == table {
            edit.take()
        } else {
            None
        }
    }

    /// Keep table `table`'s cell editor after its draw.
    pub(crate) fn keep_cell_edit(&mut self, table: &'static str, edit: Option<grid::CellEdit>) {
        if edit.is_some() || self.views.cell_edit.0 == table {
            self.views.cell_edit = (table, edit);
        }
    }

    /// Set user defined field `type_id` of record `fk` to the cell text
    /// `text`, through the shared edit layer. Returns whether the value
    /// changed; the caller makes the undo step.
    pub(crate) fn set_udf_cell(
        &mut self,
        type_id: i64,
        fk: i64,
        text: &str,
    ) -> Result<bool, String> {
        let doc = &self.project.document;
        let origin = self.project.model.origin_serial;
        let Some(ty) = type_named(doc, type_id) else {
            return Err("No such user defined field.".into());
        };
        let keep = shared::value_row(doc, type_id, fk)
            .map(|r| crate::records::get(doc, "UDFVALUE", r, "udf_date"))
            .and_then(|d| openxer_xer::parse_relative(origin, &d).ok().flatten());
        let value = parse(doc, origin, &ty, text, keep)?;
        let pid = self.project.model.project.proj_id;
        shared::set_value(&mut self.project.document, origin, pid, type_id, fk, value)
    }

    /// A user defined cell typed or picked in a view table: one undo step
    /// when the value changed; a value the field refuses is reported and
    /// changes nothing.
    pub(crate) fn commit_udf_cell(&mut self, type_id: i64, fk: i64, text: &str, time: f64) {
        let before = self.project.clone();
        let before_revision = self.records_revision;
        match self.set_udf_cell(type_id, fk, text) {
            Ok(true) => self.edit_step(before, before_revision, time),
            Ok(false) => {}
            Err(e) => {
                self.project = before;
                self.error(e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_name_their_field() {
        assert_eq!(id_of(42), "udf_42");
        assert_eq!(type_of("udf_42"), Some(42));
        assert_eq!(type_of("rudf_42"), None);
        assert_eq!(area_of("wbs"), Some("PROJWBS"));
        assert_eq!(area_of("thresholds"), None);
    }
}
