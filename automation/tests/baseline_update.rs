//! `baseline update` through `dispatch`, the command line and MCP: a dry
//! run returns the preview Update would apply, the update stores it with a
//! new Last Update Date, and the options decide what is carried.
use openxer_automation::{
    cli::{self, Invocation},
    dispatch,
    mcp::Server,
    parse_command, response,
};
use openxer_xer::starter_portfolio;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn run(request: Value) -> Value {
    response(parse_command(request).and_then(dispatch))
}

#[track_caller]
fn ok(v: &Value) -> &Value {
    assert_eq!(v["ok"], true, "{v:#}");
    &v["result"]
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("openxer-blu-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A workspace with RIVERBEND and one baseline of it, then the project
/// changed: one activity renamed, another one's duration changed. Returns
/// the workspace, the project's id and the two Activity IDs.
fn changed(dir: &Path) -> (PathBuf, i64, String, String) {
    let xer = dir.join("portfolio.xer");
    starter_portfolio().write(&xer).unwrap();
    let db = dir.join("ws.db");
    ok(&run(json!({"command":"import","input":xer,"output":db})));
    let first = ok(&run(json!({"command":"inspect","input":db})))["projects"][0]["workspace_id"]
        .as_i64()
        .unwrap();
    let with_bl = dir.join("bl.db");
    ok(&run(
        json!({"command":"baseline","input":db,"output":with_bl,"workspace_id":first,"operation":{"op":"add"}}),
    ));
    let (renamed, longer) = ("RB-1010".to_string(), "RB-1020".to_string());
    let edited = dir.join("edited.db");
    ok(&run(
        json!({"command":"apply","input":with_bl,"output":edited,"workspace_id":first,
        "changes":{"format":"openxer-changes/1","operations":[
            {"op":"set_activity","activity":renamed,"field":"activity_name","value":"Renamed by the test"},
            {"op":"set_activity","activity":longer,"field":"original_duration","value":120}
        ]}}),
    ));
    (edited, first, renamed, longer)
}

fn kinds(preview: &Value) -> Vec<(String, String)> {
    preview["activities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            (
                c["object"].as_str().unwrap().to_string(),
                c["kind"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

#[test]
fn dry_run_previews_and_update_applies_the_same_changes() {
    let dir = temp("flow");
    let (ws, w, renamed, longer) = changed(&dir);
    let preview = |options: Value| {
        let r = run(
            json!({"command":"baseline_update","input":ws,"dry_run":true,
            "workspace_id":w,"baseline":"1","options":options}),
        );
        ok(&r)["result"]["preview"].clone()
    };
    // Defaults: the rename is listed, the duration change is not.
    let p = preview(json!({}));
    assert_eq!(
        kinds(&p),
        [(renamed.clone(), "Updated".to_string())],
        "{p:#}"
    );
    assert_eq!(p["activities"][0]["fields"][0]["field"], "Activity Name");
    assert_eq!(
        p["activities"][0]["fields"][0]["project"],
        "Renamed by the test"
    );
    // With dates the duration change is listed too.
    let p = preview(json!({"dates": true}));
    assert!(
        kinds(&p).contains(&(longer.clone(), "Updated".to_string())),
        "{p:#}"
    );
    // Unknown options are refused.
    let r = run(
        json!({"command":"baseline_update","input":ws,"dry_run":true,
        "workspace_id":w,"baseline":"1","options":{"no_such_option":true}}),
    );
    assert_eq!(r["ok"], false);
    // A list of activities limits the update.
    let r = run(
        json!({"command":"baseline_update","input":ws,"dry_run":true,
        "workspace_id":w,"baseline":"1","options":{"dates":true},"activities":[longer]}),
    );
    assert_eq!(
        kinds(&ok(&r)["result"]["preview"]),
        [(longer.clone(), "Updated".to_string())]
    );
    // The update: the same list, the Last Update Date set, a log written.
    let out = dir.join("updated.db");
    let log = dir.join("update.log");
    let r = run(json!({"command":"baseline_update","input":ws,"output":out,
        "workspace_id":w,"baseline":"1","options":{"log_path":log}}));
    let result = ok(&r)["result"].clone();
    assert_eq!(
        kinds(&result["preview"]),
        [(renamed.clone(), "Updated".to_string())]
    );
    let text = std::fs::read_to_string(&log).unwrap();
    assert!(text.contains(&renamed), "{text}");
    let band = ok(&run(
        json!({"command":"baseline_list","input":out,"workspace_id":w}),
    ))["projects"][0]
        .clone();
    assert_eq!(
        band["baselines"][0]["last_update_date"],
        result["last_update_date"]
    );
    // Nothing is left to change with the same options.
    let r = run(
        json!({"command":"baseline_update","input":out,"dry_run":true,
        "workspace_id":w,"baseline":"1"}),
    );
    assert!(kinds(&ok(&r)["result"]["preview"]).is_empty());
}

#[test]
fn the_command_line_and_mcp_reach_the_update() {
    let dir = temp("cli");
    let (ws, w, renamed, _) = changed(&dir);
    let w_text = w.to_string();
    let Invocation::Run(command) = cli::parse([
        "baseline",
        "update",
        "--input",
        ws.to_str().unwrap(),
        "--workspace-id",
        &w_text,
        "--baseline",
        "1",
        "--dry-run",
        "true",
        "--options",
        r#"{"general":true}"#,
        "--activities",
        renamed.as_str(),
    ])
    .unwrap() else {
        panic!("baseline update parses");
    };
    let r = response(dispatch(*command));
    assert_eq!(kinds(&ok(&r)["result"]["preview"]).len(), 1);
    let meta = json!({"io.modelcontextprotocol/protocolVersion":openxer_automation::mcp::PROTOCOL_VERSION,"io.modelcontextprotocol/clientCapabilities":{}});
    let mut server = Server::default();
    let listed = server
        .handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{"_meta":meta}}))
        .unwrap();
    let names: Vec<String> = listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect();
    assert!(names.contains(&"openxer_baseline_update".to_string()));
    let reply = server
        .handle(json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"_meta":meta,"name":"openxer_baseline_update",
            "arguments":{"input":ws,"workspace_id":w,"baseline":"1","dry_run":true}}}))
        .unwrap();
    let envelope = &reply["result"]["structuredContent"];
    assert_eq!(envelope["ok"], true, "{reply:#}");
    assert_eq!(
        kinds(&envelope["result"]["result"]["preview"]),
        [(renamed, "Updated".to_string())]
    );
}
