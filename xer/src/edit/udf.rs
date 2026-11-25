//! User defined field values of every subject area: the field kinds, the
//! fields a subject area offers, the values a record has, and writing one
//! value with its type's checks. Shared by the desktop (Activity Details,
//! the activity table and the view tables' User Defined columns) and the
//! command line edit commands.
use super::records::{self, format_number, num};
use crate::Document;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UdfKind {
    Text,
    Number,
    Date,
    /// One of the four indicators, kept as its name in the text value.
    Indicator,
    ReadOnly,
}

/// The four indicator names, kept as the text value of an indicator field.
pub const INDICATOR_NAMES: [&str; 4] = ["Red", "Yellow", "Green", "Blue"];

/// Subject areas: the table a field's values belong to (the UDFTYPE
/// `table_name`) and that table's key, which a value's `fk_id` holds.
pub const SUBJECT_KEYS: [(&str, &str); 10] = [
    ("TASK", "task_id"),
    ("PROJCOST", "cost_item_id"),
    ("TASKPROC", "proc_id"),
    ("PROJISSU", "issue_id"),
    ("PROJRISK", "risk_id"),
    ("PROJECT", "proj_id"),
    ("TASKRSRC", "taskrsrc_id"),
    ("RSRC", "rsrc_id"),
    ("PROJWBS", "wbs_id"),
    ("DOCUMENT", "doc_id"),
];

/// Whether values of the subject area `table` belong to one project (all
/// but resources, which every project shares).
pub fn project_area(table: &str) -> bool {
    !table.eq_ignore_ascii_case("RSRC")
}

/// A value for one user defined field, by the field's type.
#[derive(Clone, Debug, PartialEq)]
pub enum UdfValue {
    Text(String),
    Number(Option<f64>),
    Date(Option<i64>),
    /// A code-list field's value (`udf_code_id`); `None` clears it.
    Code(Option<i64>),
}

/// The indicator named by `text`, ignoring case.
pub fn indicator_name(text: &str) -> Option<&'static str> {
    INDICATOR_NAMES
        .iter()
        .find(|name| name.eq_ignore_ascii_case(text.trim()))
        .copied()
}

#[derive(Clone, Debug)]
pub struct UdfField {
    pub type_id: i64,
    pub label: String,
    pub kind: UdfKind,
    pub text: String,
    pub number: Option<f64>,
    pub date: Option<i64>,
}
pub fn udf_kind(logical: &str) -> UdfKind {
    match logical {
        "FT_TEXT" => UdfKind::Text,
        "FT_INT" | "FT_FLOAT_2_DECIMALS" | "FT_FLOAT" | "FT_MONEY" => UdfKind::Number,
        "FT_START_DATE" | "FT_END_DATE" | "FT_DATE" => UdfKind::Date,
        "FT_STATICTYPE" => UdfKind::Indicator,
        _ => UdfKind::ReadOnly,
    }
}

/// A field whose values are picked from its own code list (UDFCODE).
pub fn is_code(logical: &str) -> bool {
    logical == "FT_CODE"
}

/// One field of a subject area, as the User Defined Fields dictionary
/// defines it.
#[derive(Clone, Debug, PartialEq)]
pub struct UdfType {
    pub type_id: i64,
    /// The title (the label, else the stored name).
    pub label: String,
    /// The stored data type (`FT_TEXT`, `FT_MONEY` ...).
    pub logical: String,
}

impl UdfType {
    pub fn kind(&self) -> UdfKind {
        udf_kind(&self.logical)
    }
}

fn title_of(t: &crate::Table, r: &[String]) -> String {
    let label = t.value(r, "udf_type_label");
    if label.is_empty() {
        t.value(r, "udf_type_name").to_string()
    } else {
        label.to_string()
    }
}

/// The fields of subject area `table` (the UDFTYPE `table_name`), in the
/// dictionary's order.
pub fn udf_types(doc: &Document, table: &str) -> Vec<UdfType> {
    let Some(t) = doc.table("UDFTYPE") else {
        return vec![];
    };
    t.rows
        .iter()
        .filter(|r| t.value(r, "table_name").eq_ignore_ascii_case(table))
        .map(|r| UdfType {
            type_id: num(t.value(r, "udf_type_id")),
            label: title_of(t, r),
            logical: t.value(r, "logical_data_type").to_string(),
        })
        .collect()
}

/// The field with id `type_id` and the subject area it belongs to.
pub fn udf_type(doc: &Document, type_id: i64) -> Option<(String, UdfType)> {
    let t = doc.table("UDFTYPE")?;
    t.rows
        .iter()
        .find(|r| num(t.value(r, "udf_type_id")) == type_id)
        .map(|r| {
            (
                t.value(r, "table_name").to_string(),
                UdfType {
                    type_id,
                    label: title_of(t, r),
                    logical: t.value(r, "logical_data_type").to_string(),
                },
            )
        })
}

/// The code list of a code-list field: `(udf_code_id, short name, name)`.
pub fn udf_codes(doc: &Document, type_id: i64) -> Vec<(i64, String, String)> {
    let Some(t) = doc.table("UDFCODE") else {
        return vec![];
    };
    t.rows
        .iter()
        .filter(|r| num(t.value(r, "udf_type_id")) == type_id)
        .map(|r| {
            (
                num(t.value(r, "udf_code_id")),
                t.value(r, "short_name").to_string(),
                t.value(r, "udf_code_name").to_string(),
            )
        })
        .collect()
}

/// The UDFVALUE row of field `type_id` on record `fk_id`.
pub fn value_row(doc: &Document, type_id: i64, fk_id: i64) -> Option<usize> {
    doc.table("UDFVALUE").and_then(|t| {
        t.rows.iter().position(|v| {
            num(t.value(v, "udf_type_id")) == type_id && num(t.value(v, "fk_id")) == fk_id
        })
    })
}

/// User defined fields of subject area `table` with the values of record
/// `fk_id`.
pub fn fields_of(doc: &Document, table: &str, fk_id: i64, origin: i64) -> Vec<UdfField> {
    let values = doc.table("UDFVALUE");
    udf_types(doc, table)
        .into_iter()
        .map(|ty| {
            let kind = ty.kind();
            let row = value_row(doc, ty.type_id, fk_id).and_then(|i| values?.rows.get(i));
            let get =
                |k: &str| row.map_or(String::new(), |v| values.unwrap().value(v, k).to_string());
            let mut text = get("udf_text");
            if kind == UdfKind::ReadOnly && text.is_empty() {
                let code = num(&get("udf_code_id"));
                if code != 0 {
                    text = records::lookup(doc, "UDFCODE", "udf_code_id", code, "short_name");
                }
                if text.is_empty() {
                    text = get("udf_number");
                }
            }
            let number = get("udf_number").trim().parse::<f64>().ok();
            let date = crate::parse_relative(origin, &get("udf_date")).unwrap_or(None);
            UdfField {
                type_id: ty.type_id,
                label: ty.label,
                kind,
                text,
                number,
                date,
            }
        })
        .collect()
}

/// Activity user defined fields with this activity's values.
pub fn udf_fields(doc: &Document, task_id: i64, origin: i64) -> Vec<UdfField> {
    fields_of(doc, "TASK", task_id, origin)
}

/// Write one UDF value. `None` removes the activity's row for that field.
pub fn set_udf(
    doc: &mut Document,
    type_id: i64,
    task_id: i64,
    pid: i64,
    column: &str,
    value: Option<String>,
) {
    write(doc, type_id, task_id, Some(pid), column, value);
}

/// Write one stored value of field `type_id` on record `fk_id`: `None`
/// removes the record's row for that field; a new row carries the project
/// `pid` when the field's values belong to one.
fn write(
    doc: &mut Document,
    type_id: i64,
    fk_id: i64,
    pid: Option<i64>,
    column: &str,
    value: Option<String>,
) {
    let row = value_row(doc, type_id, fk_id);
    match (row, value) {
        (Some(r), None) => records::remove_row(doc, "UDFVALUE", r),
        (Some(r), Some(v)) => records::set(doc, "UDFVALUE", r, column, v),
        (None, Some(v)) => {
            let mut cells = vec![
                ("udf_type_id", type_id.to_string()),
                ("fk_id", fk_id.to_string()),
            ];
            if let Some(pid) = pid {
                cells.push(("proj_id", pid.to_string()));
            }
            cells.push((column, v));
            records::add_row(doc, "UDFVALUE", &cells);
        }
        (None, None) => {}
    }
}

/// Set field `type_id` of record `fk_id` to `value`, checked against the
/// field's type: text is cleaned, an integer field takes whole numbers, an
/// indicator one of the four names, a code-list field one of its codes.
/// `pid` is the open project (new values of a project's records carry it).
/// Returns whether the stored value changed.
pub fn set_value(
    doc: &mut Document,
    origin: i64,
    pid: i64,
    type_id: i64,
    fk_id: i64,
    value: UdfValue,
) -> Result<bool, String> {
    let (table, ty) = udf_type(doc, type_id).ok_or("No such user defined field.")?;
    let logical = ty.logical.as_str();
    let (column, stored) = match (ty.kind(), value) {
        (UdfKind::Text, UdfValue::Text(t)) => {
            let t = records::clean(t.trim());
            ("udf_text", (!t.is_empty()).then_some(t))
        }
        (UdfKind::Number, UdfValue::Number(v)) => {
            if let Some(v) = v {
                if !v.is_finite() {
                    return Err("Enter a number.".into());
                }
                if logical == "FT_INT" && v.fract() != 0. {
                    return Err("Enter a whole number.".into());
                }
            }
            ("udf_number", v.map(format_number))
        }
        (UdfKind::Date, UdfValue::Date(v)) => {
            ("udf_date", v.map(|v| crate::date_string(origin, Some(v))))
        }
        (UdfKind::Indicator, UdfValue::Text(t)) => {
            let t = t.trim();
            if t.is_empty() {
                ("udf_text", None)
            } else {
                let name = indicator_name(t).ok_or("Choose Red, Yellow, Green or Blue.")?;
                ("udf_text", Some(name.to_string()))
            }
        }
        (UdfKind::ReadOnly, UdfValue::Code(code)) if is_code(logical) => {
            if let Some(c) = code {
                if !udf_codes(doc, type_id).iter().any(|(id, _, _)| *id == c) {
                    return Err(format!("{} has no such code.", ty.label));
                }
            }
            ("udf_code_id", code.map(|c| c.to_string()))
        }
        (UdfKind::ReadOnly, _) => {
            return Err("This user defined field is not editable here.".into())
        }
        _ => return Err("The value does not match the field's type.".into()),
    };
    let now = value_row(doc, type_id, fk_id).map(|r| records::get(doc, "UDFVALUE", r, column));
    let now = now.map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
    let same = match column {
        "udf_number" => now.and_then(|v| v.parse::<f64>().ok()).map(format_number) == stored,
        "udf_date" => {
            let at = now.and_then(|v| crate::parse_relative(origin, &v).ok().flatten());
            at.map(|d| crate::date_string(origin, Some(d))) == stored
        }
        "udf_code_id" => now.map(|v| num(&v)).filter(|c| *c != 0) == stored.as_deref().map(num),
        _ => now == stored,
    };
    if same {
        return Ok(false);
    }
    let pid = project_area(&table).then_some(pid);
    write(doc, type_id, fk_id, pid, column, stored);
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A document with one field of each data type on every subject area.
    fn areas() -> (Document, i64) {
        let p = crate::starter();
        let (mut doc, origin) = (p.updated_document(), p.model.origin_serial);
        let kinds = [
            "FT_TEXT",
            "FT_START_DATE",
            "FT_END_DATE",
            "FT_MONEY",
            "FT_INT",
            "FT_FLOAT_2_DECIMALS",
            "FT_STATICTYPE",
            "FT_CODE",
        ];
        for (a, (table, _)) in SUBJECT_KEYS.iter().enumerate() {
            for (k, kind) in kinds.iter().enumerate() {
                let id = 100 * (a + 1) + k;
                doc.table_mut("UDFTYPE").add(&[
                    ("udf_type_id", id.to_string()),
                    ("table_name", table.to_string()),
                    ("udf_type_name", format!("user_field_{id}")),
                    ("udf_type_label", format!("{table} {kind}")),
                    ("logical_data_type", kind.to_string()),
                ]);
            }
            let code = 100 * (a + 1) + 7;
            doc.table_mut("UDFCODE").add(&[
                ("udf_code_id", (code * 10).to_string()),
                ("udf_type_id", code.to_string()),
                ("short_name", "A1".into()),
                ("udf_code_name", "Area one".into()),
            ]);
        }
        (doc, origin)
    }

    #[test]
    fn values_of_every_subject_area_and_type_are_written_checked_and_kept_in_the_file() {
        let (mut doc, origin) = areas();
        let day = |d: i64| Some(d * 1440 + 480);
        for (a, (table, _)) in SUBJECT_KEYS.iter().enumerate() {
            let fk = 7000 + a as i64;
            let base = 100 * (a as i64 + 1);
            let values = [
                UdfValue::Text(format!("{table}\tnote")),
                UdfValue::Date(day(10)),
                UdfValue::Date(day(20)),
                UdfValue::Number(Some(1250.5)),
                UdfValue::Number(Some(4.)),
                UdfValue::Number(Some(0.25)),
                UdfValue::Text("green".into()),
                UdfValue::Code(Some(base * 10 + 70)),
            ];
            for (k, value) in values.into_iter().enumerate() {
                let changed = set_value(&mut doc, origin, 1, base + k as i64, fk, value.clone());
                assert_eq!(changed, Ok(true), "{table} field {k}");
                // The same value again changes nothing.
                let again = set_value(&mut doc, origin, 1, base + k as i64, fk, value);
                assert_eq!(again, Ok(false));
            }
            // Each type refuses what it cannot hold.
            let half = UdfValue::Number(Some(1.5));
            let whole = set_value(&mut doc, origin, 1, base + 4, fk, half);
            assert_eq!(whole, Err("Enter a whole number.".into()));
            let pink = UdfValue::Text("Pink".into());
            let color = set_value(&mut doc, origin, 1, base + 6, fk, pink);
            assert!(color.is_err());
            let code = set_value(&mut doc, origin, 1, base + 7, fk, UdfValue::Code(Some(1)));
            assert!(code.is_err());
            let kind = set_value(&mut doc, origin, 1, base, fk, UdfValue::Number(Some(1.)));
            assert!(kind.is_err());
        }
        // Saved and read back, every value is there as written.
        let back = Document::parse(&doc.text()).unwrap();
        for (a, (table, _)) in SUBJECT_KEYS.iter().enumerate() {
            let fk = 7000 + a as i64;
            let fields = fields_of(&back, table, fk, origin);
            assert_eq!(fields.len(), 8, "{table}");
            assert_eq!(fields[0].text, format!("{table} note"));
            assert_eq!(fields[1].date, day(10));
            assert_eq!(fields[2].date, day(20));
            assert_eq!(fields[3].number, Some(1250.5));
            assert_eq!(fields[4].number, Some(4.));
            assert_eq!(fields[5].number, Some(0.25));
            assert_eq!(fields[6].text, "Green");
            assert_eq!(fields[7].text, "A1");
            // Resources belong to no one project.
            let row = value_row(&back, 100 * (a as i64 + 1), fk).unwrap();
            let pid = records::get(&back, "UDFVALUE", row, "proj_id");
            assert_eq!(pid, if *table == "RSRC" { "" } else { "1" }, "{table}");
        }
        // Clearing removes the row.
        let base = 100;
        assert_eq!(
            set_value(&mut doc, origin, 1, base, 7000, UdfValue::Text(" ".into())),
            Ok(true)
        );
        assert_eq!(value_row(&doc, base, 7000), None);
    }
}
