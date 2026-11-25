//! Deleting financial periods (`FINDATES`). A period that holds stored
//! period performance (`TASKFIN` or `TRSRCFIN` rows) is kept: the delete
//! is refused before anything changes, so the stored history cannot be
//! lost by accident. The desktop's Financial Period Calendars dialog, the
//! command line and the MCP server all delete periods through here.
use super::records::num;
use crate::workspace::tree;
use crate::{Document, ProjectData};

type R<T> = Result<T, String>;

/// The tables that hold stored period performance.
pub const STORED: [&str; 2] = ["TASKFIN", "TRSRCFIN"];

/// Rows of stored period performance (activities and assignments) that
/// refer to any of the periods.
pub fn stored_rows(doc: &Document, ids: &[i64]) -> usize {
    STORED
        .iter()
        .map(|t| tree::uses(doc, t, "fin_dates_id", ids))
        .sum()
}

/// The message given when a delete is refused.
pub fn refusal(periods: usize) -> String {
    if periods > 1 {
        "Stored period performance of activities or assignments refers to at least one of \
         these periods, so none of them is removed. Only periods without stored values can \
         be deleted."
            .into()
    } else {
        "Stored period performance of activities or assignments refers to this period, so it \
         is kept. Only a period without stored values can be deleted."
            .into()
    }
}

/// Delete the periods, all or none: refused when any of them holds stored
/// period performance. A project whose last stored period is deleted keeps
/// no last period.
pub fn delete(doc: &mut Document, ids: &[i64]) -> R<()> {
    if stored_rows(doc, ids) > 0 {
        return Err(refusal(ids.len()));
    }
    tree::remove_uses(doc, "FINDATES", "fin_dates_id", ids);
    tree::retarget(doc, "PROJECT", "last_fin_dates_id", ids, None);
    Ok(())
}

/// The period named `name` (exact, else ignoring case), within the
/// calendar `calendar` (by name) when one is given.
pub fn find(doc: &Document, name: &str, calendar: Option<&str>) -> R<i64> {
    let Some(t) = doc.table("FINDATES") else {
        return Err("The file has no financial periods.".into());
    };
    let calendar_id = match calendar {
        None => None,
        Some(c) => {
            let c = c.trim();
            let found = doc.table("FINTMPL").and_then(|ct| {
                ct.rows
                    .iter()
                    .find(|r| ct.value(r, "fintmpl_name") == c)
                    .or_else(|| {
                        ct.rows
                            .iter()
                            .find(|r| ct.value(r, "fintmpl_name").eq_ignore_ascii_case(c))
                    })
                    .map(|r| num(ct.value(r, "fintmpl_id")))
            });
            Some(found.ok_or_else(|| format!("No financial period calendar named {c:?}."))?)
        }
    };
    let name = name.trim();
    let in_calendar = |r: &Vec<String>| {
        calendar_id
            .is_none_or(|c| t.field("fintmpl_id").is_none() || num(t.value(r, "fintmpl_id")) == c)
    };
    let matching = |exact: bool| -> Vec<i64> {
        t.rows
            .iter()
            .filter(|r| in_calendar(r))
            .filter(|r| {
                let n = t.value(r, "fin_dates_name");
                if exact {
                    n == name
                } else {
                    n.eq_ignore_ascii_case(name)
                }
            })
            .map(|r| num(t.value(r, "fin_dates_id")))
            .collect()
    };
    let mut ids = matching(true);
    if ids.is_empty() {
        ids = matching(false);
    }
    match ids.as_slice() {
        [id] => Ok(*id),
        [] => Err(format!("No financial period named {name:?}.")),
        _ => Err(format!(
            "{} financial periods are named {name:?}; name the calendar too.",
            ids.len()
        )),
    }
}

/// Delete one period by name (the `delete_financial_period` operation).
pub fn delete_by_name(p: &mut ProjectData, name: &str, calendar: Option<&str>) -> R<()> {
    let id = find(&p.document, name, calendar)?;
    delete(&mut p.document, &[id])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc() -> Document {
        let mut p = crate::starter();
        let d = &mut p.document;
        super::super::records::add_row(
            d,
            "FINTMPL",
            &[("fintmpl_id", "1".into()), ("fintmpl_name", "Weeks".into())],
        );
        for (id, name) in [(10, "2026-11-22"), (11, "2026-11-29")] {
            super::super::records::add_row(
                d,
                "FINDATES",
                &[
                    ("fin_dates_id", id.to_string()),
                    ("fintmpl_id", "1".into()),
                    ("fin_dates_name", name.into()),
                ],
            );
        }
        super::super::records::add_row(
            d,
            "TASKFIN",
            &[("task_id", "1".into()), ("fin_dates_id", "10".into())],
        );
        p.document
    }

    #[test]
    fn a_period_with_stored_values_is_kept_and_an_empty_one_goes() {
        let mut d = doc();
        let periods = |d: &Document| d.table("FINDATES").map_or(0, |t| t.rows.len());
        let e = delete(&mut d, &[10]).unwrap_err();
        assert!(e.contains("kept"), "{e}");
        assert_eq!(periods(&d), 2, "nothing changed");
        assert_eq!(stored_rows(&d, &[10]), 1);
        // All or none: with a stored period in the selection nothing goes.
        assert!(delete(&mut d, &[10, 11]).is_err());
        assert_eq!(periods(&d), 2);
        delete(&mut d, &[11]).unwrap();
        assert_eq!(periods(&d), 1);
        assert!(find(&d, "2026-11-29", None).is_err());
        assert_eq!(find(&d, "2026-11-22", Some("weeks")).unwrap(), 10);
        // Assignment rows count as stored values too.
        super::super::records::remove_row(&mut d, "TASKFIN", 0);
        super::super::records::add_row(
            &mut d,
            "TRSRCFIN",
            &[("taskrsrc_id", "5".into()), ("fin_dates_id", "10".into())],
        );
        assert!(delete(&mut d, &[10]).is_err());
    }
}
