//! `global_change` (Tools > Global Change) through the single edit command,
//! the CLI flags and the MCP tool, on the built-in starter project: a rule
//! given inline, one saved in a workspace by name, the dry run's report
//! lines and the written values. The rule model and its evaluation are
//! tested in `xer/src/edit/global_change/tests.rs`; that the lines match
//! the app's report is the desktop workflow `global_change_cli`.
use openxer_automation::{
    cli::{self, Invocation},
    dispatch,
    mcp::Server,
    parse_command, response,
};
use openxer_xer::{starter, Document, ProjectData};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn run(request: Value) -> Value {
    response(parse_command(request).and_then(dispatch))
}

fn source(dir: &Path) -> PathBuf {
    let input = dir.join("source.xer");
    starter().updated_document().write(&input).unwrap();
    input
}

/// Double the original duration of the construction activities (WBS RB.3)
/// and of any activity named "...design...".
fn rule() -> Value {
    json!({
        "match": "any",
        "conditions": [
            {"column": "WBS", "op": "is under", "value": "RB.3"},
            {"column": "Activity Name", "op": "contains", "value": "design"}
        ],
        "then": [{"column": "Original Duration", "value": "Original Duration", "operator": "x", "value2": "2"}]
    })
}

fn durations(path: &Path) -> Vec<(String, i64)> {
    let p = ProjectData::from_document(Document::read(path).unwrap(), 1, "out".into()).unwrap();
    p.model
        .activities
        .iter()
        .map(|a| {
            (
                a.task_code.clone(),
                p.details[&a.task_id].original_duration / 60,
            )
        })
        .collect()
}

#[test]
fn a_dry_run_lists_the_report_lines_and_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let input = source(dir.path());
    let r = run(json!({"command":"edit","input":input,"dry_run":true,
        "operation":{"op":"global_change","rule":rule()}}));
    assert_eq!(r["ok"], true, "{r}");
    let result = &r["result"]["report"]["results"][0];
    let rows: Vec<(String, String, String, String)> = result["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            let s = |k: &str| row[k].as_str().unwrap_or_default().to_string();
            (s("activity"), s("field"), s("old"), s("new"))
        })
        .collect();
    // RB-1020 (design, 48h) and the construction activities RB-1060 to
    // RB-1100 (16h, 40h, 32h, 48h, 40h).
    let line = |a: &str, old: i64| {
        (
            a.to_string(),
            "Original Duration".to_string(),
            format!("{old}h"),
            format!("{}h", 2 * old),
        )
    };
    assert_eq!(
        rows,
        [
            line("RB-1020", 48),
            line("RB-1060", 16),
            line("RB-1070", 40),
            line("RB-1080", 32),
            line("RB-1090", 48),
            line("RB-1100", 40)
        ]
    );
    assert_eq!(result["changed"], 6);
    assert_eq!(result["refused"], 0);
    assert_eq!(r["result"]["output"], Value::Null, "nothing written");
}

#[test]
fn the_cli_and_mcp_write_the_lines_and_count_them() {
    let dir = tempfile::tempdir().unwrap();
    let input = source(dir.path());
    let before = durations(&input);
    let out = dir.path().join("cli.xer");
    let Invocation::Run(command) = cli::parse([
        "global-change",
        "--input",
        input.to_str().unwrap(),
        "--rule",
        &rule().to_string(),
        "--output",
        out.to_str().unwrap(),
    ])
    .unwrap() else {
        panic!("global-change runs")
    };
    let r = response(dispatch(*command));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["result"]["report"]["results"][0]["changed"], 6);
    let after = durations(&out);
    let doubled = [
        "RB-1020", "RB-1060", "RB-1070", "RB-1080", "RB-1090", "RB-1100",
    ];
    for ((code, old), (_, new)) in before.iter().zip(&after) {
        let want = if doubled.contains(&code.as_str()) {
            2 * old
        } else {
            *old
        };
        assert_eq!(*new, want, "{code}");
    }
    // The MCP tool writes the same file.
    let mcp_out = dir.path().join("mcp.xer");
    let mut server = Server::default();
    let meta = json!({"io.modelcontextprotocol/protocolVersion":"2026-07-28",
        "io.modelcontextprotocol/clientCapabilities":{}});
    let arguments = json!({"input":input,"output":mcp_out,"rule":rule()});
    let reply = server
        .handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
            "params":{"_meta":meta,"name":"openxer_global_change","arguments":arguments}}))
        .unwrap();
    let envelope = &reply["result"]["structuredContent"];
    assert_eq!(envelope["ok"], true, "{envelope}");
    assert_eq!(durations(&mcp_out), after);
    // Refusals: neither a name nor a rule, an unknown comparison, a bad flag.
    let r = run(json!({"command":"edit","input":input,"dry_run":true,
        "operation":{"op":"global_change"}}));
    assert_eq!(r["ok"], false);
    assert!(r.to_string().contains("needs name or rule"), "{r}");
    let r = run(json!({"command":"edit","input":input,"dry_run":true,
        "operation":{"op":"global_change",
            "rule":{"conditions":[{"column":"wbs","op":"is not under","value":"RB.3"}]}}}));
    assert_eq!(r["ok"], false, "is not under is not offered: {r}");
    assert!(cli::parse([
        "global-change",
        "--input",
        "x.xer",
        "--rule",
        "[1]",
        "--dry-run"
    ])
    .is_err());
}

#[test]
fn a_saved_global_change_runs_by_name_from_a_workspace() {
    let dir = tempfile::tempdir().unwrap();
    let input = source(dir.path());
    let workspace = dir.path().join("workspace.db");
    let r = run(json!({"command":"import","input":input,"output":workspace}));
    assert_eq!(r["ok"], true, "{r}");
    // As the app saves it: a list of definitions, each If row with its join.
    let saved = json!([{
        "name": "Double construction",
        "conditions": [
            {"and": false, "column": "wbs", "op": "under", "value": "RB.3", "high": ""},
            {"and": false, "column": "activity_name", "op": "contains", "value": "design", "high": ""}
        ],
        "then": [{"column": "original_duration", "value": "Original Duration", "operator": "x", "value2": "2"}],
        "otherwise": []
    }]);
    let c = rusqlite::Connection::open(&workspace).unwrap();
    c.execute(
        "CREATE TABLE IF NOT EXISTS preferences(key TEXT PRIMARY KEY,value TEXT NOT NULL)",
        [],
    )
    .unwrap();
    c.execute(
        "INSERT INTO preferences(key,value) VALUES(?1,?2)",
        ["global_changes_v1", saved.to_string().as_str()],
    )
    .unwrap();
    drop(c);
    let r = run(json!({"command":"edit","input":workspace,"dry_run":true,
        "operation":{"op":"global_change","name":"double construction"}}));
    assert_eq!(r["ok"], true, "{r}");
    let result = &r["result"]["report"]["results"][0];
    assert_eq!(result["name"], "Double construction");
    assert_eq!(result["changed"], 6);
    let r = run(json!({"command":"edit","input":workspace,"dry_run":true,
        "operation":{"op":"global_change","name":"Missing"}}));
    assert_eq!(r["ok"], false);
    assert!(r.to_string().contains("Double construction"), "{r}");
    // An XER input has no saved global changes.
    let r = run(json!({"command":"edit","input":input,"dry_run":true,
        "operation":{"op":"global_change","name":"Double construction"}}));
    assert_eq!(r["ok"], false);
}
