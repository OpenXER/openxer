//! `eps list` and the EPS edits through `dispatch`, the command line and MCP,
//! on a workspace imported from the synthetic two-project starter file.
use openxer_automation::{
    cli::{self, Invocation},
    dispatch,
    mcp::Server,
    parse_command, response,
};
use openxer_xer::starter_portfolio;
use openxer_xer::workspace::rollup;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn run(request: Value) -> Value {
    response(parse_command(request).and_then(dispatch))
}

fn ok(v: &Value) -> &Value {
    assert_eq!(v["ok"], true, "{v:#}");
    &v["result"]
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("openxer-eps-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A workspace holding RIVERBEND and NORTHYARD.
fn workspace(dir: &Path) -> PathBuf {
    let xer = dir.join("portfolio.xer");
    starter_portfolio().write(&xer).unwrap();
    let db = dir.join("ws.db");
    ok(&run(json!({"command":"import","input":xer,"output":db})));
    db
}

fn list(db: &Path) -> Value {
    ok(&run(json!({"command":"eps_list","input":db}))).clone()
}

fn folder<'a>(l: &'a Value, code: &str) -> &'a Value {
    l["folders"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["eps_id"] == code)
        .unwrap_or_else(|| panic!("no folder {code} in {l:#}"))
}

fn edit(db: &Path, out: &Path, op: Value) -> Value {
    run(json!({"command":"eps","input":db,"output":out,"operation":op}))
}

#[test]
fn list_gives_every_project_and_folder_rollup() {
    let dir = temp("list");
    let db = workspace(&dir);
    let l = list(&db);
    let projects = l["projects"].as_array().unwrap();
    assert_eq!(projects.len(), 2);
    let top = folder(&l, "WORKSPACE");
    assert_eq!(top["projects"].as_array().unwrap().len(), 2);
    // The folder adds its projects up, with the shared rules.
    let doc = starter_portfolio();
    let mut want = rollup::Rollup::default();
    for (pid, _) in doc.projects() {
        want.add(&rollup::of_document(&doc, pid).unwrap());
    }
    assert_eq!(top["rollup"]["activities"], want.activities);
    assert_eq!(top["rollup"]["not_started"], want.not_started);
    let sum: u64 = projects
        .iter()
        .map(|p| p["rollup"]["activities"].as_u64().unwrap())
        .sum();
    assert_eq!(top["rollup"]["activities"], sum);
    assert_eq!(
        top["rollup"]["start"],
        json!(openxer_xer::date_string(0, want.start))
    );
    assert_eq!(
        top["rollup"]["schedule_percent_complete"],
        json!(want.schedule_percent())
    );
    assert!(l["rules"].as_str().unwrap().contains("duration based"));
    // An XER input is refused: the EPS lives in workspaces.
    let xer = dir.join("portfolio.xer");
    assert_eq!(run(json!({"command":"eps_list","input":xer}))["ok"], false);
}

#[test]
fn folders_are_added_filed_moved_renamed_and_deleted_on_new_copies() {
    let dir = temp("edit");
    let db = workspace(&dir);
    let source = std::fs::read(&db).unwrap();
    // A dry run writes nothing.
    let dry = run(
        json!({"command":"eps","input":db,"dry_run":true,"operation":{"op":"add_folder","name":"East Region","eps_id":"EAST","parent":"WORKSPACE"}}),
    );
    assert_eq!(ok(&dry)["result"]["added"], "EAST");
    let a = dir.join("a.db");
    ok(&edit(
        &db,
        &a,
        json!({"op":"add_folder","name":"East Region","eps_id":"EAST","parent":"WORKSPACE"}),
    ));
    assert_eq!(
        std::fs::read(&db).unwrap(),
        source,
        "the input is untouched"
    );
    assert!(
        edit(&db, &a, json!({"op":"add_folder","name":"X"}))["ok"] == false,
        "existing output refused"
    );
    // File NORTHYARD under EAST.
    let l = list(&a);
    let north = l["projects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["project_id"] == "NORTHYARD")
        .unwrap()["workspace_id"]
        .as_i64()
        .unwrap();
    let b = dir.join("b.db");
    ok(&edit(
        &a,
        &b,
        json!({"op":"file_project","workspace_id":north,"folder":"east"}),
    ));
    let l = list(&b);
    let east = folder(&l, "EAST");
    assert_eq!(east["projects"], json!([north]));
    assert_eq!(east["path"], json!(["Workspace", "East Region"]));
    assert_eq!(east["rollup"]["projects"], 1);
    // The top folder still rolls up both, its child folder included.
    assert_eq!(folder(&l, "WORKSPACE")["rollup"]["projects"], 2);
    // A new project in EAST with the wizard's rules.
    let c = dir.join("c.db");
    let made = edit(
        &b,
        &c,
        json!({"op":"create_project","folder":"EAST","project_id":"TA-27","name":"Turnaround 2027","planned_start":"2027-01-04","must_finish":"2027-03-30","rate_type":"Price / Unit2"}),
    );
    let made = ok(&made);
    assert_eq!(made["result"]["planned_start"], "2027-01-04 08:00");
    assert_eq!(made["result"]["must_finish_by"], "2027-03-30 17:00");
    assert_eq!(made["result"]["rate_type"], "COST_PER_QTY2");
    let l = list(&c);
    assert_eq!(folder(&l, "EAST")["rollup"]["projects"], 2);
    let refused = edit(
        &c,
        &dir.join("d.db"),
        json!({"op":"create_project","folder":"EAST","project_id":"ta-27","planned_start":"2027-01-04"}),
    );
    assert_eq!(refused["ok"], false);
    assert!(refused["error"]["message"]
        .as_str()
        .unwrap()
        .contains("already exists"));
    // Rename, move to the top level, then delete: the folder goes with the
    // projects filed in it (folder_delete_takes_its_projects_and_baselines).
    let e = dir.join("e.db");
    ok(&edit(
        &c,
        &e,
        json!({"op":"rename_folder","folder":"EAST","name":"Eastern Region","new_eps_id":"EASTERN"}),
    ));
    let f = dir.join("f.db");
    ok(&edit(
        &e,
        &f,
        json!({"op":"move_folder","folder":"EASTERN"}),
    ));
    let l = list(&f);
    assert_eq!(folder(&l, "EASTERN")["depth"], 0);
    let g = dir.join("g.db");
    let gone = edit(
        &f,
        &g,
        json!({"op":"delete_folder","folder":"EASTERN","delete_projects":true}),
    );
    assert_eq!(ok(&gone)["result"]["deleted"], json!(["EASTERN"]));
    assert_eq!(
        ok(&gone)["result"]["projects_deleted"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let l = list(&g);
    assert_eq!(
        folder(&l, "WORKSPACE")["projects"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    // The last folder stays.
    let last = edit(
        &g,
        &dir.join("h.db"),
        json!({"op":"delete_folder","folder":"WORKSPACE","delete_projects":true}),
    );
    assert_eq!(last["ok"], false);
    // The new workspace still opens as a workspace with the project left.
    let inspect = ok(&run(json!({"command":"inspect","input":g}))).clone();
    assert_eq!(inspect["projects"].as_array().unwrap().len(), 1);
    let rid = l["projects"][0]["workspace_id"].clone();
    assert_eq!(l["projects"][0]["project_id"], "RIVERBEND");
    let loaded = run(json!({"command":"load","input":g,"workspace_id":rid}));
    assert_eq!(loaded["ok"], true, "{loaded:#}");
    let _ = std::fs::remove_dir_all(dir);
}

/// Deleting a folder takes the folders under it and every project filed in
/// them, with their baselines and Assign Baselines rows, as the app's
/// Delete does. Without delete_projects a folder holding projects is
/// refused and nothing is written; a dry run lists what would go.
#[test]
fn folder_delete_takes_its_projects_and_baselines() {
    let dir = temp("delete");
    let db = workspace(&dir);
    let id_of = |l: &Value, code: &str| {
        l["projects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["project_id"] == code)
            .unwrap()["workspace_id"]
            .as_i64()
            .unwrap()
    };
    let l = list(&db);
    let (river, north) = (id_of(&l, "RIVERBEND"), id_of(&l, "NORTHYARD"));
    // PLANT with a child UNIT1: NORTHYARD in PLANT, RIVERBEND in UNIT1, and
    // a baseline of RIVERBEND assigned as its primary.
    let a = dir.join("a.db");
    ok(&edit(
        &db,
        &a,
        json!({"op":"add_folder","name":"Plant","eps_id":"PLANT"}),
    ));
    let b = dir.join("b.db");
    ok(&edit(
        &a,
        &b,
        json!({"op":"add_folder","name":"Unit 1","eps_id":"UNIT1","parent":"PLANT"}),
    ));
    let c = dir.join("c.db");
    ok(&edit(
        &b,
        &c,
        json!({"op":"file_project","workspace_id":north,"folder":"PLANT"}),
    ));
    let d = dir.join("d.db");
    ok(&edit(
        &c,
        &d,
        json!({"op":"file_project","workspace_id":river,"folder":"UNIT1"}),
    ));
    let e = dir.join("e.db");
    ok(&run(
        json!({"command":"baseline","input":d,"output":e,"workspace_id":river,"operation":{"op":"add"}}),
    ));
    let e2 = dir.join("e2.db");
    ok(&run(
        json!({"command":"baseline","input":e,"output":e2,"workspace_id":river,"operation":{"op":"assign","primary":"RIVERBEND - B1"}}),
    ));
    let e = e2;
    let rows = |db: &Path, sql: &str| -> i64 {
        let c = rusqlite::Connection::open(db).unwrap();
        c.query_row(sql, [], |r| r.get(0)).unwrap()
    };
    assert_eq!(rows(&e, "SELECT COUNT(*) FROM baselines"), 1);
    assert_eq!(rows(&e, "SELECT COUNT(*) FROM baseline_assign"), 1);
    // Without delete_projects: refused, nothing written.
    let out = dir.join("out.db");
    let refused = edit(&e, &out, json!({"op":"delete_folder","folder":"PLANT"}));
    assert_eq!(refused["ok"], false, "{refused:#}");
    assert_eq!(refused["error"]["code"], "refused");
    let message = refused["error"]["message"].as_str().unwrap();
    assert!(
        message.contains("2 project(s) (4 item(s)") && message.contains("1 baseline(s)"),
        "{message}"
    );
    assert!(!out.exists(), "nothing is written");
    // A dry run lists what would go.
    let dry = run(
        json!({"command":"eps","input":e,"dry_run":true,"operation":{"op":"delete_folder","folder":"PLANT"}}),
    );
    let dry = &ok(&dry)["result"];
    assert_eq!(dry["deleted"], json!(["PLANT", "UNIT1"]));
    assert_eq!(dry["items"], 4);
    assert_eq!(dry["baselines_deleted"], 1);
    // With it: both folders, both projects and the baseline go.
    let gone = edit(
        &e,
        &out,
        json!({"op":"delete_folder","folder":"PLANT","delete_projects":true}),
    );
    let result = &ok(&gone)["result"];
    let mut ids: Vec<i64> = result["projects_deleted"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["workspace_id"].as_i64().unwrap())
        .collect();
    ids.sort();
    let mut want = vec![river, north];
    want.sort();
    assert_eq!(ids, want);
    assert_eq!(rows(&out, "SELECT COUNT(*) FROM projects"), 0);
    assert_eq!(rows(&out, "SELECT COUNT(*) FROM baselines"), 0);
    assert_eq!(rows(&out, "SELECT COUNT(*) FROM baseline_assign"), 0);
    let l = list(&out);
    let codes: Vec<&str> = l["folders"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["eps_id"].as_str().unwrap())
        .collect();
    assert_eq!(codes, ["WORKSPACE"]);
    // An empty folder needs no flag.
    let f = dir.join("f.db");
    ok(&edit(
        &out,
        &f,
        json!({"op":"add_folder","name":"Spare","eps_id":"SPARE"}),
    ));
    ok(&edit(
        &f,
        &dir.join("g.db"),
        json!({"op":"delete_folder","folder":"SPARE"}),
    ));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_command_line_and_mcp_reach_the_same_commands() {
    let parsed = cli::parse(["eps", "list", "--input", "w.db"]).unwrap();
    assert!(matches!(parsed, Invocation::Run(_)));
    let parsed = cli::parse([
        "eps",
        "file-project",
        "--input",
        "w.db",
        "--output",
        "n.db",
        "--workspace-id",
        "4",
        "--folder",
        "EAST",
    ])
    .unwrap();
    match parsed {
        Invocation::Run(c) => {
            let text = format!("{c:?}");
            assert!(
                text.contains("FileProject") && text.contains("workspace_id: 4"),
                "{text}"
            );
        }
        _ => panic!("not a run"),
    }
    // delete-folder takes --delete-projects on its own or with true/false.
    for (args, want) in [
        (vec!["--delete-projects"], true),
        (vec!["--delete-projects", "false"], false),
        (vec![], false),
    ] {
        let mut line = vec![
            "eps",
            "delete-folder",
            "--input",
            "w.db",
            "--output",
            "n.db",
            "--folder",
            "EAST",
        ];
        line.extend(args);
        match cli::parse(line).unwrap() {
            Invocation::Run(c) => {
                let text = format!("{c:?}");
                assert!(text.contains(&format!("delete_projects: {want}")), "{text}");
            }
            _ => panic!("not a run"),
        }
    }
    assert!(cli::HELP.contains("--delete-projects"));
    assert!(cli::parse(["eps", "explode", "--input", "w.db"]).is_err());
    assert!(
        cli::parse(["eps", "add-folder", "--input", "w.db", "--output", "n.db"]).is_err(),
        "--name is required"
    );
    assert!(matches!(
        cli::parse(["eps", "--help"]).unwrap(),
        Invocation::Help
    ));
    assert!(cli::HELP.contains("eps create-project"));
    // MCP lists both tools and runs them.
    let dir = temp("mcp");
    let db = workspace(&dir);
    let meta = json!({"io.modelcontextprotocol/protocolVersion":openxer_automation::mcp::PROTOCOL_VERSION,"io.modelcontextprotocol/clientCapabilities":{}});
    let mut server = Server::default();
    let tools = server
        .handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{"_meta":meta}}))
        .unwrap();
    let names: Vec<String> = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect();
    assert!(names.contains(&"openxer_eps_list".to_string()));
    assert!(names.contains(&"openxer_eps_edit".to_string()));
    let listed = server
        .handle(json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"_meta":meta,"name":"openxer_eps_list","arguments":{"input":db}}}))
        .unwrap();
    assert_eq!(listed["result"]["isError"], false, "{listed:#}");
    let out = dir.join("m.db");
    let added = server
        .handle(json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"_meta":meta,"name":"openxer_eps_edit","arguments":{"input":db,"output":out,"op":"add_folder","name":"West"}}}))
        .unwrap();
    assert_eq!(added["result"]["isError"], false, "{added:#}");
    assert!(list(&out)["folders"].as_array().unwrap().len() == 2);
    // delete_folder through MCP: a folder holding projects needs
    // delete_projects, then takes them.
    let both = dir.join("both.db");
    let filed = server
        .handle(json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"_meta":meta,"name":"openxer_eps_edit","arguments":{"input":out,"output":both,"op":"file_project","workspace_id":list(&out)["projects"][0]["workspace_id"],"folder":list(&out)["folders"][1]["eps_id"]}}}))
        .unwrap();
    assert_eq!(filed["result"]["isError"], false, "{filed:#}");
    let west = list(&both)["folders"][1]["eps_id"].clone();
    let call = |id: i64, args: Value| {
        let mut server = Server::default();
        server
            .handle(json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"_meta":meta,"name":"openxer_eps_edit","arguments":args}}))
            .unwrap()
    };
    let refused = call(
        5,
        json!({"input":both,"output":dir.join("r.db"),"op":"delete_folder","folder":west}),
    );
    assert_eq!(refused["result"]["isError"], true, "{refused:#}");
    let gone_db = dir.join("gone.db");
    let gone = call(
        6,
        json!({"input":both,"output":gone_db,"op":"delete_folder","folder":west,"delete_projects":true}),
    );
    assert_eq!(gone["result"]["isError"], false, "{gone:#}");
    assert_eq!(list(&gone_db)["projects"].as_array().unwrap().len(), 1);
    let _ = std::fs::remove_dir_all(dir);
}

/// The synthetic sample's hand-computed values (see the xer workspace
/// tests) come out of `eps list` too: ALPHA 46.67 %, the folder 28 %.
#[test]
fn list_reports_the_hand_computed_sample_values() {
    use openxer_xer::workspace::sample;
    let dir = temp("sample");
    let db = workspace(&dir);
    {
        let c = rusqlite::Connection::open(&db).unwrap();
        for (source, key, title, doc) in [
            ("alpha.xer", 1, "ALPHA", sample::project_a()),
            ("bravo.xer", 7, "BRAVO", sample::project_b()),
        ] {
            c.execute(
                "INSERT INTO projects(source_name,project_key,title,xer_content) VALUES(?1,?2,?3,?4)",
                rusqlite::params![source, key, title, doc.text()],
            )
            .unwrap();
        }
    }
    let a = dir.join("a.db");
    ok(&edit(
        &db,
        &a,
        json!({"op":"add_folder","name":"East Region","eps_id":"EAST"}),
    ));
    let l = list(&a);
    let id = |code: &str| {
        l["projects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["project_id"] == code)
            .unwrap()["workspace_id"]
            .clone()
    };
    let (alpha, bravo) = (id("ALPHA"), id("BRAVO"));
    let b = dir.join("b.db");
    ok(&edit(
        &a,
        &b,
        json!({"op":"file_project","workspace_id":alpha,"folder":"EAST"}),
    ));
    let c = dir.join("c.db");
    ok(&edit(
        &b,
        &c,
        json!({"op":"file_project","workspace_id":bravo,"folder":"EAST"}),
    ));
    let l = list(&c);
    let alpha = l["projects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["project_id"] == "ALPHA")
        .unwrap();
    assert_eq!(alpha["status"], "WS_Open");
    assert_eq!(alpha["name"], "Alpha Unit Revamp");
    assert_eq!(alpha["rollup"]["schedule_percent_complete"], json!(46.67));
    assert_eq!(alpha["rollup"]["total_float_hours"], json!(8.0));
    assert_eq!(alpha["rollup"]["costs"]["budgeted"], json!(5500.0));
    let east = folder(&l, "EAST")["rollup"].clone();
    assert_eq!(east["schedule_percent_complete"], json!(28.0));
    assert_eq!(east["total_float_hours"], json!(-8.0));
    assert_eq!(east["data_date"], "2027-01-13 08:00");
    assert_eq!(east["start"], "2027-01-04 08:00");
    assert_eq!(east["finish"], "2027-02-05 17:00");
    assert_eq!(east["activities"], 4);
    let _ = std::fs::remove_dir_all(dir);
}

/// A workspace moved to the shared dictionary (schema 3) keeps the
/// calendars in the dictionary; the rollups are the same, folder edits and
/// create_project work.
#[test]
fn a_schema_2_workspace_rolls_up_the_same_and_keeps_folder_edits() {
    use openxer_xer::workspace::sample;
    let dir = temp("schema2");
    let db = workspace(&dir);
    {
        let c = rusqlite::Connection::open(&db).unwrap();
        c.execute(
            "INSERT INTO projects(source_name,project_key,title,xer_content) VALUES('alpha.xer',1,'ALPHA',?1)",
            [sample::project_a().text()],
        )
        .unwrap();
    }
    let alpha = |l: &Value| {
        l["projects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["project_id"] == "ALPHA")
            .unwrap()["rollup"]
            .clone()
    };
    let before = alpha(&list(&db));
    let v2 = dir.join("v2.db");
    ok(&run(
        json!({"command":"workspace_migrate","input":db,"output":v2}),
    ));
    let after = list(&v2);
    // The migration may rename a calendar to keep names unique.
    let strip = |mut v: Value| {
        v.as_object_mut().unwrap().remove("working_time_calendar");
        v
    };
    assert_eq!(
        strip(alpha(&after)),
        strip(before),
        "same rollup from the dictionary"
    );
    assert_eq!(alpha(&after)["schedule_percent_complete"], json!(46.67));
    let a = dir.join("a.db");
    ok(&edit(
        &v2,
        &a,
        json!({"op":"add_folder","name":"East Region","eps_id":"EAST"}),
    ));
    assert_eq!(folder(&list(&a), "EAST")["depth"], 0);
    // create_project works there too: the new project's calendar goes to
    // the dictionary, and it opens with it.
    let b = dir.join("b.db");
    let made = edit(
        &a,
        &b,
        json!({"op":"create_project","folder":"EAST","project_id":"N-1","planned_start":"2027-01-04"}),
    );
    ok(&made);
    let id = made["result"]["result"]["workspace_id"].as_i64().unwrap();
    let loaded = run(json!({"command":"load","input":b,"workspace_id":id}));
    assert_eq!(loaded["ok"], true, "{loaded:#}");
    let c = rusqlite::Connection::open(&b).unwrap();
    let text: String = c
        .query_row("SELECT xer_content FROM projects WHERE id=?1", [id], |r| {
            r.get(0)
        })
        .unwrap();
    assert!(
        !openxer_xer::Document::parse(&text)
            .unwrap()
            .table("CALENDAR")
            .is_some_and(|t| t.rows.iter().any(|r| t.value(r, "clndr_type") == "CA_Base")),
        "the stored text keeps no global calendar"
    );
    let _ = std::fs::remove_dir_all(dir);
}
