//! Milestones carry no resource or role assignments, through every edit
//! path; a file that already holds them loads and saves them untouched.
use openxer_automation::{dispatch, mcp::Server, parse_command, response};
use openxer_xer::{starter, Document, ProjectData};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

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
    p
}

fn write(dir: &Path, name: &str, p: &ProjectData) -> PathBuf {
    let path = dir.join(name);
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

/// A milestone of the starter and a task.
fn codes(p: &ProjectData) -> (String, String) {
    let mile = p
        .model
        .activities
        .iter()
        .find(|a| openxer_xer::assignments::is_milestone(a.task_type))
        .expect("the starter has a milestone");
    let task = p
        .model
        .activities
        .iter()
        .find(|a| !openxer_xer::assignments::is_milestone(a.task_type))
        .unwrap();
    (mile.task_code.clone(), task.task_code.clone())
}

#[test]
fn assigning_a_resource_to_a_milestone_is_refused_from_apply_and_mcp() {
    let dir = tempfile::tempdir().unwrap();
    let p = project();
    let (mile, _) = codes(&p);
    let input = write(dir.path(), "source.xer", &p);
    let out = dir.path().join("out.xer");
    let r = apply(
        &input,
        &out,
        json!([{"op":"assign_resource","activity":mile,"resource":"PF"}]),
    );
    assert_eq!(r["ok"], false, "{r}");
    assert_eq!(r["error"]["code"], "milestone_assignment", "{r}");
    assert!(r["error"]["message"]
        .as_str()
        .unwrap()
        .contains("milestones cannot have resource or role assignments"));
    assert!(!out.exists());
    let mut server = Server::default();
    let meta = json!({"io.modelcontextprotocol/protocolVersion":"2026-07-28",
        "io.modelcontextprotocol/clientCapabilities":{}});
    let reply = server
        .handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
            "params":{"_meta":meta,"name":"openxer_assign","arguments":{"input":input,"output":out,
            "activity":mile,"resource":"PF"}}}))
        .unwrap();
    assert_eq!(reply["result"]["isError"], true, "{reply}");
    assert_eq!(
        reply["result"]["structuredContent"]["error"]["code"],
        "milestone_assignment"
    );
    // A milestone may have a primary resource (its owner) and expenses.
    let r = apply(
        &input,
        &out,
        json!([
            {"op":"set_activity","activity":mile,"field":"primary_resource","value":"PF"},
            {"op":"add_expense","activity":mile,"name":"Inspection fee"}
        ]),
    );
    assert_eq!(r["ok"], true, "{r}");
    let doc = Document::read(&out).unwrap();
    let t = doc.table("TASK").unwrap();
    let row = t
        .rows
        .iter()
        .find(|r| t.value(r, "task_code") == mile)
        .unwrap();
    assert_eq!(t.value(row, "rsrc_id"), "77");
}

#[test]
fn a_task_with_assignments_is_not_turned_into_a_milestone() {
    let dir = tempfile::tempdir().unwrap();
    let p = project();
    let (_, task) = codes(&p);
    let input = write(dir.path(), "source.xer", &p);
    let out = dir.path().join("out.xer");
    let r = apply(
        &input,
        &out,
        json!([
            {"op":"assign_resource","activity":task,"resource":"PF"},
            {"op":"set_activity","activity":task,"field":"activity_type","value":"TT_FinMile"}
        ]),
    );
    assert_eq!(r["ok"], false, "{r}");
    assert_eq!(r["error"]["code"], "milestone_assignment", "{r}");
    let message = r["error"]["message"].as_str().unwrap();
    assert!(
        message.contains("Remove its assignments first (PF)"),
        "{message}"
    );
    // Without assignments the type changes.
    let r = apply(
        &input,
        &out,
        json!([{"op":"set_activity","activity":task,"field":"activity_type","value":"TT_FinMile"}]),
    );
    assert_eq!(r["ok"], true, "{r}");
}

#[test]
fn an_imported_milestone_with_assignments_loads_and_saves_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let mut p = project();
    let (mile, _) = codes(&p);
    let task_id = p
        .model
        .activities
        .iter()
        .find(|a| a.task_code == mile)
        .unwrap()
        .task_id;
    // Written by another program: the row is in the file as given.
    p.document = p.updated_document();
    p.document.table_mut("TASKRSRC").add(&[
        ("taskrsrc_id", "900".into()),
        ("task_id", task_id.to_string()),
        ("proj_id", p.model.project.proj_id.to_string()),
        ("rsrc_id", "77".into()),
        ("target_qty", "8".into()),
        ("remain_qty", "8".into()),
    ]);
    let path = dir.path().join("imported.xer");
    p.document.write(&path).unwrap();
    let reread = ProjectData::from_document(
        Document::read(&path).unwrap(),
        p.model.project.proj_id,
        String::new(),
    )
    .unwrap();
    let flagged = openxer_xer::assignments::milestones_with_assignments(&reread);
    assert_eq!(flagged.len(), 1);
    assert_eq!(flagged[0].1, vec!["PF".to_string()]);
    // An edit elsewhere and an export keep the row.
    let out = dir.path().join("edited.xer");
    let r = apply(
        &path,
        &out,
        json!([{"op":"set_project","field":"must_finish","value":"2027-06-30 17:00"}]),
    );
    assert_eq!(r["ok"], true, "{r}");
    let exported = dir.path().join("exported.xer");
    let r = run(json!({"command":"export","input":out,"output":exported}));
    assert_eq!(r["ok"], true, "{r}");
    let doc = Document::read(&exported).unwrap();
    let t = doc.table("TASKRSRC").unwrap();
    let row = t
        .rows
        .iter()
        .find(|r| t.value(r, "taskrsrc_id") == "900")
        .expect("the milestone's assignment is kept");
    assert_eq!(t.value(row, "task_id"), task_id.to_string());
    assert_eq!(t.value(row, "target_qty"), "8");
}

fn task_row(path: &Path, code: &str) -> std::collections::BTreeMap<String, String> {
    let doc = Document::read(path).unwrap();
    let t = doc.table("TASK").unwrap();
    let row = t
        .rows
        .iter()
        .find(|r| t.value(r, "task_code") == code)
        .unwrap();
    t.fields
        .iter()
        .map(|f| (f.clone(), t.value(row, f).to_string()))
        .collect()
}

#[test]
fn exact_writes_keep_the_milestone_rule() {
    let dir = tempfile::tempdir().unwrap();
    let p = project();
    let (_, task) = codes(&p);
    let input = write(dir.path(), "source.xer", &p);
    let assigned = dir.path().join("assigned.xer");
    let r = apply(
        &input,
        &assigned,
        json!([{"op":"assign_resource","activity":task,"resource":"PF"}]),
    );
    assert_eq!(r["ok"], true, "{r}");
    // recalculate false writes columns exactly, but not a milestone with
    // assignments.
    let out = dir.path().join("out.xer");
    let r = run(json!({"command":"apply","input":assigned,"output":out,
        "changes":{"format":"openxer-changes/1","recalculate":false,"operations":[
            {"op":"set_activity","activity":task,"field":"activity_type","value":"TT_FinMile"}]}}));
    assert_eq!(r["ok"], false, "{r}");
    assert_eq!(r["error"]["code"], "milestone_assignment", "{r}");
    assert!(
        r["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Remove its assignments first (PF)"),
        "{r}"
    );
    assert!(!out.exists());
    // An activity without assignments changes type exactly.
    let r = run(json!({"command":"apply","input":input,"output":out,
        "changes":{"format":"openxer-changes/1","recalculate":false,"operations":[
            {"op":"set_activity","activity":task,"field":"activity_type","value":"TT_FinMile"}]}}));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(task_row(&out, &task)["task_type"], "TT_FinMile");
}

#[test]
fn a_milestone_that_becomes_a_task_drops_a_primary_resource_it_has_not_assigned() {
    let dir = tempfile::tempdir().unwrap();
    let p = project();
    let (mile, _) = codes(&p);
    let input = write(dir.path(), "source.xer", &p);
    let owned = dir.path().join("owned.xer");
    let r = apply(
        &input,
        &owned,
        json!([{"op":"set_activity","activity":mile,"field":"primary_resource","value":"PF"}]),
    );
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(task_row(&owned, &mile)["rsrc_id"], "77");
    for (name, recalculate) in [("rules.xer", true), ("exact.xer", false)] {
        let out = dir.path().join(name);
        let r = run(json!({"command":"apply","input":owned,"output":out,
            "changes":{"format":"openxer-changes/1","recalculate":recalculate,"operations":[
                {"op":"set_activity","activity":mile,"field":"activity_type","value":"TT_Task"}]}}));
        assert_eq!(r["ok"], true, "{r}");
        let row = task_row(&out, &mile);
        assert_eq!(row["task_type"], "TT_Task", "{name}");
        assert_eq!(
            row["rsrc_id"], "",
            "{name}: a task's primary resource must be assigned"
        );
    }
    // Assigned in the same batch, the primary resource stays.
    let out = dir.path().join("kept.xer");
    let r = apply(
        &owned,
        &out,
        json!([
            {"op":"set_activity","activity":mile,"field":"activity_type","value":"TT_Task"},
            {"op":"assign_resource","activity":mile,"resource":"PF"},
            {"op":"set_activity","activity":mile,"field":"primary_resource","value":"PF"}
        ]),
    );
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(task_row(&out, &mile)["rsrc_id"], "77");
}
