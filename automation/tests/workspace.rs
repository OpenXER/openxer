//! Workspaces with a shared dictionary (schema 3) through the command line
//! and MCP: commands read and write them, an edit of a global calendar
//! reaches every project, `workspace dictionary` and `dictionary-usage`
//! list the dictionary and its uses, import matches dictionary rows (Keep
//! and Update), and an export holds only the rows its project uses.
use openxer_automation::{cli, dispatch, mcp, parse_command};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("openxer-auto-ws-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn run(args: &[&str]) -> Value {
    match cli::parse(args.iter().copied()).unwrap() {
        cli::Invocation::Run(c) => dispatch(*c).unwrap_or_else(|e| panic!("{e}")),
        other => panic!("expected a command, got {other:?}"),
    }
}

fn fails(args: &[&str]) -> String {
    match cli::parse(args.iter().copied()) {
        Ok(cli::Invocation::Run(c)) => dispatch(*c).expect_err("refused").code,
        Ok(other) => panic!("expected a command, got {other:?}"),
        Err(e) => e.code,
    }
}

fn s(p: &Path) -> &str {
    p.to_str().unwrap()
}

/// The starter project as an XER file, its standard calendar given
/// `hours` a day and a welder.
fn xer(d: &Path, name: &str, hours: &str) -> PathBuf {
    let mut p = openxer_xer::starter();
    openxer_xer::edit::tables::add_resource(&mut p, "WLD").unwrap();
    let mut doc = p.updated_document();
    let t = doc.table_mut("CALENDAR");
    for i in 0..t.rows.len() {
        if t.value(&t.rows[i], "clndr_name") == "Standard 5-day / 8-hour" {
            t.set(i, "day_hr_cnt", hours.into());
        }
    }
    let path = d.join(format!("{name}.xer"));
    doc.write(&path).unwrap();
    path
}

/// A workspace with two projects made from the same file (they share its
/// calendar and welder).
fn two_projects(d: &Path) -> PathBuf {
    let file = xer(d, "starter", "8");
    let one = d.join("one.db");
    run(&["import", "--input", s(&file), "--output", s(&one)]);
    let two = d.join("two.db");
    let r = run(&[
        "import",
        "--input",
        s(&file),
        "--output",
        s(&two),
        "--workspace",
        s(&one),
        "--action",
        "new",
    ]);
    assert_eq!(r["projects"][0]["workspace_id"], 2);
    two
}

fn calendar_data(input: &Path, id: i64) -> String {
    let out = input.with_file_name(format!("export-{id}.xer"));
    let _ = std::fs::remove_file(&out);
    run(&[
        "export",
        "--input",
        s(input),
        "--output",
        s(&out),
        "--workspace-id",
        &id.to_string(),
    ]);
    let doc = openxer_xer::Document::read(&out).unwrap();
    let t = doc.table("CALENDAR").unwrap();
    t.rows
        .iter()
        .find(|r| t.value(r, "clndr_name") == "Standard 5-day / 8-hour")
        .map(|r| t.value(r, "clndr_data").to_string())
        .unwrap()
}

#[test]
fn a_global_calendar_edited_in_one_project_reaches_every_project() {
    let d = dir("calendar");
    let ws = two_projects(&d);
    let inspected = run(&["inspect", "--input", s(&ws)]);
    assert_eq!(
        inspected["workspace_schema"], 3,
        "a new workspace shares its dictionary"
    );
    assert_eq!(
        inspected["dictionary"]["rows"]["RSRC"], 1,
        "one welder for both"
    );
    let before = calendar_data(&ws, 2);
    let edited = d.join("edited.db");
    run(&[
        "calendar-weekday",
        "--input",
        s(&ws),
        "--output",
        s(&edited),
        "--workspace-id",
        "1",
        "--calendar",
        "Standard 5-day / 8-hour",
        "--weekday",
        "6",
        "--shifts",
        r#"[["08:00","12:00"]]"#,
    ]);
    let after = calendar_data(&edited, 2);
    assert_ne!(
        after, before,
        "project 2 sees the Saturday hours set in project 1"
    );
    assert_eq!(after, calendar_data(&edited, 1));
    // Scheduling project 2 from the edited workspace works with it.
    let scheduled = d.join("scheduled.db");
    run(&[
        "schedule",
        "--input",
        s(&edited),
        "--output",
        s(&scheduled),
        "--workspace-id",
        "2",
    ]);
}

#[test]
fn the_dictionary_and_its_uses_are_listed() {
    let d = dir("list");
    let ws = two_projects(&d);
    let list = run(&["workspace", "dictionary", "--input", s(&ws)]);
    let rsrc = list["tables"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["table"] == "RSRC")
        .unwrap();
    assert_eq!(rsrc["kind"], "Resource");
    assert_eq!(rsrc["rows"][0]["name"], "WLD");
    let only = run(&[
        "workspace",
        "dictionary",
        "--input",
        s(&ws),
        "--table",
        "calendar",
    ]);
    assert_eq!(only["tables"].as_array().unwrap().len(), 1);
    let cal = &only["tables"][0]["rows"][0];
    assert_eq!(
        cal["used_by"],
        json!([1, 2]),
        "both projects use the calendar"
    );
    let usage = run(&[
        "workspace",
        "dictionary-usage",
        "--input",
        s(&ws),
        "--table",
        "calendar",
        "--name",
        "Standard 5-day / 8-hour",
    ]);
    assert_eq!(usage["in_use"], true);
    assert_eq!(usage["used_by"].as_array().unwrap().len(), 2);
    let welder = run(&[
        "workspace",
        "dictionary-usage",
        "--input",
        s(&ws),
        "--table",
        "resource",
        "--name",
        "WLD",
    ]);
    assert_eq!(welder["in_use"], false, "no activity uses the welder");
    assert_eq!(
        fails(&[
            "workspace",
            "dictionary-usage",
            "--input",
            s(&ws),
            "--table",
            "resource"
        ]),
        "invalid_arguments"
    );
    // MCP: the same commands as tools.
    let mut server = mcp::Server::default();
    let meta = json!({"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{}});
    let listed = server
        .handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{"_meta":meta}}))
        .unwrap();
    let names: Vec<String> = listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect();
    for tool in [
        "openxer_workspace_migrate",
        "openxer_workspace_dictionary",
        "openxer_workspace_dictionary_usage",
    ] {
        assert!(names.contains(&tool.to_string()), "{tool}");
    }
    let called = server
        .handle(json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"openxer_workspace_dictionary_usage","arguments":{"input":ws,"table":"calendar","name":"Standard 5-day / 8-hour"},"_meta":meta}}))
        .unwrap();
    assert_eq!(
        called["result"]["structuredContent"]["result"]["in_use"],
        true
    );
    // The help names the commands and the shared dictionary.
    for text in [
        "workspace dictionary",
        "dictionary-usage",
        "--dictionary",
        "schema 3",
    ] {
        assert!(cli::HELP.contains(text), "{text}");
    }
}

#[test]
fn import_matches_dictionary_rows_and_keep_or_update_choose_otherwise() {
    let d = dir("import");
    let ws = two_projects(&d);
    // The same file again: nothing new in the dictionary.
    let same = xer(&d, "same", "8");
    let again = d.join("again.db");
    let r = run(&[
        "import",
        "--input",
        s(&same),
        "--output",
        s(&again),
        "--workspace",
        s(&ws),
        "--action",
        "new",
    ]);
    let rows = |db: &Path| run(&["inspect", "--input", s(db)])["dictionary"]["rows"].clone();
    assert_eq!(rows(&again), rows(&ws));
    assert!(r["projects"][0]["dictionary"]
        .as_array()
        .unwrap()
        .iter()
        .all(|l| !l.as_str().unwrap().contains("is added as")));
    // Other hours: a reported copy by default.
    let ten = xer(&d, "ten", "10");
    let copied = d.join("copied.db");
    let r = run(&[
        "import",
        "--input",
        s(&ten),
        "--output",
        s(&copied),
        "--workspace",
        s(&ws),
        "--action",
        "new",
    ]);
    let lines = r["projects"][0]["dictionary"].to_string();
    assert!(
        lines.contains("is added as \\\"Standard 5-day / 8-hour_1\\\"")
            && lines.contains("day_hr_cnt"),
        "{lines}"
    );
    assert_eq!(rows(&copied)["CALENDAR"], 2);
    // Keep: the workspace's calendar; Update: the file's values for all.
    let kept = d.join("kept.db");
    run(&[
        "import",
        "--input",
        s(&ten),
        "--output",
        s(&kept),
        "--workspace",
        s(&ws),
        "--action",
        "new",
        "--dictionary",
        "calendar=keep",
    ]);
    assert_eq!(rows(&kept)["CALENDAR"], 1);
    let updated = d.join("updated.db");
    run(&[
        "import",
        "--input",
        s(&ten),
        "--output",
        s(&updated),
        "--workspace",
        s(&ws),
        "--action",
        "new",
        "--dictionary",
        "calendar=update",
    ]);
    assert_eq!(rows(&updated)["CALENDAR"], 1);
    let list = run(&[
        "workspace",
        "dictionary",
        "--input",
        s(&updated),
        "--table",
        "CALENDAR",
    ]);
    assert_eq!(list["tables"][0]["rows"][0]["used_by"], json!([1, 2, 3]));
    assert_eq!(
        fails(&[
            "import",
            "--input",
            s(&ten),
            "--output",
            s(&d.join("bad.db")),
            "--workspace",
            s(&ws),
            "--dictionary",
            "calendar=merge",
        ]),
        "invalid_arguments"
    );
}

#[test]
fn an_export_holds_the_dictionary_rows_its_project_uses() {
    let d = dir("export");
    let ws = two_projects(&d);
    // Project 2 gets a crane of its own.
    let crane = d.join("crane.db");
    let changes = d.join("crane.json");
    std::fs::write(
        &changes,
        json!({"format":"openxer-changes/1","operations":[{"op":"add_resource","resource":"CRN"}]})
            .to_string(),
    )
    .unwrap();
    run(&[
        "apply",
        "--input",
        s(&ws),
        "--output",
        s(&crane),
        "--workspace-id",
        "2",
        "--changes",
        s(&changes),
    ]);
    let request = json!({"command":"load","input":crane,"workspace_id":1});
    let loaded = dispatch(parse_command(request).unwrap()).unwrap();
    assert_eq!(loaded["project"]["name"], "RIVERBEND");
    let out = d.join("one.xer");
    run(&[
        "export",
        "--input",
        s(&crane),
        "--output",
        s(&out),
        "--workspace-id",
        "1",
    ]);
    let doc = openxer_xer::Document::read(&out).unwrap();
    assert!(
        !doc.table("RSRC").is_some_and(|t| t
            .rows
            .iter()
            .any(|r| t.value(r, "rsrc_short_name") == "CRN")),
        "project 1 does not use the crane"
    );
}
