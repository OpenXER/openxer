//! Record edits by identifier: resource limits, activity codes and the code
//! dictionary, notebooks, user defined field values, calendars, resources,
//! WBS fields, schedule options and project fields. Each writes the
//! project document only; the caller re-reads the project
//! (`ProjectData::from_document`) before scheduling, so every derived input
//! (calendars, leveling limits and priorities) comes from these rows the
//! same way an import reads them. Values are written as given.
use super::records::{self, num};
use crate::{Document, ProjectData};

type R<T> = Result<T, String>;

fn pid(p: &ProjectData) -> i64 {
    p.model.project.proj_id
}

fn find_row(
    doc: &Document,
    table: &str,
    test: impl Fn(&crate::Table, &[String]) -> bool,
) -> Vec<usize> {
    doc.table(table).map_or(vec![], |t| {
        t.rows
            .iter()
            .enumerate()
            .filter(|(_, r)| test(t, r))
            .map(|(i, _)| i)
            .collect()
    })
}

fn one(rows: Vec<usize>, what: &str) -> R<usize> {
    match rows.as_slice() {
        [r] => Ok(*r),
        [] => Err(format!("No {what}.")),
        _ => Err(format!("{what} is not unique.")),
    }
}

/// A resource's `rsrc_id` by short name.
pub fn resource_id(p: &ProjectData, short_name: &str) -> R<i64> {
    let rows = find_row(&p.document, "RSRC", |t, r| {
        t.value(r, "rsrc_short_name") == short_name
    });
    let row = one(rows, &format!("resource {short_name:?}"))?;
    Ok(num(&records::get(&p.document, "RSRC", row, "rsrc_id")))
}

/// A calendar's `clndr_id` by name. A name used twice is told apart with
/// " [project]", " [global]" or " [resource]".
pub fn calendar_id(p: &ProjectData, name: &str) -> R<String> {
    let (base, kind) = match name.rsplit_once(" [") {
        Some((b, k)) if k.ends_with(']') => (b, Some(&k[..k.len() - 1])),
        _ => (name, None),
    };
    let want = |t: &crate::Table, r: &[String]| {
        let typ = t.value(r, "clndr_type");
        t.value(r, "clndr_name") == base
            && match kind {
                None => true,
                Some("project") => typ == "CA_Project",
                Some("global") => typ == "CA_Base" || typ.is_empty(),
                Some(_) => typ == "CA_Rsrc",
            }
    };
    let rows = find_row(&p.document, "CALENDAR", want);
    let row = match rows.as_slice() {
        [r] => *r,
        [] => return Err(format!("No calendar {name:?}.")),
        many => {
            // Prefer this project's calendar, then a global one.
            let doc = &p.document;
            let pick = |typ: &str| {
                many.iter().copied().find(|&r| {
                    records::get(doc, "CALENDAR", r, "clndr_type") == typ
                        && (typ != "CA_Project"
                            || num(&records::get(doc, "CALENDAR", r, "proj_id")) == pid(p))
                })
            };
            pick("CA_Project")
                .or_else(|| pick("CA_Base"))
                .ok_or_else(|| format!("Calendar name {name:?} is not unique."))?
        }
    };
    Ok(records::get(&p.document, "CALENDAR", row, "clndr_id"))
}

/// A WBS element by path; the empty path is the project node.
fn wbs_id(p: &ProjectData, path: &str) -> R<i64> {
    if path.trim().is_empty() {
        return super::wbs::project_node(p)
            .ok_or_else(|| "The project has no project node.".into());
    }
    super::wbs::find_path(p, path).ok_or_else(|| format!("No WBS element {path:?}."))
}

/// The small JSON-like value the record edits take, so this crate needs no
/// JSON dependency.
pub mod serde_like {
    #[derive(Clone, Debug, PartialEq)]
    pub enum Value {
        Null,
        Text(String),
    }
    impl Value {
        pub fn text(&self) -> String {
            match self {
                Value::Null => String::new(),
                Value::Text(t) => t.clone(),
            }
        }
        pub fn is_null(&self) -> bool {
            matches!(self, Value::Null)
        }
    }
}
use serde_like::Value;

fn clean(v: &Value) -> String {
    records::clean(&v.text())
}

/// A key or parent-key column a raw record edit does not write, and what to
/// use instead.
type Refused = (&'static str, &'static str);

const KEY: &str = "is the row's key and cannot be changed";

/// The PROJECT columns of the format. OpenXER never creates the PROJECT
/// table, so it has no layout in `standard_fields`; this list only checks
/// raw project edits against a file that lacks a column.
const PROJECT_FIELDS: &[&str] = &[
    "proj_id",
    "fy_start_month_num",
    "rsrc_self_add_flag",
    "allow_complete_flag",
    "rsrc_multi_assign_flag",
    "checkout_flag",
    "project_flag",
    "step_complete_flag",
    "cost_qty_recalc_flag",
    "batch_sum_flag",
    "name_sep_char",
    "def_complete_pct_type",
    "proj_short_name",
    "acct_id",
    "orig_proj_id",
    "source_proj_id",
    "base_type_id",
    "clndr_id",
    "sum_base_proj_id",
    "task_code_base",
    "task_code_step",
    "priority_num",
    "wbs_max_sum_level",
    "strgy_priority_num",
    "last_checksum",
    "critical_drtn_hr_cnt",
    "def_cost_per_qty",
    "last_recalc_date",
    "plan_start_date",
    "plan_end_date",
    "scd_end_date",
    "add_date",
    "last_tasksum_date",
    "fcst_start_date",
    "def_duration_type",
    "task_code_prefix",
    "guid",
    "def_qty_type",
    "add_by_name",
    "web_local_root_path",
    "proj_url",
    "def_rate_type",
    "add_act_remain_flag",
    "act_this_per_link_flag",
    "def_task_type",
    "act_pct_link_flag",
    "critical_path_type",
    "task_code_prefix_flag",
    "def_rollup_dates_flag",
    "use_project_baseline_flag",
    "rem_target_link_flag",
    "reset_planned_flag",
    "allow_neg_act_flag",
    "sum_assign_level",
    "last_fin_dates_id",
    "last_baseline_update_date",
    "cr_external_key",
    "apply_actuals_date",
    "location_id",
    "loaded_scope_level",
    "export_flag",
    "new_fin_dates_id",
    "baselines_to_export",
    "baseline_names_to_export",
    "next_data_date",
    "close_period_flag",
    "sum_refresh_date",
    "trsrcsum_loaded",
    "sumtask_loaded",
];

/// Check that `field` may be written to `table` by a raw record edit: not a
/// key or parent key (`refused`), and a column the table holds or the format
/// lists for it (`named` are the edit's own field names). A misspelled name
/// would otherwise become a new column on every row of the saved file.
fn writable(
    doc: &Document,
    table: &str,
    field: &str,
    refused: &[Refused],
    named: &[&str],
) -> R<()> {
    if let Some((_, why)) = refused.iter().find(|(c, _)| *c == field) {
        return Err(format!("{table} field {field} {why}."));
    }
    let project: &[&str] = if table == "PROJECT" {
        PROJECT_FIELDS
    } else {
        &[]
    };
    let standard = || {
        crate::standard_fields(table)
            .unwrap_or(&[])
            .iter()
            .chain(records::standard_fields(table))
            .chain(project)
            .copied()
    };
    let held = doc.table(table).map_or(&[][..], |t| t.fields.as_slice());
    if named.contains(&field) || held.iter().any(|f| f == field) || standard().any(|f| f == field) {
        return Ok(());
    }
    let mut valid: std::collections::BTreeSet<&str> = named.iter().copied().collect();
    valid.extend(held.iter().map(String::as_str));
    valid.extend(standard());
    for (c, _) in refused {
        valid.remove(c);
    }
    Err(format!(
        "{field:?} is not a {table} field. Fields that can be written: {}.",
        valid.into_iter().collect::<Vec<_>>().join(", ")
    ))
}

/// Set one PROJECT column of the open project.
pub fn set_project_field(p: &mut ProjectData, field: &str, value: &Value) -> R<()> {
    let id = pid(p);
    let row = one(
        find_row(&p.document, "PROJECT", |t, r| {
            num(t.value(r, "proj_id")) == id
        }),
        "project row",
    )?;
    writable(
        &p.document,
        "PROJECT",
        field,
        &[("proj_id", KEY)],
        &["calendar"],
    )?;
    let v = match field {
        "calendar" => calendar_id(p, &value.text())?,
        _ => clean(value),
    };
    let column = if field == "calendar" {
        "clndr_id"
    } else {
        field
    };
    records::set(&mut p.document, "PROJECT", row, column, v);
    Ok(())
}

/// Set one SCHEDOPTIONS column of the open project (the row is added when
/// missing).
pub fn set_schedule_option(p: &mut ProjectData, field: &str, value: &Value) -> R<()> {
    writable(
        &p.document,
        "SCHEDOPTIONS",
        field,
        &[
            ("schedoptions_id", KEY),
            (
                "proj_id",
                "names the project the options belong to and cannot be changed",
            ),
        ],
        &[],
    )?;
    let id = pid(p);
    let rows = find_row(&p.document, "SCHEDOPTIONS", |t, r| {
        num(t.value(r, "proj_id")) == id
    });
    let row = match rows.as_slice() {
        [r] => *r,
        [] => {
            let next = records::next_id(&p.document, "SCHEDOPTIONS", "schedoptions_id");
            records::add_row(
                &mut p.document,
                "SCHEDOPTIONS",
                &[
                    ("schedoptions_id", next.to_string()),
                    ("proj_id", id.to_string()),
                ],
            )
        }
        _ => return Err("The project has several schedule option rows.".into()),
    };
    records::set(&mut p.document, "SCHEDOPTIONS", row, field, clean(value));
    Ok(())
}

/// One resource rate row (a limit from a date): `max_units_per_hour` and
/// the five prices. `remove` deletes the row.
pub fn set_resource_limit(
    p: &mut ProjectData,
    resource: &str,
    from: &str,
    shift_period: Option<&str>,
    values: &[(&str, Value)],
    remove: bool,
) -> R<()> {
    let rsrc = resource_id(p, resource)?;
    let shift = shift_period.unwrap_or("");
    let rows = find_row(&p.document, "RSRCRATE", |t, r| {
        num(t.value(r, "rsrc_id")) == rsrc
            && t.value(r, "start_date").starts_with(from.trim())
            && t.value(r, "shift_period_id") == shift
    });
    if remove {
        let row = one(rows, &format!("limit of {resource} from {from}"))?;
        records::remove_row(&mut p.document, "RSRCRATE", row);
        return Ok(());
    }
    let row = match rows.as_slice() {
        [r] => *r,
        [] => {
            let id = records::next_id(&p.document, "RSRCRATE", "rsrc_rate_id");
            let mut cells = vec![
                ("rsrc_rate_id", id.to_string()),
                ("rsrc_id", rsrc.to_string()),
                ("start_date", from.trim().to_string()),
            ];
            if !shift.is_empty() {
                cells.push(("shift_period_id", shift.to_string()));
            }
            records::add_row(&mut p.document, "RSRCRATE", &cells)
        }
        _ => return Err(format!("Limit of {resource} from {from} is not unique.")),
    };
    for (column, value) in values {
        records::set(&mut p.document, "RSRCRATE", row, column, clean(value));
    }
    Ok(())
}

fn code_type_id(p: &ProjectData, name: &str) -> R<i64> {
    let id = pid(p);
    let rows = find_row(&p.document, "ACTVTYPE", |t, r| {
        t.value(r, "actv_code_type") == name
            && (t.value(r, "proj_id").is_empty() || num(t.value(r, "proj_id")) == id)
    });
    let row = one(rows, &format!("activity code {name:?}"))?;
    Ok(num(&records::get(
        &p.document,
        "ACTVTYPE",
        row,
        "actv_code_type_id",
    )))
}

fn code_value_row(p: &ProjectData, type_id: i64, code: &str) -> R<usize> {
    one(
        find_row(&p.document, "ACTVCODE", |t, r| {
            num(t.value(r, "actv_code_type_id")) == type_id && t.value(r, "short_name") == code
        }),
        &format!("code value {code:?}"),
    )
}

/// Assign the code value `code` (by short name) of code type `code_type`
/// to an activity, replacing its value of that type; `None` removes it.
pub fn set_activity_code(
    p: &mut ProjectData,
    task_id: i64,
    code_type: &str,
    code: Option<&str>,
) -> R<()> {
    let type_id = code_type_id(p, code_type)?;
    match code {
        Some(code) => {
            let row = code_value_row(p, type_id, code)?;
            let id = num(&records::get(&p.document, "ACTVCODE", row, "actv_code_id"));
            super::activity::assign_code(p, task_id, type_id, id);
        }
        None => {
            let id = pid(p);
            let rows = find_row(&p.document, "TASKACTV", |t, r| {
                num(t.value(r, "task_id")) == task_id
                    && num(t.value(r, "actv_code_type_id")) == type_id
                    && (t.field("proj_id").is_none() || num(t.value(r, "proj_id")) == id)
            });
            for r in rows.into_iter().rev() {
                records::remove_row(&mut p.document, "TASKACTV", r);
            }
        }
    }
    Ok(())
}

/// Add an activity code type: `scope` is "project" (this project only),
/// "eps" (the projects under this project's EPS node) or "global";
/// `max_length` is the longest code value (default 60). Written as the
/// desktop's Activity Codes dictionary writes one (AS_Project with the
/// project id, AS_EPS with the EPS node id, AS_Global with neither).
pub fn add_code_type(
    p: &mut ProjectData,
    name: &str,
    scope: &str,
    max_length: Option<i64>,
) -> R<()> {
    let name = name.trim();
    if name.is_empty() {
        return Err("A code type needs a name.".into());
    }
    if name.chars().count() > 60 {
        return Err("A code type name can be at most 60 characters long.".into());
    }
    // Names are unique across every scope, ignoring case.
    let clash = !find_row(&p.document, "ACTVTYPE", |t, r| {
        t.value(r, "actv_code_type")
            .trim()
            .eq_ignore_ascii_case(name)
    })
    .is_empty();
    if clash {
        return Err(format!("Activity code {name:?} already exists."));
    }
    let id = pid(p);
    let (scope, proj, wbs) = match scope {
        "project" => ("AS_Project", id.to_string(), String::new()),
        "global" => ("AS_Global", String::new(), String::new()),
        "eps" => {
            // The EPS node is the parent of the project's own WBS node.
            let rows = find_row(&p.document, "PROJWBS", |t, r| {
                num(t.value(r, "proj_id")) == id && t.value(r, "proj_node_flag") == "Y"
            });
            let eps = rows
                .first()
                .map(|&r| records::get(&p.document, "PROJWBS", r, "parent_wbs_id"))
                .unwrap_or_default();
            ("AS_EPS", String::new(), eps)
        }
        other => {
            return Err(format!(
                "Unknown code type scope {other:?}; use project, eps or global."
            ))
        }
    };
    let len = max_length.unwrap_or(60);
    if !(1..=60).contains(&len) {
        return Err("max_length must be from 1 to 60.".into());
    }
    let seq = p.document.table("ACTVTYPE").map_or(0, |t| {
        t.rows
            .iter()
            .map(|r| num(t.value(r, "seq_num")))
            .max()
            .unwrap_or(0)
    }) + 1;
    let type_id = records::next_id(&p.document, "ACTVTYPE", "actv_code_type_id");
    records::add_row(
        &mut p.document,
        "ACTVTYPE",
        &[
            ("actv_code_type_id", type_id.to_string()),
            ("actv_short_len", len.to_string()),
            ("seq_num", seq.to_string()),
            ("actv_code_type", name.to_string()),
            ("proj_id", proj),
            ("wbs_id", wbs),
            ("actv_code_type_scope", scope.into()),
        ],
    );
    Ok(())
}

/// Add a code value to a code type's dictionary.
pub fn add_code_value(p: &mut ProjectData, code_type: &str, code: &str) -> R<()> {
    let type_id = code_type_id(p, code_type)?;
    if code_value_row(p, type_id, code).is_ok() {
        return Err(format!("Code value {code:?} already exists."));
    }
    let id = records::next_id(&p.document, "ACTVCODE", "actv_code_id");
    records::add_row(
        &mut p.document,
        "ACTVCODE",
        &[
            ("actv_code_id", id.to_string()),
            ("actv_code_type_id", type_id.to_string()),
            ("short_name", code.to_string()),
        ],
    );
    Ok(())
}

/// Set one ACTVCODE column; `parent` takes the parent's short name.
pub fn set_code_value(
    p: &mut ProjectData,
    code_type: &str,
    code: &str,
    field: &str,
    value: &Value,
) -> R<()> {
    let type_id = code_type_id(p, code_type)?;
    let row = code_value_row(p, type_id, code)?;
    writable(
        &p.document,
        "ACTVCODE",
        field,
        &[
            ("actv_code_id", KEY),
            (
                "actv_code_type_id",
                "names the code type the value belongs to and cannot be changed",
            ),
            (
                "parent_actv_code_id",
                "is set through the field parent (the parent value's short name)",
            ),
        ],
        &["parent"],
    )?;
    let (column, v) = match field {
        "parent" if value.is_null() => ("parent_actv_code_id", String::new()),
        "parent" => {
            let parent = code_value_row(p, type_id, &value.text())?;
            (
                "parent_actv_code_id",
                records::get(&p.document, "ACTVCODE", parent, "actv_code_id"),
            )
        }
        other => (other, clean(value)),
    };
    records::set(&mut p.document, "ACTVCODE", row, column, v);
    Ok(())
}

/// Delete a code value and its activity assignments.
pub fn delete_code_value(p: &mut ProjectData, code_type: &str, code: &str) -> R<()> {
    let type_id = code_type_id(p, code_type)?;
    let row = code_value_row(p, type_id, code)?;
    let id = num(&records::get(&p.document, "ACTVCODE", row, "actv_code_id"));
    records::remove_row(&mut p.document, "ACTVCODE", row);
    let used = find_row(&p.document, "TASKACTV", |t, r| {
        num(t.value(r, "actv_code_id")) == id
    });
    for r in used.into_iter().rev() {
        records::remove_row(&mut p.document, "TASKACTV", r);
    }
    Ok(())
}

/// The notebook topic id by name, added (for activities and WBS) when new.
fn memo_type_id(p: &mut ProjectData, topic: &str) -> i64 {
    let rows = find_row(&p.document, "MEMOTYPE", |t, r| {
        t.value(r, "memo_type") == topic
    });
    if let Some(&r) = rows.first() {
        return num(&records::get(&p.document, "MEMOTYPE", r, "memo_type_id"));
    }
    let id = records::next_id(&p.document, "MEMOTYPE", "memo_type_id");
    records::add_row(
        &mut p.document,
        "MEMOTYPE",
        &[
            ("memo_type_id", id.to_string()),
            ("seq_num", id.to_string()),
            ("eps_flag", "N".into()),
            ("proj_flag", "N".into()),
            ("wbs_flag", "Y".into()),
            ("task_flag", "Y".into()),
            ("memo_type", topic.to_string()),
        ],
    );
    id
}

/// Set (or with `None` remove) an activity's notebook topic text (HTML as
/// stored).
pub fn set_notebook(p: &mut ProjectData, task_id: i64, topic: &str, text: Option<&str>) -> R<()> {
    memo(
        p,
        "TASKMEMO",
        "task_id",
        "memo_id",
        "task_memo",
        task_id,
        topic,
        text,
    )
}

/// Set (or remove) a WBS element's notebook topic text.
pub fn set_wbs_notebook(p: &mut ProjectData, wbs: &str, topic: &str, text: Option<&str>) -> R<()> {
    let id = wbs_id(p, wbs)?;
    memo(
        p,
        "WBSMEMO",
        "wbs_id",
        "wbs_memo_id",
        "wbs_memo",
        id,
        topic,
        text,
    )
}

#[allow(clippy::too_many_arguments)]
fn memo(
    p: &mut ProjectData,
    table: &str,
    owner: &str,
    key: &str,
    column: &str,
    owner_id: i64,
    topic: &str,
    text: Option<&str>,
) -> R<()> {
    let type_id = memo_type_id(p, topic);
    let id = pid(p);
    let rows = find_row(&p.document, table, |t, r| {
        num(t.value(r, owner)) == owner_id && num(t.value(r, "memo_type_id")) == type_id
    });
    match (rows.first().copied(), text) {
        (Some(r), Some(text)) => records::set(&mut p.document, table, r, column, text.to_string()),
        (Some(_), None) => {
            for r in rows.into_iter().rev() {
                records::remove_row(&mut p.document, table, r);
            }
        }
        (None, Some(text)) => {
            let next = records::next_id(&p.document, table, key);
            records::add_row(
                &mut p.document,
                table,
                &[
                    (key, next.to_string()),
                    ("proj_id", id.to_string()),
                    (owner, owner_id.to_string()),
                    ("memo_type_id", type_id.to_string()),
                    (column, text.to_string()),
                ],
            );
        }
        (None, None) => {}
    }
    Ok(())
}

/// Set (or with null remove) a user defined field value by its label, on a
/// record of any subject area (`TASK`, `RSRC`, `PROJWBS`, `PROJECT`,
/// `TASKRSRC`, `PROJCOST`, `TASKPROC`, `PROJISSU`, `PROJRISK`, `DOCUMENT`;
/// `owner_id` is the record's key). The value goes to the column the field's
/// type uses; a new value of a project's record carries the project.
pub fn set_udf(
    p: &mut ProjectData,
    table: &str,
    owner_id: i64,
    label: &str,
    value: &Value,
) -> R<()> {
    let rows = find_row(&p.document, "UDFTYPE", |t, r| {
        t.value(r, "table_name").eq_ignore_ascii_case(table)
            && (t.value(r, "udf_type_label") == label || t.value(r, "udf_type_name") == label)
    });
    let row = one(rows, &format!("user defined field {label:?}"))?;
    let type_id = num(&records::get(&p.document, "UDFTYPE", row, "udf_type_id"));
    let logical = records::get(&p.document, "UDFTYPE", row, "logical_data_type");
    let column = match super::udf::udf_kind(&logical) {
        super::udf::UdfKind::Number => "udf_number",
        super::udf::UdfKind::Date => "udf_date",
        super::udf::UdfKind::ReadOnly if logical == "FT_STATICTYPE" => "udf_text",
        super::udf::UdfKind::ReadOnly => "udf_code_id",
        _ => "udf_text",
    };
    let existing = find_row(&p.document, "UDFVALUE", |t, r| {
        num(t.value(r, "udf_type_id")) == type_id && num(t.value(r, "fk_id")) == owner_id
    });
    if value.is_null() {
        for r in existing.into_iter().rev() {
            records::remove_row(&mut p.document, "UDFVALUE", r);
        }
        return Ok(());
    }
    let mut v = clean(value);
    if super::udf::is_code(&logical) && v.trim().parse::<i64>().is_err() {
        // A code-list value named by its code.
        let code = super::udf::udf_codes(&p.document, type_id)
            .into_iter()
            .find(|(_, short, _)| short.eq_ignore_ascii_case(v.trim()))
            .ok_or_else(|| format!("{label} has no code {v:?}."))?;
        v = code.0.to_string();
    }
    match existing.first() {
        Some(&r) => records::set(&mut p.document, "UDFVALUE", r, column, v),
        None => {
            let mut cells = vec![
                ("udf_type_id", type_id.to_string()),
                ("fk_id", owner_id.to_string()),
                (column, v),
            ];
            if super::udf::project_area(table) {
                cells.push(("proj_id", pid(p).to_string()));
            }
            records::add_row(&mut p.document, "UDFVALUE", &cells);
        }
    }
    Ok(())
}

/// Add a calendar (a project calendar of the open project until its fields
/// say otherwise) with an empty workweek.
pub fn add_calendar(p: &mut ProjectData, name: &str) -> R<()> {
    if calendar_id(p, name).is_ok() {
        return Err(format!("Calendar {name:?} already exists."));
    }
    let id = records::next_id(&p.document, "CALENDAR", "clndr_id");
    let project = pid(p);
    let week: String = (1..=7).map(|d| format!("(0||{d}()())")).collect();
    records::add_row(
        &mut p.document,
        "CALENDAR",
        &[
            ("clndr_id", id.to_string()),
            ("default_flag", "N".into()),
            ("clndr_name", name.to_string()),
            ("proj_id", project.to_string()),
            ("clndr_type", "CA_Project".into()),
            (
                "clndr_data",
                format!("(0||CalendarData()((0||DaysOfWeek()({week}))(0||Exceptions()())))"),
            ),
        ],
    );
    Ok(())
}

/// Set one calendar field: CALENDAR columns, `base_calendar` (a name),
/// `project_calendar` (Y: this project's, N: none). `other_data` (the
/// calendar text besides its work periods) is not written: work periods
/// come from calendar_day and calendar_weekday.
pub fn set_calendar(p: &mut ProjectData, name: &str, field: &str, value: &Value) -> R<bool> {
    let id = calendar_id(p, name)?;
    let row = one(
        find_row(&p.document, "CALENDAR", |t, r| t.value(r, "clndr_id") == id),
        "calendar row",
    )?;
    if matches!(field, "other_data" | "clndr_data" | "clndr_id") {
        return Ok(false);
    }
    writable(
        &p.document,
        "CALENDAR",
        field,
        &[
            (
                "base_clndr_id",
                "is set through the field base_calendar (a calendar name)",
            ),
            (
                "proj_id",
                "is set through the field project_calendar (Y or N)",
            ),
        ],
        &["base_calendar", "project_calendar"],
    )?;
    let (column, v) = match field {
        "base_calendar" if value.is_null() => ("base_clndr_id", String::new()),
        "base_calendar" => ("base_clndr_id", calendar_id(p, &value.text())?),
        "project_calendar" => (
            "proj_id",
            if value.text() == "Y" {
                pid(p).to_string()
            } else {
                String::new()
            },
        ),
        other => (other, clean(value)),
    };
    records::set(&mut p.document, "CALENDAR", row, column, v);
    Ok(true)
}

/// Delete a calendar no activity uses.
pub fn delete_calendar(p: &mut ProjectData, name: &str) -> R<()> {
    let id = calendar_id(p, name)?;
    super::calendars::delete(p, &id)
}

/// Delete a calendar and move everything that uses it to the global
/// calendar `replacement` (a name), or to the default global calendar when
/// `replacement` is `None`. Returns how many activities moved.
pub fn delete_calendar_replacing(
    p: &mut ProjectData,
    name: &str,
    replacement: Option<&str>,
) -> R<usize> {
    let id = calendar_id(p, name)?;
    let to = match replacement {
        Some(other) => calendar_id(p, other)?,
        None => super::calendars::default_global(&p.document)
            .ok_or("There is no default global calendar to move the users to.")?,
    };
    super::calendars::delete_replacing(p, &id, &to)
}

/// Add a resource by short name.
pub fn add_resource(p: &mut ProjectData, short_name: &str) -> R<()> {
    if resource_id(p, short_name).is_ok() {
        return Err(format!("Resource {short_name:?} already exists."));
    }
    let id = records::next_id(&p.document, "RSRC", "rsrc_id");
    records::add_row(
        &mut p.document,
        "RSRC",
        &[
            ("rsrc_id", id.to_string()),
            ("rsrc_short_name", short_name.to_string()),
            ("rsrc_name", short_name.to_string()),
        ],
    );
    Ok(())
}

/// Set one resource field: RSRC columns, `parent` (short name) or
/// `calendar` (name).
pub fn set_resource(p: &mut ProjectData, short_name: &str, field: &str, value: &Value) -> R<()> {
    let id = resource_id(p, short_name)?;
    let row = one(
        find_row(&p.document, "RSRC", |t, r| num(t.value(r, "rsrc_id")) == id),
        "resource row",
    )?;
    writable(
        &p.document,
        "RSRC",
        field,
        &[
            ("rsrc_id", KEY),
            (
                "parent_rsrc_id",
                "is set through the field parent (the parent resource's short name)",
            ),
        ],
        &["parent", "calendar"],
    )?;
    let (column, v) = match field {
        "parent" if value.is_null() => ("parent_rsrc_id", String::new()),
        "parent" => ("parent_rsrc_id", resource_id(p, &value.text())?.to_string()),
        "calendar" if value.is_null() => ("clndr_id", String::new()),
        "calendar" => ("clndr_id", calendar_id(p, &value.text())?),
        other => (other, clean(value)),
    };
    records::set(&mut p.document, "RSRC", row, column, v);
    Ok(())
}

/// Set one PROJWBS column of the element at `path`.
pub fn set_wbs_field(p: &mut ProjectData, path: &str, field: &str, value: &Value) -> R<()> {
    let id = wbs_id(p, path)?;
    let row = one(
        find_row(&p.document, "PROJWBS", |t, r| {
            num(t.value(r, "wbs_id")) == id
        }),
        "WBS row",
    )?;
    writable(
        &p.document,
        "PROJWBS",
        field,
        &[
            ("wbs_id", KEY),
            (
                "parent_wbs_id",
                "places the element in the WBS; move it with shift_wbs instead",
            ),
            (
                "proj_id",
                "names the element's project and cannot be changed",
            ),
        ],
        &[],
    )?;
    records::set(&mut p.document, "PROJWBS", row, field, clean(value));
    Ok(())
}

/// Write a relationship row's comments.
pub fn set_relationship_comments(
    p: &mut ProjectData,
    pred: i64,
    succ: i64,
    kind: &str,
    comments: &str,
) -> R<()> {
    let row = one(
        find_row(&p.document, "TASKPRED", |t, r| {
            num(t.value(r, "pred_task_id")) == pred
                && num(t.value(r, "task_id")) == succ
                && t.value(r, "pred_type") == kind
        }),
        "relationship row",
    )?;
    records::set(
        &mut p.document,
        "TASKPRED",
        row,
        "comments",
        records::clean(comments),
    );
    Ok(())
}
