//! The fourth route: an OpenAI-compatible server with the user's API key,
//! or a model running on this PC (`openxer_chat::compat`). It is a second
//! instance of the direct route ([`super::Direct`]): the same
//! conversation, snapshot, context, tools, proposals and Apply as ChatGPT.
//! One connection is kept; its plain settings live in the user's
//! preferences (the window), its key only in the credential store.
use super::*;
use openxer_chat::compat::{self as cc, CompatClient, Connection};

/// The agent id of this route.
pub(crate) const ENDPOINT: &str = "endpoint";
/// How the route is named before a connection is set up.
pub(crate) const ENDPOINT_NAME: &str = "API key or local model";
/// What the setup list says it needs.
pub(crate) const ENDPOINT_SETUP: &str = "Your own API key (OpenAI, OpenRouter and others) or a model on this PC. Use counts against that provider's account.";
/// The privacy line before a connection is set up.
const ENDPOINT_PRIVACY: &str = "With an API key, OpenXER sends your questions and the schedule data its tools read to that provider; with a model on this PC, nothing leaves this computer.";

/// The connection and its model list.
#[derive(Default)]
pub(crate) struct Endpoint {
    pub conn: Option<Connection>,
    pub models: Vec<oc::ModelInfo>,
    models_rx: Option<Receiver<Result<Vec<oc::ModelInfo>, oc::ChatError>>>,
    /// The connection the model list was asked for.
    models_for: Option<String>,
    pub note: Option<String>,
    /// Whether a key that may go to the connection's address is stored,
    /// as last read on a worker thread (`None`: not known yet). Frames
    /// never read the credential store.
    pub key_present: Option<bool>,
    pub(crate) key_rx: Option<Receiver<bool>>,
    /// A save or key removal running on a worker thread.
    pub(crate) saving: Option<Receiver<bool>>,
}

/// The endpoint route's turns.
struct Compat(Arc<CompatClient>);

impl Direct for Compat {
    fn start_turn(
        &self,
        history: oc::Conversation,
        request: oc::TurnRequest,
        tools: Arc<dyn oc::Tools>,
    ) -> oc::TurnHandle {
        self.0.start_turn(history, request, tools)
    }
}

impl LiveBackend {
    fn endpoint_key_present(&self, _conn: &Connection) -> bool {
        self.endpoint.key_present == Some(true)
    }

    /// Reads on a worker thread whether the connection's key is stored.
    pub(super) fn refresh_endpoint_key(&mut self) {
        let Some(conn) = self.endpoint.conn.clone() else {
            self.endpoint.key_present = Some(false);
            return;
        };
        let (send, receive) = mpsc::channel();
        let keys = self.config.keys.clone();
        std::thread::spawn(move || {
            let _ = send.send(cc::has_key(keys.as_ref(), &conn).unwrap_or(false));
        });
        self.endpoint.key_rx = Some(receive);
    }

    /// Whether the connection can be used now: an allowed address, a model,
    /// and a key when it needs one.
    fn endpoint_problem(&self, conn: &Connection) -> Option<String> {
        if let Err(e) = conn.address() {
            return Some(format!("The address cannot be used: {e}."));
        }
        if self.endpoint.saving.is_some() {
            return Some("Saving...".into());
        }
        if conn.needs_key && self.endpoint.key_present.is_none() {
            return Some("Checking the stored key...".into());
        }
        if conn.needs_key && !self.endpoint_key_present(conn) {
            return Some("It needs an API key: choose Set Up... and enter one.".into());
        }
        if conn.model.trim().is_empty() {
            return Some("No model is chosen: choose Set Up... and pick one.".into());
        }
        None
    }

    pub(super) fn endpoint_info(&self) -> AgentInfo {
        let e = &self.endpoint;
        let Some(conn) = &e.conn else {
            return AgentInfo {
                id: ENDPOINT.into(),
                name: ENDPOINT_NAME.into(),
                state: AgentState::NotSetUp,
                privacy: ENDPOINT_PRIVACY.into(),
                consent: None,
                can_sign_out: false,
                models: vec![],
                lower_cost: Some("Lower cost (the same model)".into()),
                setup: ENDPOINT_SETUP.into(),
                install_url: None,
                plan: false,
                notice: None,
                configurable: true,
                ask_only: false,
                use_consent: None,
            };
        };
        let problem = self.endpoint_problem(conn);
        let notes: Vec<String> = [
            problem.clone(),
            e.note.clone(),
            conn.ask_only().then(|| cc::ASK_ONLY_NOTE.to_string()),
            conn.preset.info().note.map(str::to_string),
        ]
        .into_iter()
        .flatten()
        .collect();
        AgentInfo {
            id: ENDPOINT.into(),
            name: format!("{} ({})", conn.name, conn.host()),
            state: if problem.is_some() {
                AgentState::NotSetUp
            } else {
                AgentState::Ready
            },
            privacy: cc::privacy_text(conn),
            consent: None,
            can_sign_out: false,
            models: e.models.iter().map(|m| m.display_name.clone()).collect(),
            lower_cost: Some("Lower cost (the same model)".into()),
            setup: ENDPOINT_SETUP.into(),
            install_url: None,
            plan: false,
            notice: (!notes.is_empty()).then(|| notes.join(" ")),
            configurable: true,
            ask_only: conn.ask_only(),
            // A service on another computer: asked once per host.
            use_consent: (!conn.is_local()).then(|| {
                (
                    format!("chat_consent_endpoint_{}", conn.host()),
                    cc::privacy_text(conn),
                )
            }),
        }
    }

    /// Asks the server for its models once per connection (and again after
    /// a failure, when the connection is saved again).
    pub(super) fn poll_endpoint(&mut self) {
        for rx in [&mut self.endpoint.key_rx, &mut self.endpoint.saving] {
            if let Some(r) = rx {
                match r.try_recv() {
                    Ok(present) => {
                        self.endpoint.key_present = Some(present);
                        *rx = None;
                    }
                    Err(TryRecvError::Disconnected) => *rx = None,
                    Err(TryRecvError::Empty) => {}
                }
            }
        }
        if let Some(rx) = &self.endpoint.models_rx {
            match rx.try_recv() {
                Ok(Ok(models)) => {
                    self.endpoint.models = models;
                    self.endpoint.models_rx = None;
                }
                Ok(Err(e)) => {
                    self.endpoint.note = Some(format!("The model list did not load: {e}"));
                    self.endpoint.models_rx = None;
                }
                Err(TryRecvError::Disconnected) => self.endpoint.models_rx = None,
                Err(TryRecvError::Empty) => {}
            }
        }
        let Some(conn) = self.endpoint.conn.clone() else {
            return;
        };
        let key = conn.to_json().to_string();
        if self.endpoint.models_for.as_ref() == Some(&key)
            || conn.address().is_err()
            || self.endpoint.saving.is_some()
            || (conn.needs_key && !self.endpoint_key_present(&conn))
        {
            return;
        }
        self.endpoint.models_for = Some(key);
        let (send, receive) = mpsc::channel();
        let store = self.config.keys.clone();
        std::thread::spawn(move || {
            let result =
                CompatClient::new(conn, store).and_then(|c| c.list_models(&oc::CancelToken::new()));
            let _ = send.send(result);
        });
        self.endpoint.models_rx = Some(receive);
    }

    /// A message on the endpoint route.
    pub(super) fn send_endpoint(&mut self, turn: &Turn, project: &ProjectData) {
        let Some(conn) = self.endpoint.conn.clone() else {
            self.out.push(error(
                ErrorKind::Failed,
                "No API key or local model is set up. Choose it in the setup list and Set Up....",
            ));
            return;
        };
        let model = match turn.model {
            ModelChoice::Named(name) => self
                .endpoint
                .models
                .iter()
                .find(|m| &m.display_name == name || &m.slug == name)
                .map(|m| m.slug.clone())
                .unwrap_or_else(|| name.clone()),
            // Empty: the connection's own model.
            _ => String::new(),
        };
        if model.is_empty() {
            if let Some(problem) = self.endpoint_problem(&conn) {
                self.out.push(error(ErrorKind::Failed, problem));
                return;
            }
        }
        let client = match CompatClient::new(conn.clone(), self.config.keys.clone()) {
            Ok(c) => Arc::new(c),
            Err(e) => {
                self.out.push(direct_error(&e));
                return;
            }
        };
        if conn.ask_only() && self.conv.as_ref().is_some_and(|c| c.sent == 0) {
            self.out.push(AgentEvent::Notice(cc::ASK_ONLY_NOTE.into()));
        }
        let provider: Arc<dyn Direct> = Arc::new(Compat(client));
        self.send_direct(turn, project, provider, model, None, move || {
            conn.new_conversation()
        });
    }

    /// Uses `conn` from now on. On a worker thread: the old connection's
    /// key goes when the address changed (a key is never sent to another
    /// host), and a typed key is stored for the new address. The receiver
    /// says when that is done, or why it failed.
    pub(super) fn save_endpoint(
        &mut self,
        conn: Option<Connection>,
        key: Option<oc::Secret>,
    ) -> Receiver<Result<(), String>> {
        if self.conv.as_ref().is_some_and(|c| c.agent == ENDPOINT) {
            self.end_conv();
        }
        let old = self.endpoint.conn.take();
        self.endpoint = Endpoint {
            conn: conn.clone(),
            ..Endpoint::default()
        };
        let keys = self.config.keys.clone();
        let (done, result) = mpsc::channel();
        let (present, receive) = mpsc::channel();
        std::thread::spawn(move || {
            let mut outcome = Ok(());
            if let Some(old) = &old {
                let moved = conn
                    .as_ref()
                    .is_none_or(|c| c.id != old.id || cc::key_origin(c) != cc::key_origin(old));
                if moved {
                    let _ = cc::delete_key(keys.as_ref(), old);
                }
            }
            if let (Some(conn), Some(key)) = (&conn, &key) {
                if !key.expose().trim().is_empty() {
                    if let Err(e) = cc::store_key(keys.as_ref(), conn, key) {
                        outcome = Err(format!("The key was not stored: {e}"));
                    }
                }
            }
            let has = conn
                .as_ref()
                .is_some_and(|c| cc::has_key(keys.as_ref(), c).unwrap_or(false));
            let _ = present.send(has);
            let _ = done.send(outcome);
        });
        self.endpoint.saving = Some(receive);
        result
    }

    /// Removes the connection's stored key, on a worker thread.
    pub(super) fn remove_endpoint_key(&mut self) -> Receiver<Result<(), String>> {
        let conn = self.endpoint.conn.clone();
        let keys = self.config.keys.clone();
        let (done, result) = mpsc::channel();
        let (present, receive) = mpsc::channel();
        std::thread::spawn(move || {
            let outcome = match &conn {
                Some(c) => cc::delete_key(keys.as_ref(), c)
                    .map_err(|e| format!("The key was not removed: {e}")),
                None => Ok(()),
            };
            let _ = present.send(false);
            let _ = done.send(outcome);
        });
        self.endpoint.saving = Some(receive);
        if self.conv.as_ref().is_some_and(|c| c.agent == ENDPOINT) {
            self.end_conv();
        }
        result
    }

    /// Tests `conn` with a typed key (kept in memory only) or the stored one.
    pub(super) fn test_endpoint(
        &mut self,
        conn: Connection,
        key: Option<oc::Secret>,
    ) -> Receiver<EndpointCheck> {
        let typed = key.filter(|k| !k.expose().trim().is_empty());
        let store: Arc<dyn oc::CredentialStore> = match typed {
            Some(k) => {
                let memory = Arc::new(oc::MemoryStore::new());
                let _ = cc::store_key(memory.as_ref(), &conn, &k);
                memory
            }
            None => self.config.keys.clone(),
        };
        let (send, receive) = mpsc::channel();
        std::thread::spawn(move || {
            let outcome = cc::test_connection(&conn, store.as_ref(), &oc::CancelToken::new());
            let ok = matches!(outcome, cc::TestOutcome::Ok { .. });
            let models = if ok {
                CompatClient::new(conn, store)
                    .and_then(|c| c.list_models(&oc::CancelToken::new()))
                    .map(|m| m.into_iter().map(|m| m.slug).collect())
                    .unwrap_or_default()
            } else {
                vec![]
            };
            let _ = send.send(EndpointCheck {
                line: outcome.to_string(),
                ok,
                models,
            });
        });
        receive
    }
}

/// The Check Setup rows of the endpoint route: set up, reachable with the
/// key accepted, the model there.
pub(crate) fn check_rows(
    conn: Option<&Connection>,
    keys: &Arc<dyn oc::CredentialStore>,
    cancel: &oc::CancelToken,
) -> Vec<super::super::checks::Row> {
    use super::super::checks::{Fix, Mark, Row};
    let row = |mark, text: String, fix: Option<Fix>| Row {
        mark,
        area: ENDPOINT_NAME.into(),
        text,
        fix,
    };
    let set_up = || Some(Fix::SetUp);
    let Some(conn) = conn else {
        return vec![row(Mark::Note, "Not set up.".into(), set_up())];
    };
    let mut out = vec![row(
        Mark::Pass,
        format!("Set up: {} at {}.", conn.name, conn.host()),
        None,
    )];
    let outcome = cc::test_connection(conn, keys.as_ref(), cancel);
    match outcome {
        cc::TestOutcome::Ok { models } => out.push(row(
            Mark::Pass,
            if conn.needs_key {
                format!(
                    "Reaches {} and the key is accepted ({models} models).",
                    conn.host()
                )
            } else {
                format!("Reaches {} ({models} models).", conn.host())
            },
            None,
        )),
        other => {
            out.push(row(Mark::Fail, other.to_string(), set_up()));
            return out;
        }
    }
    let listed: Vec<String> = CompatClient::new(conn.clone(), keys.clone())
        .and_then(|c| c.list_models(cancel))
        .map(|m| m.into_iter().map(|m| m.slug).collect())
        .unwrap_or_default();
    out.push(if conn.model.trim().is_empty() {
        row(Mark::Fail, "No model is chosen.".into(), set_up())
    } else if listed.is_empty() || listed.iter().any(|m| m == &conn.model) {
        row(
            Mark::Pass,
            format!(
                "Model {} is set{}.",
                conn.model,
                if listed.is_empty() { "" } else { " and listed" }
            ),
            None,
        )
    } else {
        row(
            Mark::Fail,
            format!("Model {} is not in the server's list.", conn.model),
            set_up(),
        )
    });
    out
}
