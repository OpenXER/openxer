//! What the Chat window needs from the agent layer: the agents on this PC
//! with their state, and a conversation driven by plain-data events the
//! window polls once a frame. Nothing here blocks: a backend runs its agent
//! elsewhere (a child process, a network client on its own thread) and hands
//! back what happened since the last poll.
//!
//! The app runs [`super::live::LiveBackend`] (Claude Code, Codex and
//! ChatGPT); [`OfflineBackend`] stands in until Chat is first opened. The
//! window's own tests and renders drive a scripted [`FakeBackend`].
use openxer_xer::ProjectData;

/// The state of one agent entry, as the window shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AgentState {
    /// Installed and usable now.
    Ready,
    /// The program is not on this PC.
    NotInstalled,
    /// It needs the user to sign in first (the window offers Sign In...).
    SignInNeeded,
    /// A sign-in is under way in the browser (the window offers Cancel).
    SigningIn,
    /// Signed in, under this account; usable now.
    SignedIn { account: String },
    /// Still being looked for on this PC.
    Checking,
    /// A route the user sets up first (the API key or local model).
    NotSetUp,
}

impl AgentState {
    /// A conversation can be held with the agent in this state.
    pub(crate) fn usable(&self) -> bool {
        matches!(self, AgentState::Ready | AgentState::SignedIn { .. })
    }
}

/// One agent the backend can talk to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AgentInfo {
    /// Stable key (kept in the user's preferences).
    pub id: String,
    /// The name the Agent drop-down shows.
    pub name: String,
    pub state: AgentState,
    /// What the empty conversation says about where the questions go, for
    /// this agent: only an agent that runs under its own sign-in can say
    /// that OpenXER itself sends nothing off this computer.
    pub privacy: String,
    /// Asked once, in a message box, before the first sign-in: what signing
    /// in lets OpenXER send and to whom. None: nothing to ask.
    pub consent: Option<String>,
    /// The agent can be signed out from OpenXER.
    pub can_sign_out: bool,
    /// Models the user may pick besides the agent's default and a lower
    /// cost one; empty when the agent offers no choice.
    pub models: Vec<String>,
    /// How Chat Options names the lower cost choice (None: "Lower cost").
    pub lower_cost: Option<String>,
    /// The setup list's sentence: what this route needs, whose account it
    /// uses and that use counts against that plan.
    pub setup: String,
    /// The vendor's install page, when the program is not installed.
    pub install_url: Option<String>,
    /// The route uses the user's ChatGPT plan: the window shows OpenAI's
    /// required wording ("Using ChatGPT plan", "Manage usage", the welcome
    /// box once).
    pub plan: bool,
    /// One more line for the setup list (an old version, why the last
    /// sign-in failed).
    pub notice: Option<String>,
    /// Set up in OpenXER (Set Up...): the API key or local model.
    pub configurable: bool,
    /// Its answers may be unreliable: the window starts it in Ask only.
    pub ask_only: bool,
    /// Asked once before the first message goes to this service: the
    /// preference key that remembers it, and what is sent where. None for
    /// a route that sends nothing off this PC itself, or asks at sign-in.
    pub use_consent: Option<(String, String)>,
}

/// What a Test of the endpoint settings found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EndpointCheck {
    /// The plain result line.
    pub line: String,
    pub ok: bool,
    /// The models the server lists.
    pub models: Vec<String>,
}

/// Whether the agent may only answer, or may also propose changes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum Mode {
    #[default]
    Ask,
    Edit,
}

/// Which model a conversation uses.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum ModelChoice {
    /// The agent's own default.
    #[default]
    AgentDefault,
    /// The agent's lower cost model.
    LowerCost,
    /// A model the backend listed by name.
    Named(String),
}

/// What is selected in the Activities table.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum Selection {
    #[default]
    None,
    /// One activity, by Activity ID.
    One(String),
    /// Several, by Activity ID, in table order.
    Many(Vec<String>),
}

/// What the agent is told with every message without being asked.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ChatContext {
    pub project_id: String,
    pub selection: Selection,
    /// The data date as the status bar shows it.
    pub data_date: String,
    /// Assigned project baseline name, if any.
    pub baseline: Option<String>,
    /// Active filter summary, if any filter is restricting rows.
    pub filter: Option<String>,
    /// Open activity detail tab name, if Details pane is visible.
    pub detail_tab: Option<String>,
}

/// One message the user sends.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Turn<'a> {
    /// The agent the window uses.
    pub agent: &'a str,
    pub text: &'a str,
    pub mode: Mode,
    pub context: &'a ChatContext,
    pub model: &'a ModelChoice,
    /// The open project, for the snapshot the tools read and the preamble.
    pub project: Option<&'a ProjectData>,
    /// Changes whenever the project changes (a new snapshot is due).
    pub revision: u64,
    /// Optional per-project standing rules for this schedule.
    pub project_instructions: Option<&'a str>,
    /// Optional recap of an earlier visible conversation when resuming after
    /// reopening Chat or loading from History.
    pub recap: Option<&'a str>,
}

/// One change of a proposal: an activity's field from one value to another.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct ChangeRow {
    pub activity: String,
    pub field: String,
    pub old: String,
    pub new: String,
}

/// Tokens a reply used, when the agent reports them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Usage {
    pub input: u64,
    pub output: u64,
}

/// Why a turn ended in an error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ErrorKind {
    /// Anything else that went wrong.
    Failed,
    /// The account's usage limit is reached; nothing is retried.
    UsageLimit,
    /// The sign-in ran out or was withdrawn.
    NotSignedIn,
}

/// What happened in the conversation since the last poll. Plain data, so an
/// agent layer can send it across a channel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AgentEvent {
    /// The agent took the conversation (a session started).
    Ready,
    /// More text of the reply being written.
    TextDelta(String),
    /// A whole reply; it replaces the text streamed for it so far.
    Message(String),
    /// The agent started a tool (`call` names this run of it).
    ToolStarted {
        call: String,
        tool: String,
        target: String,
    },
    ToolFinished {
        call: String,
        ok: bool,
        millis: u64,
    },
    /// Changes the agent proposes (Edit mode); nothing is written until the
    /// user applies them.
    Proposed {
        proposal: u64,
        rows: Vec<ChangeRow>,
    },
    /// Short schedule impact headline for proposal `proposal`.
    ProposalImpact {
        proposal: u64,
        summary: String,
    },
    /// The reply is complete.
    TurnFinished {
        usage: Option<Usage>,
    },
    Error {
        kind: ErrorKind,
        message: String,
    },
    /// A note in the conversation that does not end the reply (an old
    /// agent version, a personal instructions file, a refreshed proposal).
    Notice(String),
    /// The server gave no token counts for the reply that ends next.
    UsageNotReported,
    /// A live desktop UI action queued by `openxer_ui` (filter, view, sort,
    /// columns, select, critical path, zoom, dialog, export, undo/redo).
    UiAction(serde_json::Value),
}

/// What Apply leads to.
#[derive(Debug)]
pub(crate) enum Applied {
    /// The backend writes nothing itself (a scripted backend).
    #[cfg_attr(not(test), allow(dead_code))]
    Nothing,
    /// The project with the change made, to put in place as one undo step,
    /// and a status line.
    Project(Box<ProjectData>, String),
    /// The project changed since the preview and the change now reads
    /// differently: these rows, for the user to look at and apply again.
    Refreshed(Vec<ChangeRow>),
}

/// The agent layer as the Chat window drives it, once a frame on the UI
/// thread. No call may block.
pub(crate) trait ChatBackend {
    /// The agents this backend offers, with their state now.
    fn agents(&self) -> Vec<AgentInfo>;
    /// Starts a new, empty conversation with `agent` (a reset).
    fn new_conversation(&mut self, agent: &str);
    /// Sends one message of the conversation.
    fn send(&mut self, turn: Turn);
    /// What happened since the last poll.
    fn poll(&mut self) -> Vec<AgentEvent>;
    /// Stops the reply being written.
    fn stop(&mut self);
    /// Works out a proposal on `project` (whose state is `revision`) for
    /// Apply: called once per Apply; the window puts the result in place
    /// as one undo step.
    fn apply(
        &mut self,
        proposal: u64,
        project: &ProjectData,
        revision: u64,
    ) -> Result<Applied, String>;
    fn discard(&mut self, proposal: u64);
    /// Starts signing in to `agent` (usually in the browser).
    fn sign_in(&mut self, agent: &str);
    fn cancel_sign_in(&mut self, agent: &str);
    fn sign_out(&mut self, agent: &str);
    /// Ends the conversation and removes its files (the window closed, the
    /// project closed, the app exits).
    fn end(&mut self) {}
    /// Looks for `agent` on this PC again.
    fn check_again(&mut self, _agent: &str) {}
    /// Check Setup: the checks of the last run (`restart`: a new run).
    fn check_setup(&mut self, _restart: bool) -> Option<&mut super::checks::Checks> {
        None
    }
    /// The stand-in until a real backend is made.
    fn is_offline(&self) -> bool {
        false
    }
    /// The API key or local model connection, when one is set up.
    fn endpoint(&self) -> Option<openxer_chat::compat::Connection> {
        None
    }
    /// Uses `conn` from now on (None: none); a typed `key` is stored for it
    /// on a worker thread. The receiver says when that is done.
    fn set_endpoint(
        &mut self,
        _conn: Option<openxer_chat::compat::Connection>,
        _key: Option<openxer_chat::Secret>,
    ) -> std::sync::mpsc::Receiver<Result<(), String>> {
        let (send, receive) = std::sync::mpsc::channel();
        let _ = send.send(Err("No agent is set up.".into()));
        receive
    }
    /// Removes the stored key of the connection, on a worker thread.
    fn remove_endpoint_key(&mut self) -> Option<std::sync::mpsc::Receiver<Result<(), String>>> {
        None
    }
    /// Whether a key is stored for `conn`.
    fn endpoint_has_key(&self, _conn: &openxer_chat::compat::Connection) -> bool {
        false
    }
    /// Tests `conn` (with a typed `key`, else the stored one) on a worker
    /// thread: the receiver gets the result.
    fn check_endpoint(
        &mut self,
        _conn: openxer_chat::compat::Connection,
        _key: Option<openxer_chat::Secret>,
    ) -> Option<std::sync::mpsc::Receiver<EndpointCheck>> {
        None
    }
}

/// The backend until an agent layer is joined: no agents.
#[derive(Default)]
pub(crate) struct OfflineBackend;

impl ChatBackend for OfflineBackend {
    fn agents(&self) -> Vec<AgentInfo> {
        vec![]
    }
    fn new_conversation(&mut self, _agent: &str) {}
    fn send(&mut self, _turn: Turn) {}
    fn poll(&mut self) -> Vec<AgentEvent> {
        vec![]
    }
    fn stop(&mut self) {}
    fn apply(&mut self, _: u64, _: &ProjectData, _: u64) -> Result<Applied, String> {
        Err("No agent is set up.".into())
    }
    fn discard(&mut self, _proposal: u64) {}
    fn sign_in(&mut self, _agent: &str) {}
    fn cancel_sign_in(&mut self, _agent: &str) {}
    fn sign_out(&mut self, _agent: &str) {}
    fn is_offline(&self) -> bool {
        true
    }
}

/// The privacy line of an agent program that runs under its own sign-in.
pub(crate) const OWN_SIGN_IN: &str =
    "It uses its own sign-in; OpenXER itself sends nothing off this computer.";

#[cfg(test)]
pub(crate) use fake::{FakeBackend, FakeLog};

#[cfg(test)]
mod fake {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    /// Every call the window made, and the events still to come.
    #[derive(Default, Debug)]
    pub(crate) struct FakeLog {
        pub agents: Vec<AgentInfo>,
        pub conversations: Vec<String>,
        /// Messages sent: text, mode, context, model.
        pub sent: Vec<(String, Mode, ChatContext, ModelChoice)>,
        pub stops: usize,
        pub applied: Vec<u64>,
        pub discarded: Vec<u64>,
        pub sign_ins: Vec<String>,
        pub cancels: Vec<String>,
        pub sign_outs: Vec<String>,
        pub ends: usize,
        /// What the agent does for each message, in order: one batch of
        /// events per poll.
        pub replies: VecDeque<Vec<Vec<AgentEvent>>>,
        /// Batches still to hand out, one per poll.
        pub queue: VecDeque<Vec<AgentEvent>>,
        /// Apply fails with this message.
        pub apply_error: Option<String>,
        /// When set, Apply succeeds with `Applied::Project(project.clone(), status)`.
        pub apply_status: Option<String>,
    }

    /// A scripted agent layer. The test keeps a handle on its log to push
    /// events and read the calls.
    pub(crate) struct FakeBackend {
        pub log: Arc<Mutex<FakeLog>>,
        /// Check Setup's rows: a fixed mixed run.
        pub checks: Option<super::super::checks::Checks>,
    }

    impl FakeBackend {
        /// Claude Code ready, Codex not installed, ChatGPT needing a
        /// sign-in (with its consent and two models).
        pub(crate) fn new() -> (Self, Arc<Mutex<FakeLog>>) {
            let log = Arc::new(Mutex::new(FakeLog {
                agents: vec![
                    AgentInfo {
                        id: "claude-code".into(),
                        name: "Claude Code".into(),
                        state: AgentState::Ready,
                        privacy: OWN_SIGN_IN.into(),
                        consent: None,
                        can_sign_out: false,
                        models: vec![],
                        lower_cost: None,
                        setup: "Runs under your Claude account.".into(),
                        install_url: None,
                        plan: false,
                        notice: None,
                        configurable: false,
                        ask_only: false,
                        use_consent: None,
                    },
                    AgentInfo {
                        id: "codex".into(),
                        name: "Codex".into(),
                        state: AgentState::NotInstalled,
                        privacy: OWN_SIGN_IN.into(),
                        consent: None,
                        can_sign_out: false,
                        models: vec![],
                        lower_cost: None,
                        setup: "Needs the Codex app.".into(),
                        install_url: Some("https://example.test/codex".into()),
                        plan: false,
                        notice: None,
                        configurable: false,
                        ask_only: false,
                        use_consent: None,
                    },
                    AgentInfo {
                        id: "chatgpt".into(),
                        name: "ChatGPT".into(),
                        state: AgentState::SignInNeeded,
                        privacy: "Your questions and the schedule data it reads go to OpenAI under your ChatGPT account.".into(),
                        consent: Some(openxer_chat::CONSENT_TEXT.into()),
                        can_sign_out: true,
                        models: vec!["large-model".into(), "small-model".into()],
                        lower_cost: None,
                        setup: "Use your ChatGPT plan.".into(),
                        install_url: None,
                        plan: true,
                        notice: None,
                        configurable: false,
                        ask_only: false,
                        use_consent: None,
                    },
                ],
                ..Default::default()
            }));
            (
                Self {
                    log: log.clone(),
                    checks: None,
                },
                log,
            )
        }

        fn set_state(&self, agent: &str, state: AgentState) {
            let mut log = self.log.lock().unwrap();
            if let Some(a) = log.agents.iter_mut().find(|a| a.id == agent) {
                a.state = state;
            }
        }
    }

    impl FakeLog {
        /// The agent finishes signing in as `account`.
        pub(crate) fn finish_sign_in(&mut self, agent: &str, account: &str) {
            if let Some(a) = self.agents.iter_mut().find(|a| a.id == agent) {
                a.state = AgentState::SignedIn {
                    account: account.into(),
                };
            }
        }
    }

    impl ChatBackend for FakeBackend {
        fn agents(&self) -> Vec<AgentInfo> {
            self.log.lock().unwrap().agents.clone()
        }
        fn new_conversation(&mut self, agent: &str) {
            let mut log = self.log.lock().unwrap();
            log.conversations.push(agent.into());
            log.queue.clear();
        }
        fn send(&mut self, turn: Turn) {
            let mut log = self.log.lock().unwrap();
            log.sent.push((
                turn.text.into(),
                turn.mode,
                turn.context.clone(),
                turn.model.clone(),
            ));
            if let Some(batches) = log.replies.pop_front() {
                log.queue.extend(batches);
            }
        }
        fn poll(&mut self) -> Vec<AgentEvent> {
            self.log
                .lock()
                .unwrap()
                .queue
                .pop_front()
                .unwrap_or_default()
        }
        fn stop(&mut self) {
            let mut log = self.log.lock().unwrap();
            log.stops += 1;
            log.queue.clear();
        }
        fn apply(
            &mut self,
            proposal: u64,
            project: &ProjectData,
            _: u64,
        ) -> Result<Applied, String> {
            let mut log = self.log.lock().unwrap();
            log.applied.push(proposal);
            if let Some(e) = log.apply_error.clone() {
                return Err(e);
            }
            if let Some(status) = log.apply_status.clone() {
                return Ok(Applied::Project(Box::new(project.clone()), status));
            }
            Ok(Applied::Nothing)
        }
        fn discard(&mut self, proposal: u64) {
            self.log.lock().unwrap().discarded.push(proposal);
        }
        fn sign_in(&mut self, agent: &str) {
            self.log.lock().unwrap().sign_ins.push(agent.into());
            self.set_state(agent, AgentState::SigningIn);
        }
        fn cancel_sign_in(&mut self, agent: &str) {
            self.log.lock().unwrap().cancels.push(agent.into());
            self.set_state(agent, AgentState::SignInNeeded);
        }
        fn sign_out(&mut self, agent: &str) {
            self.log.lock().unwrap().sign_outs.push(agent.into());
            self.set_state(agent, AgentState::SignInNeeded);
        }
        fn end(&mut self) {
            self.log.lock().unwrap().ends += 1;
        }
        fn check_setup(&mut self, restart: bool) -> Option<&mut super::super::checks::Checks> {
            if restart || self.checks.is_none() {
                self.checks = Some(super::super::checks::Checks::sample());
            }
            self.checks.as_mut()
        }
    }
}
