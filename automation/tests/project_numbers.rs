//! Every project the workspace makes gets an internal project number (the
//! XER `proj_id`) no other workspace project or baseline uses: `eps
//! create-project` through the command line and MCP, and `baseline restore`.
//! Relationships between projects name the other project by this number.
use openxer_automation::{dispatch, mcp::Server, parse_command, response};
use openxer_xer::{starter_portfolio, Document};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn run(request: Value) -> Value {
    response(parse_command(request).and_then(dispatch))
}

#[track_caller]
fn ok(v: &Value) -> &Value {
    assert_eq!(v["ok"], true, "{v:#}");
    &v["result"]
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("openxer-numbers-{name}-{}", std::process::id()));
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

/// (title, project key, every project number its stored text holds).
fn stored(db: &Path) -> Vec<(String, i64, BTreeSet<i64>)> {
    let c = rusqlite::Connection::open(db).unwrap();
    let mut stmt = c
        .prepare("SELECT title,project_key,xer_content FROM projects ORDER BY id")
        .unwrap();
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .unwrap();
    rows.map(|r| {
        let (title, key, text) = r.unwrap();
        let doc = Document::parse(&text).unwrap();
        let mut numbers = BTreeSet::new();
        for t in &doc.tables {
            if let Some(col) = t.field("proj_id") {
                numbers.extend(
                    t.rows
                        .iter()
                        .filter_map(|r| r[col].trim().parse::<i64>().ok()),
                );
            }
        }
        (title, key, numbers)
    })
    .collect()
}

fn create(db: &Path, out: &Path, id: &str) -> Value {
    run(json!({"command":"eps","input":db,"output":out,
        "operation":{"op":"create_project","folder":"WORKSPACE","project_id":id,"planned_start":"2027-01-04"}}))
}

#[track_caller]
fn assert_unique(rows: &[(String, i64, BTreeSet<i64>)]) {
    let keys: BTreeSet<i64> = rows.iter().map(|r| r.1).collect();
    assert_eq!(
        keys.len(),
        rows.len(),
        "every project has its own number: {rows:?}"
    );
}

#[test]
fn create_project_gives_each_new_project_a_number_of_its_own() {
    let dir = temp("create");
    let db = workspace(&dir);
    let before = stored(&db);
    let high = before.iter().map(|r| r.1).max().unwrap();
    // The command line route.
    let a = dir.join("a.db");
    let made = create(&db, &a, "NEW-1");
    let first = ok(&made)["result"]["project_number"].as_i64().unwrap();
    assert!(
        first > high,
        "{first} is above every number in use ({high})"
    );
    // The MCP route, on the result.
    let meta = json!({"io.modelcontextprotocol/protocolVersion":openxer_automation::mcp::PROTOCOL_VERSION,"io.modelcontextprotocol/clientCapabilities":{}});
    let mut server = Server::default();
    let b = dir.join("b.db");
    let called = server
        .handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"_meta":meta,"name":"openxer_eps_edit",
            "arguments":{"input":a,"output":b,"op":"create_project","folder":"WORKSPACE","project_id":"NEW-2","planned_start":"2027-01-11"}}}))
        .unwrap();
    assert_eq!(called["result"]["isError"], false, "{called:#}");
    let rows = stored(&b);
    assert_eq!(rows.len(), before.len() + 2);
    assert_unique(&rows);
    for (title, key, numbers) in &rows[before.len()..] {
        // Every row of the new project carries its own number, and only it.
        assert_eq!(numbers, &BTreeSet::from([*key]), "{title}");
    }
    let second = rows.iter().find(|r| r.0 == "NEW-2").unwrap().1;
    assert!(second > first, "the second is above the first");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_restored_baseline_becomes_a_project_with_a_number_of_its_own() {
    let dir = temp("restore");
    let db = workspace(&dir);
    let projects = ok(&run(json!({"command":"inspect","input":db})))["projects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["workspace_id"].as_i64().unwrap())
        .collect::<Vec<_>>();
    let edit = |input: &Path, step: &str, op: Value| -> (PathBuf, Value) {
        let out = dir.join(format!("{step}.db"));
        let r = run(
            json!({"command":"baseline","input":input,"output":out,"workspace_id":projects[0],"operation":op}),
        );
        let result = ok(&r)["result"].clone();
        (out, result)
    };
    let (w1, made) = edit(&db, "add", json!({"op":"add"}));
    let baseline = made["baseline_id"].as_str().unwrap().to_string();
    let (w2, _) = edit(
        &w1,
        "rename",
        json!({"op":"set","baseline":baseline,"baseline_id":"KEEP"}),
    );
    let (w3, _) = edit(&w2, "restore", json!({"op":"restore","baseline":"KEEP"}));
    let rows = stored(&w3);
    assert_unique(&rows);
    let restored = rows.iter().find(|r| r.0 == "KEEP").expect("restored");
    let others = rows
        .iter()
        .filter(|r| r.0 != "KEEP")
        .map(|r| r.1)
        .max()
        .unwrap();
    assert!(restored.1 > others, "{rows:?}");
    assert!(restored.2.contains(&restored.1), "{rows:?}");
    let _ = std::fs::remove_dir_all(dir);
}
