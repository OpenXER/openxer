//! Newline-delimited JSON-RPC transport for MCP.
use crate::{dispatch, parse_command, response, VERSION};
use serde_json::{json, Map, Value};
use std::io::{self, BufRead, Write};

pub const PROTOCOL_VERSION: &str = "2026-07-28";
const LEGACY: &[&str] = &["2025-11-25", "2025-06-18", "2024-11-05"];
const VERSION_KEY: &str = "io.modelcontextprotocol/protocolVersion";
const CAPS_KEY: &str = "io.modelcontextprotocol/clientCapabilities";
const INSTRUCTIONS: &str = "Native OpenXER scheduling. 1. When the user has not named a file (\"inspect my schedule\"), call openxer_find_schedules first: it lists the OpenXER workspace on this PC with its projects, most recently saved first; then pass that workspace as input with a workspace_id. 2. Before you analyze, build or change a schedule, call openxer_skills and follow the skill whose use_when matches (openxer_skill gets it; the user may also pick one from this server's prompts). 3. Never compute schedule numbers yourself: every count, date, float and percent comes from a tool result (openxer_check_schedule, openxer_paths, openxer_compare, openxer_lookahead, openxer_milestones, openxer_schedule_stats, openxer_resource_loading, openxer_wbs_summary and the others). Find_schedules, inspect, load, curves, usage, documents, read_document and the analysis tools are read-only; read_document returns the text of a project document's linked local file, addressed by the document, never by a path. Every mutation requires a new explicit output path; existing files are never overwritten; try it with dry_run first. Dates are project-local; durations and lags are integer working minutes in load and schedule, hours in the edit tools. Relationship editing does not implicitly recalculate. Edit tools (openxer_apply and the single-operation tools) name activities by Activity ID, resources by short name, calendars by name and WBS elements by code path; they use the desktop app's setters and run all or nothing. No other scheduling application or Python runtime is used.";

#[derive(Default)]
pub struct Server {
    initialized: bool,
    ready: bool,
    /// The Chat window's tool set (`mcp --toolset chat`) instead of the
    /// full catalog.
    chat: Option<crate::chat_tools::ChatContext>,
}

impl Server {
    /// A server of the chat tool set.
    pub fn chat(context: crate::chat_tools::ChatContext) -> Server {
        Server {
            chat: Some(context),
            ..Server::default()
        }
    }

    fn instructions(&self) -> String {
        match &self.chat {
            Some(chat) => crate::chat_tools::instructions(chat.mode),
            None => INSTRUCTIONS.to_string(),
        }
    }

    fn tool_list(&self) -> Vec<Value> {
        match &self.chat {
            Some(chat) => crate::chat_tools::tool_list(chat),
            None => tool_definitions(),
        }
    }

    fn server_capabilities(&self) -> Value {
        if self.chat.is_some() {
            json!({"tools":{}})
        } else {
            capabilities()
        }
    }
}
fn rpc_error(id: Value, code: i64, message: &str, data: Option<Value>) -> Value {
    let mut error = json!({"code":code,"message":message});
    if let Some(data) = data {
        error["data"] = data;
    }
    json!({"jsonrpc":"2.0","id":id,"error":error})
}
fn result(id: Value, mut value: Value, modern: bool) -> Value {
    if modern {
        value["resultType"] = json!("complete");
        value["_meta"] =
            json!({"io.modelcontextprotocol/serverInfo":{"name":"openxer","version":VERSION}});
    }
    json!({"jsonrpc":"2.0","id":id,"result":value})
}
fn has_only(params: &Map<String, Value>, fields: &[&str]) -> bool {
    params
        .keys()
        .all(|k| k == "_meta" || fields.contains(&k.as_str()))
}

impl Server {
    /// Handles one request. Modern requests are self-contained; legacy sessions
    /// retain only handshake state, never an implicit current project.
    pub fn handle(&mut self, request: Value) -> Option<Value> {
        let Some(object) = request.as_object() else {
            return Some(rpc_error(
                Value::Null,
                -32600,
                "Expected one JSON-RPC request object",
                None,
            ));
        };
        let id = object.get("id").cloned();
        let valid_id = id
            .as_ref()
            .is_none_or(|v| v.is_string() || v.as_i64().is_some() || v.as_u64().is_some());
        if object.get("jsonrpc") != Some(&json!("2.0"))
            || object.get("method").and_then(Value::as_str).is_none()
            || !valid_id
        {
            return Some(rpc_error(
                if valid_id {
                    id.unwrap_or(Value::Null)
                } else {
                    Value::Null
                },
                -32600,
                "Invalid JSON-RPC request",
                None,
            ));
        }
        let method = object["method"].as_str().expect("validated method");
        if id.is_none() {
            if method == "notifications/initialized" && self.initialized {
                self.ready = true;
            }
            // Never execute a mutating tool in a notification: no result could
            // tell the caller whether its requested file was actually created.
            return None;
        }
        let id = id.expect("request id");
        let empty = Map::new();
        let params = match object.get("params") {
            None => &empty,
            Some(Value::Object(v)) => v,
            _ => return Some(rpc_error(id, -32602, "params must be an object", None)),
        };
        if params.get("_meta").is_some_and(|value| !value.is_object()) {
            return Some(rpc_error(
                id,
                -32602,
                "params._meta must be an object",
                None,
            ));
        }
        let meta = params.get("_meta").and_then(Value::as_object);
        let modern = method == "server/discover"
            || meta.is_some_and(|m| m.contains_key(VERSION_KEY) || m.contains_key(CAPS_KEY));
        if modern {
            let version = meta
                .and_then(|m| m.get(VERSION_KEY))
                .and_then(Value::as_str);
            let caps = meta
                .and_then(|m| m.get(CAPS_KEY))
                .and_then(Value::as_object);
            if version.is_none() || caps.is_none() {
                return Some(rpc_error(id,-32602,"Modern requests require protocolVersion and clientCapabilities in params._meta",None));
            }
            if version != Some(PROTOCOL_VERSION) {
                return Some(rpc_error(
                    id,
                    -32022,
                    "Unsupported protocol version",
                    Some(json!({"requested":version,"supported":[PROTOCOL_VERSION]})),
                ));
            }
        }
        if method == "initialize" && !modern {
            if !has_only(params, &["protocolVersion", "capabilities", "clientInfo"])
                || params
                    .get("protocolVersion")
                    .and_then(Value::as_str)
                    .is_none()
                || !params.get("capabilities").is_some_and(Value::is_object)
                || !params.get("clientInfo").is_some_and(|v| {
                    v.get("name").is_some_and(Value::is_string)
                        && v.get("version").is_some_and(Value::is_string)
                })
            {
                return Some(rpc_error(
                    id,
                    -32602,
                    "initialize requires protocolVersion, capabilities and clientInfo",
                    None,
                ));
            }
            if self.initialized {
                return Some(rpc_error(
                    id,
                    -32600,
                    "Session is already initialized",
                    None,
                ));
            }
            let requested = params["protocolVersion"]
                .as_str()
                .expect("validated version");
            let negotiated = if LEGACY.contains(&requested) {
                requested
            } else {
                LEGACY[0]
            };
            self.initialized = true;
            return Some(result(
                id,
                json!({"protocolVersion":negotiated,"capabilities":self.server_capabilities(),"serverInfo":{"name":"openxer","version":VERSION},"instructions":self.instructions()}),
                false,
            ));
        }
        if !modern && !self.ready && method != "ping" {
            return Some(rpc_error(
                id,
                -32600,
                "Initialize the legacy session or supply per-request MCP metadata",
                None,
            ));
        }
        let value = match method {
            "ping" if has_only(params, &[]) => json!({}),
            "server/discover" if modern && has_only(params, &[]) => {
                json!({"supportedVersions":[PROTOCOL_VERSION],"capabilities":self.server_capabilities(),"instructions":self.instructions(),"ttlMs":3600000,"cacheScope":"public"})
            }
            "tools/list" if has_only(params, &["cursor"]) => {
                if params.get("cursor").is_some_and(|v| !v.is_null()) {
                    return Some(rpc_error(
                        id,
                        -32602,
                        "This server has a single tool page; omit cursor",
                        None,
                    ));
                }
                let mut list = json!({"tools":self.tool_list()});
                if modern {
                    list["ttlMs"] = json!(3600000);
                    list["cacheScope"] = json!("public");
                }
                list
            }
            "tools/call" if has_only(params, &["name", "arguments"]) => {
                let Some(name) = params.get("name").and_then(Value::as_str) else {
                    return Some(rpc_error(id, -32602, "Tool name must be a string", None));
                };
                if let Some(chat) = &self.chat {
                    let Some(args) = params.get("arguments").filter(|v| v.is_object()) else {
                        return Some(rpc_error(
                            id,
                            -32602,
                            "Tool arguments must be an object",
                            None,
                        ));
                    };
                    if !crate::chat_tools::tools(chat)
                        .iter()
                        .any(|t| t.name == name)
                    {
                        return Some(rpc_error(id, -32602, "Unknown OpenXER tool", None));
                    }
                    // Text only: the chat tools publish no output schema, so
                    // the result is not sent twice.
                    let done = crate::chat_tools::call(chat, name, args.clone());
                    let value = json!({"content":[{"type":"text","text":done.text}],"isError":done.is_error});
                    return Some(result(id, value, modern));
                }
                let Some(command) = tool_command(name) else {
                    return Some(rpc_error(id, -32602, "Unknown OpenXER tool", None));
                };
                let Some(mut args) = params.get("arguments").and_then(Value::as_object).cloned()
                else {
                    return Some(rpc_error(
                        id,
                        -32602,
                        "Tool arguments must be an object",
                        None,
                    ));
                };
                if args.contains_key("command") {
                    return Some(rpc_error(
                        id,
                        -32602,
                        "Tool arguments must not contain command",
                        None,
                    ));
                }
                // "p6" is the old name of the tables layout; old scripts still
                // send it, so it is accepted without being published.
                if command == "export_xlsx"
                    && args.get("layout").and_then(Value::as_str) == Some("p6")
                {
                    args.insert("layout".into(), json!("tables"));
                }
                let schema = tool_definitions()
                    .into_iter()
                    .find(|t| t["name"] == name)
                    .expect("known tool has schema");
                if let Err(message) = validate_arguments(
                    &Value::Object(args.clone()),
                    &schema["inputSchema"],
                    "arguments",
                ) {
                    return Some(rpc_error(id, -32602, &message, None));
                }
                if let Some(op) = edit_tool(name) {
                    args = edit_arguments(op, args);
                }
                if name == "openxer_eps_edit" {
                    args = eps_arguments(args);
                }
                if name == "openxer_baseline_edit" {
                    args = crate::baseline_cmd::mcp_arguments(args);
                }
                args.insert("command".into(), json!(command));
                let command = match parse_command(Value::Object(args)) {
                    Ok(c) => c,
                    Err(e) => return Some(rpc_error(id, -32602, &e.message, None)),
                };
                let execution = dispatch(command);
                let failed = execution.is_err();
                let envelope = response(execution);
                json!({"content":[{"type":"text","text":envelope.to_string()}],"structuredContent":envelope,"isError":failed})
            }
            // One prompt per skill, read from the skill folder on every
            // request, so an edited or new skill shows up without a restart.
            "prompts/list" if has_only(params, &["cursor"]) => {
                if params.get("cursor").is_some_and(|v| !v.is_null()) {
                    return Some(rpc_error(
                        id,
                        -32602,
                        "This server has a single prompt page; omit cursor",
                        None,
                    ));
                }
                json!({"prompts": prompt_list()})
            }
            "prompts/get" if has_only(params, &["name", "arguments"]) => {
                let Some(name) = params.get("name").and_then(Value::as_str) else {
                    return Some(rpc_error(id, -32602, "Prompt name must be a string", None));
                };
                if params
                    .get("arguments")
                    .is_some_and(|v| !v.is_null() && !v.is_object())
                {
                    return Some(rpc_error(
                        id,
                        -32602,
                        "Prompt arguments must be an object",
                        None,
                    ));
                }
                let catalog = crate::skills::catalog();
                let Some(entry) = catalog.get(name) else {
                    return Some(rpc_error(id, -32602, "Unknown prompt", None));
                };
                json!({"description": entry.skill.description,
                       "messages":[{"role":"user","content":{"type":"text","text":crate::skills::prompt_text(&entry.skill)}}]})
            }
            "ping" | "server/discover" | "tools/list" | "tools/call" | "prompts/list"
            | "prompts/get" => {
                return Some(rpc_error(id, -32602, "Invalid method parameters", None))
            }
            _ => return Some(rpc_error(id, -32601, "Method not found", None)),
        };
        Some(result(id, value, modern))
    }
}

/// The command an MCP tool runs.
pub(crate) fn tool_command(name: &str) -> Option<&'static str> {
    Some(match name {
        "openxer_inspect" => "inspect",
        "openxer_find_schedules" => "find_schedules",
        "openxer_import" => "import",
        "openxer_load" => "load",
        "openxer_check_schedule" => "check_schedule",
        "openxer_playbook" => "playbook",
        "openxer_curves" => "curves",
        "openxer_usage" => "usage",
        "openxer_schedule" => "schedule",
        "openxer_relationship" => "relationship",
        "openxer_export" => "export",
        "openxer_export_xlsx" => "export_xlsx",
        "openxer_export_pdf" => "export_pdf",
        "openxer_export_scurve" => "export_scurve",
        "openxer_export_earned_hours" => "export_earned_hours",
        "openxer_import_xlsx" => "import_xlsx",
        "openxer_documents" => "documents",
        "openxer_records" => "records",
        "openxer_earned_value" => "earned_value",
        "openxer_read_document" => "read_document",
        "openxer_apply" => "apply",
        "openxer_eps_list" => "eps_list",
        "openxer_eps_edit" => "eps",
        "openxer_baseline_list" => "baseline_list",
        "openxer_baseline_edit" => "baseline",
        "openxer_baseline_update" => "baseline_update",
        "openxer_paths" => "paths",
        "openxer_compare" => "compare",
        "openxer_lookahead" => "lookahead",
        "openxer_milestones" => "milestones",
        "openxer_schedule_stats" => "schedule_stats",
        "openxer_resource_loading" => "resource_loading",
        "openxer_wbs_summary" => "wbs_summary",
        "openxer_skills" => "skills",
        "openxer_skill" => "skill",
        other => {
            if let Some((_, c)) = crate::workspace_cmd::TOOLS
                .iter()
                .find(|(t, _)| *t == other)
            {
                c
            } else if edit_tool(other).is_some() {
                "edit"
            } else {
                return None;
            }
        }
    })
}

/// The fields of edit operation `op` as its single edit tool takes them:
/// (field, required). Empty for an operation without its own tool.
pub(crate) fn edit_tool_fields(op: &str) -> Vec<(&'static str, bool)> {
    EDIT_TOOLS
        .iter()
        .find(|t| t.1 == op)
        .map(|t| t.3.iter().map(|(f, _, required)| (*f, *required)).collect())
        .unwrap_or_default()
}

// Validate the vocabulary used by our published schemas before serde conversion;
// optional does not imply nullable, and integer bounds apply before execution.
pub(crate) fn validate_arguments(value: &Value, schema: &Value, path: &str) -> Result<(), String> {
    let matches_type = |kind: &str| match kind {
        "object" => value.is_object(),
        "string" => value.is_string(),
        "boolean" => value.is_boolean(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "null" => value.is_null(),
        "number" => value.is_number(),
        "array" => value.is_array(),
        _ => false,
    };
    let valid_type = match &schema["type"] {
        Value::String(kind) => matches_type(kind),
        Value::Array(kinds) => kinds.iter().filter_map(Value::as_str).any(matches_type),
        _ => true,
    };
    if !valid_type {
        return Err(format!("{path} has the wrong JSON type"));
    }
    if let Some(values) = schema["enum"].as_array().filter(|v| !v.contains(value)) {
        let names: Vec<&str> = values.iter().filter_map(Value::as_str).collect();
        return Err(if names.len() == values.len() && names.len() <= 20 {
            format!(
                "{path} is not an allowed value; use one of {}",
                names.join(", ")
            )
        } else {
            format!("{path} is not an allowed value")
        });
    }
    if let Some(number) = value.as_i64() {
        if schema["minimum"].as_i64().is_some_and(|min| number < min)
            || schema["maximum"].as_i64().is_some_and(|max| number > max)
        {
            return Err(format!("{path} is outside the allowed range"));
        }
    } else if let Some(number) = value.as_u64() {
        if schema["maximum"].as_u64().is_some_and(|max| number > max) {
            return Err(format!("{path} is outside the allowed range"));
        }
    }
    if let Some(text) = value.as_str() {
        if schema["minLength"]
            .as_u64()
            .is_some_and(|min| (text.chars().count() as u64) < min)
        {
            return Err(format!("{path} is too short"));
        }
    }
    if let Some(object) = value.as_object() {
        if let Some(required) = schema["required"].as_array() {
            for field in required.iter().filter_map(Value::as_str) {
                if !object.contains_key(field) {
                    return Err(format!("{path}.{field} is required"));
                }
            }
        }
        for (field, child) in object {
            if let Some(child_schema) = schema["properties"].get(field) {
                validate_arguments(child, child_schema, &format!("{path}.{field}"))?;
            } else if schema["additionalProperties"] == false {
                return Err(format!("Unknown field {path}.{field}"));
            }
        }
    }
    Ok(())
}

pub fn run_stdio() -> io::Result<()> {
    run_stdio_with(Server::default())
}

/// Serve `server` over stdin and stdout until stdin closes.
pub fn run_stdio_with(mut server: Server) -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut output = stdout.lock();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str(&line) {
            Ok(value) => server.handle(value),
            Err(_) => Some(rpc_error(Value::Null, -32700, "Parse error", None)),
        };
        if let Some(reply) = reply {
            serde_json::to_writer(&mut output, &reply)?;
            output.write_all(b"\n")?;
            output.flush()?;
        }
    }
    Ok(())
}

/// The single-operation edit tools: tool name, operation, description and
/// the operation's own fields (name, schema, required).
type EditTool = (
    &'static str,
    &'static str,
    &'static str,
    &'static [(&'static str, &'static str, bool)],
);
const EDIT_TOOLS: &[EditTool] = &[
    ("openxer_set_activity", "set_activity", "Set one activity field through the desktop's setters (status, durations and assignments follow the same rules). Fields are OpenXER column keys such as remaining_duration, actual_start, physical_percent_complete, calendar, duration_type.", &[("activity", "text", true), ("field", "text", true), ("value", "value", true)]),
    ("openxer_add_activity", "add_activity", "Add an activity with the project's defaults for new activities (types, calendar, 5 day duration). Without an Activity ID it takes the next ID from the project's auto-numbering; selected names the activity to increment from, as the app's selection does.", &[("activity", "text", false), ("selected", "text", false), ("name", "text", false), ("wbs", "text", false)]),
    ("openxer_delete_activity", "delete_activity", "Delete an activity with its relationships and assignments.", &[("activity", "text", true)]),
    ("openxer_dissolve_activity", "dissolve_activity", "Edit > Dissolve: remove an activity that has at least one predecessor and one successor, and tie each predecessor to each successor. The new relationship starts where the predecessor relationship starts and ends where the successor relationship ends; a pair already related keeps its relationship. Lag is 0, or with retain_lag the two lags added. Nothing is rescheduled.", &[("activity", "text", true), ("retain_lag", "boolean", false)]),
    ("openxer_assign", "assign_resource", "Assign a resource (by short name) to an activity.", &[("activity", "text", true), ("resource", "text", true)]),
    ("openxer_unassign", "remove_assignment", "Remove a resource assignment.", &[("activity", "text", true), ("resource", "text", true)]),
    ("openxer_set_assignment", "set_assignment", "Set one assignment field (budgeted_units, remaining_units, actual_regular_units, price_per_unit, rate_type, lag_hours and more) with the duration type rules. period_remaining_units with DATE=UNITS or START,FINISH=UNITS types the remaining units of those days, as in the usage spreadsheet (the curve becomes Manual; curve null removes the typed values).", &[("activity", "text", true), ("resource", "text", true), ("field", "text", true), ("value", "value", true)]),
    ("openxer_calendar_day", "calendar_day", "Set one date of a calendar: shifts [[\"08:00\",\"12:00\"],...], [] for nonwork, or null for the weekday pattern.", &[("calendar", "text", true), ("date", "text", true), ("shifts", "shifts_or_null", false)]),
    ("openxer_calendar_weekday", "calendar_weekday", "Set a calendar's work periods for one weekday (0 = Sunday); [] is nonwork.", &[("calendar", "text", true), ("weekday", "weekday", true), ("shifts", "shifts", true)]),
    ("openxer_set_project", "set_project", "Set the data date or must finish date (project-local YYYY-MM-DD HH:MM).", &[("field", "project_field", true), ("value", "value", true), ("schedule", "boolean", false)]),
    ("openxer_add_wbs", "add_wbs", "Add the WBS elements of a code path (1.2.3) that do not exist yet.", &[("wbs", "text", true), ("name", "text", false)]),
    ("openxer_add_document", "add_document", "Add a work product or document titled title, under the document parent (reference number, title or id) or at the top level; fields sets more of its fields at once.", &[("title", "text", true), ("parent", "text", false), ("reference", "text", false), ("fields", "object", false)]),
    ("openxer_set_document", "set_document", "Set one field of a document (named by reference number, title or document id): title, reference, deliverable, status, category (by name), author, version, date (YYYY-MM-DD[ HH:MM]), private_location, public_location or description; null clears.", &[("document", "text", true), ("field", "text", true), ("value", "value", true)]),
    ("openxer_delete_document", "delete_document", "Delete a document with the documents under it and their links to activities and WBS elements.", &[("document", "text", true)]),
    ("openxer_link_document", "link_document", "Link a document to an activity (Activity ID) or to a WBS element itself (path); work_product marks the link as a work product.", &[("document", "text", true), ("activity", "text", false), ("wbs", "text", false), ("work_product", "boolean", false)]),
    ("openxer_unlink_document", "unlink_document", "Remove a document's link to an activity or a WBS element.", &[("document", "text", true), ("activity", "text", false), ("wbs", "text", false)]),
    ("openxer_add_expense", "add_expense", "Add an expense item to an activity, with name and any fields (category, cost_account, accrual, unit_of_measure, vendor, po_number, auto_compute_actuals, budgeted_units, price_per_unit, budgeted_cost, actual_cost, remaining_cost, description).", &[("activity", "text", true), ("name", "text", false), ("fields", "fields", false)]),
    ("openxer_set_expense", "set_expense", "Set one field of an expense item (by id or name) as the Expenses view does: budgeted units x price is the budgeted cost, and an item without actuals keeps its budget remaining. activity moves it.", &[("expense", "text", true), ("field", "text", true), ("value", "value", true)]),
    ("openxer_delete_expense", "delete_expense", "Delete an expense item (by id or name).", &[("expense", "text", true)]),
    ("openxer_add_issue", "add_issue", "Add an open issue identified today, with name and any fields (responsible_manager, priority, status, identified_on, identified_by, resolution_date, activity or wbs, resource, notes).", &[("name", "text", false), ("fields", "fields", false)]),
    ("openxer_set_issue", "set_issue", "Set one field of an issue (by id or name); activity and wbs are exclusive (setting one clears the other).", &[("issue", "text", true), ("field", "text", true), ("value", "value", true)]),
    ("openxer_delete_issue", "delete_issue", "Delete an issue with its history.", &[("issue", "text", true)]),
    ("openxer_add_risk", "add_risk", "Add a proposed threat identified today with the next free Risk ID, with name and any fields (risk_id, category, type, status, owner, identified_by, identified_on, pre/post response probability, schedule and cost, response_type, response_description, description, cause, effect, notes).", &[("name", "text", false), ("fields", "fields", false)]),
    ("openxer_set_risk", "set_risk", "Set one field of a risk (by id, Risk ID or name).", &[("risk", "text", true), ("field", "text", true), ("value", "value", true)]),
    ("openxer_delete_risk", "delete_risk", "Delete a risk with its activity assignments.", &[("risk", "text", true)]),
    ("openxer_assign_risk", "assign_risk", "Assign a risk to an activity.", &[("risk", "text", true), ("activity", "text", true)]),
    ("openxer_unassign_risk", "unassign_risk", "Remove a risk's assignment to an activity.", &[("risk", "text", true), ("activity", "text", true)]),
    ("openxer_add_threshold", "add_threshold", "Add an enabled threshold (total float, lower 0, activities) or one on parameter (name or measured field, see openxer_records kind thresholds) with any fields (lower_threshold, upper_threshold, detail_to_monitor, wbs, responsible_manager, priority, status, from_date, to_date).", &[("parameter", "text", false), ("fields", "fields", false)]),
    ("openxer_set_threshold", "set_threshold", "Set one field of a threshold (by id).", &[("threshold", "text", true), ("field", "text", true), ("value", "value", true)]),
    ("openxer_delete_threshold", "delete_threshold", "Delete a threshold; the issues it raised stay.", &[("threshold", "text", true)]),
    ("openxer_update_progress", "update_progress", "Tools > Update Progress: record progress as scheduled up to the new data_date for every activity (or only activities, a list of Activity IDs), spend flagged expenses of the activities it progresses, then move the data date. remaining is always (remaining work to the scheduled finish) or duration_type. Schedule afterwards.", &[("data_date", "text", true), ("activities", "list", false), ("remaining", "remaining", false)]),
    ("openxer_apply_actuals", "apply_actuals", "Tools > Apply Actuals: progress what is set to compute actuals up to the new data_date. The activity flag, or every assignment's resource flag, progresses the activity and all its assignments; flagged resources on some assignments give only those actuals (units/time x working time since their remaining start); a flagged expense of a started activity accrues uniformly. A project already at data_date is left alone. Stamps the apply actuals date.", &[("data_date", "text", true), ("remaining", "remaining", false)]),
    ("openxer_recalculate_costs", "recalculate_costs", "Tools > Recalculate Assignment Costs from the resources' prices; assignments priced from a role or an override, with costs not linked to units, or with a price change during the assignment are skipped and listed.", &[("active_only", "boolean", false), ("sync_overtime", "boolean", false)]),
    ("openxer_add_step", "add_step", "Add a step to an activity, at position at (1 is first; the end when omitted).", &[("activity", "text", true), ("name", "text", true), ("at", "count", false)]),
    ("openxer_set_step", "set_step", "Set one field of an activity step (step by number from 1, or name): name, complete, percent_complete (100 completes it), weight or description. When the project's steps drive percent complete the activity's physical percent follows.", &[("activity", "text", true), ("step", "step", true), ("field", "text", true), ("value", "value", true)]),
    ("openxer_delete_step", "delete_step", "Delete an activity step (by number or name).", &[("activity", "text", true), ("step", "step", true)]),
    ("openxer_move_step", "move_step", "Move an activity step to position to (1 is first).", &[("activity", "text", true), ("step", "step", true), ("to", "count", true)]),
    ("openxer_add_steps_from_template", "add_steps_from_template", "Add a step template's steps (by name) after the activity's step after (number or name), or after its last step.", &[("activity", "text", true), ("template", "text", true), ("after", "value", false)]),
    ("openxer_create_step_template", "create_step_template", "Create a step template named name from the activity's steps.", &[("activity", "text", true), ("name", "text", true)]),
    ("openxer_set_feedback", "set_feedback", "Set an activity's notes to resources (null clears) and whether its feedback needs review.", &[("activity", "text", true), ("notes", "value", false), ("review_required", "boolean", false)]),
    ("openxer_top_down_estimate", "top_down_estimate", "Tools > Top Down Estimation: spread labor_units over the not started Task and Resource Dependent activities below wbs (the whole project when omitted) by the WBS estimation weights; weights {path: weight} are stored first.", &[("labor_units", "number", true), ("wbs", "text", false), ("not_started_only", "boolean", false), ("weights", "fields", false)]),
    ("openxer_shift_wbs", "shift_wbs", "Move a WBS element up or down among its siblings, left out to its parent's level or right into the sibling above.", &[("wbs", "text", true), ("direction", "direction", true)]),
    ("openxer_rename_wbs_code", "rename_wbs_code", "Change a WBS element's own code (unique among its siblings).", &[("wbs", "text", true), ("code", "text", true)]),
    ("openxer_copy_calendar", "copy_calendar", "Add a calendar named calendar of kind project (default), global or resource, copied from source (a standard 5-day week without one).", &[("calendar", "text", true), ("source", "text", false), ("kind", "calendar_kind", false)]),
    ("openxer_rename_calendar", "rename_calendar", "Rename a calendar.", &[("calendar", "text", true), ("name", "text", true)]),
    ("openxer_set_default_calendar", "set_default_calendar", "Make a global calendar the default.", &[("calendar", "text", true)]),
    ("openxer_convert_calendar", "convert_calendar", "Convert a project calendar to global, or a resource calendar to shared or personal.", &[("calendar", "text", true), ("to", "calendar_to", true)]),
    ("openxer_global_change", "global_change", "Tools > Global Change on every activity of the project: run a global change saved in the workspace input (name) or one given as rule {\"match\":\"all\"|\"any\",\"conditions\":[{\"column\",\"op\",\"value\",\"high\"}],\"then\":[{\"column\",\"value\",\"operator\",\"value2\"}],\"otherwise\":[...]}. Columns are field ids or titles (user defined fields and activity codes included); op is a key (eq, ne, contains, not_contains, starts_with, lt, le, gt, ge, within, not_within, under) or its caption, under (is under) for WBS and activity codes; operator is +, -, x, / or & with value2; a value is a literal, a field title or a quoted text. dry_run returns the report lines (activity, field, old, new, exception) the app shows, durations in hours; otherwise the lines without an exception are written and changed counts them.", &[("name", "text", false), ("rule", "rule", false)]),
    ("openxer_monitor_thresholds", "monitor_thresholds", "Monitor Thresholds: evaluate every enabled threshold as the app does (float and date variances in days, duration and cost percents, earned value amounts and indexes against the assigned baselines) and raise an issue for each activity or WBS element outside its bounds, or update the open issue raised before. from and to replace every threshold's own window; today dates new issues (default today).", &[("from", "text", false), ("to", "text", false), ("today", "text", false)]),
];

/// The operation an edit tool runs.
fn edit_tool(name: &str) -> Option<&'static str> {
    EDIT_TOOLS.iter().find(|t| t.0 == name).map(|t| t.1)
}

/// Move an edit tool's operation fields into the `operation` of an `edit`
/// command; the file and mode arguments stay at the top.
fn edit_arguments(op: &str, args: Map<String, Value>) -> Map<String, Value> {
    let mut top = Map::new();
    let mut operation = Map::new();
    operation.insert("op".into(), json!(op));
    for (k, v) in args {
        if matches!(
            k.as_str(),
            "input" | "output" | "project_id" | "workspace_id" | "dry_run" | "recalculate"
        ) {
            top.insert(k, v);
        } else {
            operation.insert(k, v);
        }
    }
    top.insert("operation".into(), Value::Object(operation));
    top
}

fn edit_schema(kind: &str) -> Value {
    let shifts = json!({"type":"array","items":{"type":"array"},"description":"[[\"HH:MM\",\"HH:MM\"],...]"});
    match kind {
        "text" => json!({"type":"string","minLength":1}),
        "value" => {
            json!({"type":["string","number","boolean","null"],"description":"Hours for durations, YYYY-MM-DD HH:MM for dates, codes or names for choices, true or false for yes/no fields; null clears"})
        }
        "shifts" => shifts,
        "shifts_or_null" => {
            json!({"type":["array","null"],"description":"[[\"HH:MM\",\"HH:MM\"],...]; [] nonwork; null the weekday pattern"})
        }
        "weekday" => json!({"type":"integer","minimum":0,"maximum":6}),
        "boolean" => json!({"type":"boolean"}),
        "object" => {
            json!({"type":"object","description":"Document fields and values, as for set_document"})
        }
        "fields" => {
            json!({"type":"object","description":"Field names and values, as for the matching set_ operation"})
        }
        "list" => json!({"type":"array","items":{"type":"string","minLength":1}}),
        "count" => json!({"type":"integer","minimum":1}),
        "number" => json!({"type":"number"}),
        "project_field" => json!({"type":"string","enum":["data_date","must_finish"]}),
        "step" => {
            json!({"type":["string","integer"],"description":"Step number from 1, or the step's name"})
        }
        "direction" => json!({"type":"string","enum":["up","down","left","right"]}),
        "calendar_to" => json!({"type":"string","enum":["global","shared","personal"]}),
        "calendar_kind" => json!({"type":"string","enum":["project","global","resource"]}),
        "remaining" => json!({"type":"string","enum":["always","duration_type"]}),
        "rule" => {
            json!({"type":"object","description":"A global change: match, conditions, then, otherwise (see the tool description)"})
        }
        _ => json!({}),
    }
}

/// Input, output and mode arguments every edit tool takes.
fn edit_common(p: &mut Map<String, Value>) {
    p.insert("input".into(),json!({"type":"string","minLength":1,"description":"Local XER or OpenXER workspace path; read-only"}));
    p.insert("output".into(),json!({"type":"string","minLength":1,"description":"New destination path; existing files are refused. Omit with dry_run"}));
    p.insert(
        "project_id".into(),
        json!({"type":"integer","description":"Project id (the XER proj_id) returned by inspect"}),
    );
    p.insert(
        "workspace_id".into(),
        json!({"type":"integer","description":"Workspace row id, useful when project ids repeat"}),
    );
    p.insert("dry_run".into(),json!({"type":"boolean","default":false,"description":"Return the field diff without writing"}));
    p.insert("recalculate".into(),json!({"type":"boolean","description":"false writes set_activity and set_assignment values exactly, with no duration type, units or cost knock-on"}));
}

/// The `openxer_apply` description: what it does and every operation, from
/// the one list the edit format has.
fn apply_description() -> String {
    format!(
        "Apply an openxer-changes/1 batch in order, all or nothing, to a new file (dry_run returns the field diff). Operations: {}. Each operation may carry a meta object that is ignored; see the single edit tools for each operation's fields.",
        crate::edit::OPERATIONS.join(", ")
    )
}

fn edit_tool_definitions() -> Vec<Value> {
    let output = json!({"type":"object","properties":{"schema_version":{"type":"string"},"ok":{"type":"boolean"},"result":{"type":"object"},"error":{"type":"object"}},"required":["schema_version","ok"]});
    let annotations = json!({"readOnlyHint":false,"destructiveHint":false,"idempotentHint":false,"openWorldHint":false});
    let mut tools = vec![];
    let mut p = Map::new();
    edit_common(&mut p);
    p.insert("changes".into(),json!({"type":"object","description":"An openxer-changes/1 document: {\"format\":\"openxer-changes/1\",\"recalculate\":true,\"operations\":[...]}"}));
    p.insert("changes_file".into(),json!({"type":"string","minLength":1,"description":"Path of an openxer-changes/1 .json file, instead of changes"}));
    tools.push(json!({"name":"openxer_apply","description":apply_description(),
        "inputSchema":object(Value::Object(p),&["input"]),"outputSchema":output,"annotations":annotations}));
    for (name, _, description, fields) in EDIT_TOOLS {
        let mut p = Map::new();
        edit_common(&mut p);
        let mut required = vec!["input"];
        for (field, kind, needed) in fields.iter() {
            p.insert((*field).into(), edit_schema(kind));
            if *needed {
                required.push(field);
            }
        }
        tools.push(json!({"name":name,"description":description,"inputSchema":object(Value::Object(p),&required),"outputSchema":output,"annotations":annotations}));
    }
    tools
}

fn object(properties: Value, required: &[&str]) -> Value {
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}
fn options_schema() -> Value {
    let mut p = Map::new();
    for field in [
        "retained_logic",
        "progress_override",
        "lag_from_early_start",
        "use_expected_finish",
        "level_resources",
        "ignore_external_relationships",
        "open_ends_critical",
        "use_must_finish",
        "multiple_float_paths",
        "float_paths_use_total",
    ] {
        p.insert(field.into(), json!({"type":"boolean"}));
    }
    p.insert(
        "data_date".into(),
        json!({"type":"string","description":"Project-local YYYY-MM-DD or YYYY-MM-DD HH:MM"}),
    );
    p.insert(
        "must_finish".into(),
        json!({"type":["string","null"],"description":"Project-local date, or null to clear"}),
    );
    p.insert(
        "lag_calendar".into(),
        json!({"type":"string","enum":["predecessor","successor","24_hour","project"]}),
    );
    p.insert(
        "float_type".into(),
        json!({"type":"string","enum":["start","finish","minimum"]}),
    );
    p.insert(
        "max_float_paths".into(),
        json!({"type":"integer","minimum":1}),
    );
    p.insert(
        "float_paths_end_activity".into(),
        json!({"type":["integer","null"]}),
    );
    object(Value::Object(p), &[])
}
pub fn tool_definitions() -> Vec<Value> {
    let mut tools = vec![json!({"name":"openxer_find_schedules",
        "description":"Start here when the user has not named a file. Lists the OpenXER workspace on this PC (the one the OpenXER app opens, and another copy's when there is one) with its projects, most recently saved first: workspace_id, project_id, name and the XER file each came from. Pass the workspace path as input with a workspace_id to the other tools. With folder (an absolute path), also lists the XER files and workspaces directly in that folder. Read-only; needs no arguments.",
        "inputSchema":object(json!({"folder":{"type":"string","minLength":1,"description":"Absolute path of a folder to list schedule files in (not its subfolders), such as the user's Downloads"}}),&[]),
        "outputSchema":{"type":"object","properties":{"schema_version":{"type":"string"},"ok":{"type":"boolean"},"result":{"type":"object"},"error":{"type":"object"}},"required":["schema_version","ok"]},
        "annotations":{"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}})];
    let playbooks: Vec<&str> = crate::playbooks::PLAYBOOKS
        .iter()
        .map(|(n, _, _)| *n)
        .collect();
    tools.push(json!({"name":"openxer_playbook",
        "description":"Scheduling playbooks: step-by-step methods that name the tools to use. Without name, the list; with name, one playbook. Load the matching one before a build, an update, an audit or a health check.",
        "inputSchema":object(json!({"name":{"type":"string","enum":playbooks}}),&[]),
        "outputSchema":{"type":"object","properties":{"schema_version":{"type":"string"},"ok":{"type":"boolean"},"result":{"type":"object"},"error":{"type":"object"}},"required":["schema_version","ok"]},
        "annotations":{"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}}));
    for (name,description,write) in [
        ("inspect","Inspect XER tables or list projects in an OpenXER workspace without modifying it. Do not know the file? Call openxer_find_schedules first.",false),
        ("import","Import selected or all XER projects into a new desktop-compatible OpenXER workspace. The input may also be a project XML file (root element APIBusinessObjects); the format is told from the contents.",true),
        ("load","Load project options and independently paged activity/relationship data without scheduling. fields, activities, wbs and critical keep the answer small and sort orders it; with an activity filter the relationships are those touching the matched activities.",false),
        ("check_schedule","Tools > Check Schedule, read-only: the app's 15 schedule health checks with its default targets, one row each (check, count, of, result as a percent or the BEI index, target, pass, and the first five offenders). check (a row's name) also lists that row's Activity IDs or relationships, up to limit. Late Activities and BEI compare with the project's own planned finishes.",false),
        ("curves","List the resource curves an assignment can use (the file's own and OpenXER's presets) with their 21 points, read-only. Set one with set_assignment field curve and the curve name.",false),
        ("usage","Time-phased units (or costs) of an activity's or a resource's assignments per day, week or month, read-only: budgeted, actual, remaining and remaining late per period, spread as the desktop usage views spread them, so resource curves shape it on Fixed Duration activities. Also each assignment's windows, curve and whether it applies.",false),
        ("schedule","Run the native scheduler and optional resource leveling; write a new XER/workspace and report up to 200 changed activities.",true),
        ("relationship","Add, update, or remove a validated relationship; write a new file. Does not implicitly schedule.",true),
        ("export","Export selected workspace/XER project state to a new XER, retaining other tables and projects in its source document. With projects, writes only those projects; version sets the header version label. format xml (or an output ending in .xml) writes project XML instead: projects, WBS, activities, relationships, calendars, resources, roles, assignments, codes, UDFs, expenses and baselines in the file.",true),
        ("export_xlsx","Export one project to a new .xlsx: the modern layout (real dates, durations in a stated unit, WBS outline, Relationships and About sheets) or the tables layout (TASK, TASKPRED and TASKRSRC sheets with XER field names).",true),
        ("export_pdf","Print one project's activity layout (table and Gantt, or Activity Network) to a new vector PDF with its saved Page Setup, using the desktop renderer headless.",true),
        ("export_scurve","Write a project's S-curve to a new .xlsx, as Tools > Reports > S-Curve does: per period (week from Sunday by default) the budgeted (from the assigned baseline, else the current plan), actual (to the data date) and remaining values with running sums and percents as formulas, earned to the data date and a native chart; one sheet each for budgeted, actual and remaining by resource or WBS level; an Info sheet with the data date, baseline, detection and warnings. measure auto picks labor hours when labor assignments carry budgeted units, else cost, else duration (each activity weighs its planned working days).",true),
        ("export_earned_hours","Write cumulative earned labor hours to the data date to a new .xlsx, as Tools > Reports > Earned Hours does: Earned to date (one row per resource, WBS element or activity: budgeted, earned, actual, remaining, at completion, % earned) and By activity (one row per labor assignment, by Activity ID then resource). Earned = baseline budgeted units x performance % complete, as earned value computes it; rows and columns keep a stable order so two exports subtract in Excel. Error no_labor_units when the project has no labor hours.",true),
        ("import_xlsx","Preview (dry_run) or apply a spreadsheet of activity and relationship changes to a project; writes a new file. Rows match by Activity ID; deletes need an explicit D flag.",true),
        ("records","List the project's expense items, issues, risks or thresholds (kind) with every field as the edits name it: choices by label, activities by Activity ID, WBS elements by code path, resources by short name; thresholds come with the parameter list. activity keeps only that activity's records. Read-only.",false),
        ("earned_value","Earned value per activity and for the project, as the earned value columns compute it (BAC, PV, EV, AC, SV, CV, SPI, CPI, ETC, EAC, VAC, performance and schedule percent complete) in cost or labor_units, against the workspace's assigned project or primary baseline (the project itself when none). Read-only.",false),
        ("documents","List a project's work products and documents: title, reference, version, status, category, author, date, private and public file locations, description, and the activities and WBS elements each is linked to. activity or wbs keeps only the documents linked there.",false),
        ("read_document","Read the text of the file a project document points at (named by reference number, title or document id; never a free path). Only existing files on a local drive by absolute path, under the input file's folder or a folder in the OPENXER_DOCUMENT_ROOTS environment variable of the server; relative, network and web locations, programs and scripts are refused. A PDF page cut by limit is marked truncated. txt, md, csv, json, xml, log and docx come back as text paged by offset and limit characters; PDF text page by page with page and pages. Read-only.",false),
    ] {
        let mut p = Map::new();
        p.insert("input".into(),json!({"type":"string","minLength":1,"description":"Local XER or OpenXER workspace path; read-only"}));
        if name != "inspect" { p.insert("project_id".into(),json!({"type":"integer","description":"Project id (the XER proj_id) returned by inspect"})); }
        if !["inspect","import"].contains(&name) { p.insert("workspace_id".into(),json!({"type":"integer","description":"Workspace row id, useful when project ids repeat"})); }
        if write { p.insert("output".into(),json!({"type":"string","minLength":1,"description":"New destination path; existing files are refused"})); }
        if name == "load" {
            for field in ["offset","relationship_offset"] { p.insert(field.into(),json!({"type":"integer","minimum":0,"default":0})); }
            for field in ["limit","relationship_limit"] { p.insert(field.into(),json!({"type":"integer","minimum":0,"maximum":2000,"default":200,"description":"0 returns counts only"})); }
            p.insert("fields".into(),json!({"type":"array","items":{"type":"string","enum":crate::load::FIELDS},"description":"Only these activity fields (task_code is always kept)"}));
            p.insert("activities".into(),json!({"type":"array","items":{"type":"string","minLength":1},"description":"Only these Activity IDs"}));
            p.insert("wbs".into(),json!({"type":"string","minLength":1,"description":"Only activities under this WBS code path (1.2.3)"}));
            p.insert("critical".into(),json!({"type":"boolean","default":false,"description":"Only critical activities (the project's definition)"}));
            p.insert("sort".into(),json!({"type":"string","minLength":1,"description":"Order activities by a field, - first for descending (-early_finish: latest first)"}));
        }
        if name == "schedule" { p.insert("options".into(),options_schema()); }
        if name == "check_schedule" {
            let names: Vec<&str> = openxer_xer::schedule_check::CHECKS.iter().map(|c| c.name()).collect();
            p.insert("check".into(),json!({"type":"string","enum":names,"description":"Also list this row's Activity IDs or relationships"}));
            p.insert("limit".into(),json!({"type":"integer","minimum":1,"maximum":crate::checks::MAX_ITEMS,"default":crate::checks::DEFAULT_ITEMS,"description":"Most items listed"}));
            p.insert("long_lag_hours".into(),json!({"type":"number","minimum":0,"description":"Long lag limit in hours (default 352)"}));
            p.insert("large_float_hours".into(),json!({"type":"number","minimum":0,"description":"Large float limit in hours (default 352)"}));
            p.insert("large_duration_hours".into(),json!({"type":"number","minimum":0,"description":"Large duration limit in hours (default 352)"}));
        }
        if name == "import" {
            p.insert("workspace".into(),json!({"type":"string","minLength":1,"description":"Existing OpenXER workspace to import into; it is copied to output and not modified"}));
            p.insert("action".into(),json!({"type":"string","enum":["new","update","replace","add"],"description":"Import action for the XER project when workspace is given (add: Add Into Existing Project)"}));
            p.insert("target_workspace_id".into(),json!({"type":"integer","description":"Workspace row to update or replace"}));
            p.insert("config".into(),json!({"type":"string","description":"Per-type actions, e.g. activities=update+delete,calendars=keep"}));
            p.insert("dictionary".into(),json!({"type":"string","description":"A workspace with a shared dictionary: what a calendar, resource, code or other dictionary row whose name the workspace has with other values does, per kind, e.g. calendar=keep,resource=update (default copy: added as <name>_<n>)"}));
            p.insert("format".into(),json!({"type":"string","enum":["xer","xml"],"description":"Refuse the input unless it is in this format; without it the format is told from the contents"}));
        }
        if name == "export" {
            p.insert("version".into(),json!({"type":"string","minLength":1,"description":"XER header version label, e.g. 19.12"}));
            p.insert("projects".into(),json!({"type":"string","minLength":1,"description":"Comma separated project ids (workspace ids for a workspace input) to export together"}));
            p.insert("format".into(),json!({"type":"string","enum":["xer","xml"],"description":"File format; without it an output ending in .xml is project XML and any other is XER"}));
        }
        if name == "export_xlsx" {
            p.insert("layout".into(),json!({"type":"string","enum":["modern","tables"],"default":"modern"}));
            p.insert("unit".into(),json!({"type":"string","enum":["days","hours"],"default":"days"}));
            p.insert("columns".into(),json!({"type":"string","description":"Comma separated column keys or titles for the modern layout"}));
            p.insert("template".into(),json!({"type":"string","description":"Saved export template name (from the workspace input) or the path of a template .json file; sets columns, filter, sort and options"}));
        }
        if name == "export_pdf" {
            p.insert("layout".into(),json!({"type":"string","minLength":1,"description":"Saved layout name; the workspace's current layout when omitted"}));
            p.insert("view".into(),json!({"type":"string","enum":["gantt","network"]}));
            p.insert("paper".into(),json!({"type":"string","description":"Letter, Legal, Tabloid, Executive, A5, A4, A3, B4 (JIS), ANSI C, ANSI D or ANSI E"}));
            p.insert("orientation".into(),json!({"type":"string","enum":["portrait","landscape","automatic"]}));
        }
        if name == "export_scurve" {
            p.insert("measure".into(),json!({"type":"string","enum":["auto","labor","cost","duration"],"default":"auto","description":"auto: labor hours, else cost, else duration, by what the project holds"}));
            p.insert("period".into(),json!({"type":"string","enum":["day","week","month","financial"],"default":"week","description":"financial: the project's financial periods"}));
            p.insert("breakdown".into(),json!({"type":"string","enum":["total","resource","wbs1","wbs2"],"description":"Rows of the breakdown sheets; resource by default when the measure has resources, else total"}));
            p.insert("late".into(),json!({"type":"boolean","default":false,"description":"Add the late dates curve (actual plus remaining on the late dates)"}));
            p.insert("week_start".into(),json!({"type":"string","enum":["sunday","monday"],"default":"sunday"}));
        }
        if name == "export_earned_hours" {
            p.insert("breakdown".into(),json!({"type":"string","enum":["resource","wbs","activity"],"default":"resource","description":"Rows of the Earned to date sheet"}));
        }
        if name == "import_xlsx" {
            p.insert("spreadsheet".into(),json!({"type":"string","minLength":1,"description":"The .xlsx to import; read-only"}));
            p.insert("dry_run".into(),json!({"type":"boolean","default":false,"description":"Only return the preview"}));
        }
        if name == "usage" {
            p.insert("activity".into(),json!({"type":"string","minLength":1,"description":"Only this Activity ID's assignments"}));
            p.insert("resource".into(),json!({"type":"string","minLength":1,"description":"Only this resource's assignments (short name)"}));
            p.insert("interval".into(),json!({"type":"string","enum":["day","week","month"],"default":"week","description":"Period length; weeks start on Monday"}));
            p.insert("from".into(),json!({"type":"string","description":"Project-local YYYY-MM-DD or YYYY-MM-DD HH:MM; by default the first assignment window"}));
            p.insert("to".into(),json!({"type":"string","description":"Project-local date; by default the last assignment window"}));
            p.insert("costs".into(),json!({"type":"boolean","default":false,"description":"Costs instead of units"}));
        }
        if name == "records" {
            p.insert("kind".into(),json!({"type":"string","enum":["expenses","issues","risks","thresholds"]}));
            p.insert("activity".into(),json!({"type":"string","minLength":1,"description":"Only the records of this Activity ID"}));
        }
        if name == "earned_value" {
            p.insert("basis".into(),json!({"type":"string","enum":["cost","labor_units"],"default":"cost"}));
            p.insert("activity".into(),json!({"type":"string","minLength":1,"description":"Only this Activity ID's row (the project totals stay whole)"}));
        }
        if name == "documents" {
            p.insert("activity".into(),json!({"type":"string","minLength":1,"description":"Only documents linked to this Activity ID"}));
            p.insert("wbs".into(),json!({"type":"string","minLength":1,"description":"Only documents linked to this WBS element (code path 1.2.3) itself"}));
        }
        if name == "read_document" {
            p.insert("document".into(),json!({"type":"string","minLength":1,"description":"The document's reference number, title or document id (from openxer_documents)"}));
            p.insert("location".into(),json!({"type":"string","enum":["private","public"],"description":"Which file location to read; by default private when it is set, else public"}));
            p.insert("offset".into(),json!({"type":"integer","minimum":0,"default":0,"description":"First character (text and docx)"}));
            p.insert("limit".into(),json!({"type":"integer","minimum":1,"maximum":200000,"default":20000,"description":"Most characters returned"}));
            p.insert("page".into(),json!({"type":"integer","minimum":1,"default":1,"description":"First PDF page"}));
            p.insert("pages".into(),json!({"type":"integer","minimum":1,"maximum":100,"default":10,"description":"Most PDF pages returned"}));
        }
        if name == "relationship" {
            p.insert("edit".into(),object(json!({
                "action":{"type":"string","enum":["add","update","remove"]},
                "predecessor":{"type":"string","description":"Exact activity code"},"successor":{"type":"string","description":"Exact activity code"},
                "relationship_type":{"type":"string","enum":["FS","SS","FF","SF"]},
                "lag_minutes":{"type":"integer","minimum":-6000000,"maximum":6000000},
                "new_type":{"type":"string","enum":["FS","SS","FF","SF"]}
            }),&["action","predecessor","successor","relationship_type"]));
        }
        let required = if name == "relationship" { vec!["input","output","edit"] } else if name == "import_xlsx" { vec!["input","spreadsheet"] } else if name == "read_document" { vec!["input","document"] } else if name == "records" { vec!["input","kind"] } else if write { vec!["input","output"] } else { vec!["input"] };
        tools.push(json!({"name":format!("openxer_{name}"),"description":description,"inputSchema":object(Value::Object(p),&required),
            "outputSchema":{"type":"object","properties":{"schema_version":{"type":"string"},"ok":{"type":"boolean"},"result":{"type":"object"},"error":{"type":"object"}},"required":["schema_version","ok"]},
            "annotations":{"readOnlyHint":!write,"destructiveHint":false,"idempotentHint":!write,"openWorldHint":false}}));
    }
    tools.extend(edit_tool_definitions());
    tools.extend(eps_tool_definitions());
    tools.extend(crate::workspace_cmd::tool_definitions());
    tools.extend(crate::baseline_cmd::tool_definitions());
    tools.extend(analysis_tool_definitions());
    tools.extend(skill_tool_definitions());
    tools
}

/// What the server offers: tools, and one prompt per skill.
fn capabilities() -> Value {
    json!({"tools":{},"prompts":{"listChanged":false}})
}

/// The prompts: one per skill, named by its id.
fn prompt_list() -> Vec<Value> {
    crate::skills::catalog()
        .entries
        .iter()
        .map(|e| {
            json!({"name": e.skill.id, "title": e.skill.name,
                   "description": format!("{} ({})", e.skill.description, e.skill.kind.label())})
        })
        .collect()
}

const OUTPUT: &str = r#"{"type":"object","properties":{"schema_version":{"type":"string"},"ok":{"type":"boolean"},"result":{"type":"object"},"error":{"type":"object"}},"required":["schema_version","ok"]}"#;

fn read_only() -> Value {
    json!({"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false})
}

/// The read-only analysis tools: the numbers the skills report.
fn analysis_tool_definitions() -> Vec<Value> {
    let output: Value = serde_json::from_str(OUTPUT).expect("output schema");
    let mut tools = vec![];
    let common = |p: &mut Map<String, Value>| {
        p.insert("input".into(),json!({"type":"string","minLength":1,"description":"Local XER or OpenXER workspace path; read-only"}));
        p.insert("project_id".into(),json!({"type":"integer","description":"Project id (the XER proj_id) returned by inspect"}));
        p.insert("workspace_id".into(),json!({"type":"integer","description":"Workspace row id, useful when project ids repeat"}));
    };
    let defs: [(&str, &str, Value); 7] = [
        ("openxer_paths",
         "The chains of work into one activity (or the project finish when activity is omitted), ranked by float: path 1 drives it, path 2 is the next closest, with each path's lowest float, how far behind path 1 it is, what holds its first activity, its activities in order, merge points, and the counts of open work on the longest path and at zero or negative float. From the stored dates and float. Read-only.",
         json!({"activity":{"type":"string","minLength":1,"description":"Activity ID of the milestone or activity; the project finish when omitted"},
                "count":{"type":"integer","minimum":1,"maximum":10,"default":5,"description":"Number of paths"}})),
        ("openxer_compare",
         "Compare the project with a reference: an earlier XER or workspace (reference) or a stored baseline of a workspace project (baseline project or primary). Gives the project finish then and now, milestones moved, the biggest finish moves, activities added and deleted, ties added, removed and changed (type or lag), original durations changed, remaining duration up on work in progress, constraints and calendars changed, renamed, and progress (started, finished, actual dates rewritten, status gone back). Moves are calendar days. Read-only.",
         json!({"reference":{"type":"string","minLength":1,"description":"An earlier XER or workspace to compare with; read-only"},
                "reference_project_id":{"type":"integer","description":"Project id in the reference"},
                "reference_workspace_id":{"type":"integer","description":"Workspace row id in the reference"},
                "baseline":{"type":"string","enum":["project","primary"],"description":"A stored baseline of the workspace project instead of reference"}})),
        ("openxer_lookahead",
         "The work in progress or due in the next weeks from the data date (or from), by WBS: each activity with dates, float, resources, and whether it is in progress, ready (nothing before it is open) or waiting, with the open predecessors it waits on and their expected finish. Read-only.",
         json!({"weeks":{"type":"integer","minimum":1,"maximum":26,"default":3},
                "from":{"type":"string","description":"Project-local YYYY-MM-DD; the data date when omitted"}})),
        ("openxer_milestones",
         "Every milestone in date order: date, status, total float, constraints, the baseline date and the move since it (workspace project baseline), and the driving ties into it. Read-only.",
         json!({})),
        ("openxer_schedule_stats",
         "Counts and spreads for reviewing activities, durations, calendars and the WBS: activities by type and status, every calendar with its weekly hours, working days, nonwork exception dates and the activities on it, original durations in working days (median, 90th percentile, spread, the longest, zero-duration tasks, those longer than two update periods), repeated and one-word names, open work with no resource or expense, activities by WBS level and empty WBS elements. Read-only.",
         json!({"update_period_days":{"type":"number","description":"The update cycle in working days, to list activities longer than two periods"}})),
        ("openxer_resource_loading",
         "Each assigned resource's units per period (actual plus remaining, spread as the usage views spread them), its peak period, its limit in units per hour and the periods over the limit. Read-only; to see what leveling would move, schedule a copy with level_resources.",
         json!({"interval":{"type":"string","enum":["day","week","month"],"default":"week"}})),
        ("openxer_wbs_summary",
         "The schedule rolled up by WBS element down to level: activities, completed, start, finish, lowest total float, baseline finish and the move since it, and the milestones in each element. For a summary schedule and for checking that levels of detail agree. Read-only.",
         json!({"level":{"type":"integer","minimum":1,"maximum":10,"default":2}})),
    ];
    for (name, description, extra) in defs {
        let mut p = Map::new();
        common(&mut p);
        if let Some(extra) = extra.as_object() {
            p.extend(extra.clone());
        }
        tools.push(json!({"name":name,"description":description,"inputSchema":object(Value::Object(p),&["input"]),"outputSchema":output.clone(),"annotations":read_only()}));
    }
    tools
}

/// The skill tools: list and get.
fn skill_tool_definitions() -> Vec<Value> {
    let output: Value = serde_json::from_str(OUTPUT).expect("output schema");
    vec![
        json!({"name":"openxer_skills",
            "description":"List the skills: instructions for analyzing (kind check, read-only) and building or changing (kind workflow) schedules with these tools. Each has id, name, description, use_when, request, kind and source (built_in, yours or customized). Call this before analyzing or building a schedule and follow the skill that matches.",
            "inputSchema":object(json!({}),&[]),"outputSchema":output,"annotations":read_only()}),
        json!({"name":"openxer_skill",
            "description":"Get one skill's instructions by id (from openxer_skills) and follow them. Every number you report comes from a tool result; never compute schedule metrics yourself.",
            "inputSchema":object(json!({"id":{"type":"string","minLength":1}}),&["id"]),"outputSchema":output,"annotations":read_only()}),
    ]
}

/// `openxer_eps_edit` takes the operation's fields at the top; they go
/// into the `operation` of an `eps` command.
fn eps_arguments(args: Map<String, Value>) -> Map<String, Value> {
    let mut top = Map::new();
    let mut operation = Map::new();
    for (k, v) in args {
        if matches!(k.as_str(), "input" | "output" | "dry_run") {
            top.insert(k, v);
        } else {
            operation.insert(k, v);
        }
    }
    top.insert("operation".into(), Value::Object(operation));
    top
}

fn eps_tool_definitions() -> Vec<Value> {
    let output = json!({"type":"object","properties":{"schema_version":{"type":"string"},"ok":{"type":"boolean"},"result":{"type":"object"},"error":{"type":"object"}},"required":["schema_version","ok"]});
    let input = json!({"type":"string","minLength":1,"description":"OpenXER workspace database path; read-only"});
    let text = json!({"type":"string","minLength":1});
    let list = json!({"name":"openxer_eps_list",
        "description":"List the EPS (project folder tree) of an OpenXER workspace with every project and folder rollup: start, finish, late dates, data date (folders: latest), Must Finish By, baseline finish and finish variance (working hours), total float (lowest unfinished activity; folders: lowest project), duration-based schedule percent complete, labor units and costs (budgeted/actual/remaining) and activity counts. Read from stored projects, so closed projects are current. Use it to answer how all the jobs are doing.",
        "inputSchema":object(json!({"input":input}),&["input"]),"outputSchema":output,
        "annotations":{"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}});
    let edit = json!({"name":"openxer_eps_edit",
        "description":"Edit the EPS of an OpenXER workspace on a copy written to output (or dry_run): add_folder (name, eps_id, parent), rename_folder (folder, name, new_eps_id), move_folder (folder, parent; omitted parent is the top level), delete_folder (folder, delete_projects; deletes the folder, the folders under it and every project filed in them with their baselines; a folder holding projects is refused unless delete_projects is true, dry_run lists what would go; the last top-level folder is refused), file_project (workspace_id, folder), create_project (folder, project_id, planned_start, name, must_finish, manager, rate_type; the New Project wizard's rules). Folders are named by EPS ID from openxer_eps_list; dates are YYYY-MM-DD or YYYY-MM-DD HH:MM.",
        "inputSchema":object(json!({
            "input":input,
            "output":{"type":"string","minLength":1,"description":"New workspace path; existing files are refused. Omit with dry_run"},
            "dry_run":{"type":"boolean","default":false},
            "op":{"type":"string","enum":["add_folder","rename_folder","move_folder","delete_folder","file_project","create_project"]},
            "folder":text,"parent":text,"name":text,"eps_id":text,"new_eps_id":text,
            "workspace_id":{"type":"integer"},"project_id":text,"planned_start":text,"must_finish":text,
            "delete_projects":{"type":"boolean","default":false,"description":"delete_folder: also delete the projects filed in the folder and its subfolders, with their baselines"},
            "manager":text,"rate_type":{"type":"string","description":"COST_PER_QTY .. COST_PER_QTY5 or a rate type name"}
        }),&["input","op"]),"outputSchema":output,
        "annotations":{"readOnlyHint":false,"destructiveHint":false,"idempotentHint":false,"openWorldHint":false}});
    vec![list, edit]
}
