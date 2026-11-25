//! Expense items, issues, risks, thresholds and Monitor Thresholds through
//! `apply`, the single edit commands, MCP and the `records` and
//! `earned_value` reads, on the synthetic starter project.
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

fn source(dir: &Path) -> PathBuf {
    let path = dir.join("source.xer");
    starter().updated_document().write(&path).unwrap();
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
    let Some(t) = doc.table(table) else {
        return vec![];
    };
    t.rows
        .iter()
        .map(|r| t.fields.iter().cloned().zip(r.iter().cloned()).collect())
        .collect()
}

fn apply(input: &Path, output: &Path, ops: Value) -> Value {
    run(json!({"command":"apply","input":input,"output":output,"changes":changes(ops)}))
}

fn records(input: &Path, kind: &str) -> Vec<Value> {
    let r = run(json!({"command":"records","input":input,"kind":kind}));
    assert_eq!(r["ok"], true, "{r}");
    r["result"]["records"].as_array().unwrap().clone()
}

fn first_activity(input: &Path) -> String {
    let r = run(json!({"command":"load","input":input,"limit":5}));
    r["result"]["activities"][1]["task_code"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn expense_items_keep_their_costs_consistent() {
    let dir = tempfile::tempdir().unwrap();
    let input = source(dir.path());
    let code = first_activity(&input);
    let output = dir.path().join("expenses.xer");
    let r = apply(
        &input,
        &output,
        json!([
            {"op":"add_expense","activity":code,"name":"Crane rental",
             "fields":{"accrual":"Start of Activity","vendor":"Lift Co","budgeted_units":4}},
            {"op":"set_expense","expense":"Crane rental","field":"price_per_unit","value":"$1,250"},
            {"op":"add_expense","activity":code,"name":"Permits"},
            {"op":"set_expense","expense":"Permits","field":"budgeted_cost","value":300},
            {"op":"delete_expense","expense":"Permits"}
        ]),
    );
    assert_eq!(r["ok"], true, "{r}");
    let items = rows(&output, "PROJCOST");
    assert_eq!(items.len(), 1, "{items:?}");
    let crane = &items[0];
    assert_eq!(crane["cost_name"], "Crane rental");
    assert_eq!(crane["cost_load_type"], "CL_Start");
    assert_eq!(crane["vendor_name"], "Lift Co");
    // 4 units at 1250 is the budget; without actuals it is all remaining.
    assert_eq!(crane["target_cost"], "5000");
    assert_eq!(crane["remain_cost"], "5000");
    let listed = records(&output, "expenses");
    assert_eq!(listed[0]["activity"], code);
    assert_eq!(listed[0]["accrual"], "Start of Activity");
    assert_eq!(listed[0]["budgeted_cost"], "5000");
    // Refused, writing nothing: no activity, an unknown field, a bad
    // number, an unknown choice, an unknown item.
    for (k, op) in [
        json!({"op":"add_expense","activity":"NOPE"}),
        json!({"op":"set_expense","expense":"Crane rental","field":"color","value":1}),
        json!({"op":"set_expense","expense":"Crane rental","field":"actual_cost","value":"lots"}),
        json!({"op":"set_expense","expense":"Crane rental","field":"accrual","value":"Monthly"}),
        json!({"op":"delete_expense","expense":"Fuel"}),
    ]
    .into_iter()
    .enumerate()
    {
        let out = dir.path().join(format!("bad-{k}.xer"));
        let r = apply(&output, &out, json!([op]));
        assert_eq!(r["ok"], false, "{k}: {r}");
        assert!(!out.exists());
    }
}

#[test]
fn issues_and_risks_name_activities_wbs_and_resources_as_the_edits_do() {
    let dir = tempfile::tempdir().unwrap();
    let input = source(dir.path());
    let code = first_activity(&input);
    let output = dir.path().join("register.xer");
    let r = apply(
        &input,
        &output,
        json!([
            {"op":"add_issue","name":"Late steel","fields":{"priority":"High","activity":code,
             "identified_on":"2026-11-03","notes":"Mill rolled late"}},
            {"op":"set_issue","issue":"Late steel","field":"wbs","value":"RB.3"},
            {"op":"add_risk","name":"Crane availability","fields":{"type":"Opportunity","status":"Open",
             "response_type":"Avoid","cause":"Two jobs share one crane"}},
            {"op":"assign_risk","risk":"RISK-1","activity":code},
            {"op":"add_risk","name":"Weather"},
            {"op":"delete_risk","risk":"Weather"}
        ]),
    );
    assert_eq!(r["ok"], true, "{r}");
    let issue = &rows(&output, "PROJISSU")[0];
    assert_eq!(issue["priority_type"], "PT_High");
    assert_eq!(issue["add_date"], "2026-11-03 00:00");
    // Moving the issue to a WBS element clears its activity.
    assert_eq!(issue["task_id"], "");
    assert_ne!(issue["wbs_id"], "");
    let listed = records(&output, "issues");
    assert_eq!(listed[0]["wbs"], "RB.3");
    assert_eq!(listed[0]["notes"], "Mill rolled late");
    let risks = rows(&output, "PROJRISK");
    assert_eq!(risks.len(), 1);
    assert_eq!(risks[0]["risk_code"], "RISK-1");
    assert_eq!(risks[0]["risk_to_type"], "RT_Opport");
    assert_eq!(risks[0]["response_type"], "RP_Avoid");
    let listed = records(&output, "risks");
    assert_eq!(listed[0]["activities"], json!([code]));
    let only = run(json!({"command":"records","input":output,"kind":"risks","activity":code}));
    assert_eq!(only["result"]["count"], 1);
    let unassigned = dir.path().join("unassigned.xer");
    let r = apply(
        &output,
        &unassigned,
        json!([{"op":"unassign_risk","risk":"Crane availability","activity":code}]),
    );
    assert_eq!(r["ok"], true, "{r}");
    assert!(rows(&unassigned, "TASKRISK").is_empty());
    // A second unassign has nothing to remove.
    let again = dir.path().join("again.xer");
    let r = apply(
        &unassigned,
        &again,
        json!([{"op":"unassign_risk","risk":"RISK-1","activity":code}]),
    );
    assert_eq!(r["ok"], false, "{r}");
    // Values Monitor Thresholds sets are not edited by hand.
    let r = apply(
        &output,
        &again,
        json!([{"op":"set_issue","issue":"Late steel","field":"actual_value","value":3}]),
    );
    assert_eq!(r["ok"], false, "{r}");
}

#[test]
fn monitor_thresholds_raises_issues_once_and_updates_them_after() {
    let dir = tempfile::tempdir().unwrap();
    let input = source(dir.path());
    let output = dir.path().join("monitored.xer");
    let r = apply(
        &input,
        &output,
        json!([
            {"op":"schedule"},
            {"op":"add_threshold","parameter":"Total Float","fields":{"lower_threshold":10000,
             "priority":"Top"}},
            {"op":"add_threshold","parameter":"free_float_hr_cnt","fields":{"status":"Disabled"}},
            {"op":"monitor_thresholds","today":"2026-11-08"}
        ]),
    );
    assert_eq!(r["ok"], true, "{r}");
    let report = &r["result"]["report"]["results"][3];
    assert_eq!(report["thresholds"], 1, "{report}");
    let raised = report["issues_raised"].as_u64().unwrap();
    let activities = rows(&output, "TASK").len() as u64;
    assert_eq!(raised, activities, "every unfinished activity has float");
    let issues = rows(&output, "PROJISSU");
    assert_eq!(issues.len() as u64, raised);
    assert!(issues.iter().all(|i| i["priority_type"] == "PT_Top"
        && i["add_by_name"] == "Monitor Thresholds"
        && i["add_date"] == "2026-11-08 00:00"
        && i["status_code"] == "IS_Open"));
    assert_eq!(
        report["baselines"]["project_baseline"], "Current Project",
        "an XER stands in as its own baseline"
    );
    // A second run raises nothing new and updates nothing.
    let again = dir.path().join("again.xer");
    let r = run(json!({"command":"edit","input":output,"output":again,
        "operation":{"op":"monitor_thresholds"}}));
    assert_eq!(r["ok"], true, "{r}");
    let report = &r["result"]["report"]["results"][0];
    assert_eq!(
        (
            report["issues_raised"].as_u64(),
            report["issues_updated"].as_u64()
        ),
        (Some(0), Some(0))
    );
    assert_eq!(rows(&again, "PROJISSU").len() as u64, raised);
    // Dry run: the diff shows the issues, nothing is written.
    let r = run(json!({"command":"edit","input":input,"dry_run":true,
        "operation":{"op":"monitor_thresholds"}}));
    assert_eq!(r["ok"], true, "{r}");
    let listed = records(&output, "thresholds");
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0]["parameter"], "Total Float (days)");
    assert_eq!(listed[0]["lower_threshold"], "10000");
    assert_eq!(listed[1]["status"], "Disabled");
}

#[test]
fn earned_value_reads_the_same_figures_as_the_columns() {
    let dir = tempfile::tempdir().unwrap();
    let input = source(dir.path());
    let r = run(json!({"command":"earned_value","input":input}));
    assert_eq!(r["ok"], true, "{r}");
    let result = &r["result"];
    assert_eq!(result["basis"], "cost");
    let rows = result["activities"].as_array().unwrap();
    assert!(!rows.is_empty());
    let sum: f64 = rows
        .iter()
        .map(|a| a["budget_at_completion"].as_f64().unwrap())
        .sum();
    let total = result["project"]["budget_at_completion"].as_f64().unwrap();
    assert!((sum - total).abs() < 0.05, "{sum} {total}");
    let bad = run(json!({"command":"earned_value","input":input,"basis":"hours"}));
    assert_eq!(bad["ok"], false);
}

#[test]
fn register_edits_run_the_same_from_the_command_line_and_mcp() {
    let dir = tempfile::tempdir().unwrap();
    let input = source(dir.path());
    let code = first_activity(&input);
    let cli_out = dir.path().join("cli.xer");
    let Invocation::Run(command) = cli::parse([
        "add-expense",
        "--input",
        input.to_str().unwrap(),
        "--activity",
        &code,
        "--name",
        "Scaffold",
        "--fields",
        r#"{"budgeted_cost":900}"#,
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
            "params":{"_meta":meta,"name":"openxer_add_expense","arguments":{"input":input,"output":mcp_out,
            "activity":code,"name":"Scaffold","fields":{"budgeted_cost":900}}}}))
        .unwrap();
    assert_eq!(reply["result"]["structuredContent"]["ok"], true, "{reply}");
    assert_eq!(rows(&cli_out, "PROJCOST"), rows(&mcp_out, "PROJCOST"));
    assert_eq!(rows(&cli_out, "PROJCOST")[0]["target_cost"], "900");
    // The reads: records and earned_value from the command line.
    for args in [
        vec![
            "records",
            "--input",
            cli_out.to_str().unwrap(),
            "--kind",
            "expenses",
        ],
        vec![
            "earned-value",
            "--input",
            cli_out.to_str().unwrap(),
            "--basis",
            "labor_units",
        ],
        vec![
            "monitor-thresholds",
            "--input",
            cli_out.to_str().unwrap(),
            "--dry-run",
            "true",
        ],
    ] {
        let Invocation::Run(command) = cli::parse(args.clone()).unwrap() else {
            panic!()
        };
        let r = response(dispatch(*command));
        assert_eq!(r["ok"], true, "{args:?}: {r}");
    }
}

/// A workspace baseline that cannot be read leaves the current project in
/// its place, and the result says so instead of passing it over.
#[test]
fn an_unreadable_assigned_baseline_is_reported() {
    let dir = tempfile::tempdir().unwrap();
    let input = source(dir.path());
    let ws = dir.path().join("ws.db");
    let r = run(json!({"command":"import","input":input,"output":ws}));
    assert_eq!(r["ok"], true, "{r}");
    let text = std::fs::read_to_string(&input).unwrap();
    {
        let c = rusqlite::Connection::open(&ws).unwrap();
        c.execute_batch(
            "CREATE TABLE IF NOT EXISTS baselines(id INTEGER PRIMARY KEY, parent INTEGER NOT NULL,
               project_key INTEGER NOT NULL, short_name TEXT NOT NULL DEFAULT '', name TEXT NOT NULL,
               xer_content TEXT NOT NULL, source_windows_encoding INTEGER NOT NULL DEFAULT 0);
             CREATE TABLE IF NOT EXISTS baseline_assign(parent INTEGER PRIMARY KEY,
               primary_bl TEXT NOT NULL DEFAULT 'current');",
        )
        .unwrap();
        let parent: i64 = c
            .query_row("SELECT id FROM projects", [], |r| r.get(0))
            .unwrap();
        // The stored text holds no project 424242.
        c.execute(
            "INSERT INTO baselines(id,parent,project_key,name,xer_content) VALUES(7,?1,424242,'Gone',?2)",
            rusqlite::params![parent, text],
        )
        .unwrap();
        c.execute(
            "INSERT INTO baseline_assign(parent,primary_bl) VALUES(?1,'7')",
            [parent],
        )
        .unwrap();
    }
    let r = run(json!({"command":"earned_value","input":ws}));
    assert_eq!(r["ok"], true, "{r}");
    let baselines = &r["result"]["baselines"];
    assert_eq!(
        baselines["primary_baseline"], "Current Project",
        "{baselines}"
    );
    let warnings = baselines["warnings"].as_array().expect("a warning");
    assert!(
        warnings[0].as_str().unwrap().contains("\"Gone\""),
        "{baselines}"
    );
}
