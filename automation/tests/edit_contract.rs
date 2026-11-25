//! The edit contract the README promises, on the synthetic starter: file
//! order in a batch, values as typed on the command line, the loaded
//! leveling setting, the MCP value types, exit codes, a dry-run diff for
//! every table an operation writes, missing keys that change nothing, and
//! raw record edits that cannot overwrite keys or invent columns.
use openxer_automation::{
    cli::{self, Invocation},
    dispatch,
    mcp::Server,
    parse_command, response,
};
use openxer_xer::{starter, Document, ProjectData};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command as Process;

/// The starter with a labor resource PF assigned to RB-1020.
fn project() -> ProjectData {
    let mut p = starter();
    p.document.table_mut("RSRC").add(&[
        ("rsrc_id", "77".into()),
        ("rsrc_name", "Pipefitter".into()),
        ("rsrc_short_name", "PF".into()),
        ("rsrc_type", "RT_Labor".into()),
        ("def_qty_per_hr", "1".into()),
        ("clndr_id", "1".into()),
    ]);
    p.add_assignment(1020, Some(77), None).unwrap();
    p
}

fn source(dir: &Path) -> PathBuf {
    let path = dir.join("source.xer");
    project().updated_document().write(&path).unwrap();
    path
}

fn run(request: Value) -> Value {
    response(parse_command(request).and_then(dispatch))
}

fn apply(input: &Path, output: &Path, ops: Value) -> Value {
    run(json!({"command":"apply","input":input,"output":output,
        "changes":{"format":"openxer-changes/1","operations":ops}}))
}

fn dry_run(input: &Path, ops: Value) -> Value {
    run(json!({"command":"apply","input":input,"dry_run":true,
        "changes":{"format":"openxer-changes/1","operations":ops}}))
}

fn cli_run(args: &[&str]) -> Value {
    match cli::parse(args.iter().copied()).unwrap() {
        Invocation::Run(command) => response(dispatch(*command)),
        other => panic!("Expected a command, got {other:?}"),
    }
}

fn rows(path: &Path, table: &str) -> Vec<BTreeMap<String, String>> {
    let doc = Document::read(path).unwrap();
    let Some(t) = doc.table(table) else {
        return vec![];
    };
    t.rows
        .iter()
        .map(|r| t.fields.iter().cloned().zip(r.iter().cloned()).collect())
        .collect()
}

fn mcp(server: &mut Server, name: &str, arguments: Value) -> Value {
    let meta = json!({"io.modelcontextprotocol/protocolVersion":"2026-07-28",
        "io.modelcontextprotocol/clientCapabilities":{}});
    server
        .handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
            "params":{"_meta":meta,"name":name,"arguments":arguments}}))
        .unwrap()
}

// B1: a batch runs in file order even where additions are gathered.
#[test]
fn an_assignment_waits_for_the_resource_edits_before_it() {
    let dir = tempfile::tempdir().unwrap();
    let input = source(dir.path());
    let out = dir.path().join("added.xer");
    let r = apply(
        &input,
        &out,
        json!([
            {"op":"set_activity","activity":"RB-1010","field":"original_duration","value":16},
            {"op":"add_resource","resource":"WELD"},
            {"op":"assign_resource","activity":"RB-1010","resource":"WELD"}
        ]),
    );
    assert_eq!(r["ok"], true, "{r}");
    let renamed = dir.path().join("renamed.xer");
    let r = apply(
        &input,
        &renamed,
        json!([
            {"op":"set_resource","resource":"PF","field":"rsrc_short_name","value":"FITTER"},
            {"op":"assign_resource","activity":"RB-1010","resource":"FITTER"}
        ]),
    );
    assert_eq!(r["ok"], true, "{r}");
    // A resource deleted first cannot be assigned after.
    let gone = dir.path().join("gone.xer");
    let r = apply(
        &input,
        &gone,
        json!([
            {"op":"add_resource","resource":"SPARE"},
            {"op":"delete_resource","resource":"SPARE"},
            {"op":"assign_resource","activity":"RB-1010","resource":"SPARE"}
        ]),
    );
    assert_eq!(r["ok"], false, "{r}");
    assert!(
        r["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Operation 3"),
        "{r}"
    );
}

// B2: --value keeps the token typed for text fields.
#[test]
fn command_line_values_keep_the_text_typed() {
    let dir = tempfile::tempdir().unwrap();
    let input = source(dir.path());
    let input = input.to_str().unwrap();
    for (field, typed, column) in [
        ("activity_name", "1.10", "task_name"),
        ("activity_name", "12345678901234567890", "task_name"),
        ("activity_name", "0042", "task_name"),
    ] {
        let r = cli_run(&[
            "set-activity",
            "--input",
            input,
            "--activity",
            "RB-1030",
            "--field",
            field,
            "--value",
            typed,
            "--dry-run",
        ]);
        assert_eq!(r["ok"], true, "{r}");
        let after =
            &r["result"]["report"]["diff"]["activities"]["changed"]["RB-1030"][column]["after"];
        assert_eq!(after, typed, "{r}");
    }
    let r = cli_run(&[
        "set-activity",
        "--input",
        input,
        "--activity",
        "RB-1030",
        "--field",
        "activity_id",
        "--value",
        "1E5",
        "--dry-run",
    ]);
    assert_eq!(r["ok"], true, "{r}");
    let added = &r["result"]["report"]["diff"]["activities"]["added"];
    assert_eq!(added, &json!(["1E5"]), "{r}");
    // Numeric fields still read numbers from the text.
    let r = cli_run(&[
        "set-activity",
        "--input",
        input,
        "--activity",
        "RB-1030",
        "--field",
        "original_duration",
        "--value",
        "24",
        "--dry-run",
    ]);
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(
        r["result"]["report"]["diff"]["activities"]["changed"]["RB-1030"]["target_drtn_hr_cnt"]
            ["after"],
        "24"
    );
}

// B3: a bare schedule operation keeps the loaded leveling setting.
#[test]
fn a_schedule_operation_keeps_the_loaded_leveling_setting() {
    let mut p = project();
    p.model.options.do_level = true;
    let op = openxer_automation::edit::operation(json!({"op":"schedule"})).unwrap();
    let report = openxer_automation::edit::apply(&mut p, std::slice::from_ref(&op), true).unwrap();
    assert!(p.model.options.do_level, "leveling stays on");
    assert_eq!(report["results"][0]["level_resources"], true, "{report}");
    let op = openxer_automation::edit::operation(json!({"op":"schedule","level_resources":false}))
        .unwrap();
    openxer_automation::edit::apply(&mut p, std::slice::from_ref(&op), true).unwrap();
    assert!(!p.model.options.do_level, "an explicit false turns it off");
}

// B4: MCP takes the booleans and step numbers the handlers take.
#[test]
fn mcp_values_take_booleans_and_steps_take_numbers() {
    let dir = tempfile::tempdir().unwrap();
    let input = source(dir.path());
    let stepped = dir.path().join("stepped.xer");
    let r = apply(
        &input,
        &stepped,
        json!([{"op":"add_step","activity":"RB-1030","name":"Pour"}]),
    );
    assert_eq!(r["ok"], true, "{r}");
    let mut server = Server::default();
    let reply = mcp(
        &mut server,
        "openxer_set_step",
        json!({"input":stepped,"dry_run":true,"activity":"RB-1030","step":1,"field":"complete","value":true}),
    );
    assert_eq!(reply["result"]["isError"], false, "{reply}");
    let reply = mcp(
        &mut server,
        "openxer_shift_wbs",
        json!({"input":stepped,"dry_run":true,"wbs":"RB.2","direction":"sideways"}),
    );
    assert!(
        reply["error"]["message"]
            .as_str()
            .is_some_and(|m| m.contains("not an allowed value")),
        "{reply}"
    );
}

// B5: invalid arguments exit 2, command failures 1.
#[test]
fn invalid_arguments_exit_2_wherever_they_are_found() {
    let dir = tempfile::tempdir().unwrap();
    let input = source(dir.path());
    let exe = env!("CARGO_BIN_EXE_openxer-cli");
    let input = input.to_str().unwrap();
    let missing = dir.path().join("missing.xer");
    for (args, code) in [
        // 0 is allowed (counts only); above the page limit is not.
        (vec!["load", "--input", input, "--limit", "2001"], 2),
        (
            vec![
                "set-activity",
                "--input",
                input,
                "--activity",
                "RB-1030",
                "--field",
                "activity_name",
                "--value",
                "x",
            ],
            2,
        ),
        (vec!["records", "--input", input, "--kind", "bogus"], 2),
        (vec!["inspect", "--input", missing.to_str().unwrap()], 1),
    ] {
        let out = Process::new(exe).args(&args).output().unwrap();
        let value: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(out.status.code(), Some(code), "{args:?}: {value}");
        if code == 2 {
            assert_eq!(value["error"]["code"], "invalid_arguments", "{value}");
        }
    }
}

// B6: the dry-run diff shows every table the operations write.
#[test]
fn the_dry_run_diff_covers_registers_resources_and_options() {
    let dir = tempfile::tempdir().unwrap();
    let input = source(dir.path());
    let r = dry_run(
        &input,
        json!([
            {"op":"add_expense","activity":"RB-1030","name":"Crane rental"},
            {"op":"add_issue","name":"Late steel"},
            {"op":"set_resource","resource":"PF","field":"rsrc_name","value":"Fitters"},
            {"op":"add_step","activity":"RB-1030","name":"Pour"},
            {"op":"set_notebook","activity":"RB-1030","topic":"Change Log","text":"moved"},
            {"op":"set_schedule_option","field":"level_within_float_flag","value":"Y"}
        ]),
    );
    assert_eq!(r["ok"], true, "{r}");
    let diff = &r["result"]["report"]["diff"];
    for label in [
        "expenses",
        "issues",
        "resources",
        "steps",
        "notebooks",
        "schedule_options",
    ] {
        assert!(diff.get(label).is_some(), "{label} missing from {diff}");
    }
    assert_eq!(
        diff["resources"]["changed"]["PF"]["rsrc_name"]["after"],
        "Fitters"
    );
    assert!(diff["changed_field_count"].as_u64().unwrap() >= 6, "{diff}");
}

// B7: a missing key changes nothing; only null removes.
#[test]
fn a_missing_text_or_value_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mut p = project();
    p.document.table_mut("ACTVTYPE").add(&[
        ("actv_code_type_id", "5".into()),
        ("actv_code_type", "Area".into()),
        ("proj_id", "1".into()),
        ("seq_num", "1".into()),
    ]);
    p.document.table_mut("ACTVCODE").add(&[
        ("actv_code_id", "51".into()),
        ("actv_code_type_id", "5".into()),
        ("short_name", "N".into()),
        ("seq_num", "1".into()),
    ]);
    let input = dir.path().join("coded.xer");
    p.updated_document().write(&input).unwrap();
    let noted = dir.path().join("noted.xer");
    let r = apply(
        &input,
        &noted,
        json!([
            {"op":"set_notebook","activity":"RB-1030","topic":"Change Log","text":"moved"},
            {"op":"set_wbs_notebook","wbs":"RB.1","topic":"Change Log","text":"note"},
            {"op":"set_activity_code","activity":"RB-1030","code_type":"Area","value":"N"}
        ]),
    );
    assert_eq!(r["ok"], true, "{r}");
    let kept = dir.path().join("kept.xer");
    let r = apply(
        &noted,
        &kept,
        json!([
            {"op":"set_notebook","activity":"RB-1030","topic":"Change Log"},
            {"op":"set_wbs_notebook","wbs":"RB.1","topic":"Change Log"},
            {"op":"set_activity_code","activity":"RB-1030","code_type":"Area"}
        ]),
    );
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(rows(&kept, "TASKMEMO").len(), 1);
    assert_eq!(rows(&kept, "WBSMEMO").len(), 1);
    assert_eq!(rows(&kept, "TASKACTV").len(), 1);
    let removed = dir.path().join("removed.xer");
    let r = apply(
        &noted,
        &removed,
        json!([
            {"op":"set_notebook","activity":"RB-1030","topic":"Change Log","text":null},
            {"op":"set_wbs_notebook","wbs":"RB.1","topic":"Change Log","text":null},
            {"op":"set_activity_code","activity":"RB-1030","code_type":"Area","value":null}
        ]),
    );
    assert_eq!(r["ok"], true, "{r}");
    assert!(rows(&removed, "TASKMEMO").is_empty());
    assert!(rows(&removed, "WBSMEMO").is_empty());
    assert!(rows(&removed, "TASKACTV").is_empty());
}

// Raw record edits: keys and parent keys are refused, and a name that is
// neither a column of the table nor a standard field of it is refused
// with the fields that can be written.
#[test]
fn record_edits_refuse_keys_and_unknown_columns() {
    let dir = tempfile::tempdir().unwrap();
    let mut p = project();
    p.document.table_mut("ACTVTYPE").add(&[
        ("actv_code_type_id", "5".into()),
        ("actv_code_type", "Area".into()),
        ("proj_id", "1".into()),
        ("seq_num", "1".into()),
    ]);
    p.document.table_mut("ACTVCODE").add(&[
        ("actv_code_id", "51".into()),
        ("actv_code_type_id", "5".into()),
        ("short_name", "N".into()),
        ("seq_num", "1".into()),
    ]);
    let input = dir.path().join("coded.xer");
    p.updated_document().write(&input).unwrap();
    for (op, needle) in [
        (
            json!({"op":"set_wbs","wbs":"RB.1","field":"wbs_id","value":"999"}),
            "key",
        ),
        (
            json!({"op":"set_wbs","wbs":"RB.1","field":"parent_wbs_id","value":"999"}),
            "shift_wbs",
        ),
        (
            json!({"op":"set_resource","resource":"PF","field":"rsrc_id","value":"999"}),
            "key",
        ),
        (
            json!({"op":"set_resource","resource":"PF","field":"parent_rsrc_id","value":"999"}),
            "parent",
        ),
        (
            json!({"op":"set_code_value","code_type":"Area","code":"N","field":"actv_code_id","value":"999"}),
            "key",
        ),
        (
            json!({"op":"set_project","field":"proj_id","value":"999"}),
            "key",
        ),
        (
            json!({"op":"set_schedule_option","field":"proj_id","value":"999"}),
            "cannot be changed",
        ),
        (
            json!({"op":"set_calendar","calendar":"Standard 5-day / 8-hour","field":"base_clndr_id","value":"1"}),
            "base_calendar",
        ),
        (
            json!({"op":"set_wbs","wbs":"RB.1","field":"wbs_nmae","value":"Typo"}),
            "wbs_name",
        ),
        (
            json!({"op":"set_resource","resource":"PF","field":"rsrc_nmae","value":"Typo"}),
            "rsrc_name",
        ),
    ] {
        let out = dir.path().join("out.xer");
        let r = apply(&input, &out, json!([op.clone()]));
        assert_eq!(r["ok"], false, "{op} was accepted: {r}");
        let message = r["error"]["message"].as_str().unwrap();
        assert!(message.contains(needle), "{op}: {message}");
        assert!(!out.exists());
    }
    // Real columns still write.
    let out = dir.path().join("named.xer");
    let r = apply(
        &input,
        &out,
        json!([
            {"op":"set_wbs","wbs":"RB.1","field":"wbs_name","value":"Site work"},
            {"op":"set_resource","resource":"PF","field":"parent","value":null},
            {"op":"set_code_value","code_type":"Area","code":"N","field":"actv_code_name","value":"North"}
        ]),
    );
    assert_eq!(r["ok"], true, "{r}");
    let wbs = rows(&out, "PROJWBS");
    assert!(wbs.iter().any(|w| w["wbs_name"] == "Site work"));
    assert!(wbs.iter().all(|w| !w.contains_key("wbs_nmae")));
}

// B11 and B20: schedule option errors list every choice; max_float_paths
// is a positive integer from every front end.
#[test]
fn schedule_options_are_checked_the_same_from_every_front_end() {
    let dir = tempfile::tempdir().unwrap();
    let input = source(dir.path());
    let out = dir.path().join("out.xer");
    let r = run(json!({"command":"schedule","input":input,"output":out,
        "options":{"lag_calendar":"sideways"}}));
    assert_eq!(r["error"]["code"], "invalid_arguments", "{r}");
    let message = r["error"]["message"].as_str().unwrap();
    assert!(message.contains("24_hour, or project"), "{message}");
    let r = run(json!({"command":"schedule","input":input,"output":out,
        "options":{"multiple_float_paths":true,"max_float_paths":0}}));
    assert_eq!(r["error"]["code"], "invalid_arguments", "{r}");
    assert!(!out.exists());
}

// The float-path ending activity is an activity of the project: another ID
// is refused before scheduling, from the CLI and from MCP alike.
#[test]
fn a_float_path_ending_activity_the_project_does_not_hold_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let input = source(dir.path());
    let out = dir.path().join("out.xer");
    let options = r#"{"multiple_float_paths":true,"float_paths_end_activity":987654}"#;
    let r = cli_run(&[
        "schedule",
        "--input",
        input.to_str().unwrap(),
        "--output",
        out.to_str().unwrap(),
        "--options",
        options,
    ]);
    assert_eq!(r["error"]["code"], "invalid_arguments", "{r}");
    let message = r["error"]["message"].as_str().unwrap();
    assert!(message.contains("float_paths_end_activity"), "{message}");
    assert!(message.contains("987654"), "{message}");
    assert!(!out.exists());
    let mut server = Server::default();
    let reply = mcp(
        &mut server,
        "openxer_schedule",
        json!({"input":input,"output":out,
            "options":{"multiple_float_paths":true,"float_paths_end_activity":987654}}),
    );
    assert_eq!(reply["result"]["isError"], true, "{reply}");
    assert_eq!(
        reply["result"]["structuredContent"]["error"]["code"], "invalid_arguments",
        "{reply}"
    );
    assert!(!out.exists());
    // An activity of the project, or null, is accepted.
    let r = run(json!({"command":"schedule","input":input,"output":out,
        "options":{"multiple_float_paths":true,"float_paths_end_activity":1050}}));
    assert_eq!(r["ok"], true, "{r}");
    let cleared = dir.path().join("cleared.xer");
    let r = run(json!({"command":"schedule","input":input,"output":cleared,
        "options":{"multiple_float_paths":true,"float_paths_end_activity":null}}));
    assert_eq!(r["ok"], true, "{r}");
}

// B14: one set_udf with no owner is refused; a batch replaying an orphan
// value says it wrote nothing.
#[test]
fn an_ownerless_udf_value_is_refused_alone_and_reported_in_a_batch() {
    let dir = tempfile::tempdir().unwrap();
    let input = source(dir.path());
    let r = run(json!({"command":"edit","input":input,"dry_run":true,
        "operation":{"op":"set_udf","udf":"Crew Size","value":4}}));
    assert_eq!(r["ok"], false, "{r}");
    assert_eq!(r["error"]["code"], "invalid_arguments", "{r}");
    let r = dry_run(
        &input,
        json!([{"op":"set_udf","udf":"Crew Size","value":4},
               {"op":"set_notebook","activity":"RB-1030","topic":"Change Log"}]),
    );
    assert_eq!(r["ok"], true, "{r}");
    let warnings = r["result"]["report"]["warnings"].as_array().unwrap();
    assert_eq!(warnings.len(), 2, "{r}");
    assert!(warnings[0].as_str().unwrap().contains("Operation 1"), "{r}");
}

// B18: notes typed as text that start with "<" stay text.
#[test]
fn notes_starting_with_a_less_than_sign_are_text() {
    let dir = tempfile::tempdir().unwrap();
    let input = source(dir.path());
    let out = dir.path().join("notes.xer");
    let r = apply(
        &input,
        &out,
        json!([{"op":"set_feedback","activity":"RB-1030","notes":"<5 days float left"}]),
    );
    assert_eq!(r["ok"], true, "{r}");
    let notes = rows(&out, "TASKNOTE");
    assert_eq!(
        notes[0]["task_notes"],
        "<html><body>&lt;5 days float left</body></html>"
    );
    let r = dry_run(
        &input,
        json!([{"op":"set_feedback","activity":"RB-1030","notes":"<p>Hold</p>"}]),
    );
    assert_eq!(
        r["result"]["report"]["diff"]["notes_to_resources"]["added"],
        json!(["RB-1030"]),
        "{r}"
    );
}
