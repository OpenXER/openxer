//! Project code assignments (`PROJPCAT`): a project holds at most one value
//! of each project code type (`PCATTYPE`, values in `PCATVAL`). Assigning a
//! value replaces the project's value of that type; removing takes it off.
//! The desktop's Project Details Codes tab and the command line's
//! `set_project_code` both write through here.
use super::records::{self, num};
use crate::{Document, ProjectData};

type R<T> = Result<T, String>;

/// One code assignment of a project, with the names it shows under.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Assigned {
    pub type_id: i64,
    pub value_id: i64,
    pub code_type: String,
    pub value: String,
    pub description: String,
}

/// The project code types: (id, name), in dictionary order.
pub fn code_types(doc: &Document) -> Vec<(i64, String)> {
    let Some(t) = doc.table("PCATTYPE") else {
        return vec![];
    };
    let mut v: Vec<(i64, i64, String)> = t
        .rows
        .iter()
        .map(|r| {
            (
                num(t.value(r, "seq_num")),
                num(t.value(r, "proj_catg_type_id")),
                t.value(r, "proj_catg_type").to_string(),
            )
        })
        .collect();
    v.sort();
    v.into_iter().map(|(_, id, name)| (id, name)).collect()
}

/// The values of code type `type_id`: (id, value, description), in
/// dictionary order.
pub fn values(doc: &Document, type_id: i64) -> Vec<(i64, String, String)> {
    let Some(t) = doc.table("PCATVAL") else {
        return vec![];
    };
    let mut v: Vec<(i64, String, i64, String)> = t
        .rows
        .iter()
        .filter(|r| num(t.value(r, "proj_catg_type_id")) == type_id)
        .map(|r| {
            (
                num(t.value(r, "seq_num")),
                t.value(r, "proj_catg_short_name").to_string(),
                num(t.value(r, "proj_catg_id")),
                t.value(r, "proj_catg_name").to_string(),
            )
        })
        .collect();
    v.sort();
    v.into_iter()
        .map(|(_, short, id, name)| (id, short, name))
        .collect()
}

/// The code type of value `value_id`, if the value exists.
pub fn type_of(doc: &Document, value_id: i64) -> Option<i64> {
    let t = doc.table("PCATVAL")?;
    t.rows
        .iter()
        .find(|r| num(t.value(r, "proj_catg_id")) == value_id)
        .map(|r| num(t.value(r, "proj_catg_type_id")))
}

/// Project `pid`'s code assignments, in file order.
pub fn assigned(doc: &Document, pid: i64) -> Vec<Assigned> {
    let Some(t) = doc.table("PROJPCAT") else {
        return vec![];
    };
    t.rows
        .iter()
        .filter(|r| num(t.value(r, "proj_id")) == pid)
        .map(|r| {
            let type_id = num(t.value(r, "proj_catg_type_id"));
            let value_id = num(t.value(r, "proj_catg_id"));
            let v = |c: &str| records::lookup(doc, "PCATVAL", "proj_catg_id", value_id, c);
            Assigned {
                type_id,
                value_id,
                code_type: records::lookup(
                    doc,
                    "PCATTYPE",
                    "proj_catg_type_id",
                    type_id,
                    "proj_catg_type",
                ),
                value: v("proj_catg_short_name"),
                description: v("proj_catg_name"),
            }
        })
        .collect()
}

/// Give project `pid` code value `value_id`, in place of its value of the
/// same code type. Returns whether anything changed.
pub fn assign(doc: &mut Document, pid: i64, value_id: i64) -> R<bool> {
    let type_id = type_of(doc, value_id).ok_or("No such project code value.")?;
    let mine = |t: &crate::Table, r: &[String]| {
        num(t.value(r, "proj_id")) == pid && num(t.value(r, "proj_catg_type_id")) == type_id
    };
    let rows: Vec<usize> = doc.table("PROJPCAT").map_or(vec![], |t| {
        t.rows
            .iter()
            .enumerate()
            .filter(|(_, r)| mine(t, r))
            .map(|(i, _)| i)
            .collect()
    });
    if let [only] = rows.as_slice() {
        if num(&records::get(doc, "PROJPCAT", *only, "proj_catg_id")) == value_id {
            return Ok(false);
        }
    }
    for r in rows.into_iter().rev() {
        records::remove_row(doc, "PROJPCAT", r);
    }
    records::add_row(
        doc,
        "PROJPCAT",
        &[
            ("proj_id", pid.to_string()),
            ("proj_catg_type_id", type_id.to_string()),
            ("proj_catg_id", value_id.to_string()),
        ],
    );
    Ok(true)
}

/// Take code value `value_id` off project `pid`. Returns whether it had it.
pub fn remove(doc: &mut Document, pid: i64, value_id: i64) -> bool {
    let rows: Vec<usize> = doc.table("PROJPCAT").map_or(vec![], |t| {
        t.rows
            .iter()
            .enumerate()
            .filter(|(_, r)| {
                num(t.value(r, "proj_id")) == pid && num(t.value(r, "proj_catg_id")) == value_id
            })
            .map(|(i, _)| i)
            .collect()
    });
    let had = !rows.is_empty();
    for r in rows.into_iter().rev() {
        records::remove_row(doc, "PROJPCAT", r);
    }
    had
}

/// By name, for the command line: give the project the value `value` (its
/// short name) of the code type named `code_type`; `None` removes the
/// project's value of that type.
pub fn set_project_code(p: &mut ProjectData, code_type: &str, value: Option<&str>) -> R<()> {
    let pid = p.model.project.proj_id;
    let doc = &mut p.document;
    let types: Vec<i64> = code_types(doc)
        .into_iter()
        .filter(|(_, n)| n == code_type)
        .map(|(id, _)| id)
        .collect();
    let type_id = match types.as_slice() {
        [one] => *one,
        [] => return Err(format!("No project code {code_type:?}.")),
        _ => return Err(format!("Project code {code_type:?} is not unique.")),
    };
    match value {
        Some(short) => {
            let found: Vec<i64> = values(doc, type_id)
                .into_iter()
                .filter(|(_, s, _)| s == short)
                .map(|(id, _, _)| id)
                .collect();
            let [one] = found.as_slice() else {
                let what = format!("Project code {code_type:?} has no single value");
                return Err(format!("{what} {short:?}."));
            };
            assign(doc, pid, *one).map(|_| ())
        }
        None => {
            let current: Vec<i64> = assigned(doc, pid)
                .into_iter()
                .filter(|a| a.type_id == type_id)
                .map(|a| a.value_id)
                .collect();
            for id in current {
                remove(doc, pid, id);
            }
            Ok(())
        }
    }
}
