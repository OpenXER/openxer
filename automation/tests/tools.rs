//! Progress, steps, feedback, assignment and activity figures, top-down
//! estimation, WBS moves and calendar maintenance through `apply`, the CLI
//! and MCP, on the synthetic starter project.
use openxer_automation::{
    cli::{self, Invocation},
    dispatch,
    mcp::Server,
    parse_command, response,
};
use openxer_xer::{starter, Document};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The starter with a labor resource PF, a role FIT and a cost account.
fn source(dir: &Path) -> PathBuf {
    let mut p = starter();
    p.document.table_mut("RSRC").add(&[
        ("rsrc_id", "77".into()),
        ("rsrc_name", "Pipefitter".into()),
        ("rsrc_short_name", "PF".into()),
        ("rsrc_type", "RT_Labor".into()),
        ("def_qty_per_hr", "1".into()),
        ("clndr_id", "1".into()),
    ]);
    p.document.table_mut("ROLES").add(&[
        ("role_id", "3".into()),
        ("role_short_name", "FIT".into()),
        ("role_name", "Fitter".into()),
    ]);
    p.document.table_mut("ACCOUNT").add(&[
        ("acct_id", "5".into()),
        ("acct_short_name", "100".into()),
        ("acct_name", "Mechanical".into()),
    ]);
    let path = dir.join("source.xer");
    p.updated_document().write(&path).unwrap();
    path
}

fn run(request: Value) -> Value {
    response(parse_command(request).and_then(dispatch))
}

fn apply(input: &Path, output: &Path, ops: Value) -> Value {
    run(json!({"command":"apply","input":input,"output":output,
        "changes":{"format":"openxer-changes/1","operations":ops}}))
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

fn task(path: &Path, code: &str) -> BTreeMap<String, String> {
    rows(path, "TASK")
        .into_iter()
        .find(|r| r["task_code"] == code)
        .unwrap_or_else(|| panic!("no {code}"))
}

#[test]
fn update_progress_and_apply_actuals_move_the_data_date() {
    let dir = tempfile::tempdir().unwrap();
    let input = source(dir.path());
    let output = dir.path().join("progress.xer");
    let r = apply(
        &input,
        &output,
        json!([
            {"op":"schedule"},
            {"op":"update_progress","data_date":"2026-11-16 08:00"},
            {"op":"schedule"}
        ]),
    );
    assert_eq!(r["ok"], true, "{r}");
    let changed = r["result"]["report"]["results"][1]["changed_activities"]
        .as_u64()
        .unwrap();
    assert!(changed > 0, "{r}");
    let first = task(&output, "RB-1000");
    assert_ne!(first["act_start_date"], "", "{first:?}");
    assert_eq!(
        rows(&output, "PROJECT")[0]["last_recalc_date"],
        "2026-11-16 08:00"
    );
    // Only the activities named.
    let only = dir.path().join("only.xer");
    let r = apply(
        &input,
        &only,
        json!([{"op":"schedule"},
            {"op":"update_progress","data_date":"2026-11-16 08:00","activities":["RB-1010"]}]),
    );
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(task(&only, "RB-1000")["act_start_date"], "");
    // An earlier date is accepted: the data date moves back and the
    // progress already recorded stays.
    let back = dir.path().join("back.xer");
    let r = apply(
        &output,
        &back,
        json!([{"op":"update_progress","data_date":"2026-11-02 08:00"}]),
    );
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["result"]["report"]["results"][0]["changed_activities"], 0);
    assert_eq!(
        rows(&back, "PROJECT")[0]["last_recalc_date"],
        "2026-11-02 08:00"
    );
    let kept = task(&back, "RB-1000");
    assert_eq!(kept["act_start_date"], first["act_start_date"]);
    assert_eq!(kept["status_code"], first["status_code"]);
    // Setting the data date directly takes any date, earlier included.
    let set = dir.path().join("set.xer");
    let r = apply(
        &output,
        &set,
        json!([{"op":"set_project","field":"data_date",
            "value":"2026-10-26 08:00","schedule":true}]),
    );
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(
        rows(&set, "PROJECT")[0]["last_recalc_date"],
        "2026-10-26 08:00"
    );
    let kept = task(&set, "RB-1000");
    assert_eq!(kept["act_start_date"], first["act_start_date"]);
    // Apply Actuals touches only activities set to compute actuals and
    // stamps the project.
    let actuals = dir.path().join("actuals.xer");
    let r = apply(
        &input,
        &actuals,
        json!([{"op":"schedule"},
            {"op":"apply_actuals","data_date":"2026-11-09 08:00"}]),
    );
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["result"]["report"]["results"][1]["changed_activities"], 0);
    assert_eq!(
        rows(&actuals, "PROJECT")[0]["apply_actuals_date"],
        "2026-11-09 08:00"
    );
}

#[test]
fn steps_are_added_edited_moved_and_drive_nothing_unless_asked() {
    let dir = tempfile::tempdir().unwrap();
    let input = source(dir.path());
    let output = dir.path().join("steps.xer");
    let r = apply(
        &input,
        &output,
        json!([
            {"op":"add_step","activity":"RB-1010","name":"Set forms"},
            {"op":"add_step","activity":"RB-1010","name":"Pour"},
            {"op":"add_step","activity":"RB-1010","name":"Strip","at":1},
            {"op":"set_step","activity":"RB-1010","step":"Pour","field":"weight","value":3},
            {"op":"set_step","activity":"RB-1010","step":"Set forms","field":"percent_complete","value":100},
            {"op":"move_step","activity":"RB-1010","step":"Strip","to":3},
            {"op":"add_step","activity":"RB-1010","name":"Cure"},
            {"op":"delete_step","activity":"RB-1010","step":4},
            {"op":"create_step_template","activity":"RB-1010","name":"Concrete"},
            {"op":"add_steps_from_template","activity":"RB-1020","template":"Concrete"},
            {"op":"set_feedback","activity":"RB-1010","notes":"Call before pour","review_required":true}
        ]),
    );
    assert_eq!(r["ok"], true, "{r}");
    let mut steps: Vec<_> = rows(&output, "TASKPROC");
    let id = task(&output, "RB-1010")["task_id"].clone();
    steps.retain(|s| s["task_id"] == id);
    steps.sort_by_key(|s| s["seq_num"].parse::<i64>().unwrap());
    let names: Vec<&str> = steps.iter().map(|s| s["proc_name"].as_str()).collect();
    assert_eq!(names, ["Set forms", "Pour", "Strip"]);
    assert_eq!(steps[0]["complete_flag"], "Y");
    assert_eq!(steps[1]["proc_wt"], "3");
    let other = task(&output, "RB-1020")["task_id"].clone();
    assert_eq!(
        rows(&output, "TASKPROC")
            .iter()
            .filter(|s| s["task_id"] == other)
            .count(),
        3
    );
    assert_eq!(rows(&output, "PROCGROUP").len(), 1);
    assert!(rows(&output, "TASKNOTE")[0]["task_notes"].contains("Call before pour"));
    assert_eq!(task(&output, "RB-1010")["rev_fdbk_flag"], "Y");
    for (k, op) in [
        json!({"op":"set_step","activity":"RB-1010","step":"Nope","field":"name","value":"x"}),
        json!({"op":"set_step","activity":"RB-1010","step":1,"field":"percent_complete","value":120}),
        json!({"op":"add_steps_from_template","activity":"RB-1010","template":"Missing"}),
    ]
    .into_iter()
    .enumerate()
    {
        let out = dir.path().join(format!("bad-{k}.xer"));
        let r = apply(&output, &out, json!([op]));
        assert_eq!(r["ok"], false, "{k}: {r}");
    }
}

#[test]
fn assignment_role_account_and_activity_figures_use_the_setters() {
    let dir = tempfile::tempdir().unwrap();
    let input = source(dir.path());
    let output = dir.path().join("assigned.xer");
    let r = apply(
        &input,
        &output,
        json!([
            {"op":"assign_resource","activity":"RB-1010","resource":"PF"},
            {"op":"set_assignment","activity":"RB-1010","resource":"PF","field":"role","value":"FIT"},
            {"op":"set_assignment","activity":"RB-1010","resource":"PF","field":"cost_account","value":"Mechanical"},
            {"op":"set_assignment","activity":"RB-1010","resource":"PF","field":"drive_activity_dates","value":false},
            {"op":"set_activity","activity":"RB-1020","field":"budgeted_nonlabor_units","value":12},
            {"op":"set_activity","activity":"RB-1010","field":"budgeted_labor_units","value":40}
        ]),
    );
    assert_eq!(r["ok"], true, "{r}");
    let a = &rows(&output, "TASKRSRC")[0];
    assert_eq!(a["role_id"], "3");
    assert_eq!(a["acct_id"], "5");
    assert_eq!(a["target_qty"], "40");
    assert_eq!(task(&output, "RB-1020")["target_equip_qty"], "12");
    let r = apply(
        &output,
        &dir.path().join("bad.xer"),
        json!([{"op":"set_activity","activity":"RB-1020","field":"budgeted_total_cost","value":1}]),
    );
    assert_eq!(r["ok"], false, "total cost is not edited: {r}");
}

#[test]
fn top_down_wbs_and_calendar_maintenance() {
    let dir = tempfile::tempdir().unwrap();
    let input = source(dir.path());
    let output = dir.path().join("tools.xer");
    let r = apply(
        &input,
        &output,
        json!([
            {"op":"top_down_estimate","labor_units":1000,"weights":{"RB.3":3}},
            {"op":"shift_wbs","wbs":"RB.2","direction":"down"},
            {"op":"rename_wbs_code","wbs":"RB.4","code":"RB.9"},
            {"op":"copy_calendar","calendar":"Night shift","source":"Standard 5-day / 8-hour","kind":"global"},
            {"op":"rename_calendar","calendar":"Night shift","name":"Nights"},
            {"op":"set_default_calendar","calendar":"Nights"}
        ]),
    );
    assert_eq!(r["ok"], true, "{r}");
    let total: f64 = rows(&output, "TASK")
        .iter()
        .map(|t| t["target_work_qty"].parse::<f64>().unwrap_or(0.))
        .sum();
    assert!((total - 1000.).abs() < 0.01, "{total}");
    let wbs = rows(&output, "PROJWBS");
    let code = |c: &str| wbs.iter().find(|w| w["wbs_short_name"] == c).cloned();
    assert!(code("RB.9").is_some() && code("RB.4").is_none());
    assert_eq!(code("RB.3").unwrap()["est_wt"], "3");
    let seq = |c: &str| code(c).unwrap()["seq_num"].parse::<i64>().unwrap();
    assert!(seq("RB.2") > seq("RB.3"), "{wbs:?}");
    let cals = rows(&output, "CALENDAR");
    let nights = cals.iter().find(|c| c["clndr_name"] == "Nights").unwrap();
    assert_eq!(nights["clndr_type"], "CA_Base");
    assert_eq!(nights["default_flag"], "Y");
    assert_eq!(cals.iter().filter(|c| c["default_flag"] == "Y").count(), 1);
    for (k, op) in [
        json!({"op":"shift_wbs","wbs":"RB.1","direction":"sideways"}),
        json!({"op":"top_down_estimate","labor_units":0}),
        json!({"op":"convert_calendar","calendar":"Nights","to":"global"}),
    ]
    .into_iter()
    .enumerate()
    {
        let r = apply(
            &output,
            &dir.path().join(format!("bad-{k}.xer")),
            json!([op]),
        );
        assert_eq!(r["ok"], false, "{k}: {r}");
    }
}

#[test]
fn the_new_edits_run_from_the_command_line_and_mcp() {
    let dir = tempfile::tempdir().unwrap();
    let input = source(dir.path());
    let cli_out = dir.path().join("cli.xer");
    let Invocation::Run(command) = cli::parse([
        "add-step",
        "--input",
        input.to_str().unwrap(),
        "--activity",
        "RB-1010",
        "--name",
        "Survey",
        "--output",
        cli_out.to_str().unwrap(),
    ])
    .unwrap() else {
        panic!()
    };
    assert_eq!(response(dispatch(*command))["ok"], true);
    let mcp_out = dir.path().join("mcp.xer");
    let mut server = Server::default();
    let meta = json!({"io.modelcontextprotocol/protocolVersion":"2026-07-28",
        "io.modelcontextprotocol/clientCapabilities":{}});
    let reply = server
        .handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
            "params":{"_meta":meta,"name":"openxer_add_step","arguments":{"input":input,"output":mcp_out,
            "activity":"RB-1010","name":"Survey"}}}))
        .unwrap();
    assert_eq!(reply["result"]["structuredContent"]["ok"], true, "{reply}");
    assert_eq!(rows(&cli_out, "TASKPROC"), rows(&mcp_out, "TASKPROC"));
    for args in [
        vec!["update-progress", "--data-date", "2026-11-16 08:00"],
        vec!["recalculate-costs"],
        vec!["shift-wbs", "--wbs", "RB.2", "--direction", "up"],
        vec!["set-feedback", "--activity", "RB-1010", "--notes", "Hold"],
    ] {
        let mut full = args.clone();
        full.extend(["--input", input.to_str().unwrap(), "--dry-run", "true"]);
        let Invocation::Run(command) =
            cli::parse(full.clone()).unwrap_or_else(|e| panic!("{full:?}: {e:?}"))
        else {
            panic!()
        };
        let r = response(dispatch(*command));
        assert_eq!(r["ok"], true, "{full:?}: {r}");
    }
}
