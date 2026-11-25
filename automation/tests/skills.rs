//! Skills and the read-only analysis tools through the CLI and MCP. Every
//! run that reads the skill folder is a subprocess with OPENXER_DATA_DIR set
//! to a temporary folder, so the real data folder is never read or written.
use openxer_automation::{dispatch, mcp::Server, parse_command, response};
use openxer_xer::starter;
use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn cli(data: &Path, args: &[&str]) -> (Value, i32) {
    let out = Command::new(env!("CARGO_BIN_EXE_openxer-cli"))
        .args(args)
        .env("OPENXER_DATA_DIR", data)
        .output()
        .unwrap();
    let text = String::from_utf8(out.stdout).unwrap();
    (
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("{e}: {text}")),
        out.status.code().unwrap(),
    )
}

/// One MCP session over stdio: each request in order, the replies.
fn mcp(data: &Path, requests: &[Value]) -> Vec<Value> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_openxer-cli"))
        .arg("mcp")
        .env("OPENXER_DATA_DIR", data)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let mut stdin = child.stdin.take().unwrap();
        for r in requests {
            writeln!(stdin, "{r}").unwrap();
        }
    }
    let out = child.wait_with_output().unwrap();
    String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn handshake() -> Vec<Value> {
    vec![
        json!({"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    ]
}

const MINE: &str = "---
id: crew-check
name: Crew check
description: Checks the crews.
request: Run the crew check.
use_when: The user asks about crews.
kind: check
cites:
- A private handbook
tools:
- openxer_resource_loading
---
Run `openxer_resource_loading` and report the peaks.
";

#[test]
fn the_cli_lists_shows_validates_creates_and_finds_skills() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path();
    let (path, code) = cli(data, &["skills", "path"]);
    assert_eq!(code, 0, "{path}");
    assert_eq!(
        PathBuf::from(path["result"]["folder"].as_str().unwrap()),
        data.join("skills"),
        "the folder is the data folder's skills"
    );
    assert_eq!(path["result"]["exists"], false);

    let (list, code) = cli(data, &["skills", "list"]);
    assert_eq!(code, 0);
    let skills = list["result"]["skills"].as_array().unwrap();
    assert!(skills.len() >= 30, "{}", skills.len());
    assert!(skills.iter().all(|s| s["source"] == "built_in"));
    assert!(skills
        .iter()
        .any(|s| s["id"] == "logic-health-check" && s["kind"] == "check"));
    assert!(skills
        .iter()
        .any(|s| s["id"] == "build-schedule" && s["kind"] == "workflow"));

    let (show, code) = cli(data, &["skills", "show", "look-ahead"]);
    assert_eq!(code, 0);
    assert!(show["result"]["instructions"]
        .as_str()
        .unwrap()
        .contains("openxer_lookahead"));
    let (missing, code) = cli(data, &["skills", "show", "nope"]);
    assert_eq!(
        (missing["error"]["code"].as_str(), code),
        (Some("not_found"), 1)
    );

    // new writes the template into the data folder; it validates.
    let (new, code) = cli(data, &["skills", "new", "crew-check"]);
    assert_eq!(code, 0, "{new}");
    let file = data.join("skills").join("crew-check.md");
    assert!(file.is_file());
    let (again, code) = cli(data, &["skills", "new", "crew-check"]);
    assert_eq!(code, 1, "{again}");
    std::fs::write(&file, MINE).unwrap();
    let (ok, code) = cli(data, &["skills", "validate", file.to_str().unwrap()]);
    assert_eq!(
        (ok["result"]["valid"].as_bool(), code),
        (Some(true), 0),
        "{ok}"
    );

    // A broken file is reported by validate and left out of the list.
    let bad = data.join("skills").join("broken.md");
    std::fs::write(&bad, "---\nid: broken\n---\nx\n").unwrap();
    let (one, code) = cli(data, &["skills", "validate", bad.to_str().unwrap()]);
    assert_eq!(code, 1);
    assert_eq!(one["error"]["code"], "invalid_skill");
    assert!(one["error"]["message"]
        .as_str()
        .unwrap()
        .contains("has no name"));
    let (all, _) = cli(data, &["skills", "validate"]);
    assert_eq!(all["result"]["valid"], false);
    assert_eq!(all["result"]["problems"].as_array().unwrap().len(), 1);
    let (list, _) = cli(data, &["skills", "list"]);
    let skills = list["result"]["skills"].as_array().unwrap();
    assert!(skills
        .iter()
        .any(|s| s["id"] == "crew-check" && s["source"] == "yours"));
    assert!(!skills.iter().any(|s| s["id"] == "broken"));
    assert!(list["result"]["problems"][0]["path"]
        .as_str()
        .unwrap()
        .ends_with("broken.md"));

    // Bad command lines.
    let (_, code) = cli(data, &["skills"]);
    assert_eq!(code, 2);
    let (_, code) = cli(data, &["skills", "show"]);
    assert_eq!(code, 2);
    let (_, code) = cli(data, &["skills", "list", "extra"]);
    assert_eq!(code, 2);
}

#[test]
fn mcp_serves_skills_as_tools_and_prompts_and_sees_new_files_at_once() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path();
    let folder = data.join("skills");
    std::fs::create_dir_all(&folder).unwrap();
    let mut requests = handshake();
    requests.extend([
        json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"openxer_skills","arguments":{}}}),
        json!({"jsonrpc":"2.0","id":2,"method":"prompts/list","params":{}}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"openxer_skill","arguments":{"id":"near-critical-paths"}}}),
        json!({"jsonrpc":"2.0","id":4,"method":"prompts/get","params":{"name":"near-critical-paths"}}),
        json!({"jsonrpc":"2.0","id":5,"method":"prompts/get","params":{"name":"nope"}}),
    ]);
    let replies = mcp(data, &requests);
    let init = &replies[0]["result"];
    assert!(init["capabilities"]["prompts"].is_object(), "{init}");
    let instructions = init["instructions"].as_str().unwrap();
    assert!(
        instructions.contains("openxer_skills")
            && instructions.contains("Never compute schedule numbers yourself")
    );
    let listed = &replies[1]["result"]["structuredContent"]["result"]["skills"];
    let prompts = replies[2]["result"]["prompts"].as_array().unwrap();
    assert_eq!(prompts.len(), listed.as_array().unwrap().len());
    assert!(prompts
        .iter()
        .any(|p| p["name"] == "look-ahead" && p["title"] == "Look-ahead"));
    let skill = &replies[3]["result"]["structuredContent"]["result"];
    assert_eq!(skill["kind"], "check");
    assert!(skill["instructions"]
        .as_str()
        .unwrap()
        .contains("openxer_paths"));
    let text = replies[4]["result"]["messages"][0]["content"]["text"]
        .as_str()
        .unwrap();
    assert!(
        text.contains("never compute schedule metrics yourself") && text.contains("openxer_paths"),
        "{text}"
    );
    assert_eq!(replies[5]["error"]["code"], -32602);

    // A file added while the server runs is listed at once; its cites stay
    // out of what the agent receives.
    std::fs::write(folder.join("crew-check.md"), MINE).unwrap();
    let mut requests = handshake();
    requests.extend([
        json!({"jsonrpc":"2.0","id":1,"method":"prompts/list"}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"openxer_skill","arguments":{"id":"crew-check"}}}),
        json!({"jsonrpc":"2.0","id":3,"method":"prompts/get","params":{"name":"crew-check","arguments":{}}}),
    ]);
    let replies = mcp(data, &requests);
    assert!(replies[1]["result"]["prompts"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["name"] == "crew-check"));
    let mine = &replies[2]["result"]["structuredContent"]["result"];
    assert_eq!(mine["source"], "yours");
    assert!(!replies[2].to_string().contains("private handbook"));
    assert!(!replies[3].to_string().contains("private handbook"));
}

#[test]
fn every_new_tool_is_listed_with_a_schema() {
    let mut server = Server::default();
    server.handle(json!({"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"t","version":"1"}}}));
    server.handle(json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
    let list = server
        .handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}))
        .unwrap();
    let names: Vec<&str> = list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    for name in [
        "openxer_skills",
        "openxer_skill",
        "openxer_check_schedule",
        "openxer_paths",
        "openxer_compare",
        "openxer_lookahead",
        "openxer_milestones",
        "openxer_schedule_stats",
        "openxer_resource_loading",
        "openxer_wbs_summary",
    ] {
        assert!(names.contains(&name), "{name}");
    }
}

fn run(request: Value) -> Value {
    response(parse_command(request).and_then(dispatch))
}

/// The starter project scheduled, and a revised copy: a longer activity, a
/// new one tied in, a crew assigned.
fn files(dir: &Path) -> (PathBuf, PathBuf) {
    let source = dir.join("source.xer");
    starter().updated_document().write(&source).unwrap();
    let scheduled = dir.join("scheduled.xer");
    let r = run(json!({"command":"schedule","input":source,"output":scheduled}));
    assert_eq!(r["ok"], true, "{r}");
    let revised = dir.join("revised.xer");
    let r = run(json!({"command":"apply","input":scheduled,"output":revised,
        "changes":{"format":"openxer-changes/1","operations":[
            {"op":"set_activity","activity":"RB-1070","field":"original_duration","value":80},
            {"op":"add_activity","activity":"RB-2000","name":"Install added supports","wbs":"RB.3"},
            {"op":"set_activity","activity":"RB-2000","field":"original_duration","value":16},
            {"op":"relationship","action":"add","predecessor":"RB-1070","successor":"RB-2000","kind":"FS"},
            {"op":"relationship","action":"add","predecessor":"RB-2000","successor":"RB-1080","kind":"FS"},
            {"op":"add_resource","resource":"PF"},
            {"op":"set_resource_limit","resource":"PF","from":"2026-11-02 08:00","max_units_per_hour":1},
            {"op":"copy_calendar","calendar":"Six day","kind":"resource"},
            {"op":"calendar_weekday","calendar":"Six day","weekday":6,"shifts":[["08:00","12:00"],["13:00","17:00"]]},
            {"op":"set_resource","resource":"PF","field":"calendar","value":"Six day"},
            {"op":"assign_resource","activity":"RB-1090","resource":"PF"},
            {"op":"set_assignment","activity":"RB-1090","resource":"PF","field":"budgeted_units","value":200},
            {"op":"schedule"}]}}));
    assert_eq!(r["ok"], true, "{r}");
    (scheduled, revised)
}

#[test]
fn the_analysis_tools_answer_from_the_schedule() {
    let dir = tempfile::tempdir().unwrap();
    let (scheduled, revised) = files(dir.path());

    let r = run(json!({"command":"check_schedule","input":scheduled}));
    assert_eq!(r["ok"], true, "{r}");
    let checks = r["result"]["rows"].as_array().unwrap();
    assert_eq!(checks.len(), 15);
    let logic = checks.iter().find(|c| c["check"] == "Logic").unwrap();
    assert!(logic["of"].as_u64().unwrap() > 0);
    let resources = checks
        .iter()
        .find(|c| c["check"] == "Resources / Cost")
        .unwrap();
    assert_eq!(resources["pass"], false, "the starter has no resources");

    let r = run(json!({"command":"paths","input":scheduled,"count":3}));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["result"]["target"]["activity"], "RB-1130");
    let path1 = &r["result"]["paths"][0];
    assert_eq!(path1["behind_path_1_hours"], 0.0);
    assert!(path1["activities"]
        .as_array()
        .unwrap()
        .iter()
        .any(|a| a["activity"] == "RB-1000"));
    assert!(
        r["result"]["critical_counts"]["on_the_longest_path"]
            .as_u64()
            .unwrap()
            > 0
    );
    let r = run(json!({"command":"paths","input":scheduled,"activity":"NOPE"}));
    assert_eq!(r["error"]["code"], "not_found");

    let r = run(json!({"command":"compare","input":revised,"reference":scheduled}));
    assert_eq!(r["ok"], true, "{r}");
    let s = &r["result"]["summary"];
    assert_eq!(s["activities_added"], 1);
    assert_eq!(s["ties_added"], 2);
    assert_eq!(s["original_durations_changed"], 1);
    assert!(
        r["result"]["project_finish"]["moved_days"]
            .as_f64()
            .unwrap()
            > 0.
    );
    let r = run(json!({"command":"compare","input":revised}));
    assert_eq!(r["error"]["code"], "invalid_arguments");
    let r = run(json!({"command":"compare","input":revised,"baseline":"project"}));
    assert_eq!(
        r["error"]["code"], "no_baseline",
        "an XER has no stored baseline"
    );

    let r = run(json!({"command":"lookahead","input":scheduled,"weeks":3}));
    assert_eq!(r["ok"], true, "{r}");
    let sum = &r["result"]["summary"];
    assert!(sum["activities"].as_u64().unwrap() > 0);
    assert!(sum["ready"].as_u64().unwrap() >= 1);
    let r = run(json!({"command":"lookahead","input":scheduled,"weeks":0}));
    assert_eq!(r["ok"], false);

    let r = run(json!({"command":"milestones","input":scheduled}));
    assert_eq!(r["ok"], true, "{r}");
    let ms = r["result"]["milestones"].as_array().unwrap();
    assert_eq!(ms.len(), 3);
    let handover = ms.iter().find(|m| m["activity"] == "RB-1130").unwrap();
    assert_eq!(handover["driven_by"][0]["activity"], "RB-1120");

    let r = run(json!({"command":"schedule_stats","input":scheduled,"update_period_days":5}));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["result"]["activities"], 14);
    assert_eq!(r["result"]["calendars"][0]["working_days_per_week"], 5);
    assert_eq!(r["result"]["durations"]["longest"], 8.0);

    let r = run(json!({"command":"resource_loading","input":revised}));
    assert_eq!(r["ok"], true, "{r}");
    let pf = &r["result"]["resources"][0];
    assert_eq!(pf["resource"], "PF");
    assert_eq!(pf["limit_units_per_hour"], 1.0);
    assert!(pf["peak"]["units"].as_f64().unwrap() > 0.);
    // The limit is the profile's: 1 unit an hour over the resource's own
    // six-day calendar (48 hours a week), not the project's five-day one.
    assert_eq!(pf["calendar"], "Six day", "{pf}");
    let limits: Vec<f64> = pf["per_period"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["limit"].as_f64().unwrap())
        .collect();
    assert!(limits.contains(&48.0), "{limits:?}");
    assert!(!limits.contains(&40.0), "{limits:?}");

    let r = run(json!({"command":"wbs_summary","input":scheduled,"level":1}));
    assert_eq!(r["ok"], true, "{r}");
    let elements = r["result"]["elements"].as_array().unwrap();
    assert_eq!(elements.len(), 4);
    assert!(elements.iter().all(|e| e["level"] == 1));
}

#[test]
fn the_analysis_commands_parse_on_the_command_line() {
    use openxer_automation::cli::{parse, Invocation};
    for args in [
        vec![
            "check-schedule",
            "--input",
            "a.xer",
            "--large-float-hours",
            "40",
        ],
        vec![
            "paths",
            "--input",
            "a.xer",
            "--activity",
            "A1",
            "--count",
            "3",
        ],
        vec!["compare", "--input", "a.xer", "--reference", "b.xer"],
        vec![
            "compare",
            "--input",
            "a.db",
            "--baseline",
            "project",
            "--workspace-id",
            "2",
        ],
        vec!["lookahead", "--input", "a.xer", "--weeks", "6"],
        vec!["milestones", "--input", "a.xer"],
        vec![
            "schedule-stats",
            "--input",
            "a.xer",
            "--update-period-days",
            "5",
        ],
        vec![
            "resource-loading",
            "--input",
            "a.xer",
            "--interval",
            "month",
        ],
        vec!["wbs-summary", "--input", "a.xer", "--wbs-level", "2"],
        vec!["skills", "list"],
        vec!["skills", "validate"],
        vec!["skills", "show", "look-ahead"],
    ] {
        assert!(
            matches!(parse(args.clone()), Ok(Invocation::Run(_))),
            "{args:?}"
        );
    }
    assert!(parse(["paths", "--input", "a.xer", "--count", "x"]).is_err());
    assert!(parse([
        "check-schedule",
        "--input",
        "a.xer",
        "--long-lag-hours",
        "x"
    ])
    .is_err());
}
