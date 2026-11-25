//! Resource curves for agents: `curves` lists the file's curves and the
//! presets with their points, and `set_assignment` field `curve` sets one by
//! name from the command line and MCP, on the synthetic starter project.
use openxer_automation::{
    cli::{self, Invocation},
    dispatch,
    mcp::Server,
    parse_command, response,
};
use openxer_xer::edit::curves;
use openxer_xer::{starter, Document};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn source(dir: &Path) -> PathBuf {
    let mut p = starter();
    p.document.table_mut("RSRC").add(&[
        ("rsrc_id", "77".into()),
        ("rsrc_name", "Pipe crew".into()),
        ("rsrc_short_name", "PIPE".into()),
        ("rsrc_type", "RT_Labor".into()),
        ("def_qty_per_hr", "1".into()),
        ("clndr_id", "1".into()),
    ]);
    p.add_assignment(1020, Some(77), None).unwrap();
    p.add_assignment(1040, Some(77), None).unwrap();
    let path = dir.join("source.xer");
    p.updated_document().write(&path).unwrap();
    path
}

fn cli(args: &[&str]) -> Value {
    let Invocation::Run(command) = cli::parse(args.iter().copied()).unwrap() else {
        panic!("not a command")
    };
    response(dispatch(*command))
}

fn curve_of(path: &Path, task: &str) -> Option<String> {
    let doc = Document::read(path).unwrap();
    let t = doc.table("TASKRSRC").unwrap();
    let row = t
        .rows
        .iter()
        .position(|r| t.value(r, "task_id") == task && t.value(r, "rsrc_id") == "77")
        .unwrap();
    curves::of_assignment(&doc, row).and_then(|id| curves::by_id(&doc, id).map(|c| c.name))
}

fn code_of(path: &Path, task_id: &str) -> String {
    let doc = Document::read(path).unwrap();
    let t = doc.table("TASK").unwrap();
    t.rows
        .iter()
        .find(|r| t.value(r, "task_id") == task_id)
        .map(|r| t.value(r, "task_code").to_string())
        .unwrap()
}

#[test]
fn curves_lists_presets_with_points_and_the_cli_sets_one_by_name() {
    let dir = tempfile::tempdir().unwrap();
    let input = source(dir.path());
    let listed = cli(&["curves", "--input", input.to_str().unwrap()]);
    assert_eq!(listed["ok"], true, "{listed}");
    let list = listed["result"]["curves"].as_array().unwrap();
    assert_eq!(list.len(), curves::presets().len());
    let bell = list.iter().find(|c| c["name"] == "Bell, Centered").unwrap();
    assert_eq!(bell["kind"], "preset");
    assert_eq!(bell["in_file"], false);
    assert_eq!(bell["points"].as_array().unwrap().len(), 21);
    let total: f64 = bell["points"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .sum();
    assert!((total - 100.).abs() < 1e-9);
    assert_eq!(listed["result"]["applied_to_spreading"], true);

    let activity = code_of(&input, "1020");
    let out = dir.path().join("curved.xer");
    let set = cli(&[
        "set-assignment",
        "--input",
        input.to_str().unwrap(),
        "--activity",
        &activity,
        "--resource",
        "PIPE",
        "--field",
        "curve",
        "--value",
        "Bell, Centered",
        "--output",
        out.to_str().unwrap(),
    ]);
    assert_eq!(set["ok"], true, "{set}");
    assert_eq!(curve_of(&out, "1020").as_deref(), Some("Bell, Centered"));
    assert_eq!(curve_of(&out, "1040"), None);

    // The saved file now holds the preset once and lists it once.
    let again = cli(&["curves", "--input", out.to_str().unwrap()]);
    let list = again["result"]["curves"].as_array().unwrap();
    let bells: Vec<&Value> = list
        .iter()
        .filter(|c| c["name"] == "Bell, Centered")
        .collect();
    assert_eq!(bells.len(), 1);
    assert_eq!(bells[0]["in_file"], true);
    assert_eq!(bells[0]["kind"], "preset");
    assert_eq!(bells[0]["assignments"], 1);

    // The same edit through MCP, exact mode, gives the same row.
    let mcp_out = dir.path().join("mcp.xer");
    let mut server = Server::default();
    let meta = json!({"io.modelcontextprotocol/protocolVersion":"2026-07-28",
        "io.modelcontextprotocol/clientCapabilities":{}});
    let reply = server
        .handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
            "params":{"_meta":meta,"name":"openxer_set_assignment","arguments":{
                "input":input,"output":mcp_out,"activity":activity,"resource":"PIPE",
                "field":"curve","value":"bell, centered","recalculate":false}}}))
        .unwrap();
    assert_eq!(reply["result"]["structuredContent"]["ok"], true, "{reply}");
    assert_eq!(
        curve_of(&mcp_out, "1020").as_deref(),
        Some("Bell, Centered")
    );
    let listed = server
        .handle(json!({"jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{"_meta":meta,"name":"openxer_curves","arguments":{"input":mcp_out}}}))
        .unwrap();
    assert_eq!(
        listed["result"]["structuredContent"]["ok"], true,
        "{listed}"
    );

    // An unknown name fails and writes nothing; null clears.
    let bad = dir.path().join("bad.xer");
    let refused = response(
        parse_command(json!({"command":"apply","input":out,"output":bad,
            "changes":{"format":"openxer-changes/1","operations":[
                {"op":"set_assignment","activity":activity,"resource":"PIPE","field":"curve","value":"Wobbly"}]}}))
        .and_then(dispatch),
    );
    assert_eq!(refused["ok"], false);
    assert!(!bad.exists());
    let cleared = dir.path().join("cleared.xer");
    let ok = response(
        parse_command(json!({"command":"apply","input":out,"output":cleared,
            "changes":{"format":"openxer-changes/1","operations":[
                {"op":"set_assignment","activity":activity,"resource":"PIPE","field":"curve","value":null}]}}))
        .and_then(dispatch),
    );
    assert_eq!(ok["ok"], true, "{ok}");
    assert_eq!(curve_of(&cleared, "1020"), None);

    assert!(cli::HELP.contains("openxer-cli curves"));
}

#[test]
fn usage_spreads_a_curved_assignment_and_reports_its_rules() {
    let dir = tempfile::tempdir().unwrap();
    let unscheduled = source(dir.path());
    let input = dir.path().join("scheduled.xer");
    let scheduled = cli(&[
        "schedule",
        "--input",
        unscheduled.to_str().unwrap(),
        "--output",
        input.to_str().unwrap(),
    ]);
    assert_eq!(scheduled["ok"], true, "{scheduled}");
    let activity = code_of(&input, "1020");
    let curved = dir.path().join("curved.xer");
    let set = cli(&[
        "set-assignment",
        "--input",
        input.to_str().unwrap(),
        "--activity",
        &activity,
        "--resource",
        "PIPE",
        "--field",
        "curve",
        "--value",
        "Early Weighted",
        "--output",
        curved.to_str().unwrap(),
    ]);
    assert_eq!(set["ok"], true, "{set}");
    assert_eq!(
        set["result"]["report"]["results"][0]["curve_applies"], true,
        "{set}"
    );

    let read = |path: &Path| {
        cli(&[
            "usage",
            "--input",
            path.to_str().unwrap(),
            "--activity",
            &activity,
            "--interval",
            "day",
        ])
    };
    let even = read(&input);
    let shaped = read(&curved);
    assert_eq!(shaped["ok"], true, "{shaped}");
    let row = |v: &Value| v["result"]["assignments"][0].clone();
    assert_eq!(row(&even)["shaped"], false);
    assert_eq!(row(&shaped)["shaped"], true);
    assert_eq!(row(&shaped)["curve"], "Early Weighted");
    assert_eq!(shaped["result"]["verified"], false);
    // The totals keep the stored units; the curve moves them to the start.
    let per = |v: &Value| -> Vec<f64> {
        row(v)["remaining"]["per_period"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_f64().unwrap())
            .filter(|x| *x > 0.)
            .collect()
    };
    let (e, s) = (per(&even), per(&shaped));
    let stored = row(&shaped)["stored"]["remaining"].as_f64().unwrap();
    assert!(stored > 0.);
    assert!((e.iter().sum::<f64>() - stored).abs() < 1e-9, "{even}");
    assert!((s.iter().sum::<f64>() - stored).abs() < 1e-9);
    assert!(e.windows(2).all(|w| (w[0] - w[1]).abs() < 1e-9), "{e:?}");
    assert!(s[0] > e[0] && s[s.len() - 1] < e[e.len() - 1], "{s:?}");
    assert!(s.windows(2).all(|w| w[0] >= w[1]), "{s:?}");

    // Fixed Units ignores the curve: an even spread again, and the set
    // result says so.
    let fixed_units = dir.path().join("fixed.xer");
    let set = cli(&[
        "set-activity",
        "--input",
        curved.to_str().unwrap(),
        "--activity",
        &activity,
        "--field",
        "duration_type",
        "--value",
        "DT_FixedQty",
        "--output",
        fixed_units.to_str().unwrap(),
    ]);
    assert_eq!(set["ok"], true, "{set}");
    let ignored = read(&fixed_units);
    assert_eq!(row(&ignored)["shaped"], false);
    assert_eq!(row(&ignored)["curve_applies"], false);
    assert_eq!(row(&ignored)["curve"], "Early Weighted");
    let again = dir.path().join("again.xer");
    let set = cli(&[
        "set-assignment",
        "--input",
        fixed_units.to_str().unwrap(),
        "--activity",
        &activity,
        "--resource",
        "PIPE",
        "--field",
        "curve",
        "--value",
        "Ramp Up",
        "--output",
        again.to_str().unwrap(),
    ]);
    assert_eq!(
        set["result"]["report"]["results"][0]["curve_applies"], false,
        "{set}"
    );
    assert!(
        set["result"]["report"]["results"][0]["note"].is_string(),
        "{set}"
    );

    // By resource and by week through MCP; weeks start on Monday.
    let mut server = Server::default();
    let meta = json!({"io.modelcontextprotocol/protocolVersion":"2026-07-28",
        "io.modelcontextprotocol/clientCapabilities":{}});
    let reply = server
        .handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
            "params":{"_meta":meta,"name":"openxer_usage","arguments":{
                "input":curved,"resource":"PIPE","interval":"week"}}}))
        .unwrap();
    let result = &reply["result"]["structuredContent"]["result"];
    assert_eq!(reply["result"]["structuredContent"]["ok"], true, "{reply}");
    assert_eq!(result["assignments"].as_array().unwrap().len(), 2);
    let start = result["periods"][0]["start"].as_str().unwrap();
    assert!(start.ends_with("00:00"), "{start}");
    let monday = openxer_xer::parse_absolute(start).unwrap().div_euclid(1440);
    assert_eq!((monday - 2).rem_euclid(7), 0, "{start} is a Monday");
    assert!(openxer_automation::mcp::tool_definitions()
        .iter()
        .any(|t| t["name"] == "openxer_usage"));

    // Neither an activity nor a resource is refused.
    let refused = cli(&["usage", "--input", input.to_str().unwrap()]);
    assert_eq!(refused["ok"], false);
    assert!(cli::HELP.contains("openxer-cli usage"));
}

#[test]
fn usage_totals_plus_unplaced_equal_the_stored_quantities() {
    // Two PIPE assignments with quantities that do not divide evenly (their
    // own durations differ from their activities'), one Early Weighted.
    // Over the full range every series adds up to the stored quantity
    // exactly, by day, week and month; over a narrower range the rest is
    // reported as unplaced.
    let dir = tempfile::tempdir().unwrap();
    let mut p = starter();
    p.document.table_mut("RSRC").add(&[
        ("rsrc_id", "77".into()),
        ("rsrc_name", "Pipe crew".into()),
        ("rsrc_short_name", "PIPE".into()),
        ("rsrc_type", "RT_Labor".into()),
        ("def_qty_per_hr", "1".into()),
        ("clndr_id", "1".into()),
    ]);
    for (task, qty) in [(1020, "37.3"), (1040, "61.7")] {
        p.add_assignment(task, Some(77), None).unwrap();
        let row = p.assignment_rows(task)[0];
        let t = p.document.table_mut("TASKRSRC");
        for field in ["target_qty", "remain_qty"] {
            t.set(row, field, qty.into());
        }
        t.set(row, "target_cost", "1234.56".into());
        t.set(row, "remain_cost", "1234.56".into());
    }
    let unscheduled = dir.path().join("source.xer");
    p.updated_document().write(&unscheduled).unwrap();
    let input = dir.path().join("scheduled.xer");
    let scheduled = cli(&[
        "schedule",
        "--input",
        unscheduled.to_str().unwrap(),
        "--output",
        input.to_str().unwrap(),
    ]);
    assert_eq!(scheduled["ok"], true, "{scheduled}");
    let curved = dir.path().join("curved.xer");
    let set = cli(&[
        "set-assignment",
        "--input",
        input.to_str().unwrap(),
        "--activity",
        &code_of(&input, "1020"),
        "--resource",
        "PIPE",
        "--field",
        "curve",
        "--value",
        "Early Weighted",
        "--output",
        curved.to_str().unwrap(),
    ]);
    assert_eq!(set["ok"], true, "{set}");
    let read = |extra: &[&str]| {
        let mut args = vec![
            "usage",
            "--input",
            curved.to_str().unwrap(),
            "--resource",
            "PIPE",
        ];
        args.extend_from_slice(extra);
        let v = cli(&args);
        assert_eq!(v["ok"], true, "{v}");
        v["result"].clone()
    };
    let stored_of = |a: &Value, series: &str| {
        let key = if series == "remaining_late" {
            "remaining"
        } else {
            series
        };
        a["stored"][key].as_f64().unwrap()
    };
    const SERIES: [&str; 4] = ["budget", "actual", "remaining", "remaining_late"];
    for costs in ["false", "true"] {
        for interval in ["day", "week", "month"] {
            let r = read(&["--interval", interval, "--costs", costs]);
            let rows = r["assignments"].as_array().unwrap();
            assert_eq!(rows.len(), 2);
            assert!(rows.iter().any(|a| a["shaped"] == true));
            for series in SERIES {
                let mut stored_sum = 0.;
                for a in rows {
                    let stored = stored_of(a, series);
                    stored_sum += stored;
                    let total = a[series]["total"].as_f64().unwrap();
                    assert_eq!(total, stored, "{interval} {series} costs {costs}: {a}");
                    assert_eq!(a[series]["unplaced"].as_f64().unwrap(), 0.);
                }
                let t = &r["totals"][series];
                let total = t["total"].as_f64().unwrap();
                let unplaced = t["unplaced"].as_f64().unwrap();
                assert!((total + unplaced - stored_sum).abs() < 1e-9, "{t}");
                assert!(unplaced.abs() < 1e-9, "{t}");
            }
        }
    }
    // A range that misses the end of the work: what falls outside is
    // unplaced, and total + unplaced is still the stored quantity.
    let full = read(&["--interval", "day"]);
    let first = full["periods"][0]["start"].as_str().unwrap().to_string();
    let third = full["periods"][2]["finish"].as_str().unwrap().to_string();
    let part = read(&["--interval", "day", "--from", &first, "--to", &third]);
    let mut outside = 0.;
    for a in part["assignments"].as_array().unwrap() {
        for series in SERIES {
            let total = a[series]["total"].as_f64().unwrap();
            let unplaced = a[series]["unplaced"].as_f64().unwrap();
            assert!(
                (total + unplaced - stored_of(a, series)).abs() < 1e-9,
                "{a}"
            );
            outside += unplaced;
        }
    }
    assert!(outside > 1., "{part}");
}
