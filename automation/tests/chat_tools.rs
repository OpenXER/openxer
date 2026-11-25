//! The Chat window's tool set (`mcp --toolset chat`), the compact load
//! options through the CLI and MCP, and applying a change batch to a
//! project in memory, on the synthetic sample project.
use openxer_automation::{
    chat_tools::{self, ChatContext, ChatMode},
    cli::{self, Invocation},
    dispatch, edit,
    mcp::{tool_definitions, Server},
    parse_command, response,
};
use openxer_xer::{sample_project::sample_project, Document, ProjectData};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// The sample project as an XER in `dir`.
fn sample(dir: &Path) -> PathBuf {
    let path = dir.join("sample.xer");
    sample_project()
        .current
        .updated_document()
        .write(&path)
        .unwrap();
    path
}

fn context(snapshot: &Path, mode: ChatMode) -> ChatContext {
    ChatContext {
        snapshot: Some(snapshot.to_path_buf()),
        mode,
        playbooks_dir: None,
        ui_actions: None,
    }
}

fn run(request: Value) -> Value {
    response(parse_command(request).and_then(dispatch))
}

fn names(tools: &[Value]) -> Vec<String> {
    tools
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect()
}

/// A legacy MCP session: initialize, then the requests.
fn session(mut server: Server, requests: &[Value]) -> Vec<Value> {
    let init = server
        .handle(json!({"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}))
        .unwrap();
    server.handle(json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
    let mut out = vec![init];
    for (n, r) in requests.iter().enumerate() {
        let mut r = r.clone();
        r["jsonrpc"] = json!("2.0");
        r["id"] = json!(n + 1);
        out.push(server.handle(r).unwrap());
    }
    out
}

fn files_in(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut list: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| {
            let e = e.unwrap();
            (
                e.file_name().to_string_lossy().into_owned(),
                std::fs::read(e.path()).unwrap_or_default(),
            )
        })
        .collect();
    list.sort();
    list
}

#[test]
fn the_chat_tool_set_is_small_and_read_only_in_ask_mode() {
    let ask = chat_tools::tools_for(ChatMode::Ask, true);
    let edit = chat_tools::tools_for(ChatMode::Edit, true);
    let ask_names: Vec<&str> = ask.iter().map(|t| t.name).collect();
    let edit_names: Vec<&str> = edit.iter().map(|t| t.name).collect();
    assert_eq!(
        ask_names,
        [
            "openxer_load",
            "openxer_records",
            "openxer_earned_value",
            "openxer_usage",
            "openxer_documents",
            "openxer_read_document",
            "openxer_curves",
            "openxer_check_schedule",
            "openxer_playbook"
        ]
    );
    assert_eq!(edit_names[..9], ask_names[..]);
    assert_eq!(edit_names[9], "openxer_apply");
    // Every chat tool is a tool of the full set, with the same arguments
    // apart from those the snapshot fills in or the chat never takes.
    let full = tool_definitions();
    for tool in &edit {
        let def = full.iter().find(|t| t["name"] == tool.name).unwrap();
        let own: Vec<&String> = tool.input_schema["properties"]
            .as_object()
            .unwrap()
            .keys()
            .collect();
        for key in &own {
            assert!(
                def["inputSchema"]["properties"].get(key.as_str()).is_some(),
                "{} {key}",
                tool.name
            );
        }
        for hidden in ["input", "output", "dry_run", "workspace_id", "project_id"] {
            assert!(
                tool.input_schema["properties"].get(hidden).is_none(),
                "{} {hidden}",
                tool.name
            );
        }
        assert!(!tool.description.is_empty());
    }
    // The apply description names every operation.
    let apply = &edit[9].description;
    for op in edit::OPERATIONS {
        assert!(apply.contains(op), "{op}");
    }
    assert!(
        apply.contains("set_activity(activity, field, value)"),
        "{apply}"
    );
}

#[test]
fn without_a_snapshot_each_call_names_its_file() {
    let dir = tempfile::tempdir().unwrap();
    let xer = sample(dir.path());
    let tools = chat_tools::tools_for(ChatMode::Ask, false);
    let load = tools.iter().find(|t| t.name == "openxer_load").unwrap();
    assert_eq!(load.input_schema["required"], json!(["input"]));
    let free = ChatContext {
        snapshot: None,
        mode: ChatMode::Ask,
        playbooks_dir: None,
        ui_actions: None,
    };
    let missing = chat_tools::call(&free, "openxer_load", json!({}));
    assert!(missing.is_error);
    assert!(
        missing.text.contains("input is required"),
        "{}",
        missing.text
    );
    let named = chat_tools::call(&free, "openxer_load", json!({"input":xer,"limit":1}));
    assert!(!named.is_error, "{}", named.text);
    // With a snapshot no input is taken at all.
    let fixed = context(&xer, ChatMode::Ask);
    let given = chat_tools::call(&fixed, "openxer_load", json!({"input":xer}));
    assert!(given.is_error);
    assert!(given.text.contains("Unknown field"), "{}", given.text);
    let plain = chat_tools::call(&fixed, "openxer_load", json!({}));
    assert!(!plain.is_error, "{}", plain.text);
}

#[test]
fn chat_load_defaults_to_a_small_page_of_a_few_fields() {
    let dir = tempfile::tempdir().unwrap();
    let xer = sample(dir.path());
    let r = chat_tools::call(&context(&xer, ChatMode::Ask), "openxer_load", json!({}));
    assert!(!r.is_error, "{}", r.text);
    let result = &r.envelope["result"];
    let activities = result["activities"].as_array().unwrap();
    assert_eq!(activities.len(), chat_tools::DEFAULT_LOAD_LIMIT);
    let mut keys: Vec<&String> = activities[0].as_object().unwrap().keys().collect();
    keys.sort();
    let mut expected: Vec<&str> = chat_tools::DEFAULT_LOAD_FIELDS.to_vec();
    expected.sort();
    assert_eq!(keys, expected);
    assert_eq!(result["next_offset"], chat_tools::DEFAULT_LOAD_LIMIT);
    assert_eq!(
        result["relationships"].as_array().unwrap().len(),
        chat_tools::DEFAULT_LOAD_LIMIT
    );
}

#[test]
fn proposed_changes_never_write_a_file() {
    let dir = tempfile::tempdir().unwrap();
    let xer = sample(dir.path());
    let before = files_in(dir.path());
    let changes = json!({"format":"openxer-changes/1","operations":[
        {"op":"set_activity","activity":"PM0910","field":"remaining_duration","value":40},
        {"op":"set_activity","activity":"PM0910","field":"activity_name","value":"Submit as-built drawings"}
    ]});
    let r = chat_tools::call(
        &context(&xer, ChatMode::Edit),
        "openxer_apply",
        json!({"changes":changes}),
    );
    assert!(!r.is_error, "{}", r.text);
    let result = &r.envelope["result"];
    assert_eq!(result["proposed"], true);
    assert!(result.get("output").is_none());
    assert!(result["report"]["diff"].is_object(), "{result}");
    assert!(r.text.contains("Submit as-built drawings"));
    assert_eq!(files_in(dir.path()), before, "nothing was written");
    // output is not an argument of the chat apply at all.
    let out = dir.path().join("new.xer");
    let refused = chat_tools::call(
        &context(&xer, ChatMode::Edit),
        "openxer_apply",
        json!({"changes":changes,"output":out}),
    );
    assert!(refused.is_error);
    assert!(!out.exists());
    // Through the MCP server too.
    let replies = session(
        Server::chat(context(&xer, ChatMode::Edit)),
        &[
            json!({"method":"tools/call","params":{"name":"openxer_apply","arguments":{"changes":changes}}}),
        ],
    );
    let call = &replies[1]["result"];
    assert_eq!(call["isError"], false, "{call}");
    assert!(call.get("structuredContent").is_none());
    let text: Value = serde_json::from_str(call["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(text["result"]["proposed"], true);
    assert_eq!(files_in(dir.path()), before, "nothing was written");
}

#[test]
fn ask_mode_offers_no_apply() {
    let dir = tempfile::tempdir().unwrap();
    let xer = sample(dir.path());
    let ask = context(&xer, ChatMode::Ask);
    let r = chat_tools::call(
        &ask,
        "openxer_apply",
        json!({"changes":{"format":"openxer-changes/1","operations":[{"op":"delete_activity","activity":"PM0110"}]}}),
    );
    assert!(r.is_error);
    assert_eq!(r.envelope["error"]["code"], "unknown_tool");
    let replies = session(
        Server::chat(ask),
        &[
            json!({"method":"tools/list","params":{}}),
            json!({"method":"tools/call","params":{"name":"openxer_apply","arguments":{"changes":{}}}}),
            json!({"method":"tools/call","params":{"name":"openxer_set_activity","arguments":{}}}),
        ],
    );
    let listed = names(replies[1]["result"]["tools"].as_array().unwrap());
    assert!(!listed.iter().any(|n| n == "openxer_apply"));
    assert_eq!(replies[2]["error"]["message"], "Unknown OpenXER tool");
    assert_eq!(replies[3]["error"]["message"], "Unknown OpenXER tool");
    // Every listed tool says it is read-only.
    for t in replies[1]["result"]["tools"].as_array().unwrap() {
        assert_eq!(t["annotations"]["readOnlyHint"], true);
    }
}

#[test]
fn oversized_results_are_refused_with_a_hint() {
    let dir = tempfile::tempdir().unwrap();
    let xer = sample(dir.path());
    let r = chat_tools::call_limited(
        &context(&xer, ChatMode::Ask),
        "openxer_load",
        json!({"limit":200}),
        5_000,
    );
    assert!(r.is_error);
    assert_eq!(r.envelope["error"]["code"], "result_too_large");
    assert!(r.text.len() < 1_000);
    assert!(r.text.contains("fewer fields"));
}

/// Serialized size of the replies of a session's initialize and tools/list.
fn handshake_bytes(server: Server) -> (usize, usize, usize) {
    let replies = session(server, &[json!({"method":"tools/list","params":{}})]);
    let tools = replies[1]["result"]["tools"].as_array().unwrap().len();
    (
        replies[0].to_string().len(),
        replies[1].to_string().len(),
        tools,
    )
}

/// The budgets the chat tool set must stay under, set after measuring
/// (full set: 79 tools, about 118 KB; chat set: about 6.7 KB Ask, 9.2 KB
/// Edit; 50 activities with 5 fields about 9.5 KB). Every byte of the
/// tool list is sent to the model on every turn.
const ASK_BUDGET: usize = 7_500;
const EDIT_BUDGET: usize = 10_000;
const LOAD_50_BUDGET: usize = 11_000;

#[test]
fn the_chat_tool_set_stays_under_its_byte_budget() {
    let dir = tempfile::tempdir().unwrap();
    let xer = sample(dir.path());
    let full = handshake_bytes(Server::default());
    let ask = handshake_bytes(Server::chat(context(&xer, ChatMode::Ask)));
    let edit = handshake_bytes(Server::chat(context(&xer, ChatMode::Edit)));
    println!(
        "full: {} tools, initialize {} B, tools/list {} B",
        full.2, full.0, full.1
    );
    println!(
        "chat ask: {} tools, initialize {} B, tools/list {} B",
        ask.2, ask.0, ask.1
    );
    println!(
        "chat edit: {} tools, initialize {} B, tools/list {} B",
        edit.2, edit.0, edit.1
    );
    assert!(ask.0 + ask.1 < ASK_BUDGET, "{ask:?}");
    assert!(edit.0 + edit.1 < EDIT_BUDGET, "{edit:?}");
    assert!(
        (edit.0 + edit.1) * 5 < full.0 + full.1,
        "{edit:?} vs {full:?}"
    );
}

#[test]
fn loading_five_fields_of_fifty_activities_stays_under_its_budget() {
    let dir = tempfile::tempdir().unwrap();
    let xer = sample(dir.path());
    let r = chat_tools::call(
        &context(&xer, ChatMode::Ask),
        "openxer_load",
        json!({"fields":["task_code","name","status","early_finish","total_float_minutes"],"limit":50,"relationship_limit":0}),
    );
    assert!(!r.is_error, "{}", r.text);
    assert_eq!(
        r.envelope["result"]["activities"].as_array().unwrap().len(),
        50
    );
    let everything = run(json!({"command":"load","input":xer,"limit":50,"relationship_limit":50}));
    println!(
        "load of 50 activities: 5 fields {} B, every field with 50 relationships {} B",
        r.text.len(),
        everything.to_string().len()
    );
    assert!(r.text.len() < LOAD_50_BUDGET, "{}", r.text.len());
}

#[test]
fn chat_server_instructions_are_short_and_name_the_mode() {
    let dir = tempfile::tempdir().unwrap();
    let xer = sample(dir.path());
    let ask = session(Server::chat(context(&xer, ChatMode::Ask)), &[]);
    let edit = session(Server::chat(context(&xer, ChatMode::Edit)), &[]);
    let ask = ask[0]["result"]["instructions"].as_str().unwrap();
    let edit = edit[0]["result"]["instructions"].as_str().unwrap();
    assert!(ask.len() < 400 && edit.len() < 600);
    assert!(!ask.contains("openxer_apply"));
    assert!(edit.contains("openxer_apply"));
}

#[test]
fn the_cli_starts_the_chat_server_with_its_snapshot_and_mode() {
    let parsed = cli::parse([
        "mcp",
        "--toolset",
        "chat",
        "--snapshot",
        "s.xer",
        "--mode",
        "edit",
    ])
    .unwrap();
    match parsed {
        Invocation::ChatMcp(c) => {
            assert_eq!(c.snapshot, Some(PathBuf::from("s.xer")));
            assert_eq!(c.mode, ChatMode::Edit);
        }
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        cli::parse(["mcp", "--toolset", "chat"]).unwrap(),
        Invocation::ChatMcp(ChatContext {
            snapshot: None,
            mode: ChatMode::Ask,
            playbooks_dir: None,
            ui_actions: None,
        })
    ));
    assert!(matches!(cli::parse(["mcp"]).unwrap(), Invocation::Mcp));
    assert!(cli::parse(["mcp", "--toolset", "all"]).is_err());
    assert!(cli::parse(["mcp", "--mode", "edit"]).is_err());
    assert!(cli::parse(["mcp", "--toolset", "chat", "--mode", "write"]).is_err());
    let args = chat_tools::server_arguments(&context(Path::new("s.xer"), ChatMode::Ask));
    assert_eq!(
        args,
        [
            "mcp",
            "--toolset",
            "chat",
            "--snapshot",
            "s.xer",
            "--mode",
            "ask"
        ]
    );
    let with_ui = ChatContext {
        snapshot: Some(PathBuf::from("s.xer")),
        mode: ChatMode::Ask,
        playbooks_dir: None,
        ui_actions: Some(PathBuf::from("ui.jsonl")),
    };
    let ui_args = chat_tools::server_arguments(&with_ui);
    assert!(ui_args
        .windows(2)
        .any(|w| w == ["--ui-actions", "ui.jsonl"]));
    let parsed_ui = cli::parse(ui_args).unwrap();
    assert!(matches!(
        parsed_ui,
        Invocation::ChatMcp(ChatContext { ui_actions: Some(p), .. }) if p == Path::new("ui.jsonl")
    ));
}

#[test]
fn openxer_ui_tool_is_offered_only_with_queue_and_appends_validated_actions() {
    let dir = tempfile::tempdir().unwrap();
    let xer = sample(dir.path());
    let queue = dir.path().join("ui-actions.jsonl");
    let without = context(&xer, ChatMode::Ask);
    assert!(!chat_tools::tools(&without)
        .iter()
        .any(|t| t.name == "openxer_ui"));
    assert!(!chat_tools::tools(&without)
        .iter()
        .any(|t| t.name == "openxer_ask_user"));
    let with = ChatContext {
        snapshot: Some(xer),
        mode: ChatMode::Ask,
        playbooks_dir: None,
        ui_actions: Some(queue.clone()),
    };
    assert!(chat_tools::tools(&with)
        .iter()
        .any(|t| t.name == "openxer_ui"));
    assert!(chat_tools::tools(&with)
        .iter()
        .any(|t| t.name == "openxer_ask_user"));
    let ok = chat_tools::call(
        &with,
        "openxer_ui",
        json!({"action": "filter", "filter": "Critical"}),
    );
    assert!(!ok.is_error, "{}", ok.text);
    let ask_ok = chat_tools::call(
        &with,
        "openxer_ask_user",
        json!({
            "title": "Baseline choice",
            "questions": [{
                "question": "Which baseline should I compare against?",
                "options": [
                    {"label": "Current Baseline"},
                    {"label": "Primary Baseline"}
                ]
            }]
        }),
    );
    assert!(!ask_ok.is_error, "{}", ask_ok.text);
    let bad = chat_tools::call(&with, "openxer_ui", json!({"action": "nope"}));
    assert!(bad.is_error);
    let lines: Vec<String> = std::fs::read_to_string(&queue)
        .unwrap()
        .lines()
        .map(String::from)
        .collect();
    assert_eq!(lines.len(), 2);
    let v: Value = serde_json::from_str(&lines[0]).unwrap();
    assert_eq!(v["action"], "filter");
    assert_eq!(v["filter"], "Critical");
    let qv: Value = serde_json::from_str(&lines[1]).unwrap();
    assert_eq!(qv["action"], "ask_user");
    assert_eq!(qv["title"], "Baseline choice");
}

#[test]
fn the_preamble_names_the_project_its_dates_and_the_selection() {
    let p = sample_project().current;
    let text = chat_tools::preamble(&p, &["PM0110".into(), "NOPE".into()]);
    assert!(text.contains("RIVERBEND"), "{text}");
    assert!(text.contains("Riverbend Process Water Upgrade"));
    assert!(text.contains(&format!("{} activities", p.model.activities.len())));
    assert!(text.contains(&format!("{} relationships", p.model.links.len())));
    assert!(text.contains("Data date 20"));
    assert!(text.contains("scheduled finish 20"));
    assert!(text.contains("PM0110 Submit project schedule"), "{text}");
    assert!(text.contains("NOPE"));
    assert!(text.lines().count() <= 4);
    let many: Vec<String> = p
        .model
        .activities
        .iter()
        .map(|a| a.task_code.clone())
        .collect();
    let long = chat_tools::preamble(&p, &many);
    assert!(long.contains(&format!("and {} more", many.len() - 20)));
}

// ---------------------------------------------------------- load options

#[test]
fn load_picks_fields_and_filters_by_activity_wbs_and_critical() {
    let dir = tempfile::tempdir().unwrap();
    let xer = sample(dir.path());
    let r = run(
        json!({"command":"load","input":xer,"fields":["name","wbs","critical"],
        "activities":["PM0110","PM0100","MISSING"]}),
    );
    assert_eq!(r["ok"], true, "{r}");
    let result = &r["result"];
    let rows = result["activities"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    let mut keys: Vec<&String> = rows[0].as_object().unwrap().keys().collect();
    keys.sort();
    assert_eq!(keys, ["critical", "name", "task_code", "wbs"]);
    assert_eq!(rows[0]["wbs"], "PM");
    assert_eq!(result["matched_activities"], 2);
    assert_eq!(result["not_found"], json!(["MISSING"]));
    for link in result["relationships"].as_array().unwrap() {
        assert!(
            ["PM0110", "PM0100"].contains(&link["predecessor"].as_str().unwrap())
                || ["PM0110", "PM0100"].contains(&link["successor"].as_str().unwrap())
        );
    }
    assert_eq!(
        result["matched_relationships"].as_u64().unwrap() as usize,
        result["relationships"].as_array().unwrap().len()
    );
    // A WBS element and everything under it.
    let eng = run(json!({"command":"load","input":xer,"wbs":"ENG","fields":["wbs"],"limit":2000}));
    let rows = eng["result"]["activities"].as_array().unwrap();
    assert!(!rows.is_empty());
    assert!(rows
        .iter()
        .all(|a| a["wbs"].as_str().unwrap().starts_with("ENG")));
    assert!(rows.iter().any(|a| a["wbs"] == "ENG.CS"), "{rows:?}");
    // Critical by the project's definition.
    let critical = run(
        json!({"command":"load","input":xer,"critical":true,"fields":["critical"],"limit":2000}),
    );
    let rows = critical["result"]["activities"].as_array().unwrap();
    assert!(!rows.is_empty());
    assert!(rows.iter().all(|a| a["critical"] == true));
    let all = run(json!({"command":"load","input":xer,"fields":["critical"],"limit":2000}));
    let flagged = all["result"]["activities"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|a| a["critical"] == true)
        .count();
    assert_eq!(flagged, rows.len());
    // Counts only.
    let counts = run(json!({"command":"load","input":xer,"limit":0,"relationship_limit":0}));
    assert!(counts["result"]["activities"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(counts["result"]["relationships"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(counts["result"]["total_activities"].as_u64().unwrap() > 100);
    // Refusals.
    let bad = run(json!({"command":"load","input":xer,"fields":["colour"]}));
    assert_eq!(bad["error"]["code"], "invalid_arguments");
    assert!(bad["error"]["message"]
        .as_str()
        .unwrap()
        .contains("task_code"));
    let nowbs = run(json!({"command":"load","input":xer,"wbs":"NOPE"}));
    assert_eq!(nowbs["error"]["code"], "invalid_arguments");
}

#[test]
fn load_sorts_by_a_field_either_way() {
    let dir = tempfile::tempdir().unwrap();
    let xer = sample(dir.path());
    let latest = run(
        json!({"command":"load","input":xer,"sort":"-early_finish","limit":3,"fields":["early_finish"]}),
    );
    let rows = latest["result"]["activities"].as_array().unwrap();
    // PM0910 and PM0920 finish together, in project order.
    assert_eq!(rows[0]["task_code"], "PM0910", "{rows:?}");
    assert_eq!(rows[1]["task_code"], "PM0920");
    assert!(rows[1]["early_finish"].as_str() >= rows[2]["early_finish"].as_str());
    let all = run(
        json!({"command":"load","input":xer,"sort":"total_float_minutes","limit":2000,"fields":["total_float_minutes"]}),
    );
    let floats: Vec<Option<i64>> = all["result"]["activities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["total_float_minutes"].as_i64())
        .collect();
    let numbers: Vec<i64> = floats.iter().flatten().copied().collect();
    assert!(numbers.windows(2).all(|w| w[0] <= w[1]), "{numbers:?}");
    // Empty values (completed activities) go last.
    let first_empty = floats
        .iter()
        .position(Option::is_none)
        .unwrap_or(floats.len());
    assert!(floats[first_empty..].iter().all(Option::is_none));
    let bad = run(json!({"command":"load","input":xer,"sort":"documents"}));
    assert_eq!(bad["error"]["code"], "invalid_arguments");
    let input = xer.display().to_string();
    let Invocation::Run(command) = cli::parse([
        "load",
        "--input",
        input.as_str(),
        "--sort",
        "-early_finish",
        "--limit",
        "1",
    ])
    .unwrap() else {
        panic!("load runs")
    };
    assert_eq!(
        response(dispatch(*command))["result"]["activities"][0]["task_code"],
        "PM0910"
    );
}

#[test]
fn load_without_the_new_options_is_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let xer = sample(dir.path());
    let r = run(json!({"command":"load","input":xer,"limit":3}));
    let result = &r["result"];
    assert!(result.get("matched_activities").is_none());
    assert!(result.get("not_found").is_none());
    let keys: Vec<&String> = result["activities"][0]
        .as_object()
        .unwrap()
        .keys()
        .collect();
    assert!(!keys.iter().any(|k| *k == "wbs" || *k == "critical"));
    assert!(keys.iter().any(|k| *k == "documents"));
    assert_eq!(keys.len(), 19, "{keys:?}");
}

#[test]
fn load_options_work_the_same_through_the_cli_and_mcp() {
    let dir = tempfile::tempdir().unwrap();
    let xer = sample(dir.path());
    let input = xer.display().to_string();
    let Invocation::Run(command) = cli::parse([
        "load",
        "--input",
        input.as_str(),
        "--fields",
        "name,early_finish",
        "--activities",
        "PM0100,PM0110",
        "--critical",
        "false",
        "--relationship-limit",
        "0",
    ])
    .unwrap() else {
        panic!("load runs")
    };
    let from_cli = response(dispatch(*command));
    let replies = session(
        Server::default(),
        &[
            json!({"method":"tools/call","params":{"name":"openxer_load","arguments":{
            "input":xer,"fields":["name","early_finish"],"activities":["PM0100","PM0110"],
            "critical":false,"relationship_limit":0}}}),
        ],
    );
    let from_mcp = &replies[1]["result"]["structuredContent"];
    assert_eq!(from_cli["ok"], true, "{from_cli}");
    assert_eq!(&from_cli, from_mcp);
    assert_eq!(
        from_cli["result"]["activities"].as_array().unwrap().len(),
        2
    );
    assert!(from_cli["result"]["relationships"]
        .as_array()
        .unwrap()
        .is_empty());
    // A limit of 0 passes the MCP schema now; above 2000 does not.
    let replies = session(
        Server::default(),
        &[
            json!({"method":"tools/call","params":{"name":"openxer_load","arguments":{"input":xer,"relationship_limit":0}}}),
            json!({"method":"tools/call","params":{"name":"openxer_load","arguments":{"input":xer,"limit":2001}}}),
        ],
    );
    assert_eq!(replies[1]["result"]["isError"], false, "{}", replies[1]);
    assert!(replies[2]["error"]["message"]
        .as_str()
        .unwrap()
        .contains("outside the allowed range"));
    let Invocation::Run(command) = cli::parse([
        "load",
        "--input",
        input.as_str(),
        "--wbs",
        "ENG",
        "--critical",
    ])
    .unwrap() else {
        panic!("load runs")
    };
    let r = response(dispatch(*command));
    assert_eq!(r["ok"], true, "{r}");
    assert!(r["result"]["matched_activities"].is_number());
}

// ------------------------------------------------------ in-memory apply

#[test]
fn applying_changes_in_memory_matches_the_file_apply() {
    let dir = tempfile::tempdir().unwrap();
    let xer = sample(dir.path());
    let out = dir.path().join("applied.xer");
    let changes = json!({"format":"openxer-changes/1","operations":[
        {"op":"set_activity","activity":"PM0910","field":"remaining_duration","value":40},
        {"op":"add_activity","activity":"PM0915","name":"Review closeout with owner","wbs":"PM"},
        {"op":"relationship","action":"add","predecessor":"PM0910","successor":"PM0915","kind":"FS"},
        {"op":"schedule"}
    ]});
    let file = run(json!({"command":"apply","input":xer,"output":out,"changes":changes}));
    assert_eq!(file["ok"], true, "{file}");
    let doc = Document::read(&xer).unwrap();
    let pid = doc.projects()[0].0;
    let mut project = ProjectData::from_document(doc, pid, "sample.xer".into()).unwrap();
    let report = edit::apply_changes(&mut project, changes, None, &Default::default()).unwrap();
    assert_eq!(report, file["result"]["report"]);
    let written = Document::read(&out).unwrap();
    let memory = project.updated_document();
    for table in ["TASK", "TASKPRED", "TASKRSRC", "PROJECT"] {
        let a = written.table(table).unwrap();
        let b = memory.table(table).unwrap();
        assert_eq!(a.fields, b.fields, "{table}");
        assert_eq!(a.rows.len(), b.rows.len(), "{table}");
        for (ra, rb) in a.rows.iter().zip(&b.rows) {
            for (n, field) in a.fields.iter().enumerate() {
                // A new row's GUID and dates are minted per run (to the
                // minute, so two runs can differ).
                if matches!(field.as_str(), "guid" | "create_date" | "update_date") {
                    continue;
                }
                assert_eq!(ra[n], rb[n], "{table}.{field}");
            }
        }
    }
    // A failing batch leaves the project as it was.
    let before = project.updated_document().text();
    let bad = json!({"format":"openxer-changes/1","operations":[
        {"op":"set_activity","activity":"PM0110","field":"name","value":"Changed"},
        {"op":"delete_activity","activity":"NOPE"}
    ]});
    assert!(edit::apply_changes(&mut project, bad, None, &Default::default()).is_err());
    assert_eq!(project.updated_document().text(), before);
    assert_eq!(
        edit::apply_changes(
            &mut project,
            json!({"format":"x"}),
            None,
            &Default::default()
        )
        .unwrap_err()
        .code,
        "invalid_changes"
    );
}

// ------------------------------------------------- review fixes (tests)

#[test]
fn an_approved_proposal_applies_exactly_as_previewed() {
    let dir = tempfile::tempdir().unwrap();
    let xer = sample(dir.path());
    let edit_ctx = context(&xer, ChatMode::Edit);
    let mut diffs = vec![];
    for recalculate in [true, false] {
        let changes = json!({"format":"openxer-changes/1","recalculate":recalculate,"operations":[
            {"op":"set_activity","activity":"PM0910","field":"remaining_duration","value":40},
            {"op":"set_activity","activity":"PM0910","field":"original_duration","value":60}
        ]});
        let preview = chat_tools::call(&edit_ctx, "openxer_apply", json!({"changes":changes}));
        assert!(!preview.is_error, "{}", preview.text);
        let doc = Document::read(&xer).unwrap();
        let pid = doc.projects()[0].0;
        let mut project = ProjectData::from_document(doc, pid, "sample.xer".into()).unwrap();
        let applied = chat_tools::apply_proposal(&mut project, changes).unwrap();
        assert_eq!(
            applied["diff"], preview.envelope["result"]["report"]["diff"],
            "recalculate {recalculate}"
        );
        diffs.push(applied["diff"].clone());
    }
    // The two runs differ, so the document's own recalculate is what counts.
    assert_ne!(diffs[0], diffs[1]);
    // A top-level recalculate the preview could use is not taken at all.
    let refused = chat_tools::call(
        &edit_ctx,
        "openxer_apply",
        json!({"changes":{"format":"openxer-changes/1","operations":[{"op":"schedule"}]},"recalculate":false}),
    );
    assert!(refused.is_error);
    assert!(refused.text.contains("recalculate"), "{}", refused.text);
}

#[test]
fn counts_only_pages_have_no_next_offset() {
    let dir = tempfile::tempdir().unwrap();
    let xer = sample(dir.path());
    let r = run(json!({"command":"load","input":xer,"limit":0,"relationship_limit":0}));
    assert_eq!(
        r["result"]["next_offset"],
        Value::Null,
        "{}",
        r["result"]["next_offset"]
    );
    assert_eq!(r["result"]["next_relationship_offset"], Value::Null);
    let paged = run(json!({"command":"load","input":xer,"limit":1,"relationship_limit":1}));
    assert_eq!(paged["result"]["next_offset"], 1);
    assert_eq!(paged["result"]["next_relationship_offset"], 1);
}

#[test]
fn wbs_paths_sort_element_by_element() {
    use openxer_automation::load::compare_wbs_paths as c;
    use std::cmp::Ordering::*;
    assert_eq!(c("1.2", "1.10"), Less);
    assert_eq!(c("1.10", "1.9"), Greater);
    assert_eq!(c("2", "10"), Less);
    assert_eq!(c("1", "1.1"), Less);
    assert_eq!(c("A.2", "A.10"), Less);
    assert_eq!(c("CONST.A100", "CONST.A20"), Less, "text where not numbers");
    assert_eq!(c("ENG", "ENG"), Equal);
    let dir = tempfile::tempdir().unwrap();
    let xer = sample(dir.path());
    let r = run(json!({"command":"load","input":xer,"sort":"wbs","fields":["wbs"],"limit":2000}));
    let paths: Vec<String> = r["result"]["activities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["wbs"].as_str().unwrap().to_string())
        .collect();
    assert!(paths.windows(2).all(|w| c(&w[0], &w[1]) != Greater));
}
