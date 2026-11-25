//! The compatible-endpoint route against fake servers on 127.0.0.1 (plain
//! HTTP). No test reaches a real host.

use super::*;
use crate::client::{ChatError, ChatEvent, TurnRequest};
use crate::conversation::{Conversation, BASE_INSTRUCTIONS};
use crate::store::MemoryStore;
use crate::testkit::{FakeServer, Framing, Reply, Seen};
use crate::tools::{ProposedChanges, ToolDef, ToolOutput, Tools};
use std::sync::Mutex;
use std::time::Instant;

const KEY: &str = "sk-TESTKEY-0123456789";

use crate::testkit::completions::*;

struct FakeTools {
    calls: Mutex<Vec<(String, Value)>>,
}

fn tools() -> Arc<FakeTools> {
    Arc::new(FakeTools {
        calls: Mutex::new(Vec::new()),
    })
}

impl Tools for FakeTools {
    fn definitions(&self) -> Vec<ToolDef> {
        ["slow", "find", "big", "propose"]
            .iter()
            .map(|n| ToolDef {
                name: n.to_string(),
                description: format!("The {n} tool."),
                parameters: json!({"type": "object", "properties": {"query": {"type": "string"}}}),
            })
            .collect()
    }
    fn call(&self, name: &str, args: &Value, cancel: &CancelToken) -> ToolOutput {
        self.calls
            .lock()
            .unwrap()
            .push((name.to_string(), args.clone()));
        match name {
            "find" => ToolOutput::ok(format!("found {}", args["query"].as_str().unwrap_or(""))),
            "big" => ToolOutput::ok("x".repeat(20_000)),
            "slow" => {
                cancel.sleep(Duration::from_secs(5));
                ToolOutput::ok("slow done")
            }
            "propose" => ToolOutput::proposed(
                "Proposed.",
                ProposedChanges {
                    summary: "Change A1010".into(),
                    payload: json!({}),
                },
            ),
            _ => ToolOutput::error("unknown tool"),
        }
    }
}

fn base(server: &FakeServer) -> String {
    let u = server.url("/v1");
    format!("http://{}:{}/v1", u.host, u.port)
}

fn conn(server: &FakeServer, preset: Preset) -> Connection {
    let mut c = Connection::new(preset);
    c.base_url = base(server);
    c.model = "model-a".into();
    c
}

fn fast() -> CompatConfig {
    CompatConfig {
        http: HttpConfig {
            connect_timeout: Duration::from_secs(6),
            read_timeout: Duration::from_secs(10),
        },
        retry_delays: vec![Duration::from_millis(10), Duration::from_millis(10)],
        max_retry_after: Duration::from_millis(50),
        ..CompatConfig::default()
    }
}

struct Setup {
    server: FakeServer,
    store: Arc<MemoryStore>,
    client: Arc<CompatClient>,
}

fn setup(preset: Preset, with_key: bool) -> Setup {
    let server = FakeServer::start();
    let store = Arc::new(MemoryStore::new());
    let c = conn(&server, preset);
    if with_key {
        store_key(&*store, &c, &Secret::new(KEY)).unwrap();
    }
    let client = Arc::new(CompatClient::with_config(c, store.clone(), fast()).unwrap());
    Setup {
        server,
        store,
        client,
    }
}

fn request(text: &str) -> TurnRequest {
    TurnRequest {
        model: String::new(),
        context: "Project RIVERBEND.".into(),
        user_text: text.into(),
        reasoning_effort: None,
    }
}

impl Setup {
    fn turn(
        &self,
        conv: &mut Conversation,
        text: &str,
        tools: Arc<dyn Tools>,
    ) -> (Result<Usage, ChatError>, Vec<ChatEvent>) {
        let (tx, rx) = mpsc::channel();
        let r = self
            .client
            .run_turn(conv, &request(text), &tools, &tx, &CancelToken::new());
        drop(tx);
        (r, rx.into_iter().collect())
    }
    fn requests(&self) -> Vec<Seen> {
        self.server.seen("POST", "/v1/chat/completions")
    }
    fn reply(&self, replies: Vec<Reply>) {
        self.server
            .route_seq("POST", "/v1/chat/completions", replies);
    }
}

fn assert_no_key(text: &str) {
    assert!(!text.contains("TESTKEY"), "the key leaked in: {text}");
}

#[test]
fn a_streamed_answer_with_usage() {
    let s = setup(Preset::OpenAi, true);
    s.reply(vec![text_reply("Hello world", true)]);
    let mut conv = s.client.connection().new_conversation();
    let (r, events) = s.turn(&mut conv, "Hi", tools());
    let u = r.unwrap();
    assert!(u.is_known());
    assert_eq!(
        (u.input_tokens, u.cached_tokens, u.output_tokens, u.requests),
        (100, 40, 10, 1)
    );
    assert_eq!(
        events,
        vec![
            ChatEvent::TextDelta("Hello".into()),
            ChatEvent::TextDelta(" world".into()),
            ChatEvent::Message("Hello world".into()),
            ChatEvent::TurnFinished(u),
        ]
    );
    let req = &s.requests()[0];
    assert_eq!(req.header("Authorization"), Some(&*format!("Bearer {KEY}")));
    let b = req.json();
    assert_eq!(b["model"], "model-a");
    assert_eq!(b["stream"], true);
    assert_eq!(b["stream_options"], json!({"include_usage": true}));
    assert_eq!(b["messages"][0]["role"], "system");
    assert!(b["messages"][0]["content"]
        .as_str()
        .unwrap()
        .starts_with(BASE_INSTRUCTIONS));
    assert_eq!(b["messages"][1], json!({"role": "user", "content": "Hi"}));
    let names: Vec<&str> = b["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["function"]["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["big", "find", "propose", "slow"], "sorted, stable");
    assert_eq!(b["tools"][0]["type"], "function");
}

#[test]
fn usage_the_server_does_not_give_is_unknown_never_made_up() {
    let s = setup(Preset::Ollama, false);
    s.reply(vec![text_reply("Hi there", false)]);
    let (r, events) = s.turn(&mut Conversation::new(), "Hi", tools());
    let u = r.unwrap();
    assert!(!u.is_known());
    assert_eq!(u, Usage::default());
    assert_eq!(
        events.last(),
        Some(&ChatEvent::TurnFinished(Usage::default()))
    );
    // No key stored and none needed: no Authorization header.
    assert!(s.requests()[0].header("Authorization").is_none());
    // A turn where one round of two lacks usage is unknown too.
    let s = setup(Preset::OpenAi, true);
    s.reply(vec![
        calls_reply(&[("call_1", "find", r#"{"query":"A1"}"#)]),
        text_reply("done", false),
    ]);
    let u = s.turn(&mut Conversation::new(), "Go", tools()).0.unwrap();
    assert!(!u.is_known());
}

#[test]
fn the_tool_loop_with_arguments_in_pieces_and_several_calls() {
    let s = setup(Preset::OpenAi, true);
    s.reply(vec![
        calls_reply(&[
            ("call_a", "find", r#"{"query":"A1010"}"#),
            ("call_b", "find", r#"{"query":"A2020"}"#),
        ]),
        text_reply("Both found.", true),
    ]);
    let t = tools();
    let mut conv = Conversation::new();
    let (r, events) = s.turn(&mut conv, "Find two", t.clone());
    let u = r.unwrap();
    assert_eq!((u.requests, u.input_tokens), (2, 250));
    let calls: Vec<(String, Value)> = t.calls.lock().unwrap().clone();
    assert_eq!(
        calls,
        [
            ("find".to_string(), json!({"query": "A1010"})),
            ("find".to_string(), json!({"query": "A2020"}))
        ]
    );
    assert!(events.iter().any(
        |e| matches!(e, ChatEvent::ToolStarted { id, target, .. } if id == "call_b" && target == "A2020")
    ));
    let m = s.requests()[1].json()["messages"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(m[2]["role"], "assistant");
    assert_eq!(m[2]["tool_calls"][0]["id"], "call_a");
    assert_eq!(
        m[2]["tool_calls"][1]["function"],
        json!({"name": "find", "arguments": "{\"query\":\"A2020\"}"})
    );
    assert_eq!(
        m[3],
        json!({"role": "tool", "tool_call_id": "call_a", "content": "found A1010"})
    );
    assert_eq!(m[4]["tool_call_id"], "call_b");
    // The second turn sends the whole history again, in the same order.
    s.reply(vec![text_reply("Again.", true)]);
    s.turn(&mut conv, "More", tools()).0.unwrap();
    let m2 = s.requests()[2].json()["messages"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(&m2[..m.len()], &m[..]);
}

#[test]
fn dialect_tolerances() {
    // Missing ids, no index, arguments as an object, reasoning text,
    // OpenRouter comment lines, usage-only final chunk.
    let s = setup(Preset::OpenRouter, true);
    let e = vec![
        ": OPENROUTER PROCESSING\n\n".to_string(),
        chunk(
            json!({"choices": [{"delta": {"reasoning_content": "thinking hard", "reasoning": "more"}}]}),
        ),
        chunk(
            json!({"choices": [{"delta": {"tool_calls": [{"function": {"name": "find",
            "arguments": {"query": "A1"}}}]}}]}),
        ),
        chunk(
            json!({"choices": [{"delta": {"tool_calls": [{"index": 5, "function": {"name": "find",
            "arguments": "{\"query\":"}}]}}]}),
        ),
        chunk(
            json!({"choices": [{"delta": {"tool_calls": [{"index": 5, "function": {"arguments": "\"A2\"}"}}]}}]}),
        ),
        chunk(json!({"choices": [{"delta": {}, "finish_reason": "tool_calls"}]})),
        chunk(json!({"usage": {"prompt_tokens": 5, "completion_tokens": 1}})),
        done(),
    ];
    s.reply(vec![Reply::sse(&e), text_reply("ok", true)]);
    let t = tools();
    let (r, events) = s.turn(&mut Conversation::new(), "Go", t.clone());
    r.unwrap();
    assert_eq!(
        t.calls.lock().unwrap().clone(),
        [
            ("find".to_string(), json!({"query": "A1"})),
            ("find".to_string(), json!({"query": "A2"}))
        ]
    );
    assert!(
        !format!("{events:?}").contains("thinking"),
        "reasoning is not shown"
    );
    let m = s.requests()[1].json()["messages"]
        .as_array()
        .unwrap()
        .clone();
    let ids: Vec<&str> = m[2]["tool_calls"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["call_1", "call_2"], "made-up ids");
    assert_eq!(m[3]["tool_call_id"], "call_1");
    assert_eq!(m[4]["tool_call_id"], "call_2");
    // Repeated ids are made unique.
    let s = setup(Preset::Custom, true);
    s.reply(vec![
        calls_reply(&[
            ("dup", "find", r#"{"query":"x"}"#),
            ("dup", "find", r#"{"query":"y"}"#),
        ]),
        text_reply("ok", true),
    ]);
    s.turn(&mut Conversation::new(), "Go", tools()).0.unwrap();
    let m = s.requests()[1].json()["messages"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(m[2]["tool_calls"][0]["id"], "dup");
    assert_eq!(m[2]["tool_calls"][1]["id"], "dup_2");
    assert_eq!(m[4]["tool_call_id"], "dup_2");
}

#[test]
fn a_server_that_does_not_stream_answers_with_one_body() {
    let s = setup(Preset::LmStudio, false);
    s.reply(vec![
        Reply::json(
            200,
            &json!({"choices": [{"index": 0, "finish_reason": "tool_calls", "message": {
                "role": "assistant", "content": null, "tool_calls": [{"id": "t1", "type": "function",
                "function": {"name": "find", "arguments": "{\"query\":\"Q\"}"}}]}}],
                "usage": {"prompt_tokens": 7, "completion_tokens": 3}}),
        ),
        Reply::json(
            200,
            &json!({"choices": [{"index": 0, "finish_reason": "stop",
                "message": {"role": "assistant", "content": "All done."}}]}),
        )
        .framed(Framing::Length),
    ]);
    let t = tools();
    let (r, events) = s.turn(&mut Conversation::new(), "Go", t.clone());
    let u = r.unwrap();
    assert!(!u.is_known(), "the second reply had no usage");
    assert_eq!(t.calls.lock().unwrap()[0].1, json!({"query": "Q"}));
    assert!(events.contains(&ChatEvent::Message("All done.".into())));
    assert!(events.contains(&ChatEvent::TextDelta("All done.".into())));
}

#[test]
fn ends_of_streams() {
    // A finish reason without [DONE], and close framing: fine.
    let s = setup(Preset::Ollama, false);
    s.reply(vec![
        Reply::sse(&text_chunks("no done")).framed(Framing::Close)
    ]);
    s.turn(&mut Conversation::new(), "Hi", tools()).0.unwrap();
    // Text and then the end, with neither: accepted.
    let s = setup(Preset::Ollama, false);
    let mut e = text_chunks("bare");
    e.pop();
    s.reply(vec![Reply::sse(&e)]);
    s.turn(&mut Conversation::new(), "Hi", tools()).0.unwrap();
    // Cut off in the middle of an event.
    let s = setup(Preset::Ollama, false);
    let mut e = text_chunks("cut");
    e.pop();
    let half = e[2].len() / 2;
    e[2].truncate(half);
    let mut reply = Reply::sse(&e);
    reply.cut = true;
    s.reply(vec![reply]);
    let err = s
        .turn(&mut Conversation::new(), "Hi", tools())
        .0
        .unwrap_err();
    assert!(
        matches!(err, ChatError::BadResponse(_) | ChatError::Network(_)),
        "{err:?}"
    );
    // Nothing at all.
    let s = setup(Preset::Ollama, false);
    s.reply(vec![Reply::sse(&[])]);
    assert!(matches!(
        s.turn(&mut Conversation::new(), "Hi", tools())
            .0
            .unwrap_err(),
        ChatError::BadResponse(_)
    ));
    // An error in the stream (OpenRouter style), with the key echoed.
    let s = setup(Preset::OpenRouter, true);
    s.reply(vec![Reply::sse(&[
        chunk(json!({"choices": [{"delta": {"content": "par"}}]})),
        chunk(
            json!({"error": {"code": 502, "message": format!("upstream failed for {KEY}")},
            "choices": [{"delta": {}, "finish_reason": "error"}]}),
        ),
    ])]);
    let err = s
        .turn(&mut Conversation::new(), "Hi", tools())
        .0
        .unwrap_err();
    assert!(
        matches!(&err, ChatError::Api { code, .. } if code == "502"),
        "{err:?}"
    );
    assert_no_key(&format!("{err:?} {err}"));
    // The length limit.
    let s = setup(Preset::OpenAi, true);
    s.reply(vec![Reply::sse(&[
        chunk(json!({"choices": [{"delta": {"content": "long"}, "finish_reason": "length"}]})),
        done(),
    ])]);
    assert!(matches!(
        s.turn(&mut Conversation::new(), "Hi", tools())
            .0
            .unwrap_err(),
        ChatError::Incomplete { .. }
    ));
}

#[test]
fn stream_options_are_dropped_when_the_server_rejects_them() {
    let s = setup(Preset::Custom, true);
    s.reply(vec![
        Reply::json(
            400,
            &json!({"error": {"message": "Unrecognized request argument: stream_options"}}),
        ),
        text_reply("ok", false),
        text_reply("ok", false),
    ]);
    let mut conv = Conversation::new();
    s.turn(&mut conv, "Hi", tools()).0.unwrap();
    s.turn(&mut conv, "Again", tools()).0.unwrap();
    let r = s.requests();
    assert_eq!(r.len(), 3);
    assert!(r[0].json().get("stream_options").is_some());
    assert!(r[1].json().get("stream_options").is_none());
    assert!(r[2].json().get("stream_options").is_none());
    // A 400 without stream_options to blame is an error.
    let s = setup(Preset::Custom, true);
    s.reply(vec![Reply::json(
        400,
        &json!({"error": {"message": "bad"}}),
    )]);
    assert!(matches!(
        s.turn(&mut Conversation::new(), "Hi", tools())
            .0
            .unwrap_err(),
        ChatError::Api { status: 400, .. }
    ));
    assert_eq!(s.requests().len(), 2);
}

#[test]
fn the_round_cap_and_proposals() {
    let s = setup(Preset::OpenAi, true);
    s.reply(vec![calls_reply(&[("c", "find", r#"{"query":"z"}"#)])]);
    let mut conv = Conversation::new().with_budgets(Budgets {
        max_rounds: 3,
        ..Default::default()
    });
    let (r, _) = s.turn(&mut conv, "Loop", tools());
    assert_eq!(r.unwrap_err(), ChatError::RoundCap { rounds: 3 });
    let req = s.requests();
    assert_eq!(req.len(), 3);
    assert!(req[0].json().get("tools").is_some());
    assert!(
        req[2].json().get("tools").is_none(),
        "the last round has no tools"
    );
    assert!(conv.is_empty());
    let s = setup(Preset::OpenAi, true);
    s.reply(vec![
        calls_reply(&[("p", "propose", "{}")]),
        text_reply("Review it.", true),
    ]);
    let (_, events) = s.turn(&mut Conversation::new(), "Change", tools());
    assert!(events
        .iter()
        .any(|e| matches!(e, ChatEvent::ProposedChanges { id, .. } if id == "p")));
}

#[test]
fn cancel_during_the_stream_and_during_a_tool() {
    let s = setup(Preset::Ollama, false);
    let mut reply = text_reply("a slow answer here", false);
    reply.delay = Duration::from_millis(400);
    s.reply(vec![reply]);
    let h = s
        .client
        .start_turn(Conversation::new(), request("Go"), tools());
    let first = h.events.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(matches!(first, ChatEvent::TextDelta(_)), "{first:?}");
    let t = Instant::now();
    h.cancel();
    let rest: Vec<ChatEvent> = h.events.iter().collect();
    assert_eq!(rest.last(), Some(&ChatEvent::Cancelled));
    assert!(
        t.elapsed() < Duration::from_millis(400),
        "{:?}",
        t.elapsed()
    );
    assert!(h.join().is_empty());

    let s = setup(Preset::Ollama, false);
    s.reply(vec![
        calls_reply(&[("s", "slow", "{}")]),
        text_reply("never", false),
    ]);
    let h = s
        .client
        .start_turn(Conversation::new(), request("Go"), tools());
    loop {
        if matches!(
            h.events.recv_timeout(Duration::from_secs(5)).unwrap(),
            ChatEvent::ToolStarted { .. }
        ) {
            break;
        }
    }
    let t = Instant::now();
    h.cancel();
    let rest: Vec<ChatEvent> = h.events.iter().collect();
    assert_eq!(rest.last(), Some(&ChatEvent::Cancelled));
    assert!(t.elapsed() < Duration::from_millis(500));
    assert_eq!(s.requests().len(), 1);
}

#[test]
fn status_errors_and_retries() {
    let s = setup(Preset::OpenAi, true);
    s.reply(vec![Reply::json(
        401,
        &json!({"error": {"message": format!("bad key {KEY}")}}),
    )]);
    let e = s
        .turn(&mut Conversation::new(), "Hi", tools())
        .0
        .unwrap_err();
    assert_eq!(e, ChatError::KeyRejected { status: 401 });
    assert_eq!(s.requests().len(), 1, "no retry on a wrong key");
    assert_no_key(&format!("{e:?} {e}"));
    let s = setup(Preset::OpenAi, true);
    s.reply(vec![Reply::json(
        404,
        &json!({"error": {"message": "model not found"}}),
    )]);
    assert!(matches!(
        s.turn(&mut Conversation::new(), "Hi", tools()).0.unwrap_err(),
        ChatError::NotFound { message } if message == "model not found"
    ));
    let s = setup(Preset::OpenAi, true);
    s.reply(vec![
        Reply::json(429, &json!({})).header("Retry-After", "30"),
        Reply::json(503, &json!({})),
        text_reply("ok", true),
    ]);
    let t = Instant::now();
    s.turn(&mut Conversation::new(), "Hi", tools()).0.unwrap();
    assert_eq!(s.requests().len(), 3);
    assert!(t.elapsed() < Duration::from_secs(2));
    let s = setup(Preset::OpenAi, true);
    s.reply(vec![Reply::json(
        500,
        &json!({"error": {"message": "boom"}}),
    )]);
    assert!(matches!(
        s.turn(&mut Conversation::new(), "Hi", tools())
            .0
            .unwrap_err(),
        ChatError::Api { status: 500, .. }
    ));
    assert_eq!(s.requests().len(), 3);
    // A key is needed and none is stored: no request at all.
    let s = setup(Preset::OpenAi, false);
    assert_eq!(
        s.turn(&mut Conversation::new(), "Hi", tools())
            .0
            .unwrap_err(),
        ChatError::NoKey
    );
    assert!(s.requests().is_empty());
    // No model chosen.
    let s = setup(Preset::Ollama, false);
    let mut c = s.client.connection().clone();
    c.model.clear();
    let client = CompatClient::with_config(c, s.store.clone(), fast()).unwrap();
    let (tx, _rx) = mpsc::channel();
    let t: Arc<dyn Tools> = tools();
    assert!(matches!(
        client.run_turn(
            &mut Conversation::new(),
            &request("Hi"),
            &t,
            &tx,
            &CancelToken::new()
        ),
        Err(ChatError::NotFound { .. })
    ));
}

fn closed_port() -> u16 {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let p = l.local_addr().unwrap().port();
    drop(l);
    p
}

#[test]
fn nothing_running_locally_gives_the_start_hint() {
    let mut c = Connection::new(Preset::Ollama);
    c.base_url = format!("http://127.0.0.1:{}/v1", closed_port());
    c.model = "m".into();
    let store = Arc::new(MemoryStore::new());
    let client = CompatClient::with_config(c, store, fast()).unwrap();
    crate::http::FAKE_PROXY.with(|f| f.set(Some(false)));
    let (tx, _rx) = mpsc::channel();
    let t: Arc<dyn Tools> = tools();
    let e = client
        .run_turn(
            &mut Conversation::new(),
            &request("Hi"),
            &t,
            &tx,
            &CancelToken::new(),
        )
        .unwrap_err();
    crate::http::FAKE_PROXY.with(|f| f.set(None));
    match e {
        ChatError::ServerNotRunning { hint } => assert!(hint.contains("Start Ollama")),
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_address_rules() {
    let ok = [
        ("http://127.0.0.1:11434/v1", "127.0.0.1"),
        ("http://localhost:1234/v1/", "localhost"),
        ("HTTP://LOCALHOST:1234/v1", "localhost"),
        ("http://[::1]:8080/v1", "::1"),
        ("http://127.8.9.10/v1", "127.8.9.10"),
        ("https://api.openai.com/v1", "api.openai.com"),
        ("HTTPS://OpenRouter.ai/api/v1", "openrouter.ai"),
        ("https://[2001:db8::1]/v1", "2001:db8::1"),
    ];
    for (s, host) in ok {
        let u = check_address(s).unwrap_or_else(|e| panic!("{s}: {e}"));
        assert_eq!(u.host, host, "{s}");
        assert!(!u.path.ends_with('/'), "{s}");
    }
    assert_eq!(
        check_address("http://localhost:1234/v1/").unwrap().path,
        "/v1"
    );
    assert_eq!(check_address("http://[::1]:8080/v1").unwrap().port, 8080);
    let bad = [
        "http://api.openai.com/v1",
        "http://127.0.0.1.evil.example/v1",
        "http://localhost.evil.example/v1",
        "http://evil.example/127.0.0.1",
        "http://127.0.0.1@evil.example/v1",
        "https://user:pass@api.openai.com/v1",
        "http://user@127.0.0.1:11434/v1",
        "http://[2001:db8::1]/v1",
        "http://10.0.0.5:11434/v1",
        "ftp://127.0.0.1/v1",
        "127.0.0.1:11434/v1",
        "",
        "https://api.openai.com/v1?x=1",
        "https://api.openai.com/v1#f",
        "https://api.open ai.com/v1",
        "http://127.0.0.1%2eevil.example/v1",
        "http://[::1/v1",
    ];
    for s in bad {
        assert!(check_address(s).is_err(), "{s} must be refused");
    }
    assert!(is_loopback("::1") && is_loopback("127.0.0.1") && is_loopback("LocalHost"));
    assert!(!is_loopback("127.0.0.1.evil.example") && !is_loopback("0.0.0.0"));
    // Refused before any request: the client cannot be made.
    let mut c = Connection::new(Preset::Custom);
    c.base_url = "http://192.168.1.20:11434/v1".into();
    assert!(matches!(
        CompatClient::new(c.clone(), Arc::new(MemoryStore::new())),
        Err(ChatError::AddressNotAllowed(_))
    ));
    assert!(matches!(
        test_connection(&c, &MemoryStore::new(), &CancelToken::new()),
        TestOutcome::AddressNotAllowed(_)
    ));
}

#[test]
fn the_key_goes_only_to_the_configured_host() {
    let other = FakeServer::start();
    other.route_json("POST", "/v1/chat/completions", 200, json!({}));
    other.route_json("GET", "/v1/models", 200, json!({"data": []}));
    let elsewhere = base(&other);
    let s = setup(Preset::OpenAi, true);
    let moved = Reply::empty(302).header("Location", &format!("{elsewhere}/chat/completions"));
    s.reply(vec![moved]);
    let e = s
        .turn(&mut Conversation::new(), "Hi", tools())
        .0
        .unwrap_err();
    assert!(matches!(e, ChatError::Api { status: 302, .. }), "{e:?}");
    let moved = Reply::empty(301).header("Location", &format!("{elsewhere}/models"));
    s.server.route_seq("GET", "/v1/models", vec![moved]);
    assert!(s.client.list_models(&CancelToken::new()).is_err());
    assert_eq!(
        test_connection_with(
            s.client.connection(),
            &*s.store,
            &test_http_config(),
            &CancelToken::new()
        ),
        TestOutcome::ServerError { status: 301 }
    );
    assert_eq!(other.count("POST", "/v1/chat/completions"), 0);
    assert_eq!(other.count("GET", "/v1/models"), 0);
    // Every request that did go out went to the configured host only.
    for r in s.requests() {
        assert_eq!(r.header("Authorization"), Some(&*format!("Bearer {KEY}")));
    }
}

#[test]
fn the_key_is_stored_apart_and_never_shown() {
    let store = MemoryStore::new();
    let c = Connection::new(Preset::OpenRouter);
    assert!(!has_key(&store, &c).unwrap());
    store_key(&store, &c, &Secret::new(format!("  {KEY}\n"))).unwrap();
    assert!(has_key(&store, &c).unwrap());
    let item = c.key_item().unwrap();
    assert!(item.is_secret());
    assert_eq!(
        stored_key(&store, &c).unwrap().unwrap().expose(),
        KEY,
        "trimmed"
    );
    // Stored bound to the address it was entered for.
    let raw = store.read(item).unwrap().unwrap();
    assert!(raw.expose().ends_with(KEY));
    assert!(raw.expose().contains("https://openrouter.ai:443"));
    // Another connection has its own key.
    let c2 = Connection::new(Preset::OpenAi);
    assert_ne!(c2.id, c.id);
    assert!(!has_key(&store, &c2).unwrap());
    // Preferences data and Debug output carry no key.
    let prefs = c.to_json().to_string();
    assert_no_key(&prefs);
    assert_eq!(Connection::from_json(&c.to_json()).unwrap(), c);
    let s = setup(Preset::OpenAi, true);
    assert_no_key(&format!(
        "{:?} {:?} {:?} {:?}",
        s.client,
        s.client.connection(),
        store,
        item
    ));
    delete_key(&store, &c).unwrap();
    assert!(!has_key(&store, &c).unwrap());
    // An ID that cannot name a key.
    let mut bad = c.clone();
    bad.id = "has spaces".into();
    assert!(bad.key_item().is_none());
    assert!(store_key(&store, &bad, &Secret::new("k")).is_err());
    assert!(Connection::from_json(&bad.to_json()).is_none());
    assert!(KeyId::new(&"x".repeat(33)).is_none());
}

#[test]
fn test_connection_outcomes() {
    let check = |s: &Setup| test_connection(s.client.connection(), &*s.store, &CancelToken::new());
    // Ok, with the model count, and the key sent.
    let s = setup(Preset::OpenAi, true);
    s.server.route_json(
        "GET",
        "/v1/models",
        200,
        json!({"object": "list", "data": [{"id": "a"}, {"id": "b"}, {"id": "c"}]}),
    );
    assert_eq!(check(&s), TestOutcome::Ok { models: 3 });
    assert_eq!(
        s.server.seen("GET", "/v1/models")[0].header("Authorization"),
        Some(&*format!("Bearer {KEY}"))
    );
    // Wrong key, 401 and 403.
    for status in [401, 403] {
        let s = setup(Preset::OpenAi, true);
        s.server.route_json(
            "GET",
            "/v1/models",
            status,
            json!({"error": {"message": KEY}}),
        );
        let o = check(&s);
        assert_eq!(o, TestOutcome::WrongKey { status });
        assert_no_key(&format!("{o:?} {o}"));
    }
    // No key stored.
    let s = setup(Preset::OpenAi, false);
    assert_eq!(check(&s), TestOutcome::NoKey);
    assert_eq!(s.server.count("GET", "/v1/models"), 0);
    // Not found (no route).
    let s = setup(Preset::LmStudio, false);
    assert_eq!(check(&s), TestOutcome::NotFound);
    // Server error.
    let s = setup(Preset::Ollama, false);
    s.server.route_json("GET", "/v1/models", 500, json!({}));
    assert_eq!(check(&s), TestOutcome::ServerError { status: 500 });
    // OpenRouter: the key check, then the list.
    let s = setup(Preset::OpenRouter, true);
    s.server
        .route_json("GET", "/v1/key", 200, json!({"data": {"label": "x"}}));
    s.server
        .route_json("GET", "/v1/models", 200, json!({"data": [{"id": "m"}]}));
    assert_eq!(check(&s), TestOutcome::Ok { models: 1 });
    assert_eq!(
        s.server.seen("GET", "/v1/key")[0].header("Authorization"),
        Some(&*format!("Bearer {KEY}"))
    );
    let s = setup(Preset::OpenRouter, true);
    s.server.route_json("GET", "/v1/key", 401, json!({}));
    assert_eq!(check(&s), TestOutcome::WrongKey { status: 401 });
    // Anthropic: its own key headers on the model list.
    let s = setup(Preset::Anthropic, true);
    s.server.route_json(
        "GET",
        "/v1/models",
        200,
        json!({"data": [{"id": "claude"}]}),
    );
    assert_eq!(check(&s), TestOutcome::Ok { models: 1 });
    let seen = &s.server.seen("GET", "/v1/models")[0];
    assert_eq!(seen.header("x-api-key"), Some(KEY));
    assert_eq!(seen.header("anthropic-version"), Some("2023-06-01"));
    // Timeout.
    let s = setup(Preset::Ollama, false);
    let mut slow = Reply::json(200, &json!({"data": []}));
    slow.stall = Duration::from_secs(3);
    s.server.route("GET", "/v1/models", move |_| slow.clone());
    let cfg = HttpConfig {
        connect_timeout: Duration::from_secs(2),
        read_timeout: Duration::from_millis(300),
    };
    assert_eq!(
        test_connection_with(s.client.connection(), &*s.store, &cfg, &CancelToken::new()),
        TestOutcome::Timeout
    );
    // TLS: https to a plain HTTP server on this computer.
    let s = setup(Preset::Custom, true);
    let mut c = s.client.connection().clone();
    c.base_url = c.base_url.replace("http://", "https://");
    // A key entered for the https address (the http one's stays home).
    store_key(&*s.store, &c, &Secret::new(KEY)).unwrap();
    match test_connection(&c, &*s.store, &CancelToken::new()) {
        TestOutcome::Tls(_) | TestOutcome::Unreachable(_) => {}
        other => panic!("{other:?}"),
    }
    // Nothing running, with the preset's hint; and a proxy in the way.
    let mut c = Connection::new(Preset::LmStudio);
    c.base_url = format!("http://127.0.0.1:{}/v1", closed_port());
    let store = MemoryStore::new();
    crate::http::FAKE_PROXY.with(|f| f.set(Some(false)));
    let o = test_connection(&c, &store, &CancelToken::new());
    crate::http::FAKE_PROXY.with(|f| f.set(Some(true)));
    let p = test_connection(&c, &store, &CancelToken::new());
    crate::http::FAKE_PROXY.with(|f| f.set(None));
    assert!(
        matches!(&o, TestOutcome::NotRunning { hint } if hint.contains("LM Studio")),
        "{o:?}"
    );
    assert_eq!(p, TestOutcome::Proxy);
    assert!(p.to_string().contains("proxy"));
    // Cancelled.
    let cancel = CancelToken::new();
    cancel.cancel();
    let s = setup(Preset::Ollama, false);
    assert_eq!(
        test_connection(s.client.connection(), &*s.store, &cancel),
        TestOutcome::Cancelled
    );
    for o in [
        TestOutcome::Ok { models: 2 },
        TestOutcome::NotFound,
        TestOutcome::Timeout,
        TestOutcome::NoKey,
    ] {
        assert!(!o.to_string().is_empty());
    }
}

#[test]
fn the_model_list() {
    let s = setup(Preset::OpenAi, true);
    s.server.route_json(
        "GET",
        "/v1/models",
        200,
        json!({"data": [{"id": "gpt-x"}, {"id": "gpt-y", "name": "GPT Y"}]}),
    );
    let m = s.client.list_models(&CancelToken::new()).unwrap();
    assert_eq!(m.len(), 2);
    assert_eq!(m[1].display_name, "GPT Y");
    // A server without a list: empty, the user types a name.
    let s = setup(Preset::Custom, true);
    assert!(s
        .client
        .list_models(&CancelToken::new())
        .unwrap()
        .is_empty());
    let s = setup(Preset::OpenAi, true);
    s.server.route_json("GET", "/v1/models", 401, json!({}));
    assert_eq!(
        s.client.list_models(&CancelToken::new()).unwrap_err(),
        ChatError::KeyRejected { status: 401 }
    );
}

#[test]
fn local_presets_get_small_budgets() {
    assert_eq!(
        local_budgets(),
        Budgets {
            max_tool_result_chars: 3_000,
            old_result_keep_chars: 200,
            summary_quote_chars: 80,
            max_rounds: 5,
            max_history_chars: 12_000,
            max_context_chars: 1_500,
        }
    );
    assert_eq!(Connection::new(Preset::Ollama).budgets(), local_budgets());
    assert_eq!(Connection::new(Preset::LmStudio).budgets(), local_budgets());
    assert_eq!(
        Connection::new(Preset::OpenAi).budgets(),
        Budgets::default()
    );
    let mut custom = Connection::new(Preset::Custom);
    custom.base_url = "http://localhost:8080/v1".into();
    assert_eq!(custom.budgets(), local_budgets());
    assert!(custom.ask_only());
    // A large tool result is cut to the local budget, and the history is
    // kept small.
    let s = setup(Preset::Ollama, false);
    s.reply(vec![
        calls_reply(&[("b", "big", "{}")]),
        text_reply("ok", false),
    ]);
    let mut conv = s.client.connection().new_conversation();
    assert_eq!(conv.budgets, local_budgets());
    s.turn(&mut conv, "All", tools()).0.unwrap();
    let m = s.requests()[1].json()["messages"]
        .as_array()
        .unwrap()
        .clone();
    let tool = m.iter().find(|x| x["role"] == "tool").unwrap();
    let out = tool["content"].as_str().unwrap();
    assert!(out.chars().count() <= 3_000 + 250, "{}", out.len());
    assert!(out.contains("[Truncated"));
    for i in 0..6 {
        s.reply(vec![text_reply(
            &format!("answer {i} {}", "w".repeat(3000)),
            false,
        )]);
        s.turn(&mut conv, &format!("q{i}"), tools()).0.unwrap();
        let chars: usize = conv
            .chat_messages()
            .iter()
            .map(|v| v.to_string().len())
            .sum();
        assert!(chars <= 12_000 + 3_200, "{chars}");
    }
}

#[test]
fn presets_flags_and_texts() {
    let urls: Vec<(&str, &str, bool, bool)> = presets()
        .iter()
        .map(|p| (p.id, p.base_url, p.needs_key, p.ask_only))
        .collect();
    assert_eq!(
        urls,
        [
            ("openai", "https://api.openai.com/v1", true, false),
            ("openrouter", "https://openrouter.ai/api/v1", true, false),
            ("anthropic", "https://api.anthropic.com/v1", true, false),
            ("ollama", "http://127.0.0.1:11434/v1", false, true),
            ("lmstudio", "http://127.0.0.1:1234/v1", false, true),
            ("custom", "", true, false),
        ]
    );
    for p in presets() {
        assert_eq!(Preset::from_id(p.id), Some(p.preset));
        if !p.base_url.is_empty() {
            assert!(check_address(p.base_url).is_ok(), "{}", p.base_url);
        }
    }
    assert!(Preset::Anthropic.info().note.unwrap().contains("testing"));
    let local = Connection::new(Preset::Ollama);
    let t = privacy_text(&local);
    assert!(t.contains("runs on this PC") && t.contains("Nothing is sent off this computer"));
    let remote = Connection::new(Preset::OpenRouter);
    let t = privacy_text(&remote);
    assert!(t.contains("openrouter.ai") && t.contains("under your API key") && t.contains("terms"));
    assert!(KEY_CAUTION.contains("pay-as-you-go"));
    assert!(local.ask_only() && !remote.ask_only());
    assert!(!ASK_ONLY_NOTE.is_empty());
    for text in [
        privacy_text(&local),
        privacy_text(&remote),
        KEY_CAUTION.to_string(),
    ] {
        assert!(!text.contains('\u{2013}') && !text.contains('\u{2014}'));
    }
}

#[test]
fn a_stored_key_goes_only_to_the_address_it_was_entered_for() {
    let store = MemoryStore::new();
    let mut c = Connection::new(Preset::OpenAi);
    store_key(&store, &c, &Secret::new(KEY)).unwrap();
    assert_eq!(stored_key(&store, &c).unwrap().unwrap().expose(), KEY);
    // Another preset with the same connection id: not that host's key.
    let openai = c.clone();
    let info = Preset::OpenRouter.info();
    c.preset = info.preset;
    c.base_url = info.base_url.into();
    assert!(stored_key(&store, &c).unwrap().is_none());
    assert!(!has_key(&store, &c).unwrap());
    // Another address of a custom server, another port: none either.
    let mut custom = openai.clone();
    custom.preset = Preset::Custom;
    custom.base_url = "https://example.test/v1".into();
    assert!(stored_key(&store, &custom).unwrap().is_none());
    let mut port = openai.clone();
    port.base_url = "https://api.openai.com:8443/v1".into();
    assert!(stored_key(&store, &port).unwrap().is_none());
    // A local preset needs no key: none is ever sent.
    let mut local = openai.clone();
    let info = Preset::Ollama.info();
    local.preset = info.preset;
    local.base_url = info.base_url.into();
    local.needs_key = info.needs_key;
    assert!(stored_key(&store, &local).unwrap().is_none());
    // The address it was entered for still has it; the path may differ.
    let mut path = openai.clone();
    path.base_url = "https://API.openai.com/v2".into();
    assert_eq!(stored_key(&store, &path).unwrap().unwrap().expose(), KEY);
}

#[test]
fn test_connection_never_sends_a_key_bound_to_another_address() {
    let server = FakeServer::start();
    server.route_json("GET", "/v1/models", 200, json!({"data": [{"id": "m"}]}));
    let store = MemoryStore::new();
    let mut c = Connection::new(Preset::Custom);
    c.base_url = "https://elsewhere.test/v1".into();
    store_key(&store, &c, &Secret::new(KEY)).unwrap();
    // The same connection now points at this server: the key stays home.
    c.base_url = format!("http://127.0.0.1:{}/v1", server.url("/").port);
    c.needs_key = true;
    assert_eq!(
        test_connection(&c, &store, &CancelToken::new()),
        TestOutcome::NoKey
    );
    c.needs_key = false;
    assert!(matches!(
        test_connection(&c, &store, &CancelToken::new()),
        TestOutcome::Ok { .. }
    ));
    let seen = server.seen("GET", "/v1/models");
    assert!(seen.iter().all(|s| s.header("Authorization").is_none()));
}
