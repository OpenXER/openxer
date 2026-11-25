//! `add_activity` without an explicit value takes the project's Defaults
//! tab values, as the app's Edit > Add does.
use openxer_automation::{dispatch, parse_command, response};
use openxer_xer::{starter, Document};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Path;

fn run(request: Value) -> Value {
    response(parse_command(request).and_then(dispatch))
}

fn tasks(path: &Path) -> Vec<BTreeMap<String, String>> {
    let doc = Document::read(path).unwrap();
    let t = doc.table("TASK").unwrap();
    t.rows
        .iter()
        .map(|r| t.fields.iter().cloned().zip(r.iter().cloned()).collect())
        .collect()
}

#[test]
fn add_activity_follows_the_project_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("source.xer");
    starter().updated_document().write(&input).unwrap();
    let output = dir.path().join("out.xer");
    let ops = json!([
        {"op":"set_project","field":"def_task_type","value":"TT_Rsrc"},
        {"op":"set_project","field":"def_duration_type","value":"DT_FixedRate"},
        {"op":"set_project","field":"def_complete_pct_type","value":"CP_Phys"},
        {"op":"set_project","field":"task_code_prefix","value":"A"},
        {"op":"set_project","field":"task_code_base","value":"1000"},
        {"op":"set_project","field":"task_code_step","value":"10"},
        {"op":"set_project","field":"task_code_prefix_flag","value":"Y"},
        // Nothing selected: the prefix and suffix (no A IDs yet).
        {"op":"add_activity","wbs":"RB.2"},
        // Incremented from a selected activity past the IDs in use, in its WBS.
        {"op":"add_activity","selected":"RB-1000","name":"Survey tie-in points"},
        // An explicit ID is kept; the types still come from the defaults.
        {"op":"add_activity","activity":"TI-1","name":"Tie-in 1"},
        // Next after the highest A ID.
        {"op":"set_project","field":"task_code_prefix_flag","value":"N"},
        {"op":"add_activity","selected":"RB-1000"}
    ]);
    let r = run(json!({
        "command":"apply","input":input,"output":output,
        "changes":{"format":"openxer-changes/1","operations":ops}
    }));
    assert_eq!(r["ok"], true, "{r}");
    let rows = tasks(&output);
    let task = |code: &str| {
        rows.iter()
            .find(|r| r["task_code"] == code)
            .unwrap_or_else(|| panic!("no {code}"))
    };
    let rb1000 = task("RB-1000");
    for code in ["A1000", "RB-1140", "TI-1", "A1010"] {
        let t = task(code);
        assert_eq!(t["task_type"], "TT_Rsrc", "{code}");
        assert_eq!(t["duration_type"], "DT_FixedRate", "{code}");
        assert_eq!(t["complete_pct_type"], "CP_Phys", "{code}");
        assert_eq!(t["target_drtn_hr_cnt"], "40", "{code}");
        assert_eq!(t["clndr_id"], "1", "{code}");
    }
    assert_eq!(task("A1000")["task_name"], "New Activity");
    assert_eq!(task("RB-1140")["task_name"], "Survey tie-in points");
    assert_eq!(task("RB-1140")["wbs_id"], rb1000["wbs_id"]);
}
