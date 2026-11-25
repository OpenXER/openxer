//! `delete_financial_period` through `apply`, the command line and MCP: a
//! period holding stored period performance is refused with nothing
//! written; an empty one is deleted.
use openxer_automation::{
    cli::{self, Invocation},
    dispatch,
    mcp::Server,
    parse_command, response,
};
use openxer_xer::{starter, Document};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// The starter with a weekly calendar of two periods; the first holds
/// stored performance of one activity.
fn source(dir: &Path) -> PathBuf {
    let mut p = starter();
    let d = &mut p.document;
    d.table_mut("FINTMPL").add(&[
        ("fintmpl_id", "1".into()),
        ("fintmpl_name", "Weeks".into()),
        ("default_flag", "Y".into()),
    ]);
    for (id, name, start, end) in [
        (10, "2026-11-22", "2026-11-22 00:00", "2026-11-28 23:59"),
        (11, "2026-11-29", "2026-11-29 00:00", "2026-12-05 23:59"),
    ] {
        d.table_mut("FINDATES").add(&[
            ("fin_dates_id", id.to_string()),
            ("fintmpl_id", "1".into()),
            ("fin_dates_name", name.into()),
            ("start_date", start.into()),
            ("end_date", end.into()),
        ]);
    }
    let pid = p.model.project.proj_id;
    let task = p.model.activities[0].task_id;
    p.document.table_mut("TASKFIN").add(&[
        ("task_id", task.to_string()),
        ("fin_dates_id", "10".into()),
        ("proj_id", pid.to_string()),
        ("act_work_qty", "40".into()),
    ]);
    let path = dir.join("source.xer");
    p.updated_document().write(&path).unwrap();
    path
}

fn run(request: Value) -> Value {
    response(parse_command(request).and_then(dispatch))
}

fn changes(operations: Value) -> Value {
    json!({"format":"openxer-changes/1","operations":operations})
}

fn period_names(path: &Path) -> Vec<String> {
    let doc = Document::read(path).unwrap();
    doc.table("FINDATES").map_or(vec![], |t| {
        t.rows
            .iter()
            .map(|r| t.value(r, "fin_dates_name").to_string())
            .collect()
    })
}

#[test]
fn a_period_with_stored_performance_is_refused_and_an_empty_one_deleted() {
    let dir = tempfile::tempdir().unwrap();
    let input = source(dir.path());
    assert_eq!(period_names(&input), ["2026-11-22", "2026-11-29"]);
    // Stored values: refused, no file written.
    let bad = dir.path().join("bad.xer");
    let r = run(json!({"command":"apply","input":input,"output":bad,
        "changes":changes(json!([{"op":"delete_financial_period","period":"2026-11-22"}]))}));
    assert_eq!(r["ok"], false, "{r}");
    assert!(
        r["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Only a period without stored values"),
        "{r}"
    );
    assert!(!bad.exists());
    // Unknown names are refused too.
    let r = run(json!({"command":"apply","input":input,"output":bad,
        "changes":changes(json!([{"op":"delete_financial_period","period":"Nope"}]))}));
    assert_eq!(r["ok"], false, "{r}");

    // An empty period goes; the command line and MCP give the same file.
    let batch = changes(json!([
        {"op":"delete_financial_period","period":"2026-11-29","calendar":"Weeks"}
    ]));
    let file = dir.path().join("changes.json");
    std::fs::write(&file, batch.to_string()).unwrap();
    let cli_out = dir.path().join("cli.xer");
    let Invocation::Run(command) = cli::parse([
        "apply",
        "--input",
        input.to_str().unwrap(),
        "--changes",
        file.to_str().unwrap(),
        "--output",
        cli_out.to_str().unwrap(),
    ])
    .unwrap() else {
        panic!()
    };
    let via_cli = response(dispatch(*command));
    assert_eq!(via_cli["ok"], true, "{via_cli}");
    assert_eq!(period_names(&cli_out), ["2026-11-22"]);

    let mcp_out = dir.path().join("mcp.xer");
    let mut server = Server::default();
    let meta = json!({"io.modelcontextprotocol/protocolVersion":"2026-07-28",
        "io.modelcontextprotocol/clientCapabilities":{}});
    let reply = server
        .handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
            "params":{"_meta":meta,"name":"openxer_apply","arguments":{"input":input,"output":mcp_out,"changes":batch}}}))
        .unwrap();
    assert_eq!(reply["result"]["structuredContent"]["ok"], true, "{reply}");
    assert_eq!(period_names(&mcp_out), period_names(&cli_out));
    // And the refusal reaches MCP the same way.
    let refused = server
        .handle(json!({"jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{"_meta":meta,"name":"openxer_apply","arguments":{"input":input,"output":bad,
            "changes":changes(json!([{"op":"delete_financial_period","period":"2026-11-22"}]))}}}))
        .unwrap();
    assert_ne!(
        refused["result"]["structuredContent"]["ok"], true,
        "{refused}"
    );
    assert!(!bad.exists());
    assert!(cli::HELP.contains("delete_financial_period"));
}
