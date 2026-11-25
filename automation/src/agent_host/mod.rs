//! The agent host of the Chat window: OpenXER starts an AI agent the user
//! already has on this PC (Claude Code or Codex) as a child process, which
//! uses its own sign-in; OpenXER holds no key and sends nothing itself.
//!
//! The agent gets only OpenXER's chat tools (`openxer-cli mcp --toolset
//! chat`, see [`crate::chat_tools`]): no command-running or file tools,
//! the user's own agent settings, hooks and other MCP servers ignored, and
//! in Ask mode only read-only tools. An edit is only proposed: the host
//! turns the agent's `openxer_apply` call into [`Event::ProposedChanges`],
//! and the app applies it on approval ([`crate::edit::apply_changes`]).
//!
//! A [`Session`] is driven from a UI thread without blocking: calls return
//! at once and [`Event`]s arrive on a channel. Processes start through a
//! [`Spawner`], so tests run every path against a fake agent.
pub mod claude;
pub mod codex;
pub mod compat;
pub mod process;

pub use process::{capture, AgentProcess, Captured, Launch, Spawner, SystemSpawner};

use crate::chat_tools::{ChatContext, ChatMode};
use crate::connect;
use serde::Serialize;
use serde_json::Value;
use std::fmt;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The agents OpenXER can host.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Agent {
    ClaudeCode,
    Codex,
}

impl Agent {
    pub const ALL: [Agent; 2] = [Agent::ClaudeCode, Agent::Codex];

    pub fn name(self) -> &'static str {
        match self {
            Agent::ClaudeCode => "Claude Code",
            Agent::Codex => "Codex",
        }
    }
    /// The command on `PATH`.
    pub fn command(self) -> &'static str {
        match self {
            Agent::ClaudeCode => "claude",
            Agent::Codex => "codex",
        }
    }
}

/// Which model the agent runs.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelChoice {
    /// Whatever the agent uses by default.
    #[default]
    AgentDefault,
    /// A cheaper, faster model ([`claude::LOWER_COST_MODEL`],
    /// [`codex::LOWER_COST_MODEL`] at low reasoning effort).
    LowerCost,
    /// A model the agent knows by this name.
    Named(String),
}

/// The choices a model picker offers for `agent`, with their labels.
pub fn model_choices(agent: Agent) -> Vec<(ModelChoice, String)> {
    let cheaper = match agent {
        Agent::ClaudeCode => format!("Lower cost ({})", claude::LOWER_COST_MODEL),
        Agent::Codex => format!(
            "Lower cost ({}, {} reasoning)",
            codex::LOWER_COST_MODEL,
            codex::LOWER_COST_EFFORT
        ),
    };
    vec![
        (ModelChoice::AgentDefault, "Agent default".into()),
        (ModelChoice::LowerCost, cheaper),
    ]
}

/// Tokens one turn used. `input_tokens` counts every input token, the
/// cached ones included.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Usage {
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    pub output_tokens: u64,
}

/// What went wrong.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentError {
    /// The agent's program is not on this PC.
    NotInstalled { agent: Agent },
    /// The agent is installed but not signed in.
    NotSignedIn { agent: Agent, message: String },
    /// The agent's process ended in the middle of a reply.
    ProcessDied { message: String },
    /// A line of the agent's output was not what the host reads.
    BadOutput { line: String },
    /// A reply is still running; cancel it or wait for it.
    Busy,
    /// Any other failure the agent reported.
    Failed { message: String },
}

impl fmt::Display for AgentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AgentError::NotInstalled { agent } => {
                write!(f, "{} is not installed on this PC.", agent.name())
            }
            AgentError::NotSignedIn { agent, message } => {
                write!(f, "{} is not signed in. {message}", agent.name())
            }
            AgentError::ProcessDied { message } => write!(f, "{message}"),
            AgentError::BadOutput { line } => write!(f, "Unexpected agent output: {line}"),
            AgentError::Busy => write!(f, "A reply is still running."),
            AgentError::Failed { message } => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for AgentError {}

/// What a session reports, in order. Plain data: any chat provider can
/// produce these for the window.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Event {
    /// The agent started; `model` as it names it (once per session).
    Ready {
        model: String,
    },
    /// A piece of the reply as it is written.
    TextDelta {
        text: String,
    },
    /// One whole message of the reply (the deltas before it, joined).
    Message {
        text: String,
    },
    /// A tool call began. `target` is a few words on what it reads.
    ToolStarted {
        id: String,
        tool: String,
        target: String,
    },
    /// A tool call ended.
    ToolFinished {
        id: String,
        tool: String,
        ok: bool,
        error: Option<String>,
        duration_ms: u64,
    },
    /// The agent proposed edits: the `openxer-changes/1` batch and the
    /// field diff of its dry run. Nothing has been changed.
    ProposedChanges {
        id: String,
        changes: Value,
        diff: Option<Value>,
    },
    /// The reply ended (`cancelled` when the user stopped it).
    TurnFinished {
        usage: Usage,
        /// What the agent reports the turn cost, when it does.
        cost_usd: Option<f64>,
        cancelled: bool,
    },
    Error {
        error: AgentError,
    },
}

/// A tool call the parser saw start.
#[derive(Clone, Debug)]
pub(crate) struct ToolCall {
    pub tool: String,
    pub input: Value,
    pub started: Instant,
}

/// `text` cut to `max` characters.
pub(crate) fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut cut: String = text.chars().take(max).collect();
    cut.push_str("...");
    cut
}

/// A few words on what a tool call reads, for the window's tool line.
pub fn tool_target(tool: &str, input: &Value) -> String {
    let s = |k: &str| input[k].as_str().map(str::to_string);
    match tool {
        "openxer_load" => {
            let mut parts = vec![];
            if let Some(list) = input["activities"].as_array() {
                let ids: Vec<&str> = list.iter().filter_map(Value::as_str).collect();
                let shown = ids.iter().take(5).copied().collect::<Vec<_>>().join(", ");
                parts.push(if ids.len() > 5 {
                    format!("{shown} and {} more", ids.len() - 5)
                } else {
                    shown
                });
            }
            if let Some(w) = s("wbs") {
                parts.push(format!("WBS {w}"));
            }
            if input["critical"] == true {
                parts.push("critical activities".into());
            }
            if parts.is_empty() {
                parts.push(if input["limit"] == 0 {
                    "project summary".into()
                } else {
                    "activities".into()
                });
            }
            if let Some(n) = input["offset"].as_u64().filter(|n| *n > 0) {
                parts.push(format!("from row {}", n + 1));
            }
            parts.join(", ")
        }
        "openxer_records" => {
            let kind = s("kind").unwrap_or_else(|| "records".into());
            match s("activity") {
                Some(a) => format!("{kind} of {a}"),
                None => kind,
            }
        }
        "openxer_earned_value" | "openxer_usage" => s("activity")
            .or_else(|| s("resource"))
            .unwrap_or_else(|| "project".into()),
        "openxer_documents" => s("activity")
            .or_else(|| s("wbs").map(|w| format!("WBS {w}")))
            .unwrap_or_else(|| "all documents".into()),
        "openxer_playbook" => s("name")
            .map(|n| format!("{n} playbook"))
            .unwrap_or_else(|| "playbook list".into()),
        "openxer_check_schedule" => s("check").unwrap_or_else(|| "all checks".into()),
        "openxer_apply" => {
            let n = input["changes"]["operations"]
                .as_array()
                .map_or(0, Vec::len);
            format!("{n} change{}", if n == 1 { "" } else { "s" })
        }
        _ => String::new(),
    }
}

/// The events of a finished tool call: `ToolFinished`, and for a
/// successful `openxer_apply` the proposal.
pub(crate) fn tool_finished(
    id: String,
    call: Option<ToolCall>,
    failed: bool,
    text: &str,
    now: Instant,
) -> Vec<Event> {
    let envelope: Value = serde_json::from_str(text).unwrap_or(Value::Null);
    let ok = !failed && envelope["ok"] != false;
    let error = (!ok).then(|| {
        envelope["error"]["message"]
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| clip(text, 300))
    });
    let tool = call.as_ref().map_or(String::new(), |c| c.tool.clone());
    let duration_ms = call.as_ref().map_or(0, |c| {
        now.saturating_duration_since(c.started).as_millis() as u64
    });
    let mut events = vec![Event::ToolFinished {
        id: id.clone(),
        tool: tool.clone(),
        ok,
        error,
        duration_ms,
    }];
    if ok && tool == crate::chat_tools::APPLY_TOOL {
        if let Some(call) = call {
            let diff = envelope["result"]["report"]["diff"].clone();
            events.push(Event::ProposedChanges {
                id,
                changes: call.input["changes"].clone(),
                diff: (!diff.is_null()).then_some(diff),
            });
        }
    }
    events
}

/// An agent's failure message as an error: sign-in problems apart.
pub fn classify_failure(agent: Agent, message: &str) -> AgentError {
    let lower = message.to_ascii_lowercase();
    let sign_in = [
        "not logged in",
        "/login",
        "please log in",
        "please login",
        "login required",
        "not signed in",
        "sign in again",
        "401",
        "unauthorized",
        "invalid api key",
        "authentication",
    ]
    .iter()
    .any(|k| lower.contains(k));
    if sign_in {
        AgentError::NotSignedIn {
            agent,
            message: clip(message.trim(), 300),
        }
    } else {
        AgentError::Failed {
            message: clip(message.trim(), 500),
        }
    }
}

// ------------------------------------------------------------- detection

/// How long a `--version` or sign-in check may take before it is stopped.
/// [`detect`] runs two checks, so one agent takes at most twice this, and
/// [`detect_all`] (one agent after the other) four times: 40 seconds in
/// the worst case, normally about a second. A UI thread uses
/// [`detect_all_in_background`] instead.
pub const CHECK_TIMEOUT: Duration = Duration::from_secs(10);

/// Whether an agent is on this PC and ready.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AgentStatus {
    pub agent: Agent,
    pub installed: bool,
    pub executable: Option<PathBuf>,
    pub version: Option<String>,
    /// `None` when the agent could not say.
    pub signed_in: Option<bool>,
    /// The user's own instruction file the agent will read in every chat
    /// although OpenXER starts it without the user's setup: Codex reads
    /// `AGENTS.override.md` or `AGENTS.md` in its home folder, and no
    /// documented setting turns that off. The window can say so.
    pub user_instructions: Option<PathBuf>,
}

/// Codex's own instruction file in its home folder, when it is not empty
/// (the first of `AGENTS.override.md` and `AGENTS.md`, as Codex reads them).
pub fn codex_user_instructions(env: &connect::Env) -> Option<PathBuf> {
    let home = env.codex_file()?.parent()?.to_path_buf();
    ["AGENTS.override.md", "AGENTS.md"]
        .iter()
        .map(|n| home.join(n))
        .find(|p| {
            std::fs::read_to_string(p)
                .map(|t| !t.trim().is_empty())
                .unwrap_or(false)
        })
}

/// A model name the host passes on: letters, digits and `.-_:/[]`, not
/// starting with `-`.
pub fn check_model_name(name: &str) -> Result<(), AgentError> {
    let ok = !name.is_empty()
        && !name.starts_with('-')
        && name.len() <= 100
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || ".-_:/[]".contains(c));
    if ok {
        Ok(())
    } else {
        Err(AgentError::Failed {
            message: format!("{name:?} is not a model name OpenXER can pass on"),
        })
    }
}

/// The agent's program on `env`'s `PATH`.
pub fn find(agent: Agent, env: &connect::Env) -> Option<PathBuf> {
    compat::find_program(agent, env)
}

/// The first line of `--version`.
pub fn version(spawner: &dyn Spawner, executable: &std::path::Path) -> Option<String> {
    let out = capture(
        spawner,
        &Launch {
            program: executable.to_path_buf(),
            args: vec!["--version".into()],
            ..Default::default()
        },
        CHECK_TIMEOUT,
    )?;
    (out.code == 0)
        .then(|| out.stdout.lines().next().unwrap_or("").trim().to_string())
        .filter(|s| !s.is_empty())
}

/// The command that reports whether the agent is signed in.
pub fn sign_in_status_args(agent: Agent) -> Vec<String> {
    match agent {
        Agent::ClaudeCode => vec!["auth".into(), "status".into(), "--json".into()],
        Agent::Codex => vec!["login".into(), "status".into()],
    }
}

/// Read the sign-in status command's output. Claude Code prints JSON with
/// `loggedIn`; Codex prints "Logged in using ..." or "Not logged in".
pub fn parse_sign_in_status(agent: Agent, out: &Captured) -> Option<bool> {
    match agent {
        Agent::ClaudeCode => {
            let v: Value = serde_json::from_str(out.stdout.trim()).ok()?;
            v["loggedIn"].as_bool()
        }
        Agent::Codex => {
            let text = format!("{}\n{}", out.stdout, out.stderr).to_ascii_lowercase();
            if text.contains("not logged in") {
                Some(false)
            } else if text.contains("logged in") && out.code == 0 {
                Some(true)
            } else if out.code != 0 {
                Some(false)
            } else {
                None
            }
        }
    }
}

/// Whether the agent is signed in (`None` when it could not say).
pub fn signed_in(
    agent: Agent,
    spawner: &dyn Spawner,
    executable: &std::path::Path,
) -> Option<bool> {
    let out = capture(
        spawner,
        &Launch {
            program: executable.to_path_buf(),
            args: sign_in_status_args(agent),
            ..Default::default()
        },
        CHECK_TIMEOUT,
    )?;
    parse_sign_in_status(agent, &out)
}

/// Whether `agent` is installed, its version and whether it is signed in.
/// Everything comes from `env` and `spawner`: an `Env::default()` finds
/// nothing, so a test never sees the real user profile.
pub fn detect(agent: Agent, env: &connect::Env, spawner: &dyn Spawner) -> AgentStatus {
    let user_instructions = match agent {
        Agent::Codex => codex_user_instructions(env),
        Agent::ClaudeCode => None,
    };
    let Some(executable) = find(agent, env) else {
        return AgentStatus {
            agent,
            installed: false,
            executable: None,
            version: None,
            signed_in: None,
            user_instructions: None,
        };
    };
    AgentStatus {
        agent,
        installed: true,
        version: version(spawner, &executable),
        signed_in: signed_in(agent, spawner, &executable),
        executable: Some(executable),
        user_instructions,
    }
}

/// [`detect`] for every agent, one after the other (it blocks: see
/// [`CHECK_TIMEOUT`]).
pub fn detect_all(env: &connect::Env, spawner: &dyn Spawner) -> Vec<AgentStatus> {
    Agent::ALL
        .iter()
        .map(|a| detect(*a, env, spawner))
        .collect()
}

/// [`detect`] for every agent at once on background threads; each status
/// arrives on the receiver when it is known (the channel closes after the
/// last). Returns at once, so a UI thread can poll with `try_recv`.
pub fn detect_all_in_background(
    env: connect::Env,
    spawner: Arc<dyn Spawner>,
) -> Receiver<AgentStatus> {
    let (send, receive) = mpsc::channel();
    for agent in Agent::ALL {
        let (env, spawner, send) = (env.clone(), spawner.clone(), send.clone());
        std::thread::spawn(move || {
            let _ = send.send(detect(agent, &env, spawner.as_ref()));
        });
    }
    receive
}

// --------------------------------------------------------------- sign-in

/// The command that starts the agent's own sign-in (it opens the browser).
pub fn sign_in_args(agent: Agent) -> Vec<String> {
    match agent {
        Agent::ClaudeCode => vec!["auth".into(), "login".into()],
        Agent::Codex => vec!["login".into()],
    }
}

/// A running sign-in: the agent's own `login` command, which opens the
/// browser and ends when the user has signed in. OpenXER only starts it.
pub struct SignIn {
    process: Option<Box<dyn AgentProcess>>,
    output: Receiver<String>,
    lines: Vec<String>,
}

/// Where a sign-in stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SignInState {
    Running,
    Finished { ok: bool },
}

fn forward_lines(reader: Option<Box<dyn Read + Send>>, send: Sender<String>) {
    if let Some(reader) = reader {
        std::thread::spawn(move || {
            process::for_each_line(reader, |line| {
                let _ = send.send(line);
            });
        });
    }
}

impl SignIn {
    /// Start `agent`'s sign-in.
    pub fn start(
        agent: Agent,
        executable: &std::path::Path,
        spawner: &dyn Spawner,
    ) -> Result<SignIn, AgentError> {
        let mut process = spawner
            .spawn(&Launch {
                program: executable.to_path_buf(),
                args: sign_in_args(agent),
                ..Default::default()
            })
            .map_err(|e| spawn_error(agent, e))?;
        let (send, output) = mpsc::channel();
        forward_lines(process.take_stdout(), send.clone());
        forward_lines(process.take_stderr(), send);
        Ok(SignIn {
            process: Some(process),
            output,
            lines: vec![],
        })
    }

    /// What the command has printed so far (the sign-in link, when the
    /// browser did not open).
    pub fn output(&mut self) -> &[String] {
        while let Ok(line) = self.output.try_recv() {
            self.lines.push(line);
        }
        &self.lines
    }

    /// Whether it is still running; never blocks.
    pub fn poll(&mut self) -> SignInState {
        let Some(process) = self.process.as_mut() else {
            return SignInState::Finished { ok: false };
        };
        match process.try_wait() {
            Ok(Some(code)) => {
                self.process = None;
                SignInState::Finished { ok: code == 0 }
            }
            Ok(None) => SignInState::Running,
            Err(_) => SignInState::Finished { ok: false },
        }
    }

    /// Stop the sign-in.
    pub fn cancel(&mut self) {
        if let Some(mut process) = self.process.take() {
            let _ = process.kill();
        }
    }
}

impl Drop for SignIn {
    fn drop(&mut self) {
        self.cancel();
    }
}

fn spawn_error(agent: Agent, e: std::io::Error) -> AgentError {
    if e.kind() == std::io::ErrorKind::NotFound {
        AgentError::NotInstalled { agent }
    } else if e.kind() == std::io::ErrorKind::InvalidInput {
        AgentError::Failed {
            message: format!(
                "OpenXER cannot start {} safely from this install (a batch file launcher that cannot take one of the arguments). Install {} as a program, or move its work folder to a path without unusual characters.",
                agent.name(),
                agent.name()
            ),
        }
    } else {
        AgentError::Failed {
            message: format!("Cannot start {}: {e}", agent.name()),
        }
    }
}

// --------------------------------------------------------------- session

/// What a conversation runs with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionOptions {
    pub agent: Agent,
    /// The agent's program (from [`detect`]).
    pub executable: PathBuf,
    pub model: ModelChoice,
    pub mode: ChatMode,
    /// The `openxer-cli` that serves the tools.
    pub cli: PathBuf,
    /// `OPENXER_DATA_DIR` for the tool server.
    pub data_dir: PathBuf,
    /// The folder the agent runs in; the host writes its config there.
    pub work_dir: PathBuf,
    /// The project file every tool reads (a copy of the open project).
    pub snapshot: PathBuf,
    /// Lines put before the first message ([`crate::chat_tools::preamble`]).
    pub preamble: String,
    /// Also append the agent's raw output lines to this file.
    pub transcript: Option<PathBuf>,
    /// The user's standing instructions, sent after the scheduling core
    /// ([`crate::chat_prompt::read_user_instructions`]); `None` for none.
    pub user_instructions: Option<String>,
    /// Optional per-project standing rules for this schedule.
    pub project_instructions: Option<String>,
    /// Optional folder of user-authored `.md` playbooks (`chat/playbooks`).
    pub playbooks_dir: Option<PathBuf>,
    /// Optional JSONL queue file where `openxer_ui` writes live desktop UI actions.
    pub ui_actions: Option<PathBuf>,
    /// Keep the conversation's files when the session ends. By default
    /// ending removes them: the work folder (when the session made it,
    /// with the agent config and a snapshot inside it), the snapshot when
    /// it lies in that folder, and the agent's own stored copy of the
    /// conversation ([`forget_conversation`]), which holds schedule data.
    pub keep_files: bool,
}

impl SessionOptions {
    pub(crate) fn chat_context(&self) -> ChatContext {
        ChatContext {
            snapshot: Some(self.snapshot.clone()),
            mode: self.mode,
            playbooks_dir: self.playbooks_dir.clone(),
            ui_actions: self.ui_actions.clone(),
        }
    }
}

/// Where a session writes the chat's instructions for its agent.
pub fn instructions_file(options: &SessionOptions) -> PathBuf {
    options.work_dir.join("openxer-chat-instructions.md")
}

/// The whole of the agent's instructions: [`crate::chat_prompt::system_prompt_with`]
/// with the session's mode, user standing instructions and project rules.
pub fn rules(options: &SessionOptions) -> String {
    crate::chat_prompt::system_prompt_with(
        options.mode,
        options.user_instructions.as_deref(),
        options.project_instructions.as_deref(),
    )
}

#[derive(Default)]
struct Shared {
    busy: bool,
    cancelled: bool,
    closed: bool,
    ready_sent: bool,
    /// Claude's session id or Codex's thread id.
    conversation: Option<String>,
    /// Bumped for each process; a reader of an older one stays quiet.
    generation: u64,
    /// The process the running reply is on (0 while it has none yet), so
    /// a process that ends after its own reply never ends the next one.
    turn_generation: u64,
}

type Slot = Arc<Mutex<Option<Box<dyn AgentProcess>>>>;

/// One conversation with an agent. Calls never block on the agent;
/// replies arrive as [`Event`]s on the receiver [`Session::start`] gave.
/// Dropping the session stops its process.
pub struct Session {
    options: SessionOptions,
    spawner: Arc<dyn Spawner>,
    events: Sender<Event>,
    shared: Arc<Mutex<Shared>>,
    process: Slot,
    stdin: Option<Box<dyn Write + Send>>,
    first: bool,
    /// Whether `start` made the work folder (only then is it removed and
    /// Claude Code's data for it purged).
    created_work_dir: bool,
    cleaned: bool,
}

/// The agent's own command that deletes its stored copy of a conversation:
/// Claude Code keeps it under `~/.claude/projects/<work folder>/` and
/// `claude purge -y <work folder>` deletes Claude Code's data for that
/// folder (so the folder must be the session's own); Codex keeps it under
/// `~/.codex/sessions/` and `codex delete --force <thread id>` deletes it.
/// `None` when there is nothing to delete.
pub fn forget_arguments(
    agent: Agent,
    work_dir: &std::path::Path,
    conversation: Option<&str>,
) -> Option<Vec<String>> {
    match agent {
        Agent::ClaudeCode => Some(vec![
            "purge".into(),
            "-y".into(),
            work_dir.display().to_string(),
        ]),
        Agent::Codex => conversation
            .filter(|id| codex::valid_thread_id(id))
            .map(|id| vec!["delete".into(), "--force".into(), id.into()]),
    }
}

/// Delete the agent's stored copy of a conversation ([`forget_arguments`]);
/// blocks for at most [`CHECK_TIMEOUT`]. Whether the command succeeded.
pub fn forget_conversation(
    agent: Agent,
    executable: &std::path::Path,
    spawner: &dyn Spawner,
    work_dir: &std::path::Path,
    conversation: Option<&str>,
) -> bool {
    let Some(args) = forget_arguments(agent, work_dir, conversation) else {
        return true;
    };
    capture(
        spawner,
        &Launch {
            program: executable.to_path_buf(),
            args,
            ..Default::default()
        },
        CHECK_TIMEOUT,
    )
    .is_some_and(|out| out.code == 0)
}

impl Session {
    /// Prepare a conversation: checks the agent's program and writes the
    /// config files the agent reads into `work_dir`. The agent starts with
    /// the first message.
    pub fn start(
        options: SessionOptions,
        spawner: Arc<dyn Spawner>,
    ) -> Result<(Session, Receiver<Event>), AgentError> {
        if options.executable.as_os_str().is_empty() {
            return Err(AgentError::NotInstalled {
                agent: options.agent,
            });
        }
        if let ModelChoice::Named(name) = &options.model {
            check_model_name(name)?;
        }
        let created_work_dir = !options.work_dir.exists();
        let write = |path: PathBuf, text: String| {
            std::fs::create_dir_all(&options.work_dir)
                .and_then(|()| std::fs::write(&path, text))
                .map_err(|e| AgentError::Failed {
                    message: format!("Cannot write {}: {e}", path.display()),
                })
        };
        write(instructions_file(&options), rules(&options))?;
        if options.agent == Agent::ClaudeCode {
            write(
                claude::config_path(&options),
                claude::mcp_config(&options).to_string(),
            )?;
        }
        let (events, receive) = mpsc::channel();
        Ok((
            Session {
                options,
                spawner,
                events,
                shared: Arc::default(),
                process: Arc::default(),
                stdin: None,
                first: true,
                created_work_dir,
                cleaned: false,
            },
            receive,
        ))
    }

    pub fn options(&self) -> &SessionOptions {
        &self.options
    }

    /// Whether a reply is running.
    pub fn is_busy(&self) -> bool {
        self.shared.lock().map(|s| s.busy).unwrap_or(false)
    }

    /// The conversation id the agent gave (to resume it).
    pub fn conversation_id(&self) -> Option<String> {
        self.shared.lock().ok().and_then(|s| s.conversation.clone())
    }

    /// Send a user message. `context` (for example the activities now
    /// selected) goes before it; the first message also carries the
    /// preamble. Returns at once; the reply arrives as events.
    pub fn send(&mut self, text: &str, context: Option<&str>) -> Result<(), AgentError> {
        {
            let mut shared = self.shared.lock().expect("session state");
            if shared.busy {
                return Err(AgentError::Busy);
            }
            shared.busy = true;
            shared.cancelled = false;
            shared.turn_generation = 0;
        }
        let mut message = String::new();
        if self.first && !self.options.preamble.trim().is_empty() {
            message.push_str(self.options.preamble.trim());
            message.push_str("\n\n");
        }
        if let Some(context) = context.filter(|c| !c.trim().is_empty()) {
            message.push_str("Context: ");
            message.push_str(context.trim());
            message.push_str("\n\n");
        }
        message.push_str(text);
        let sent = match self.options.agent {
            Agent::ClaudeCode => self.send_claude(&message),
            Agent::Codex => self.start_codex_turn(&message),
        };
        match sent {
            Ok(()) => {
                self.first = false;
                Ok(())
            }
            Err(e) => {
                if let Ok(mut shared) = self.shared.lock() {
                    shared.busy = false;
                }
                Err(e)
            }
        }
    }

    fn process_running(&self) -> bool {
        self.process
            .lock()
            .ok()
            .and_then(|mut p| p.as_mut().map(|p| matches!(p.try_wait(), Ok(None))))
            .unwrap_or(false)
    }

    fn send_claude(&mut self, message: &str) -> Result<(), AgentError> {
        let line = claude::user_line(message);
        let running = self.process_running();
        if let Some(stdin) = self.stdin.as_mut().filter(|_| running) {
            // The reply runs on the live process.
            if let Ok(mut shared) = self.shared.lock() {
                shared.turn_generation = shared.generation;
            }
            if writeln!(stdin, "{line}")
                .and_then(|()| stdin.flush())
                .is_ok()
            {
                return Ok(());
            }
        }
        // No live process: start one, resuming the conversation when the
        // agent has given it an id.
        let resume = self.conversation_id();
        let args = claude::arguments(
            &self.options,
            &claude::config_path(&self.options),
            &instructions_file(&self.options),
            resume.as_deref(),
        )?;
        let mut stdin = self.launch(args, true, Parser::Claude(claude::ClaudeParser::new()))?;
        let written = stdin
            .as_mut()
            .map(|s| writeln!(s, "{line}").and_then(|()| s.flush()));
        if !matches!(written, Some(Ok(()))) {
            self.stop_process();
            return Err(AgentError::ProcessDied {
                message: "Claude Code stopped before it read the message.".into(),
            });
        }
        self.stdin = stdin.take();
        Ok(())
    }

    fn start_codex_turn(&mut self, message: &str) -> Result<(), AgentError> {
        let resume = self.conversation_id();
        let args = codex::arguments(&self.options, resume.as_deref())?;
        let model = match &self.options.model {
            ModelChoice::AgentDefault => "Codex default".to_string(),
            ModelChoice::LowerCost => codex::LOWER_COST_MODEL.to_string(),
            ModelChoice::Named(n) => n.clone(),
        };
        // The prompt goes in on stdin, which is then closed: it is never
        // on the command line.
        let stdin = self.launch(args, true, Parser::Codex(codex::CodexParser::new(model)))?;
        let written = stdin.map(|mut s| s.write_all(message.as_bytes()).and_then(|()| s.flush()));
        if !matches!(written, Some(Ok(()))) {
            self.stop_process();
            return Err(AgentError::ProcessDied {
                message: "Codex stopped before it read the message.".into(),
            });
        }
        Ok(())
    }

    /// Start the agent's process with a reader thread for its output.
    fn launch(
        &mut self,
        args: Vec<String>,
        stdin: bool,
        parser: Parser,
    ) -> Result<Option<Box<dyn Write + Send>>, AgentError> {
        // A new generation first, so the old process's reader stays quiet
        // when it sees its output end.
        let generation = {
            let mut shared = self.shared.lock().expect("session state");
            shared.generation += 1;
            shared.turn_generation = shared.generation;
            shared.generation
        };
        self.stop_process();
        let mut env = vec![(
            "OPENXER_DATA_DIR".to_string(),
            self.options.data_dir.display().to_string(),
        )];
        if self.options.agent == Agent::ClaudeCode {
            env.extend(
                claude::ENVIRONMENT
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string())),
            );
        }
        let launch = Launch {
            program: self.options.executable.clone(),
            args,
            cwd: Some(self.options.work_dir.clone()),
            env,
            stdin,
        };
        let mut process = self
            .spawner
            .spawn(&launch)
            .map_err(|e| spawn_error(self.options.agent, e))?;
        let input = process.take_stdin();
        let stdout = process.take_stdout();
        let stderr = process.take_stderr();
        *self.process.lock().expect("process slot") = Some(process);
        let tail = Arc::new(Mutex::new(Vec::<String>::new()));
        let stderr_done = stderr.map(|stderr| {
            let tail = tail.clone();
            let (done, finished) = mpsc::channel::<()>();
            std::thread::spawn(move || {
                // Read to the end whatever it holds, so a talkative child
                // never blocks on a full pipe.
                process::for_each_line(stderr, |line| {
                    if let Ok(mut t) = tail.lock() {
                        t.push(line);
                        if t.len() > 20 {
                            t.remove(0);
                        }
                    }
                });
                let _ = done.send(());
            });
            finished
        });
        let reader = Reader {
            agent: self.options.agent,
            events: self.events.clone(),
            shared: self.shared.clone(),
            process: self.process.clone(),
            generation,
            parser,
            transcript: self.options.transcript.clone(),
            tail,
            stderr_done,
        };
        match stdout {
            Some(stdout) => {
                std::thread::spawn(move || reader.run(stdout));
            }
            None => {
                self.stop_process();
                return Err(AgentError::Failed {
                    message: "The agent's output could not be read.".into(),
                });
            }
        }
        Ok(input)
    }

    fn stop_process(&mut self) {
        self.stdin = None;
        if let Ok(mut slot) = self.process.lock() {
            if let Some(mut process) = slot.take() {
                let _ = process.kill();
            }
        }
    }

    /// Stop the running reply: the agent's process (and every process it
    /// started) is stopped and the cancelled `TurnFinished` is sent at once,
    /// whatever its output does. The conversation goes on with the next
    /// message (resumed by its id).
    pub fn cancel(&mut self) {
        {
            let Ok(mut shared) = self.shared.lock() else {
                return;
            };
            if !shared.busy {
                return;
            }
            // A new generation: nothing more from the stopped process.
            shared.generation += 1;
            shared.busy = false;
            shared.cancelled = true;
            let _ = self.events.send(Event::TurnFinished {
                usage: Usage::default(),
                cost_usd: None,
                cancelled: true,
            });
        }
        self.stop_process();
    }

    /// End the conversation: stop the agent and, unless
    /// [`SessionOptions::keep_files`], remove the conversation's files and
    /// the agent's stored copy. The returned handle ends when that is done
    /// (it never needs to be waited for).
    pub fn end(mut self) -> std::thread::JoinHandle<()> {
        self.finish()
    }

    fn finish(&mut self) -> std::thread::JoinHandle<()> {
        if let Ok(mut shared) = self.shared.lock() {
            shared.closed = true;
        }
        self.stop_process();
        if self.cleaned || self.options.keep_files {
            self.cleaned = true;
            return std::thread::spawn(|| {});
        }
        self.cleaned = true;
        let options = self.options.clone();
        let spawner = self.spawner.clone();
        let conversation = self.conversation_id();
        let created = self.created_work_dir;
        // The files go now (quick), so a later session in the same folder
        // never loses its own.
        let _ = std::fs::remove_file(claude::config_path(&options));
        let _ = std::fs::remove_file(instructions_file(&options));
        if options.snapshot.starts_with(&options.work_dir) {
            let _ = std::fs::remove_file(&options.snapshot);
        }
        if created {
            let _ = std::fs::remove_dir_all(&options.work_dir);
        }
        // The agent's own delete runs a process: never on the caller's
        // thread. Claude Code's purge is by folder: only for a folder this
        // session made, so no other data of the user's is touched.
        std::thread::spawn(move || {
            if options.agent == Agent::Codex || created {
                forget_conversation(
                    options.agent,
                    &options.executable,
                    spawner.as_ref(),
                    &options.work_dir,
                    conversation.as_deref(),
                );
            }
        })
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

enum Parser {
    Claude(claude::ClaudeParser),
    Codex(codex::CodexParser),
}

impl Parser {
    fn parse(&mut self, line: &str, now: Instant) -> Vec<Event> {
        match self {
            Parser::Claude(p) => p.parse_line(line, now),
            Parser::Codex(p) => p.parse_line(line, now),
        }
    }
    fn conversation(&self) -> Option<&String> {
        match self {
            Parser::Claude(p) => p.session_id.as_ref(),
            Parser::Codex(p) => p.thread_id.as_ref(),
        }
    }
}

struct Reader {
    agent: Agent,
    events: Sender<Event>,
    shared: Arc<Mutex<Shared>>,
    process: Slot,
    generation: u64,
    parser: Parser,
    transcript: Option<PathBuf>,
    tail: Arc<Mutex<Vec<String>>>,
    /// Signals that the error output has been read to its end.
    stderr_done: Option<Receiver<()>>,
}

impl Reader {
    fn current(&self, shared: &Shared) -> bool {
        shared.generation == self.generation && !shared.closed
    }

    /// Whether a reply is running on this reader's process.
    fn owns_turn(&self, shared: &Shared) -> bool {
        self.current(shared) && shared.busy && shared.turn_generation == self.generation
    }

    fn run(mut self, stdout: Box<dyn Read + Send>) {
        let mut log = self.transcript.as_ref().and_then(|p| {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(p)
                .ok()
        });
        let mut stale = false;
        // Lines are bytes decoded leniently: an invalid UTF-8 byte never
        // stops the reading.
        process::for_each_line(stdout, |line| {
            if stale {
                return;
            }
            if let Some(log) = log.as_mut() {
                let _ = writeln!(log, "{line}");
            }
            let events = self.parser.parse(&line, Instant::now());
            let mut shared = self.shared.lock().expect("session state");
            if !self.current(&shared) {
                // Stopped or replaced: keep draining, say nothing.
                stale = true;
                return;
            }
            if let Some(id) = self.parser.conversation() {
                shared.conversation = Some(id.clone());
            }
            for mut event in events {
                match &mut event {
                    Event::Ready { .. } if shared.ready_sent => continue,
                    Event::Ready { .. } => shared.ready_sent = true,
                    Event::TurnFinished { .. } => shared.busy = false,
                    // Agents number tool calls per process (Codex starts
                    // at item_0 every turn): ids carry the process, so they
                    // are unique in the session.
                    Event::ToolStarted { id, .. }
                    | Event::ToolFinished { id, .. }
                    | Event::ProposedChanges { id, .. } => {
                        *id = format!("{}-{id}", self.generation);
                    }
                    _ => {}
                }
                let _ = self.events.send(event);
            }
        });
        if stale {
            return;
        }
        // The output ended: the process stopped (or is stopping). Nothing
        // to say unless a reply of this process was still running.
        {
            let shared = self.shared.lock().expect("session state");
            if !self.owns_turn(&shared) {
                return;
            }
        }
        let code = self.exit_code();
        if let Some(done) = &self.stderr_done {
            let _ = done.recv_timeout(Duration::from_secs(1));
        }
        let mut shared = self.shared.lock().expect("session state");
        if !self.owns_turn(&shared) {
            return;
        }
        shared.busy = false;
        if shared.cancelled {
            let _ = self.events.send(Event::TurnFinished {
                usage: Usage::default(),
                cost_usd: None,
                cancelled: true,
            });
            return;
        }
        let tail = self.tail.lock().map(|t| t.join("\n")).unwrap_or_default();
        let error = match classify_failure(self.agent, &tail) {
            e @ AgentError::NotSignedIn { .. } => e,
            _ => AgentError::ProcessDied {
                message: format!(
                    "{} stopped before the reply ended{}.{}",
                    self.agent.name(),
                    code.map(|c| format!(" (exit code {c})"))
                        .unwrap_or_default(),
                    if tail.trim().is_empty() {
                        String::new()
                    } else {
                        format!(" {}", clip(tail.trim(), 500))
                    }
                ),
            },
        };
        let _ = self.events.send(Event::Error { error });
        let _ = self.events.send(Event::TurnFinished {
            usage: Usage::default(),
            cost_usd: None,
            cancelled: false,
        });
    }

    /// The process's exit code, waiting a little for it to end.
    fn exit_code(&self) -> Option<i32> {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            {
                let mut slot = self.process.lock().ok()?;
                match slot.as_mut().map(|p| p.try_wait()) {
                    Some(Ok(Some(code))) => return Some(code),
                    Some(Ok(None)) => {}
                    _ => return None,
                }
            }
            if Instant::now() >= deadline {
                return None;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
