//! Workflow: Global Change, round two (backlog 3.8 and 3.9). Modify Global
//! Change has the All / Any heading row over its "Where" rows, offers "is
//! under" for WBS and activity code parameters (and WBS starts on it),
//! lists user defined fields as Then parameters and activity codes and user
//! defined fields as If parameters, and moves the selected statement up
//! and down with the shared move pad. The command line and MCP run the same
//! global change, saved in the workspace or given as JSON, and their dry
//! run lists the lines the report shows.
use super::*;
use crate::grid::columns;
use serde_json::{json, Value as Json};

/// The starter with an activity code type "Area" (N with NE under it, and
/// S) and a text user defined field "Crew"; RB-1080 is in area NE and
/// RB-1000 in S.
fn coded() -> ProjectData {
    let mut doc = openxer_xer::starter().document;
    doc.table_mut("ACTVTYPE").add(&[
        ("actv_code_type_id", "7".into()),
        ("actv_code_type", "Area".into()),
        ("actv_code_type_scope", "AS_Project".into()),
        ("proj_id", "1".into()),
        ("seq_num", "1".into()),
    ]);
    for (id, short, parent) in [(71, "N", ""), (72, "NE", "71"), (73, "S", "")] {
        doc.table_mut("ACTVCODE").add(&[
            ("actv_code_id", id.to_string()),
            ("actv_code_type_id", "7".into()),
            ("short_name", short.into()),
            ("actv_code_name", format!("Area {short}")),
            ("parent_actv_code_id", parent.into()),
            ("seq_num", id.to_string()),
        ]);
    }
    for (task, code) in [("1080", "72"), ("1000", "73")] {
        doc.table_mut("TASKACTV").add(&[
            ("task_id", task.into()),
            ("actv_code_type_id", "7".into()),
            ("actv_code_id", code.into()),
            ("proj_id", "1".into()),
        ]);
    }
    doc.table_mut("UDFTYPE").add(&[
        ("udf_type_id", "5".into()),
        ("table_name", "TASK".into()),
        ("udf_type_name", "user_field_5".into()),
        ("udf_type_label", "Crew".into()),
        ("logical_data_type", "FT_TEXT".into()),
    ]);
    let mut p = ProjectData::from_document(doc, 1, "Coded starter".into()).unwrap();
    schedule_model(&mut p.model, &p.resources).unwrap();
    p
}

fn edit(d: &Driver) -> &crate::editing::dialogs::GcEdit {
    d.app
        .editing
        .dialogs
        .gc_edit
        .as_ref()
        .expect("Modify is open")
}

/// Open drop-down `combo`, pick its item `item` (`<combo>_<value>`).
fn pick(d: &mut Driver, combo: &str, item: &str) {
    d.click(combo);
    let key = format!("{combo}_{item}");
    assert!(d.has(&key), "{key} is listed");
    d.click(&key);
}

/// Open drop-down `combo`, report whether it lists `item`, and close it.
fn lists(d: &mut Driver, combo: &str, item: &str) -> bool {
    d.click(combo);
    d.frames(1);
    let found = d.has(&format!("{combo}_{item}"));
    d.click(combo);
    found
}

/// Tools > Global Change, New.
fn open_new(d: &mut Driver, name: &str) {
    d.home();
    d.click("menu_bar_tools");
    d.click("tools_global_change");
    assert!(
        d.app.editing.dialogs.gc_list.is_some(),
        "Global Change opens"
    );
    d.click("gc_new");
    assert!(d.app.editing.dialogs.gc_edit.is_some(), "New opens Modify");
    type_into(d, "gc_edit_name", name);
}

/// The Crew value of every activity, by task id.
fn crews(d: &Driver) -> Vec<(i64, String)> {
    let p = &d.app.project;
    let catalog = columns::catalog(p);
    let crew = columns::find(&catalog, "udf:Crew").expect("the Crew column");
    let ix = d.app.grid_index();
    (0..p.model.activities.len())
        .map(|i| {
            let v = columns::value(p, &ix, crew, i);
            (
                p.model.activities[i].task_id,
                columns::display(p, Some(i), &v),
            )
        })
        .collect()
}

#[test]
fn workflow_global_change_heading_is_under_udf_parameters_and_move_arrows() {
    let mut d = Driver::new("qa_gc_round2", coded());
    let before_crews = crews(&d);
    let before_durations: Vec<i64> = d
        .app
        .project
        .model
        .activities
        .iter()
        .map(|a| d.app.project.details[&a.task_id].original_duration)
        .collect();
    open_new(&mut d, "Night crew");

    // 18.023, 18.024: the heading row joins every row; Any for this one.
    assert!(d.has("gc_if_heading"), "the If heading row is drawn");
    pick(&mut d, "gc_if_heading", "any");
    assert!(!edit(&d).gc.conditions[0].and, "Any of the following");
    d.click("gc_if_0");
    d.click("gc_edit_add");
    let e = edit(&d);
    assert_eq!(e.gc.conditions.len(), 2);
    assert!(
        !e.gc.conditions[1].and,
        "a new row takes the heading's join"
    );
    pick(&mut d, "gc_if_heading", "all");
    assert!(
        edit(&d).gc.conditions.iter().all(|c| c.and),
        "All joins both"
    );
    pick(&mut d, "gc_if_heading", "any");

    // 16.092: codes and user defined fields are If parameters.
    assert!(lists(&mut d, "gc_if_param_0", "code:Area"));
    assert!(lists(&mut d, "gc_if_param_0", "udf:Crew"));
    // 18.041: picking WBS starts on "is under", which a text field lacks.
    pick(&mut d, "gc_if_param_0", "wbs");
    assert_eq!(
        edit(&d).gc.conditions[0].op,
        "under",
        "WBS starts on is under"
    );
    assert!(lists(&mut d, "gc_if_op_0", "under"));
    pick(&mut d, "gc_if_param_1", "activity_name");
    assert!(
        !lists(&mut d, "gc_if_op_1", "under"),
        "no is under for text"
    );
    assert!(!lists(&mut d, "gc_if_op_1", "not_under"), "no is not under");
    pick(&mut d, "gc_if_op_1", "contains");
    type_into(&mut d, "gc_if_value_0", "RB.3");
    type_into(&mut d, "gc_if_value_1", "design");

    // 16.092: a user defined field is a Then parameter.
    assert!(lists(&mut d, "gc_Then_param_0", "udf:Crew"));
    {
        // The list is long; set it as a click in the list would.
        let e = d.app.editing.dialogs.gc_edit.as_mut().unwrap();
        e.gc.then[0].column = "udf:Crew".into();
    }
    type_into(&mut d, "gc_Then_value_0", "\"Night\"");
    d.click("gc_section_Then");
    d.click("gc_edit_add");
    {
        let e = d.app.editing.dialogs.gc_edit.as_mut().unwrap();
        assert_eq!(e.selected, (1, Some(1)), "Add selects the new Then row");
        e.gc.then[1].column = "original_duration".into();
        e.gc.then[1].operator = "x".into();
        e.gc.then[1].value2 = "2".into();
    }
    type_into(&mut d, "gc_Then_value_1", "Original Duration");

    // 18.036, 18.061: the move pad. The second (last) row moves up only.
    d.click("gc_Then_1");
    assert!(d.has("gc_move_up") && d.has("gc_move_down"));
    d.click("gc_move_down");
    assert_eq!(
        edit(&d).gc.then[1].column,
        "original_duration",
        "down is off"
    );
    d.click("gc_move_up");
    let e = edit(&d);
    assert_eq!(
        (e.gc.then[0].column.as_str(), e.gc.then[1].column.as_str()),
        ("original_duration", "udf:Crew"),
        "Move Up swaps the rows"
    );
    assert_eq!(e.selected, (1, Some(0)), "the moved row stays selected");
    d.click("gc_move_up");
    assert_eq!(edit(&d).gc.then[0].column, "original_duration", "up is off");
    d.click("gc_move_down");
    assert_eq!(edit(&d).gc.then[1].column, "original_duration");
    d.click("gc_move_up");
    // An If row moves too.
    d.click("gc_if_1");
    d.click("gc_move_up");
    let e = edit(&d);
    assert_eq!(e.gc.conditions[0].column, "activity_name");
    assert!(
        e.gc.conditions.iter().all(|c| !c.and),
        "the heading is kept"
    );
    d.click("gc_move_down");

    d.click("gc_edit_ok");
    assert!(d.app.editing.dialogs.gc_edit.is_none(), "{:?}", d.app.error);
    let k = d
        .app
        .editing
        .global_changes
        .iter()
        .position(|g| g.name == "Night crew")
        .expect("saved");
    d.click(&format!("gc_row_{k}"));
    d.click("gc_apply");
    let report = d
        .app
        .editing
        .dialogs
        .gc_report
        .as_ref()
        .expect("Apply Change opens the report")
        .changes
        .clone();
    // Any: the activities under RB.3 or whose name holds "design".
    let p = &d.app.project;
    let meets: BTreeSet<i64> = p
        .model
        .activities
        .iter()
        .enumerate()
        .filter(|(i, a)| {
            p.details[&a.task_id].wbs == 30 || p.name(*i).to_lowercase().contains("design")
        })
        .map(|(_, a)| a.task_id)
        .collect();
    let crewed: BTreeSet<i64> = report
        .iter()
        .filter(|c| c.column == "udf:Crew")
        .map(|c| c.task_id)
        .collect();
    assert_eq!(crewed, meets, "{report:#?}");
    assert!(report
        .iter()
        .filter(|c| c.column == "udf:Crew")
        .all(|c| c.new == "Night" && c.error.is_none()));
    let undo = d.app.history.counts().0;
    d.click("gc_commit");
    d.assert_clean("commit");
    assert_eq!(d.app.history.counts().0, undo + 1, "one undo step");
    for (task, crew) in crews(&d) {
        let want = if meets.contains(&task) { "Night" } else { "" };
        assert_eq!(crew, want, "task {task}");
    }
    d.click("undo");
    assert_eq!(crews(&d), before_crews, "undo restores the crews");
    let durations: Vec<i64> = d
        .app
        .project
        .model
        .activities
        .iter()
        .map(|a| d.app.project.details[&a.task_id].original_duration)
        .collect();
    assert_eq!(durations, before_durations, "and the durations");
}

/// A value as the table reads it: report text of the app and the command
/// line's text compare as values (the command line shows durations in
/// hours and dates as YYYY-MM-DD HH:MM; the app in the user's formats).
fn read_as(p: &ProjectData, kind: columns::Kind, text: &str) -> columns::Value {
    columns::parse_input(p, None, kind, text).unwrap_or_else(|e| panic!("{text:?}: {e}"))
}

#[test]
fn workflow_global_change_from_the_command_line_and_mcp_lists_the_report_lines() {
    let mut d = Driver::new("qa_gc_cli", coded());
    // The rule saved in the app: Any of WBS under RB.3, Area under N; then
    // double the duration and set the crew, else set the crew to Day.
    let rule = json!({
        "name": "Area crews",
        "conditions": [
            {"and": false, "column": "wbs", "op": "under", "value": "RB.3"},
            {"and": false, "column": "code:Area", "op": "under", "value": "N"}
        ],
        "then": [
            {"column": "original_duration", "value": "Original Duration", "operator": "x", "value2": "2"},
            {"column": "udf:Crew", "value": "\"Night\""}
        ],
        "otherwise": [{"column": "udf:Crew", "value": "\"Day\""}]
    });
    let g: crate::editing::gc::GlobalChange = serde_json::from_value(rule.clone()).unwrap();
    d.app.editing.global_changes = vec![g.clone()];
    d.app.save_global_changes();
    d.app.save();
    let desktop = d.app.global_change_preview(&g);
    assert!(desktop.len() > 14, "{desktop:#?}");
    let workspace = d.dir.join("workspace.db");

    // The command line, by its saved name, as a dry run.
    let run = |request: Json| {
        openxer_automation::parse_command(request).and_then(openxer_automation::dispatch)
    };
    let by_name = run(json!({"command":"edit","input":workspace,"dry_run":true,
        "operation":{"op":"global_change","name":"area crews"}}))
    .unwrap_or_else(|e| panic!("{}", e.message));
    let result = &by_name["report"]["results"][0];
    assert_eq!(result["name"], "Area crews");
    let rows = result["rows"].as_array().unwrap();
    // The same lines as the report: activity, field, old and new value.
    let p = &d.app.project;
    let catalog = columns::catalog(p);
    assert_eq!(rows.len(), desktop.len(), "{rows:#?}");
    for (row, line) in rows.iter().zip(&desktop) {
        assert_eq!(row["activity"], line.activity.as_str());
        assert_eq!(row["field"], line.title.as_str());
        assert_eq!(row["field_id"], line.column.as_str());
        assert_eq!(row["exception"], json!(line.error));
        let kind = columns::find(&catalog, &line.column).unwrap().kind;
        for (cli, app) in [(&row["old"], &line.old), (&row["new"], &line.new)] {
            assert_eq!(
                read_as(p, kind, cli.as_str().unwrap()),
                read_as(p, kind, app),
                "{} {}: {cli} against {app}",
                line.activity,
                line.title
            );
        }
    }
    let clean = desktop.iter().filter(|c| c.error.is_none()).count();
    assert_eq!(result["changed"], clean, "a dry run counts them");

    // MCP with the rule given inline writes a copy; the app's commit of the
    // same lines gives the same values.
    let output = d.dir.join("crews.xer");
    let mut server = openxer_automation::mcp::Server::default();
    let meta = json!({"io.modelcontextprotocol/protocolVersion":"2026-07-28",
        "io.modelcontextprotocol/clientCapabilities":{}});
    let arguments = json!({"input":workspace,"output":output,"rule":rule});
    let reply = server
        .handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
            "params":{"_meta":meta,"name":"openxer_global_change","arguments":arguments}}))
        .unwrap();
    let envelope = &reply["result"]["structuredContent"];
    assert_eq!(envelope["ok"], true, "{envelope:#}");
    assert_eq!(envelope["result"]["report"]["results"][0]["changed"], clean);
    let written = Document::read(&output).unwrap();
    let key = d.app.project.model.project.proj_id;
    let q = ProjectData::from_document(written, key, "crews".into()).unwrap();
    d.app.editing.dialogs.gc_report = Some(d.app.gc_report_for(&g.name, desktop.clone()));
    let undo = d.app.history.counts().0;
    d.click("gc_commit");
    assert_eq!(d.app.history.counts().0, undo + 1, "one undo step");
    for a in &d.app.project.model.activities {
        assert_eq!(
            q.details[&a.task_id].original_duration,
            d.app.project.details[&a.task_id].original_duration,
            "{}",
            a.task_code
        );
    }
    let fields = openxer_xer::edit::udf::udf_fields(&q.document, 1080, q.model.origin_serial);
    let crew = fields.iter().find(|f| f.label == "Crew");
    assert_eq!(crew.map(|f| f.text.as_str()), Some("Night"));
}
