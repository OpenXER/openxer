//! `merge_resources`, `delete_resource` and `set_wbs_earned_value` through
//! `apply`, the command line and MCP, on the synthetic starter project.
use openxer_automation::{
    cli::{self, Invocation},
    dispatch,
    mcp::Server,
    parse_command, response,
};
use openxer_xer::edit::wbs;
use openxer_xer::{starter, Document, ProjectData};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The starter with PIPE, its imported duplicate PIPE-2 (assigned to
/// RB-1020 with PIPE, and to RB-1040), and an unused resource SPARE.
fn duplicates() -> ProjectData {
    let mut p = starter();
    for (id, code) in [(77, "PIPE"), (78, "PIPE-2"), (79, "SPARE")] {
        p.document.table_mut("RSRC").add(&[
            ("rsrc_id", id.to_string()),
            ("rsrc_name", "Pipe crew".into()),
            ("rsrc_short_name", code.into()),
            ("rsrc_type", "RT_Labor".into()),
            ("def_qty_per_hr", "1".into()),
            ("clndr_id", "1".into()),
        ]);
    }
    for (id, date, price) in [(77, "2020-01-01 00:00", "1"), (78, "2026-03-01 00:00", "2")] {
        p.document.table_mut("RSRCRATE").add(&[
            ("rsrc_id", id.to_string()),
            ("start_date", date.into()),
            ("cost_per_qty", price.into()),
        ]);
    }
    p.add_assignment(1020, Some(77), None).unwrap();
    p.add_assignment(1020, Some(78), None).unwrap();
    p.add_assignment(1040, Some(78), None).unwrap();
    p
}

fn source(dir: &Path) -> PathBuf {
    let path = dir.join("source.xer");
    duplicates().updated_document().write(&path).unwrap();
    path
}

fn run(request: Value) -> Value {
    response(parse_command(request).and_then(dispatch))
}

fn changes(operations: Value) -> Value {
    json!({"format":"openxer-changes/1","operations":operations})
}

fn rows(path: &Path, table: &str) -> Vec<BTreeMap<String, String>> {
    let doc = Document::read(path).unwrap();
    doc.table(table).map_or(vec![], |t| {
        t.rows
            .iter()
            .map(|r| t.fields.iter().cloned().zip(r.iter().cloned()).collect())
            .collect()
    })
}

fn with_rsrc(path: &Path, id: &str) -> usize {
    rows(path, "TASKRSRC")
        .iter()
        .filter(|r| r["rsrc_id"] == id)
        .count()
}

#[test]
fn merge_resources_moves_assignments_combines_and_reports() {
    let dir = tempfile::tempdir().unwrap();
    let input = source(dir.path());
    let ops = changes(json!([{"op":"merge_resources","from":["PIPE-2"],"into":"PIPE"}]));
    // Dry run: the report, nothing written.
    let r = run(json!({"command":"apply","input":input,"dry_run":true,"changes":ops}));
    assert_eq!(r["ok"], true, "{r}");
    let report = &r["result"]["report"]["results"][0];
    assert_eq!(report["into"], "PIPE");
    assert_eq!(report["from"], json!(["PIPE-2"]));
    assert_eq!(report["assignments_moved"][0]["count"], 2);
    assert_eq!(report["combined"][0]["activity"], "RB-1020");
    assert_eq!(report["rates"], json!({"moved":1,"dropped":0}));
    assert!(report["summary"][0]
        .as_str()
        .unwrap()
        .starts_with("Merge PIPE-2 into PIPE"));
    // Applied.
    let out = dir.path().join("merged.xer");
    let r = run(json!({"command":"apply","input":input,"output":out,"changes":ops}));
    assert_eq!(r["ok"], true, "{r}");
    assert!(rows(&out, "RSRC")
        .iter()
        .all(|r| r["rsrc_short_name"] != "PIPE-2"));
    assert_eq!(with_rsrc(&out, "78"), 0);
    assert_eq!(
        with_rsrc(&out, "77"),
        2,
        "1020 combined into one, 1040 moved"
    );
    let rates: Vec<_> = rows(&out, "RSRCRATE")
        .into_iter()
        .filter(|r| r["rsrc_id"] == "77")
        .collect();
    assert_eq!(rates.len(), 2, "the later rate row moved: {rates:?}");
    // Refused, nothing written: unknown names, a resource into itself.
    for (k, op) in [
        json!({"op":"merge_resources","from":["NOPE"],"into":"PIPE"}),
        json!({"op":"merge_resources","from":["PIPE"],"into":"PIPE"}),
        json!({"op":"merge_resources","from":[],"into":"PIPE"}),
        json!({"op":"merge_resources","from":["PIPE-2"],"into":"NOPE"}),
    ]
    .into_iter()
    .enumerate()
    {
        let bad = dir.path().join(format!("bad-{k}.xer"));
        let r = run(
            json!({"command":"apply","input":input,"output":bad,"changes":changes(json!([op]))}),
        );
        assert_eq!(r["ok"], false, "{k}: {r}");
        assert!(!bad.exists());
    }
}

#[test]
fn delete_resource_removes_an_unused_resource_only() {
    let dir = tempfile::tempdir().unwrap();
    let input = source(dir.path());
    let out = dir.path().join("deleted.xer");
    let r = run(json!({"command":"apply","input":input,"output":out,
        "changes":changes(json!([{"op":"delete_resource","resource":"SPARE"}]))}));
    assert_eq!(r["ok"], true, "{r}");
    assert!(rows(&out, "RSRC")
        .iter()
        .all(|r| r["rsrc_short_name"] != "SPARE"));
    let bad = dir.path().join("bad.xer");
    let r = run(json!({"command":"apply","input":input,"output":bad,
        "changes":changes(json!([{"op":"delete_resource","resource":"PIPE-2"}]))}));
    assert_eq!(r["ok"], false, "{r}");
    assert!(
        r["error"]["message"]
            .as_str()
            .unwrap()
            .contains("merge_resources")
            || r["error"]["message"].as_str().unwrap().contains("Merge")
    );
    // Merge first, then delete in the same batch.
    let r = run(json!({"command":"apply","input":input,"output":bad,
    "changes":changes(json!([
        {"op":"merge_resources","from":["PIPE-2"],"into":"PIPE"},
        {"op":"delete_resource","resource":"SPARE"}
    ]))}));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(rows(&bad, "RSRC").len(), 1);
}

fn ev_fields(path: &Path, wbs_id: &str) -> [String; 4] {
    let row = rows(path, "PROJWBS")
        .into_iter()
        .find(|r| r["wbs_id"] == wbs_id)
        .unwrap();
    [
        row.get("ev_compute_type").cloned().unwrap_or_default(),
        row.get("ev_user_pct").cloned().unwrap_or_default(),
        row.get("ev_etc_compute_type").cloned().unwrap_or_default(),
        row.get("ev_etc_user_value").cloned().unwrap_or_default(),
    ]
}

#[test]
fn set_wbs_earned_value_writes_the_projwbs_fields_and_refuses_bad_values() {
    let dir = tempfile::tempdir().unwrap();
    let input = source(dir.path());
    let p = duplicates();
    let (path, _) = wbs::paths(&p)
        .into_iter()
        .find(|(_, id)| *id == 30)
        .unwrap();
    let out = dir.path().join("ev.xer");
    let r = run(json!({"command":"apply","input":input,"output":out,
        "changes":changes(json!([{"op":"set_wbs_earned_value","wbs":path,
            "performance":"custom","custom_percent":"35%",
            "estimate_to_complete":"pf","performance_factor":1.25}]))}));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["result"]["report"]["results"][0]["changed"], true);
    assert_eq!(
        ev_fields(&out, "30"),
        ["EC_User_pct", "35", "EE_PF_usr", "1.25"].map(String::from)
    );
    // Captions and codes work too; the row reads back unchanged.
    let again = dir.path().join("ev2.xer");
    let r = run(json!({"command":"apply","input":out,"output":again,
        "changes":changes(json!([{"op":"set_wbs_earned_value","wbs":path,
            "performance":"50/50","estimate_to_complete":"EE_PF_cpi_spi"}]))}));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(
        ev_fields(&again, "30"),
        ["EC_50_50", "35", "EE_PF_cpi_spi", "1.25"].map(String::from)
    );
    for (k, op) in [
        json!({"op":"set_wbs_earned_value","wbs":path,"performance":"half"}),
        json!({"op":"set_wbs_earned_value","wbs":path,"custom_percent":140}),
        json!({"op":"set_wbs_earned_value","wbs":path,"performance_factor":0}),
        json!({"op":"set_wbs_earned_value","wbs":path}),
        json!({"op":"set_wbs_earned_value","wbs":"NO.SUCH","performance":"0/100"}),
        // All or nothing: a good technique with a bad percent writes neither.
        json!({"op":"set_wbs_earned_value","wbs":path,"performance":"0/100","custom_percent":-1}),
    ]
    .into_iter()
    .enumerate()
    {
        let bad = dir.path().join(format!("bad-{k}.xer"));
        let r = run(
            json!({"command":"apply","input":again,"output":bad,"changes":changes(json!([op]))}),
        );
        assert_eq!(r["ok"], false, "{k}: {r}");
        assert!(!bad.exists());
    }
}

#[test]
fn merge_applies_the_same_from_the_command_line_and_mcp() {
    let dir = tempfile::tempdir().unwrap();
    let input = source(dir.path());
    let batch = changes(json!([{"op":"merge_resources","from":["PIPE-2"],"into":"PIPE"}]));
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
    let mcp_out = dir.path().join("mcp.xer");
    let mut server = Server::default();
    let meta = json!({"io.modelcontextprotocol/protocolVersion":"2026-07-28",
        "io.modelcontextprotocol/clientCapabilities":{}});
    let reply = server
        .handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
            "params":{"_meta":meta,"name":"openxer_apply","arguments":{"input":input,"output":mcp_out,"changes":batch}}}))
        .unwrap();
    assert_eq!(reply["result"]["structuredContent"]["ok"], true, "{reply}");
    assert_eq!(rows(&mcp_out, "TASKRSRC"), rows(&cli_out, "TASKRSRC"));
    assert_eq!(rows(&mcp_out, "RSRC"), rows(&cli_out, "RSRC"));
    // The help names the operations.
    let help = cli::HELP;
    for op in ["merge_resources", "delete_resource", "set_wbs_earned_value"] {
        assert!(help.contains(op), "{op} in --help");
    }
}
