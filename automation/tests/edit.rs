//! `apply`, the single edit commands and their MCP tools on synthetic
//! projects (the built-in starter plus one labor resource).
use openxer_automation::{
    cli::{self, Invocation},
    dispatch,
    mcp::Server,
    parse_command, response,
};
use openxer_xer::{starter, Document, ProjectData};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const CREW: i64 = 77;

/// The starter project with a labor resource `PIPE` assigned to RB-1020.
fn project() -> ProjectData {
    let mut p = starter();
    p.document.table_mut("RSRC").add(&[
        ("rsrc_id", CREW.to_string()),
        ("rsrc_name", "Pipe crew".into()),
        ("rsrc_short_name", "PIPE".into()),
        ("rsrc_type", "RT_Labor".into()),
        ("def_qty_per_hr", "1".into()),
        ("clndr_id", "1".into()),
    ]);
    p.document.table_mut("RSRCRATE").add(&[
        ("rsrc_id", CREW.to_string()),
        ("start_date", "2020-01-01 00:00".into()),
        ("cost_per_qty", "1".into()),
    ]);
    p.add_assignment(1020, Some(CREW), None).unwrap();
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

fn changes(operations: Value) -> Value {
    json!({"format":"openxer-changes/1","operations":operations})
}

fn rows(path: &Path, table: &str) -> Vec<std::collections::BTreeMap<String, String>> {
    let doc = Document::read(path).unwrap();
    let t = doc.table(table).unwrap();
    t.rows
        .iter()
        .map(|r| t.fields.iter().cloned().zip(r.iter().cloned()).collect())
        .collect()
}

fn task(path: &Path, code: &str) -> std::collections::BTreeMap<String, String> {
    rows(path, "TASK")
        .into_iter()
        .find(|r| r["task_code"] == code)
        .unwrap_or_else(|| panic!("no {code}"))
}

fn assignment(path: &Path, task_id: &str) -> std::collections::BTreeMap<String, String> {
    rows(path, "TASKRSRC")
        .into_iter()
        .find(|r| r["task_id"] == task_id)
        .unwrap()
}

fn apply(dir: &Path, name: &str, ops: Value) -> (Value, PathBuf) {
    let input = source(dir);
    let output = dir.join(name);
    let r = run(json!({"command":"apply","input":input,"output":output,"changes":changes(ops)}));
    (r, output)
}

#[test]
fn every_operation_applies_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let (r, out) = apply(
        dir.path(),
        "all.xer",
        json!([
            {"op":"set_project","field":"data_date","value":"2026-11-09 08:00"},
            {"op":"add_wbs","wbs":"RB.3.9","name":"Tie-ins"},
            {"op":"add_activity","activity":"TI-100","name":"Tie-in 1","wbs":"RB.3.9"},
            {"op":"set_activity","activity":"TI-100","field":"original_duration","value":16},
            {"op":"set_activity","activity":"TI-100","field":"activity_type","value":"Task Dependent"},
            {"op":"set_activity","activity":"TI-100","field":"duration_type","value":"Fixed Units"},
            {"op":"set_activity","activity":"TI-100","field":"activity_priority","value":"PT_High"},
            {"op":"set_activity","activity":"TI-100","field":"primary_constraint","value":"CS_MSOA"},
            {"op":"set_activity","activity":"TI-100","field":"primary_constraint_date","value":"2026-11-20 08:00"},
            {"op":"relationship","action":"add","predecessor":"RB-1080","successor":"TI-100","kind":"SS","lag_hours":8},
            {"op":"relationship","action":"edit","predecessor":"RB-1080","successor":"TI-100","kind":"SS","lag_hours":4},
            {"op":"relationship","action":"remove","predecessor":"RB-1000","successor":"RB-1010","kind":"FS"},
            {"op":"set_activity","activity":"RB-1020","field":"actual_start","value":"2026-11-02 08:00"},
            {"op":"set_activity","activity":"RB-1020","field":"remaining_duration","value":20},
            {"op":"set_activity","activity":"RB-1020","field":"physical_percent_complete","value":40},
            {"op":"set_assignment","activity":"RB-1020","resource":"PIPE","field":"actual_regular_units","value":30},
            {"op":"set_assignment","activity":"RB-1020","resource":"PIPE","field":"remaining_units","value":20},
            {"op":"assign_resource","activity":"RB-1050","resource":"PIPE"},
            {"op":"remove_assignment","activity":"RB-1050","resource":"PIPE"},
            {"op":"calendar_day","calendar":"Standard 5-day / 8-hour","date":"2026-11-26","shifts":[]},
            {"op":"calendar_weekday","calendar":"Standard 5-day / 8-hour","weekday":6,"shifts":[["08:00","12:00"]]},
            {"op":"delete_activity","activity":"RB-1130"},
            {"op":"schedule","level_resources":false}
        ]),
    );
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["result"]["report"]["operations"], 23);
    let t = task(&out, "TI-100");
    assert_eq!(t["task_name"], "Tie-in 1");
    assert_eq!(t["target_drtn_hr_cnt"], "16");
    assert_eq!(t["duration_type"], "DT_FixedQty");
    assert_eq!(t["priority_type"], "PT_High");
    assert_eq!(t["cstr_type"], "CS_MSOA");
    assert_eq!(t["cstr_date"], "2026-11-20 08:00");
    assert!(!t["early_start_date"].is_empty(), "scheduled");
    let w = rows(&out, "PROJWBS");
    let tie = w.iter().find(|r| r["wbs_short_name"] == "9").unwrap();
    assert_eq!(tie["wbs_name"], "Tie-ins");
    assert_eq!(t["wbs_id"], tie["wbs_id"]);
    let b = task(&out, "RB-1020");
    assert_eq!(b["status_code"], "TK_Active");
    assert_eq!(b["act_start_date"], "2026-11-02 08:00");
    assert_eq!(b["remain_drtn_hr_cnt"], "20");
    assert_eq!(b["phys_complete_pct"], "40");
    let a = assignment(&out, &b["task_id"]);
    assert_eq!(a["act_reg_qty"], "30");
    assert_eq!(a["remain_qty"], "20");
    assert_eq!(a["remain_qty_per_hr"], "1", "20 units over 20 hours");
    let links = rows(&out, "TASKPRED");
    assert!(links
        .iter()
        .any(|l| l["pred_type"] == "PR_SS" && l["lag_hr_cnt"] == "4"));
    assert!(!links
        .iter()
        .any(|l| l["pred_task_id"] == "1000" && l["task_id"] == "1010"));
    assert!(rows(&out, "TASK")
        .iter()
        .all(|r| r["task_code"] != "RB-1130"));
    let tasks_with_crew = rows(&out, "TASKRSRC").len();
    assert_eq!(tasks_with_crew, 1, "RB-1050's assignment was removed");
    let cal = &rows(&out, "CALENDAR")[0]["clndr_data"];
    assert!(cal.contains("46352"), "2026-11-26 is an exception: {cal}");
    let diff = &r["result"]["report"]["diff"];
    assert!(diff["activities"]["added"]
        .as_array()
        .unwrap()
        .contains(&json!("TI-100")));
    assert_eq!(
        diff["activities"]["changed"]["RB-1020"]["status_code"]["after"],
        "TK_Active"
    );
}

#[test]
fn a_failing_operation_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (r, out) = apply(
        dir.path(),
        "never.xer",
        json!([
            {"op":"set_activity","activity":"RB-1020","field":"remaining_duration","value":10},
            {"op":"set_activity","activity":"NOPE","field":"remaining_duration","value":10},
            {"op":"schedule"}
        ]),
    );
    assert_eq!(r["ok"], false);
    assert_eq!(r["error"]["code"], "operation_failed");
    let message = r["error"]["message"].as_str().unwrap();
    assert!(
        message.starts_with("Operation 2 (set_activity NOPE"),
        "{message}"
    );
    assert!(!out.exists());
    // Unknown fields, enum values and operations are refused the same way.
    for op in [
        json!({"op":"set_activity","activity":"RB-1020","field":"colour","value":1}),
        json!({"op":"set_activity","activity":"RB-1020","field":"duration_type","value":"DT_Nope"}),
        json!({"op":"set_assignment","activity":"RB-1020","resource":"NOBODY","field":"remaining_units","value":1}),
        json!({"op":"relationship","action":"edit","predecessor":"RB-1000","successor":"RB-1020","kind":"FS","lag_hours":1}),
        json!({"op":"calendar_day","calendar":"Standard 5-day / 8-hour","date":"2026-11-26","shifts":[["08:00","12:00"],["11:00","13:00"]]}),
    ] {
        let (r, out) = apply(dir.path(), "never2.xer", json!([op]));
        assert_eq!(r["ok"], false, "{op}");
        assert!(!out.exists());
    }
    let (r, _) = apply(dir.path(), "never3.xer", json!([{"op":"teleport"}]));
    assert_eq!(r["error"]["code"], "invalid_changes");
}

#[test]
fn order_matters_and_a_primary_resource_waits_for_its_assignment() {
    let dir = tempfile::tempdir().unwrap();
    // Finishing first makes a later remaining duration of zero no change;
    // a nonzero one is refused on the completed activity.
    let (r, _) = apply(
        dir.path(),
        "a.xer",
        json!([
            {"op":"set_activity","activity":"RB-1020","field":"actual_start","value":"2026-10-26 08:00"},
            {"op":"set_activity","activity":"RB-1020","field":"actual_finish","value":"2026-10-30 17:00"},
            {"op":"set_activity","activity":"RB-1020","field":"remaining_duration","value":0}
        ]),
    );
    assert_eq!(r["ok"], true, "{r}");
    let (r, _) = apply(
        dir.path(),
        "b.xer",
        json!([
            {"op":"set_activity","activity":"RB-1020","field":"actual_start","value":"2026-10-26 08:00"},
            {"op":"set_activity","activity":"RB-1020","field":"actual_finish","value":"2026-10-30 17:00"},
            {"op":"set_activity","activity":"RB-1020","field":"remaining_duration","value":8}
        ]),
    );
    assert_eq!(r["ok"], false);
    let (r, out) = apply(
        dir.path(),
        "c.xer",
        json!([
            {"op":"set_activity","activity":"RB-1050","field":"primary_resource","value":"PIPE"},
            {"op":"assign_resource","activity":"RB-1050","resource":"PIPE"}
        ]),
    );
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(task(&out, "RB-1050")["rsrc_id"], CREW.to_string());
}

#[test]
fn choices_accept_codes_and_names() {
    let dir = tempfile::tempdir().unwrap();
    for (n, (field, code, name)) in [
        ("activity_type", "TT_FinMile", "Finish Milestone"),
        ("percent_complete_type", "CP_Phys", "Physical"),
        ("duration_type", "DT_FixedRate", "Fixed Units/Time"),
        ("activity_priority", "PT_Low", "Low"),
        ("primary_constraint", "CS_ALAP", "As Late As Possible"),
    ]
    .into_iter()
    .enumerate()
    {
        let mut values = vec![];
        for (k, value) in [code, name].into_iter().enumerate() {
            let (r, out) = apply(
                dir.path(),
                &format!("{n}-{k}.xer"),
                json!([{"op":"set_activity","activity":"RB-1050","field":field,"value":value}]),
            );
            assert_eq!(r["ok"], true, "{field} {value}: {r}");
            values.push(task(&out, "RB-1050"));
        }
        assert_eq!(values[0], values[1], "{field}");
    }
}

#[test]
fn exact_mode_writes_values_as_given_and_rules_mode_recalculates() {
    let dir = tempfile::tempdir().unwrap();
    let input = source(dir.path());
    let ops = json!([
        {"op":"set_activity","activity":"RB-1020","field":"actual_start","value":"2026-10-26 08:00"},
        {"op":"set_activity","activity":"RB-1020","field":"remaining_duration","value":30},
        {"op":"set_activity","activity":"RB-1020","field":"actual_labor_units","value":12.5},
        {"op":"set_assignment","activity":"RB-1020","resource":"PIPE","field":"remaining_units","value":7.25},
        {"op":"set_assignment","activity":"RB-1020","resource":"PIPE","field":"remaining_units_per_hour","value":3.5},
        {"op":"set_assignment","activity":"RB-1020","resource":"PIPE","field":"remaining_cost","value":99.99},
        {"op":"set_assignment","activity":"RB-1020","resource":"PIPE","field":"act_reg_qty","value":12.5}
    ]);
    let exact = dir.path().join("exact.xer");
    let mut doc = changes(ops.clone());
    doc["recalculate"] = json!(false);
    let r = run(json!({"command":"apply","input":input,"output":exact,"changes":doc}));
    assert_eq!(r["ok"], true, "{r}");
    let t = task(&exact, "RB-1020");
    assert_eq!(t["act_start_date"], "2026-10-26 08:00");
    assert_eq!(t["remain_drtn_hr_cnt"], "30");
    assert_eq!(t["act_work_qty"], "12.5");
    let a = assignment(&exact, "1020");
    assert_eq!(a["remain_qty"], "7.25");
    assert_eq!(a["remain_qty_per_hr"], "3.5");
    assert_eq!(a["remain_cost"], "99.99");
    assert_eq!(a["act_reg_qty"], "12.5");
    // The derived fields only exist in exact mode (actual_labor_units is a
    // Status tab field the rules also take, so it is left out here).
    let rules = dir.path().join("rules.xer");
    let derived: Vec<Value> = ops
        .as_array()
        .unwrap()
        .iter()
        .filter(|o| o["field"] != "actual_labor_units")
        .cloned()
        .collect();
    let r = run(
        json!({"command":"apply","input":input,"output":rules,"changes":changes(Value::Array(derived))}),
    );
    assert_eq!(r["ok"], false);
    assert!(!rules.exists());
    // With the rules, units over the remaining duration give the rate.
    let r = run(
        json!({"command":"apply","input":input,"output":rules,"changes":changes(json!([
            {"op":"set_activity","activity":"RB-1020","field":"actual_start","value":"2026-10-26 08:00"},
            {"op":"set_activity","activity":"RB-1020","field":"remaining_duration","value":30},
            {"op":"set_assignment","activity":"RB-1020","resource":"PIPE","field":"remaining_units","value":15}
        ]))}),
    );
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(assignment(&rules, "1020")["remain_qty_per_hr"], "0.5");
    // The batch default yields to a per-operation recalculate and to the
    // command's own.
    let mixed = dir.path().join("mixed.xer");
    let r = run(
        json!({"command":"apply","input":input,"output":mixed,"recalculate":false,"changes":changes(json!([
            {"op":"set_assignment","activity":"RB-1020","resource":"PIPE","field":"remaining_units_per_hour","value":2.75}
        ]))}),
    );
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(assignment(&mixed, "1020")["remain_qty_per_hr"], "2.75");
}

#[test]
fn dry_run_reports_the_diff_and_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let input = source(dir.path());
    let r = run(json!({"command":"edit","input":input,"dry_run":true,
        "operation":{"op":"set_activity","activity":"RB-1050","field":"original_duration","value":12}}));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["result"]["output"], Value::Null);
    let change = &r["result"]["report"]["diff"]["activities"]["changed"]["RB-1050"];
    assert_eq!(change["target_drtn_hr_cnt"]["after"], "12");
    assert_eq!(change["remain_drtn_hr_cnt"]["after"], "12");
    let names: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
    assert_eq!(names.len(), 1, "only the source exists");
    let both = run(
        json!({"command":"edit","input":input,"dry_run":true,"output":dir.path().join("x.xer"),
        "operation":{"op":"delete_activity","activity":"RB-1050"}}),
    );
    assert_eq!(both["error"]["code"], "invalid_arguments");
}

/// A written file's text with the clock stamps and generated GUIDs of new
/// rows blanked.
fn unstamped(path: &Path) -> String {
    let mut doc = Document::read(path).unwrap();
    for t in &mut doc.tables {
        for column in ["create_date", "update_date", "last_chng_date", "guid"] {
            if let Some(c) = t.field(column) {
                for r in &mut t.rows {
                    if let Some(v) = r.get_mut(c) {
                        v.clear();
                    }
                }
            }
        }
    }
    doc.text()
}

fn without_paths(mut v: Value) -> Value {
    v["result"]["output"] = Value::Null;
    v
}

#[test]
fn cli_and_mcp_build_the_same_edits() {
    let dir = tempfile::tempdir().unwrap();
    let input = source(dir.path());
    let input = input.to_str().unwrap();
    let mut server = Server::default();
    let meta = json!({"io.modelcontextprotocol/protocolVersion":"2026-07-28",
        "io.modelcontextprotocol/clientCapabilities":{}});
    let cases: Vec<(Vec<String>, &str, Value)> = vec![
        (
            vec![
                "set-activity",
                "--activity",
                "RB-1020",
                "--field",
                "remaining_duration",
                "--value",
                "36",
            ],
            "openxer_set_activity",
            json!({"activity":"RB-1020","field":"remaining_duration","value":36}),
        ),
        (
            vec![
                "add-activity",
                "--activity",
                "N-1",
                "--name",
                "New work",
                "--wbs",
                "RB.2",
            ],
            "openxer_add_activity",
            json!({"activity":"N-1","name":"New work","wbs":"RB.2"}),
        ),
        (
            vec!["delete-activity", "--activity", "RB-1130"],
            "openxer_delete_activity",
            json!({"activity":"RB-1130"}),
        ),
        (
            vec![
                "dissolve-activity",
                "--activity",
                "RB-1050",
                "--retain-lag",
                "true",
            ],
            "openxer_dissolve_activity",
            json!({"activity":"RB-1050","retain_lag":true}),
        ),
        (
            vec!["assign", "--activity", "RB-1050", "--resource", "PIPE"],
            "openxer_assign",
            json!({"activity":"RB-1050","resource":"PIPE"}),
        ),
        (
            vec!["unassign", "--activity", "RB-1020", "--resource", "PIPE"],
            "openxer_unassign",
            json!({"activity":"RB-1020","resource":"PIPE"}),
        ),
        (
            vec![
                "set-assignment",
                "--activity",
                "RB-1020",
                "--resource",
                "PIPE",
                "--field",
                "budgeted_units",
                "--value",
                "60",
            ],
            "openxer_set_assignment",
            json!({"activity":"RB-1020","resource":"PIPE","field":"budgeted_units","value":60}),
        ),
        (
            vec![
                "calendar-day",
                "--calendar",
                "Standard 5-day / 8-hour",
                "--date",
                "2026-11-26",
                "--shifts",
                "nonwork",
            ],
            "openxer_calendar_day",
            json!({"calendar":"Standard 5-day / 8-hour","date":"2026-11-26","shifts":[]}),
        ),
        (
            vec![
                "calendar-weekday",
                "--calendar",
                "Standard 5-day / 8-hour",
                "--weekday",
                "6",
                "--shifts",
                r#"[["08:00","12:00"]]"#,
            ],
            "openxer_calendar_weekday",
            json!({"calendar":"Standard 5-day / 8-hour","weekday":6,"shifts":[["08:00","12:00"]]}),
        ),
        (
            vec![
                "set-project",
                "--field",
                "data_date",
                "--value",
                "2026-11-09 08:00",
            ],
            "openxer_set_project",
            json!({"field":"data_date","value":"2026-11-09 08:00"}),
        ),
        (
            vec!["add-wbs", "--wbs", "RB.4.1", "--name", "Punch list"],
            "openxer_add_wbs",
            json!({"wbs":"RB.4.1","name":"Punch list"}),
        ),
    ]
    .into_iter()
    .map(|(a, t, v)| (a.into_iter().map(String::from).collect(), t, v))
    .collect();
    for (n, (args, tool, fields)) in cases.into_iter().enumerate() {
        let cli_out = dir.path().join(format!("cli-{n}.xer"));
        let mut argv = args.clone();
        argv.extend([
            "--input".into(),
            input.into(),
            "--output".into(),
            cli_out.to_str().unwrap().into(),
        ]);
        let Invocation::Run(command) = cli::parse(argv).unwrap() else {
            panic!("{args:?}")
        };
        let via_cli = response(dispatch(*command));
        assert_eq!(via_cli["ok"], true, "{args:?}: {via_cli}");
        let mcp_out = dir.path().join(format!("mcp-{n}.xer"));
        let mut arguments = fields.clone();
        arguments["input"] = json!(input);
        arguments["output"] = json!(mcp_out);
        let reply = server
            .handle(json!({"jsonrpc":"2.0","id":n,"method":"tools/call",
                "params":{"_meta":meta,"name":tool,"arguments":arguments}}))
            .unwrap();
        let via_mcp = reply["result"]["structuredContent"].clone();
        assert_eq!(via_mcp["ok"], true, "{tool}: {reply}");
        assert_eq!(without_paths(via_cli), without_paths(via_mcp), "{tool}");
        assert_eq!(
            unstamped(&cli_out),
            unstamped(&mcp_out),
            "{tool} writes the same file"
        );
    }
    // apply from a file on the command line equals openxer_apply.
    let file = dir.path().join("changes.json");
    let batch = changes(json!([
        {"op":"set_activity","activity":"RB-1020","field":"original_duration","value":40},
        {"op":"schedule","level_resources":false}
    ]));
    std::fs::write(&file, batch.to_string()).unwrap();
    let Invocation::Run(command) = cli::parse([
        "apply",
        "--input",
        input,
        "--changes",
        file.to_str().unwrap(),
        "--dry-run",
    ])
    .unwrap() else {
        panic!()
    };
    let via_cli = response(dispatch(*command));
    let reply = server
        .handle(json!({"jsonrpc":"2.0","id":99,"method":"tools/call",
            "params":{"_meta":meta,"name":"openxer_apply","arguments":{"input":input,"changes":batch,"dry_run":true}}}))
        .unwrap();
    assert_eq!(via_cli, reply["result"]["structuredContent"]);
    assert_eq!(via_cli["ok"], true, "{via_cli}");
}

/// The starter plus an activity code type, a UDF on activities and on
/// resources, and a notebook topic.
fn dictionary_project() -> ProjectData {
    let mut p = project();
    let doc = &mut p.document;
    doc.table_mut("ACTVTYPE").add(&[
        ("actv_code_type_id", "5".into()),
        ("actv_code_type", "Priority".into()),
        ("proj_id", "1".into()),
        ("seq_num", "1".into()),
    ]);
    doc.table_mut("ACTVCODE").add(&[
        ("actv_code_id", "51".into()),
        ("actv_code_type_id", "5".into()),
        ("short_name", "P1".into()),
        ("seq_num", "1".into()),
    ]);
    for (id, table, label, kind) in [
        ("8", "TASK", "Crew Size", "FT_INT"),
        ("9", "RSRC", "Craft", "FT_TEXT"),
    ] {
        doc.table_mut("UDFTYPE").add(&[
            ("udf_type_id", id.into()),
            ("table_name", table.into()),
            ("udf_type_name", format!("user_field_{id}")),
            ("udf_type_label", label.into()),
            ("logical_data_type", kind.into()),
        ]);
    }
    p
}

/// `set_udf` reaches every subject area: an assignment by activity and
/// resource, a step by activity and step, a WBS element, the project and
/// the register records by name; a record named twice is refused.
#[test]
fn set_udf_writes_values_on_every_subject_area() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("areas.xer");
    let mut p = project();
    let areas = [
        ("20", "TASKRSRC", "Bill Rate", "FT_MONEY"),
        ("21", "TASKPROC", "Inspector", "FT_TEXT"),
        ("22", "PROJWBS", "Area Lead", "FT_TEXT"),
        ("23", "PROJECT", "Contract", "FT_TEXT"),
        ("24", "PROJCOST", "PO Number", "FT_INT"),
        ("25", "PROJISSU", "Raised On", "FT_START_DATE"),
        ("26", "PROJRISK", "Rating", "FT_STATICTYPE"),
        ("27", "DOCUMENT", "Sheet Count", "FT_FLOAT_2_DECIMALS"),
    ];
    for (id, table, label, kind) in areas {
        p.document.table_mut("UDFTYPE").add(&[
            ("udf_type_id", id.into()),
            ("table_name", table.into()),
            ("udf_type_name", format!("user_field_{id}")),
            ("udf_type_label", label.into()),
            ("logical_data_type", kind.into()),
        ]);
    }
    p.updated_document().write(&input).unwrap();
    let output = dir.path().join("areas-out.xer");
    let doc = changes(json!([
        {"op":"add_step","activity":"RB-1030","name":"Pour"},
        {"op":"add_expense","activity":"RB-1030","name":"Crane rental"},
        {"op":"add_issue","name":"Late steel"},
        {"op":"add_risk","name":"Rain delay"},
        {"op":"add_document","title":"Pump datasheet","reference":"DS-100"},
        {"op":"set_udf","activity":"RB-1020","resource":"PIPE","udf":"Bill Rate","value":85.5},
        {"op":"set_udf","activity":"RB-1030","step":"Pour","udf":"Inspector","value":"J. Ortiz"},
        {"op":"set_udf","wbs":"RB.1","udf":"Area Lead","value":"North"},
        {"op":"set_udf","project":true,"udf":"Contract","value":"C-77"},
        {"op":"set_udf","expense":"Crane rental","udf":"PO Number","value":4410},
        {"op":"set_udf","issue":"Late steel","udf":"Raised On","value":"2026-11-02 08:00"},
        {"op":"set_udf","risk":"Rain delay","udf":"Rating","value":"Yellow"},
        {"op":"set_udf","document":"DS-100","udf":"Sheet Count","value":12.5}
    ]));
    let r = run(json!({"command":"apply","input":input,"output":output,"changes":doc}));
    assert_eq!(r["ok"], true, "{r}");
    let values = rows(&output, "UDFVALUE");
    let value = |ty: &str, column: &str| -> [String; 3] {
        let v = values
            .iter()
            .find(|v| v["udf_type_id"] == ty)
            .unwrap_or_else(|| panic!("no value of {ty}"));
        ["fk_id", "proj_id", column].map(|c| v[c].clone())
    };
    let key = |table: &str, column: &str, field: &str, wanted: &str| {
        rows(&output, table)
            .into_iter()
            .find(|r| r[field] == wanted)
            .unwrap_or_else(|| panic!("no {wanted} in {table}"))[column]
            .clone()
    };
    let assignment = assignment(&output, "1020")["taskrsrc_id"].clone();
    let want = [assignment, "1".into(), "85.5".into()];
    assert_eq!(value("20", "udf_number"), want);
    let step = key("TASKPROC", "proc_id", "proc_name", "Pour");
    let want = [step, "1".into(), "J. Ortiz".into()];
    assert_eq!(value("21", "udf_text"), want);
    let [wbs, _, text] = value("22", "udf_text");
    assert_eq!(text, "North");
    assert!(rows(&output, "PROJWBS")
        .iter()
        .any(|w| w["wbs_id"] == wbs && w["proj_node_flag"] != "Y"));
    assert_eq!(value("23", "udf_text"), ["1", "1", "C-77"]);
    let cost = key("PROJCOST", "cost_item_id", "cost_name", "Crane rental");
    assert_eq!(value("24", "udf_number"), [cost, "1".into(), "4410".into()]);
    let issue = key("PROJISSU", "issue_id", "issue_name", "Late steel");
    let [fk, _, date] = value("25", "udf_date");
    assert_eq!(fk, issue);
    assert!(date.starts_with("2026-11-02"), "{date}");
    let risk = key("PROJRISK", "risk_id", "risk_name", "Rain delay");
    assert_eq!(value("26", "udf_text"), [risk, "1".into(), "Yellow".into()]);
    let doc = key("DOCUMENT", "doc_id", "doc_short_name", "DS-100");
    assert_eq!(value("27", "udf_number"), [doc, "1".into(), "12.5".into()]);
    // One record only.
    let two = changes(json!([
        {"op":"set_udf","wbs":"RB.1","project":true,"udf":"Area Lead","value":"x"}
    ]));
    let output = dir.path().join("two.xer");
    let r = run(json!({"command":"apply","input":input,"output":output,"changes":two}));
    assert_eq!(r["ok"], false, "{r}");
}

#[test]
fn meta_is_kept_out_of_operations_and_unknown_fields_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let (r, out) = apply(
        dir.path(),
        "meta.xer",
        json!([{"op":"set_activity","activity":"RB-1050","field":"original_duration","value":12,
                "meta":{"source":"xer","evidence":"Change Log 10-13","written_by":"script","intermediate":true}}]),
    );
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(task(&out, "RB-1050")["target_drtn_hr_cnt"], "12");
    let (r, _) = apply(
        dir.path(),
        "bad.xer",
        json!([{"op":"set_activity","activity":"RB-1050","field":"original_duration","value":12,"source":"xer"}]),
    );
    assert_eq!(r["ok"], false);
    assert!(
        r["error"]["message"].as_str().unwrap().contains("source"),
        "{r}"
    );
    let (r, _) = apply(
        dir.path(),
        "bad2.xer",
        json!([{"op":"set_activity","activity":"RB-1050","field":"original_duration","value":12,"meta":"text"}]),
    );
    assert_eq!(r["ok"], false);
    // The single edit command takes meta too.
    let input = source(dir.path());
    let r = run(json!({"command":"edit","input":input,"dry_run":true,
        "operation":{"op":"delete_activity","activity":"RB-1050","meta":{"basis":"test"}}}));
    assert_eq!(r["ok"], true, "{r}");
}

#[test]
fn record_edits_write_codes_limits_notebooks_udfs_and_dictionaries() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("dict.xer");
    dictionary_project()
        .updated_document()
        .write(&input)
        .unwrap();
    let output = dir.path().join("dict-out.xer");
    let mut doc = changes(json!([
        {"op":"add_code_value","code_type":"Priority","code":"P2"},
        {"op":"set_code_value","code_type":"Priority","code":"P2","field":"seq_num","value":2},
        {"op":"set_code_value","code_type":"Priority","code":"P2","field":"actv_code_name","value":"Second"},
        {"op":"set_activity_code","activity":"RB-1020","code_type":"Priority","value":"P2"},
        {"op":"set_activity_code","activity":"RB-1030","code_type":"Priority","value":"P1"},
        {"op":"set_activity_code","activity":"RB-1030","code_type":"Priority","value":null},
        {"op":"delete_code_value","code_type":"Priority","code":"P1"},
        {"op":"set_resource_limit","resource":"PIPE","from":"2026-11-10 00:00","max_units_per_hour":2,"price_per_unit":90},
        {"op":"set_resource_limit","resource":"PIPE","from":"2020-01-01 00:00","max_units_per_hour":null},
        {"op":"set_notebook","activity":"RB-1020","topic":"Change Log","text":"<p>moved</p>"},
        {"op":"set_notebook","activity":"RB-1020","topic":"Change Log","text":"<p>moved again</p>"},
        {"op":"set_wbs_notebook","wbs":"RB.1","topic":"Change Log","text":"<p>wbs note</p>"},
        {"op":"set_udf","activity":"RB-1020","udf":"Crew Size","value":4},
        {"op":"set_udf","resource":"PIPE","udf":"Craft","value":"Pipefitter"},
        {"op":"add_calendar","calendar":"Night 7x12"},
        {"op":"set_calendar","calendar":"Night 7x12","field":"day_hr_cnt","value":12},
        {"op":"calendar_weekday","calendar":"Night 7x12","weekday":1,"shifts":[["19:00","07:00"]]},
        {"op":"add_resource","resource":"WELD"},
        {"op":"set_resource","resource":"WELD","field":"rsrc_name","value":"Welders"},
        {"op":"set_resource","resource":"WELD","field":"calendar","value":"Night 7x12"},
        {"op":"set_wbs","wbs":"RB.2","field":"wbs_name","value":"Procurement and expediting"},
        {"op":"set_schedule_option","field":"level_within_float_flag","value":"Y"},
        {"op":"set_project","field":"def_duration_type","value":"DT_FixedDUR2"},
        {"op":"relationship","action":"add","predecessor":"RB-1050","successor":"RB-1060","kind":"FS","lag_hours":0,"comments":"tie-in"},
        {"op":"relationship","action":"edit","predecessor":"RB-1050","successor":"RB-1060","kind":"FS","comments":"tie-in moved"},
        {"op":"schedule","level_resources":true}
    ]));
    doc["recalculate"] = json!(false);
    let r = run(json!({"command":"apply","input":input,"output":output,"changes":doc}));
    assert_eq!(r["ok"], true, "{r}");
    let codes = rows(&output, "ACTVCODE");
    assert_eq!(codes.len(), 1, "P1 deleted, P2 added");
    assert_eq!(codes[0]["short_name"], "P2");
    assert_eq!(codes[0]["seq_num"], "2");
    assert_eq!(codes[0]["actv_code_name"], "Second");
    let assigned = rows(&output, "TASKACTV");
    assert_eq!(assigned.len(), 1);
    assert_eq!(assigned[0]["task_id"], "1020");
    assert_eq!(assigned[0]["actv_code_id"], codes[0]["actv_code_id"]);
    let limits = rows(&output, "RSRCRATE");
    assert_eq!(limits.len(), 1);
    assert_eq!(limits[0]["max_qty_per_hr"], "2");
    assert_eq!(limits[0]["cost_per_qty"], "90");
    let memo = rows(&output, "TASKMEMO");
    assert_eq!(memo.len(), 1);
    assert_eq!(memo[0]["task_memo"], "<p>moved again</p>");
    assert_eq!(rows(&output, "WBSMEMO")[0]["wbs_memo"], "<p>wbs note</p>");
    let udf = rows(&output, "UDFVALUE");
    assert!(udf
        .iter()
        .any(|u| u["fk_id"] == "1020" && u["udf_number"] == "4"));
    assert!(udf
        .iter()
        .any(|u| u["fk_id"] == CREW.to_string() && u["udf_text"] == "Pipefitter"));
    let cal = rows(&output, "CALENDAR")
        .into_iter()
        .find(|c| c["clndr_name"] == "Night 7x12")
        .unwrap();
    assert_eq!(cal["day_hr_cnt"], "12");
    let weld = rows(&output, "RSRC")
        .into_iter()
        .find(|r| r["rsrc_short_name"] == "WELD")
        .unwrap();
    assert_eq!(weld["rsrc_name"], "Welders");
    assert_eq!(weld["clndr_id"], cal["clndr_id"]);
    assert!(rows(&output, "PROJWBS")
        .iter()
        .any(|w| w["wbs_name"] == "Procurement and expediting"));
    assert_eq!(
        rows(&output, "PROJECT")[0]["def_duration_type"],
        "DT_FixedDUR2"
    );
    let link = rows(&output, "TASKPRED")
        .into_iter()
        .find(|l| l["pred_task_id"] == "1050" && l["task_id"] == "1060")
        .unwrap();
    assert_eq!(link["comments"], "tie-in moved");
    // A record edit that names nothing known writes nothing.
    let none = dir.path().join("none.xer");
    let mut bad = changes(json!([
        {"op":"set_activity_code","activity":"RB-1020","code_type":"Priority","value":"P9"}
    ]));
    bad["recalculate"] = json!(false);
    let r = run(json!({"command":"apply","input":input,"output":none,"changes":bad}));
    assert_eq!(r["ok"], false);
    assert!(!none.exists());
}

#[test]
fn add_code_type_builds_codes_from_nothing_and_assigns_them() {
    let dir = tempfile::tempdir().unwrap();
    // The project's own WBS node sits under EPS node 900.
    let input = dir.path().join("eps.xer");
    let mut doc = project().updated_document();
    let pid = project().model.project.proj_id.to_string();
    doc.table_mut("PROJWBS").add(&[
        ("wbs_id", "9001".into()),
        ("proj_id", pid),
        ("proj_node_flag", "Y".into()),
        ("parent_wbs_id", "900".into()),
        ("wbs_short_name", "RB".into()),
        ("wbs_name", "Riverbend".into()),
    ]);
    doc.write(&input).unwrap();
    let output = dir.path().join("codes.xer");
    let ops = json!([
            {"op":"add_code_type","code_type":"Area","max_length":10},
            {"op":"add_code_type","code_type":"Phase","scope":"global"},
            {"op":"add_code_type","code_type":"Region","scope":"eps"},
            {"op":"add_code_value","code_type":"Area","code":"North"},
            {"op":"set_activity_code","activity":"RB-1020","code_type":"Area","value":"North"}
    ]);
    let r = run(json!({"command":"apply","input":input,"output":output,"changes":changes(ops)}));
    assert_eq!(r["ok"], true, "{r}");
    let types = rows(&output, "ACTVTYPE");
    let area = types
        .iter()
        .find(|t| t["actv_code_type"] == "Area")
        .unwrap();
    assert_eq!(area["actv_code_type_scope"], "AS_Project");
    assert_eq!(area["proj_id"], rows(&output, "PROJECT")[0]["proj_id"]);
    assert_eq!(area["actv_short_len"], "10");
    let phase = types
        .iter()
        .find(|t| t["actv_code_type"] == "Phase")
        .unwrap();
    assert_eq!(phase["actv_code_type_scope"], "AS_Global");
    assert_eq!(phase["proj_id"], "");
    assert_eq!(phase["wbs_id"], "");
    assert_eq!(area["wbs_id"], "");
    // An EPS code sits on the project's EPS node, the parent of its own node.
    let region = types
        .iter()
        .find(|t| t["actv_code_type"] == "Region")
        .unwrap();
    assert_eq!(region["actv_code_type_scope"], "AS_EPS");
    assert_eq!(region["proj_id"], "");
    assert_eq!(region["wbs_id"], "900");
    let value = rows(&output, "ACTVCODE")
        .into_iter()
        .find(|c| c["short_name"] == "North")
        .unwrap();
    assert_eq!(value["actv_code_type_id"], area["actv_code_type_id"]);
    let assigned = rows(&output, "TASKACTV");
    assert!(assigned
        .iter()
        .any(|a| a["task_id"] == "1020" && a["actv_code_id"] == value["actv_code_id"]));
    // A duplicate name, an unknown scope or a bad length writes nothing.
    for (k, op) in [
        json!({"op":"add_code_type","code_type":"area"}),
        json!({"op":"add_code_type","code_type":"Zone","scope":"portfolio"}),
        json!({"op":"add_code_type","code_type":"Zone","max_length":0}),
    ]
    .into_iter()
    .enumerate()
    {
        let input = output.clone();
        let out = dir.path().join(format!("bad-{k}.xer"));
        let r = run(
            json!({"command":"apply","input":input,"output":out,"changes":changes(json!([op]))}),
        );
        assert_eq!(r["ok"], false, "{k}: {r}");
        assert!(!out.exists());
    }
}

/// The starter with two funding sources in the dictionary.
fn funded_source(dir: &Path) -> PathBuf {
    let mut p = project();
    for (id, name) in [("7", "Capital"), ("9", "Operating Grant")] {
        p.document
            .table_mut("FUNDSRC")
            .add(&[("fund_id", id.into()), ("fund_name", name.into())]);
    }
    let path = dir.join("funded.xer");
    p.updated_document().write(&path).unwrap();
    path
}

#[test]
fn project_funding_is_added_changed_and_deleted_by_source_name() {
    let dir = tempfile::tempdir().unwrap();
    let input = funded_source(dir.path());
    let output = dir.path().join("funding.xer");
    let ops = json!([
        {"op":"add_project_funding","source":"Capital","amount":250000,"share":40},
        {"op":"add_project_funding","source":"Operating Grant"},
        {"op":"set_project_funding","source":"Operating Grant","field":"amount","value":"75,000.00"},
        {"op":"set_project_funding","source":"Operating Grant","field":"share","value":"60%"},
        {"op":"set_project_funding","source":"Capital","field":"amount","value":300000}
    ]);
    let r = run(json!({"command":"apply","input":input,"output":output,"changes":changes(ops)}));
    assert_eq!(r["ok"], true, "{r}");
    let pid = rows(&output, "PROJECT")[0]["proj_id"].clone();
    let funds = rows(&output, "PROJFUND");
    assert_eq!(funds.len(), 2, "{funds:?}");
    let capital = funds.iter().find(|f| f["fund_id"] == "7").unwrap();
    assert_eq!(capital["proj_id"], pid);
    assert_eq!(capital["fund_cost"], "300000");
    assert_eq!(capital["fund_wt"], "40");
    let grant = funds.iter().find(|f| f["fund_id"] == "9").unwrap();
    assert_eq!(grant["fund_cost"], "75000");
    assert_eq!(grant["fund_wt"], "60");
    assert_ne!(capital["proj_fund_id"], grant["proj_fund_id"]);
    // Move the grant row to another source name, then delete Capital: the
    // file read back has one row, for the grant amount under Capital.
    let moved = dir.path().join("moved.xer");
    let ops = json!([
        {"op":"delete_project_funding","source":"Capital"},
        {"op":"set_project_funding","source":"Operating Grant","field":"source","value":"Capital"}
    ]);
    let r = run(json!({"command":"apply","input":output,"output":moved,"changes":changes(ops)}));
    assert_eq!(r["ok"], true, "{r}");
    let funds = rows(&moved, "PROJFUND");
    assert_eq!(funds.len(), 1);
    assert_eq!(funds[0]["fund_id"], "7");
    assert_eq!(funds[0]["fund_cost"], "75000");
    // Refused, writing nothing: an unknown source, a source listed twice, a
    // share over 100, a negative amount, an unknown field, a source the
    // project is not funded by.
    for (k, op) in [
        json!({"op":"add_project_funding","source":"Nope"}),
        json!({"op":"add_project_funding","source":"Capital"}),
        json!({"op":"set_project_funding","source":"Capital","field":"share","value":120}),
        json!({"op":"set_project_funding","source":"Capital","field":"amount","value":-1}),
        json!({"op":"set_project_funding","source":"Capital","field":"color","value":1}),
        json!({"op":"delete_project_funding","source":"Operating Grant"}),
    ]
    .into_iter()
    .enumerate()
    {
        let out = dir.path().join(format!("bad-{k}.xer"));
        let r = run(
            json!({"command":"apply","input":moved,"output":out,"changes":changes(json!([op]))}),
        );
        assert_eq!(r["ok"], false, "{k}: {r}");
        assert!(!out.exists());
    }
}

#[test]
fn project_funding_applies_the_same_from_the_command_line_and_mcp() {
    let dir = tempfile::tempdir().unwrap();
    let input = funded_source(dir.path());
    let batch = changes(json!([
        {"op":"add_project_funding","source":"Capital","amount":1000,"share":100}
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
    let funds = rows(&cli_out, "PROJFUND");
    assert_eq!(funds.len(), 1);
    assert_eq!(funds[0]["fund_cost"], "1000");
    assert_eq!(funds[0]["fund_wt"], "100");
    let mcp_out = dir.path().join("mcp.xer");
    let mut server = Server::default();
    let meta = json!({"io.modelcontextprotocol/protocolVersion":"2026-07-28",
        "io.modelcontextprotocol/clientCapabilities":{}});
    let reply = server
        .handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
            "params":{"_meta":meta,"name":"openxer_apply","arguments":{"input":input,"output":mcp_out,"changes":batch}}}))
        .unwrap();
    assert_eq!(reply["result"]["structuredContent"]["ok"], true, "{reply}");
    assert_eq!(rows(&mcp_out, "PROJFUND"), funds);
}

/// `delete_calendar` refuses a calendar in use unless told what takes its
/// place; with `replacement` (or `default_global`) every user moves there.
#[test]
fn delete_calendar_in_use_needs_a_replacement() {
    let standard = "Standard 5-day / 8-hour";
    let setup = [
        json!({"op":"add_calendar","calendar":"Night shift"}),
        json!({"op":"set_activity","activity":"RB-1020","field":"calendar","value":"Night shift"}),
    ];
    let with = |op: Value| {
        let mut ops = setup.to_vec();
        ops.push(op);
        Value::Array(ops)
    };
    let dir = tempfile::tempdir().unwrap();
    let plain = json!({"op":"delete_calendar","calendar":"Night shift"});
    let (r, out) = apply(dir.path(), "plain.xer", with(plain));
    assert_eq!(r["ok"], false, "in use: refused without a replacement");
    assert!(!out.exists());
    let (r, out) = apply(
        dir.path(),
        "replaced.xer",
        with(json!({"op":"delete_calendar","calendar":"Night shift","replacement":standard})),
    );
    assert_eq!(r["ok"], true, "{r}");
    let calendars = rows(&out, "CALENDAR");
    assert!(calendars.iter().all(|c| c["clndr_name"] != "Night shift"));
    let row = calendars.iter().find(|c| c["clndr_name"] == standard);
    let to = &row.expect("the standard calendar")["clndr_id"];
    assert_eq!(&task(&out, "RB-1020")["clndr_id"], to, "moved");
}

/// `set_project_code`: the Project Details Codes tab's Assign and Remove,
/// by code type name and value short name. One value per code type.
#[test]
fn set_project_code_assigns_replaces_and_removes_a_project_code() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("codes.xer");
    let mut p = project();
    let doc = &mut p.document;
    doc.table_mut("PCATTYPE").add(&[
        ("proj_catg_type_id", "31".into()),
        ("seq_num", "1".into()),
        ("proj_catg_type", "Region".into()),
    ]);
    for (id, short) in [("310", "N"), ("311", "S")] {
        doc.table_mut("PCATVAL").add(&[
            ("proj_catg_id", id.into()),
            ("proj_catg_type_id", "31".into()),
            ("seq_num", id.into()),
            ("proj_catg_short_name", short.into()),
        ]);
    }
    p.updated_document().write(&input).unwrap();
    let output = dir.path().join("codes-out.xer");
    let ops = changes(json!([
        {"op":"set_project_code","code_type":"Region","value":"N"},
        {"op":"set_project_code","code_type":"Region","value":"S"}
    ]));
    let r = run(json!({"command":"apply","input":input,"output":output,"changes":ops}));
    assert_eq!(r["ok"], true, "{r}");
    let assigned = rows(&output, "PROJPCAT");
    assert_eq!(assigned.len(), 1, "the second value replaces the first");
    assert_eq!(assigned[0]["proj_catg_id"], "311");
    let removed = dir.path().join("codes-removed.xer");
    let ops = changes(json!([{"op":"set_project_code","code_type":"Region","value":null}]));
    let r = run(json!({"command":"apply","input":output,"output":removed,"changes":ops}));
    assert_eq!(r["ok"], true, "{r}");
    let left = Document::read(&removed).unwrap();
    assert!(left.table("PROJPCAT").is_none_or(|t| t.rows.is_empty()));
    let ops = changes(json!([{"op":"set_project_code","code_type":"Region","value":"X"}]));
    let r = run(json!({"command":"apply","input":input,"output":removed,"changes":ops}));
    assert_eq!(r["ok"], false, "an unknown value is refused");
}
