//! `workspace` commands on an OpenXER workspace database: `migrate` (see
//! `storage::workspace_migrate`), `dictionary` (list the shared workspace
//! dictionary), `dictionary-usage` (which projects use one of its rows) and
//! `preference` (the User Preferences, Assistance switches, see
//! `user_prefs`).
//! The command line and the MCP tools come from the same definitions here.
use crate::storage::{self, invalid};
use crate::workspace_db;
use crate::{AutomationError, Result};
use openxer_xer::workspace_scope::{self, merge::display_name};
use openxer_xer::Table;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::path::Path;

/// Plain names a person may give for a dictionary table, with the table.
const KINDS: &[(&str, &str)] = &[
    ("calendar", "CALENDAR"),
    ("calendars", "CALENDAR"),
    ("resource", "RSRC"),
    ("resources", "RSRC"),
    ("role", "ROLES"),
    ("roles", "ROLES"),
    ("activity_code", "ACTVTYPE"),
    ("activity_codes", "ACTVTYPE"),
    ("code", "ACTVTYPE"),
    ("codes", "ACTVTYPE"),
    ("code_value", "ACTVCODE"),
    ("code_values", "ACTVCODE"),
    ("udf", "UDFTYPE"),
    ("udfs", "UDFTYPE"),
    ("user_defined_field", "UDFTYPE"),
    ("account", "ACCOUNT"),
    ("accounts", "ACCOUNT"),
    ("cost_account", "ACCOUNT"),
    ("obs", "OBS"),
    ("currency", "CURRTYPE"),
    ("currencies", "CURRTYPE"),
];

/// The dictionary table `name` names (a table name or a plain kind such as
/// `calendar`).
pub fn table_of(name: &str) -> Result<&'static str> {
    let key = name.trim().to_ascii_lowercase().replace([' ', '-'], "_");
    if let Some((_, t)) = KINDS.iter().find(|(k, _)| *k == key) {
        return Ok(t);
    }
    workspace_scope::WORKSPACE_TABLES
        .iter()
        .map(|(t, _)| *t)
        .find(|t| t.eq_ignore_ascii_case(name.trim()))
        .ok_or_else(|| {
            AutomationError::new(
                "invalid_arguments",
                format!("{name:?} is not a workspace dictionary table; use calendar, resource, role, activity_code, code_value, udf, account or a table name such as RSRCRATE"),
            )
        })
}

/// Choices for an import's dictionary rows whose name the workspace has
/// with different content: `KIND=copy|keep|update,...`.
pub fn parse_choices(text: &str) -> Result<workspace_db::Choices> {
    let mut out = workspace_db::Choices::new();
    for part in text.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let (kind, choice) = part.split_once('=').ok_or_else(|| {
            AutomationError::new(
                "invalid_arguments",
                format!("{part:?}: write KIND=copy, KIND=keep or KIND=update"),
            )
        })?;
        let choice =
            openxer_xer::workspace_scope::merge::Choice::parse(choice).ok_or_else(|| {
                AutomationError::new(
                    "invalid_arguments",
                    format!("{choice:?} is not copy, keep or update"),
                )
            })?;
        out.insert(table_of(kind)?.to_string(), choice);
    }
    Ok(out)
}

fn shared_database(input: &Path) -> Result<rusqlite::Connection> {
    if !storage::is_database(input)? {
        return Err(AutomationError::new(
            "invalid_arguments",
            "workspace dictionary needs an OpenXER workspace database (.db)",
        ));
    }
    let c = storage::read_database(input)?;
    if !storage::shared(&c)? {
        return Err(AutomationError::new(
            "workspace_needs_migration",
            "This workspace keeps calendars, resources and codes inside each project (workspace schema 1); `workspace migrate` converts it to a shared dictionary.",
        ));
    }
    Ok(c)
}

/// The name of a dictionary row as people know it.
fn row_name(t: &Table, r: &[String]) -> String {
    match t.name.to_ascii_uppercase().as_str() {
        "ACTVCODE" => t.value(r, "short_name").to_string(),
        "UDFTYPE" => t.value(r, "udf_type_label").to_string(),
        _ => display_name(t, r),
    }
}

fn row_json(t: &Table, r: &[String], used: &BTreeMap<(String, i64), Vec<i64>>) -> Value {
    let table = t.name.to_ascii_uppercase();
    let id = workspace_scope::key_column(&table)
        .and_then(|k| t.field(k))
        .and_then(|k| r[k].trim().parse::<i64>().ok());
    let mut v = json!({"name": row_name(t, r)});
    if let Some(id) = id {
        v["id"] = json!(id);
        v["used_by"] = json!(used.get(&(table.clone(), id)).cloned().unwrap_or_default());
    }
    if table == "ACTVTYPE" {
        v["scope"] = json!(match t.value(r, "actv_code_type_scope") {
            "AS_EPS" => "eps",
            _ => "global",
        });
        if t.value(r, "actv_code_type_scope") == "AS_EPS" {
            v["eps_node"] = json!(t.value(r, "wbs_id"));
        }
    }
    if table == "CALENDAR" {
        v["type"] = json!(t.value(r, "clndr_type"));
        v["default"] = json!(t.value(r, "default_flag") == "Y");
    }
    if table == "ACTVCODE" {
        v["code_type_id"] = json!(t.value(r, "actv_code_type_id"));
    }
    v
}

/// `workspace dictionary`: the shared dictionary's rows (optionally of one
/// table), each with the workspace projects that use it.
pub fn dictionary(input: &Path, table: Option<&str>) -> Result<Value> {
    let c = shared_database(input)?;
    let only = table.map(table_of).transpose()?;
    let (dict, revision) = workspace_db::read_dictionary(&c).map_err(invalid)?;
    let users: BTreeMap<(String, i64), Vec<i64>> = workspace_db::users(&c, None)
        .map_err(invalid)?
        .into_iter()
        .map(|(k, v)| (k, v.into_iter().collect()))
        .collect();
    let mut tables = vec![];
    for t in &dict.tables {
        let name = t.name.to_ascii_uppercase();
        if only.is_some_and(|o| o != name) || t.rows.is_empty() {
            continue;
        }
        tables.push(json!({
            "table": name,
            "kind": workspace_db::kind_label(&name),
            "count": t.rows.len(),
            "rows": t.rows.iter().map(|r| row_json(t, r, &users)).collect::<Vec<_>>(),
        }));
    }
    Ok(json!({
        "input": input,
        "workspace_schema": workspace_db::DICTIONARY_SCHEMA,
        "revision": revision,
        "tables": tables,
    }))
}

/// `workspace dictionary-usage`: the projects that use one dictionary row,
/// named by id or by name.
pub fn dictionary_usage(
    input: &Path,
    table: &str,
    id: Option<i64>,
    name: Option<&str>,
) -> Result<Value> {
    let c = shared_database(input)?;
    let table = table_of(table)?;
    let (dict, _) = workspace_db::read_dictionary(&c).map_err(invalid)?;
    let t = dict.table(table);
    let key = workspace_scope::key_column(table).ok_or_else(|| {
        AutomationError::new(
            "invalid_arguments",
            format!("{table} rows have no id of their own; ask about their owner row"),
        )
    })?;
    let found: Vec<(i64, String)> = t
        .map(|t| {
            let k = t.field(key);
            t.rows
                .iter()
                .filter_map(|r| {
                    let rid = k.and_then(|k| r[k].trim().parse::<i64>().ok())?;
                    let n = row_name(t, r);
                    let hit = match (id, name) {
                        (Some(i), _) => i == rid,
                        (None, Some(n2)) => n.eq_ignore_ascii_case(n2.trim()),
                        (None, None) => false,
                    };
                    hit.then_some((rid, n))
                })
                .collect()
        })
        .unwrap_or_default();
    if id.is_none() && name.is_none() {
        return Err(AutomationError::new(
            "invalid_arguments",
            "dictionary-usage needs id or name",
        ));
    }
    let (rid, rname) = match found.as_slice() {
        [one] => one.clone(),
        [] => {
            return Err(AutomationError::new(
                "not_found",
                format!("No {table} row matches in the workspace dictionary"),
            ))
        }
        _ => {
            return Err(AutomationError::new(
                "ambiguous",
                format!("Several {table} rows have that name; pass id"),
            ))
        }
    };
    let used = workspace_db::dictionary_usage(&c, table, rid, None).map_err(invalid)?;
    Ok(json!({
        "input": input,
        "table": table,
        "kind": workspace_db::kind_label(table),
        "id": rid,
        "name": rname,
        "used_by": used.iter().map(|(w, n)| json!({"workspace_id": w, "name": n})).collect::<Vec<_>>(),
        "in_use": !used.is_empty(),
    }))
}

/// The `workspace` command line: `migrate`, `dictionary`,
/// `dictionary-usage`. Returns the request for the shared dispatcher.
pub fn parse_cli(
    args: &[String],
    flags: impl Fn(&[String], &[&str]) -> Result<BTreeMap<String, String>>,
) -> Result<Value> {
    let sub = args.first().map(String::as_str).unwrap_or("");
    let rest = args.get(1..).unwrap_or(&[]);
    let mut request = Map::new();
    let input = |fields: &BTreeMap<String, String>| {
        fields.get("input").cloned().ok_or_else(|| {
            AutomationError::new(
                "invalid_arguments",
                format!("workspace {sub} requires --input"),
            )
        })
    };
    match sub {
        "migrate" => {
            let fields = flags(rest, &["input", "output", "dry-run"])?;
            request.insert("command".into(), Value::from("workspace_migrate"));
            request.insert("input".into(), Value::from(input(&fields)?));
            if let Some(output) = fields.get("output") {
                request.insert("output".into(), Value::from(output.as_str()));
            }
            let dry_run = match fields.get("dry-run").map(String::as_str) {
                None | Some("false") => false,
                Some("true") => true,
                Some(other) => {
                    return Err(AutomationError::new(
                        "invalid_arguments",
                        format!("--dry-run takes true or false, not {other:?}"),
                    ))
                }
            };
            if dry_run == fields.contains_key("output") {
                return Err(AutomationError::new(
                    "invalid_arguments",
                    "workspace migrate requires exactly one of --output or --dry-run",
                ));
            }
            request.insert("dry_run".into(), Value::from(dry_run));
        }
        "dictionary" => {
            let fields = flags(rest, &["input", "table"])?;
            request.insert("command".into(), Value::from("workspace_dictionary"));
            request.insert("input".into(), Value::from(input(&fields)?));
            if let Some(t) = fields.get("table") {
                request.insert("table".into(), Value::from(t.as_str()));
            }
        }
        "dictionary-usage" => {
            let fields = flags(rest, &["input", "table", "id", "name"])?;
            request.insert("command".into(), Value::from("workspace_dictionary_usage"));
            request.insert("input".into(), Value::from(input(&fields)?));
            let table = fields.get("table").ok_or_else(|| {
                AutomationError::new(
                    "invalid_arguments",
                    "workspace dictionary-usage requires --table",
                )
            })?;
            request.insert("table".into(), Value::from(table.as_str()));
            if let Some(id) = fields.get("id") {
                let id: i64 = id.parse().map_err(|_| {
                    AutomationError::new(
                        "invalid_arguments",
                        format!("--id {id:?} is not an integer"),
                    )
                })?;
                request.insert("id".into(), Value::from(id));
            }
            if let Some(n) = fields.get("name") {
                request.insert("name".into(), Value::from(n.as_str()));
            }
            if !fields.contains_key("id") && !fields.contains_key("name") {
                return Err(AutomationError::new(
                    "invalid_arguments",
                    "workspace dictionary-usage requires --id or --name",
                ));
            }
        }
        "preference" => {
            let fields = flags(rest, &["input", "name", "value", "output", "dry-run"])?;
            request.insert("command".into(), Value::from("workspace_preference"));
            request.insert("input".into(), Value::from(input(&fields)?));
            let flag = |key: &str| -> Result<Option<bool>> {
                match fields.get(key).map(String::as_str) {
                    None => Ok(None),
                    Some("true") => Ok(Some(true)),
                    Some("false") => Ok(Some(false)),
                    Some(other) => Err(AutomationError::new(
                        "invalid_arguments",
                        format!("--{key} takes true or false, not {other:?}"),
                    )),
                }
            };
            if let Some(n) = fields.get("name") {
                request.insert("name".into(), Value::from(n.as_str()));
            }
            if let Some(v) = flag("value")? {
                request.insert("value".into(), Value::from(v));
            }
            if let Some(output) = fields.get("output") {
                request.insert("output".into(), Value::from(output.as_str()));
            }
            request.insert(
                "dry_run".into(),
                Value::from(flag("dry-run")?.unwrap_or(false)),
            );
        }
        _ => {
            return Err(AutomationError::new(
                "invalid_arguments",
                "workspace requires migrate, dictionary, dictionary-usage or preference",
            ))
        }
    }
    Ok(Value::Object(request))
}

/// The MCP tool name of each `workspace` command.
pub const TOOLS: &[(&str, &str)] = &[
    ("openxer_workspace_migrate", "workspace_migrate"),
    ("openxer_workspace_dictionary", "workspace_dictionary"),
    (
        "openxer_workspace_dictionary_usage",
        "workspace_dictionary_usage",
    ),
    ("openxer_workspace_preference", "workspace_preference"),
];

/// The MCP tool definitions of the `workspace` commands.
pub fn tool_definitions() -> Vec<Value> {
    let output = json!({"type":"object","properties":{"schema_version":{"type":"string"},"ok":{"type":"boolean"},"result":{"type":"object"},"error":{"type":"object"}},"required":["schema_version","ok"]});
    let input = json!({"type":"string","minLength":1,"description":"OpenXER workspace database path; read-only"});
    let table = json!({"type":"string","minLength":1,"description":"calendar, resource, role, activity_code, code_value, udf, account, or a dictionary table name such as RSRCRATE"});
    let read = json!({"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false});
    vec![
        json!({"name":"openxer_workspace_migrate",
            "description":"Move a workspace that keeps calendars, resources, codes and the other dictionaries inside each project (workspace schema 1) to one shared workspace dictionary (schema 3). dry_run reports the plan per project (rows shared, added, renumbered, kept as renamed copies with the fields that differ); otherwise the converted copy is written to output and the input stays as its backup. Every project is checked first: same rows and the same dates and float, unleveled and leveled.",
            "inputSchema":{"type":"object","additionalProperties":false,"properties":{"input":input,"output":{"type":"string","minLength":1,"description":"New workspace path for the converted copy; existing files are refused. Omit with dry_run"},"dry_run":{"type":"boolean","default":false}},"required":["input"]},
            "outputSchema":output,
            "annotations":{"readOnlyHint":false,"destructiveHint":false,"idempotentHint":false,"openWorldHint":false}}),
        json!({"name":"openxer_workspace_dictionary",
            "description":"List the shared workspace dictionary of an OpenXER workspace (schema 3): global calendars, resources and rates, roles, global and EPS activity codes and values, UDFs, cost accounts and the other workspace tables, each row with its id, name and the workspace ids of the projects that use it. table limits the list to one table.",
            "inputSchema":{"type":"object","additionalProperties":false,"properties":{"input":input,"table":table},"required":["input"]},
            "outputSchema":output,"annotations":read}),
        json!({"name":"openxer_workspace_dictionary_usage",
            "description":"Which workspace projects use one row of the shared workspace dictionary (named by table and id or name). A row in use cannot be deleted in the app.",
            "inputSchema":{"type":"object","additionalProperties":false,"properties":{"input":input,"table":table,"id":{"type":"integer"},"name":{"type":"string","minLength":1}},"required":["input","table"]},
            "outputSchema":output,"annotations":read}),
        json!({"name":"openxer_workspace_preference",
            "description":"The app's User Preferences, Assistance tab, kept in a workspace: new_activity_wizard (on by default: Edit > Add opens the New Activity wizard; off: Add inserts the row at once) and new_resource_wizard (on by default: the Resources view's Add opens the New Resource wizard; off: Add inserts the resource at once). Without value it reads them; with name and value it writes a copy to output with that switch set, or with dry_run reports the change. The app reads the switch when it starts.",
            "inputSchema":{"type":"object","additionalProperties":false,"properties":{"input":input,"name":{"type":"string","enum":["new_activity_wizard","new_resource_wizard"]},"value":{"type":"boolean"},"output":{"type":"string","minLength":1,"description":"New workspace path for the copy; existing files are refused. Omit with dry_run"},"dry_run":{"type":"boolean","default":false}},"required":["input"]},
            "outputSchema":output,
            "annotations":{"readOnlyHint":false,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}}),
    ]
}
