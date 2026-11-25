//! The fourth way to connect Chat: any server that speaks the OpenAI Chat
//! Completions protocol, with the user's own API key (OpenAI, OpenRouter,
//! Anthropic's compatibility layer) or a model running on this PC (Ollama,
//! LM Studio). It reuses the HTTP client, the SSE parser, the conversation
//! budgets, the [`Tools`] trait and [`ChatEvent`], so the window treats it
//! like the ChatGPT route.
//!
//! - [`Preset`] and [`presets`]: the known servers, with their documented
//!   base URL and whether a key is needed.
//! - [`Connection`]: one named connection (plain data for user
//!   preferences, see [`Connection::to_json`]); its key is kept apart, in
//!   the [`CredentialStore`] ([`store_key`], [`delete_key`]).
//! - [`CompatClient`]: model list and streamed turns.
//! - [`test_connection`]: the check for the settings dialog and Check Setup.
//! - [`privacy_text`], [`KEY_CAUTION`]: what the dialog shows.
//!
//! Rules: plain `http` only for this computer (`127.0.0.1`, `::1`,
//! `localhost`), `https` everywhere else, checked before any request;
//! redirects are never followed, so the key only ever goes to the
//! configured host.
//!
//! Retry policy: 401 and 403 end the turn (wrong key, no retry); 404 ends
//! it (wrong address or model); 400 or 422 while `stream_options` is sent
//! is retried once without it (servers that do not know the field); 429,
//! 5xx and network failures before the reply are retried with the
//! configured waits (a `Retry-After` of up to 20 s is honored); nothing
//! answering at a local address ends the turn at once with a hint.

use crate::client::{
    api_error, run_tool, ChatError, ChatEvent, ModelInfo, TurnHandle, TurnRequest,
};
use crate::conversation::{Budgets, Conversation, Entry};
use crate::http::{self, HttpConfig, HttpError, Url};
use crate::sse::SseParser;
use crate::store::{CredentialStore, Item, KeyId, StoreError};
use crate::stream::Usage;
use crate::tools::Tools;
use crate::util::{self, CancelToken, Secret};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fmt;
use std::net::IpAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;
use std::time::Duration;

/// A known kind of server.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Preset {
    OpenAi,
    OpenRouter,
    Anthropic,
    Ollama,
    LmStudio,
    Custom,
}

/// What a preset brings: its documented base URL and how it is used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PresetInfo {
    pub preset: Preset,
    /// Stable ID for preferences.
    pub id: &'static str,
    pub label: &'static str,
    /// The documented base URL (empty for Custom).
    pub base_url: &'static str,
    pub needs_key: bool,
    /// Runs on this PC.
    pub local: bool,
    /// Small local models may answer badly or call tools wrongly: the
    /// window should default to Ask only and say so in one line.
    pub ask_only: bool,
    /// What to start when nothing answers at the address.
    pub start_hint: &'static str,
    /// A note from the provider's own documentation, if any.
    pub note: Option<&'static str>,
}

const PRESETS: [PresetInfo; 6] = [
    PresetInfo {
        preset: Preset::OpenAi,
        id: "openai",
        label: "OpenAI",
        base_url: "https://api.openai.com/v1",
        needs_key: true,
        local: false,
        ask_only: false,
        start_hint: "Check the internet connection.",
        note: None,
    },
    PresetInfo {
        preset: Preset::OpenRouter,
        id: "openrouter",
        label: "OpenRouter",
        base_url: "https://openrouter.ai/api/v1",
        needs_key: true,
        local: false,
        ask_only: false,
        start_hint: "Check the internet connection.",
        note: None,
    },
    PresetInfo {
        preset: Preset::Anthropic,
        id: "anthropic",
        label: "Anthropic (OpenAI-compatible layer)",
        base_url: "https://api.anthropic.com/v1",
        needs_key: true,
        local: false,
        ask_only: false,
        start_hint: "Check the internet connection.",
        note: Some(
            "Anthropic describes this compatibility layer as meant for testing and comparing \
models, not as a long-term or production solution.",
        ),
    },
    PresetInfo {
        preset: Preset::Ollama,
        id: "ollama",
        label: "Ollama (on this PC)",
        base_url: "http://127.0.0.1:11434/v1",
        needs_key: false,
        local: true,
        ask_only: true,
        start_hint:
            "Start Ollama on this PC (open the Ollama app or run \"ollama serve\"), then try again.",
        note: None,
    },
    PresetInfo {
        preset: Preset::LmStudio,
        id: "lmstudio",
        label: "LM Studio (on this PC)",
        base_url: "http://127.0.0.1:1234/v1",
        needs_key: false,
        local: true,
        ask_only: true,
        start_hint: "Start the local server in LM Studio and load a model, then try again.",
        note: None,
    },
    PresetInfo {
        preset: Preset::Custom,
        id: "custom",
        label: "Other compatible server",
        base_url: "",
        needs_key: true,
        local: false,
        ask_only: false,
        start_hint: "Start the server at that address, then try again.",
        note: None,
    },
];

/// All presets, in the order a dialog lists them.
pub fn presets() -> &'static [PresetInfo] {
    &PRESETS
}

impl Preset {
    pub fn info(self) -> &'static PresetInfo {
        PRESETS
            .iter()
            .find(|p| p.preset == self)
            .expect("every preset is listed")
    }
    pub fn id(self) -> &'static str {
        self.info().id
    }
    pub fn from_id(id: &str) -> Option<Preset> {
        PRESETS.iter().find(|p| p.id == id).map(|p| p.preset)
    }
}

/// The caution line for the connection dialog.
pub const KEY_CAUTION: &str =
    "Some providers' flat-rate coding plans allow their keys only in the \
tools they list. A pay-as-you-go API key is the safe choice here.";

/// The line the window shows for a connection whose answers may be
/// unreliable (see [`Connection::ask_only`]).
pub const ASK_ONLY_NOTE: &str =
    "Small local models can answer wrongly or misuse tools, so this connection starts in Ask only. Check any change it proposes carefully.";

/// Check a base URL: `http` only for this computer, `https` otherwise, no
/// user name or password, no query or fragment. Returns the URL without a
/// trailing slash.
pub fn check_address(base: &str) -> Result<Url, String> {
    let s = base.trim();
    if s.is_empty() {
        return Err("no address is set".into());
    }
    if s.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err("the address has spaces or control characters".into());
    }
    let (scheme, rest) = s
        .split_once("://")
        .ok_or_else(|| "the address must start with http:// or https://".to_string())?;
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return Err("the address must start with http:// or https://".into());
    }
    if rest.contains('?') || rest.contains('#') {
        return Err("the address must not have a query or fragment".into());
    }
    let authority = rest.split('/').next().unwrap_or("");
    if authority.contains('@') {
        return Err("a user name or password in the address is not allowed".into());
    }
    if authority.contains('\\') || authority.contains('%') {
        return Err("the host name has characters that are not allowed".into());
    }
    let mut url = Url::parse(&format!("{scheme}://{rest}"))?;
    url.host = url.host.to_ascii_lowercase();
    if url.host.is_empty() {
        return Err("the address has no host".into());
    }
    let trimmed = url.path.trim_end_matches('/').to_string();
    url.path = if trimmed.is_empty() {
        String::new()
    } else {
        trimmed
    };
    if !url.secure && !is_loopback(&url.host) {
        return Err(
            "a server that is not on this computer must use https:// (plain http is only for 127.0.0.1, ::1 and localhost)"
                .into(),
        );
    }
    Ok(url)
}

/// Whether `host` is this computer: `localhost`, or a loopback IP
/// (127.0.0.0/8, ::1). A name that only starts like one
/// (`127.0.0.1.example`) is not.
pub fn is_loopback(host: &str) -> bool {
    let h = host.trim_end_matches('.').to_ascii_lowercase();
    if h == "localhost" {
        return true;
    }
    h.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

/// One named connection. Plain data: the desktop keeps it in user
/// preferences ([`Connection::to_json`]); the key is stored apart.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Connection {
    /// Stable ID; names the stored key ([`Item::ApiKey`]).
    pub id: String,
    pub name: String,
    pub preset: Preset,
    pub base_url: String,
    /// The model name sent; may be empty until one is picked or typed.
    pub model: String,
    pub needs_key: bool,
}

impl Connection {
    /// A new connection from a preset, with a fresh ID.
    pub fn new(preset: Preset) -> Connection {
        let info = preset.info();
        Connection {
            id: format!("c{}", util::random_token(9)),
            name: info.label.to_string(),
            preset,
            base_url: info.base_url.to_string(),
            model: String::new(),
            needs_key: info.needs_key,
        }
    }

    /// The checked base URL.
    pub fn address(&self) -> Result<Url, String> {
        check_address(&self.base_url)
    }

    /// Whether the address is this computer (false when it is invalid).
    pub fn is_local(&self) -> bool {
        self.address().is_ok_and(|u| is_loopback(&u.host))
    }

    /// The host shown to the user.
    pub fn host(&self) -> String {
        self.address()
            .map(|u| u.host.clone())
            .unwrap_or_else(|_| self.base_url.trim().to_string())
    }

    /// Whether the window should default this connection to Ask only
    /// ("answers may be unreliable"): the local presets, and any server on
    /// this computer.
    pub fn ask_only(&self) -> bool {
        self.preset.info().ask_only || self.is_local()
    }

    /// The conversation budgets for this connection: smaller for a server
    /// on this computer, where models usually have small context windows.
    pub fn budgets(&self) -> Budgets {
        if self.is_local() || self.preset.info().local {
            local_budgets()
        } else {
            Budgets::default()
        }
    }

    /// A new conversation with [`Connection::budgets`].
    pub fn new_conversation(&self) -> Conversation {
        Conversation::new().with_budgets(self.budgets())
    }

    /// The stored key's item, or `None` when the ID is unusable.
    pub fn key_item(&self) -> Option<Item> {
        KeyId::new(&self.id).map(Item::ApiKey)
    }

    /// Plain data for user preferences (no key).
    pub fn to_json(&self) -> Value {
        json!({"id": self.id, "name": self.name, "preset": self.preset.id(),
            "base_url": self.base_url, "model": self.model, "needs_key": self.needs_key})
    }

    pub fn from_json(v: &Value) -> Option<Connection> {
        let preset = Preset::from_id(v["preset"].as_str()?)?;
        let id = v["id"].as_str()?.to_string();
        KeyId::new(&id)?;
        Some(Connection {
            id,
            name: v["name"]
                .as_str()
                .unwrap_or(preset.info().label)
                .to_string(),
            preset,
            base_url: v["base_url"].as_str().unwrap_or("").to_string(),
            model: v["model"].as_str().unwrap_or("").to_string(),
            needs_key: v["needs_key"].as_bool().unwrap_or(preset.info().needs_key),
        })
    }
}

/// The budgets for a model on this computer: a much smaller history and
/// smaller tool results than [`Budgets::default`]. Pinned by tests.
pub fn local_budgets() -> Budgets {
    Budgets {
        max_tool_result_chars: 3_000,
        old_result_keep_chars: 200,
        summary_quote_chars: 80,
        max_rounds: 5,
        max_history_chars: 12_000,
        max_context_chars: 1_500,
    }
}

/// The short consent or notice text the connection dialog shows.
pub fn privacy_text(conn: &Connection) -> String {
    match conn.address() {
        Ok(u) if is_loopback(&u.host) => format!(
            "This connection runs on this PC. Nothing is sent off this computer by OpenXER: \
your questions and the schedule data the tools read go only to the program answering at {}.",
            conn.base_url.trim()
        ),
        Ok(u) => format!(
            "With this connection OpenXER sends your chat questions, and the schedule data its \
tools read to answer them (for example activity IDs, names, dates, logic, resources and costs \
from the open project), from this computer to {} under your API key. That provider's terms and \
privacy policy apply, and the use may be billed to your account. Remove the key in the \
connection settings to stop.",
            u.host
        ),
        Err(e) => format!("This connection cannot be used: {e}."),
    }
}

fn key_item(conn: &Connection) -> Result<Item, StoreError> {
    conn.key_item()
        .ok_or_else(|| StoreError("the connection ID cannot name a stored key".into()))
}

/// The start of a stored key's value, before the address it is bound to.
const KEY_BINDING: &str = "openxer-key-v1 ";

/// Where a key of `conn` may be sent: the scheme, host and port of its
/// address (`None` when the address cannot be used).
pub fn key_origin(conn: &Connection) -> Option<String> {
    conn.address().ok().map(|u| {
        format!(
            "{}://{}:{}",
            if u.secure { "https" } else { "http" },
            u.host.to_ascii_lowercase(),
            u.port
        )
    })
}

/// Store the API key of `conn` (Windows Credential Manager in the app),
/// bound to the address it is entered for: it is never sent anywhere else.
pub fn store_key(
    store: &dyn CredentialStore,
    conn: &Connection,
    key: &Secret,
) -> Result<(), StoreError> {
    let origin = key_origin(conn).ok_or_else(|| StoreError("the address cannot be used".into()))?;
    let bound = Secret::new(format!("{KEY_BINDING}{origin}\n{}", key.expose().trim()));
    store.write(key_item(conn)?, &bound)
}

/// The key stored for `conn`, only when the connection needs one and the
/// key was entered for exactly its address (scheme, host and port).
pub fn stored_key(
    store: &dyn CredentialStore,
    conn: &Connection,
) -> Result<Option<Secret>, StoreError> {
    if !conn.needs_key {
        return Ok(None);
    }
    let Some(origin) = key_origin(conn) else {
        return Ok(None);
    };
    let Some(raw) = store.read(key_item(conn)?)? else {
        return Ok(None);
    };
    let key = raw
        .expose()
        .strip_prefix(KEY_BINDING)
        .and_then(|rest| rest.split_once('\n'))
        .filter(|(bound, key)| *bound == origin && !key.is_empty())
        .map(|(_, key)| Secret::new(key));
    Ok(key)
}

/// Delete the API key of `conn`.
pub fn delete_key(store: &dyn CredentialStore, conn: &Connection) -> Result<(), StoreError> {
    store.delete(key_item(conn)?)
}

/// Whether a key is stored for `conn` that may be sent to its address.
pub fn has_key(store: &dyn CredentialStore, conn: &Connection) -> Result<bool, StoreError> {
    Ok(stored_key(store, conn)?.is_some())
}

/// Timeouts and limits for a [`CompatClient`].
#[derive(Clone, Debug)]
pub struct CompatConfig {
    pub http: HttpConfig,
    pub retry_delays: Vec<Duration>,
    pub max_retry_after: Duration,
    pub max_event_bytes: usize,
    pub max_stream_bytes: usize,
}

impl Default for CompatConfig {
    fn default() -> Self {
        CompatConfig {
            http: HttpConfig {
                connect_timeout: Duration::from_secs(8),
                // Local models can be slow to start answering.
                read_timeout: Duration::from_secs(300),
            },
            retry_delays: vec![Duration::from_secs(1), Duration::from_secs(4)],
            max_retry_after: Duration::from_secs(20),
            max_event_bytes: 2 << 20,
            max_stream_bytes: 32 << 20,
        }
    }
}

/// Chat over one compatible connection.
pub struct CompatClient {
    conn: Connection,
    url: Url,
    store: Arc<dyn CredentialStore>,
    config: CompatConfig,
    /// The server refused `stream_options`; left out from then on.
    no_stream_options: AtomicBool,
}

impl fmt::Debug for CompatClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CompatClient({:?})", self.url)
    }
}

fn net(e: HttpError, conn: &Connection, url: &Url) -> ChatError {
    match e {
        HttpError::Cancelled => ChatError::Cancelled,
        HttpError::Proxy => ChatError::ProxyNotSupported,
        HttpError::Connect(_) if is_loopback(&url.host) => ChatError::ServerNotRunning {
            hint: conn.preset.info().start_hint.to_string(),
        },
        e => ChatError::Network(e.to_string()),
    }
}

/// A server message with the key taken out, in case a server echoes it.
fn scrub(message: &str, key: Option<&Secret>) -> String {
    match key {
        Some(k) if !k.is_empty() => message.replace(k.expose(), "<redacted>"),
        _ => message.to_string(),
    }
}

enum Attempt {
    Done(ChatDecoder),
    Retry(Option<Duration>, ChatError),
    DropStreamOptions,
}

impl CompatClient {
    /// Fails when the address is not allowed.
    pub fn new(
        conn: Connection,
        store: Arc<dyn CredentialStore>,
    ) -> Result<CompatClient, ChatError> {
        CompatClient::with_config(conn, store, CompatConfig::default())
    }

    pub fn with_config(
        conn: Connection,
        store: Arc<dyn CredentialStore>,
        config: CompatConfig,
    ) -> Result<CompatClient, ChatError> {
        let url = conn.address().map_err(ChatError::AddressNotAllowed)?;
        Ok(CompatClient {
            conn,
            url,
            store,
            config,
            no_stream_options: AtomicBool::new(false),
        })
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    /// The stored key ([`stored_key`]: bound to this address, and only for
    /// a connection that needs one); `NoKey` when it needs one and none
    /// may be sent.
    fn key(&self) -> Result<Option<Secret>, ChatError> {
        let key = stored_key(self.store.as_ref(), &self.conn)
            .map_err(|e| ChatError::Network(e.to_string()))?;
        if key.is_none() && self.conn.needs_key {
            return Err(ChatError::NoKey);
        }
        Ok(key)
    }

    fn auth_header(key: &Option<Secret>) -> Option<Secret> {
        key.as_ref()
            .map(|k| Secret::new(format!("Bearer {}", k.expose())))
    }

    /// The models the server lists (`GET {base}/models`), in its order.
    /// A server without a model list (404, 405, 501) gives an empty list:
    /// the user types a model name.
    pub fn list_models(&self, cancel: &CancelToken) -> Result<Vec<ModelInfo>, ChatError> {
        let key = self.key()?;
        list_models_at(&self.conn, &self.url, &key, &self.config.http, cancel)
    }

    /// Start a turn on its own thread, as [`crate::ChatClient::start_turn`].
    pub fn start_turn(
        self: &Arc<Self>,
        mut conversation: Conversation,
        request: TurnRequest,
        tools: Arc<dyn Tools>,
    ) -> TurnHandle {
        let (tx, rx) = mpsc::channel();
        let cancel = CancelToken::new();
        let (me, c) = (self.clone(), cancel.clone());
        let thread = std::thread::spawn(move || {
            let _ = me.run_turn(&mut conversation, &request, &tools, &tx, &c);
            conversation
        });
        TurnHandle {
            events: rx,
            cancel,
            thread,
        }
    }

    /// Run one turn on this thread, as [`crate::ChatClient::run_turn`]:
    /// every event, including the final `TurnFinished`, `Error` or
    /// `Cancelled`, goes to `events`, and a failed turn leaves the
    /// conversation as it was. `request.model` empty means the
    /// connection's model. Usage is reported only when the server gave it
    /// for every request of the turn ([`Usage::is_known`]).
    pub fn run_turn(
        &self,
        conv: &mut Conversation,
        request: &TurnRequest,
        tools: &Arc<dyn Tools>,
        events: &Sender<ChatEvent>,
        cancel: &CancelToken,
    ) -> Result<Usage, ChatError> {
        let before = conv.entries.len();
        let result = self.turn_inner(conv, request, tools, events, cancel);
        match &result {
            Ok(u) => {
                conv.finish_turn();
                let _ = events.send(ChatEvent::TurnFinished(*u));
            }
            Err(ChatError::Cancelled) => {
                conv.entries.truncate(before);
                let _ = events.send(ChatEvent::Cancelled);
            }
            Err(e) => {
                conv.entries.truncate(before);
                let _ = events.send(ChatEvent::Error(e.clone()));
            }
        }
        result
    }

    fn turn_inner(
        &self,
        conv: &mut Conversation,
        request: &TurnRequest,
        tools: &Arc<dyn Tools>,
        events: &Sender<ChatEvent>,
        cancel: &CancelToken,
    ) -> Result<Usage, ChatError> {
        let key = self.key()?;
        let model = if request.model.trim().is_empty() {
            self.conn.model.trim().to_string()
        } else {
            request.model.trim().to_string()
        };
        if model.is_empty() {
            return Err(ChatError::NotFound {
                message: "no model is chosen for this connection".into(),
            });
        }
        let turn = conv.turn;
        conv.push(Entry::User {
            turn,
            text: request.user_text.clone(),
        });
        let mut defs = tools.definitions();
        defs.sort_by(|a, b| a.name.cmp(&b.name));
        let tool_json: Vec<Value> = defs
            .iter()
            .map(|d| {
                json!({"type": "function", "function": {"name": d.name,
                    "description": d.description, "parameters": d.parameters}})
            })
            .collect();
        let instructions = conv.instructions(&request.context);
        let max_rounds = conv.budgets.max_rounds.max(1);
        let mut usage = Usage::default();
        let mut all_reported = true;
        for round in 0..max_rounds {
            let last = round + 1 == max_rounds;
            let mut messages = vec![json!({"role": "system", "content": instructions})];
            messages.extend(conv.chat_messages());
            let mut body = json!({"model": model, "messages": messages, "stream": true});
            // The last round offers no tools, so the model must answer.
            if !tool_json.is_empty() && !last {
                body["tools"] = Value::Array(tool_json.clone());
            }
            let decoder = self.stream(&mut body, &key, events, cancel)?;
            match decoder.usage {
                Some(u) => usage.add(&u),
                None => all_reported = false,
            }
            if !decoder.text.is_empty() {
                conv.push(Entry::Assistant {
                    turn,
                    text: decoder.text.clone(),
                });
            }
            let calls = decoder.calls();
            for (call_id, name, arguments) in &calls {
                conv.push(Entry::Call {
                    turn,
                    call_id: call_id.clone(),
                    name: name.clone(),
                    namespace: None,
                    arguments: arguments.clone(),
                });
            }
            if calls.is_empty() {
                return Ok(if all_reported {
                    usage
                } else {
                    Usage::default()
                });
            }
            if last {
                return Err(ChatError::RoundCap { rounds: max_rounds });
            }
            for (call_id, name, arguments) in calls {
                let (text, summary) =
                    run_tool(tools, &call_id, &name, &arguments, conv, events, cancel)?;
                conv.push(Entry::Output {
                    turn,
                    call_id,
                    text,
                    summary,
                });
            }
        }
        Err(ChatError::RoundCap { rounds: max_rounds })
    }

    fn stream(
        &self,
        body: &mut Value,
        key: &Option<Secret>,
        events: &Sender<ChatEvent>,
        cancel: &CancelToken,
    ) -> Result<ChatDecoder, ChatError> {
        let mut retries = self.config.retry_delays.iter();
        let mut dropped_once = false;
        loop {
            if self.no_stream_options.load(Ordering::SeqCst) {
                if let Some(o) = body.as_object_mut() {
                    o.remove("stream_options");
                }
            } else if !dropped_once {
                body["stream_options"] = json!({"include_usage": true});
            }
            match self.attempt(body, key, events, cancel)? {
                Attempt::Done(d) => return Ok(d),
                Attempt::DropStreamOptions => {
                    dropped_once = true;
                    self.no_stream_options.store(true, Ordering::SeqCst);
                }
                Attempt::Retry(after, err) => {
                    let Some(delay) = retries.next() else {
                        return Err(err);
                    };
                    let wait = after
                        .map(|a| a.min(self.config.max_retry_after))
                        .unwrap_or(*delay);
                    if cancel.sleep(wait) {
                        return Err(ChatError::Cancelled);
                    }
                }
            }
        }
    }

    fn attempt(
        &self,
        body: &Value,
        key: &Option<Secret>,
        events: &Sender<ChatEvent>,
        cancel: &CancelToken,
    ) -> Result<Attempt, ChatError> {
        let url = self.url.join("chat/completions");
        let auth = Self::auth_header(key);
        let mut headers = vec![
            ("Content-Type", "application/json"),
            ("Accept", "text/event-stream, application/json"),
        ];
        if let Some(a) = &auth {
            headers.push(("Authorization", a.expose()));
        }
        let payload = body.to_string();
        let sent = http::send(
            "POST",
            &url,
            &headers,
            payload.as_bytes(),
            &self.config.http,
            cancel,
        );
        let mut resp = match sent {
            Ok(r) => r,
            Err(HttpError::Cancelled) => return Err(ChatError::Cancelled),
            Err(e @ (HttpError::Proxy | HttpError::InvalidRequest(_))) => {
                return Err(net(e, &self.conn, &self.url))
            }
            Err(e @ HttpError::Connect(_)) if is_loopback(&self.url.host) => {
                return Err(net(e, &self.conn, &self.url))
            }
            Err(e) => return Ok(Attempt::Retry(None, net(e, &self.conn, &self.url))),
        };
        let status = resp.status;
        if status != 200 {
            let retry_after = resp
                .header("retry-after")
                .and_then(|v| v.trim().parse::<u64>().ok())
                .map(Duration::from_secs);
            let reply = resp.read_all(1 << 20).unwrap_or_default();
            let (code, message, _) = api_error(status, &reply);
            let message = scrub(&message, key.as_ref());
            return Ok(match status {
                401 | 403 => return Err(ChatError::KeyRejected { status }),
                404 => return Err(ChatError::NotFound { message }),
                400 | 422 if body.get("stream_options").is_some() => Attempt::DropStreamOptions,
                429 | 500..=599 => Attempt::Retry(
                    retry_after,
                    ChatError::Api {
                        status,
                        code,
                        message,
                    },
                ),
                _ => {
                    return Err(ChatError::Api {
                        status,
                        code,
                        message,
                    })
                }
            });
        }
        let streamed = resp
            .header("content-type")
            .is_some_and(|c| c.to_ascii_lowercase().contains("text/event-stream"));
        let mut decoder = ChatDecoder::default();
        if !streamed {
            // A server that answers with one JSON body (no streaming).
            let reply = resp
                .read_all(self.config.max_stream_bytes)
                .map_err(|e| net(e, &self.conn, &self.url))?;
            let v: Value = serde_json::from_slice(&reply)
                .map_err(|_| ChatError::BadResponse("the reply is not JSON".into()))?;
            for emit in decoder.handle_full(&v) {
                let _ = events.send(emit);
            }
            return finish(decoder, key, false, true);
        }
        let mut parser = SseParser::with_max_line(self.config.max_event_bytes);
        let mut buf = vec![0u8; 16 * 1024];
        let mut total = 0usize;
        while !decoder.done {
            if cancel.is_cancelled() {
                return Err(ChatError::Cancelled);
            }
            let n = match resp.read_some(&mut buf) {
                Ok(n) => n,
                Err(HttpError::Cancelled) => return Err(ChatError::Cancelled),
                Err(e) if !decoder.emitted => {
                    return Ok(Attempt::Retry(None, net(e, &self.conn, &self.url)))
                }
                Err(e) => return Err(net(e, &self.conn, &self.url)),
            };
            if n == 0 {
                break;
            }
            total += n;
            if total > self.config.max_stream_bytes {
                return Err(ChatError::BadResponse("the reply was too large".into()));
            }
            let evs = parser.push(&buf[..n]);
            if parser.overflowed() {
                return Err(ChatError::BadResponse(
                    "an event in the reply was too large".into(),
                ));
            }
            for ev in evs {
                for emit in decoder.handle_event(&ev.data) {
                    let _ = events.send(emit);
                }
                if decoder.done {
                    break;
                }
            }
        }
        if let Some(m) = decoder.message() {
            let _ = events.send(ChatEvent::Message(m));
        }
        let cut = parser.has_partial();
        finish(decoder, key, cut, false)
    }
}

fn finish(
    decoder: ChatDecoder,
    key: &Option<Secret>,
    cut: bool,
    whole_body: bool,
) -> Result<Attempt, ChatError> {
    if let Some((code, message)) = &decoder.error {
        return Err(ChatError::Api {
            status: 200,
            code: code.clone(),
            message: scrub(message, key.as_ref()),
        });
    }
    let complete = decoder.saw_done || decoder.finish.is_some() || whole_body;
    if !complete {
        if cut {
            return Err(ChatError::BadResponse(
                "the stream was cut off in the middle of an event".into(),
            ));
        }
        // Liberal: a server that ends the stream without a finish reason
        // or [DONE], after a whole answer.
        if decoder.text.is_empty() && decoder.calls.is_empty() {
            return Err(ChatError::BadResponse(
                "the stream ended before the answer was complete".into(),
            ));
        }
    }
    if decoder.finish.as_deref() == Some("length") && decoder.calls.is_empty() {
        return Err(ChatError::Incomplete {
            reason: "the answer reached the model's length limit".into(),
        });
    }
    Ok(Attempt::Done(decoder))
}

/// One tool call being put together from deltas.
#[derive(Clone, Debug, Default)]
struct CallBuild {
    id: Option<String>,
    name: String,
    arguments: String,
    /// Arguments sent as a JSON object instead of a string.
    object: Option<Value>,
}

/// Puts a Chat Completions stream (or one whole reply) together.
#[derive(Debug, Default)]
pub(crate) struct ChatDecoder {
    pub(crate) text: String,
    calls: Vec<CallBuild>,
    by_index: HashMap<u64, usize>,
    pub(crate) usage: Option<Usage>,
    pub(crate) finish: Option<String>,
    pub(crate) error: Option<(String, String)>,
    pub(crate) saw_done: bool,
    pub(crate) done: bool,
    pub(crate) emitted: bool,
    message_sent: bool,
}

fn chat_usage(u: &Value) -> Option<Usage> {
    let input = u["prompt_tokens"].as_u64();
    let output = u["completion_tokens"].as_u64();
    if input.is_none() && output.is_none() {
        return None;
    }
    Some(Usage {
        input_tokens: input.unwrap_or(0),
        cached_tokens: u["prompt_tokens_details"]["cached_tokens"]
            .as_u64()
            .unwrap_or(0),
        output_tokens: output.unwrap_or(0),
        reasoning_tokens: u["completion_tokens_details"]["reasoning_tokens"]
            .as_u64()
            .unwrap_or(0),
        requests: 1,
    })
}

fn error_of(e: &Value) -> (String, String) {
    let code = e["code"]
        .as_str()
        .map(str::to_string)
        .or_else(|| e["code"].as_i64().map(|n| n.to_string()))
        .or_else(|| e["type"].as_str().map(str::to_string))
        .unwrap_or_else(|| "error".into());
    let message = e["message"].as_str().or(e.as_str()).unwrap_or("");
    (util::safe_code(&code), util::cut(message, 300).to_string())
}

impl ChatDecoder {
    /// One SSE `data:` payload.
    pub(crate) fn handle_event(&mut self, data: &str) -> Vec<ChatEvent> {
        let data = data.trim();
        if data == "[DONE]" {
            self.saw_done = true;
            self.done = true;
            return Vec::new();
        }
        let Ok(v) = serde_json::from_str::<Value>(data) else {
            return Vec::new();
        };
        self.handle_chunk(&v)
    }

    fn handle_chunk(&mut self, v: &Value) -> Vec<ChatEvent> {
        let mut out = Vec::new();
        if v["error"].is_object() || v["error"].is_string() {
            self.error = Some(error_of(&v["error"]));
            self.done = true;
            return out;
        }
        if v["usage"].is_object() {
            if let Some(u) = chat_usage(&v["usage"]) {
                self.usage = Some(u);
            }
        }
        let Some(choice) = v["choices"].as_array().and_then(|c| c.first()) else {
            // A usage-only final chunk, or something to ignore.
            return out;
        };
        let delta = if choice["delta"].is_object() {
            &choice["delta"]
        } else {
            &choice["message"]
        };
        // `reasoning_content` and `reasoning` are thinking text: not shown,
        // not kept.
        if let Some(t) = delta["content"].as_str() {
            if !t.is_empty() {
                self.text.push_str(t);
                self.emitted = true;
                out.push(ChatEvent::TextDelta(t.to_string()));
            }
        }
        if let Some(calls) = delta["tool_calls"].as_array() {
            for tc in calls {
                self.call_delta(tc);
            }
        }
        if let Some(f) = choice["finish_reason"].as_str() {
            if f == "error" && self.error.is_none() {
                self.error = Some(error_of(&choice["error"]));
            }
            self.finish = Some(f.to_string());
        }
        out
    }

    fn call_delta(&mut self, tc: &Value) {
        let id = tc["id"].as_str().filter(|s| !s.is_empty());
        let slot = match tc["index"].as_u64() {
            Some(i) => match self.by_index.get(&i) {
                Some(&s) => s,
                None => {
                    self.calls.push(CallBuild::default());
                    self.by_index.insert(i, self.calls.len() - 1);
                    self.calls.len() - 1
                }
            },
            // No index: a new call when it brings an ID not seen yet (or
            // there is none), else more of the last one.
            None => {
                let known = id.is_some_and(|id| {
                    self.calls
                        .last()
                        .is_some_and(|c| c.id.as_deref() == Some(id))
                });
                if self.calls.is_empty() || (id.is_some() && !known) {
                    self.calls.push(CallBuild::default());
                }
                self.calls.len() - 1
            }
        };
        let c = &mut self.calls[slot];
        if c.id.is_none() {
            c.id = id.map(str::to_string);
        }
        let f = &tc["function"];
        if let Some(n) = f["name"].as_str() {
            if c.name.is_empty() {
                c.name = n.to_string();
            }
        }
        match &f["arguments"] {
            Value::String(s) => c.arguments.push_str(s),
            Value::Object(_) => c.object = Some(f["arguments"].clone()),
            _ => {}
        }
        self.emitted = true;
    }

    /// One whole (not streamed) reply.
    pub(crate) fn handle_full(&mut self, v: &Value) -> Vec<ChatEvent> {
        let mut out = Vec::new();
        if v["error"].is_object() || v["error"].is_string() {
            self.error = Some(error_of(&v["error"]));
            return out;
        }
        if let Some(u) = chat_usage(&v["usage"]) {
            self.usage = Some(u);
        }
        let Some(choice) = v["choices"].as_array().and_then(|c| c.first()) else {
            self.error = Some(("no_choices".into(), "the reply has no answer".into()));
            return out;
        };
        let msg = &choice["message"];
        if let Some(t) = msg["content"].as_str() {
            if !t.is_empty() {
                self.text = t.to_string();
                out.push(ChatEvent::TextDelta(t.to_string()));
            }
        }
        if let Some(calls) = msg["tool_calls"].as_array() {
            for (i, tc) in calls.iter().enumerate() {
                let mut tc = tc.clone();
                if tc["index"].is_null() {
                    tc["index"] = json!(i);
                }
                self.call_delta(&tc);
            }
        }
        self.finish = choice["finish_reason"].as_str().map(str::to_string);
        if let Some(m) = self.message() {
            out.push(ChatEvent::Message(m));
        }
        out
    }

    /// The whole assistant text, once.
    fn message(&mut self) -> Option<String> {
        if self.message_sent || self.text.is_empty() {
            return None;
        }
        self.message_sent = true;
        Some(self.text.clone())
    }

    /// The finished calls: (id, name, arguments as a JSON string). Calls
    /// without a name are dropped; missing IDs are made up and repeated
    /// ones made unique, so each tool result matches one call.
    pub(crate) fn calls(&self) -> Vec<(String, String, String)> {
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for (i, c) in self.calls.iter().enumerate() {
            if c.name.trim().is_empty() {
                continue;
            }
            let mut id = c.id.clone().unwrap_or_else(|| format!("call_{}", i + 1));
            if !seen.insert(id.clone()) {
                id = format!("{id}_{}", i + 1);
                seen.insert(id.clone());
            }
            let arguments = match &c.object {
                Some(o) => o.to_string(),
                None if c.arguments.trim().is_empty() => "{}".to_string(),
                None => c.arguments.clone(),
            };
            out.push((id, c.name.clone(), arguments));
        }
        out
    }
}

fn list_models_at(
    conn: &Connection,
    url: &Url,
    key: &Option<Secret>,
    cfg: &HttpConfig,
    cancel: &CancelToken,
) -> Result<Vec<ModelInfo>, ChatError> {
    let auth = CompatClient::auth_header(key);
    let mut headers = vec![("Accept", "application/json")];
    if let Some(a) = &auth {
        headers.push(("Authorization", a.expose()));
    }
    let resp = http::send("GET", &url.join("models"), &headers, b"", cfg, cancel)
        .map_err(|e| net(e, conn, url))?;
    let status = resp.status;
    let body = resp.read_all(4 << 20).map_err(|e| net(e, conn, url))?;
    match status {
        200 => {}
        401 | 403 => return Err(ChatError::KeyRejected { status }),
        404 | 405 | 501 => return Ok(Vec::new()),
        _ => {
            let (code, message, _) = api_error(status, &body);
            return Err(ChatError::Api {
                status,
                code,
                message: scrub(&message, key.as_ref()),
            });
        }
    }
    let v: Value = serde_json::from_slice(&body)
        .map_err(|_| ChatError::BadResponse("the model list is not JSON".into()))?;
    Ok(parse_models(&v))
}

fn parse_models(v: &Value) -> Vec<ModelInfo> {
    let list = v["data"]
        .as_array()
        .or(v["models"].as_array())
        .or(v.as_array())
        .cloned()
        .unwrap_or_default();
    list.iter()
        .filter_map(|m| {
            let id = m["id"]
                .as_str()
                .or(m["slug"].as_str())
                .or(m["name"].as_str())
                .or(m.as_str())?;
            Some(ModelInfo {
                slug: id.to_string(),
                display_name: m["display_name"]
                    .as_str()
                    .or(m["name"].as_str())
                    .unwrap_or(id)
                    .to_string(),
            })
        })
        .collect()
}

/// What [`test_connection`] found. `Display` gives the plain sentence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TestOutcome {
    /// The server answered and accepted the key; `models` it lists.
    Ok {
        models: usize,
    },
    /// The key was refused (HTTP 401 or 403).
    WrongKey {
        status: u16,
    },
    /// The connection needs a key and none is stored.
    NoKey,
    /// The address answered "not found": a wrong base URL.
    NotFound,
    /// Nothing answers at a local address; `hint` says what to start.
    NotRunning {
        hint: String,
    },
    /// The connection failed and a proxy is set up on this computer.
    Proxy,
    /// The secure connection failed.
    Tls(String),
    Timeout,
    /// The address is not allowed or cannot be read.
    AddressNotAllowed(String),
    /// Could not connect to a remote server.
    Unreachable(String),
    /// The server answered with another error.
    ServerError {
        status: u16,
    },
    Cancelled,
}

impl fmt::Display for TestOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TestOutcome::Ok { models } => {
                write!(f, "Connected. The server lists {models} model(s).")
            }
            TestOutcome::WrongKey { status } => write!(
                f,
                "The server refused the API key (HTTP {status}). Check the key."
            ),
            TestOutcome::NoKey => f.write_str("This connection needs an API key."),
            TestOutcome::NotFound => {
                f.write_str("The server answered \"not found\". Check the address.")
            }
            TestOutcome::NotRunning { hint } => write!(f, "Nothing answers at that address. {hint}"),
            TestOutcome::Proxy => f.write_str(
                "Could not connect. A proxy server is set up on this computer, and this connection does not support proxies yet.",
            ),
            TestOutcome::Tls(e) => write!(f, "The secure connection failed: {e}."),
            TestOutcome::Timeout => f.write_str("The server did not answer in time."),
            TestOutcome::AddressNotAllowed(e) => write!(f, "The address cannot be used: {e}."),
            TestOutcome::Unreachable(e) => write!(f, "Could not reach the server: {e}."),
            TestOutcome::ServerError { status } => {
                write!(f, "The server answered with an error (HTTP {status}).")
            }
            TestOutcome::Cancelled => f.write_str("Cancelled."),
        }
    }
}

/// The timeouts [`test_connection`] uses.
pub fn test_http_config() -> HttpConfig {
    HttpConfig {
        connect_timeout: Duration::from_secs(4),
        read_timeout: Duration::from_secs(8),
    }
}

/// Check a connection for the settings dialog and Check Setup: reach the
/// server and check the key, with requests that cost nothing:
/// `GET {base}/models` (OpenAI, Anthropic, Ollama, LM Studio, Custom; for
/// Anthropic also with its own key headers), and for OpenRouter, whose
/// model list needs no key, `GET {base}/key` (its documented key check),
/// then the model list for the count. Short timeouts, cancellable.
pub fn test_connection(
    conn: &Connection,
    store: &dyn CredentialStore,
    cancel: &CancelToken,
) -> TestOutcome {
    test_connection_with(conn, store, &test_http_config(), cancel)
}

/// [`test_connection`] with other timeouts.
pub fn test_connection_with(
    conn: &Connection,
    store: &dyn CredentialStore,
    cfg: &HttpConfig,
    cancel: &CancelToken,
) -> TestOutcome {
    let url = match conn.address() {
        Ok(u) => u,
        Err(e) => return TestOutcome::AddressNotAllowed(e),
    };
    let key = match stored_key(store, conn) {
        Ok(k) => k,
        Err(e) => return TestOutcome::Unreachable(e.to_string()),
    };
    if key.is_none() && conn.needs_key {
        return TestOutcome::NoKey;
    }
    let get = |path: &str, extra: &[(&str, &str)]| -> Result<(u16, Vec<u8>), TestOutcome> {
        let auth = CompatClient::auth_header(&key);
        let mut headers: Vec<(&str, &str)> = vec![("Accept", "application/json")];
        if let Some(a) = &auth {
            headers.push(("Authorization", a.expose()));
        }
        headers.extend_from_slice(extra);
        let resp = http::send("GET", &url.join(path), &headers, b"", cfg, cancel)
            .map_err(|e| outcome_of(e, conn, &url))?;
        let status = resp.status;
        let body = resp
            .read_all(4 << 20)
            .map_err(|e| outcome_of(e, conn, &url))?;
        Ok((status, body))
    };
    let status_outcome = |status: u16| match status {
        401 | 403 => TestOutcome::WrongKey { status },
        404 => TestOutcome::NotFound,
        s => TestOutcome::ServerError { status: s },
    };
    if conn.preset == Preset::OpenRouter {
        match get("key", &[]) {
            Ok((200, _)) => {}
            Ok((s, _)) => return status_outcome(s),
            Err(o) => return o,
        }
    }
    let anthropic_key;
    let mut extra: Vec<(&str, &str)> = Vec::new();
    if conn.preset == Preset::Anthropic {
        if let Some(k) = &key {
            anthropic_key = k.clone();
            extra.push(("x-api-key", anthropic_key.expose()));
            extra.push(("anthropic-version", "2023-06-01"));
        }
    }
    match get("models", &extra) {
        Ok((200, body)) => {
            let models = serde_json::from_slice::<Value>(&body)
                .map(|v| parse_models(&v).len())
                .unwrap_or(0);
            TestOutcome::Ok { models }
        }
        Ok((s, _)) => status_outcome(s),
        Err(o) => o,
    }
}

fn outcome_of(e: HttpError, conn: &Connection, url: &Url) -> TestOutcome {
    match e {
        HttpError::Cancelled => TestOutcome::Cancelled,
        HttpError::Proxy => TestOutcome::Proxy,
        HttpError::Timeout => TestOutcome::Timeout,
        HttpError::Tls(m) => TestOutcome::Tls(m),
        HttpError::Connect(_) if is_loopback(&url.host) => TestOutcome::NotRunning {
            hint: conn.preset.info().start_hint.to_string(),
        },
        e => TestOutcome::Unreachable(e.to_string()),
    }
}

#[cfg(test)]
mod tests;
