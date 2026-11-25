//! Fakes for tests, here and in the app (the `testkit` feature): a fake
//! HTTP server on 127.0.0.1 (plain HTTP, std::net) that stands in for the
//! authorization server and the API, a fake browser that follows the
//! redirect itself, and builders for streamed replies. No test talks to a
//! real host or opens a browser.

use crate::auth::{Auth, AuthConfig, SCOPES};
use crate::http::{HttpConfig, Url};
use crate::jwt::testing::Signer;
use crate::store::MemoryStore;
use crate::util::{b64url, parse_query};
use serde_json::json;
use serde_json::Value;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::AtomicU64;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// The app's tests: 0 reads the real proxy settings, 1 pretends none is
/// set up and 2 pretends one is, for every thread of the process.
#[cfg(feature = "testkit")]
pub static FAKE_PROXY_ALL: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// A request the fake server received.
#[derive(Clone, Debug)]
pub struct Seen {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Seen {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
    pub fn form(&self) -> HashMap<String, String> {
        crate::util::parse_query(&String::from_utf8_lossy(&self.body))
            .into_iter()
            .collect()
    }
    pub fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap()
    }
}

/// What the fake server sends back.
#[derive(Clone, Debug)]
pub struct Reply {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    /// Sent as chunks of a chunked body, with `delay` before each.
    pub chunks: Vec<Vec<u8>>,
    pub delay: Duration,
    /// Close the connection before the final chunk (a cut-off stream).
    pub cut: bool,
    /// Wait this long before answering at all.
    pub stall: Duration,
    pub framing: Framing,
}

/// How the fake frames a body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Framing {
    Chunked,
    /// One Content-Length body.
    Length,
    /// No length: the body ends when the connection closes.
    Close,
}

impl Reply {
    pub fn json(status: u16, v: &Value) -> Reply {
        Reply {
            status,
            headers: vec![("Content-Type".into(), "application/json".into())],
            chunks: vec![v.to_string().into_bytes()],
            delay: Duration::ZERO,
            cut: false,
            stall: Duration::ZERO,
            framing: Framing::Chunked,
        }
    }
    pub fn sse(events: &[String]) -> Reply {
        Reply {
            status: 200,
            headers: vec![("Content-Type".into(), "text/event-stream".into())],
            chunks: events.iter().map(|e| e.clone().into_bytes()).collect(),
            delay: Duration::ZERO,
            cut: false,
            stall: Duration::ZERO,
            framing: Framing::Chunked,
        }
    }
    pub fn empty(status: u16) -> Reply {
        Reply {
            status,
            headers: Vec::new(),
            chunks: Vec::new(),
            delay: Duration::ZERO,
            cut: false,
            stall: Duration::ZERO,
            framing: Framing::Chunked,
        }
    }
    pub fn framed(mut self, framing: Framing) -> Reply {
        self.framing = framing;
        self
    }
    pub fn header(mut self, k: &str, v: &str) -> Reply {
        self.headers.push((k.into(), v.into()));
        self
    }
}

type Handler = Box<dyn Fn(&Seen) -> Reply + Send + Sync>;

#[derive(Default)]
struct State {
    routes: Mutex<HashMap<(String, String), Arc<Handler>>>,
    seen: Mutex<Vec<Seen>>,
}

pub struct FakeServer {
    port: u16,
    state: Arc<State>,
    stop: Arc<AtomicBool>,
}

impl FakeServer {
    pub fn start() -> FakeServer {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let state = Arc::new(State::default());
        let stop = Arc::new(AtomicBool::new(false));
        let (s2, stop2) = (state.clone(), stop.clone());
        std::thread::spawn(move || {
            for conn in listener.incoming() {
                if stop2.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(conn) = conn else { continue };
                let s3 = s2.clone();
                std::thread::spawn(move || handle(conn, &s3));
            }
        });
        FakeServer { port, state, stop }
    }

    pub fn url(&self, path: &str) -> Url {
        Url::parse(&format!("http://127.0.0.1:{}{}", self.port, path)).unwrap()
    }

    pub fn route(
        &self,
        method: &str,
        path: &str,
        h: impl Fn(&Seen) -> Reply + Send + Sync + 'static,
    ) {
        self.state
            .routes
            .lock()
            .unwrap()
            .insert((method.into(), path.into()), Arc::new(Box::new(h)));
    }

    pub fn route_json(&self, method: &str, path: &str, status: u16, v: Value) {
        self.route(method, path, move |_| Reply::json(status, &v));
    }

    /// Answer with each reply in turn, repeating the last.
    pub fn route_seq(&self, method: &str, path: &str, replies: Vec<Reply>) {
        let i = Mutex::new(0usize);
        self.route(method, path, move |_| {
            let mut i = i.lock().unwrap();
            let r = replies[(*i).min(replies.len() - 1)].clone();
            *i += 1;
            r
        });
    }

    pub fn seen(&self, method: &str, path: &str) -> Vec<Seen> {
        self.state
            .seen
            .lock()
            .unwrap()
            .iter()
            .filter(|s| s.method == method && s.path == path)
            .cloned()
            .collect()
    }

    pub fn count(&self, method: &str, path: &str) -> usize {
        self.seen(method, path).len()
    }
}

impl Drop for FakeServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
    }
}

fn handle(mut conn: TcpStream, state: &State) {
    let _ = conn.set_read_timeout(Some(Duration::from_secs(10)));
    let mut buf = Vec::new();
    let mut tmp = [0u8; 8192];
    let head_end = loop {
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i;
        }
        match conn.read(&mut tmp) {
            Ok(0) | Err(_) => return,
            Ok(n) => buf.extend_from_slice(&tmp[..n]),
        }
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
    let mut lines = head.split("\r\n");
    let first = lines.next().unwrap_or("");
    let mut parts = first.split(' ');
    let method = parts.next().unwrap_or("").to_string();
    let target = parts.next().unwrap_or("").to_string();
    let path = target.split('?').next().unwrap_or("").to_string();
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect();
    let len: usize = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0);
    let mut body = buf[head_end + 4..].to_vec();
    while body.len() < len {
        match conn.read(&mut tmp) {
            Ok(0) | Err(_) => return,
            Ok(n) => body.extend_from_slice(&tmp[..n]),
        }
    }
    let seen = Seen {
        method: method.clone(),
        path: path.clone(),
        headers,
        body,
    };
    let handler = state.routes.lock().unwrap().get(&(method, path)).cloned();
    state.seen.lock().unwrap().push(seen.clone());
    let reply = match handler {
        Some(h) => h(&seen),
        None => Reply::empty(404),
    };
    std::thread::sleep(reply.stall);
    let total: usize = reply.chunks.iter().map(Vec::len).sum();
    let mut out = format!("HTTP/1.1 {} X\r\n", reply.status);
    match reply.framing {
        Framing::Chunked => out.push_str("Transfer-Encoding: chunked\r\n"),
        Framing::Length => out.push_str(&format!("Content-Length: {total}\r\n")),
        Framing::Close => {}
    }
    for (k, v) in &reply.headers {
        out.push_str(&format!("{k}: {v}\r\n"));
    }
    out.push_str("\r\n");
    if conn.write_all(out.as_bytes()).is_err() {
        return;
    }
    for c in &reply.chunks {
        std::thread::sleep(reply.delay);
        if c.is_empty() {
            continue;
        }
        let bytes = if reply.framing == Framing::Chunked {
            let mut chunk = format!("{:x}\r\n", c.len()).into_bytes();
            chunk.extend_from_slice(c);
            chunk.extend_from_slice(b"\r\n");
            chunk
        } else {
            c.clone()
        };
        if conn.write_all(&bytes).is_err() {
            return;
        }
        let _ = conn.flush();
    }
    if !reply.cut && reply.framing == Framing::Chunked {
        let _ = conn.write_all(b"0\r\n\r\n");
    }
    let _ = conn.flush();
    let _ = conn.shutdown(std::net::Shutdown::Both);
}

// ------------------------------------------------ the fake sign-in service

/// Distinctive token values, so tests can assert they never leak.
pub const ISSUED_CLIENT: &str = "oaiapp_test_client";
pub const SECRET_MARKS: &[&str] = &["AT-SECRET", "RT-SECRET", "CODE-SECRET", "eyJ"];

pub const T0: u64 = 1_900_000_000;

/// What the fake browser does with the authorization URL.
#[derive(Clone, Debug)]
pub enum Browser {
    /// Approve; the callback carries this `client_id` (or none).
    Approve {
        client_id: Option<String>,
        scope: String,
    },
    Deny,
    /// Only answers that carry a wrong or no state.
    WrongState,
    /// Stray answers first (wrong state, no state), then approve.
    StrayThenApprove,
    /// Open a connection to the callback and send nothing.
    Hold,
    /// Never come back.
    Nothing,
    /// Hit another path first, then approve.
    FaviconThenApprove,
}

impl Browser {
    pub fn approve_first() -> Browser {
        Browser::Approve {
            client_id: Some(ISSUED_CLIENT.into()),
            scope: SCOPES.into(),
        }
    }
    pub fn approve_again() -> Browser {
        Browser::Approve {
            client_id: None,
            scope: SCOPES.into(),
        }
    }
}

/// Shared state of the fake authorization server.
#[derive(Default)]
pub struct AuthState {
    /// The parameters of each authorize URL opened.
    pub authorize: Vec<HashMap<String, String>>,
    /// Refresh token generation: the valid refresh token is RT-SECRET-<n>.
    pub generation: u32,
    /// When set, the token endpoint answers this (status, body).
    pub token_override: Option<(u16, Value)>,
    pub refresh_override: Option<(u16, Value)>,
    /// When set, refresh answers only after this long.
    pub refresh_stall: Duration,
    pub id_email: Option<String>,
    /// The scopes the user granted in the fake browser.
    pub granted: Option<String>,
    /// Refresh replies carry a new ID token (with this email).
    pub refresh_id_email: Option<String>,
    /// Lifetime of issued access tokens (0 means 3600).
    pub expires_in: u64,
}

pub struct Fixture {
    pub server: FakeServer,
    pub store: Arc<MemoryStore>,
    pub auth: Arc<Auth>,
    pub clock: Arc<AtomicU64>,
    pub state: Arc<Mutex<AuthState>>,
}

pub fn get(url: &str) {
    let rest = url.strip_prefix("http://").unwrap();
    let (authority, path) = rest.split_at(rest.find('/').unwrap());
    let mut s = TcpStream::connect(authority).unwrap();
    write!(s, "GET {path} HTTP/1.1\r\nHost: {authority}\r\n\r\n").unwrap();
    let mut out = Vec::new();
    let _ = s.read_to_end(&mut out);
}

impl Default for Fixture {
    fn default() -> Self {
        Fixture::new()
    }
}

impl Fixture {
    pub fn new() -> Fixture {
        Fixture::with_config(|_| {})
    }

    pub fn with_config(tune: impl FnOnce(&mut AuthConfig)) -> Fixture {
        let server = FakeServer::start();
        let signer = Arc::new(Signer::new("kid-1"));
        server.route_json("GET", "/jwks", 200, signer.jwks());
        let state = Arc::new(Mutex::new(AuthState {
            id_email: Some("pat@example.test".into()),
            ..Default::default()
        }));
        let clock = Arc::new(AtomicU64::new(T0));
        {
            let (state, signer, clock) = (state.clone(), signer.clone(), clock.clone());
            server.route("POST", "/token", move |seen| {
                token_endpoint(seen, &state, &signer, &clock)
            });
        }
        server.route("POST", "/revoke", |_| Reply::empty(200));
        let mut config = AuthConfig {
            issuer: "https://issuer.test".into(),
            authorize_url: server.url("/authorize"),
            token_url: server.url("/token"),
            revoke_url: server.url("/revoke"),
            jwks_url: server.url("/jwks"),
            preferred_port: 0,
            callback_timeout: Duration::from_secs(10),
            http: HttpConfig {
                connect_timeout: Duration::from_secs(5),
                read_timeout: Duration::from_secs(10),
            },
            revoke_retry_delays: vec![Duration::from_millis(10), Duration::from_millis(10)],
            ..AuthConfig::openai()
        };
        tune(&mut config);
        let store = Arc::new(MemoryStore::new());
        let c2 = clock.clone();
        let auth = Arc::new(
            Auth::new(config, store.clone())
                .with_clock(Arc::new(move || c2.load(Ordering::SeqCst))),
        );
        Fixture {
            server,
            store,
            auth,
            clock,
            state,
        }
    }

    pub fn set_time(&self, t: u64) {
        self.clock.store(t, Ordering::SeqCst);
    }

    /// A browser function for `Auth::sign_in`.
    pub fn browser(&self, how: Browser) -> impl Fn(&str) -> Result<(), String> + Sync {
        let state = self.state.clone();
        move |url: &str| {
            let query = url.split_once('?').map(|(_, q)| q).unwrap_or("");
            let params: HashMap<String, String> = parse_query(query).into_iter().collect();
            {
                let mut st = state.lock().unwrap();
                st.authorize.push(params.clone());
                st.granted = match &how {
                    Browser::Approve { scope, .. } => Some(scope.clone()),
                    _ => Some(SCOPES.into()),
                };
            }
            let redirect = params["redirect_uri"].clone();
            let st = params["state"].clone();
            let nonce = params["nonce"].clone();
            let how = how.clone();
            std::thread::spawn(move || {
                let cb = |extra: &str| format!("{redirect}?state={st}{extra}");
                match how {
                    Browser::Nothing => {}
                    Browser::Deny => get(&cb("&error=access_denied&error_description=no")),
                    Browser::WrongState => {
                        get(&format!("{redirect}?state=forged&code=CODE-SECRET.{nonce}"));
                        get(&format!("{redirect}?code=CODE-SECRET.{nonce}"));
                    }
                    Browser::StrayThenApprove => {
                        get(&format!("{redirect}?state=forged&code=CODE-SECRET.{nonce}"));
                        get(&format!("{redirect}?error=access_denied"));
                        get(&cb(&format!(
                            "&code=CODE-SECRET.{nonce}&client_id={ISSUED_CLIENT}&scope={}",
                            crate::util::form_encode(SCOPES)
                        )));
                    }
                    Browser::Hold => {
                        let rest = redirect.strip_prefix("http://").unwrap();
                        let authority = &rest[..rest.find('/').unwrap()];
                        let s = TcpStream::connect(authority).unwrap();
                        std::thread::sleep(Duration::from_secs(5));
                        drop(s);
                    }
                    Browser::FaviconThenApprove => {
                        let base = redirect
                            .rsplit_once('/')
                            .unwrap()
                            .0
                            .rsplit_once('/')
                            .unwrap()
                            .0;
                        get(&format!("{base}/favicon.ico"));
                        get(&cb(&format!(
                            "&code=CODE-SECRET.{nonce}&client_id={ISSUED_CLIENT}&scope={}",
                            crate::util::form_encode(SCOPES)
                        )));
                    }
                    Browser::Approve { client_id, scope } => {
                        let client = client_id
                            .map(|c| format!("&client_id={c}"))
                            .unwrap_or_default();
                        get(&cb(&format!(
                            "&code=CODE-SECRET.{nonce}{client}&scope={}",
                            crate::util::form_encode(&scope)
                        )));
                    }
                }
            });
            Ok(())
        }
    }

    /// Sign in with a first-time approval; panics on failure.
    pub fn signed_in(self) -> Fixture {
        let b = self.browser(Browser::approve_first());
        self.auth
            .sign_in(&b, &crate::util::CancelToken::new())
            .expect("sign-in");
        self
    }

    /// Another Auth on the same store, as a second process or a restart:
    /// its signing keys are not cached yet.
    pub fn second_auth(&self) -> Arc<Auth> {
        let c2 = self.clock.clone();
        Arc::new(
            Auth::new(self.auth.config().clone(), self.store.clone())
                .with_clock(Arc::new(move || c2.load(Ordering::SeqCst))),
        )
    }

    /// The callback port of authorize request `i`.
    pub fn callback_port(&self, i: usize) -> u16 {
        self.authorize(i)["redirect_uri"]
            .trim_start_matches("http://127.0.0.1:")
            .split('/')
            .next()
            .unwrap()
            .parse()
            .unwrap()
    }

    pub fn authorize(&self, i: usize) -> HashMap<String, String> {
        self.state.lock().unwrap().authorize[i].clone()
    }

    pub fn stored_session(&self) -> Value {
        use crate::store::{CredentialStore, Item};
        let s = self
            .store
            .read(Item::Session)
            .unwrap()
            .expect("a stored session");
        serde_json::from_str(s.expose()).unwrap()
    }
}

fn id_token(signer: &Signer, nonce: Option<&str>, now: u64, email: Option<&str>) -> String {
    let mut c = json!({"iss": "https://issuer.test", "aud": ISSUED_CLIENT, "sub": "user-1",
        "iat": now, "exp": now + 3600, "name": "Pat Example", "email_verified": true});
    if let Some(n) = nonce {
        c["nonce"] = json!(n);
    }
    if let Some(e) = email {
        c["email"] = json!(e);
    }
    signer.sign(&c)
}

fn token_endpoint(
    seen: &Seen,
    state: &Mutex<AuthState>,
    signer: &Signer,
    clock: &AtomicU64,
) -> Reply {
    let f = seen.form();
    let now = clock.load(Ordering::SeqCst);
    let mut st = state.lock().unwrap();
    let bad = |what: &str| Reply::json(400, &json!({"error": "invalid_request", "what": what}));
    if f.get("resource").map(String::as_str) != Some("https://api.openai.com/v1") {
        return bad("resource");
    }
    match f.get("grant_type").map(String::as_str) {
        Some("authorization_code") => {
            if let Some((s, v)) = &st.token_override {
                return Reply::json(*s, v);
            }
            let auth = st.authorize.last().cloned().unwrap_or_default();
            if f.get("client_id").map(String::as_str) != Some(ISSUED_CLIENT) {
                return bad("client_id");
            }
            if f.get("redirect_uri") != auth.get("redirect_uri") {
                return bad("redirect_uri");
            }
            let verifier = f.get("code_verifier").cloned().unwrap_or_default();
            let challenge = b64url(&crate::win32::sha256(verifier.as_bytes()));
            if Some(&challenge) != auth.get("code_challenge") {
                return bad("pkce");
            }
            let code = f.get("code").cloned().unwrap_or_default();
            let Some(nonce) = code.strip_prefix("CODE-SECRET.") else {
                return bad("code");
            };
            st.generation += 1;
            let g = st.generation;
            Reply::json(
                200,
                &json!({"access_token": format!("AT-SECRET-{g}"), "refresh_token": format!("RT-SECRET-{g}"),
                    "id_token": id_token(signer, Some(nonce), now, st.id_email.as_deref()),
                    "token_type": "Bearer", "expires_in": if st.expires_in == 0 { 3600 } else { st.expires_in },
                    "scope": st.granted.clone().unwrap_or_default()}),
            )
        }
        Some("refresh_token") => {
            let stall = st.refresh_stall;
            if let Some((s, v)) = st.refresh_override.clone() {
                drop(st);
                std::thread::sleep(stall);
                return Reply::json(s, &v);
            }
            if f.contains_key("scope") {
                return bad("scope must be omitted");
            }
            if f.get("client_id").map(String::as_str) != Some(ISSUED_CLIENT) {
                return bad("client_id");
            }
            let expected = format!("RT-SECRET-{}", st.generation);
            if f.get("refresh_token") != Some(&expected) {
                return Reply::json(400, &json!({"error": "refresh_token_reused"}));
            }
            st.generation += 1;
            let g = st.generation;
            let id_email = st.refresh_id_email.clone();
            let expires_in = if st.expires_in == 0 {
                3600
            } else {
                st.expires_in
            };
            drop(st);
            std::thread::sleep(stall);
            let mut body = json!({"access_token": format!("AT-SECRET-{g}"), "refresh_token": format!("RT-SECRET-{g}"),
                "token_type": "Bearer", "expires_in": expires_in, "scope": SCOPES});
            if let Some(e) = id_email {
                body["id_token"] = json!(id_token(signer, None, now, Some(&e)));
            }
            Reply::json(200, &body)
        }
        _ => bad("grant_type"),
    }
}

/// Assert that none of the secret values appear in `text`.
pub fn assert_no_secrets(text: &str) {
    for m in SECRET_MARKS {
        assert!(!text.contains(m), "secret mark {m:?} leaked in: {text}");
    }
}

// ---------------------------------------------------- streamed replies

/// One SSE event.
pub fn ev(v: Value) -> String {
    format!("event: {}\ndata: {}\n\n", v["type"].as_str().unwrap(), v)
}

pub fn completed(input: u64, cached: u64, output: u64) -> String {
    ev(
        json!({"type": "response.completed", "response": {"id": "resp_1", "status": "completed",
        "usage": {"input_tokens": input, "input_tokens_details": {"cached_tokens": cached},
            "output_tokens": output, "output_tokens_details": {"reasoning_tokens": 2}}}}),
    )
}

/// A message streamed in two deltas, at `index`.
pub fn message_events(index: u64, text: &str) -> Vec<String> {
    let (a, b) = text.split_at(text.len() / 2);
    vec![
        ev(
            json!({"type": "response.output_item.added", "output_index": index,
            "item": {"type": "message", "id": "msg_1", "role": "assistant", "content": []}}),
        ),
        ev(
            json!({"type": "response.output_text.delta", "output_index": index, "content_index": 0,
            "item_id": "msg_1", "delta": a}),
        ),
        ev(
            json!({"type": "response.output_text.delta", "output_index": index, "content_index": 0,
            "item_id": "msg_1", "delta": b}),
        ),
        ev(
            json!({"type": "response.output_text.done", "output_index": index, "content_index": 0,
            "item_id": "msg_1", "text": text}),
        ),
        ev(
            json!({"type": "response.output_item.done", "output_index": index,
            "item": {"type": "message", "id": "msg_1", "role": "assistant", "status": "completed",
                "content": [{"type": "output_text", "text": text, "annotations": []}]}}),
        ),
    ]
}

/// A function call whose arguments arrive in three pieces, at `index`.
pub fn call_events(index: u64, call_id: &str, name: &str, args: &str) -> Vec<String> {
    let item_id = format!("fc_{call_id}");
    let third = args.len() / 3;
    let pieces = [&args[..third], &args[third..2 * third], &args[2 * third..]];
    let mut out = vec![ev(
        json!({"type": "response.output_item.added", "output_index": index,
        "item": {"type": "function_call", "id": item_id, "call_id": call_id, "name": name,
            "namespace": "openxer", "arguments": "", "status": "in_progress"}}),
    )];
    for p in pieces {
        out.push(ev(json!({"type": "response.function_call_arguments.delta",
            "output_index": index, "item_id": item_id, "delta": p})));
    }
    out.push(ev(
        json!({"type": "response.function_call_arguments.done", "output_index": index,
        "item_id": item_id, "arguments": args}),
    ));
    out.push(ev(
        json!({"type": "response.output_item.done", "output_index": index,
        "item": {"type": "function_call", "id": item_id, "call_id": call_id, "name": name,
            "namespace": "openxer", "arguments": args, "status": "completed"}}),
    ));
    out
}

pub fn reasoning_events(index: u64) -> Vec<String> {
    vec![
        ev(
            json!({"type": "response.output_item.added", "output_index": index,
            "item": {"type": "reasoning", "id": "rs_1", "summary": []}}),
        ),
        ev(
            json!({"type": "response.output_item.done", "output_index": index,
            "item": {"type": "reasoning", "id": "rs_1", "summary": [], "encrypted_content": "ENC"}}),
        ),
    ]
}

pub fn text_reply(text: &str) -> Reply {
    let mut e = vec![ev(
        json!({"type": "response.created", "response": {"id": "resp_1"}}),
    )];
    e.extend(message_events(0, text));
    e.push(completed(100, 40, 10));
    Reply::sse(&e)
}

pub fn calls_reply(calls: &[(&str, &str, &str)]) -> Reply {
    let mut e = reasoning_events(0);
    for (i, (id, name, args)) in calls.iter().enumerate() {
        e.extend(call_events(i as u64 + 1, id, name, args));
    }
    e.push(completed(150, 0, 20));
    Reply::sse(&e)
}

/// Streamed replies of an OpenAI-compatible Chat Completions server, for
/// the endpoint route's tests.
pub mod completions {
    use super::Reply;
    use serde_json::{json, Value};

    pub fn chunk(v: Value) -> String {
        format!("data: {v}\n\n")
    }

    pub fn done() -> String {
        "data: [DONE]\n\n".to_string()
    }

    pub fn text_chunks(text: &str) -> Vec<String> {
        let (a, b) = text.split_at(text.len() / 2);
        vec![
            chunk(json!({"id": "c1", "object": "chat.completion.chunk",
                "choices": [{"index": 0, "delta": {"role": "assistant", "content": ""}, "finish_reason": null}]})),
            chunk(
                json!({"choices": [{"index": 0, "delta": {"content": a}, "finish_reason": null}]}),
            ),
            chunk(
                json!({"choices": [{"index": 0, "delta": {"content": b}, "finish_reason": null}]}),
            ),
            chunk(json!({"choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}]})),
        ]
    }

    pub fn usage_chunk(input: u64, cached: u64, output: u64) -> String {
        chunk(
            json!({"choices": [], "usage": {"prompt_tokens": input, "completion_tokens": output,
            "total_tokens": input + output, "prompt_tokens_details": {"cached_tokens": cached}}}),
        )
    }

    /// A streamed text answer, with usage when `usage`.
    pub fn text_reply(text: &str, usage: bool) -> Reply {
        let mut e = text_chunks(text);
        if usage {
            e.push(usage_chunk(100, 40, 10));
        }
        e.push(done());
        Reply::sse(&e)
    }

    /// Tool calls, arguments in three pieces each, the usual OpenAI shape.
    pub fn calls_reply(calls: &[(&str, &str, &str)]) -> Reply {
        let mut e = Vec::new();
        for (i, (id, name, args)) in calls.iter().enumerate() {
            e.push(chunk(
                json!({"choices": [{"index": 0, "delta": {"tool_calls": [{"index": i,
                "id": id, "type": "function", "function": {"name": name, "arguments": ""}}]}}]}),
            ));
            let third = args.len() / 3;
            for piece in [&args[..third], &args[third..2 * third], &args[2 * third..]] {
                e.push(chunk(
                    json!({"choices": [{"index": 0, "delta": {"tool_calls": [{"index": i,
                    "function": {"arguments": piece}}]}}]}),
                ));
            }
        }
        e.push(chunk(
            json!({"choices": [{"index": 0, "delta": {}, "finish_reason": "tool_calls"}]}),
        ));
        e.push(usage_chunk(150, 0, 20));
        e.push(done());
        Reply::sse(&e)
    }
}
