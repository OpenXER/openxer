//! The chat client: one turn is one or more streamed requests to
//! `POST /v1/responses` (`store: false`, `stream: true`), with the tool loop
//! in between, reported as [`ChatEvent`]s on a channel.
//!
//! Retry policy (stated here, pinned by tests):
//! - 401: refresh the token once and resend; a second 401 ends the turn
//!   with [`ChatError::SignedOut`].
//! - 429 with `subscription_sharing_usage_limit_exceeded`: no retry, no
//!   other billing path; [`ChatError::UsageLimit`].
//! - other 429, 5xx, and network failures before the stream starts: up to
//!   [`ApiConfig::retry_delays`] retries (1 s then 4 s by default, a
//!   `Retry-After` of up to 20 s is honored), then the error.
//! - 400 `subscription_sharing_unsupported_capability` naming an optional
//!   field this client sends (`prompt_cache_key`, `include`, `reasoning`,
//!   `tool_choice`): resend once without it, and leave it out for the rest
//!   of the conversation.
//! - 403 and other 4xx: no retry.
//! - once events have started arriving, a failure is not retried.

use crate::auth::{Auth, AuthError, SignInStatus};
use crate::conversation::{cap_tool_result, summarize_result, Conversation, Entry};
use crate::http::{self, HttpConfig, HttpError, Url};
use crate::sse::SseParser;
use crate::stream::{Decoder, Emit, End, OutputItem, Usage};
use crate::tools::{ProposedChanges, ToolOutput, Tools};
use crate::util::{self, CancelToken, Secret};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::fmt;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Where usage is managed, for the "Manage usage" link.
pub const USAGE_SETTINGS_URL: &str = "https://chatgpt.com/settings/usage";
/// The error code of a usage-limit hit.
pub const USAGE_LIMIT_CODE: &str = "subscription_sharing_usage_limit_exceeded";

/// What a turn reports, in order. Plain data.
#[derive(Clone, Debug, PartialEq)]
pub enum ChatEvent {
    /// More answer text as it streams.
    TextDelta(String),
    /// A whole assistant message (the deltas so far, joined).
    Message(String),
    ToolStarted {
        id: String,
        name: String,
        target: String,
    },
    ToolFinished {
        id: String,
        name: String,
        ok: bool,
        duration: Duration,
    },
    /// A propose-only tool produced changes for the user to review.
    ProposedChanges {
        id: String,
        changes: ProposedChanges,
    },
    /// The turn ended normally; token use summed over its requests.
    TurnFinished(Usage),
    /// The turn ended with an error. The conversation is as before the turn.
    Error(ChatError),
    /// The turn was cancelled. The conversation is as before the turn.
    Cancelled,
}

/// Why a turn failed. Messages carry no tokens or request content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChatError {
    /// Not signed in, or the sign-in could not be renewed.
    SignedOut,
    /// Signed in, but the user did not allow use of their ChatGPT plan.
    PlanUsageNotEnabled,
    /// The plan's or this app's usage limit is reached (HTTP 429,
    /// `subscription_sharing_usage_limit_exceeded`). Point the user at
    /// [`USAGE_SETTINGS_URL`]; do not assume a reset time.
    UsageLimit {
        message: String,
    },
    /// The account or region may not use this route (403).
    NotAllowed {
        code: String,
        message: String,
    },
    Network(String),
    /// The connection failed and a proxy server is set up on this
    /// computer; this route does not support proxies yet.
    ProxyNotSupported,
    /// The reply could not be understood, was too large, or the stream was
    /// cut off.
    BadResponse(String),
    /// Another API error.
    Api {
        status: u16,
        code: String,
        message: String,
    },
    /// The model stopped early (for example a content filter).
    Incomplete {
        reason: String,
    },
    /// The model kept calling tools past the round limit.
    RoundCap {
        rounds: u32,
    },
    Cancelled,
    /// A compatible endpoint refused the API key (HTTP 401 or 403).
    KeyRejected {
        status: u16,
    },
    /// A compatible endpoint needs an API key and none is stored.
    NoKey,
    /// The endpoint answered "not found": the address or the model name is
    /// wrong.
    NotFound {
        message: String,
    },
    /// Nothing answers at a local address; `hint` says what to start.
    ServerNotRunning {
        hint: String,
    },
    /// The connection's address is not allowed (not https, and not this
    /// computer) or cannot be read.
    AddressNotAllowed(String),
}

impl fmt::Display for ChatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChatError::SignedOut => f.write_str("Not signed in to ChatGPT. Sign in again."),
            ChatError::PlanUsageNotEnabled => f.write_str(
                "Signed in, but OpenXER is not allowed to use your ChatGPT plan. Sign in again and allow plan usage.",
            ),
            ChatError::UsageLimit { .. } => f.write_str(
                "Usage limit reached. Review your plan or this app's limit in ChatGPT settings.",
            ),
            ChatError::NotAllowed { code, .. } => {
                write!(f, "This account cannot use ChatGPT plan usage here ({code}).")
            }
            ChatError::Network(e) => write!(f, "Network error: {e}."),
            ChatError::ProxyNotSupported => f.write_str(
                "Could not connect. A proxy server is set up on this computer, and chat with a ChatGPT plan does not support proxies yet.",
            ),
            ChatError::BadResponse(e) => write!(f, "Unexpected reply: {e}."),
            ChatError::Api { status, code, message } => {
                if message.is_empty() {
                    write!(f, "The request failed (HTTP {status}, {code}).")
                } else {
                    write!(f, "The request failed (HTTP {status}, {code}): {message}")
                }
            }
            ChatError::Incomplete { reason } => write!(f, "The answer stopped early ({reason})."),
            ChatError::RoundCap { rounds } => write!(
                f,
                "Stopped after {rounds} rounds of tool calls. Ask a narrower question."
            ),
            ChatError::Cancelled => f.write_str("Cancelled."),
            ChatError::KeyRejected { status } => write!(
                f,
                "The server refused the API key (HTTP {status}). Check the key in the connection settings."
            ),
            ChatError::NoKey => f.write_str(
                "This connection needs an API key. Add one in the connection settings.",
            ),
            ChatError::NotFound { message } => {
                if message.is_empty() {
                    f.write_str("The server answered \"not found\": check the address and the model name.")
                } else {
                    write!(
                        f,
                        "The server answered \"not found\": check the address and the model name ({message})."
                    )
                }
            }
            ChatError::ServerNotRunning { hint } => write!(f, "Nothing answers at that address. {hint}"),
            ChatError::AddressNotAllowed(e) => write!(f, "The address cannot be used: {e}."),
        }
    }
}

impl std::error::Error for ChatError {}

impl From<AuthError> for ChatError {
    fn from(e: AuthError) -> Self {
        match e {
            AuthError::Cancelled => ChatError::Cancelled,
            AuthError::Network(m) => ChatError::Network(m),
            AuthError::ProxyNotSupported => ChatError::ProxyNotSupported,
            AuthError::Server { status } => ChatError::Api {
                status,
                code: "auth_unavailable".into(),
                message: String::new(),
            },
            _ => ChatError::SignedOut,
        }
    }
}

/// Where the API is and how to call it.
#[derive(Clone, Debug)]
pub struct ApiConfig {
    /// `https://api.openai.com/v1`.
    pub base_url: Url,
    pub http: HttpConfig,
    /// Waits before each retry of a retryable failure.
    pub retry_delays: Vec<Duration>,
    /// The longest `Retry-After` honored.
    pub max_retry_after: Duration,
    /// Put the tools in one namespace (as the plan-usage docs ask) rather
    /// than at the top level.
    pub tool_namespace: Option<String>,
    /// The largest single server-sent event line accepted.
    pub max_event_bytes: usize,
    /// The largest streamed reply accepted.
    pub max_stream_bytes: usize,
}

impl ApiConfig {
    pub fn openai() -> ApiConfig {
        ApiConfig {
            base_url: Url::parse("https://api.openai.com/v1").expect("built-in URL"),
            http: HttpConfig::default(),
            retry_delays: vec![Duration::from_secs(1), Duration::from_secs(4)],
            max_retry_after: Duration::from_secs(20),
            tool_namespace: Some("openxer".into()),
            max_event_bytes: 2 << 20,
            max_stream_bytes: 32 << 20,
        }
    }
}

/// A model the account may use.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelInfo {
    /// Sent as `model`.
    pub slug: String,
    /// Shown in the picker.
    pub display_name: String,
}

/// The model to preselect: the first one the endpoint lists. The docs name
/// no default and no cost order, so none is assumed; the user can pick.
pub fn default_model(models: &[ModelInfo]) -> Option<&ModelInfo> {
    models.first()
}

/// One turn's inputs.
#[derive(Clone, Debug)]
pub struct TurnRequest {
    pub model: String,
    /// Short context about the open schedule (project name, data date). It
    /// follows the fixed instructions and should change rarely.
    pub context: String,
    pub user_text: String,
    /// `low`, `medium` or `high`, when the model supports it.
    pub reasoning_effort: Option<String>,
}

/// A turn running on its own thread.
pub struct TurnHandle {
    pub events: Receiver<ChatEvent>,
    pub(crate) cancel: CancelToken,
    pub(crate) thread: JoinHandle<Conversation>,
}

impl TurnHandle {
    pub fn cancel(&self) {
        self.cancel.cancel();
    }
    /// The conversation after the turn (call after the last event).
    pub fn join(self) -> Conversation {
        self.thread.join().expect("chat turn thread")
    }
}

/// Talks to the Responses API with the stored sign-in.
pub struct ChatClient {
    auth: Arc<Auth>,
    api: ApiConfig,
    /// Optional fields the server refused; left out from then on.
    dropped: Mutex<BTreeSet<&'static str>>,
}

impl fmt::Debug for ChatClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ChatClient({:?})", self.api.base_url)
    }
}

const OPTIONAL_FIELDS: &[&str] = &["prompt_cache_key", "include", "reasoning", "tool_choice"];

enum Attempt {
    Done(Decoder),
    Retry(Option<Duration>, ChatError),
    Refresh(Secret),
    Drop(&'static str),
}

pub(crate) fn api_error(status: u16, body: &[u8]) -> (String, String, Option<String>) {
    let v: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
    let e = if v["error"].is_object() {
        &v["error"]
    } else {
        &v
    };
    let code = e["code"]
        .as_str()
        .or(e["type"].as_str())
        .map(util::safe_code)
        .unwrap_or_else(|| format!("http_{status}"));
    let message = e["message"]
        .as_str()
        .or(v["detail"].as_str())
        .map(|m| util::cut(m, 300).to_string())
        .unwrap_or_default();
    (code, message, e["param"].as_str().map(str::to_string))
}

fn net(e: HttpError) -> ChatError {
    match e {
        HttpError::Cancelled => ChatError::Cancelled,
        HttpError::Proxy => ChatError::ProxyNotSupported,
        e => ChatError::Network(e.to_string()),
    }
}

/// A server message with our token taken out, in case a server echoes it.
fn scrub(message: &str, token: &Secret) -> String {
    if token.is_empty() {
        return message.to_string();
    }
    message.replace(token.expose(), "<redacted>")
}

impl ChatClient {
    pub fn new(auth: Arc<Auth>, api: ApiConfig) -> ChatClient {
        ChatClient {
            auth,
            api,
            dropped: Mutex::new(BTreeSet::new()),
        }
    }

    pub fn auth(&self) -> &Arc<Auth> {
        &self.auth
    }

    fn check_signed_in(&self) -> Result<(), ChatError> {
        match self.auth.status() {
            SignInStatus::SignedIn {
                plan_usage: true, ..
            } => Ok(()),
            SignInStatus::SignedIn { .. } => Err(ChatError::PlanUsageNotEnabled),
            _ => Err(ChatError::SignedOut),
        }
    }

    /// The models the signed-in account may use (`GET /models`, those with
    /// `visibility: "list"`), in the order the endpoint gives.
    pub fn list_models(&self, cancel: &CancelToken) -> Result<Vec<ModelInfo>, ChatError> {
        self.check_signed_in()?;
        let url = self.api.base_url.join("models");
        let mut token = self.auth.access_token(cancel)?;
        let mut refreshed = false;
        loop {
            let auth = Secret::new(format!("Bearer {}", token.expose()));
            let resp = http::send(
                "GET",
                &url,
                &[
                    ("Authorization", auth.expose()),
                    ("Accept", "application/json"),
                ],
                b"",
                &self.api.http,
                cancel,
            )
            .map_err(net)?;
            let status = resp.status;
            let body = resp.read_all(4 << 20).map_err(net)?;
            if status == 401 && !refreshed {
                refreshed = true;
                token = self.auth.refresh_after_rejection(&token, cancel)?;
                continue;
            }
            if status == 401 {
                return Err(ChatError::SignedOut);
            }
            if status != 200 {
                let (code, message, _) = api_error(status, &body);
                return Err(ChatError::Api {
                    status,
                    code,
                    message,
                });
            }
            let v: Value = serde_json::from_slice(&body)
                .map_err(|_| ChatError::BadResponse("the model list is not JSON".into()))?;
            let list = v["models"]
                .as_array()
                .or(v["data"].as_array())
                .cloned()
                .unwrap_or_default();
            let mut out = Vec::new();
            for m in list {
                if m.get("visibility").is_some_and(|x| x != "list") {
                    continue;
                }
                let Some(slug) = m["slug"].as_str().or(m["id"].as_str()) else {
                    continue;
                };
                out.push(ModelInfo {
                    slug: slug.to_string(),
                    display_name: m["display_name"].as_str().unwrap_or(slug).to_string(),
                });
            }
            return Ok(out);
        }
    }

    /// Start a turn on its own thread. Read events from the handle; after
    /// the last one (`TurnFinished`, `Error` or `Cancelled`) take the
    /// conversation back with [`TurnHandle::join`].
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

    /// Run one turn on this thread. Every event, including the final
    /// `TurnFinished`, `Error` or `Cancelled`, goes to `events`. On failure
    /// the conversation is left as it was before the turn.
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
        self.check_signed_in()?;
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
                json!({"type": "function", "name": d.name, "description": d.description,
                    "parameters": d.parameters, "strict": false})
            })
            .collect();
        let tools_field = match (&self.api.tool_namespace, tool_json.is_empty()) {
            (_, true) => None,
            (Some(ns), false) => Some(json!([{"type": "namespace", "name": ns,
                "description": "Tools that read the open schedule in OpenXER.",
                "tools": tool_json}])),
            (None, false) => Some(Value::Array(tool_json)),
        };
        let instructions = conv.instructions(&request.context);
        let max_rounds = conv.budgets.max_rounds.max(1);
        let mut usage = Usage::default();
        for round in 0..max_rounds {
            let last = round + 1 == max_rounds;
            let body = self.body(conv, request, &instructions, tools_field.as_ref(), last);
            let decoder = self.stream(&body, events, cancel)?;
            if let Some(End::Completed(u)) = &decoder.end {
                usage.add(u);
            }
            let items = decoder.items();
            let mut calls = Vec::new();
            for item in items {
                match item {
                    OutputItem::Message(text) => conv.push(Entry::Assistant { turn, text }),
                    OutputItem::Reasoning(item) => conv.push(Entry::Reasoning { turn, item }),
                    OutputItem::Call {
                        call_id,
                        name,
                        namespace,
                        arguments,
                    } => {
                        conv.push(Entry::Call {
                            turn,
                            call_id: call_id.clone(),
                            name: name.clone(),
                            namespace,
                            arguments: arguments.clone(),
                        });
                        calls.push((call_id, name, arguments));
                    }
                }
            }
            if calls.is_empty() {
                return Ok(usage);
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

    fn body(
        &self,
        conv: &Conversation,
        request: &TurnRequest,
        instructions: &str,
        tools: Option<&Value>,
        last_round: bool,
    ) -> Value {
        let mut b = json!({
            "model": request.model,
            "instructions": instructions,
            "input": conv.input_items(),
            "store": false,
            "stream": true,
            "prompt_cache_key": conv.cache_key(),
            "include": ["reasoning.encrypted_content"],
        });
        if let Some(t) = tools {
            b["tools"] = t.clone();
            if last_round {
                b["tool_choice"] = json!("none");
            }
        }
        if let Some(effort) = &request.reasoning_effort {
            b["reasoning"] = json!({"effort": effort});
        }
        let dropped = self.dropped.lock().unwrap();
        if let Some(o) = b.as_object_mut() {
            for f in dropped.iter() {
                o.remove(*f);
            }
        }
        b
    }

    /// Send one request and read its stream, with the retry policy above.
    fn stream(
        &self,
        body: &Value,
        events: &Sender<ChatEvent>,
        cancel: &CancelToken,
    ) -> Result<Decoder, ChatError> {
        let mut body = body.clone();
        let mut token = self.auth.access_token(cancel)?;
        let mut refreshed = false;
        let mut retries = self.api.retry_delays.iter();
        loop {
            match self.attempt(&body, &token, events, cancel)? {
                Attempt::Done(d) => return Ok(d),
                Attempt::Refresh(rejected) => {
                    if refreshed {
                        return Err(ChatError::SignedOut);
                    }
                    refreshed = true;
                    token = self.auth.refresh_after_rejection(&rejected, cancel)?;
                }
                Attempt::Drop(field) => {
                    self.dropped.lock().unwrap().insert(field);
                    if let Some(o) = body.as_object_mut() {
                        o.remove(field);
                    }
                }
                Attempt::Retry(after, err) => {
                    let Some(delay) = retries.next() else {
                        return Err(err);
                    };
                    let wait = after
                        .map(|a| a.min(self.api.max_retry_after))
                        .unwrap_or(*delay);
                    if cancel.sleep(wait) {
                        return Err(ChatError::Cancelled);
                    }
                    token = self.auth.access_token(cancel)?;
                }
            }
        }
    }

    fn attempt(
        &self,
        body: &Value,
        token: &Secret,
        events: &Sender<ChatEvent>,
        cancel: &CancelToken,
    ) -> Result<Attempt, ChatError> {
        let url = self.api.base_url.join("responses");
        let auth = Secret::new(format!("Bearer {}", token.expose()));
        let payload = body.to_string();
        let sent = http::send(
            "POST",
            &url,
            &[
                ("Authorization", auth.expose()),
                ("Content-Type", "application/json"),
                ("Accept", "text/event-stream"),
            ],
            payload.as_bytes(),
            &self.api.http,
            cancel,
        );
        let mut resp = match sent {
            Ok(r) => r,
            Err(HttpError::Cancelled) => return Err(ChatError::Cancelled),
            Err(e @ (HttpError::Proxy | HttpError::InvalidRequest(_))) => return Err(net(e)),
            Err(e) => return Ok(Attempt::Retry(None, net(e))),
        };
        let status = resp.status;
        if status != 200 {
            let retry_after = resp
                .header("retry-after")
                .and_then(|v| v.trim().parse::<u64>().ok())
                .map(Duration::from_secs);
            let reply = resp.read_all(1 << 20).unwrap_or_default();
            let (code, message, param) = api_error(status, &reply);
            let message = scrub(&message, token);
            return Ok(match status {
                401 => Attempt::Refresh(token.clone()),
                429 if code == USAGE_LIMIT_CODE => {
                    return Err(ChatError::UsageLimit { message });
                }
                400 if code == "subscription_sharing_unsupported_capability" => {
                    let field = param.as_deref().and_then(|p| {
                        OPTIONAL_FIELDS
                            .iter()
                            .find(|f| {
                                p == **f
                                    || p.starts_with(&format!("{f}."))
                                    || p.starts_with(&format!("{f}["))
                            })
                            .copied()
                    });
                    match field {
                        // Only a field this request carries: each can be
                        // dropped once, so this cannot loop.
                        Some(f) if body.get(f).is_some() => Attempt::Drop(f),
                        _ => {
                            return Err(ChatError::Api {
                                status,
                                code,
                                message,
                            })
                        }
                    }
                }
                403 => {
                    return Err(ChatError::NotAllowed { code, message });
                }
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
        let mut parser = SseParser::with_max_line(self.api.max_event_bytes);
        let mut decoder = Decoder::new();
        let mut buf = vec![0u8; 16 * 1024];
        let mut total = 0usize;
        loop {
            if cancel.is_cancelled() {
                return Err(ChatError::Cancelled);
            }
            let n = match resp.read_some(&mut buf) {
                Ok(n) => n,
                Err(HttpError::Cancelled) => return Err(ChatError::Cancelled),
                Err(e) if !decoder.emitted => return Ok(Attempt::Retry(None, net(e))),
                Err(e) => return Err(net(e)),
            };
            if n == 0 {
                break;
            }
            total += n;
            if total > self.api.max_stream_bytes {
                return Err(ChatError::BadResponse("the reply was too large".into()));
            }
            let events_now = parser.push(&buf[..n]);
            if parser.overflowed() {
                return Err(ChatError::BadResponse(
                    "an event in the reply was too large".into(),
                ));
            }
            for ev in events_now {
                for emit in decoder.handle(&ev) {
                    let _ = events.send(match emit {
                        Emit::TextDelta(t) => ChatEvent::TextDelta(t),
                        Emit::Message(m) => ChatEvent::Message(m),
                    });
                }
                if decoder.end.is_some() {
                    break;
                }
            }
            if decoder.end.is_some() {
                break;
            }
        }
        match decoder.end.clone() {
            Some(End::Completed(_)) => Ok(Attempt::Done(decoder)),
            Some(End::Failed { code, message, .. }) => {
                let message = scrub(&message, token);
                if code == USAGE_LIMIT_CODE {
                    Err(ChatError::UsageLimit { message })
                } else {
                    Err(ChatError::Api {
                        status: 200,
                        code,
                        message,
                    })
                }
            }
            Some(End::Incomplete { reason, .. }) => Err(ChatError::Incomplete { reason }),
            None if parser.has_partial() => Err(ChatError::BadResponse(
                "the stream was cut off in the middle of an event".into(),
            )),
            None => Err(ChatError::BadResponse(
                "the stream ended before the answer was complete".into(),
            )),
        }
    }
}

/// Run one tool call on its own thread (cancel stops the wait at once),
/// report it, and return the capped result text and its summary line.
/// Shared by the ChatGPT route and the compatible-endpoint route.
pub(crate) fn run_tool(
    tools: &Arc<dyn Tools>,
    call_id: &str,
    name: &str,
    arguments: &str,
    conv: &Conversation,
    events: &Sender<ChatEvent>,
    cancel: &CancelToken,
) -> Result<(String, String), ChatError> {
    let args: Result<Value, _> = if arguments.trim().is_empty() {
        Ok(json!({}))
    } else {
        serde_json::from_str(arguments)
    };
    let target = args
        .as_ref()
        .map(|a| tools.target(name, a))
        .unwrap_or_default();
    let _ = events.send(ChatEvent::ToolStarted {
        id: call_id.to_string(),
        name: name.to_string(),
        target: target.clone(),
    });
    let started = Instant::now();
    let output = match args {
        Err(_) => ToolOutput::error("The arguments were not valid JSON. Send a JSON object."),
        Ok(args) => {
            let (tx, rx) = mpsc::channel();
            let (t, n, c) = (tools.clone(), name.to_string(), cancel.clone());
            std::thread::spawn(move || {
                let _ = tx.send(t.call(&n, &args, &c));
            });
            loop {
                if cancel.is_cancelled() {
                    return Err(ChatError::Cancelled);
                }
                match rx.recv_timeout(Duration::from_millis(20)) {
                    Ok(o) => break o,
                    Err(mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        break ToolOutput::error("The tool stopped without a result.")
                    }
                }
            }
        }
    };
    let _ = events.send(ChatEvent::ToolFinished {
        id: call_id.to_string(),
        name: name.to_string(),
        ok: output.ok,
        duration: started.elapsed(),
    });
    if let Some(changes) = output.proposed.clone() {
        let _ = events.send(ChatEvent::ProposedChanges {
            id: call_id.to_string(),
            changes,
        });
    }
    let text = if output.ok {
        output.text
    } else {
        format!("Error: {}", output.text)
    };
    let text = cap_tool_result(&text, &conv.budgets);
    let summary = summarize_result(name, &target, &text, &conv.budgets);
    Ok((text, summary))
}
