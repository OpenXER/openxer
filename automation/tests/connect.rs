//! `openxer-cli connect` end to end. Every run clears the environment and
//! points APPDATA, LOCALAPPDATA, the profile and PATH at a temporary
//! folder, so no real client config is ever read or written.
use openxer_automation::{
    cli::{self, Invocation},
    connect::{Client, Request},
};
use serde_json::Value;
use std::{fs, path::Path, process::Command};

fn run(root: &Path, args: &[&str]) -> (Value, i32) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_openxer-cli"));
    command.env_clear();
    if let Some(system) = std::env::var_os("SystemRoot") {
        command.env("SystemRoot", system);
    }
    let out = command
        .args(args)
        .env("APPDATA", root.join("Roaming"))
        .env("LOCALAPPDATA", root.join("Local"))
        .env("USERPROFILE", root.join("home"))
        .env("HOME", root.join("home"))
        .env("PATH", root.join("bin"))
        .output()
        .unwrap();
    let text = String::from_utf8(out.stdout).unwrap();
    (
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("{e}: {text}")),
        out.status.code().unwrap(),
    )
}

fn root() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for sub in ["Roaming", "Local", "home", "bin"] {
        fs::create_dir_all(dir.path().join(sub)).unwrap();
    }
    dir
}

#[test]
fn connect_arguments_parse() {
    let parse = |args: &[&str]| cli::parse(args.iter().copied());
    assert!(matches!(
        parse(&["connect", "status"]).unwrap(),
        Invocation::Connect(Request::Status)
    ));
    assert!(matches!(
        parse(&["connect", "codex", "--disconnect", "--dry-run"]).unwrap(),
        Invocation::Connect(Request::Apply {
            client: Client::Codex,
            disconnect: true,
            dry_run: true
        })
    ));
    assert!(matches!(
        parse(&["connect", "claude-desktop"]).unwrap(),
        Invocation::Connect(Request::Apply {
            client: Client::ClaudeDesktop,
            disconnect: false,
            dry_run: false
        })
    ));
    assert!(matches!(
        parse(&["connect", "chatgpt", "--dry-run"]).unwrap(),
        Invocation::Connect(Request::Apply {
            client: Client::Codex,
            disconnect: false,
            dry_run: true
        })
    ));
    assert!(matches!(
        parse(&["connect", "all"]).unwrap(),
        Invocation::Connect(Request::All { dry_run: false })
    ));
    assert!(matches!(
        parse(&["connect", "all", "--dry-run"]).unwrap(),
        Invocation::Connect(Request::All { dry_run: true })
    ));
    for bad in [
        &["connect"][..],
        &["connect", "all", "--disconnect"],
        &["connect", "vscode"],
        &["connect", "status", "--dry-run"],
        &["connect", "claude-code", "--disconnect"],
        &["connect", "codex", "--force"],
        &["connect", "codex", "--dry-run", "--dry-run"],
    ] {
        assert_eq!(parse(bad).unwrap_err().code, "invalid_arguments", "{bad:?}");
    }
    assert!(cli::HELP.contains("openxer-cli connect status"));
}

#[test]
fn status_connect_and_disconnect_codex_in_the_envelope() {
    let dir = root();
    let (status, code) = run(dir.path(), &["connect", "status"]);
    assert_eq!(code, 0);
    assert_eq!(status["ok"], true);
    assert_eq!(status["schema_version"], "1.0");
    assert_eq!(status["result"]["cli_found"], true);
    let clients = status["result"]["clients"].as_array().unwrap();
    assert_eq!(clients.len(), 3);
    assert_eq!(clients[1]["client"], "codex");
    assert_eq!(clients[1]["name"], "ChatGPT desktop and Codex");
    assert_eq!(clients[1]["found"], false);
    assert_eq!(clients[1]["entry"]["state"], "not_connected");
    assert_eq!(clients[1]["action"], "not_installed");
    assert_eq!(status["result"]["cli_installed"], false);

    let codex = dir.path().join("home").join(".codex");
    fs::create_dir_all(&codex).unwrap();
    let original = "# keep\nmodel = \"m\"\n";
    fs::write(codex.join("config.toml"), original).unwrap();

    let (dry, _) = run(dir.path(), &["connect", "codex", "--dry-run"]);
    assert_eq!(dry["result"]["dry_run"], true);
    assert!(dry["result"]["files"][0]["preview"]
        .as_str()
        .unwrap()
        .contains("[mcp_servers.openxer]"));
    assert_eq!(
        fs::read_to_string(codex.join("config.toml")).unwrap(),
        original
    );

    let (done, code) = run(dir.path(), &["connect", "chatgpt"]);
    assert_eq!(code, 0, "{done}");
    assert_eq!(
        done["result"]["message"],
        format!("Connected. {}", Client::Codex.next_step())
    );
    assert_eq!(done["result"]["next_step"], Client::Codex.next_step());
    assert!(done["result"]["files"][0]["backup"].is_string());
    let text = fs::read_to_string(codex.join("config.toml")).unwrap();
    assert!(text.starts_with(original), "{text}");
    let (status, _) = run(dir.path(), &["connect", "status"]);
    assert_eq!(status["result"]["clients"][1]["found"], true);
    assert_eq!(
        status["result"]["clients"][1]["entry"]["state"],
        "connected"
    );

    let (undone, _) = run(dir.path(), &["connect", "codex", "--disconnect"]);
    assert_eq!(
        undone["result"]["message"],
        "Disconnected. Restart ChatGPT desktop and Codex to finish."
    );
    assert_eq!(
        fs::read_to_string(codex.join("config.toml")).unwrap(),
        original
    );
}

#[test]
fn malformed_config_fails_in_the_envelope_and_is_unchanged() {
    let dir = root();
    let claude = dir.path().join("Roaming").join("Claude");
    fs::create_dir_all(&claude).unwrap();
    let file = claude.join("claude_desktop_config.json");
    fs::write(&file, "{ not json").unwrap();
    let (out, code) = run(dir.path(), &["connect", "claude-desktop"]);
    assert_eq!(code, 1);
    assert_eq!(out["ok"], false);
    assert_eq!(out["error"]["code"], "config_malformed");
    assert_eq!(fs::read_to_string(&file).unwrap(), "{ not json");
}

#[test]
fn claude_code_prints_the_command_and_runs_nothing() {
    let dir = root();
    let (out, code) = run(dir.path(), &["connect", "claude-code"]);
    assert_eq!(code, 0);
    let command = out["result"]["command"].as_str().unwrap();
    let exe = env!("CARGO_BIN_EXE_openxer-cli");
    assert_eq!(
        command,
        format!("claude mcp add --scope user openxer -- \"{exe}\" mcp")
    );
    assert_eq!(out["result"]["found"], false);
}

#[test]
fn connect_all_connects_found_apps_and_reports_each() {
    let dir = root();
    // Nothing found: nothing to do.
    let (out, code) = run(dir.path(), &["connect", "all"]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(out["result"]["clients"], serde_json::json!([]));
    assert!(out["result"]["message"]
        .as_str()
        .unwrap()
        .starts_with("Nothing to connect"));
    // Claude Desktop and the ChatGPT/Codex app found.
    fs::create_dir_all(dir.path().join("Roaming").join("Claude")).unwrap();
    fs::create_dir_all(
        dir.path()
            .join("Local")
            .join("Packages")
            .join("OpenAI.Codex_2p2nqsd0c76g0"),
    )
    .unwrap();
    let (dry, _) = run(dir.path(), &["connect", "all", "--dry-run"]);
    assert_eq!(dry["result"]["clients"].as_array().unwrap().len(), 2);
    assert!(!dir.path().join("home").join(".codex").exists());
    let (out, code) = run(dir.path(), &["connect", "all"]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(out["result"]["ok"], true);
    let (status, _) = run(dir.path(), &["connect", "status"]);
    for i in [0, 1] {
        assert_eq!(status["result"]["clients"][i]["action"], "connected");
    }
    assert!(dir
        .path()
        .join("home")
        .join(".codex")
        .join("config.toml")
        .is_file());
}

/// A first-time agent with no file named: find_schedules (CLI and MCP)
/// names the workspace and its projects.
#[test]
fn find_schedules_from_the_cli_and_mcp() {
    let dir = root();
    let data = dir.path().join("Data folder");
    fs::create_dir_all(&data).unwrap();
    let db = data.join("openxer.db");
    let c = rusqlite::Connection::open(&db).unwrap();
    c.execute_batch(
        "CREATE TABLE projects(id INTEGER PRIMARY KEY,source_name TEXT NOT NULL,project_key INTEGER NOT NULL,title TEXT NOT NULL,xer_content TEXT NOT NULL,updated_at TEXT DEFAULT CURRENT_TIMESTAMP,UNIQUE(source_name,project_key));
         INSERT INTO projects VALUES(7,'C:\\jobs\\Tie-ins.xer',70,'Tie-ins','','2026-11-01 10:00:00');",
    )
    .unwrap();
    drop(c);
    let mut command = Command::new(env!("CARGO_BIN_EXE_openxer-cli"));
    command.env_clear();
    if let Some(system) = std::env::var_os("SystemRoot") {
        command.env("SystemRoot", system);
    }
    let out = command
        .args(["find-schedules"])
        .env("OPENXER_DATA_DIR", &data)
        .env("LOCALAPPDATA", dir.path().join("Local"))
        .output()
        .unwrap();
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["ok"], true, "{value}");
    let ws = &value["result"]["workspaces"][0];
    assert_eq!(ws["path"], serde_json::json!(db));
    assert_eq!(ws["projects"][0]["workspace_id"], 7);
    assert_eq!(ws["projects"][0]["name"], "Tie-ins");

    // The MCP tool is listed first, needs no arguments and is read-only.
    let tools = openxer_automation::mcp::tool_definitions();
    assert_eq!(tools[0]["name"], "openxer_find_schedules");
    assert_eq!(tools[0]["annotations"]["readOnlyHint"], true);
    assert_eq!(tools[0]["inputSchema"]["required"], serde_json::json!([]));
    // The server runs as a client starts it, with the same fake profile,
    // so the real data folder is never read.
    let folder = dir.path().join("Downloads");
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("Week 14.xer"), b"").unwrap();
    let requests = [
        serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"1"}}}),
        serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"openxer_find_schedules","arguments":{}}}),
        serde_json::json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"openxer_find_schedules","arguments":{"folder":folder}}}),
    ];
    let mut command = Command::new(env!("CARGO_BIN_EXE_openxer-cli"));
    command.env_clear();
    if let Some(system) = std::env::var_os("SystemRoot") {
        command.env("SystemRoot", system);
    }
    let mut child = command
        .arg("mcp")
        .env("OPENXER_DATA_DIR", &data)
        .env("LOCALAPPDATA", dir.path().join("Local"))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    {
        use std::io::Write;
        let mut stdin = child.stdin.take().unwrap();
        for r in requests {
            writeln!(stdin, "{r}").unwrap();
        }
    }
    let out = child.wait_with_output().unwrap();
    let replies: Vec<Value> = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let instructions = replies[0]["result"]["instructions"].as_str().unwrap();
    assert!(
        instructions.contains("openxer_find_schedules"),
        "{instructions}"
    );
    let first = &replies[1]["result"]["structuredContent"];
    assert_eq!(first["ok"], true, "{first}");
    assert_eq!(
        first["result"]["workspaces"][0]["projects"][0]["name"],
        "Tie-ins"
    );
    let second = &replies[2]["result"]["structuredContent"];
    assert_eq!(second["result"]["folder"]["files"][0]["kind"], "xer");
}

#[test]
fn connect_test_starts_the_real_server_and_lists_its_tools() {
    let dir = root();
    let (out, code) = run(dir.path(), &["connect", "test"]);
    assert_eq!(code, 0, "{out}");
    let tools = out["result"]["tools"].as_u64().unwrap();
    assert_eq!(
        tools as usize,
        openxer_automation::mcp::tool_definitions().len()
    );
    assert_eq!(out["result"]["message"], format!("Working: {tools} tools"));
}
