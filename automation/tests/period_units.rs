//! Typed period units for agents: `set_assignment` field
//! `period_remaining_units` sets one period's remaining units as the usage
//! spreadsheet does, from the command line and MCP, on the synthetic
//! starter project (RB-1010: Monday 2026-11-02 to Wednesday, 8 hours a day,
//! one Pipe crew assignment of 1 unit an hour).
use openxer_automation::{
    cli::{self, Invocation},
    dispatch,
    mcp::Server,
    parse_command, response,
};
use openxer_xer::{starter, Document};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn scheduled(dir: &Path) -> PathBuf {
    let mut p = starter();
    p.document.table_mut("RSRC").add(&[
        ("rsrc_id", "77".into()),
        ("rsrc_name", "Pipe crew".into()),
        ("rsrc_short_name", "PIPE".into()),
        ("rsrc_type", "RT_Labor".into()),
        ("def_qty_per_hr", "1".into()),
        ("clndr_id", "1".into()),
    ]);
    p.add_assignment(1010, Some(77), None).unwrap();
    let source = dir.join("source.xer");
    p.updated_document().write(&source).unwrap();
    let out = dir.join("scheduled.xer");
    let done = cli(&[
        "schedule",
        "--input",
        source.to_str().unwrap(),
        "--output",
        out.to_str().unwrap(),
    ]);
    assert_eq!(done["ok"], true, "{done}");
    out
}

fn cli(args: &[&str]) -> Value {
    let Invocation::Run(command) = cli::parse(args.iter().copied()).unwrap() else {
        panic!("not a command")
    };
    response(dispatch(*command))
}

fn set(input: &Path, output: &Path, field: &str, value: &str) -> Value {
    cli(&[
        "set-assignment",
        "--input",
        input.to_str().unwrap(),
        "--activity",
        "RB-1010",
        "--resource",
        "PIPE",
        "--field",
        field,
        "--value",
        value,
        "--output",
        output.to_str().unwrap(),
    ])
}

/// The assignment row's `column` in the file at `path`.
fn cell(path: &Path, column: &str) -> String {
    let doc = Document::read(path).unwrap();
    let t = doc.table("TASKRSRC").unwrap();
    let r = t
        .rows
        .iter()
        .find(|r| t.value(r, "rsrc_id") == "77")
        .unwrap();
    t.value(r, column).to_string()
}

#[test]
fn set_assignment_types_one_period_and_curve_null_removes_it() {
    let dir = tempfile::tempdir().unwrap();
    let input = scheduled(dir.path());
    assert_eq!(cell(&input, "remain_crv"), "");

    // Tuesday from the command line.
    let typed = dir.path().join("typed.xer");
    let done = set(&input, &typed, "period_remaining_units", "2026-11-03=20");
    assert_eq!(done["ok"], true, "{done}");
    assert_eq!(cell(&typed, "remain_crv"), "8:8;20:8;8:8");
    assert_eq!(cell(&typed, "target_crv"), "8:8;20:8;8:8");
    assert_eq!(cell(&typed, "remain_qty"), "36");

    // Wednesday through MCP.
    let mcp_out = dir.path().join("mcp.xer");
    let mut server = Server::default();
    let meta = json!({"io.modelcontextprotocol/protocolVersion":"2026-07-28",
        "io.modelcontextprotocol/clientCapabilities":{}});
    let reply = server
        .handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
            "params":{"_meta":meta,"name":"openxer_set_assignment","arguments":{
                "input":typed,"output":mcp_out,"activity":"RB-1010","resource":"PIPE",
                "field":"period_remaining_units","value":"2026-11-04=12"}}}))
        .unwrap();
    assert_eq!(reply["result"]["structuredContent"]["ok"], true, "{reply}");
    assert_eq!(cell(&mcp_out, "remain_crv"), "8:8;20:8;12:8");

    // A day range spreads evenly over its working time.
    let week = dir.path().join("week.xer");
    let done = set(
        &input,
        &week,
        "period_remaining_units",
        "2026-11-02,2026-11-08=30",
    );
    assert_eq!(done["ok"], true, "{done}");
    assert_eq!(cell(&week, "remain_crv"), "30:24");

    // Before the data date, a bad form: refused, nothing written.
    let refused = dir.path().join("refused.xer");
    for value in ["2026-10-30=5", "20", "2026-11-03=x"] {
        let done = set(&input, &refused, "period_remaining_units", value);
        assert_eq!(done["ok"], false, "{value}: {done}");
        assert!(!refused.exists());
    }

    // curve null takes the typed values off.
    let cleared = dir.path().join("cleared.xer");
    let done = response(
        parse_command(json!({"command":"apply","input":typed,"output":cleared,
            "changes":{"format":"openxer-changes/1","operations":[
                {"op":"set_assignment","activity":"RB-1010","resource":"PIPE",
                 "field":"curve","value":null}]}}))
        .and_then(dispatch),
    );
    assert_eq!(done["ok"], true, "{done}");
    assert_eq!(cell(&cleared, "remain_crv"), "");
    assert_eq!(cell(&cleared, "target_crv"), "");
    assert_eq!(cell(&cleared, "remain_qty"), "36");
}
