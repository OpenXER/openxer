use openxer_automation::{
    calculated_json, dispatch, edit_relationship, parse_command, schedule_project, SchedulePatch,
};
use openxer_engine::{
    model::{StatusType, TaskType},
    scheduler,
};
use openxer_xer::{Document, ProjectData};
use serde_json::{json, Value};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

fn source(dir: &Path) -> PathBuf {
    let input = dir.join("source.xer");
    openxer_xer::starter()
        .updated_document()
        .write(&input)
        .unwrap();
    input
}
fn run(value: Value) -> openxer_automation::Result<Value> {
    dispatch(parse_command(value)?)
}
fn read(path: &Path) -> ProjectData {
    ProjectData::from_document(Document::read(path).unwrap(), 1, "test".into()).unwrap()
}
fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_openxer-cli"))
}

#[test]
fn cli_schedule_matches_direct_native_engine_and_preserves_input() {
    let dir = tempfile::tempdir().unwrap();
    let input = source(dir.path());
    let output = dir.path().join("scheduled.xer");
    let original = fs::read(&input).unwrap();
    let mut expected = read(&input);
    expected.model.project.data_date =
        openxer_xer::parse_relative(expected.model.origin_serial, "2026-11-09 08:00").unwrap();
    expected.refresh_derived_durations();
    scheduler::schedule_with_resources(&mut expected.model, &expected.resources).unwrap();
    let reply = bin()
        .args(["schedule", "--input"])
        .arg(&input)
        .arg("--output")
        .arg(&output)
        .args(["--data-date", "2026-11-09 08:00"])
        .output()
        .unwrap();
    assert!(
        reply.status.success(),
        "{} {}",
        String::from_utf8_lossy(&reply.stdout),
        String::from_utf8_lossy(&reply.stderr)
    );
    let envelope: Value = serde_json::from_slice(&reply.stdout).unwrap();
    assert_eq!(envelope["ok"], true);
    assert!(
        envelope["result"]["schedule"]["changed_activity_count"]
            .as_u64()
            .unwrap()
            > 0
    );
    let actual = read(&output);
    for (want, got) in expected
        .model
        .activities
        .iter()
        .zip(&actual.model.activities)
    {
        assert_eq!(
            calculated_json(&expected, want),
            calculated_json(&actual, got),
            "{}",
            got.task_code
        );
    }
    assert_eq!(original, fs::read(&input).unwrap());
}

#[test]
fn relationship_cycle_and_extreme_lag_are_atomic_and_do_not_publish() {
    let dir = tempfile::tempdir().unwrap();
    let input = source(dir.path());
    let output = dir.path().join("invalid.xer");
    let original = fs::read(&input).unwrap();
    let p = read(&input);
    let link = &p.model.links[0];
    let mut edit = json!({"action":"add","predecessor":p.model.activities[link.succ].task_code,"successor":p.model.activities[link.pred].task_code,"relationship_type":"FS"});
    let err = run(json!({"command":"relationship","input":input,"output":output,"edit":edit}))
        .unwrap_err();
    assert_eq!(err.code, "invalid_relationship");
    assert!(!output.exists());
    edit["lag_minutes"] = json!(i64::MAX);
    let err = run(json!({"command":"relationship","input":input,"output":output,"edit":edit}))
        .unwrap_err();
    assert_eq!(err.code, "invalid_arguments");
    assert!(!output.exists());
    assert_eq!(original, fs::read(&input).unwrap());
}

#[test]
fn relationship_update_retains_dates_and_unknown_xer_data() {
    let dir = tempfile::tempdir().unwrap();
    let input = source(dir.path());
    let mut doc = Document::read(&input).unwrap();
    doc.table_mut("CUSTOM_VENDOR")
        .add(&[("payload", "opaque".into())]);
    doc.write(&input).unwrap();
    let before = read(&input);
    let link = &before.model.links[0];
    let output = dir.path().join("edited.xer");
    let report = run(json!({"command":"relationship","input":input,"output":output,"edit":{"action":"update","predecessor":before.model.activities[link.pred].task_code,"successor":before.model.activities[link.succ].task_code,"relationship_type":link.link_type.as_str().trim_start_matches("PR_"),"lag_minutes":60}})).unwrap();
    assert_eq!(report["schedule_recalculated"], false);
    let after = read(&output);
    assert_eq!(after.model.links[0].lag, 60);
    for (a, b) in before.model.activities.iter().zip(&after.model.activities) {
        assert_eq!(calculated_json(&before, a), calculated_json(&after, b));
    }
    assert_eq!(
        after.document.table("CUSTOM_VENDOR").unwrap().rows[0][0],
        "opaque"
    );
}

#[test]
fn workspace_copy_preserves_other_projects_preferences_encoding_and_level_setting() {
    let dir = tempfile::tempdir().unwrap();
    let input = source(dir.path());
    let workspace = dir.path().join("workspace.db");
    // CP1252 survives native import -> workspace -> export.
    let mut doc = Document::read(&input).unwrap();
    doc.windows_encoding = true;
    doc.table_mut("PROJECT")
        .set(0, "proj_short_name", "Café".into());
    doc.write(&input).unwrap();
    run(json!({"command":"import","input":input,"output":workspace})).unwrap();
    let connection = rusqlite::Connection::open(&workspace).unwrap();
    connection.execute("INSERT INTO projects(source_name,project_key,title,xer_content) VALUES('other',999,'Other','opaque')",[]).unwrap();
    connection
        .execute(
            "INSERT INTO preferences(key,value) VALUES('custom','keep')",
            [],
        )
        .unwrap();
    connection.close().unwrap();
    let original = fs::read(&workspace).unwrap();
    let output = dir.path().join("scheduled.db");
    run(json!({"command":"schedule","input":workspace,"output":output,"workspace_id":1,"options":{"level_resources":true}})).unwrap();
    let c = rusqlite::Connection::open(&output).unwrap();
    let (other,pref,level,encoding): (String,String,bool,bool) = c.query_row("SELECT (SELECT xer_content FROM projects WHERE project_key=999),(SELECT value FROM preferences WHERE key='custom'),level_resources,source_windows_encoding FROM projects WHERE id=1",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).unwrap();
    assert_eq!(
        (other.as_str(), pref.as_str(), level, encoding),
        ("opaque", "keep", true, true)
    );
    c.close().unwrap();
    assert_eq!(original, fs::read(&workspace).unwrap());
    let exported = dir.path().join("roundtrip.xer");
    run(json!({"command":"export","input":output,"output":exported,"workspace_id":1})).unwrap();
    assert!(Document::read(&exported).unwrap().windows_encoding);
    assert!(fs::read(&exported).unwrap().contains(&0xe9));
}

#[test]
fn load_requires_unambiguous_selection_and_pages_both_collections() {
    let dir = tempfile::tempdir().unwrap();
    let input = source(dir.path());
    let result=run(json!({"command":"load","input":input,"offset":1,"limit":2,"relationship_offset":2,"relationship_limit":3})).unwrap();
    assert_eq!(result["activities"].as_array().unwrap().len(), 2);
    assert_eq!(result["relationships"].as_array().unwrap().len(), 3);
    assert_eq!(result["next_offset"], 3);
    assert_eq!(result["next_relationship_offset"], 5);
    // A limit of 0 returns counts only; above 2000 is still refused.
    let counts = run(json!({"command":"load","input":input,"relationship_limit":0})).unwrap();
    assert!(counts["relationships"].as_array().unwrap().is_empty());
    assert!(counts["total_relationships"].as_u64().unwrap() > 0);
    assert!(run(json!({"command":"load","input":input,"relationship_limit":2001})).is_err());
    let db = dir.path().join("multiple.db");
    run(json!({"command":"import","input":input,"output":db})).unwrap();
    let c = rusqlite::Connection::open(&db).unwrap();
    c.execute("INSERT INTO projects(source_name,project_key,title,xer_content) SELECT 'second',project_key,title,xer_content FROM projects",[]).unwrap();
    c.close().unwrap();
    assert_eq!(
        run(json!({"command":"load","input":db,"project_id":1}))
            .unwrap_err()
            .code,
        "ambiguous_project"
    );
    assert!(run(json!({"command":"load","input":db,"workspace_id":2})).is_ok());
}

#[test]
fn existing_outputs_and_invalid_options_never_modify_files() {
    let dir = tempfile::tempdir().unwrap();
    let input = source(dir.path());
    let original = fs::read(&input).unwrap();
    assert_eq!(
        run(json!({"command":"schedule","input":input,"output":input}))
            .unwrap_err()
            .code,
        "output_exists"
    );
    let output = dir.path().join("rejected.xer");
    assert!(run(json!({"command":"schedule","input":input,"output":output,"options":{"retained_logic":true,"progress_override":true}})).is_err());
    assert!(parse_command(
        json!({"command":"schedule","input":input,"output":output,"options":{"typo":true}})
    )
    .is_err());
    assert!(!output.exists());
    assert_eq!(original, fs::read(&input).unwrap());
}

#[test]
fn callable_api_is_atomic_and_completed_internal_dates_are_not_public() {
    let mut p = openxer_xer::starter();
    let before = p.model.clone();
    let patch = SchedulePatch {
        retained_logic: Some(true),
        progress_override: Some(true),
        ..Default::default()
    };
    assert!(schedule_project(&mut p, &patch).is_err());
    assert_eq!(p.model.activities, before.activities);
    assert_eq!(p.model.options, before.options);
    let edit=serde_json::from_value(json!({"action":"add","predecessor":"missing","successor":"RB-1000","relationship_type":"FS"})).unwrap();
    assert!(edit_relationship(&mut p, &edit).is_err());
    assert_eq!(p.model.links, before.links);
    let a = &mut p.model.activities[0];
    a.status = StatusType::Completed;
    a.restart = None;
    a.reend = None;
    a.rem_restart = Some(10);
    a.rem_reend = Some(20);
    let data = calculated_json(&p, &p.model.activities[0]);
    assert!(data["remaining_start"].is_null());
    assert!(data["remaining_finish"].is_null());
    p.model.activities[1].task_type = TaskType::Resource;
    assert_eq!(
        schedule_project(&mut p, &SchedulePatch::default())
            .unwrap_err()
            .code,
        "unsupported_schedule"
    );
}

#[test]
fn mcp_stdio_legacy_flow_and_errors_emit_only_json() {
    let dir = tempfile::tempdir().unwrap();
    let input = source(dir.path());
    let scheduled = dir.path().join("mcp-scheduled.xer");
    let source_bytes = fs::read(&input).unwrap();
    let mut child = bin()
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let requests = [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"openxer_inspect","arguments":{"input":input}}}),
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"openxer_schedule","arguments":{"input":input,"unknown":true}}}),
        json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"openxer_load","arguments":{"input":dir.path().join("missing.xer")}}}),
        json!({"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"openxer_schedule","arguments":{"input":input,"output":scheduled,"options":{"data_date":"2026-11-10 08:00"}}}}),
    ];
    let mut stdin = child.stdin.take().unwrap();
    for request in requests {
        writeln!(stdin, "{request}").unwrap();
    }
    drop(stdin);
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let replies: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    assert_eq!(replies.len(), 6);
    assert_eq!(replies[0]["result"]["protocolVersion"], "2025-11-25");
    assert_eq!(replies[1]["result"]["tools"].as_array().unwrap().len(), 88);
    assert_eq!(replies[2]["result"]["structuredContent"]["ok"], true);
    assert_eq!(replies[3]["error"]["code"], -32602);
    assert_eq!(replies[4]["result"]["isError"], true);
    assert_eq!(replies[4]["result"]["structuredContent"]["ok"], false);
    assert_eq!(replies[5]["result"]["structuredContent"]["ok"], true);
    let mut expected = read(&input);
    expected.model.project.data_date =
        openxer_xer::parse_relative(expected.model.origin_serial, "2026-11-10 08:00").unwrap();
    expected.refresh_derived_durations();
    scheduler::schedule_with_resources(&mut expected.model, &expected.resources).unwrap();
    let actual = read(&scheduled);
    for (want, got) in expected
        .model
        .activities
        .iter()
        .zip(&actual.model.activities)
    {
        assert_eq!(
            calculated_json(&expected, want),
            calculated_json(&actual, got)
        );
    }
    assert_eq!(source_bytes, fs::read(&input).unwrap());
}

#[test]
fn modern_mcp_metadata_is_per_request_and_no_handshake_is_required() {
    let mut server = openxer_automation::mcp::Server::default();
    let meta = json!({"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{}});
    let discover = server
        .handle(
            json!({"jsonrpc":"2.0","id":"d","method":"server/discover","params":{"_meta":meta}}),
        )
        .unwrap();
    assert_eq!(discover["result"]["resultType"], "complete");
    assert_eq!(discover["result"]["supportedVersions"][0], "2026-07-28");
    let list = server
        .handle(json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{"_meta":meta}}))
        .unwrap();
    assert_eq!(list["result"]["tools"].as_array().unwrap().len(), 88);
    for options in [
        json!({"level_resources":null}),
        json!({"max_float_paths":0}),
        json!({"lag_calendar":"typo"}),
    ] {
        let invalid=server.handle(json!({"jsonrpc":"2.0","id":20,"method":"tools/call","params":{"_meta":meta,"name":"openxer_schedule","arguments":{"input":"missing.xer","output":"unused.xer","options":options}}})).unwrap();
        assert_eq!(invalid["error"]["code"], -32602, "{invalid}");
    }
    let missing = server
        .handle(json!({"jsonrpc":"2.0","id":3,"method":"server/discover"}))
        .unwrap();
    assert_eq!(missing["error"]["code"], -32602);
    let unsupported=server.handle(json!({"jsonrpc":"2.0","id":4,"method":"tools/list","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"bogus","io.modelcontextprotocol/clientCapabilities":{}}}})).unwrap();
    assert_eq!(unsupported["error"]["code"], -32022);
    assert_eq!(unsupported["error"]["data"]["requested"], "bogus");
    let stale = server
        .handle(json!({"jsonrpc":"2.0","id":5,"method":"tools/list"}))
        .unwrap();
    assert_eq!(stale["error"]["code"], -32600);
    assert_eq!(server.handle(json!([])).unwrap()["error"]["code"], -32600);
    assert_eq!(
        server
            .handle(json!({"jsonrpc":"2.0","id":null,"method":"ping"}))
            .unwrap()["error"]["code"],
        -32600
    );
    assert!(server.handle(json!({"jsonrpc":"2.0","method":"tools/call","params":{"name":"openxer_schedule","arguments":{}}})).is_none());
}

#[test]
fn schedule_change_details_are_bounded_but_counts_are_complete() {
    let mut p = openxer_xer::starter();
    let template = p.model.activities[0].clone();
    p.model.links.clear();
    p.model.activities = (0..250)
        .map(|n| {
            let mut a = template.clone();
            a.task_id = 10000 + n;
            a.task_code = format!("A{n}");
            a.preds.clear();
            a.succs.clear();
            a.early_start = None;
            a.early_finish = None;
            a.late_start = None;
            a.late_finish = None;
            a
        })
        .collect();
    let report = schedule_project(&mut p, &SchedulePatch::default()).unwrap();
    assert_eq!(report["changed_activity_count"], 250);
    assert_eq!(report["changes"].as_array().unwrap().len(), 200);
    assert_eq!(report["changes_truncated"], true);
    assert_eq!(report["change_limit"], 200);
    assert!(report["generated_field_count"].as_u64().unwrap() >= 1000);
}

#[test]
fn quote_marks_are_single_in_results_and_doubled_in_files_and_the_workspace() {
    let dir = tempfile::tempdir().unwrap();
    let mut p = openxer_xer::starter();
    let id = p.model.activities[0].task_id;
    p.details.get_mut(&id).unwrap().name = "Set 8\" valve \"V-1\"".into();
    let input = dir.path().join("quoted.xer");
    p.updated_document().write(&input).unwrap();
    let doubled = "Set 8\"\" valve \"\"V-1\"\"";
    assert!(fs::read_to_string(&input).unwrap().contains(doubled));

    let names = |result: &Value| -> Vec<String> {
        result["activities"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a["name"].as_str().unwrap().to_string())
            .collect()
    };
    let loaded = run(json!({"command":"load","input":input})).unwrap();
    assert!(names(&loaded).contains(&"Set 8\" valve \"V-1\"".to_string()));

    // A scheduled copy writes the doubled form and reads back the same.
    let output = dir.path().join("scheduled.xer");
    run(json!({"command":"schedule","input":input,"output":output})).unwrap();
    assert!(fs::read_to_string(&output).unwrap().contains(doubled));
    let again = run(json!({"command":"load","input":output})).unwrap();
    assert!(names(&again).contains(&"Set 8\" valve \"V-1\"".to_string()));

    // The workspace keeps XER text and returns single quote marks.
    let db = dir.path().join("workspace.db");
    run(json!({"command":"import","input":input,"output":db})).unwrap();
    let c = rusqlite::Connection::open(&db).unwrap();
    let stored: String = c
        .query_row("SELECT xer_content FROM projects", [], |r| r.get(0))
        .unwrap();
    assert!(stored.contains(doubled));
    c.close().unwrap();
    let from_db = run(json!({"command":"load","input":db})).unwrap();
    assert!(names(&from_db).contains(&"Set 8\" valve \"V-1\"".to_string()));
}

/// A workspace as an earlier OpenXER wrote it (each project keeps its
/// whole text, workspace schema 1) holding the project of `input`.
fn version_1_workspace(dir: &Path, input: &Path) -> PathBuf {
    let workspace = dir.join("workspace.db");
    let c = rusqlite::Connection::open(&workspace).unwrap();
    c.execute_batch("CREATE TABLE projects(id INTEGER PRIMARY KEY,source_name TEXT NOT NULL,project_key INTEGER NOT NULL,title TEXT NOT NULL,xer_content TEXT NOT NULL,updated_at TEXT DEFAULT CURRENT_TIMESTAMP,source_windows_encoding INTEGER NOT NULL DEFAULT 0,level_resources INTEGER NOT NULL DEFAULT 0,UNIQUE(source_name,project_key));CREATE TABLE preferences(key TEXT PRIMARY KEY,value TEXT NOT NULL);").unwrap();
    c.execute(
        "INSERT INTO projects(source_name,project_key,title,xer_content) VALUES('source.xer',1,'RIVERBEND',?1)",
        [Document::read(input).unwrap().text()],
    )
    .unwrap();
    workspace
}

#[test]
fn workspace_migrate_previews_then_writes_a_shared_dictionary_copy() {
    let dir = tempfile::tempdir().unwrap();
    let input = source(dir.path());
    let workspace = version_1_workspace(dir.path(), &input);
    // A second project whose standard calendar works longer days.
    let mut other = openxer_xer::starter();
    other.model.project.name = "SECOND".into();
    let mut doc = other.updated_document();
    let t = doc.table_mut("CALENDAR");
    let data = t.value(&t.rows[0], "clndr_data").replace("17:00", "18:00");
    t.set(0, "clndr_data", data);
    let c = rusqlite::Connection::open(&workspace).unwrap();
    c.execute(
        "INSERT INTO projects(source_name,project_key,title,xer_content) VALUES('second.xer',1,'SECOND',?1)",
        [doc.text()],
    )
    .unwrap();
    c.close().unwrap();
    let original = fs::read(&workspace).unwrap();
    let out = bin()
        .args(["workspace", "migrate", "--input"])
        .arg(&workspace)
        .arg("--dry-run")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let preview: Value = serde_json::from_slice(&out.stdout).unwrap();
    let result = &preview["result"];
    assert_eq!(result["dry_run"], true);
    assert_eq!(result["projects"].as_array().unwrap().len(), 2);
    let copies = &result["projects"][1]["copies"];
    assert_eq!(copies[0]["table"], "CALENDAR");
    assert_eq!(copies[0]["renamed"], "Standard 5-day / 8-hour (SECOND)");
    assert_eq!(copies[0]["differences"], json!(["clndr_data"]));
    assert!(result["report"].as_str().unwrap().contains("(SECOND)"));
    assert_eq!(
        original,
        fs::read(&workspace).unwrap(),
        "a dry run writes nothing"
    );
    let migrated = dir.path().join("migrated.db");
    let applied =
        run(json!({"command":"workspace_migrate","input":workspace,"output":migrated})).unwrap();
    assert_eq!(applied["schema_after"], 3);
    assert_eq!(
        original,
        fs::read(&workspace).unwrap(),
        "the input stays as the backup"
    );
    let inspected = run(json!({"command":"inspect","input":migrated})).unwrap();
    assert_eq!(inspected["workspace_schema"], 3);
    // Commands read the converted workspace: each project with its
    // calendars back from the dictionary.
    let loaded = run(json!({"command":"load","input":migrated,"workspace_id":2})).unwrap();
    assert_eq!(loaded["project"]["name"], "SECOND");
    // Either flag alone is required.
    assert!(run(json!({"command":"workspace_migrate","input":workspace})).is_err());
    let again =
        run(json!({"command":"workspace_migrate","input":migrated,"dry_run":true})).unwrap();
    assert_eq!(again["already_migrated"], true);
}

#[test]
fn workspace_migrate_refuses_a_workspace_it_cannot_read_and_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let input = source(dir.path());
    let workspace = version_1_workspace(dir.path(), &input);
    let c = rusqlite::Connection::open(&workspace).unwrap();
    c.execute("INSERT INTO projects(source_name,project_key,title,xer_content) VALUES('other',999,'Other','opaque')",[]).unwrap();
    c.close().unwrap();
    let migrated = dir.path().join("migrated.db");
    let e = run(json!({"command":"workspace_migrate","input":workspace,"output":migrated}))
        .unwrap_err();
    assert_eq!(e.code, "migration_failed");
    assert!(!migrated.exists());
}
