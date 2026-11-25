//! `export-scurve` and `export-earned-hours` from the command line and MCP,
//! on the synthetic crews sample (`openxer_xer::workspace::sample::crews`):
//! the defaults, the totals, the same answer from both, and the refusals.
use openxer_automation::{
    cli::{self, Invocation},
    dispatch,
    mcp::Server,
    response,
};
use openxer_xer::ProjectData;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// The crews sample after its first update, scheduled, as an XER.
fn source(dir: &Path, labor: bool) -> PathBuf {
    let mut doc = openxer_xer::workspace::sample::crews(24., true);
    if !labor {
        doc.table_mut("TASKRSRC").rows.clear();
    }
    let mut p = ProjectData::from_document(doc, 1, "Crews".into()).unwrap();
    openxer_engine::scheduler::schedule_with_resources(&mut p.model, &p.resources).unwrap();
    let path = dir.join(if labor { "crews.xer" } else { "plain.xer" });
    p.updated_document().write(&path).unwrap();
    path
}

fn cli(args: &[&str]) -> Value {
    let Invocation::Run(command) = cli::parse(args.iter().copied()).unwrap() else {
        panic!("not a command")
    };
    response(dispatch(*command))
}

fn mcp(name: &str, arguments: Value) -> Value {
    let meta = json!({"io.modelcontextprotocol/protocolVersion":"2026-07-28",
        "io.modelcontextprotocol/clientCapabilities":{}});
    let reply = Server::default()
        .handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
            "params":{"_meta":meta,"name":name,"arguments":arguments}}))
        .unwrap();
    reply["result"]["structuredContent"].clone()
}

#[test]
fn export_scurve_takes_the_detected_defaults_and_matches_mcp() {
    let dir = tempfile::tempdir().unwrap();
    let input = source(dir.path(), true);
    let out = dir.path().join("curve.xlsx");
    let r = cli(&[
        "export-scurve",
        "--input",
        input.to_str().unwrap(),
        "--output",
        out.to_str().unwrap(),
    ]);
    assert_eq!(r["ok"], true, "{r}");
    let result = &r["result"];
    assert_eq!(result["measure"], "labor");
    assert_eq!(result["period"], "week");
    assert_eq!(result["breakdown"], "resource");
    assert_eq!(result["week_starts"], "sunday");
    assert_eq!(
        result["detected"],
        "Labor hours found on 3 of 14 activities"
    );
    assert_eq!(result["totals"]["budgeted"], 104.);
    assert_eq!(result["totals"]["actual"], 24.);
    assert_eq!(result["totals"]["remaining"], 80.);
    assert_eq!(result["earned_to_data_date"], 24.);
    assert_eq!(result["rows"], 2);
    assert!(out.is_file());
    // An existing output is refused.
    let again = cli(&[
        "export-scurve",
        "--input",
        input.to_str().unwrap(),
        "--output",
        out.to_str().unwrap(),
    ]);
    assert_eq!(again["error"]["code"], "output_exists", "{again}");
    // MCP gives the same figures from the same options.
    let mcp_out = dir.path().join("mcp.xlsx");
    let m = mcp(
        "openxer_export_scurve",
        json!({"input":input,"output":mcp_out}),
    );
    assert_eq!(m["ok"], true, "{m}");
    for key in [
        "measure",
        "period",
        "breakdown",
        "totals",
        "earned_to_data_date",
    ] {
        assert_eq!(m["result"][key], result[key], "{key}");
    }
    // Other choices, and refusals of what the project cannot give.
    let monthly = dir.path().join("monthly.xlsx");
    let r = cli(&[
        "export-scurve",
        "--input",
        input.to_str().unwrap(),
        "--output",
        monthly.to_str().unwrap(),
        "--measure",
        "duration",
        "--period",
        "month",
        "--late",
    ]);
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["result"]["breakdown"], "total");
    assert_eq!(r["result"]["totals"]["budgeted"], 51.);
    let bad = cli(&[
        "export-scurve",
        "--input",
        input.to_str().unwrap(),
        "--output",
        dir.path().join("bad.xlsx").to_str().unwrap(),
        "--period",
        "financial",
    ]);
    assert_eq!(bad["error"]["code"], "invalid_arguments", "{bad}");
    assert!(!dir.path().join("bad.xlsx").exists());
}

#[test]
fn export_earned_hours_totals_and_refuses_a_project_without_labor() {
    let dir = tempfile::tempdir().unwrap();
    let input = source(dir.path(), true);
    let out = dir.path().join("earned.xlsx");
    let r = cli(&[
        "export-earned-hours",
        "--input",
        input.to_str().unwrap(),
        "--output",
        out.to_str().unwrap(),
    ]);
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["result"]["breakdown"], "resource");
    assert_eq!(r["result"]["rows"], 2);
    assert_eq!(r["result"]["assignments"], 3);
    assert_eq!(r["result"]["totals"]["earned"], 24.);
    assert_eq!(r["result"]["totals"]["budgeted"], 104.);
    assert!(r["result"]["budgeted_from"]
        .as_str()
        .unwrap()
        .contains("no baseline"));
    let m = mcp(
        "openxer_export_earned_hours",
        json!({"input":input,"output":dir.path().join("mcp.xlsx"),"breakdown":"activity"}),
    );
    assert_eq!(m["ok"], true, "{m}");
    assert_eq!(m["result"]["rows"], 3);
    assert_eq!(m["result"]["totals"], r["result"]["totals"]);
    let plain = source(dir.path(), false);
    let none = dir.path().join("none.xlsx");
    let refused = cli(&[
        "export-earned-hours",
        "--input",
        plain.to_str().unwrap(),
        "--output",
        none.to_str().unwrap(),
    ]);
    assert_eq!(refused["error"]["code"], "no_labor_units", "{refused}");
    assert!(!none.exists(), "no empty file");
}
