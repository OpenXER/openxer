//! The agent host against recorded transcripts and a scripted fake agent:
//! both parsers, the launch arguments (the sandbox), sessions (multi-turn,
//! tools, proposals, cancel, early exit, missing or signed-out agents, bad
//! output, cleanup), detection and sign-in. No real agent runs here.
use openxer_automation::agent_host::{
    self, claude, codex, model_choices, parse_sign_in_status, tool_target, Agent, AgentError,
    AgentProcess, Captured, Event, Launch, ModelChoice, Session, SessionOptions, SignIn,
    SignInState, Spawner, Usage,
};
use openxer_automation::{chat_tools::ChatMode, connect::Env};
use serde_json::json;
use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// ------------------------------------------------------------ fixtures

fn fixture(name: &str) -> Vec<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/agent")
        .join(name);
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect()
}

/// A recorded transcript cut into its turns (each ends with `end`).
fn turns(name: &str, end: &str) -> Vec<Vec<String>> {
    let mut out = vec![vec![]];
    for line in fixture(name) {
        let last = line.contains(end);
        out.last_mut().unwrap().push(line);
        if last {
            out.push(vec![]);
        }
    }
    out.retain(|t| !t.is_empty());
    out
}

fn claude_turns() -> Vec<Vec<String>> {
    turns("claude-two-turns.jsonl", "\"type\":\"result\"")
}

fn codex_turns() -> Vec<Vec<String>> {
    turns("codex-two-turns.jsonl", "\"type\":\"turn.completed\"")
}

fn parse_claude(lines: &[String]) -> (Vec<Event>, claude::ClaudeParser) {
    let mut p = claude::ClaudeParser::new();
    let now = Instant::now();
    let events = lines.iter().flat_map(|l| p.parse_line(l, now)).collect();
    (events, p)
}

fn parse_codex(lines: &[String]) -> (Vec<Event>, codex::CodexParser) {
    let mut p = codex::CodexParser::new("gpt-test");
    let now = Instant::now();
    let events = lines.iter().flat_map(|l| p.parse_line(l, now)).collect();
    (events, p)
}

fn messages(events: &[Event]) -> String {
    events
        .iter()
        .filter_map(|e| match e {
            Event::Message { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn deltas(events: &[Event]) -> String {
    events
        .iter()
        .filter_map(|e| match e {
            Event::TextDelta { text } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

fn finished(events: &[Event]) -> Vec<(Usage, Option<f64>, bool)> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::TurnFinished {
                usage,
                cost_usd,
                cancelled,
            } => Some((*usage, *cost_usd, *cancelled)),
            _ => None,
        })
        .collect()
}

// -------------------------------------------------------------- parsers

#[test]
fn the_claude_parser_reads_a_recorded_conversation() {
    let all: Vec<String> = claude_turns().concat();
    let (events, parser) = parse_claude(&all);
    assert_eq!(
        parser.session_id.as_deref(),
        Some("00000000-0000-4000-8000-000000000001")
    );
    // One init per turn; the session reports Ready once.
    let ready: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, Event::Ready { model } if model == "claude-opus-5-5"))
        .collect();
    assert_eq!(ready.len(), 2);
    assert!(
        !events.iter().any(|e| matches!(e, Event::Error { .. })),
        "{events:?}"
    );
    let tools: Vec<(String, String)> = events
        .iter()
        .filter_map(|e| match e {
            Event::ToolStarted { tool, target, .. } => Some((tool.clone(), target.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(
        tools,
        [
            (
                "openxer_load".to_string(),
                "critical activities".to_string()
            ),
            ("openxer_apply".to_string(), "1 change".to_string())
        ]
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, Event::ToolFinished { ok: true, .. }))
            .count(),
        2
    );
    let text = messages(&events);
    assert!(text.contains("PM0920"));
    assert!(text.contains("2026-10-12"));
    // The deltas add up to the whole messages.
    assert_eq!(deltas(&events).replace('\n', ""), text.replace('\n', ""));
    let proposal = events
        .iter()
        .find_map(|e| match e {
            Event::ProposedChanges { changes, diff, .. } => Some((changes, diff)),
            _ => None,
        })
        .expect("a proposal");
    assert_eq!(proposal.0["operations"][0]["activity"], "PM0910");
    assert_eq!(
        proposal.1.as_ref().unwrap()["activities"]["changed"]["PM0910"]["task_name"]["after"],
        "Submit as-built drawings and O&M manuals"
    );
    let ends = finished(&events);
    assert_eq!(ends.len(), 2);
    assert_eq!(
        ends[0].0,
        Usage {
            input_tokens: 4 + 5698 + 3746,
            cached_input_tokens: 3746,
            output_tokens: 331
        }
    );
    // Each turn's own cost, not the running total.
    assert!((ends[0].1.unwrap() - 0.0529692).abs() < 1e-6);
    assert!(ends[1].1.unwrap() < 0.02, "{:?}", ends[1]);
}

#[test]
fn the_claude_parser_takes_the_turn_cost_from_the_running_total_when_needed() {
    let mut p = claude::ClaudeParser::new();
    let now = Instant::now();
    let a = p.parse_line(
        r#"{"type":"result","subtype":"success","total_cost_usd":0.05,"usage":{}}"#,
        now,
    );
    let b = p.parse_line(
        r#"{"type":"result","subtype":"success","total_cost_usd":0.07,"usage":{}}"#,
        now,
    );
    assert!((finished(&a)[0].1.unwrap() - 0.05).abs() < 1e-9);
    assert!((finished(&b)[0].1.unwrap() - 0.02).abs() < 1e-9);
}

#[test]
fn the_codex_parser_reads_a_recorded_conversation() {
    let recorded = codex_turns();
    assert_eq!(recorded.len(), 2);
    let (first, parser) = parse_codex(&recorded[0]);
    assert_eq!(
        parser.thread_id.as_deref(),
        Some("00000000-0000-4000-8000-000000000002")
    );
    assert_eq!(
        first[0],
        Event::Ready {
            model: "gpt-test".into()
        }
    );
    let started = first
        .iter()
        .filter(|e| matches!(e, Event::ToolStarted { tool, .. } if tool == "openxer_load"))
        .count();
    let ended = first
        .iter()
        .filter(|e| matches!(e, Event::ToolFinished { ok: true, .. }))
        .count();
    assert_eq!((started, ended), (6, 6));
    assert!(first.iter().any(
        |e| matches!(e, Event::ToolStarted { target, .. } if target == "activities, from row 186")
    ));
    assert!(messages(&first).contains("PM0920"));
    assert_eq!(
        finished(&first),
        [(
            Usage {
                input_tokens: 27086,
                cached_input_tokens: 11520,
                output_tokens: 252
            },
            None,
            false
        )]
    );
    let (second, _) = parse_codex(&recorded[1]);
    let proposal = second
        .iter()
        .find_map(|e| match e {
            Event::ProposedChanges { changes, diff, .. } => Some((changes.clone(), diff.clone())),
            _ => None,
        })
        .expect("a proposal");
    assert_eq!(proposal.0["operations"][0]["field"], "activity_name");
    assert!(proposal.1.is_some());
    assert!(!second.iter().any(|e| matches!(e, Event::Error { .. })));
}

#[test]
fn parsers_report_failures_and_odd_lines() {
    let now = Instant::now();
    let mut c = claude::ClaudeParser::new();
    assert!(c.parse_line("   ", now).is_empty());
    assert!(matches!(
        c.parse_line("not json {", now).as_slice(),
        [Event::Error {
            error: AgentError::BadOutput { .. }
        }]
    ));
    // Unknown kinds are ignored.
    assert!(c
        .parse_line(r#"{"type":"rate_limit_event"}"#, now)
        .is_empty());
    let init = c.parse_line(
        r#"{"type":"system","subtype":"init","session_id":"s","model":"m","mcp_servers":[{"name":"openxer","status":"failed"}]}"#,
        now,
    );
    assert!(
        matches!(&init[1], Event::Error { error: AgentError::Failed { message } } if message.contains("failed"))
    );
    c.parse_line(
        r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"mcp__openxer__openxer_load","input":{"activities":["A1","A2","A3","A4","A5","A6"]}}]}}"#,
        now,
    );
    let failed = c.parse_line(
        r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","is_error":true,"content":[{"type":"text","text":"{\"ok\":false,\"error\":{\"code\":\"invalid_arguments\",\"message\":\"Unknown field colour\"}}"}]}]}}"#,
        now,
    );
    assert_eq!(
        failed,
        [Event::ToolFinished {
            id: "t1".into(),
            tool: "openxer_load".into(),
            ok: false,
            error: Some("Unknown field colour".into()),
            duration_ms: 0
        }]
    );
    let signed_out = c.parse_line(
        r#"{"type":"result","subtype":"success","is_error":true,"result":"Not logged in - Please run /login","usage":{}}"#,
        now,
    );
    assert!(matches!(
        &signed_out[0],
        Event::Error {
            error: AgentError::NotSignedIn {
                agent: Agent::ClaudeCode,
                ..
            }
        }
    ));
    assert!(matches!(signed_out[1], Event::TurnFinished { .. }));
    let other = c.parse_line(
        r#"{"type":"result","subtype":"error_max_turns","is_error":true,"usage":{}}"#,
        now,
    );
    assert!(
        matches!(&other[0], Event::Error { error: AgentError::Failed { message } } if message.contains("error_max_turns"))
    );

    let mut x = codex::CodexParser::new("m");
    assert!(matches!(
        x.parse_line("{broken", now).as_slice(),
        [Event::Error {
            error: AgentError::BadOutput { .. }
        }]
    ));
    // A call finished without a start line, arguments as JSON text, failed.
    let call = x.parse_line(
        r#"{"type":"item.completed","item":{"id":"i9","type":"mcp_tool_call","server":"openxer","tool":"openxer_records","arguments":"{\"kind\":\"risks\"}","status":"failed","error":{"message":"tool call failed"}}}"#,
        now,
    );
    assert_eq!(
        call[0],
        Event::ToolStarted {
            id: "i9".into(),
            tool: "openxer_records".into(),
            target: "risks".into()
        }
    );
    assert!(
        matches!(&call[1], Event::ToolFinished { ok: false, error: Some(e), .. } if e == "tool call failed")
    );
    let failed = x.parse_line(
        r#"{"type":"turn.failed","error":{"message":"unexpected status 401 Unauthorized"}}"#,
        now,
    );
    assert!(matches!(
        &failed[0],
        Event::Error {
            error: AgentError::NotSignedIn {
                agent: Agent::Codex,
                ..
            }
        }
    ));
    assert!(matches!(
        failed[1],
        Event::TurnFinished {
            cancelled: false,
            ..
        }
    ));
    let error = x.parse_line(r#"{"type":"error","message":"stream disconnected"}"#, now);
    assert!(matches!(
        &error[0],
        Event::Error {
            error: AgentError::Failed { .. }
        }
    ));
}

#[test]
fn tool_targets_are_a_few_plain_words() {
    let t = |tool: &str, input: serde_json::Value| tool_target(tool, &input);
    assert_eq!(t("openxer_load", json!({})), "activities");
    assert_eq!(t("openxer_load", json!({"limit":0})), "project summary");
    assert_eq!(t("openxer_load", json!({"activities":["A","B"]})), "A, B");
    assert_eq!(
        t(
            "openxer_load",
            json!({"activities":["1","2","3","4","5","6","7"]})
        ),
        "1, 2, 3, 4, 5 and 2 more"
    );
    assert_eq!(
        t(
            "openxer_load",
            json!({"wbs":"ENG","critical":true,"offset":50})
        ),
        "WBS ENG, critical activities, from row 51"
    );
    assert_eq!(
        t("openxer_records", json!({"kind":"issues","activity":"A1"})),
        "issues of A1"
    );
    assert_eq!(t("openxer_earned_value", json!({})), "project");
    assert_eq!(t("openxer_usage", json!({"resource":"PF"})), "PF");
    assert_eq!(t("openxer_documents", json!({"wbs":"ENG"})), "WBS ENG");
    assert_eq!(
        t("openxer_apply", json!({"changes":{"operations":[{},{}]}})),
        "2 changes"
    );
    assert_eq!(
        t("openxer_playbook", json!({"name":"audit"})),
        "audit playbook"
    );
    assert_eq!(t("openxer_playbook", json!({})), "playbook list");
    assert_eq!(
        t("openxer_check_schedule", json!({"check":"Logic"})),
        "Logic"
    );
    assert_eq!(t("openxer_check_schedule", json!({})), "all checks");
    assert_eq!(t("other", json!({})), "");
}

// ------------------------------------------------------------- sandbox

fn options(agent: Agent, dir: &Path, mode: ChatMode, model: ModelChoice) -> SessionOptions {
    SessionOptions {
        agent,
        executable: dir.join("agent.exe"),
        model,
        mode,
        cli: dir.join("openxer-cli.exe"),
        data_dir: dir.join("data"),
        work_dir: dir.join("work"),
        snapshot: dir.join("snapshot.xer"),
        preamble: "Open project: SAMPLE.".into(),
        transcript: None,
        user_instructions: None,
        project_instructions: None,
        playbooks_dir: None,
        ui_actions: None,
        keep_files: false,
    }
}

const UUID: &str = "00000000-0000-4000-8000-0000000000c1";

fn after<'a>(args: &'a [String], flag: &str) -> &'a str {
    let i = args
        .iter()
        .position(|a| a == flag)
        .unwrap_or_else(|| panic!("{flag} in {args:?}"));
    &args[i + 1]
}

#[test]
fn claude_runs_with_no_tools_but_openxers_and_none_of_the_users_settings() {
    let dir = tempfile::tempdir().unwrap();
    let o = options(
        Agent::ClaudeCode,
        dir.path(),
        ChatMode::Ask,
        ModelChoice::AgentDefault,
    );
    let config = claude::config_path(&o);
    let args = claude::arguments(&o, &config, Path::new("RULES.md"), None).unwrap();
    for flag in [
        "-p",
        "--restricted",
        "--strict-mcp-config",
        "--disable-slash-commands",
    ] {
        assert!(args.iter().any(|a| a == flag), "{flag}");
    }
    assert_eq!(after(&args, "--tools"), "");
    assert_eq!(after(&args, "--permission-prompts"), "none");
    // The instructions come from a file, never on the command line.
    assert_eq!(after(&args, "--system-prompt-file"), "RULES.md");
    assert!(!args.iter().any(|a| a == "--system-prompt"));
    assert_eq!(after(&args, "--input-format"), "stream-json");
    assert_eq!(after(&args, "--mcp-config"), config.display().to_string());
    let allowed = after(&args, "--allowedTools");
    assert!(!allowed.contains("openxer_apply"), "Ask mode: {allowed}");
    assert!(allowed
        .split(',')
        .all(|t| t.starts_with("mcp__openxer__openxer_")));
    assert!(!args.iter().any(|a| a == "--model" || a == "--resume"));
    let edit = options(
        Agent::ClaudeCode,
        dir.path(),
        ChatMode::Edit,
        ModelChoice::LowerCost,
    );
    let args = claude::arguments(&edit, &config, Path::new("RULES.md"), Some(UUID)).unwrap();
    assert!(after(&args, "--allowedTools").contains("mcp__openxer__openxer_apply"));
    assert_eq!(after(&args, "--model"), claude::LOWER_COST_MODEL);
    assert_eq!(after(&args, "--resume"), UUID);
    assert!(claude::arguments(&edit, &config, Path::new("RULES.md"), Some("-x")).is_err());
    let named = options(
        Agent::ClaudeCode,
        dir.path(),
        ChatMode::Ask,
        ModelChoice::Named("opus".into()),
    );
    assert_eq!(
        after(
            &claude::arguments(&named, &config, Path::new("R.md"), None).unwrap(),
            "--model"
        ),
        "opus"
    );
    // The MCP config holds only the chat server, on the snapshot.
    let c = claude::mcp_config(&edit);
    let servers = c["mcpServers"].as_object().unwrap();
    assert_eq!(servers.len(), 1);
    let args: Vec<&str> = servers["openxer"]["args"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a.as_str().unwrap())
        .collect();
    assert_eq!(&args[..3], ["mcp", "--toolset", "chat"]);
    assert!(args.contains(&"--snapshot") && args.contains(&"edit"));
    assert_eq!(
        servers["openxer"]["env"]["OPENXER_DATA_DIR"],
        json!(dir.path().join("data"))
    );
    assert_eq!(
        claude::user_line("hi"),
        r#"{"message":{"content":[{"text":"hi","type":"text"}],"role":"user"},"type":"user"}"#
    );
}

#[test]
fn codex_runs_read_only_with_its_extras_off_and_only_the_openxer_server() {
    let dir = tempfile::tempdir().unwrap();
    let o = options(
        Agent::Codex,
        dir.path(),
        ChatMode::Ask,
        ModelChoice::AgentDefault,
    );
    let args = codex::arguments(&o, None).unwrap();
    assert_eq!(args[0], "exec");
    assert_ne!(args[1], "resume");
    for flag in [
        "--json",
        "--skip-git-repo-check",
        "--ignore-user-config",
        "--ignore-rules",
    ] {
        assert!(args.iter().any(|a| a == flag), "{flag}");
    }
    assert!(
        !args.iter().any(|a| a == "--ephemeral"),
        "resume needs the saved thread"
    );
    let configs: Vec<&str> = args
        .windows(2)
        .filter(|w| w[0] == "-c")
        .map(|w| w[1].as_str())
        .collect();
    for expected in [
        "sandbox_mode=\"read-only\"",
        "approval_policy=\"never\"",
        "web_search=\"disabled\"",
        "include_environment_context=false",
        "project_doc_max_bytes=0",
        "mcp_servers.openxer.startup_timeout_sec=30",
    ] {
        assert!(configs.contains(&expected), "{expected} in {configs:?}");
    }
    let server_args = configs
        .iter()
        .find(|c| c.starts_with("mcp_servers.openxer.args="))
        .unwrap();
    assert!(
        server_args.contains("\"--toolset\", \"chat\""),
        "{server_args}"
    );
    assert!(server_args.contains("\"ask\""));
    assert!(configs
        .iter()
        .any(|c| c.starts_with("mcp_servers.openxer.env={OPENXER_DATA_DIR=")));
    assert!(configs
        .iter()
        .any(|c| c.starts_with("model_instructions_file=")));
    let disabled: Vec<&str> = args
        .windows(2)
        .filter(|w| w[0] == "--disable")
        .map(|w| w[1].as_str())
        .collect();
    for feature in [
        "shell_tool",
        "unified_exec",
        "multi_agent",
        "plugins",
        "apps",
    ] {
        assert!(disabled.contains(&feature), "{feature}");
    }
    assert_eq!(
        &args[args.len() - 2..],
        ["--", "-"],
        "the prompt comes on stdin"
    );
    assert!(!args.iter().any(|a| a == "-m"));
    let cheap = options(
        Agent::Codex,
        dir.path(),
        ChatMode::Edit,
        ModelChoice::LowerCost,
    );
    let args = codex::arguments(&cheap, Some(UUID)).unwrap();
    assert_eq!(&args[..2], ["exec", "resume"]);
    assert_eq!(&args[args.len() - 3..], ["--", UUID, "-"]);
    assert!(codex::arguments(&cheap, Some("--dangerously-bypass-approvals-and-sandbox")).is_err());
    assert_eq!(after(&args, "-m"), codex::LOWER_COST_MODEL);
    assert!(args.iter().any(|a| a == "model_reasoning_effort=\"low\""));
    // A path with a quote stays one TOML string.
    let mut odd = options(
        Agent::Codex,
        dir.path(),
        ChatMode::Ask,
        ModelChoice::AgentDefault,
    );
    odd.cli = PathBuf::from("C:\\it's here\\openxer-cli.exe");
    let args = codex::arguments(&odd, None).unwrap();
    let command = args
        .iter()
        .find(|a| a.starts_with("mcp_servers.openxer.command="))
        .unwrap();
    let value: toml_edit::Value = command.split_once('=').unwrap().1.parse().unwrap();
    assert_eq!(value.as_str(), Some("C:\\it's here\\openxer-cli.exe"));
}

#[test]
fn the_rules_tell_the_agent_what_its_mode_allows() {
    let dir = tempfile::tempdir().unwrap();
    let mut o = options(
        Agent::Codex,
        dir.path(),
        ChatMode::Ask,
        ModelChoice::AgentDefault,
    );
    let ask = agent_host::rules(&o);
    o.mode = ChatMode::Edit;
    o.user_instructions = Some("Write dates as 12-Mar-27.".into());
    let edit = agent_host::rules(&o);
    assert!(ask.starts_with(openxer_automation::chat_prompt::CORE));
    assert!(ask.ends_with(openxer_automation::chat_prompt::mode_line(ChatMode::Ask)));
    assert!(ask.contains("read only"));
    assert!(edit.contains("openxer_apply") && edit.contains("approves"));
    assert!(edit.contains("Write dates as 12-Mar-27."));
    assert!(!ask.contains('\u{2014}') && !edit.contains('\u{2014}'));
    let choices = model_choices(Agent::Codex);
    assert_eq!(choices[0].0, ModelChoice::AgentDefault);
    assert!(choices[1].1.contains(codex::LOWER_COST_MODEL));
    assert!(model_choices(Agent::ClaudeCode)[1].1.contains("sonnet"));
}

// ----------------------------------------------------------- fake agent

#[derive(Clone, Debug)]
enum Step {
    Line(String),
    /// Wait for one stdin line.
    Read,
    /// Keep running (and the output open) until stopped.
    Hang,
    /// Raw output bytes, as they are.
    Bytes(Vec<u8>),
    /// Keep running until stopped, then keep the output open anyway, as a
    /// grandchild that holds the pipe would.
    Orphan,
    Exit(i32),
}

#[derive(Clone, Debug, Default)]
struct Script {
    steps: Vec<Step>,
    stderr: Vec<u8>,
}

fn lines(list: &[String]) -> Vec<Step> {
    list.iter().map(|l| Step::Line(l.clone())).collect()
}

#[derive(Default)]
struct Fake {
    scripts: Mutex<VecDeque<Script>>,
    launches: Mutex<Vec<Launch>>,
    stdin: Arc<Mutex<Vec<String>>>,
    alive: Arc<AtomicUsize>,
    missing: bool,
}

impl Fake {
    fn new(scripts: Vec<Script>) -> Arc<Fake> {
        Arc::new(Fake {
            scripts: Mutex::new(scripts.into()),
            ..Default::default()
        })
    }
    fn launches(&self) -> Vec<Launch> {
        self.launches.lock().unwrap().clone()
    }
    fn written(&self) -> Vec<String> {
        self.stdin.lock().unwrap().clone()
    }
    fn alive(&self) -> usize {
        self.alive.load(Ordering::SeqCst)
    }
}

struct ChannelReader {
    receive: Receiver<Vec<u8>>,
    buffer: Vec<u8>,
}

impl Read for ChannelReader {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if self.buffer.is_empty() {
            match self.receive.recv() {
                Ok(bytes) => self.buffer = bytes,
                Err(_) => return Ok(0),
            }
        }
        let n = out.len().min(self.buffer.len());
        out[..n].copy_from_slice(&self.buffer[..n]);
        self.buffer.drain(..n);
        Ok(n)
    }
}

struct ChannelWriter {
    send: Sender<String>,
    log: Arc<Mutex<Vec<String>>>,
    partial: Vec<u8>,
}

impl Write for ChannelWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.partial.extend_from_slice(bytes);
        while let Some(i) = self.partial.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = self.partial.drain(..=i).collect();
            let line = String::from_utf8_lossy(&line).trim_end().to_string();
            self.log.lock().unwrap().push(line.clone());
            // A script that no longer reads is a process that has its
            // input already; the bytes are kept in the log either way.
            let _ = self.send.send(line);
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for ChannelWriter {
    /// Closing stdin delivers a last line without a line end.
    fn drop(&mut self) {
        if !self.partial.is_empty() {
            let line = String::from_utf8_lossy(&self.partial).to_string();
            self.log.lock().unwrap().push(line);
        }
    }
}

struct FakeProcess {
    stdin: Option<Box<dyn Write + Send>>,
    stdout: Option<Box<dyn Read + Send>>,
    stderr: Option<Box<dyn Read + Send>>,
    killed: Arc<AtomicBool>,
    exit: Arc<Mutex<Option<i32>>>,
}

impl AgentProcess for FakeProcess {
    fn take_stdin(&mut self) -> Option<Box<dyn Write + Send>> {
        self.stdin.take()
    }
    fn take_stdout(&mut self) -> Option<Box<dyn Read + Send>> {
        self.stdout.take()
    }
    fn take_stderr(&mut self) -> Option<Box<dyn Read + Send>> {
        self.stderr.take()
    }
    fn kill(&mut self) -> io::Result<()> {
        self.killed.store(true, Ordering::SeqCst);
        Ok(())
    }
    fn try_wait(&mut self) -> io::Result<Option<i32>> {
        Ok(*self.exit.lock().unwrap())
    }
}

impl Drop for FakeProcess {
    fn drop(&mut self) {
        self.killed.store(true, Ordering::SeqCst);
    }
}

impl Spawner for Fake {
    fn spawn(&self, launch: &Launch) -> io::Result<Box<dyn AgentProcess>> {
        if self.missing {
            return Err(io::Error::new(io::ErrorKind::NotFound, "not found"));
        }
        self.launches.lock().unwrap().push(launch.clone());
        let script = self.scripts.lock().unwrap().pop_front().unwrap_or_default();
        let (out, receive) = mpsc::channel::<Vec<u8>>();
        let (send_in, read_in) = mpsc::channel::<String>();
        let killed = Arc::new(AtomicBool::new(false));
        let exit = Arc::new(Mutex::new(None));
        self.alive.fetch_add(1, Ordering::SeqCst);
        let (k, e, alive) = (killed.clone(), exit.clone(), self.alive.clone());
        std::thread::spawn(move || {
            let mut code = 0;
            'steps: for step in script.steps {
                if k.load(Ordering::SeqCst) {
                    break;
                }
                match step {
                    Step::Line(l) => {
                        let _ = out.send(format!("{l}\n").into_bytes());
                    }
                    Step::Read => loop {
                        if k.load(Ordering::SeqCst) {
                            break 'steps;
                        }
                        match read_in.recv_timeout(Duration::from_millis(5)) {
                            Ok(_) => break,
                            Err(RecvTimeoutError::Timeout) => continue,
                            Err(RecvTimeoutError::Disconnected) => break 'steps,
                        }
                    },
                    Step::Hang => {
                        while !k.load(Ordering::SeqCst) {
                            std::thread::sleep(Duration::from_millis(5));
                        }
                    }
                    Step::Bytes(b) => {
                        let _ = out.send(b);
                    }
                    Step::Orphan => {
                        while !k.load(Ordering::SeqCst) {
                            std::thread::sleep(Duration::from_millis(5));
                        }
                        *e.lock().unwrap() = Some(-1);
                        alive.fetch_sub(1, Ordering::SeqCst);
                        // The pipe stays open long after the process ended.
                        std::thread::sleep(Duration::from_secs(30));
                        drop(out);
                        return;
                    }
                    Step::Exit(c) => {
                        code = c;
                        break;
                    }
                }
            }
            *e.lock().unwrap() = Some(if k.load(Ordering::SeqCst) { -1 } else { code });
            alive.fetch_sub(1, Ordering::SeqCst);
            drop(out);
        });
        Ok(Box::new(FakeProcess {
            stdin: launch.stdin.then(|| {
                Box::new(ChannelWriter {
                    send: send_in,
                    log: self.stdin.clone(),
                    partial: vec![],
                }) as Box<dyn Write + Send>
            }),
            stdout: Some(Box::new(ChannelReader {
                receive,
                buffer: vec![],
            })),
            stderr: Some(Box::new(io::Cursor::new(script.stderr))),
            killed,
            exit,
        }))
    }
}

/// Events up to and including the turn's end.
fn until_turn_end(events: &Receiver<Event>) -> Vec<Event> {
    let mut out = vec![];
    loop {
        let e = events
            .recv_timeout(Duration::from_secs(10))
            .unwrap_or_else(|_| panic!("the turn ends; so far {out:?}"));
        let done = matches!(e, Event::TurnFinished { .. });
        out.push(e);
        if done {
            return out;
        }
    }
}

fn wait_for(what: &str, mut check: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !check() {
        assert!(Instant::now() < deadline, "{what}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn start(agent: Agent, dir: &Path, mode: ChatMode, fake: &Arc<Fake>) -> (Session, Receiver<Event>) {
    Session::start(
        options(agent, dir, mode, ModelChoice::AgentDefault),
        fake.clone(),
    )
    .unwrap()
}

// --------------------------------------------------------------- sessions

#[test]
fn a_claude_conversation_keeps_one_process_for_every_turn() {
    let dir = tempfile::tempdir().unwrap();
    let t = claude_turns();
    let mut steps = vec![Step::Read];
    steps.extend(lines(&t[0]));
    steps.push(Step::Read);
    steps.extend(lines(&t[1]));
    steps.push(Step::Hang);
    let fake = Fake::new(vec![Script {
        steps,
        ..Default::default()
    }]);
    let (mut session, events) = start(Agent::ClaudeCode, dir.path(), ChatMode::Edit, &fake);
    // The config the agent reads is written; nothing starts before a message.
    let config: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.path().join("work/openxer-chat-mcp.json")).unwrap(),
    )
    .unwrap();
    assert!(config["mcpServers"]["openxer"].is_object());
    assert!(fake.launches().is_empty());
    session
        .send("Which activity finishes last?", Some("Selected: PM0910"))
        .unwrap();
    assert!(session.is_busy());
    assert_eq!(session.send("again", None), Err(AgentError::Busy));
    let first = until_turn_end(&events);
    assert!(matches!(first[0], Event::Ready { .. }));
    assert!(messages(&first).contains("PM0920"));
    assert!(!session.is_busy());
    assert_eq!(
        session.conversation_id().as_deref(),
        Some("00000000-0000-4000-8000-000000000001")
    );
    session.send("Propose the rename.", None).unwrap();
    let second = until_turn_end(&events);
    assert!(
        !second.iter().any(|e| matches!(e, Event::Ready { .. })),
        "Ready once"
    );
    assert!(second
        .iter()
        .any(|e| matches!(e, Event::ProposedChanges { .. })));
    // One process, two stdin lines; the preamble and context only where due.
    let launches = fake.launches();
    assert_eq!(launches.len(), 1);
    assert!(launches[0].stdin);
    assert_eq!(
        launches[0].cwd.as_deref(),
        Some(dir.path().join("work").as_path())
    );
    assert!(launches[0].env.contains(&(
        "OPENXER_DATA_DIR".into(),
        dir.path().join("data").display().to_string()
    )));
    let written = fake.written();
    assert_eq!(written.len(), 2);
    let one: serde_json::Value = serde_json::from_str(&written[0]).unwrap();
    let text = one["message"]["content"][0]["text"].as_str().unwrap();
    assert_eq!(
        text,
        "Open project: SAMPLE.\n\nContext: Selected: PM0910\n\nWhich activity finishes last?"
    );
    let two: serde_json::Value = serde_json::from_str(&written[1]).unwrap();
    assert_eq!(two["message"]["content"][0]["text"], "Propose the rename.");
    session.end();
    wait_for("the process stops", || fake.alive() == 0);
}

#[test]
fn a_codex_conversation_resumes_its_thread_for_each_turn() {
    let dir = tempfile::tempdir().unwrap();
    let t = codex_turns();
    let script = |l: &[String]| {
        let mut steps = lines(l);
        steps.push(Step::Exit(0));
        Script {
            steps,
            ..Default::default()
        }
    };
    let fake = Fake::new(vec![script(&t[0]), script(&t[1])]);
    let (mut session, events) = start(Agent::Codex, dir.path(), ChatMode::Edit, &fake);
    assert!(
        std::fs::read_to_string(dir.path().join("work/openxer-chat-instructions.md"))
            .unwrap()
            .contains("openxer_apply")
    );
    session.send("Which activity finishes last?", None).unwrap();
    let first = until_turn_end(&events);
    assert_eq!(
        first[0],
        Event::Ready {
            model: "Codex default".into()
        }
    );
    session.send("Propose the rename.", None).unwrap();
    let second = until_turn_end(&events);
    assert!(!second.iter().any(|e| matches!(e, Event::Ready { .. })));
    assert!(second
        .iter()
        .any(|e| matches!(e, Event::ProposedChanges { .. })));
    let launches = fake.launches();
    assert_eq!(launches.len(), 2);
    assert!(
        launches[0].stdin && launches[1].stdin,
        "the prompt goes on stdin"
    );
    assert_ne!(launches[0].args[1], "resume");
    assert_eq!(&launches[1].args[..2], ["exec", "resume"]);
    let second = &launches[1].args;
    assert_eq!(
        &second[second.len() - 3..],
        ["--", "00000000-0000-4000-8000-000000000002", "-"]
    );
    let written = fake.written().join("\n");
    assert!(
        written.starts_with("Open project: SAMPLE.\n\n"),
        "{written}"
    );
    assert!(written.ends_with("Propose the rename."), "{written}");
    wait_for("both turns' processes ended", || fake.alive() == 0);
}

fn claude_turn(lines_of_turn: &[&str]) -> Script {
    let mut steps = vec![Step::Read];
    steps.extend(lines_of_turn.iter().map(|l| Step::Line(l.to_string())));
    steps.push(Step::Hang);
    Script {
        steps,
        ..Default::default()
    }
}

const INIT: &str = r#"{"type":"system","subtype":"init","session_id":"00000000-0000-4000-8000-0000000000b1","model":"m","mcp_servers":[{"name":"openxer","status":"connected"}]}"#;
const RESULT: &str = r#"{"type":"result","subtype":"success","total_cost_usd":0.01,"usage":{"input_tokens":10,"output_tokens":2}}"#;

#[test]
fn a_failed_tool_call_is_reported_and_the_turn_goes_on() {
    let dir = tempfile::tempdir().unwrap();
    let fake = Fake::new(vec![claude_turn(&[
        INIT,
        r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t","name":"mcp__openxer__openxer_load","input":{"fields":["colour"]}}]}}"#,
        r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t","is_error":true,"content":"{\"ok\":false,\"error\":{\"message\":\"Unknown field colour\"}}"}]}}"#,
        r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Retried."}]}}"#,
        RESULT,
    ])]);
    let (mut session, events) = start(Agent::ClaudeCode, dir.path(), ChatMode::Ask, &fake);
    session.send("q", None).unwrap();
    let got = until_turn_end(&events);
    assert!(got.iter().any(|e| matches!(e,
        Event::ToolFinished { ok: false, error: Some(m), tool, .. } if m == "Unknown field colour" && tool == "openxer_load")));
    assert_eq!(messages(&got), "Retried.");
    assert!(!got
        .iter()
        .any(|e| matches!(e, Event::ProposedChanges { .. })));
}

#[test]
fn a_failed_apply_proposes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let fake = Fake::new(vec![claude_turn(&[
        INIT,
        r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t","name":"mcp__openxer__openxer_apply","input":{"changes":{"format":"openxer-changes/1","operations":[{"op":"delete_activity","activity":"NOPE"}]}}}]}}"#,
        r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t","content":[{"type":"text","text":"{\"ok\":false,\"error\":{\"message\":\"Unknown activity NOPE\"}}"}]}]}}"#,
        RESULT,
    ])]);
    let (mut session, events) = start(Agent::ClaudeCode, dir.path(), ChatMode::Edit, &fake);
    session.send("q", None).unwrap();
    let got = until_turn_end(&events);
    assert!(got
        .iter()
        .any(|e| matches!(e, Event::ToolFinished { ok: false, .. })));
    assert!(!got
        .iter()
        .any(|e| matches!(e, Event::ProposedChanges { .. })));
}

#[test]
fn cancel_stops_the_reply_at_once_and_the_next_message_resumes() {
    let dir = tempfile::tempdir().unwrap();
    let mut stuck = claude_turn(&[
        INIT,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"Think"}}}"#,
    ]);
    stuck.steps.pop();
    stuck.steps.push(Step::Hang);
    let fake = Fake::new(vec![
        stuck,
        claude_turn(&[
            INIT,
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Done."}]}}"#,
            RESULT,
        ]),
    ]);
    let (mut session, events) = start(Agent::ClaudeCode, dir.path(), ChatMode::Ask, &fake);
    session.send("long question", None).unwrap();
    loop {
        let e = events.recv_timeout(Duration::from_secs(10)).unwrap();
        if matches!(e, Event::TextDelta { .. }) {
            break;
        }
    }
    let asked = Instant::now();
    session.cancel();
    let e = events.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(
        e,
        Event::TurnFinished {
            usage: Usage::default(),
            cost_usd: None,
            cancelled: true
        }
    );
    assert!(
        asked.elapsed() < Duration::from_secs(3),
        "{:?}",
        asked.elapsed()
    );
    assert!(!session.is_busy());
    wait_for("the cancelled process stops", || fake.alive() == 0);
    // Cancel with no reply running does nothing.
    session.cancel();
    session.send("next", None).unwrap();
    let got = until_turn_end(&events);
    assert_eq!(messages(&got), "Done.");
    assert!(!got.iter().any(|e| matches!(e, Event::Ready { .. })));
    let launches = fake.launches();
    assert_eq!(launches.len(), 2);
    assert_eq!(
        after(&launches[1].args, "--resume"),
        "00000000-0000-4000-8000-0000000000b1"
    );
}

#[test]
fn an_agent_that_stops_early_is_reported_with_its_error_output() {
    let dir = tempfile::tempdir().unwrap();
    let mut script = claude_turn(&[INIT]);
    script.steps.pop();
    script.steps.push(Step::Exit(3));
    script.stderr = "something broke\nfatal: out of memory\n".into();
    let fake = Fake::new(vec![script]);
    let (mut session, events) = start(Agent::ClaudeCode, dir.path(), ChatMode::Ask, &fake);
    session.send("q", None).unwrap();
    let got = until_turn_end(&events);
    let died = got
        .iter()
        .find_map(|e| match e {
            Event::Error {
                error: AgentError::ProcessDied { message },
            } => Some(message.clone()),
            _ => None,
        })
        .expect("ProcessDied");
    assert!(died.contains("exit code 3"), "{died}");
    assert!(died.contains("out of memory"), "{died}");
    assert!(matches!(
        got.last(),
        Some(Event::TurnFinished {
            cancelled: false,
            ..
        })
    ));
    assert!(!session.is_busy());
}

#[test]
fn a_missing_agent_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let fake = Arc::new(Fake {
        missing: true,
        ..Default::default()
    });
    for agent in Agent::ALL {
        let (mut session, _events) = start(agent, dir.path(), ChatMode::Ask, &fake);
        assert_eq!(
            session.send("q", None),
            Err(AgentError::NotInstalled { agent })
        );
        assert!(!session.is_busy());
    }
    let mut o = options(
        Agent::Codex,
        dir.path(),
        ChatMode::Ask,
        ModelChoice::AgentDefault,
    );
    o.executable = PathBuf::new();
    assert!(matches!(
        Session::start(o, fake.clone()),
        Err(AgentError::NotInstalled {
            agent: Agent::Codex
        })
    ));
}

#[test]
fn a_signed_out_agent_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let fake = Fake::new(vec![claude_turn(&[
        INIT,
        r#"{"type":"result","subtype":"success","is_error":true,"result":"Not logged in - Please run /login","usage":{}}"#,
    ])]);
    let (mut session, events) = start(Agent::ClaudeCode, dir.path(), ChatMode::Ask, &fake);
    session.send("q", None).unwrap();
    let got = until_turn_end(&events);
    assert!(got.iter().any(|e| matches!(
        e,
        Event::Error {
            error: AgentError::NotSignedIn {
                agent: Agent::ClaudeCode,
                ..
            }
        }
    )));
    // Codex that exits at once with a sign-in message on its error output.
    let fake = Fake::new(vec![Script {
        steps: vec![Step::Exit(1)],
        stderr: "Error: unexpected status 401 Unauthorized: please sign in again\n".into(),
    }]);
    let (mut session, events) = start(Agent::Codex, dir.path(), ChatMode::Ask, &fake);
    session.send("q", None).unwrap();
    let got = until_turn_end(&events);
    assert!(
        got.iter().any(|e| matches!(
            e,
            Event::Error {
                error: AgentError::NotSignedIn {
                    agent: Agent::Codex,
                    ..
                }
            }
        )),
        "{got:?}"
    );
}

#[test]
fn malformed_lines_are_reported_and_reading_goes_on() {
    let dir = tempfile::tempdir().unwrap();
    let fake = Fake::new(vec![claude_turn(&[
        INIT,
        "Warning: not a JSON line",
        r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Fine."}]}}"#,
        "{\"type\":",
        RESULT,
    ])]);
    let (mut session, events) = start(Agent::ClaudeCode, dir.path(), ChatMode::Ask, &fake);
    session.send("q", None).unwrap();
    let got = until_turn_end(&events);
    let bad = got
        .iter()
        .filter(|e| {
            matches!(
                e,
                Event::Error {
                    error: AgentError::BadOutput { .. }
                }
            )
        })
        .count();
    assert_eq!(bad, 2);
    assert_eq!(messages(&got), "Fine.");
}

#[test]
fn dropping_a_session_stops_its_agent() {
    let dir = tempfile::tempdir().unwrap();
    let fake = Fake::new(vec![claude_turn(&[INIT])]);
    let (mut session, events) = start(Agent::ClaudeCode, dir.path(), ChatMode::Ask, &fake);
    session.send("q", None).unwrap();
    assert!(matches!(
        events.recv_timeout(Duration::from_secs(10)).unwrap(),
        Event::Ready { .. }
    ));
    assert_eq!(fake.alive(), 1);
    drop(session);
    wait_for("the agent stops", || fake.alive() == 0);
    // Nothing more is reported once the session is gone.
    let rest: Vec<Event> = events.try_iter().collect();
    assert!(rest.is_empty(), "{rest:?}");
    let ended = events.recv_timeout(Duration::from_secs(5));
    assert!(ended.is_err());
}

#[test]
fn the_transcript_keeps_the_raw_lines() {
    let dir = tempfile::tempdir().unwrap();
    let fake = Fake::new(vec![claude_turn(&[INIT, RESULT])]);
    let mut o = options(
        Agent::ClaudeCode,
        dir.path(),
        ChatMode::Ask,
        ModelChoice::AgentDefault,
    );
    let log = dir.path().join("t.jsonl");
    o.transcript = Some(log.clone());
    let (mut session, events) = Session::start(o, fake.clone()).unwrap();
    session.send("q", None).unwrap();
    until_turn_end(&events);
    assert_eq!(
        std::fs::read_to_string(&log).unwrap(),
        format!("{INIT}\n{RESULT}\n")
    );
}

// ---------------------------------------------------- detection, sign-in

/// A fake whose every process prints `stdout` and `stderr` and exits.
fn printing(outputs: Vec<(&str, &str, i32)>) -> Arc<Fake> {
    Fake::new(
        outputs
            .into_iter()
            .map(|(out, err, code)| Script {
                steps: out
                    .lines()
                    .map(|l| Step::Line(l.into()))
                    .chain([Step::Exit(code)])
                    .collect(),
                stderr: err.into(),
            })
            .collect(),
    )
}

#[test]
fn detection_without_an_environment_finds_nothing_and_runs_nothing() {
    let fake = printing(vec![]);
    for status in agent_host::detect_all(&Env::default(), fake.as_ref()) {
        assert!(!status.installed);
        assert_eq!(status.executable, None);
        assert_eq!(status.signed_in, None);
    }
    assert!(fake.launches().is_empty());
}

#[test]
fn detection_reads_the_version_and_the_sign_in() {
    let dir = tempfile::tempdir().unwrap();
    let exe = if cfg!(windows) { ".exe" } else { "" };
    std::fs::write(dir.path().join(format!("claude{exe}")), "").unwrap();
    std::fs::write(dir.path().join(format!("codex{exe}")), "").unwrap();
    let env = Env {
        path: Some(dir.path().as_os_str().to_owned()),
        ..Default::default()
    };
    let fake = printing(vec![
        ("2.1.330 (Claude Code)\n", "", 0),
        (r#"{"loggedIn": true, "authMethod": "claude.ai"}"#, "", 0),
        ("codex-cli 0.166.0\n", "", 0),
        ("", "Not logged in\n", 1),
    ]);
    let all = agent_host::detect_all(&env, fake.as_ref());
    assert_eq!(all[0].agent, Agent::ClaudeCode);
    assert!(all[0].installed);
    assert_eq!(all[0].version.as_deref(), Some("2.1.330 (Claude Code)"));
    assert_eq!(all[0].signed_in, Some(true));
    assert_eq!(all[1].version.as_deref(), Some("codex-cli 0.166.0"));
    assert_eq!(all[1].signed_in, Some(false));
    let args: Vec<Vec<String>> = fake.launches().iter().map(|l| l.args.clone()).collect();
    assert_eq!(
        args,
        [
            vec!["--version".to_string()],
            agent_host::sign_in_status_args(Agent::ClaudeCode),
            vec!["--version".to_string()],
            agent_host::sign_in_status_args(Agent::Codex),
        ]
    );
    assert_eq!(fake.alive(), 0);
}

#[test]
fn sign_in_status_is_read_from_each_agents_own_words() {
    let c = |out: &str, err: &str, code: i32| Captured {
        code,
        stdout: out.into(),
        stderr: err.into(),
    };
    assert_eq!(
        parse_sign_in_status(Agent::ClaudeCode, &c(r#"{"loggedIn":false}"#, "", 1)),
        Some(false)
    );
    assert_eq!(
        parse_sign_in_status(Agent::ClaudeCode, &c("garbage", "", 0)),
        None
    );
    assert_eq!(
        parse_sign_in_status(Agent::Codex, &c("", "Logged in using ChatGPT\n", 0)),
        Some(true)
    );
    assert_eq!(
        parse_sign_in_status(Agent::Codex, &c("Logged in using an API key", "", 0)),
        Some(true)
    );
    assert_eq!(
        parse_sign_in_status(Agent::Codex, &c("", "Not logged in", 1)),
        Some(false)
    );
    assert_eq!(parse_sign_in_status(Agent::Codex, &c("", "", 0)), None);
}

#[test]
fn sign_in_starts_the_agents_own_login_and_reports_its_end() {
    let fake = printing(vec![("Opening https://example.invalid/login\n", "", 0)]);
    let mut sign_in = SignIn::start(Agent::Codex, Path::new("codex.exe"), fake.as_ref()).unwrap();
    assert_eq!(fake.launches()[0].args, ["login"]);
    wait_for("the login ends", || {
        sign_in.poll() == SignInState::Finished { ok: true }
    });
    wait_for("its output is read", || !sign_in.output().is_empty());
    assert!(sign_in.output()[0].contains("example.invalid"));
    // A sign-in that is still running is stopped when cancelled or dropped.
    let fake = Fake::new(vec![Script {
        steps: vec![Step::Hang],
        ..Default::default()
    }]);
    let mut waiting =
        SignIn::start(Agent::ClaudeCode, Path::new("claude.exe"), fake.as_ref()).unwrap();
    assert_eq!(
        fake.launches()[0].args,
        agent_host::sign_in_args(Agent::ClaudeCode)
    );
    assert_eq!(waiting.poll(), SignInState::Running);
    drop(waiting);
    wait_for("the login stops", || fake.alive() == 0);
    let missing = Arc::new(Fake {
        missing: true,
        ..Default::default()
    });
    assert!(matches!(
        SignIn::start(Agent::Codex, Path::new("codex.exe"), missing.as_ref()),
        Err(AgentError::NotInstalled {
            agent: Agent::Codex
        })
    ));
}

// ------------------------------------------------- review fixes (tests)

#[test]
fn cancel_ends_the_turn_even_when_the_output_stays_open() {
    let dir = tempfile::tempdir().unwrap();
    let mut stuck = claude_turn(&[INIT]);
    stuck.steps.pop();
    stuck.steps.push(Step::Orphan);
    let fake = Fake::new(vec![
        stuck,
        claude_turn(&[
            INIT,
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Back."}]}}"#,
            RESULT,
        ]),
    ]);
    let (mut session, events) = start(Agent::ClaudeCode, dir.path(), ChatMode::Ask, &fake);
    session.send("q", None).unwrap();
    assert!(matches!(
        events.recv_timeout(Duration::from_secs(10)).unwrap(),
        Event::Ready { .. }
    ));
    session.cancel();
    assert_eq!(
        events.recv_timeout(Duration::from_secs(2)).unwrap(),
        Event::TurnFinished {
            usage: Usage::default(),
            cost_usd: None,
            cancelled: true
        }
    );
    assert!(!session.is_busy());
    // The next message works although the old output never ended.
    session.send("next", None).unwrap();
    assert_eq!(messages(&until_turn_end(&events)), "Back.");
    // Nothing more comes from the cancelled process.
    std::thread::sleep(Duration::from_millis(50));
    assert!(events.try_iter().next().is_none());
}

#[test]
fn invalid_utf8_output_is_read_and_the_turn_ends_normally() {
    let dir = tempfile::tempdir().unwrap();
    let mut bad =
        br#"{"type":"assistant","message":{"content":[{"type":"text","text":"caf"#.to_vec();
    bad.push(0xE9);
    bad.extend_from_slice(b"\"}]}}\n");
    let mut script = claude_turn(&[INIT]);
    script.steps.pop();
    script.steps.push(Step::Bytes(bad));
    script.steps.push(Step::Line(RESULT.into()));
    script.steps.push(Step::Hang);
    let fake = Fake::new(vec![script]);
    let (mut session, events) = start(Agent::ClaudeCode, dir.path(), ChatMode::Ask, &fake);
    session.send("q", None).unwrap();
    let got = until_turn_end(&events);
    assert_eq!(messages(&got), "caf\u{FFFD}");
    assert!(
        !got.iter().any(|e| matches!(e, Event::Error { .. })),
        "{got:?}"
    );
    assert!(matches!(
        got.last(),
        Some(Event::TurnFinished {
            cancelled: false,
            cost_usd: Some(_),
            ..
        })
    ));
}

#[test]
fn invalid_utf8_error_output_is_still_read_to_its_end() {
    let dir = tempfile::tempdir().unwrap();
    let mut stderr = vec![0xFF, 0xFE, b' ', b'x', b'\n'];
    stderr.extend_from_slice(b"Error: unexpected status 401 Unauthorized\n");
    let fake = Fake::new(vec![Script {
        steps: vec![Step::Exit(1)],
        stderr,
    }]);
    let (mut session, events) = start(Agent::Codex, dir.path(), ChatMode::Ask, &fake);
    session.send("q", None).unwrap();
    let got = until_turn_end(&events);
    assert!(
        got.iter().any(|e| matches!(
            e,
            Event::Error {
                error: AgentError::NotSignedIn {
                    agent: Agent::Codex,
                    ..
                }
            }
        )),
        "{got:?}"
    );
}

/// Text a schedule or a user could hold that a command line would read as
/// syntax or flags.
const HOSTILE: &[&str] = &[
    "x & calc & y",
    "\"quoted\" | more",
    "%PATH% ^ < > !USERNAME!",
    "--dangerously-bypass-approvals-and-sandbox",
    "-m evil",
    "line one\nline two",
];

#[test]
fn schedule_and_user_text_never_reach_a_command_line() {
    for agent in Agent::ALL {
        let dir = tempfile::tempdir().unwrap();
        let scripts = match agent {
            // One process reads every message.
            Agent::ClaudeCode => {
                let mut steps = vec![];
                for _ in HOSTILE {
                    steps.extend([Step::Read, Step::Line(INIT.into()), Step::Line(RESULT.into())]);
                }
                steps.push(Step::Hang);
                vec![Script {
                    steps,
                    ..Default::default()
                }]
            }
            Agent::Codex => HOSTILE
                .iter()
                .map(|_| Script {
                    steps: vec![
                        Step::Line(r#"{"type":"thread.started","thread_id":"00000000-0000-4000-8000-0000000000aa"}"#.into()),
                        Step::Line(r#"{"type":"turn.completed","usage":{}}"#.into()),
                        Step::Exit(0),
                    ],
                    ..Default::default()
                })
                .collect(),
        };
        let fake = Fake::new(scripts);
        let mut o = options(agent, dir.path(), ChatMode::Ask, ModelChoice::AgentDefault);
        o.preamble = format!("Open project: P \"{}\".", HOSTILE.join(" "));
        let (mut session, events) = Session::start(o, fake.clone()).unwrap();
        for text in HOSTILE {
            session.send(text, Some(text)).unwrap();
            until_turn_end(&events);
        }
        for launch in fake.launches() {
            for arg in &launch.args {
                for text in HOSTILE {
                    assert!(
                        !arg.contains(text),
                        "{agent:?}: {text:?} in argument {arg:?}"
                    );
                }
                assert!(!arg.contains("calc"), "{agent:?}: {arg:?}");
            }
        }
        // Every message went in on stdin.
        let written = fake.written().join("\n");
        for text in HOSTILE {
            let json = serde_json::to_string(text).unwrap();
            let json = &json[1..json.len() - 1];
            assert!(
                written.contains(text) || written.contains(json),
                "{agent:?}: {text:?}"
            );
        }
        if agent == Agent::Codex {
            let launches = fake.launches();
            assert!(launches.iter().all(|l| l.stdin));
            let first = &launches[0].args;
            assert_eq!(&first[first.len() - 2..], ["--", "-"]);
            let next = &launches[1].args;
            assert_eq!(
                &next[next.len() - 3..],
                ["--", "00000000-0000-4000-8000-0000000000aa", "-"]
            );
        }
    }
}

#[test]
fn tool_and_proposal_ids_are_unique_in_a_session() {
    let dir = tempfile::tempdir().unwrap();
    let t = codex_turns();
    let script = |l: &[String]| {
        let mut steps = lines(l);
        steps.push(Step::Exit(0));
        Script {
            steps,
            ..Default::default()
        }
    };
    // Both turns call their first tool item_0.
    let fake = Fake::new(vec![script(&t[0]), script(&t[1]), script(&t[1])]);
    let (mut session, events) = start(Agent::Codex, dir.path(), ChatMode::Edit, &fake);
    let mut started = vec![];
    let mut proposals = vec![];
    for _ in 0..3 {
        session.send("q", None).unwrap();
        for e in until_turn_end(&events) {
            match e {
                Event::ToolStarted { id, .. } => started.push(id),
                Event::ProposedChanges { id, .. } => proposals.push(id),
                _ => {}
            }
        }
    }
    let unique: std::collections::BTreeSet<&String> = started.iter().collect();
    assert_eq!(unique.len(), started.len(), "{started:?}");
    assert_eq!(proposals.len(), 2);
    assert_ne!(proposals[0], proposals[1]);
}

#[cfg(windows)]
#[test]
fn a_cmd_install_gets_no_shell_syntax_from_its_arguments() {
    use agent_host::SystemSpawner;
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("agent.cmd");
    std::fs::write(&script, "@exit /b 0\r\n").unwrap();
    let marker = dir.path().join("injected.txt");
    let hostile = "x\" & echo hit>injected.txt & rem \"".to_string();
    let launch = Launch {
        program: script.clone(),
        args: vec!["--flag".into(), hostile],
        cwd: Some(dir.path().to_path_buf()),
        ..Default::default()
    };
    if let Ok(mut p) = SystemSpawner.spawn(&launch) {
        wait_for("the script ends", || matches!(p.try_wait(), Ok(Some(_))));
    }
    std::thread::sleep(Duration::from_millis(200));
    assert!(!marker.exists(), "an argument ran as a command");
    // An argument that cannot be passed safely is refused, not run.
    let refused = SystemSpawner.spawn(&Launch {
        program: script,
        args: vec!["a\nb".into()],
        ..Default::default()
    });
    assert!(refused.is_err());
}

#[cfg(windows)]
#[test]
fn killing_an_agent_stops_its_whole_process_tree() {
    use agent_host::SystemSpawner;
    // cmd starts ping, which holds the output pipe: only a tree kill ends it.
    let mut p = SystemSpawner
        .spawn(&Launch {
            program: "C:\\Windows\\System32\\cmd.exe".into(),
            args: vec![
                "/C".into(),
                "C:\\Windows\\System32\\PING.EXE -n 30 127.0.0.1".into(),
            ],
            ..Default::default()
        })
        .unwrap();
    let mut out = p.take_stdout().unwrap();
    let (done, ended) = mpsc::channel();
    std::thread::spawn(move || {
        let mut bytes = vec![];
        let _ = out.read_to_end(&mut bytes);
        let _ = done.send(());
    });
    std::thread::sleep(Duration::from_millis(500));
    p.kill().unwrap();
    assert!(
        ended.recv_timeout(Duration::from_secs(5)).is_ok(),
        "a grandchild still holds the output open"
    );
}
#[test]
fn ending_a_session_removes_its_files_and_the_stored_conversation() {
    for agent in Agent::ALL {
        let dir = tempfile::tempdir().unwrap();
        let script = match agent {
            Agent::ClaudeCode => claude_turn(&[INIT, RESULT]),
            Agent::Codex => Script {
                steps: vec![
                    Step::Line(r#"{"type":"thread.started","thread_id":"00000000-0000-4000-8000-0000000000dd"}"#.into()),
                    Step::Line(r#"{"type":"turn.completed","usage":{}}"#.into()),
                    Step::Exit(0),
                ],
                ..Default::default()
            },
        };
        let fake = Fake::new(vec![script]);
        let mut o = options(agent, dir.path(), ChatMode::Ask, ModelChoice::AgentDefault);
        o.snapshot = o.work_dir.join("snapshot.xer");
        let outside = dir.path().join("project.xer");
        std::fs::write(&outside, "keep").unwrap();
        let work = o.work_dir.clone();
        let (mut session, events) = Session::start(o, fake.clone()).unwrap();
        std::fs::write(work.join("snapshot.xer"), "copy").unwrap();
        session.send("q", None).unwrap();
        until_turn_end(&events);
        session.end().join().unwrap();
        assert!(!work.exists(), "{agent:?}: the work folder is gone");
        assert!(outside.exists(), "files outside it stay");
        let forget = fake.launches().last().unwrap().args.clone();
        let expected = match agent {
            Agent::ClaudeCode => vec!["purge".to_string(), "-y".into(), work.display().to_string()],
            Agent::Codex => vec![
                "delete".to_string(),
                "--force".into(),
                "00000000-0000-4000-8000-0000000000dd".into(),
            ],
        };
        assert_eq!(forget, expected);
        assert_eq!(
            agent_host::forget_arguments(
                agent,
                &work,
                Some("00000000-0000-4000-8000-0000000000dd")
            ),
            Some(expected)
        );
    }
    // A work folder the session did not make is left, and Claude Code's
    // data for it is not purged; keep_files keeps everything.
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("work")).unwrap();
    let fake = Fake::new(vec![]);
    let (session, _events) = start(Agent::ClaudeCode, dir.path(), ChatMode::Ask, &fake);
    session.end().join().unwrap();
    assert!(dir.path().join("work").exists());
    assert!(!dir.path().join("work/openxer-chat-mcp.json").exists());
    assert!(
        fake.launches().is_empty(),
        "no purge of a folder we did not make"
    );
    let dir = tempfile::tempdir().unwrap();
    let mut o = options(
        Agent::Codex,
        dir.path(),
        ChatMode::Ask,
        ModelChoice::AgentDefault,
    );
    o.keep_files = true;
    let (session, _events) = Session::start(o, fake.clone()).unwrap();
    drop(session);
    std::thread::sleep(Duration::from_millis(100));
    assert!(dir
        .path()
        .join("work/openxer-chat-instructions.md")
        .exists());
    // A thread id that is not a UUID is never passed on.
    assert_eq!(
        agent_host::forget_arguments(Agent::Codex, dir.path(), Some("--all")),
        None
    );
}

#[test]
fn detection_can_run_in_the_background() {
    let dir = tempfile::tempdir().unwrap();
    let exe = if cfg!(windows) { ".exe" } else { "" };
    std::fs::write(dir.path().join(format!("codex{exe}")), "").unwrap();
    let home = dir.path().join("codex-home");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(home.join("AGENTS.md"), "My own rules").unwrap();
    let env = Env {
        path: Some(dir.path().as_os_str().to_owned()),
        codex_home: Some(home.clone()),
        ..Default::default()
    };
    let fake = printing(vec![
        ("codex-cli 1\n", "", 0),
        ("", "Logged in using ChatGPT\n", 0),
    ]);
    let started = Instant::now();
    let receive = agent_host::detect_all_in_background(env, fake.clone());
    assert!(
        started.elapsed() < Duration::from_millis(200),
        "returns at once"
    );
    let mut all: Vec<_> = receive.iter().collect();
    all.sort_by_key(|s| s.agent == Agent::Codex);
    assert_eq!(all.len(), 2);
    assert!(!all[0].installed);
    assert_eq!(all[1].signed_in, Some(true));
    assert_eq!(all[1].user_instructions, Some(home.join("AGENTS.md")));
    assert!(agent_host::CHECK_TIMEOUT <= Duration::from_secs(10));
}

#[test]
fn only_plain_model_names_are_passed_on() {
    let dir = tempfile::tempdir().unwrap();
    for bad in ["x & calc", "-m", "a b", "\"q\"", ""] {
        let o = options(
            Agent::Codex,
            dir.path(),
            ChatMode::Ask,
            ModelChoice::Named(bad.into()),
        );
        assert!(Session::start(o, Fake::new(vec![])).is_err(), "{bad:?}");
    }
    for good in ["gpt-6-luna", "opus", "claude-sonnet-4[1m]", "org/model:1.2"] {
        assert!(agent_host::check_model_name(good).is_ok(), "{good}");
    }
}
