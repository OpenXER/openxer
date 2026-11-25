//! The Chat window's real backend, over three routes that run the same
//! chat tools on the same snapshot:
//!
//! - Claude Code and Codex: the user's own installed program, hosted as a
//!   child process by `openxer_automation::agent_host` (its own sign-in;
//!   OpenXER sends nothing itself). Detection runs on background threads.
//! - ChatGPT: `openxer_chat` signs in to the user's ChatGPT plan and talks
//!   to the API directly; its tools ([`DirectTools`]) call
//!   `chat_tools::call` in-process.
//!
//! A conversation has a fresh folder in the temp directory ([`work`]) with
//! the project snapshot the tools read. The snapshot is written before the
//! first message and again only when the project changed; the preamble
//! (`chat_tools::preamble`) goes once; the selection goes along only when
//! it changed; what the user did with a proposal goes with the next
//! message. Everything returns at once: replies, sign-ins and checks run
//! on other threads and are picked up by [`LiveBackend::poll`].
pub(crate) mod endpoint;
pub(crate) mod rows;
pub(crate) mod work;

#[cfg(test)]
pub(crate) mod fakes;
#[cfg(test)]
mod tests;

use super::backend::{
    AgentEvent, AgentInfo, AgentState, Applied, ChangeRow, ChatBackend, EndpointCheck, ErrorKind,
    Mode, ModelChoice, Selection, Turn, Usage, OWN_SIGN_IN,
};
use super::checks::{self, Checks};
use openxer_automation::agent_host::{
    self as host, compat, Agent, AgentError, AgentStatus, Event, Session, SessionOptions, SignIn,
    SignInState, Spawner,
};
use openxer_automation::chat_prompt;
use openxer_automation::chat_tools::{self, ChatMode};
use openxer_automation::connect::Env;
use openxer_chat as oc;
use openxer_xer::ProjectData;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::Arc;
use std::time::Instant;

/// Opens a link or a web page in the user's browser: the app's one opener
/// ([`crate::util::open_in_default_app`]), a recording fake in tests.
pub(crate) type Opener = Arc<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

/// The app's opener.
pub(crate) fn system_opener() -> Opener {
    Arc::new(|url: &str| crate::util::open_in_default_app(std::path::Path::new(url)))
}

/// The agents' ids (kept in the user's preferences).
pub(crate) const CHATGPT: &str = "chatgpt";
pub(crate) const CLAUDE_CODE: &str = "claude-code";
pub(crate) const CODEX: &str = "codex";

/// The vendors' own install pages.
pub(crate) const CLAUDE_CODE_URL: &str = "https://code.claude.com/docs/en/setup";
pub(crate) const CODEX_URL: &str = "https://learn.chatgpt.com/docs/codex/cli";

/// The status line of a reply the agent wrote without reaching OpenXER's
/// tools: never shown as if it read the schedule.
pub(crate) const TOOLS_UNREACHABLE: &str =
    "The assistant could not reach OpenXER's tools, so its answer is not shown. Try again.";

/// Whether an agent's error says its OpenXER tools did not start: Claude
/// Code's init line (the host's "The OpenXER tools did not start"), or a
/// Codex error naming the server.
fn tools_down(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("openxer tools did not start")
        || (lower.contains("mcp") && lower.contains("openxer"))
}

/// What the setup list says each route needs.
pub(crate) const CHATGPT_SETUP: &str =
    "Use your ChatGPT plan. Nothing to install. Use counts against your ChatGPT plan.";
pub(crate) const CLAUDE_CODE_SETUP: &str = "Needs the Claude Code app from Anthropic, with a Claude subscription or API key. Use counts against your Claude account.";
pub(crate) const CODEX_SETUP: &str = "Needs the Codex app from OpenAI, with a ChatGPT plan or API key. Use counts against your OpenAI account.";

/// The privacy line of the ChatGPT route.
pub(crate) const CHATGPT_PRIVACY: &str =
    "OpenXER sends your questions and the schedule data its tools read to OpenAI, under your ChatGPT account.";

/// What the backend runs with. The app fills it from the process
/// environment ([`Config::system`]); tests from fakes.
pub(crate) struct Config {
    /// Where the agents and `openxer-cli` are looked for.
    pub env: Env,
    pub spawner: Arc<dyn Spawner>,
    pub auth: Arc<oc::Auth>,
    pub api: oc::ApiConfig,
    pub opener: Opener,
    /// Where the conversations' work folders go.
    pub temp: PathBuf,
    /// The hosts Check Setup tries to reach.
    pub hosts: Vec<oc::http::Url>,
    /// Where API keys of the endpoint route are kept (Credential Manager
    /// in the app, in memory in a test build).
    pub keys: Arc<dyn oc::CredentialStore>,
    /// How long a starting agent may stay silent.
    pub ready_timeout: std::time::Duration,
    /// The user's own chat instructions (`chat/AGENTS.md` in the data
    /// folder), read at the start of each conversation.
    pub instructions: PathBuf,
}

impl Config {
    /// The real environment. A test build sees an empty profile, an
    /// in-memory sign-in store and endpoints that reach nothing.
    pub(crate) fn system(env: Env, opener: Opener) -> Config {
        if cfg!(test) {
            let unreachable = |path: &str| {
                oc::http::Url::parse(&format!("http://127.0.0.1:9{path}")).expect("test URL")
            };
            let auth = oc::AuthConfig {
                authorize_url: unreachable("/authorize"),
                token_url: unreachable("/token"),
                revoke_url: unreachable("/revoke"),
                jwks_url: unreachable("/jwks"),
                ..oc::AuthConfig::openai()
            };
            return Config {
                env: Env {
                    cli: env.cli,
                    ..Env::default()
                },
                spawner: Arc::new(host::SystemSpawner),
                auth: Arc::new(oc::Auth::new(auth, Arc::new(oc::MemoryStore::new()))),
                api: oc::ApiConfig {
                    base_url: unreachable("/v1"),
                    ..oc::ApiConfig::openai()
                },
                opener,
                temp: std::env::temp_dir(),
                hosts: vec![unreachable("/")],
                keys: Arc::new(oc::MemoryStore::new()),
                ready_timeout: compat::READY_TIMEOUT,
                instructions: test_instructions_path(),
            };
        }
        let api = oc::ApiConfig::openai();
        let auth = oc::AuthConfig::openai();
        let mut hosts = vec![api.base_url.clone(), auth.token_url.clone()];
        for other in ["https://api.anthropic.com/", "https://chatgpt.com/"] {
            hosts.extend(oc::http::Url::parse(other));
        }
        Config {
            env,
            spawner: Arc::new(host::SystemSpawner),
            auth: Arc::new(oc::Auth::new(
                auth,
                Arc::new(oc::WindowsCredentialStore::default()),
            )),
            api,
            opener,
            temp: std::env::temp_dir(),
            hosts,
            keys: Arc::new(oc::WindowsCredentialStore::default()),
            ready_timeout: compat::READY_TIMEOUT,
            instructions: chat_prompt::instructions_path(&chat_prompt::data_folder()),
        }
    }
}

/// Claude Code or Codex on this PC.
struct Local {
    agent: Agent,
    status: Option<AgentStatus>,
    detecting: Option<Receiver<AgentStatus>>,
    sign_in: Option<SignIn>,
    /// Why the last sign-in did not finish.
    note: Option<String>,
}

/// The ChatGPT route's sign-in and models.
struct Plan {
    client: Arc<oc::ChatClient>,
    signing: Option<oc::SignInHandle>,
    models: Vec<oc::ModelInfo>,
    models_rx: Option<Receiver<Result<Vec<oc::ModelInfo>, oc::ChatError>>>,
    /// The model list was asked for under this sign-in.
    models_asked: bool,
    /// The sign-in, as last read on a worker thread.
    status: Option<oc::SignInStatus>,
    status_rx: Option<Receiver<oc::SignInStatus>>,
    /// A sign-out running on a worker thread.
    signing_out: Option<Receiver<()>>,
    /// Not before this is a failed model list asked for again.
    models_retry: Option<Instant>,
    note: Option<String>,
}

/// A proposal waiting for Apply or Discard.
struct Proposal {
    changes: Value,
    rows: Vec<ChangeRow>,
    /// The project state the preview ran on.
    revision: Option<u64>,
}

#[allow(clippy::large_enum_variant)]
enum Route {
    /// Nothing sent yet.
    Idle,
    Hosted {
        session: Session,
        events: Receiver<Event>,
        agent: Agent,
    },
    /// A route that talks to a model API itself ([`Direct`]).
    Direct {
        conversation: Option<oc::Conversation>,
        turn: Option<oc::TurnHandle>,
        /// A turn being started on a worker thread (the snapshot written,
        /// a stopped turn let go); on failure the conversation comes back.
        starting: Option<Receiver<DirectStart>>,
        /// The context the first message fixed (resent as the stable
        /// instructions prefix, so the prompt cache holds it).
        preamble: String,
    },
}

/// One conversation.
struct Conv {
    agent: String,
    dir: PathBuf,
    route: Route,
    /// The project state the snapshot holds.
    snapshot_rev: Option<u64>,
    /// The selection the assistant was last told about.
    told_selection: Option<Selection>,
    /// Lines for the next message's context.
    notes: Vec<String>,
    sent: usize,
    /// When the first message went to a starting agent, until it answers.
    waiting_since: Option<Instant>,
    ready: bool,
    /// The user's own instructions, read when the conversation starts.
    user: Option<chat_prompt::UserInstructions>,
    /// An OpenXER tool call succeeded in the agent's current process.
    tools_ok: bool,
    /// The agent's current process said its OpenXER tools did not start.
    tools_down: bool,
    /// A message being started on a worker thread (Claude Code, Codex).
    hosted_start: Option<Receiver<Prepared>>,
    /// The direct turn being started is to write the snapshot.
    snapshot_pending: bool,
    /// The user stopped while a message was being started.
    stop_pending: bool,
    /// The selection the message being started tells.
    pending_selection: Option<Selection>,
    /// Bytes already read from `dir/ui-actions.jsonl`.
    ui_actions_offset: usize,
}

/// A direct turn started on a worker thread, or why not (with the
/// conversation back).
type DirectStart = Result<oc::TurnHandle, (String, oc::Conversation)>;

/// Why starting a message on a worker thread failed.
enum PrepFail {
    Host(AgentError),
    Snapshot(String),
}

/// What the worker that starts a message hands back: the session (to put
/// back in place), what happened, whether its process is new, and whether
/// the snapshot it was to write was written.
struct Prepared {
    session: Option<(Session, Receiver<Event>)>,
    agent: Agent,
    result: Result<(), PrepFail>,
    new_process: bool,
    due: bool,
    wrote: bool,
}

impl Conv {
    fn new(agent: &str, temp: &std::path::Path, instructions: &std::path::Path) -> Conv {
        Conv {
            user: chat_prompt::read_user_instructions(instructions),
            tools_ok: false,
            tools_down: false,
            hosted_start: None,
            snapshot_pending: false,
            stop_pending: false,
            pending_selection: None,
            ui_actions_offset: 0,
            agent: agent.to_string(),
            dir: work::fresh_path(temp),
            route: Route::Idle,
            snapshot_rev: None,
            told_selection: None,
            notes: vec![],
            sent: 0,
            waiting_since: None,
            ready: false,
        }
    }
}

pub(crate) struct LiveBackend {
    config: Config,
    locals: Vec<Local>,
    plan: Plan,
    conv: Option<Conv>,
    proposals: BTreeMap<u64, Proposal>,
    next_proposal: u64,
    out: Vec<AgentEvent>,
    checks: Option<Checks>,
    /// The API key or local model connection.
    endpoint: endpoint::Endpoint,
    /// Snapshots written so far (the token rules' tests count them).
    pub(crate) snapshot_writes: usize,
}

fn spawn_detect(agent: Agent, env: &Env, spawner: &Arc<dyn Spawner>) -> Receiver<AgentStatus> {
    let (send, receive) = mpsc::channel();
    let (env, spawner) = (env.clone(), spawner.clone());
    std::thread::spawn(move || {
        let _ = send.send(host::detect(agent, &env, spawner.as_ref()));
    });
    receive
}

fn host_model(m: &ModelChoice) -> host::ModelChoice {
    match m {
        ModelChoice::AgentDefault => host::ModelChoice::AgentDefault,
        ModelChoice::LowerCost => host::ModelChoice::LowerCost,
        ModelChoice::Named(n) => host::ModelChoice::Named(n.clone()),
    }
}

fn chat_mode(m: Mode) -> ChatMode {
    match m {
        Mode::Ask => ChatMode::Ask,
        Mode::Edit => ChatMode::Edit,
    }
}

fn selected(s: &Selection) -> Vec<String> {
    match s {
        Selection::None => vec![],
        Selection::One(id) => vec![id.clone()],
        Selection::Many(ids) => ids.clone(),
    }
}

/// The selection as one context line.
fn selection_line(s: &Selection) -> String {
    let ids = selected(s);
    match ids.len() {
        0 => "No activity is selected now.".into(),
        n if n <= 20 => format!("Selected now: {}.", ids.join(", ")),
        n => format!(
            "Selected now: {} and {} more.",
            ids[..20].join(", "),
            n - 20
        ),
    }
}

fn error(kind: ErrorKind, message: impl Into<String>) -> AgentEvent {
    AgentEvent::Error {
        kind,
        message: message.into(),
    }
}

fn build_preamble(
    project: &ProjectData,
    ctx: &super::backend::ChatContext,
    recap: Option<&str>,
) -> String {
    let mut parts = vec![chat_tools::preamble(project, &selected(&ctx.selection))];
    let mut ui_bits = vec![];
    if let Some(b) = ctx.baseline.as_deref().filter(|s| !s.is_empty()) {
        ui_bits.push(format!("Assigned baseline: {b}"));
    }
    if let Some(f) = ctx.filter.as_deref().filter(|s| !s.is_empty()) {
        ui_bits.push(format!("Active filter: {f}"));
    }
    if let Some(t) = ctx.detail_tab.as_deref().filter(|s| !s.is_empty()) {
        ui_bits.push(format!("Open details tab: {t}"));
    }
    if !ui_bits.is_empty() {
        parts.push(format!("OpenXER view state: {}.", ui_bits.join("; ")));
    }
    if let Some(r) = recap.map(str::trim).filter(|s| !s.is_empty()) {
        parts.push(r.to_string());
    }
    parts.join("\n")
}

/// Name of the JSONL file inside a conversation's temp folder where
/// `openxer_ui` queues live desktop actions.
const UI_ACTIONS_FILE: &str = "ui-actions.jsonl";

/// The custom playbooks folder beside `instructions` (`chat/playbooks`),
/// returned only when it exists on disk so CLI args stay lean otherwise.
fn playbooks_dir_for(instructions: &std::path::Path) -> Option<PathBuf> {
    let dir = instructions.parent()?.join("playbooks");
    dir.is_dir().then_some(dir)
}

/// A proposal's changes and rows from what a tool reported.
fn proposal_rows(changes: &Value, diff: Option<&Value>) -> Vec<ChangeRow> {
    let rows = diff.map(rows::rows).unwrap_or_default();
    if !rows.is_empty() {
        return rows;
    }
    let n = changes["operations"].as_array().map_or(0, Vec::len);
    vec![ChangeRow {
        activity: "Project".into(),
        field: "Changes".into(),
        old: String::new(),
        new: format!("{n} operation{}", if n == 1 { "" } else { "s" }),
    }]
}

impl LiveBackend {
    pub(crate) fn new(config: Config) -> LiveBackend {
        let locals = [Agent::ClaudeCode, Agent::Codex]
            .into_iter()
            .map(|agent| Local {
                agent,
                status: None,
                detecting: Some(spawn_detect(agent, &config.env, &config.spawner)),
                sign_in: None,
                note: None,
            })
            .collect();
        let client = Arc::new(oc::ChatClient::new(config.auth.clone(), config.api.clone()));
        let mut backend = LiveBackend {
            config,
            locals,
            plan: Plan {
                client,
                signing: None,
                models: vec![],
                models_rx: None,
                models_asked: false,
                models_retry: None,
                status: None,
                status_rx: None,
                signing_out: None,
                note: None,
            },
            conv: None,
            proposals: BTreeMap::new(),
            next_proposal: 1,
            out: vec![],
            checks: None,
            endpoint: endpoint::Endpoint::default(),
            snapshot_writes: 0,
        };
        backend.refresh_plan();
        backend
    }

    fn local(&self, agent: Agent) -> &Local {
        self.locals
            .iter()
            .find(|l| l.agent == agent)
            .expect("a local agent")
    }

    fn local_mut(&mut self, agent: Agent) -> &mut Local {
        self.locals
            .iter_mut()
            .find(|l| l.agent == agent)
            .expect("a local agent")
    }

    fn agent_of(id: &str) -> Option<Agent> {
        match id {
            CLAUDE_CODE => Some(Agent::ClaudeCode),
            CODEX => Some(Agent::Codex),
            _ => None,
        }
    }

    fn id_of(agent: Agent) -> &'static str {
        match agent {
            Agent::ClaudeCode => CLAUDE_CODE,
            Agent::Codex => CODEX,
        }
    }

    /// The sign-in the ChatGPT route has now, as last read on a worker
    /// thread (frames never read the credential store).
    fn plan_status(&self) -> oc::SignInStatus {
        self.plan
            .status
            .clone()
            .unwrap_or(oc::SignInStatus::SignedOut)
    }

    /// Reads the ChatGPT sign-in on a worker thread.
    fn refresh_plan(&mut self) {
        let (send, receive) = mpsc::channel();
        let auth = self.config.auth.clone();
        std::thread::spawn(move || {
            let _ = send.send(auth.status());
        });
        self.plan.status_rx = Some(receive);
    }

    fn plan_ready(&self) -> bool {
        matches!(
            self.plan_status(),
            oc::SignInStatus::SignedIn {
                plan_usage: true,
                ..
            }
        )
    }

    fn plan_info(&self) -> AgentInfo {
        let state = if self.plan.signing.is_some() {
            AgentState::SigningIn
        } else {
            match self.plan_status() {
                oc::SignInStatus::SignedIn {
                    account,
                    plan_usage: true,
                } => AgentState::SignedIn {
                    account: account.label(),
                },
                _ => AgentState::SignInNeeded,
            }
        };
        let notice = self.plan.note.clone().or_else(|| match self.plan_status() {
            oc::SignInStatus::SignedIn {
                plan_usage: false, ..
            } => Some(oc::ChatError::PlanUsageNotEnabled.to_string()),
            oc::SignInStatus::Expired { .. } => {
                Some("The sign-in ran out after 30 days. Sign in again.".into())
            }
            _ => None,
        });
        AgentInfo {
            id: CHATGPT.into(),
            name: "ChatGPT".into(),
            can_sign_out: matches!(state, AgentState::SignedIn { .. }),
            state,
            privacy: CHATGPT_PRIVACY.into(),
            consent: Some(oc::CONSENT_TEXT.into()),
            models: self
                .plan
                .models
                .iter()
                .map(|m| m.display_name.clone())
                .collect(),
            lower_cost: Some("Lower cost (low reasoning)".into()),
            setup: CHATGPT_SETUP.into(),
            install_url: None,
            plan: true,
            notice,
            configurable: false,
            ask_only: false,
            use_consent: None,
        }
    }

    fn local_info(&self, l: &Local) -> AgentInfo {
        let agent = l.agent;
        let state = match &l.status {
            _ if l.sign_in.is_some() => AgentState::SigningIn,
            None => AgentState::Checking,
            Some(s) if !s.installed => AgentState::NotInstalled,
            Some(s) if s.signed_in == Some(false) => AgentState::SignInNeeded,
            Some(_) => AgentState::Ready,
        };
        let notice = l.note.clone().or_else(|| {
            l.status
                .as_ref()
                .and_then(|s| s.version.as_deref())
                .and_then(|v| compat::version_notice(agent, v))
        });
        AgentInfo {
            id: Self::id_of(agent).into(),
            name: agent.name().into(),
            install_url: (state == AgentState::NotInstalled).then(|| {
                match agent {
                    Agent::ClaudeCode => CLAUDE_CODE_URL,
                    Agent::Codex => CODEX_URL,
                }
                .to_string()
            }),
            state,
            privacy: OWN_SIGN_IN.into(),
            consent: None,
            can_sign_out: false,
            models: vec![],
            lower_cost: host::model_choices(agent).get(1).map(|(_, l)| l.clone()),
            setup: match agent {
                Agent::ClaudeCode => CLAUDE_CODE_SETUP,
                Agent::Codex => CODEX_SETUP,
            }
            .into(),
            plan: false,
            notice,
            configurable: false,
            ask_only: false,
            use_consent: None,
        }
    }

    /// Ends the conversation: stops its agent and removes its files.
    fn end_conv(&mut self) {
        let Some(conv) = self.conv.take() else {
            return;
        };
        self.proposals.clear();
        match conv.route {
            Route::Hosted { session, .. } => {
                // The session removes its folder (it made it) and asks the
                // agent to forget the conversation, on its own thread.
                let _ = session.end();
            }
            Route::Direct { turn, .. } => {
                if let Some(turn) = turn {
                    turn.cancel();
                }
            }
            Route::Idle => {}
        }
        work::remove(&conv.dir);
    }

    /// Writes the snapshot when the project changed since the last one.
    /// The project to write as the snapshot when it changed since the last
    /// one: a consistent copy taken on this thread, written by a worker.
    fn snapshot_due(&mut self, project: &ProjectData, revision: u64) -> Option<ProjectData> {
        let conv = self.conv.as_mut().expect("a conversation");
        if conv.snapshot_rev == Some(revision) {
            return None;
        }
        if conv.sent > 0 && !conv.notes.iter().any(|n| n.starts_with("The user applied")) {
            conv.notes.push(
                "The project was changed in the app since the last message; the tools read the current version."
                    .into(),
            );
        }
        conv.snapshot_rev = Some(revision);
        Some(project.clone())
    }

    /// The context lines for this message: the notes waiting, and the
    /// selection when it is not the one the assistant was last told (the
    /// preamble tells the first). Nothing is used up until [`Self::told`].
    fn context(&self, selection: &Selection) -> Option<String> {
        let conv = self.conv.as_ref().expect("a conversation");
        let mut lines = conv.notes.clone();
        if conv.told_selection.as_ref() != Some(selection) {
            lines.push(selection_line(selection));
        }
        (!lines.is_empty()).then(|| lines.join("\n"))
    }

    /// The message went out: its context is known to the assistant now.
    fn told(&mut self, selection: &Selection) {
        let conv = self.conv.as_mut().expect("a conversation");
        conv.notes.clear();
        conv.told_selection = Some(selection.clone());
        conv.sent += 1;
    }

    fn send_hosted(&mut self, agent: Agent, turn: &Turn, project: &ProjectData) {
        let status = self.local(agent).status.clone();
        let Some(status) = status.filter(|s| s.installed) else {
            self.out.push(error(
                ErrorKind::Failed,
                format!("{} is not installed on this PC.", agent.name()),
            ));
            return;
        };
        let cli = match self.config.env.cli() {
            Ok(cli) => cli.to_path_buf(),
            Err(e) => {
                self.out.push(error(ErrorKind::Failed, e.message));
                return;
            }
        };
        if self.conv.as_ref().is_some_and(|c| c.hosted_start.is_some()) {
            self.out.push(error(
                ErrorKind::Failed,
                "The last message is still starting. Send it again in a moment.",
            ));
            return;
        }
        let mode = chat_mode(turn.mode);
        let model = host_model(turn.model);
        let conv = self.conv.as_mut().expect("a conversation");
        let taken = match std::mem::replace(&mut conv.route, Route::Idle) {
            Route::Hosted {
                session, events, ..
            } => Some((session, events)),
            other => {
                conv.route = other;
                None
            }
        };
        let include_ctx = !turn.context.project_id.is_empty();
        let playbooks_dir = playbooks_dir_for(&self.config.instructions);
        let options = taken.is_none().then(|| SessionOptions {
            agent,
            executable: status.executable.clone().unwrap_or_default(),
            model: model.clone(),
            mode,
            cli,
            data_dir: conv.dir.clone(),
            work_dir: conv.dir.clone(),
            snapshot: conv.dir.join(work::SNAPSHOT),
            preamble: if include_ctx {
                build_preamble(project, turn.context, turn.recap)
            } else {
                turn.recap.unwrap_or_default().to_string()
            },
            transcript: None,
            keep_files: false,
            user_instructions: conv.user.as_ref().map(|u| u.text.clone()),
            project_instructions: turn.project_instructions.map(str::to_string),
            playbooks_dir,
            ui_actions: Some(conv.dir.join(UI_ACTIONS_FILE)),
        });
        if options.is_some() {
            // The preamble tells this selection when context is included.
            if include_ctx {
                conv.told_selection = Some(turn.context.selection.clone());
            }
            if let Some(v) = status.version.as_deref() {
                if let Some(line) = compat::version_notice(agent, v) {
                    self.out.push(AgentEvent::Notice(line));
                }
            }
            if let Some(file) = &status.user_instructions {
                self.out.push(AgentEvent::Notice(format!(
                    "{} also reads your own instructions file in every chat ({}). What it says applies here too.",
                    agent.name(),
                    file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
                )));
            }
        }
        let snap = self.snapshot_due(project, turn.revision);
        let context = if include_ctx {
            self.context(&turn.context.selection)
        } else {
            None
        };
        let conv = self.conv.as_mut().expect("a conversation");
        conv.pending_selection = include_ctx.then(|| turn.context.selection.clone());
        let dir = conv.dir.clone();
        let spawner = self.config.spawner.clone();
        let text = turn.text.to_string();
        let due = snap.is_some();
        let (send, receive) = mpsc::channel();
        // Starting a process, writing the snapshot and sending are never
        // done on a frame.
        std::thread::spawn(move || {
            let done = |p: Prepared| {
                let _ = send.send(p);
            };
            let fail = |session, result, wrote| Prepared {
                session,
                agent,
                result: Err(result),
                new_process: true,
                due,
                wrote,
            };
            let started = match taken {
                Some(s) => Ok(s),
                None => Session::start(options.expect("options"), spawner),
            };
            let (mut session, events) = match started {
                Ok(s) => s,
                Err(e) => return done(fail(None, PrepFail::Host(e), false)),
            };
            let mut wrote = false;
            if let Some(project) = snap {
                match work::write_snapshot(&dir, &project) {
                    Ok(_) => wrote = true,
                    Err(e) => {
                        return done(fail(Some((session, events)), PrepFail::Snapshot(e), false))
                    }
                }
            }
            if session.is_busy() {
                session.cancel();
            }
            // What a stopped reply still had in the pipe is not this reply's.
            while events.try_recv().is_ok() {}
            if let Err(e) = session.reconfigure(mode, model) {
                return done(fail(Some((session, events)), PrepFail::Host(e), wrote));
            }
            let new_process = !session.has_live_process();
            let result = session
                .send(&text, context.as_deref())
                .map_err(PrepFail::Host);
            done(Prepared {
                session: Some((session, events)),
                agent,
                result,
                new_process,
                due,
                wrote,
            });
        });
        conv.hosted_start = Some(receive);
    }

    /// A message started on a worker thread has gone out (or failed): the
    /// session back in place, and what it means for the conversation.
    fn poll_hosted_start(&mut self) {
        let Some(conv) = self.conv.as_mut() else {
            return;
        };
        let Some(rx) = &conv.hosted_start else {
            return;
        };
        let p = match rx.try_recv() {
            Ok(p) => p,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                conv.hosted_start = None;
                return;
            }
        };
        conv.hosted_start = None;
        let stopped = std::mem::take(&mut conv.stop_pending);
        let selection = conv.pending_selection.take();
        if let Some((session, events)) = p.session {
            conv.route = Route::Hosted {
                session,
                events,
                agent: p.agent,
            };
        }
        if p.wrote {
            self.snapshot_writes += 1;
        }
        let conv = self.conv.as_mut().expect("a conversation");
        if p.due && !p.wrote {
            conv.snapshot_rev = None;
        }
        match p.result {
            Ok(()) => {
                // Whether the tools were reached is known per agent
                // process: a new process reports its server anew.
                if p.new_process {
                    conv.tools_ok = false;
                    conv.tools_down = false;
                }
                if !conv.ready && conv.waiting_since.is_none() {
                    conv.waiting_since = Some(Instant::now());
                }
                if let Some(selection) = &selection {
                    self.told(selection);
                }
                if stopped {
                    let conv = self.conv.as_mut().expect("a conversation");
                    conv.waiting_since = None;
                    if let Route::Hosted {
                        session, events, ..
                    } = &mut conv.route
                    {
                        session.cancel();
                        // What it wrote before the stop arrived is dropped.
                        while events.try_recv().is_ok() {}
                    }
                }
            }
            Err(PrepFail::Host(e)) => {
                let event = self.host_error(p.agent, &e);
                self.out.push(event);
            }
            Err(PrepFail::Snapshot(e)) => self.out.push(error(ErrorKind::Failed, e)),
        }
    }

    /// An agent host error as the window's status line.
    fn host_error(&self, agent: Agent, e: &AgentError) -> AgentEvent {
        let version = self
            .local(agent)
            .status
            .as_ref()
            .and_then(|s| s.version.clone());
        match e {
            AgentError::NotSignedIn { message, .. } => error(
                ErrorKind::NotSignedIn,
                format!("{} needs you to sign in again. {message}", agent.name()),
            ),
            AgentError::ProcessDied { message } | AgentError::Failed { message } => {
                match compat::launch_problem(message) {
                    Some(problem) => error(
                        ErrorKind::Failed,
                        compat::unsupported_message(agent, version.as_deref(), &problem),
                    ),
                    None => error(ErrorKind::Failed, message.clone()),
                }
            }
            other => error(ErrorKind::Failed, other.to_string()),
        }
    }

    fn send_plan(&mut self, turn: &Turn, project: &ProjectData) {
        if !self.plan_ready() {
            let message = match self.plan_status() {
                oc::SignInStatus::SignedIn { .. } => oc::ChatError::PlanUsageNotEnabled.to_string(),
                _ => oc::ChatError::SignedOut.to_string(),
            };
            self.out.push(error(ErrorKind::NotSignedIn, message));
            return;
        }
        let slug = match turn.model {
            ModelChoice::Named(name) => self
                .plan
                .models
                .iter()
                .find(|m| &m.display_name == name || &m.slug == name)
                .map(|m| m.slug.clone())
                .unwrap_or_else(|| name.clone()),
            _ => match oc::default_model(&self.plan.models) {
                Some(m) => m.slug.clone(),
                None => {
                    self.out.push(error(
                        ErrorKind::Failed,
                        "The list of ChatGPT models has not arrived yet. Send it again in a moment.",
                    ));
                    return;
                }
            },
        };
        let provider: Arc<dyn Direct> = Arc::new(ChatGpt(self.plan.client.clone()));
        let effort = (*turn.model == ModelChoice::LowerCost).then(|| "low".to_string());
        self.send_direct(turn, project, provider, slug, effort, oc::Conversation::new);
    }

    /// A message on a direct route (ChatGPT, the endpoint): the snapshot,
    /// the context, the one chat prompt as the fixed instructions, and the
    /// turn started on `provider` (after a stopped one let go, on a thread).
    fn send_direct(
        &mut self,
        turn: &Turn,
        project: &ProjectData,
        provider: Arc<dyn Direct>,
        model: String,
        reasoning_effort: Option<String>,
        fresh: impl FnOnce() -> oc::Conversation,
    ) {
        let include_ctx = !turn.context.project_id.is_empty();
        let snap = self.snapshot_due(project, turn.revision);
        let due = snap.is_some();
        let conv = self.conv.as_mut().expect("a conversation");
        if matches!(conv.route, Route::Idle) {
            conv.route = Route::Direct {
                conversation: Some(fresh()),
                turn: None,
                starting: None,
                preamble: build_preamble(project, turn.context, turn.recap),
            };
            if include_ctx {
                conv.told_selection = Some(turn.context.selection.clone());
            }
        }
        let context = if include_ctx {
            self.context(&turn.context.selection)
        } else {
            None
        };
        let playbooks_dir = playbooks_dir_for(&self.config.instructions);
        let conv = self.conv.as_mut().expect("a conversation");
        let snapshot = conv.dir.join(work::SNAPSHOT);
        let user = conv.user.as_ref().map(|u| u.text.clone());
        let Route::Direct {
            conversation,
            turn: running,
            starting,
            preamble,
        } = &mut conv.route
        else {
            return;
        };
        if starting.is_some() {
            self.out.push(error(
                ErrorKind::Failed,
                "The last message is still starting. Send it again in a moment.",
            ));
            return;
        }
        let user_text = match context {
            Some(c) => format!("Context: {c}\n\n{}", turn.text),
            None => turn.text.to_string(),
        };
        let request = oc::TurnRequest {
            model,
            context: if include_ctx {
                preamble.clone()
            } else {
                turn.recap.unwrap_or_default().to_string()
            },
            user_text,
            reasoning_effort,
        };
        // The one chat prompt of every route, with this message's mode.
        let prompt = chat_prompt::system_prompt_with(
            chat_mode(turn.mode),
            user.as_deref(),
            turn.project_instructions,
        );
        let ui_actions = Some(conv.dir.join(UI_ACTIONS_FILE));
        let tools: Arc<dyn oc::Tools> = Arc::new(DirectTools {
            context: chat_tools::ChatContext {
                snapshot: Some(snapshot),
                mode: chat_mode(turn.mode),
                playbooks_dir,
                ui_actions,
            },
        });
        // The snapshot is written, a stopped reply lets go of the
        // conversation and the turn starts on a worker, never on a frame.
        let old = running.take();
        let history = conversation.take();
        let dir = conv.dir.clone();
        let (send, receive) = mpsc::channel();
        std::thread::spawn(move || {
            let mut history = match old {
                Some(old) => {
                    old.cancel();
                    old.join()
                }
                None => history.unwrap_or_default(),
            };
            if let Some(project) = snap {
                if let Err(e) = work::write_snapshot(&dir, &project) {
                    let _ = send.send(Err((e, history)));
                    return;
                }
            }
            history.set_instructions(prompt);
            let turn = provider.start_turn(history, request, tools);
            if let Err(mpsc::SendError(Ok(turn))) = send.send(Ok(turn)) {
                // The conversation ended meanwhile: nothing goes out, and
                // nothing is left in its folder.
                turn.cancel();
                work::remove(&dir);
            }
        });
        *starting = Some(receive);
        conv.pending_selection = Some(turn.context.selection.clone());
        conv.snapshot_pending = due;
    }

    fn keep_proposal(&mut self, changes: Value, diff: Option<&Value>) {
        let rows = proposal_rows(&changes, diff);
        let impact = diff.and_then(rows::impact_summary);
        let id = self.next_proposal;
        self.next_proposal += 1;
        let revision = self.conv.as_ref().and_then(|c| c.snapshot_rev);
        // A new proposal takes the place of one not answered.
        self.proposals.clear();
        self.proposals.insert(
            id,
            Proposal {
                changes,
                rows: rows.clone(),
                revision,
            },
        );
        self.out.push(AgentEvent::Proposed { proposal: id, rows });
        if let Some(summary) = impact {
            self.out.push(AgentEvent::ProposalImpact {
                proposal: id,
                summary,
            });
        }
    }

    fn poll_ui_actions(&mut self) {
        let Some(conv) = self.conv.as_mut() else {
            return;
        };
        let path = conv.dir.join(UI_ACTIONS_FILE);
        let Ok(bytes) = std::fs::read(&path) else {
            return;
        };
        if bytes.len() <= conv.ui_actions_offset {
            return;
        }
        let slice = &bytes[conv.ui_actions_offset..];
        let Some(last_nl) = slice.iter().rposition(|&b| b == b'\n') else {
            return;
        };
        let chunk = &slice[..=last_nl];
        conv.ui_actions_offset += chunk.len();
        if let Ok(text) = std::str::from_utf8(chunk) {
            for line in text.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                if let Ok(val) = serde_json::from_str::<Value>(line) {
                    self.out.push(AgentEvent::UiAction(val));
                }
            }
        }
    }

    fn poll_hosted(&mut self) {
        let mut got = vec![];
        let mut timed_out = None;
        if let Some(conv) = self.conv.as_mut() {
            if let Route::Hosted {
                session,
                events,
                agent,
            } = &mut conv.route
            {
                while let Ok(e) = events.try_recv() {
                    got.push(e);
                }
                if !got.is_empty() {
                    conv.ready = true;
                    conv.waiting_since = None;
                } else if let Some(at) = conv.waiting_since {
                    if at.elapsed() >= self.config.ready_timeout && session.is_busy() {
                        session.cancel();
                        while events.try_recv().is_ok() {}
                        conv.waiting_since = None;
                        timed_out = Some(*agent);
                    }
                }
            }
        }
        if let Some(agent) = timed_out {
            self.out
                .push(error(ErrorKind::Failed, compat::no_answer_message(agent)));
        }
        let agent = match self.conv.as_ref().map(|c| &c.route) {
            Some(Route::Hosted { agent, .. }) => *agent,
            _ => return,
        };
        for e in got {
            // Hold back what an agent writes without its tools.
            let (ok, down) = self
                .conv
                .as_ref()
                .map_or((true, false), |c| (c.tools_ok, c.tools_down));
            let ungrounded = down && !ok;
            match e {
                Event::Ready { .. } => self.out.push(AgentEvent::Ready),
                Event::TextDelta { .. } | Event::Message { .. } if ungrounded => {}
                Event::TextDelta { text } => self.out.push(AgentEvent::TextDelta(text)),
                Event::Message { text } => self.out.push(AgentEvent::Message(text)),
                Event::ToolStarted { id, tool, target } => self.out.push(AgentEvent::ToolStarted {
                    call: id,
                    tool: rows::tool_label(&tool),
                    target,
                }),
                Event::ToolFinished {
                    id,
                    ok,
                    duration_ms,
                    ..
                } => {
                    if ok {
                        if let Some(c) = self.conv.as_mut() {
                            c.tools_ok = true;
                        }
                    }
                    self.out.push(AgentEvent::ToolFinished {
                        call: id,
                        ok,
                        millis: duration_ms,
                    })
                }
                Event::ProposedChanges { changes, diff, .. } => {
                    self.keep_proposal(changes, diff.as_ref())
                }
                Event::TurnFinished {
                    cancelled: true, ..
                } => {}
                Event::TurnFinished {
                    cancelled: false, ..
                } if ungrounded => {
                    self.out.push(error(ErrorKind::Failed, TOOLS_UNREACHABLE));
                }
                Event::TurnFinished { usage, .. } => self.out.push(AgentEvent::TurnFinished {
                    usage: Some(Usage {
                        input: usage.input_tokens,
                        output: usage.output_tokens,
                    }),
                }),
                // A line the host could not read: reading goes on.
                Event::Error {
                    error: AgentError::BadOutput { .. },
                } => {}
                // Still connecting at start ("pending"): not a failure; a
                // tool call tells.
                Event::Error {
                    error: AgentError::Failed { message },
                } if message.contains("(pending)") => {}
                Event::Error {
                    error: AgentError::Failed { message } | AgentError::ProcessDied { message },
                } if tools_down(&message) => {
                    // The reply's end says it (the host sends one).
                    if let Some(c) = self.conv.as_mut() {
                        c.tools_down = true;
                    }
                }
                Event::Error { error: e } => {
                    let event = self.host_error(agent, &e);
                    self.out.push(event);
                }
            }
        }
    }

    fn poll_direct(&mut self) {
        let mut got = vec![];
        let mut started_snapshot = false;
        let mut told = None;
        let mut failed = None;
        if let Some(conv) = self.conv.as_mut() {
            if let Route::Direct {
                conversation,
                turn,
                starting,
                ..
            } = &mut conv.route
            {
                if let Some(rx) = starting {
                    match rx.try_recv() {
                        Ok(Ok(t)) => {
                            if std::mem::take(&mut conv.stop_pending) {
                                t.cancel();
                            }
                            *turn = Some(t);
                            *starting = None;
                            if std::mem::take(&mut conv.snapshot_pending) {
                                started_snapshot = true;
                            }
                            told = conv.pending_selection.take();
                        }
                        Ok(Err((e, history))) => {
                            *conversation = Some(history);
                            *starting = None;
                            conv.snapshot_pending = false;
                            conv.snapshot_rev = None;
                            conv.pending_selection = None;
                            failed = Some(e);
                        }
                        Err(TryRecvError::Disconnected) => *starting = None,
                        Err(TryRecvError::Empty) => {}
                    }
                }
                let mut ended = false;
                if let Some(t) = turn.as_ref() {
                    loop {
                        match t.events.try_recv() {
                            Ok(e) => {
                                ended |= matches!(
                                    e,
                                    oc::ChatEvent::TurnFinished(_)
                                        | oc::ChatEvent::Error(_)
                                        | oc::ChatEvent::Cancelled
                                );
                                got.push(e);
                            }
                            Err(TryRecvError::Empty) => break,
                            Err(TryRecvError::Disconnected) => {
                                ended = true;
                                break;
                            }
                        }
                    }
                }
                if ended {
                    if let Some(t) = turn.take() {
                        *conversation = Some(t.join());
                    }
                }
            }
        }
        if started_snapshot {
            self.snapshot_writes += 1;
        }
        if let Some(selection) = told {
            self.told(&selection);
        }
        if let Some(e) = failed {
            self.out.push(error(ErrorKind::Failed, e));
        }
        for e in got {
            match e {
                oc::ChatEvent::TextDelta(t) => self.out.push(AgentEvent::TextDelta(t)),
                oc::ChatEvent::Message(m) => self.out.push(AgentEvent::Message(m)),
                oc::ChatEvent::ToolStarted { id, name, target } => {
                    self.out.push(AgentEvent::ToolStarted {
                        call: id,
                        tool: rows::tool_label(&name),
                        target,
                    })
                }
                oc::ChatEvent::ToolFinished {
                    id, ok, duration, ..
                } => self.out.push(AgentEvent::ToolFinished {
                    call: id,
                    ok,
                    millis: duration.as_millis() as u64,
                }),
                oc::ChatEvent::ProposedChanges { changes, .. } => {
                    let diff = changes.payload["diff"].clone();
                    self.keep_proposal(
                        changes.payload["changes"].clone(),
                        (!diff.is_null()).then_some(&diff),
                    );
                }
                oc::ChatEvent::TurnFinished(u) if !u.is_known() => {
                    // The server gave no counts: say so, never zeros.
                    self.out.push(AgentEvent::UsageNotReported);
                    self.out.push(AgentEvent::TurnFinished { usage: None });
                }
                oc::ChatEvent::TurnFinished(u) => self.out.push(AgentEvent::TurnFinished {
                    usage: Some(Usage {
                        input: u.input_tokens,
                        output: u.output_tokens,
                    }),
                }),
                oc::ChatEvent::Error(e) => self.out.push(direct_error(&e)),
                oc::ChatEvent::Cancelled => {}
            }
        }
    }

    fn poll_setup(&mut self) {
        let (env, spawner) = (self.config.env.clone(), self.config.spawner.clone());
        for l in &mut self.locals {
            if let Some(rx) = &l.detecting {
                match rx.try_recv() {
                    Ok(s) => {
                        l.status = Some(s);
                        l.detecting = None;
                    }
                    Err(TryRecvError::Disconnected) => l.detecting = None,
                    Err(TryRecvError::Empty) => {}
                }
            }
            if let Some(sign_in) = &mut l.sign_in {
                if let SignInState::Finished { ok } = sign_in.poll() {
                    l.sign_in = None;
                    l.note = (!ok).then(|| {
                        format!(
                            "The {} sign-in did not finish. Try again, or sign in from a command prompt.",
                            l.agent.name()
                        )
                    });
                    l.detecting = Some(spawn_detect(l.agent, &env, &spawner));
                }
            }
        }
        if let Some(handle) = &self.plan.signing {
            match handle.result.try_recv() {
                Ok(Ok(_account)) => {
                    self.plan.signing = None;
                    self.plan.note = None;
                    self.plan.models_asked = false;
                    self.refresh_plan();
                }
                Ok(Err(oc::AuthError::Cancelled)) => self.plan.signing = None,
                Ok(Err(e)) => {
                    self.plan.signing = None;
                    self.plan.note = Some(e.to_string());
                }
                Err(TryRecvError::Disconnected) => self.plan.signing = None,
                Err(TryRecvError::Empty) => {}
            }
        }
        if let Some(rx) = &self.plan.status_rx {
            match rx.try_recv() {
                Ok(s) => {
                    self.plan.status = Some(s);
                    self.plan.status_rx = None;
                }
                Err(TryRecvError::Disconnected) => self.plan.status_rx = None,
                Err(TryRecvError::Empty) => {}
            }
        }
        if let Some(rx) = &self.plan.signing_out {
            if !matches!(rx.try_recv(), Err(TryRecvError::Empty)) {
                self.plan.signing_out = None;
                self.refresh_plan();
            }
        }
        if self.plan_ready() {
            let due = self.plan.models_retry.is_none_or(|at| Instant::now() >= at);
            if !self.plan.models_asked && due {
                self.plan.models_asked = true;
                self.plan.models_retry = None;
                let (send, receive) = mpsc::channel();
                let client = self.plan.client.clone();
                std::thread::spawn(move || {
                    let _ = send.send(client.list_models(&oc::CancelToken::new()));
                });
                self.plan.models_rx = Some(receive);
            }
        } else {
            self.plan.models_asked = false;
            self.plan.models.clear();
        }
        if let Some(rx) = &self.plan.models_rx {
            match rx.try_recv() {
                Ok(Ok(models)) => {
                    self.plan.models = models;
                    self.plan.models_rx = None;
                }
                Ok(Err(e)) => {
                    self.plan.note = Some(format!("The model list did not load: {e}"));
                    self.plan.models_rx = None;
                    self.plan.models_asked = false;
                    self.plan.models_retry =
                        Some(Instant::now() + std::time::Duration::from_secs(30));
                }
                Err(TryRecvError::Disconnected) => self.plan.models_rx = None,
                Err(TryRecvError::Empty) => {}
            }
        }
    }

    /// The environment, spawner and sign-in the checks read.
    pub(crate) fn check_inputs(&self) -> checks::Inputs {
        checks::Inputs {
            env: self.config.env.clone(),
            spawner: self.config.spawner.clone(),
            auth: self.config.auth.clone(),
            hosts: self.config.hosts.clone(),
            http: oc::http::HttpConfig {
                connect_timeout: std::time::Duration::from_secs(5),
                read_timeout: std::time::Duration::from_secs(5),
            },
            endpoint: self.endpoint.conn.clone(),
            keys: self.config.keys.clone(),
        }
    }
}

/// A route that talks to a model API itself and reports `ChatEvent`s over
/// the chat tools: the ChatGPT plan now; an OpenAI-compatible endpoint is
/// meant to be a second instance of the same adapter (the conversation,
/// snapshot, context, events, proposals and Apply are shared).
pub(crate) trait Direct: Send + Sync {
    fn start_turn(
        &self,
        history: oc::Conversation,
        request: oc::TurnRequest,
        tools: Arc<dyn oc::Tools>,
    ) -> oc::TurnHandle;
}

/// The ChatGPT plan route.
struct ChatGpt(Arc<oc::ChatClient>);

impl Direct for ChatGpt {
    fn start_turn(
        &self,
        history: oc::Conversation,
        request: oc::TurnRequest,
        tools: Arc<dyn oc::Tools>,
    ) -> oc::TurnHandle {
        self.0.start_turn(history, request, tools)
    }
}

/// Where a test build keeps the user's instructions file: a folder of its
/// own in the temp directory, never the data folder.
pub(crate) fn test_instructions_path() -> PathBuf {
    chat_prompt::instructions_path(
        &std::env::temp_dir()
            .join("openxer-chat-tests")
            .join(format!("instructions-{}", std::process::id())),
    )
}

/// A direct route's error as the window's status line.
pub(crate) fn direct_error(e: &oc::ChatError) -> AgentEvent {
    match e {
        oc::ChatError::SignedOut | oc::ChatError::PlanUsageNotEnabled => {
            error(ErrorKind::NotSignedIn, e.to_string())
        }
        oc::ChatError::UsageLimit { .. } => error(ErrorKind::UsageLimit, oc::ui::USAGE_LIMIT_BODY),
        other => error(ErrorKind::Failed, other.to_string()),
    }
}

/// The chat tools of the direct routes: the same definitions and the same
/// calls on the same snapshot as the agent host's MCP server.
pub(crate) struct DirectTools {
    pub context: chat_tools::ChatContext,
}

impl oc::Tools for DirectTools {
    fn definitions(&self) -> Vec<oc::ToolDef> {
        chat_tools::tools(&self.context)
            .into_iter()
            .map(|t| oc::ToolDef {
                name: t.name.to_string(),
                description: t.description,
                parameters: t.input_schema,
            })
            .collect()
    }

    fn call(&self, name: &str, arguments: &Value, _cancel: &oc::CancelToken) -> oc::ToolOutput {
        let result = chat_tools::call(&self.context, name, arguments.clone());
        if result.is_error {
            return oc::ToolOutput::error(result.text);
        }
        if name == chat_tools::APPLY_TOOL {
            let diff = result.envelope["result"]["report"]["diff"].clone();
            let n = arguments["changes"]["operations"]
                .as_array()
                .map_or(0, Vec::len);
            return oc::ToolOutput::proposed(
                result.text,
                oc::ProposedChanges {
                    summary: format!("{n} proposed change{}", if n == 1 { "" } else { "s" }),
                    payload: json!({"changes": arguments["changes"], "diff": diff}),
                },
            );
        }
        oc::ToolOutput::ok(result.text)
    }

    fn target(&self, name: &str, arguments: &Value) -> String {
        host::tool_target(name, arguments)
    }
}

impl ChatBackend for LiveBackend {
    fn agents(&self) -> Vec<AgentInfo> {
        let mut out = vec![self.plan_info(), self.endpoint_info()];
        out.extend(self.locals.iter().map(|l| self.local_info(l)));
        out
    }

    fn new_conversation(&mut self, _agent: &str) {
        self.end_conv();
    }

    fn send(&mut self, turn: Turn) {
        let Some(project) = turn.project else {
            self.out.push(error(
                ErrorKind::Failed,
                "No project is open. Open a project to ask about its schedule.",
            ));
            return;
        };
        if self.conv.as_ref().is_some_and(|c| c.agent != turn.agent) {
            self.end_conv();
        }
        if self.conv.is_none() {
            self.conv = Some(Conv::new(
                turn.agent,
                &self.config.temp,
                &self.config.instructions,
            ));
            // Once per conversation: the user's file was too long.
            if let Some(u) = self.conv.as_ref().and_then(|c| c.user.as_ref()) {
                if u.cut {
                    self.out.push(AgentEvent::Notice(format!(
                        "Your instructions file was cut at {} characters; the rest is not used.",
                        chat_prompt::USER_LIMIT
                    )));
                }
            }
        }
        match Self::agent_of(turn.agent) {
            Some(agent) => self.send_hosted(agent, &turn, project),
            None if turn.agent == CHATGPT => self.send_plan(&turn, project),
            None if turn.agent == endpoint::ENDPOINT => self.send_endpoint(&turn, project),
            None => self
                .out
                .push(error(ErrorKind::Failed, "That agent is not available.")),
        }
    }

    fn poll(&mut self) -> Vec<AgentEvent> {
        self.poll_setup();
        self.poll_endpoint();
        self.poll_hosted_start();
        self.poll_hosted();
        self.poll_direct();
        self.poll_ui_actions();
        if let Some(c) = &mut self.checks {
            c.poll();
        }
        std::mem::take(&mut self.out)
    }

    fn stop(&mut self) {
        let Some(conv) = self.conv.as_mut() else {
            return;
        };
        conv.waiting_since = None;
        match &mut conv.route {
            Route::Hosted { session, .. } => session.cancel(),
            Route::Direct { turn: Some(t), .. } => t.cancel(),
            _ => {}
        }
        // A message still starting is stopped as it arrives.
        let starting = matches!(
            &conv.route,
            Route::Direct {
                starting: Some(_),
                ..
            }
        );
        if conv.hosted_start.is_some() || starting {
            conv.stop_pending = true;
        }
    }

    fn apply(
        &mut self,
        proposal: u64,
        project: &ProjectData,
        revision: u64,
    ) -> Result<Applied, String> {
        let Some(p) = self.proposals.get_mut(&proposal) else {
            return Err("this proposal is no longer open.".into());
        };
        let mut next = project.clone();
        let report = chat_tools::apply_proposal(&mut next, p.changes.clone())
            .map_err(|e| format!("it no longer applies to the project: {}", e.message))?;
        if p.revision != Some(revision) {
            let now = rows::rows(&report["diff"]);
            if now != p.rows {
                p.rows = now.clone();
                p.revision = Some(revision);
                return Ok(Applied::Refreshed(now));
            }
        }
        let n = p.rows.len();
        self.proposals.remove(&proposal);
        if let Some(conv) = self.conv.as_mut() {
            conv.notes.push(format!(
                "The user applied the proposed changes ({n} change row{}). The project now includes them.",
                if n == 1 { "" } else { "s" }
            ));
        }
        Ok(Applied::Project(
            Box::new(next),
            "Chat changes applied. Ctrl+Z undoes them.".into(),
        ))
    }

    fn discard(&mut self, proposal: u64) {
        if self.proposals.remove(&proposal).is_some() {
            if let Some(conv) = self.conv.as_mut() {
                conv.notes
                    .push("The user discarded the proposed changes; nothing was changed.".into());
            }
        }
    }

    fn sign_in(&mut self, agent: &str) {
        if agent == CHATGPT {
            if self.plan.signing.is_none() {
                self.plan.note = None;
                let opener = self.config.opener.clone();
                self.plan.signing = Some(
                    self.config
                        .auth
                        .start_sign_in(Box::new(move |url: &str| opener(url))),
                );
            }
            return;
        }
        let Some(a) = Self::agent_of(agent) else {
            return;
        };
        let exe = self
            .local(a)
            .status
            .as_ref()
            .and_then(|s| s.executable.clone());
        let Some(exe) = exe else {
            return;
        };
        let spawner = self.config.spawner.clone();
        let l = self.local_mut(a);
        l.note = None;
        match SignIn::start(a, &exe, spawner.as_ref()) {
            Ok(s) => l.sign_in = Some(s),
            Err(e) => l.note = Some(e.to_string()),
        }
    }

    fn cancel_sign_in(&mut self, agent: &str) {
        if agent == CHATGPT {
            if let Some(h) = self.plan.signing.take() {
                h.cancel.cancel();
            }
            return;
        }
        if let Some(a) = Self::agent_of(agent) {
            if let Some(mut s) = self.local_mut(a).sign_in.take() {
                s.cancel();
            }
        }
    }

    fn sign_out(&mut self, agent: &str) {
        if agent != CHATGPT {
            return;
        }
        if self.conv.as_ref().is_some_and(|c| c.agent == CHATGPT) {
            self.end_conv();
        }
        let auth = self.config.auth.clone();
        // Revoking waits on the network: never on this thread. The stored
        // tokens are gone when it returns; the state follows.
        let (send, receive) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = auth.sign_out(&oc::CancelToken::new());
            let _ = send.send(());
        });
        self.plan.signing_out = Some(receive);
        self.plan.status = Some(oc::SignInStatus::SignedOut);
        self.plan.models.clear();
    }

    fn end(&mut self) {
        self.end_conv();
    }

    fn check_again(&mut self, agent: &str) {
        if let Some(a) = Self::agent_of(agent) {
            let rx = spawn_detect(a, &self.config.env, &self.config.spawner);
            let l = self.local_mut(a);
            l.status = None;
            l.note = None;
            l.detecting = Some(rx);
        }
    }

    fn endpoint(&self) -> Option<oc::compat::Connection> {
        self.endpoint.conn.clone()
    }

    fn set_endpoint(
        &mut self,
        conn: Option<oc::compat::Connection>,
        key: Option<oc::Secret>,
    ) -> Receiver<Result<(), String>> {
        self.save_endpoint(conn, key)
    }

    fn remove_endpoint_key(&mut self) -> Option<Receiver<Result<(), String>>> {
        Some(LiveBackend::remove_endpoint_key(self))
    }

    fn endpoint_has_key(&self, conn: &oc::compat::Connection) -> bool {
        self.endpoint.conn.as_ref().is_some_and(|c| {
            c.id == conn.id && oc::compat::key_origin(c) == oc::compat::key_origin(conn)
        }) && self.endpoint.key_present == Some(true)
    }

    fn check_endpoint(
        &mut self,
        conn: oc::compat::Connection,
        key: Option<oc::Secret>,
    ) -> Option<Receiver<EndpointCheck>> {
        Some(self.test_endpoint(conn, key))
    }

    fn check_setup(&mut self, restart: bool) -> Option<&mut Checks> {
        if restart || self.checks.is_none() {
            // The checks read the stores anew: so do the cached states.
            self.refresh_plan();
            self.refresh_endpoint_key();
            self.checks = Some(Checks::start(self.check_inputs()));
        }
        self.checks.as_mut()
    }
}

impl Drop for LiveBackend {
    fn drop(&mut self) {
        self.end_conv();
    }
}
