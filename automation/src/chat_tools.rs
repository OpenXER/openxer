//! The small tool set of OpenXER's Chat window.
//!
//! An agent answering questions about the project open in the app needs a
//! handful of read tools and one way to propose edits, not the full MCP
//! catalog: every tool described costs tokens on every turn. This set is
//! built from the full tool definitions (`mcp::tool_definitions`) and runs
//! through the same command dispatcher, so the chat and every other
//! client behave the same; only the descriptions are shorter, the
//! arguments that name files are filled in from the snapshot, results are
//! kept under [`RESULT_LIMIT`], and `openxer_apply` only ever proposes:
//! it runs as a dry run on the snapshot and writes nothing.
//!
//! The same functions serve `openxer-cli mcp --toolset chat` and callers
//! that run the tools in-process ([`tools`], [`call`]).
use crate::{dispatch, mcp, parse_command, response, AutomationError};
use openxer_xer::ProjectData;
use serde::Serialize;
use serde_json::{json, Value};
use std::path::PathBuf;

/// What the agent may do: Ask reads only; Edit may also propose changes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatMode {
    #[default]
    Ask,
    Edit,
}

impl ChatMode {
    pub fn id(self) -> &'static str {
        match self {
            ChatMode::Ask => "ask",
            ChatMode::Edit => "edit",
        }
    }
    pub fn parse(text: &str) -> Option<ChatMode> {
        match text {
            "ask" => Some(ChatMode::Ask),
            "edit" => Some(ChatMode::Edit),
            _ => None,
        }
    }
}

/// Where the chat tools read from and what they may do. With a snapshot
/// the tools take no `input`; without one each call names its file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChatContext {
    pub snapshot: Option<PathBuf>,
    pub mode: ChatMode,
    /// Optional folder of user-authored `.md` playbooks (`chat/playbooks`).
    pub playbooks_dir: Option<PathBuf>,
    /// Optional JSONL queue file where `openxer_ui` writes live desktop UI
    /// commands (filters, views, grouping, columns, dialogs, exports) for the
    /// open OpenXER window to execute on the next frame.
    pub ui_actions: Option<PathBuf>,
}

/// The live desktop UI control tool, available whenever `ui_actions` is set.
pub const UI_TOOL: &str = "openxer_ui";

/// One chat tool as an agent sees it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ChatTool {
    pub name: &'static str,
    pub description: String,
    pub input_schema: Value,
}

/// One tool call's answer: the response envelope (`schema_version`, `ok`,
/// `result` or `error`) and its text, which is what the agent reads.
#[derive(Clone, Debug, PartialEq)]
pub struct ChatResult {
    pub envelope: Value,
    pub text: String,
    pub is_error: bool,
}

/// The longest tool result handed to an agent, in bytes of JSON text. A
/// larger one is refused with `result_too_large` and a hint to narrow it.
pub const RESULT_LIMIT: usize = 40_000;

/// Activity fields `openxer_load` returns in the chat when none are named.
pub const DEFAULT_LOAD_FIELDS: &[&str] = &[
    "task_code",
    "name",
    "status",
    "early_start",
    "early_finish",
    "total_float_minutes",
];
/// Page size of `openxer_load` in the chat when none is given.
pub const DEFAULT_LOAD_LIMIT: usize = 50;

/// The propose-only edit tool.
pub const APPLY_TOOL: &str = "openxer_apply";

/// Interactive clarifying-question tool shown as a card with clickable options
/// and an Other / context input in the Chat window.
pub const ASK_USER_TOOL: &str = "openxer_ask_user";

/// The chat tools: name, short description, whether only Edit mode has it.
const TOOLS: &[(&str, &str, bool)] = &[
    ("openxer_load", "Read the open project: its options, activities and relationships. Default fields task_code, name, status, early_start, early_finish, total_float_minutes; 50 rows a page (offset pages on). Narrow with fields, activities (Activity IDs), wbs (code path) or critical; then the relationships are those touching the matched activities. sort orders them (-early_finish: latest first). limit 0 gives counts only. Floats and durations are working minutes.", false),
    ("openxer_records", "List the expense items, issues, risks or thresholds (kind), optionally of one activity.", false),
    ("openxer_earned_value", "Earned value per activity and for the project (BAC, PV, EV, AC, SV, CV, SPI, CPI, ETC, EAC, VAC), by cost or labor_units; activity keeps one row.", false),
    ("openxer_usage", "Budgeted, actual and remaining units (or costs) per day, week or month of one activity's or one resource's assignments.", false),
    ("openxer_documents", "The project's work products and documents, optionally only those linked to an activity or a WBS path.", false),
    ("openxer_read_document", "Read the text of a local text, Word or PDF document linked in the project (by reference number, title or document id from openxer_documents).", false),
    ("openxer_curves", "List the available resource curves (file curves and OpenXER presets) with their 21 distribution points.", false),
    ("openxer_check_schedule", "Check Schedule: the app's 15 health checks, one row each (count, of, result, target, pass, and the first five offenders). check (a row name) lists all its Activity IDs, up to limit.", false),
    ("openxer_playbook", "Scheduling methods. No name: the list. With name: that playbook. Load the matching one first for a build, update, audit or health check.", false),
    (APPLY_TOOL, "", true),
];

/// Operations the apply description spells out with their fields.
const SPELLED: &[&str] = &[
    "set_activity",
    "add_activity",
    "delete_activity",
    "dissolve_activity",
    "assign_resource",
    "remove_assignment",
    "set_assignment",
    "set_project",
    "add_wbs",
    "add_step",
    "set_step",
    "update_progress",
];

/// Short descriptions of the arguments that need one in the chat.
const ARGUMENT_HELP: &[(&str, &str)] = &[
    ("fields", "Activity fields to return"),
    ("activities", "Only these Activity IDs"),
    ("wbs", "WBS code path, e.g. 1.2"),
    ("critical", "Only critical activities"),
    ("sort", "Field to order by; -field descending"),
    ("limit", "Rows per page; 0 counts only"),
    (
        "relationship_limit",
        "Relationships per page; 0 counts only",
    ),
    ("activity", "Activity ID"),
    ("resource", "Resource short name"),
    (
        "changes",
        "{\"format\":\"openxer-changes/1\",\"operations\":[{\"op\":...},...]}",
    ),
];

/// Arguments the chat fills in or never takes.
const HIDDEN: &[&str] = &[
    "output",
    "project_id",
    "workspace_id",
    "dry_run",
    "changes_file",
    // A proposal is applied as the user saw it: only the changes document
    // (with its own recalculate) travels, so nothing outside it may change
    // the preview.
    "recalculate",
];

/// The `openxer_apply` description in the chat: proposal only, with the
/// common operations' fields taken from the single edit tools.
fn apply_description() -> String {
    let fields = |op: &str| -> String {
        let list: Vec<String> = mcp::edit_tool_fields(op)
            .iter()
            .map(|(f, required)| {
                if *required {
                    (*f).to_string()
                } else {
                    format!("{f}?")
                }
            })
            .collect();
        format!("{op}({})", list.join(", "))
    };
    let spelled: Vec<String> = SPELLED.iter().map(|op| fields(op)).collect();
    let others: Vec<&str> = crate::edit::OPERATIONS
        .iter()
        .copied()
        .filter(|op| !SPELLED.contains(op) && !matches!(*op, "relationship" | "schedule"))
        .collect();
    format!(
        "Propose changes to the open project as an openxer-changes/1 batch. Nothing is changed: it returns the field diff, and the user approves it in OpenXER. Operations: {}, relationship(action add|edit|remove, predecessor, successor, kind FS|SS|FF|SF, lag_hours?), schedule(level_resources?). Also: {}. set_activity fields include activity_name, original_duration, remaining_duration, start, finish, actual_start, actual_finish, physical_percent_complete, calendar, wbs, primary_constraint, primary_constraint_date. Activities by Activity ID, resources by short name, WBS by code path; durations and lags in hours; dates YYYY-MM-DD HH:MM; null clears.",
        spelled.join(", "),
        others.join(", ")
    )
}

/// `schema` without hidden arguments (and `input` when the snapshot fills
/// it), its property descriptions shortened or dropped.
fn chat_schema(schema: &Value, with_input: bool) -> Value {
    let mut schema = schema.clone();
    if let Some(props) = schema["properties"].as_object_mut() {
        props.retain(|k, _| !HIDDEN.contains(&k.as_str()) && (with_input || k != "input"));
        for (key, prop) in props.iter_mut() {
            if let Some(map) = prop.as_object_mut() {
                map.remove("description");
                if let Some((_, help)) = ARGUMENT_HELP.iter().find(|(k, _)| k == key) {
                    map.insert("description".into(), json!(help));
                }
                if key == "input" {
                    map.insert("description".into(), json!("XER or workspace path"));
                }
            }
        }
    }
    if let Some(required) = schema["required"].as_array_mut() {
        required.retain(|r| {
            r.as_str()
                .is_some_and(|r| !HIDDEN.contains(&r) && (with_input || r != "input"))
        });
    }
    if schema["required"].as_array().is_some_and(Vec::is_empty) {
        schema.as_object_mut().map(|m| m.remove("required"));
    }
    schema
}

/// The chat tools of `mode`. With `snapshot` true the snapshot names the
/// project and the tools take no `input`.
pub fn tools_for(mode: ChatMode, snapshot: bool) -> Vec<ChatTool> {
    let full = mcp::tool_definitions();
    TOOLS
        .iter()
        .filter(|(_, _, edit)| !*edit || mode == ChatMode::Edit)
        .map(|(name, description, _)| {
            let definition = full
                .iter()
                .find(|t| t["name"] == *name)
                .expect("every chat tool is an MCP tool");
            ChatTool {
                name,
                description: if *name == APPLY_TOOL {
                    apply_description()
                } else {
                    (*description).to_string()
                },
                input_schema: chat_schema(&definition["inputSchema"], !snapshot),
            }
        })
        .collect()
}

/// The live UI control tool definition (added whenever a desktop window is attached).
fn ui_tool() -> ChatTool {
    ChatTool {
        name: UI_TOOL,
        description: "Control the live OpenXER app window directly (safe in Ask and Edit modes; changes view state only, never edits schedule data). Actions: filter (standard name like Critical/Longest Path/Negative Float/In Progress/Milestone, Check Schedule name like Logic/Out of Sequence Activities, saved filter name, activities list, or criteria [{column,op,value,high?}] with optional match_all/highlight/save_as), clear_filter, group_sort (group_by, to_level, sort_by [{column,descending?}], show_group_totals, show_summaries_only, hide_if_empty, expand_all, collapse_all), columns (add, remove, set, reset), select (activities list), critical_path (activity?, direction backward|forward|both|off, paths?, group?), view (main, top, bottom, detail_tab), gantt (zoom fit|in|out, relationships?, scroll_to?), open_dialog (dialog name), export (format xlsx|scurve|earned_hours|pdf|xer|xml, path?), undo, redo.".into(),
        input_schema: json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["action"],
            "properties": {
                "action": {
                    "type": "string",
                    "enum": [
                        "filter", "clear_filter", "group_sort", "columns",
                        "select", "critical_path", "view", "gantt",
                        "open_dialog", "export", "undo", "redo"
                    ]
                },
                "filter": {"type": "string", "description": "Standard, Check Schedule or saved filter name"},
                "activities": {"type": "array", "items": {"type": "string"}, "description": "Activity IDs to filter or select"},
                "criteria": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "required": ["column", "op"],
                        "properties": {
                            "column": {"type": "string"},
                            "op": {"type": "string"},
                            "value": {"type": "string"},
                            "high": {"type": "string"}
                        }
                    },
                    "description": "Custom filter rows (op: equals, not_equals, contains, not_contains, greater_than, less_than, between, is_under, ...)"
                },
                "match_all": {"type": "boolean"},
                "highlight": {"type": "boolean"},
                "save_as": {"type": "string", "description": "Optional name to save a custom filter"},
                "group_by": {"type": "array", "items": {"type": "string"}},
                "to_level": {"type": "integer", "minimum": 1},
                "sort_by": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "required": ["column"],
                        "properties": {
                            "column": {"type": "string"},
                            "descending": {"type": "boolean"}
                        }
                    }
                },
                "show_group_totals": {"type": "boolean"},
                "show_summaries_only": {"type": "boolean"},
                "hide_if_empty": {"type": "boolean"},
                "expand_all": {"type": "boolean"},
                "collapse_all": {"type": "boolean"},
                "add": {"type": "array", "items": {"type": "string"}},
                "remove": {"type": "array", "items": {"type": "string"}},
                "set": {"type": "array", "items": {"type": "string"}},
                "reset": {"type": "boolean"},
                "activity": {"type": "string"},
                "direction": {"type": "string", "enum": ["backward", "forward", "both", "off"]},
                "paths": {"type": "integer", "minimum": 1, "maximum": 50},
                "group": {"type": "boolean"},
                "main": {"type": "string"},
                "top": {"type": "string"},
                "bottom": {"type": "string"},
                "detail_tab": {"type": "string"},
                "zoom": {"type": "string", "enum": ["fit", "in", "out"]},
                "relationships": {"type": "boolean"},
                "scroll_to": {"type": "string"},
                "dialog": {"type": "string"},
                "format": {"type": "string", "enum": ["xlsx", "scurve", "earned_hours", "pdf", "xer", "xml"]},
                "path": {"type": "string"}
            }
        }),
    }
}

fn ask_user_tool() -> ChatTool {
    ChatTool {
        name: ASK_USER_TOOL,
        description: "Ask the user one or more clarifying questions in Chat with clickable suggested options and an optional free-text context box. Use this when a request is ambiguous or has meaningful choices, then stop and wait for the user's reply.".to_string(),
        input_schema: json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["questions"],
            "properties": {
                "title": {
                    "type": "string",
                    "description": "Optional short heading for the question card."
                },
                "questions": {
                    "type": "array",
                    "minItems": 1,
                    "items": {
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["question", "options"],
                        "properties": {
                            "question": {
                                "type": "string",
                                "description": "The question to ask the user."
                            },
                            "multi_select": {
                                "type": "boolean",
                                "description": "Allow picking more than one option."
                            },
                            "options": {
                                "type": "array",
                                "minItems": 1,
                                "items": {
                                    "type": "object",
                                    "additionalProperties": false,
                                    "required": ["label"],
                                    "properties": {
                                        "label": {
                                            "type": "string",
                                            "description": "Short clickable option label."
                                        },
                                        "description": {
                                            "type": "string",
                                            "description": "Optional brief explanation."
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }),
    }
}

/// The chat tools of `context`.
pub fn tools(context: &ChatContext) -> Vec<ChatTool> {
    let mut list = tools_for(context.mode, context.snapshot.is_some());
    if let Some(dir) = &context.playbooks_dir {
        let all = crate::playbooks::all_playbooks(Some(dir));
        let names: Vec<String> = all.into_iter().map(|p| p.name).collect();
        if let Some(pb) = list.iter_mut().find(|t| t.name == "openxer_playbook") {
            pb.input_schema["properties"]["name"]["enum"] = json!(names);
        }
    }
    if context.ui_actions.is_some() {
        list.push(ui_tool());
        list.push(ask_user_tool());
    }
    list
}

/// The tools as MCP `tools/list` entries.
pub fn tool_list(context: &ChatContext) -> Vec<Value> {
    tools(context)
        .into_iter()
        .map(|t| {
            json!({"name":t.name,"description":t.description,"inputSchema":t.input_schema,
                "annotations":{"readOnlyHint":true,"destructiveHint":false,"openWorldHint":false}})
        })
        .collect()
}

/// The MCP server instructions in the chat.
pub fn instructions(mode: ChatMode) -> String {
    let mut text = String::from("OpenXER tools for the project open in the OpenXER app. Read it with openxer_load in small pages, choosing fields and filters. Name activities by Activity ID. Dates are project-local.");
    if mode == ChatMode::Edit {
        text.push_str(" To change the project call openxer_apply: it only proposes the batch, and the user approves it in OpenXER.");
    }
    text
}

fn refuse(code: &str, message: impl Into<String>) -> ChatResult {
    finish(response(Err(AutomationError::new(code, message))))
}

fn finish(envelope: Value) -> ChatResult {
    let text = envelope.to_string();
    let is_error = envelope["ok"] != true;
    ChatResult {
        envelope,
        text,
        is_error,
    }
}

/// Run chat tool `name` with `arguments` (a JSON object), as the chat MCP
/// server does. Never panics on bad input; every failure is an error
/// envelope.
pub fn call(context: &ChatContext, name: &str, arguments: Value) -> ChatResult {
    call_limited(context, name, arguments, RESULT_LIMIT)
}

/// [`call`] with another result size limit in bytes.
pub fn call_limited(
    context: &ChatContext,
    name: &str,
    arguments: Value,
    limit: usize,
) -> ChatResult {
    let Some(tool) = tools(context).into_iter().find(|t| t.name == name) else {
        return refuse(
            "unknown_tool",
            format!(
                "{name} is not a chat tool{}",
                if name == APPLY_TOOL {
                    " in Ask mode; switch the chat to Edit to propose changes"
                } else {
                    ""
                }
            ),
        );
    };
    let Value::Object(mut args) = arguments else {
        return refuse("invalid_arguments", "Tool arguments must be an object");
    };
    if let Err(message) = mcp::validate_arguments(
        &Value::Object(args.clone()),
        &tool.input_schema,
        "arguments",
    ) {
        return refuse("invalid_arguments", message);
    }
    // The snapshot is the input of every tool that reads the project.
    let reads_project = mcp::tool_definitions()
        .iter()
        .find(|t| t["name"] == name)
        .is_some_and(|t| t["inputSchema"]["properties"].get("input").is_some());
    if let Some(snapshot) = context.snapshot.as_ref().filter(|_| reads_project) {
        args.insert("input".into(), json!(snapshot));
    }
    if name == "openxer_load" {
        args.entry("fields").or_insert(json!(DEFAULT_LOAD_FIELDS));
        args.entry("limit").or_insert(json!(DEFAULT_LOAD_LIMIT));
        args.entry("relationship_limit")
            .or_insert(json!(DEFAULT_LOAD_LIMIT));
    }
    if name == APPLY_TOOL {
        // A proposal: always the dry run, never a file.
        args.insert("dry_run".into(), json!(true));
        args.remove("output");
    }
    if name == UI_TOOL || name == ASK_USER_TOOL {
        let Some(queue) = context.ui_actions.as_ref() else {
            return refuse(
                "unavailable",
                format!("{name} requires a live OpenXER desktop window"),
            );
        };
        if let Some(parent) = queue.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let mut payload = Value::Object(args.clone());
        if name == ASK_USER_TOOL {
            if let Some(obj) = payload.as_object_mut() {
                obj.insert("action".into(), json!("ask_user"));
            }
        }
        let line = format!("{payload}\n");
        use std::io::Write;
        let write = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(queue)
            .and_then(|mut f| f.write_all(line.as_bytes()));
        if let Err(e) = write {
            return refuse("io_error", format!("Cannot queue UI action: {e}"));
        }
        if name == ASK_USER_TOOL {
            return finish(response(Ok(json!({
                "shown": true,
                "note": "The question card is shown to the user in Chat with clickable options and an Other context box. End your turn and wait for the user's answer."
            }))));
        }
        let action = payload["action"].as_str().unwrap_or("");
        return finish(response(Ok(json!({
            "applied": true,
            "action": action,
            "note": "Queued for the live OpenXER window to execute immediately."
        }))));
    }
    if name == "openxer_playbook" {
        let pb_name = args.get("name").and_then(Value::as_str);
        let envelope = response(crate::playbooks::playbook_in(
            pb_name,
            context.playbooks_dir.as_deref(),
        ));
        let done = finish(envelope);
        if done.text.len() > limit {
            return refuse(
                "result_too_large",
                format!(
                    "The result is {} bytes, over the chat limit of {limit}.",
                    done.text.len()
                ),
            );
        }
        return done;
    }
    let command = mcp::tool_command(name).expect("chat tools are MCP tools");
    args.insert("command".into(), json!(command));
    let mut envelope = response(parse_command(Value::Object(args)).and_then(dispatch));
    if name == APPLY_TOOL && envelope["ok"] == true {
        let result = &mut envelope["result"];
        if let Some(map) = result.as_object_mut() {
            map.remove("output");
            map.remove("dry_run");
        }
        result["proposed"] = json!(true);
        result["note"] =
            json!("Not applied. The user reviews this diff and approves it in OpenXER.");
    }
    let done = finish(envelope);
    if done.text.len() > limit {
        return refuse(
            "result_too_large",
            format!(
                "The result is {} bytes, over the chat limit of {limit}. Ask for less: fewer fields, a smaller limit, or filter by activities, wbs or critical.",
                done.text.len()
            ),
        );
    }
    done
}

/// Apply an approved proposal (the `changes` of a proposed-changes event)
/// to the project open in the app, exactly as its preview ran: the same
/// document, the document's own `recalculate`, and no stored baselines
/// (the preview read an XER snapshot, which has none; a proposal holding
/// `monitor_thresholds` therefore measures against the project itself).
/// Returns the same report, field diff included; on `Err` the project is
/// unchanged. Previewed and applied on the same project, the two diffs are
/// equal.
pub fn apply_proposal(project: &mut ProjectData, changes: Value) -> crate::Result<Value> {
    crate::edit::apply_changes(project, changes, None, &Default::default())
}

/// A few lines about `project` for the start of a chat, so the agent
/// rarely needs a discovery call: the project, its dates and counts and
/// the activities selected in the app (by Activity ID; at most 20 named).
pub fn preamble(project: &ProjectData, selected: &[String]) -> String {
    let p = &project.model.project;
    let date = |v: Option<i64>| {
        let text = openxer_xer::date_string(project.model.origin_serial, v);
        if text.is_empty() {
            "none".to_string()
        } else {
            text
        }
    };
    // The Project ID is the short name; the project's name is that of its
    // WBS project node.
    let pid = p.proj_id.to_string();
    let title = project
        .document
        .table("PROJWBS")
        .and_then(|t| {
            t.rows
                .iter()
                .find(|r| t.value(r, "proj_id") == pid && t.value(r, "proj_node_flag") == "Y")
                .map(|r| t.value(r, "wbs_name").to_string())
        })
        .filter(|s| !s.is_empty() && *s != p.name);
    let mut lines = vec![
        match title {
            Some(title) => format!("Open project: {} \"{title}\".", p.name),
            None => format!("Open project: {}.", p.name),
        },
        format!(
            "Data date {}; scheduled finish {}; must finish {}.",
            date(p.data_date),
            date(p.scd_end),
            date(p.plan_end)
        ),
        format!(
            "{} activities, {} relationships.",
            project.model.activities.len(),
            project.model.links.len()
        ),
    ];
    if !selected.is_empty() {
        let named: Vec<String> = selected
            .iter()
            .take(20)
            .map(|code| {
                let name = project
                    .model
                    .activities
                    .iter()
                    .find(|a| &a.task_code == code)
                    .and_then(|a| project.details.get(&a.task_id))
                    .map_or("", |d| d.name.as_str());
                if name.is_empty() {
                    code.clone()
                } else {
                    format!("{code} {name}")
                }
            })
            .collect();
        let more = selected.len().saturating_sub(20);
        lines.push(format!(
            "Selected: {}{}.",
            named.join("; "),
            if more > 0 {
                format!("; and {more} more")
            } else {
                String::new()
            }
        ));
    }
    lines.join("\n")
}

/// Arguments for the MCP server of a chat: `mcp --toolset chat`, the
/// snapshot, the mode and optional custom playbooks folder.
pub fn server_arguments(context: &ChatContext) -> Vec<String> {
    let mut args = vec!["mcp".to_string(), "--toolset".into(), "chat".into()];
    if let Some(snapshot) = &context.snapshot {
        args.push("--snapshot".into());
        args.push(snapshot.display().to_string());
    }
    args.push("--mode".into());
    args.push(context.mode.id().into());
    if let Some(dir) = &context.playbooks_dir {
        args.push("--playbooks-dir".into());
        args.push(dir.display().to_string());
    }
    if let Some(ui_actions) = &context.ui_actions {
        args.push("--ui-actions".into());
        args.push(ui_actions.display().to_string());
    }
    args
}
