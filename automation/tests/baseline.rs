//! `baseline list` and the baseline edits through `dispatch`, the command
//! line and MCP, on a workspace imported from the synthetic two-project
//! starter file: add, copy, set, assign, the refusals for an assigned
//! baseline, delete, restore into the owner's EPS folder, convert (and its
//! refusal for the project itself) and revert.
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

#[track_caller]
fn refused(v: &Value) -> String {
    assert_eq!(v["ok"], false, "{v:#}");
    assert_eq!(v["error"]["code"], "refused", "{v:#}");
    v["error"]["message"].as_str().unwrap().to_string()
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("openxer-bl-cli-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A workspace holding RIVERBEND and NORTHYARD; returns it and their ids.
fn workspace(dir: &Path) -> (PathBuf, i64, i64) {
    let xer = dir.join("portfolio.xer");
    starter_portfolio().write(&xer).unwrap();
    let db = dir.join("ws.db");
    ok(&run(json!({"command":"import","input":xer,"output":db})));
    let projects = ok(&run(json!({"command":"inspect","input":db})))["projects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["workspace_id"].as_i64().unwrap())
        .collect::<Vec<_>>();
    (db, projects[0], projects[1])
}

/// One project's band from `baseline list`.
fn band(db: &Path, w: i64) -> Value {
    let l = ok(&run(
        json!({"command":"baseline_list","input":db,"workspace_id":w}),
    ))
    .clone();
    l["projects"][0].clone()
}

fn ids(band: &Value) -> Vec<String> {
    band["baselines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["baseline_id"].as_str().unwrap().to_string())
        .collect()
}

/// Run one edit from `input` to a new file named `step`; the new file.
fn edit(dir: &Path, input: &Path, step: &str, w: i64, op: Value) -> (PathBuf, Value) {
    let out = dir.join(format!("{step}.db"));
    let r = run(
        json!({"command":"baseline","input":input,"output":out,"workspace_id":w,"operation":op}),
    );
    (out, r)
}

#[test]
fn baselines_are_maintained_assigned_restored_and_reverted() {
    let dir = temp("flow");
    let (db, first, second) = workspace(&dir);
    let empty = band(&db, first);
    assert!(empty["baselines"].as_array().unwrap().is_empty());
    assert_eq!(empty["assigned"]["project_baseline"], "current");
    assert_eq!(empty["assigned"]["primary"], "current");
    assert_eq!(empty["assigned"]["secondary"], "none");
    let pid = empty["project_id"].as_str().unwrap().to_string();
    let name = empty["name"].as_str().unwrap().to_string();

    // Add twice, then copy B1: B1, B2, B3 named after the project.
    let (w1, r) = edit(&dir, &db, "add1", first, json!({"op":"add"}));
    let made = ok(&r)["result"].clone();
    assert_eq!(made["baseline_id"], format!("{pid} - B1"));
    assert_eq!(made["name"], format!("{name} - B1"));
    let (w2, r) = edit(&dir, &w1, "add2", first, json!({"op":"add"}));
    ok(&r);
    let (w3, r) = edit(
        &dir,
        &w2,
        "copy",
        first,
        json!({"op":"copy","baseline":format!("{pid} - B1")}),
    );
    ok(&r);
    let b = band(&w3, first);
    assert_eq!(
        ids(&b),
        [
            format!("{pid} - B1"),
            format!("{pid} - B2"),
            format!("{pid} - B3")
        ]
    );
    let rows = b["baselines"].as_array().unwrap();
    assert_eq!(rows[2]["last_update_date"], rows[0]["last_update_date"]);
    assert_eq!(rows[2]["data_date"], rows[0]["data_date"]);
    let b1 = rows[0]["id"].as_i64().unwrap();
    // The input is never changed.
    assert!(band(&db, first)["baselines"].as_array().unwrap().is_empty());
    // Baselines are not projects.
    let inspected = ok(&run(json!({"command":"inspect","input":w3})))["projects"].clone();
    assert_eq!(inspected.as_array().unwrap().len(), 2);

    // Set: Baseline ID, name; an unknown type is refused.
    let (w4, r) = edit(
        &dir,
        &w3,
        "set",
        first,
        json!({"op":"set","baseline":format!("{pid} - B3"),"baseline_id":"CTR","name":"Contract plan"}),
    );
    ok(&r);
    assert_eq!(ids(&band(&w4, first))[2], "CTR");
    let (_, r) = edit(
        &dir,
        &w4,
        "settype",
        first,
        json!({"op":"set","baseline":"CTR","type":"No Such Type"}),
    );
    refused(&r);

    // Assign: B1 project baseline, B2 tertiary.
    let (w5, r) = edit(
        &dir,
        &w4,
        "assign",
        first,
        json!({"op":"assign","project_baseline":b1.to_string(),"tertiary":format!("{pid} - B2")}),
    );
    ok(&r);
    let b = band(&w5, first);
    assert_eq!(b["assigned"]["project_baseline"]["id"], b1);
    assert_eq!(
        b["assigned"]["tertiary"]["baseline_id"],
        format!("{pid} - B2")
    );
    assert_eq!(
        b["assigned"]["primary"], "current",
        "omitted slots keep their choice"
    );
    assert_eq!(b["baselines"][0]["assigned"], json!(["project"]));
    assert_eq!(b["baselines"][1]["assigned"], json!(["tertiary"]));

    // Assigned (project or tertiary): delete and restore refused.
    for (who, op) in [
        (format!("{pid} - B1"), "delete"),
        (format!("{pid} - B1"), "restore"),
        (format!("{pid} - B2"), "delete"),
        (format!("{pid} - B2"), "restore"),
    ] {
        let (out, r) = edit(
            &dir,
            &w5,
            &format!("refused-{op}"),
            first,
            json!({"op":op,"baseline":who}),
        );
        refused(&r);
        assert!(!out.exists(), "a refused edit writes nothing");
    }

    // Delete a free baseline.
    let (w6, r) = edit(
        &dir,
        &w5,
        "assign2",
        first,
        json!({"op":"assign","tertiary":"none"}),
    );
    ok(&r);
    let (w7, r) = edit(
        &dir,
        &w6,
        "delete",
        first,
        json!({"op":"delete","baseline":format!("{pid} - B2")}),
    );
    ok(&r);
    assert_eq!(
        ids(&band(&w7, first)),
        [format!("{pid} - B1"), "CTR".to_string()]
    );

    // Restore CTR: a separate project in the owner's folder.
    let (w8, r) = edit(
        &dir,
        &w7,
        "restore",
        first,
        json!({"op":"restore","baseline":"CTR"}),
    );
    let restored = ok(&r)["result"].clone();
    assert_eq!(restored["project_id"], "CTR");
    let restored_id = restored["workspace_id"].as_i64().unwrap();
    assert_eq!(ids(&band(&w8, first)), [format!("{pid} - B1")]);
    let eps = ok(&run(json!({"command":"eps_list","input":w8}))).clone();
    let folder_of = |w: i64| {
        eps["projects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["workspace_id"] == w)
            .unwrap()["folder"]
            .clone()
    };
    assert_eq!(folder_of(restored_id), folder_of(first));
    let found = eps["projects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["workspace_id"] == restored_id)
        .unwrap();
    // Its name is the baseline's project name (the root WBS node's name
    // when the project has one, else its ID).
    let restored_name = found["name"].clone();
    assert_eq!(found["project_id"], "CTR");
    // Another baseline with the ID CTR cannot be restored: the Project ID
    // is taken.
    let (x1, r) = edit(&dir, &w8, "add3", first, json!({"op":"add"}));
    ok(&r);
    let last = ids(&band(&x1, first)).last().unwrap().clone();
    let (x2, r) = edit(
        &dir,
        &x1,
        "set-ctr",
        first,
        json!({"op":"set","baseline":last,"baseline_id":"CTR"}),
    );
    ok(&r);
    let (_, r) = edit(
        &dir,
        &x2,
        "restore-taken",
        first,
        json!({"op":"restore","baseline":"CTR"}),
    );
    assert!(refused(&r).contains("CTR"));

    // Convert: the project itself is refused; the restored project becomes
    // a baseline with its own ID and name and a blank last update date.
    let (_, r) = edit(
        &dir,
        &w8,
        "convert-self",
        first,
        json!({"op":"convert","project":first}),
    );
    assert!(refused(&r).contains("open"));
    let (w9, r) = edit(
        &dir,
        &w8,
        "convert",
        first,
        json!({"op":"convert","project":restored_id}),
    );
    ok(&r);
    let b = band(&w9, first);
    let converted = b["baselines"].as_array().unwrap().last().unwrap().clone();
    assert_eq!(converted["baseline_id"], "CTR");
    assert_eq!(converted["name"], restored_name, "keeps its own name");
    assert_eq!(converted["last_update_date"], "");
    let left = ok(&run(json!({"command":"inspect","input":w9})))["projects"].clone();
    assert_eq!(
        left.as_array().unwrap().len(),
        2,
        "the converted project left"
    );

    // Revert: the project takes the baseline's data and keeps its ID.
    let (w10, r) = edit(
        &dir,
        &w9,
        "revert",
        first,
        json!({"op":"revert","baseline":format!("{pid} - B1")}),
    );
    let reverted = ok(&r).clone();
    assert!(reverted["result"]["activities"].as_u64().unwrap() > 0);
    let b = band(&w10, first);
    assert_eq!(b["project_id"], pid.as_str());
    assert_eq!(
        b["baselines"].as_array().unwrap().len(),
        2,
        "the baseline stays"
    );
    assert_eq!(b["assigned"]["project_baseline"]["id"], b1);

    // The other project is untouched throughout.
    assert!(band(&w10, second)["baselines"]
        .as_array()
        .unwrap()
        .is_empty());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn baseline_command_line_and_mcp_tools() {
    let parsed = cli::parse(["baseline", "list", "--input", "w.db"]).unwrap();
    assert!(matches!(parsed, Invocation::Run(_)));
    let parsed = cli::parse([
        "baseline",
        "assign",
        "--input",
        "w.db",
        "--workspace-id",
        "3",
        "--output",
        "n.db",
        "--project-baseline",
        "current",
        "--primary",
        "B1",
    ])
    .unwrap();
    assert!(matches!(parsed, Invocation::Run(_)));
    assert!(
        cli::parse(["baseline", "add", "--input", "w.db", "--output", "n.db"]).is_err(),
        "--workspace-id is required"
    );
    assert!(
        cli::parse([
            "baseline",
            "copy",
            "--input",
            "w.db",
            "--workspace-id",
            "3",
            "--dry-run"
        ])
        .is_err(),
        "--baseline is required"
    );
    assert!(cli::parse(["baseline", "explode", "--input", "w.db"]).is_err());
    assert!(
        cli::HELP.contains("baseline restore") || cli::HELP.contains("copy|delete|restore|revert")
    );
    // The command line runs the same edit.
    let dir = temp("cli");
    let (db, first, _) = workspace(&dir);
    let out = dir.join("c.db");
    let Invocation::Run(command) = cli::parse([
        "baseline",
        "add",
        "--input",
        db.to_str().unwrap(),
        "--workspace-id",
        &first.to_string(),
        "--output",
        out.to_str().unwrap(),
    ])
    .unwrap() else {
        panic!("a command")
    };
    dispatch(*command).unwrap();
    assert_eq!(band(&out, first)["baselines"].as_array().unwrap().len(), 1);
    // A dry run writes nothing.
    let r = run(
        json!({"command":"baseline","input":out,"workspace_id":first,"dry_run":true,"operation":{"op":"add"}}),
    );
    assert_eq!(ok(&r)["project"]["baselines"].as_array().unwrap().len(), 2);
    assert_eq!(band(&out, first)["baselines"].as_array().unwrap().len(), 1);

    // MCP lists both tools and runs them.
    let meta = json!({"io.modelcontextprotocol/protocolVersion":openxer_automation::mcp::PROTOCOL_VERSION,"io.modelcontextprotocol/clientCapabilities":{}});
    let mut server = Server::default();
    let tools = server
        .handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{"_meta":meta}}))
        .unwrap();
    let names: Vec<String> = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect();
    assert!(names.contains(&"openxer_baseline_list".to_string()));
    assert!(names.contains(&"openxer_baseline_edit".to_string()));
    let listed = server
        .handle(json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"_meta":meta,"name":"openxer_baseline_list","arguments":{"input":out}}}))
        .unwrap();
    assert_eq!(listed["result"]["isError"], false, "{listed:#}");
    let mcp_out = dir.join("m.db");
    let copied = server
        .handle(json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"_meta":meta,"name":"openxer_baseline_edit","arguments":{"input":out,"output":mcp_out,"workspace_id":first,"op":"copy","baseline":"1"}}}))
        .unwrap();
    assert_eq!(copied["result"]["isError"], false, "{copied:#}");
    assert_eq!(
        band(&mcp_out, first)["baselines"].as_array().unwrap().len(),
        2
    );
    // A refusal comes back as an error result.
    let assign = dir.join("a.db");
    server
        .handle(json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"_meta":meta,"name":"openxer_baseline_edit","arguments":{"input":mcp_out,"output":assign,"workspace_id":first,"op":"assign","primary":"1"}}}))
        .unwrap();
    let denied = server
        .handle(json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"_meta":meta,"name":"openxer_baseline_edit","arguments":{"input":assign,"output":dir.join("d.db"),"workspace_id":first,"op":"delete","baseline":"1"}}}))
        .unwrap();
    assert_eq!(denied["result"]["isError"], true, "{denied:#}");
    assert_eq!(
        denied["result"]["structuredContent"]["error"]["code"],
        "refused"
    );
    let _ = std::fs::remove_dir_all(dir);
}
