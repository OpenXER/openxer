//! Project funding: the `PROJFUND` rows that give a project its funding
//! sources (`FUNDSRC`, the enterprise dictionary), each with an amount
//! (`fund_cost`) and a fund share (`fund_wt`, a percentage). The Projects
//! view's Funding tab, the command line and the MCP server all edit funding
//! through these functions. A project lists a funding source once.
use super::records::{self, num};
use crate::{Document, ProjectData};

pub const TABLE: &str = "PROJFUND";
pub const SOURCES: &str = "FUNDSRC";

type R<T> = Result<T, String>;

/// A funding field a cell or an operation sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Source,
    Amount,
    Share,
}

impl Field {
    /// The field named by an operation: `source` (or `funding_source`),
    /// `amount` (or `fund_cost`), `share` (or `fund_share`, `fund_wt`).
    pub fn parse(name: &str) -> R<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "source" | "funding_source" | "fund_id" => Ok(Field::Source),
            "amount" | "fund_cost" => Ok(Field::Amount),
            "share" | "fund_share" | "fund_wt" => Ok(Field::Share),
            other => Err(format!(
                "Unknown funding field {other:?}; use source, amount or share."
            )),
        }
    }
}

/// One funding row of the project.
#[derive(Clone, Debug, PartialEq)]
pub struct Funding {
    /// Row of `PROJFUND`.
    pub row: usize,
    pub fund_id: i64,
    pub source: String,
    pub amount: f64,
    pub share: f64,
}

/// The funding sources of the dictionary, `(fund_id, fund_name)` in file order.
pub fn sources(doc: &Document) -> Vec<(i64, String)> {
    doc.table(SOURCES).map_or(vec![], |t| {
        t.rows
            .iter()
            .map(|r| {
                (
                    num(t.value(r, "fund_id")),
                    t.value(r, "fund_name").to_string(),
                )
            })
            .filter(|(id, _)| *id != 0)
            .collect()
    })
}

/// A funding source's id by name (exact, else ignoring case).
pub fn source_id(doc: &Document, name: &str) -> R<i64> {
    let all = sources(doc);
    let name = name.trim();
    let exact: Vec<i64> = all
        .iter()
        .filter(|(_, n)| n == name)
        .map(|(id, _)| *id)
        .collect();
    let found = if exact.is_empty() {
        all.iter()
            .filter(|(_, n)| n.eq_ignore_ascii_case(name))
            .map(|(id, _)| *id)
            .collect()
    } else {
        exact
    };
    match found.as_slice() {
        [id] => Ok(*id),
        [] => Err(format!(
            "No funding source {name:?}. Funding sources are listed in Enterprise > Funding Sources."
        )),
        _ => Err(format!("Funding source {name:?} is not unique.")),
    }
}

/// The project's funding rows, in file order.
pub fn list(p: &ProjectData) -> Vec<Funding> {
    let names = sources(&p.document);
    let pid = p.model.project.proj_id;
    p.document.table(TABLE).map_or(vec![], |t| {
        t.rows
            .iter()
            .enumerate()
            .filter(|(_, r)| num(t.value(r, "proj_id")) == pid)
            .map(|(row, r)| {
                let fund_id = num(t.value(r, "fund_id"));
                Funding {
                    row,
                    fund_id,
                    source: names
                        .iter()
                        .find(|(id, _)| *id == fund_id)
                        .map(|(_, n)| n.clone())
                        .unwrap_or_default(),
                    amount: records::float(t.value(r, "fund_cost")),
                    share: records::float(t.value(r, "fund_wt")),
                }
            })
            .collect()
    })
}

fn row_of(p: &ProjectData, fund_id: i64) -> Option<usize> {
    list(p)
        .into_iter()
        .find(|f| f.fund_id == fund_id)
        .map(|f| f.row)
}

/// A typed amount: digits with thousands separators, a currency sign or a
/// minus sign. Negative amounts are refused.
pub fn parse_amount(text: &str) -> R<f64> {
    let t = text.trim();
    let digits: String = t
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
        .collect();
    let v = digits
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite() && v.abs() < 1e13)
        .ok_or_else(|| format!("{t:?} is not an amount such as 25,000.00."))?;
    if v < 0. {
        return Err("A funding amount cannot be negative.".into());
    }
    Ok(v)
}

/// A typed fund share: a percentage from 0 to 100, with or without `%`.
pub fn parse_share(text: &str) -> R<f64> {
    let t = text.trim().trim_end_matches('%').trim();
    let v = t
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or_else(|| format!("{t:?} is not a fund share such as 50."))?;
    if !(0. ..=100.).contains(&v) {
        return Err("The fund share is a percentage from 0 to 100.".into());
    }
    Ok(v)
}

/// Adds funding from `fund_id` (amount and share zero). Returns its row.
pub fn add(p: &mut ProjectData, fund_id: i64) -> R<usize> {
    if !sources(&p.document).iter().any(|(id, _)| *id == fund_id) {
        return Err("That funding source is not in the dictionary.".into());
    }
    if row_of(p, fund_id).is_some() {
        return Err("The project already lists that funding source.".into());
    }
    let id = records::next_id(&p.document, TABLE, "proj_fund_id");
    let pid = p.model.project.proj_id;
    Ok(records::add_row(
        &mut p.document,
        TABLE,
        &[
            ("proj_fund_id", id.to_string()),
            ("proj_id", pid.to_string()),
            ("fund_id", fund_id.to_string()),
            ("fund_cost", "0".into()),
            ("fund_wt", "0".into()),
        ],
    ))
}

/// Sets one field of the funding at `row` from typed text. The source is a
/// funding source name.
pub fn set(p: &mut ProjectData, row: usize, field: Field, text: &str) -> R<()> {
    let Some(current) = list(p).into_iter().find(|f| f.row == row) else {
        return Err("That funding row is gone.".into());
    };
    let (column, value) = match field {
        Field::Source => {
            let id = source_id(&p.document, text)?;
            if id != current.fund_id && row_of(p, id).is_some() {
                return Err("The project already lists that funding source.".into());
            }
            ("fund_id", id.to_string())
        }
        Field::Amount => ("fund_cost", records::format_number(parse_amount(text)?)),
        Field::Share => ("fund_wt", records::format_number(parse_share(text)?)),
    };
    records::set(&mut p.document, TABLE, row, column, value);
    Ok(())
}

/// Removes the funding at `row`.
pub fn delete(p: &mut ProjectData, row: usize) -> R<()> {
    if !list(p).iter().any(|f| f.row == row) {
        return Err("That funding row is gone.".into());
    }
    records::remove_row(&mut p.document, TABLE, row);
    Ok(())
}

/// The row of the project's funding from the source named `source`.
pub fn row_by_source(p: &ProjectData, source: &str) -> R<usize> {
    let id = source_id(&p.document, source)?;
    row_of(p, id).ok_or_else(|| format!("The project is not funded by {source:?}."))
}

/// `add_project_funding`: funding from the named source, with an optional
/// amount and share.
pub fn add_by_source(
    p: &mut ProjectData,
    source: &str,
    amount: Option<&str>,
    share: Option<&str>,
) -> R<()> {
    // Check the values before anything is written.
    if let Some(a) = amount {
        parse_amount(a)?;
    }
    if let Some(s) = share {
        parse_share(s)?;
    }
    let id = source_id(&p.document, source)?;
    let row = add(p, id)?;
    if let Some(a) = amount {
        set(p, row, Field::Amount, a)?;
    }
    if let Some(s) = share {
        set(p, row, Field::Share, s)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> ProjectData {
        let mut p = crate::starter();
        records::add_row(
            &mut p.document,
            SOURCES,
            &[("fund_id", "7".into()), ("fund_name", "Capital".into())],
        );
        records::add_row(
            &mut p.document,
            SOURCES,
            &[("fund_id", "9".into()), ("fund_name", "Grant".into())],
        );
        p
    }

    #[test]
    fn funding_is_added_edited_and_deleted_by_source_name() {
        let mut p = project();
        add_by_source(&mut p, "capital", Some("25,000.00"), Some("40%")).unwrap();
        let rows = list(&p);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].source, "Capital");
        assert_eq!((rows[0].amount, rows[0].share), (25_000., 40.));
        assert!(
            add_by_source(&mut p, "Capital", None, None).is_err(),
            "listed once"
        );
        let row = row_by_source(&p, "Capital").unwrap();
        set(&mut p, row, Field::Source, "Grant").unwrap();
        assert_eq!(list(&p)[0].source, "Grant");
        assert!(set(&mut p, row, Field::Share, "120").is_err());
        assert!(set(&mut p, row, Field::Amount, "-5").is_err());
        assert!(set(&mut p, row, Field::Amount, "abc").is_err());
        assert_eq!(
            list(&p)[0].amount,
            25_000.,
            "a refused value writes nothing"
        );
        delete(&mut p, row).unwrap();
        assert!(list(&p).is_empty());
        assert!(row_by_source(&p, "Grant").is_err());
    }

    #[test]
    fn a_bad_value_adds_nothing_and_unknown_sources_are_refused() {
        let mut p = project();
        assert!(add_by_source(&mut p, "Capital", Some("x"), None).is_err());
        assert!(add_by_source(&mut p, "Nope", None, None).is_err());
        assert!(list(&p).is_empty());
        let t = p.document.table(TABLE);
        assert!(t.is_none_or(|t| t.rows.is_empty()));
    }

    #[test]
    fn other_projects_rows_are_left_alone() {
        let mut p = project();
        records::add_row(
            &mut p.document,
            TABLE,
            &[
                ("proj_fund_id", "1".into()),
                ("proj_id", "999999".into()),
                ("fund_id", "7".into()),
                ("fund_cost", "5".into()),
            ],
        );
        assert!(list(&p).is_empty());
        add_by_source(&mut p, "Capital", Some("10"), None).unwrap();
        assert_eq!(list(&p).len(), 1);
        assert_eq!(p.document.table(TABLE).unwrap().rows.len(), 2);
        assert_eq!(
            p.document
                .table(TABLE)
                .unwrap()
                .value(&p.document.table(TABLE).unwrap().rows[1], "proj_fund_id"),
            "2"
        );
    }
}
