//! The chat's instructions (the scheduling core, the user's own file and
//! the mode), the playbooks and Check Schedule, on the synthetic sample.
use openxer_automation::{
    chat_prompt::{self, CORE},
    chat_tools::{self, ChatContext, ChatMode},
    cli::{self, Invocation},
    dispatch, edit, load,
    mcp::Server,
    parse_command, playbooks, response,
};
use openxer_xer::sample_project::sample_project;
use openxer_xer::schedule_check::{self as check, CheckParams, CHECKS};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn sample(dir: &Path) -> PathBuf {
    let path = dir.join("sample.xer");
    sample_project()
        .current
        .updated_document()
        .write(&path)
        .unwrap();
    path
}

fn run(request: Value) -> Value {
    response(parse_command(request).and_then(dispatch))
}

fn session(mut server: Server, requests: &[Value]) -> Vec<Value> {
    server.handle(json!({"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"1"}}}));
    server.handle(json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
    requests
        .iter()
        .enumerate()
        .map(|(n, r)| {
            let mut r = r.clone();
            r["jsonrpc"] = json!("2.0");
            r["id"] = json!(n + 1);
            server.handle(r).unwrap()
        })
        .collect()
}

// ------------------------------------------------------------ the prompt

/// The core with its tool rules and the longer mode line, in characters:
/// every chat pays for these on every turn.
const CORE_BUDGET: usize = 1_650;

#[test]
fn the_prompt_is_the_core_the_users_lines_and_the_mode() {
    for mode in [ChatMode::Ask, ChatMode::Edit] {
        let plain = chat_prompt::system_prompt(mode, None);
        assert_eq!(plain, format!("{CORE}\n{}", chat_prompt::mode_line(mode)));
        let with = chat_prompt::system_prompt(mode, Some("  Write dates as 12-Mar-27.\n"));
        assert!(with.starts_with(CORE));
        assert!(with.contains("standing instructions"));
        assert!(with.contains("\nWrite dates as 12-Mar-27.\n"));
        assert!(with.ends_with(chat_prompt::mode_line(mode)));
        // Blank instructions add nothing.
        assert_eq!(chat_prompt::system_prompt(mode, Some(" \n ")), plain);
    }
    let ask = chat_prompt::mode_line(ChatMode::Ask);
    let edit = chat_prompt::mode_line(ChatMode::Edit);
    assert!(ask.contains("read only") && !ask.contains("openxer_apply"));
    assert!(edit.contains("openxer_apply") && edit.contains("approves"));
    let longest = CORE.chars().count() + 1 + ask.chars().count().max(edit.chars().count());
    println!(
        "core {} chars; with the Ask line {}, with the Edit line {}",
        CORE.chars().count(),
        chat_prompt::system_prompt(ChatMode::Ask, None)
            .chars()
            .count(),
        chat_prompt::system_prompt(ChatMode::Edit, None)
            .chars()
            .count()
    );
    assert!(longest <= CORE_BUDGET, "{longest}");
    for text in [CORE, ask, edit] {
        assert!(!text.contains(['\u{2013}', '\u{2014}']));
    }
}

// --------------------------------------------------- the user's own file

#[test]
fn the_users_file_is_read_capped_and_never_required() {
    let dir = tempfile::tempdir().unwrap();
    let folder = chat_prompt::data_folder_in(None, Some(dir.path().as_os_str().to_owned()), None);
    assert_eq!(folder, std::path::absolute(dir.path()).unwrap());
    let path = chat_prompt::instructions_path(&folder);
    assert!(path.ends_with(Path::new("chat").join("AGENTS.md")));
    // Missing, empty, blank and comment-only files are none.
    assert_eq!(chat_prompt::read_user_instructions(&path), None);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    for text in ["", "  \n\n", "<!-- a note -->\n", "<!-- unclosed"] {
        std::fs::write(&path, text).unwrap();
        assert_eq!(chat_prompt::read_user_instructions(&path), None, "{text:?}");
    }
    std::fs::write(&path, "\u{feff}<!-- note -->\r\nName activities by ID.\r\n").unwrap();
    let read = chat_prompt::read_user_instructions(&path).unwrap();
    assert_eq!(read.text, "Name activities by ID.");
    assert!(!read.cut);
    assert_eq!(read.path, path);
    // Cut at the limit, on a character, and said so.
    let long = "é".repeat(chat_prompt::USER_LIMIT + 10);
    std::fs::write(&path, &long).unwrap();
    let read = chat_prompt::read_user_instructions(&path).unwrap();
    assert!(read.cut);
    assert_eq!(read.text.chars().count(), chat_prompt::USER_LIMIT);
    let exact = "a".repeat(chat_prompt::USER_LIMIT);
    std::fs::write(&path, &exact).unwrap();
    assert!(!chat_prompt::read_user_instructions(&path).unwrap().cut);
    // UTF-16 with its mark, and bytes that are not UTF-8 at all.
    let mut utf16 = vec![0xFF, 0xFE];
    for unit in "Use 10-hour days.".encode_utf16() {
        utf16.extend_from_slice(&unit.to_le_bytes());
    }
    std::fs::write(&path, &utf16).unwrap();
    assert_eq!(
        chat_prompt::read_user_instructions(&path).unwrap().text,
        "Use 10-hour days."
    );
    std::fs::write(&path, [0x55, 0xFF, 0xC3, 0x28, 0x7A]).unwrap();
    let odd = chat_prompt::read_user_instructions(&path).unwrap();
    assert!(odd.text.starts_with('U') && odd.text.ends_with('z'));
    let one = [0xFE, 0xFF, 0x00];
    std::fs::write(&path, one).unwrap();
    let _ = chat_prompt::read_user_instructions(&path);
}

#[test]
fn the_starter_file_is_made_once_and_never_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let path = chat_prompt::instructions_path(dir.path());
    assert_eq!(chat_prompt::create_user_instructions(&path).unwrap(), path);
    let starter = std::fs::read_to_string(&path).unwrap();
    assert_eq!(starter, chat_prompt::STARTER);
    // The starter is only a note: it sends nothing.
    assert_eq!(chat_prompt::read_user_instructions(&path), None);
    assert!(!starter.contains(['\u{2013}', '\u{2014}']));
    assert!(starter.chars().count() < 600);
    std::fs::write(&path, "My own rule.").unwrap();
    assert_eq!(chat_prompt::create_user_instructions(&path).unwrap(), path);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "My own rule.");
}

// --------------------------------------------------------------- playbooks

#[test]
fn playbooks_are_listed_and_fetched_through_every_route() {
    let dir = tempfile::tempdir().unwrap();
    let xer = sample(dir.path());
    let names: Vec<&str> = playbooks::PLAYBOOKS.iter().map(|(n, _, _)| *n).collect();
    assert_eq!(
        names,
        [
            "build",
            "update",
            "audit",
            "critical-path",
            "delay-recovery",
            "schedule-check",
            "lookahead",
            "narrative",
            "earned-value",
            "resource-review",
            "what-if"
        ]
    );
    // In-process, as the chat runs it, in both modes.
    for mode in [ChatMode::Ask, ChatMode::Edit] {
        let ctx = ChatContext {
            snapshot: Some(xer.clone()),
            mode,
            playbooks_dir: None,
            ui_actions: None,
        };
        let list = chat_tools::call(&ctx, "openxer_playbook", json!({}));
        assert!(!list.is_error, "{}", list.text);
        assert_eq!(
            list.envelope["result"]["playbooks"]
                .as_array()
                .unwrap()
                .len(),
            11
        );
        let audit = chat_tools::call(&ctx, "openxer_playbook", json!({"name":"audit"}));
        assert!(audit.envelope["result"]["text"]
            .as_str()
            .unwrap()
            .starts_with("# Audit"));
        let bad = chat_tools::call(&ctx, "openxer_playbook", json!({"name":"nope"}));
        assert!(bad.is_error);
        assert!(
            bad.text.contains("use one of build, update, audit"),
            "{}",
            bad.text
        );
    }
    // The chat MCP server and the full one.
    for server in [
        Server::chat(ChatContext {
            snapshot: Some(xer.clone()),
            mode: ChatMode::Ask,
            playbooks_dir: None,
            ui_actions: None,
        }),
        Server::default(),
    ] {
        let replies = session(
            server,
            &[
                json!({"method":"tools/call","params":{"name":"openxer_playbook","arguments":{"name":"critical-path"}}}),
            ],
        );
        let text = replies[0]["result"]["content"][0]["text"].as_str().unwrap();
        let v: Value = serde_json::from_str(text).unwrap();
        assert!(v["result"]["text"]
            .as_str()
            .unwrap()
            .contains("Total and free float"));
    }
    // The CLI, with the clear error for a name that is not a playbook.
    let Invocation::Run(command) = cli::parse(["playbook", "--name", "nope"]).unwrap() else {
        panic!("runs")
    };
    let r = response(dispatch(*command));
    assert_eq!(r["error"]["code"], "not_found");
    assert!(r["error"]["message"]
        .as_str()
        .unwrap()
        .contains("schedule-check"));
    let Invocation::Run(command) = cli::parse(["playbook"]).unwrap() else {
        panic!("runs")
    };
    assert_eq!(
        response(dispatch(*command))["result"]["playbooks"][5]["name"],
        "schedule-check"
    );
}

/// Everything a playbook may name in backquotes: the chat tools, their
/// arguments and allowed values, load's fields (and `-field` for sort),
/// the edit operations and their fields, set_activity's fields, the Check
/// Schedule rows and the playbooks.
fn known_names() -> BTreeSet<String> {
    let mut known = BTreeSet::new();
    let tools = chat_tools::tools_for(ChatMode::Edit, true);
    for t in &tools {
        known.insert(t.name.to_string());
        for (prop, schema) in t.input_schema["properties"].as_object().unwrap() {
            known.insert(prop.clone());
            for v in schema["enum"].as_array().into_iter().flatten() {
                known.insert(v.as_str().unwrap_or_default().to_string());
            }
        }
    }
    for f in load::FIELDS {
        known.insert(f.to_string());
    }
    for op in edit::OPERATIONS {
        known.insert(op.to_string());
    }
    for f in edit::ACTIVITY_FIELDS {
        known.insert(f.to_string());
    }
    for c in CHECKS {
        known.insert(c.name().to_string());
    }
    for (n, _, _) in playbooks::PLAYBOOKS {
        known.insert(n.to_string());
    }
    known
}

/// Whether `field` is a field of edit operation `op`, by the operation's
/// own definition: an unknown field is refused by name.
fn is_operation_field(field: &str) -> bool {
    edit::OPERATIONS.iter().any(|op| {
        let e = edit::operation(json!({"op": op, field: null}))
            .err()
            .unwrap_or_default();
        !e.contains(&format!("unknown field `{field}`"))
    })
}

#[test]
fn playbooks_name_only_tools_and_options_that_exist() {
    let known = known_names();
    let mut wrong = vec![];
    for (name, _, text) in playbooks::PLAYBOOKS {
        // Every tool named anywhere is a chat tool.
        for word in text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
            if word.starts_with("openxer_") && !known.contains(word) {
                wrong.push(format!("{name}: tool {word}"));
            }
        }
        // Every name in backquotes exists.
        for (i, span) in text.split('`').enumerate() {
            if i % 2 == 0 {
                continue;
            }
            let (key, value) = span.split_once('=').unwrap_or((span, ""));
            let ok_key = known.contains(key) || is_operation_field(key);
            let ok_value = match key {
                _ if value.is_empty() => true,
                "fields" => value.split(',').all(|f| load::FIELDS.contains(&f)),
                "sort" => load::FIELDS.contains(&value.trim_start_matches('-')),
                "check" => CHECKS.iter().any(|c| c.name() == value),
                "critical" => matches!(value, "true" | "false"),
                "limit" | "offset" | "relationship_limit" => value.parse::<usize>().is_ok(),
                _ => known.contains(value),
            };
            if !ok_key || !ok_value {
                wrong.push(format!("{name}: `{span}`"));
            }
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
    // The check itself catches a name that does not exist.
    assert!(!known.contains("openxer_baselines") && !is_operation_field("colour"));
    assert!(is_operation_field("lag_hours") && is_operation_field("action"));
}

/// Most characters of one playbook (each is roughly 250 to 450 words).
const PLAYBOOK_BUDGET: usize = 3_000;

#[test]
fn every_playbook_stays_under_its_budget() {
    for (name, about, text) in playbooks::PLAYBOOKS {
        let words = text.split_whitespace().count();
        println!(
            "playbook {name}: {} chars, {words} words",
            text.chars().count()
        );
        assert!(text.chars().count() <= PLAYBOOK_BUDGET, "{name}");
        assert!((230..=470).contains(&words), "{name}: {words} words");
        assert!(about.len() < 120, "{name}");
        assert!(!text.contains(['\u{2013}', '\u{2014}']), "{name}");
    }
}

// ---------------------------------------------------------- Check Schedule

#[test]
fn check_schedule_gives_the_rows_the_app_computes() {
    let dir = tempfile::tempdir().unwrap();
    let xer = sample(dir.path());
    let p = sample_project().current;
    let expected = check::run_checks_with(
        &p.model,
        &CheckParams::default(),
        &check::expense_tasks(&p),
        Some(&check::current_finishes(&p)),
    );
    let r = run(json!({"command":"check_schedule","input":xer}));
    assert_eq!(r["ok"], true, "{r}");
    let rows = r["result"]["rows"].as_array().unwrap();
    assert_eq!(rows.len(), CHECKS.len());
    for (row, want) in rows.iter().zip(&expected) {
        assert_eq!(row["check"], want.check.name());
        assert_eq!(row["count"], want.items.len());
        assert_eq!(row["of"], want.population);
        assert_eq!(row["pass"], json!(want.passed));
        assert_eq!(row["target"], json!(want.target));
        match want.value {
            Some(v) => assert!((row["result"].as_f64().unwrap() - v).abs() < 0.006),
            None => assert!(row["result"].is_null()),
        }
    }
    // One row's items, capped.
    let row = expected
        .iter()
        .find(|r| r.items.len() > 2)
        .expect("a row with items");
    let name = row.check.name();
    let r =
        run(json!({"command":"check_schedule","input":xer,"check":name.to_lowercase(),"limit":2}));
    assert_eq!(r["result"]["check"], name);
    assert_eq!(r["result"]["items"].as_array().unwrap().len(), 2);
    assert_eq!(r["result"]["items_total"], row.items.len());
    let bad = run(json!({"command":"check_schedule","input":xer,"check":"Colour"}));
    assert!(bad["error"]["message"].as_str().unwrap().contains("Logic"));
    assert_eq!(
        run(json!({"command":"check_schedule","input":xer,"limit":0}))["ok"],
        false
    );
    // The CLI, MCP and the chat give the same rows.
    let input = xer.display().to_string();
    let Invocation::Run(command) =
        cli::parse(["check-schedule", "--input", input.as_str()]).unwrap()
    else {
        panic!("runs")
    };
    let from_cli = response(dispatch(*command));
    let replies = session(
        Server::default(),
        &[
            json!({"method":"tools/call","params":{"name":"openxer_check_schedule","arguments":{"input":xer}}}),
        ],
    );
    assert_eq!(from_cli, replies[0]["result"]["structuredContent"]);
    let chat = chat_tools::call(
        &ChatContext {
            snapshot: Some(xer.clone()),
            mode: ChatMode::Ask,
            playbooks_dir: None,
            ui_actions: None,
        },
        "openxer_check_schedule",
        json!({}),
    );
    assert_eq!(chat.envelope, from_cli);
    println!("check_schedule answer: {} bytes", chat.text.len());
    assert!(chat.text.len() < 3_000);
}

#[test]
fn load_gives_constraints_actuals_and_original_duration_on_request() {
    let dir = tempfile::tempdir().unwrap();
    let xer = sample(dir.path());
    let r = run(
        json!({"command":"load","input":xer,"limit":2000,"fields":["original_duration_minutes","actual_start","actual_finish","constraint","constraint_date","status"]}),
    );
    let rows = r["result"]["activities"].as_array().unwrap();
    assert!(rows
        .iter()
        .any(|a| a["constraint"].is_string() && a["constraint_date"].is_string()));
    assert!(rows
        .iter()
        .any(|a| a["status"] == "Completed" && a["actual_finish"].is_string()));
    assert!(rows
        .iter()
        .all(|a| a["original_duration_minutes"].is_number()));
    let plain = run(json!({"command":"load","input":xer,"limit":1}));
    assert!(plain["result"]["activities"][0].get("constraint").is_none());
}

#[test]
fn custom_playbooks_and_project_rules_extend_chat_without_touching_globals() {
    let dir = tempfile::tempdir().unwrap();
    let xer = sample(dir.path());
    let pb_dir = playbooks::playbooks_dir(dir.path());
    playbooks::ensure_playbooks_dir(&pb_dir).unwrap();
    // Comment-only or empty markdown files are skipped.
    std::fs::write(pb_dir.join("ignored.md"), "<!-- only a comment -->\n").unwrap();
    std::fs::write(
        pb_dir.join("Owner Weekly.md"),
        "<!-- note -->\n# Owner Weekly Report\nList top 3 critical milestones and open issues.\n",
    )
    .unwrap();
    let found = playbooks::discover_user_playbooks(&pb_dir);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].name, "owner-weekly");
    assert_eq!(found[0].about, "Owner Weekly Report");
    assert!(found[0].custom);

    let ctx = ChatContext {
        snapshot: Some(xer),
        mode: ChatMode::Ask,
        playbooks_dir: Some(pb_dir.clone()),
        ui_actions: None,
    };
    let listed = chat_tools::call(&ctx, "openxer_playbook", json!({}));
    assert!(!listed.is_error, "{}", listed.text);
    let arr = listed.envelope["result"]["playbooks"].as_array().unwrap();
    assert_eq!(arr.len(), playbooks::PLAYBOOKS.len() + 1);
    assert!(arr
        .iter()
        .any(|p| p["name"] == "owner-weekly" && p["custom"] == true));

    let fetched = chat_tools::call(&ctx, "openxer_playbook", json!({"name": "owner-weekly"}));
    assert!(!fetched.is_error, "{}", fetched.text);
    assert!(fetched.envelope["result"]["text"]
        .as_str()
        .unwrap()
        .contains("List top 3 critical milestones"));

    let prompt = chat_prompt::system_prompt_with(
        ChatMode::Edit,
        Some("Use 10-hour days."),
        Some("Contract milestone MS-900 has $25k/day LDs."),
    );
    assert!(prompt.contains("Use 10-hour days."));
    assert!(prompt.contains("Project-specific rules for this schedule"));
    assert!(prompt.contains("Contract milestone MS-900"));
    assert!(prompt.ends_with(chat_prompt::mode_line(ChatMode::Edit)));
}
