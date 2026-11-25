//! `dissolve_activity` (Edit > Dissolve) through `apply` and the single
//! edit command, on the built-in starter project. The rule itself is tested
//! case by case in `xer/src/edit/dissolve.rs`.
use openxer_automation::{
    cli::{self, Invocation},
    dispatch, parse_command, response,
};
use openxer_xer::{starter, Document};
use serde_json::{json, Value};
use std::path::Path;

fn run(request: Value) -> Value {
    response(parse_command(request).and_then(dispatch))
}

/// TASKPRED rows as (predecessor task_id, successor task_id, type, lag hours).
fn links(path: &Path) -> Vec<(String, String, String, String)> {
    let doc = Document::read(path).unwrap();
    let t = doc.table("TASKPRED").unwrap();
    let col = |name: &str| t.field(name).unwrap();
    let (p, s, k, l) = (
        col("pred_task_id"),
        col("task_id"),
        col("pred_type"),
        col("lag_hr_cnt"),
    );
    let mut all: Vec<_> = t
        .rows
        .iter()
        .map(|r| (r[p].clone(), r[s].clone(), r[k].clone(), r[l].clone()))
        .collect();
    all.sort();
    all
}

/// The starter (RB-1030 FS RB-1050 FS RB-1080) with RB-1030 to RB-1050
/// replaced by SS 16h and RB-1050 to RB-1080 by FF 24h, then RB-1050
/// dissolved.
fn dissolved(dir: &Path, name: &str, retain_lag: Option<bool>) -> (Value, std::path::PathBuf) {
    let input = dir.join("source.xer");
    starter().updated_document().write(&input).unwrap();
    let output = dir.join(name);
    let mut dissolve = json!({"op":"dissolve_activity","activity":"RB-1050"});
    if let Some(on) = retain_lag {
        dissolve["retain_lag"] = json!(on);
    }
    let r = run(json!({"command":"apply","input":input,"output":output,
    "changes":{"format":"openxer-changes/1","operations":[
        {"op":"relationship","action":"remove","predecessor":"RB-1030","successor":"RB-1050","kind":"FS"},
        {"op":"relationship","action":"add","predecessor":"RB-1030","successor":"RB-1050","kind":"SS","lag_hours":16},
        {"op":"relationship","action":"remove","predecessor":"RB-1050","successor":"RB-1080","kind":"FS"},
        {"op":"relationship","action":"add","predecessor":"RB-1050","successor":"RB-1080","kind":"FF","lag_hours":24},
        dissolve
    ]}}));
    (r, output)
}

fn joined(path: &Path) -> (String, f64) {
    let all = links(path);
    let l = all
        .iter()
        .find(|l| l.0 == "1030" && l.1 == "1080")
        .unwrap_or_else(|| panic!("RB-1030 to RB-1080 in {all:?}"));
    (l.2.clone(), l.3.parse().unwrap())
}

#[test]
fn dissolve_activity_joins_predecessor_start_to_successor_end() {
    let dir = tempfile::tempdir().unwrap();
    // Retain Lag left out is off: no lag.
    let (r, out) = dissolved(dir.path(), "off.xer", None);
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(joined(&out), ("PR_SF".into(), 0.));
    assert!(links(&out).iter().all(|l| l.0 != "1050" && l.1 != "1050"));
    // On: 16h + 24h.
    let (r, out) = dissolved(dir.path(), "on.xer", Some(true));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(joined(&out), ("PR_SF".into(), 40.));
    let result = &r["result"]["report"]["results"][4];
    assert_eq!(result["relationships_added"], 1, "{r}");
}

#[test]
fn dissolve_activity_needs_a_predecessor_and_a_successor() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("source.xer");
    starter().updated_document().write(&input).unwrap();
    // RB-1000 starts the network: it has no predecessor.
    let Invocation::Run(command) = cli::parse([
        "dissolve-activity",
        "--input",
        input.to_str().unwrap(),
        "--activity",
        "RB-1000",
        "--dry-run",
    ])
    .unwrap() else {
        panic!()
    };
    let r = response(dispatch(*command));
    assert_eq!(r["ok"], false, "{r}");
    assert!(
        r.to_string().contains("one predecessor and one successor"),
        "{r}"
    );
    assert!(cli::parse([
        "dissolve-activity",
        "--input",
        "x.xer",
        "--activity",
        "A",
        "--retain-lag",
        "maybe"
    ])
    .is_err());
}
