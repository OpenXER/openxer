//! Chat turns against a fake Responses API: streaming, the tool loop,
//! errors and retries, cancel.

use super::*;
use crate::client::{ApiConfig, ChatClient, ChatError, ChatEvent, TurnRequest};
use crate::conversation::{Conversation, Role, BASE_INSTRUCTIONS};
use crate::sse::SseParser;
use crate::stream::{Decoder, OutputItem, Usage};
use crate::tools::{ProposedChanges, ToolDef, ToolOutput, Tools};
use crate::util::CancelToken;
use std::sync::mpsc;
use std::time::Instant;

/// Fake tools: `find` echoes its query, `fail` errors, `big` returns a
/// large result, `slow` sleeps, `propose` proposes a change.
pub struct FakeTools {
    pub calls: Mutex<Vec<(String, Value)>>,
    pub big_len: usize,
}

impl FakeTools {
    pub fn new() -> Arc<FakeTools> {
        Arc::new(FakeTools {
            calls: Mutex::new(Vec::new()),
            big_len: 50_000,
        })
    }
}

impl Tools for FakeTools {
    fn definitions(&self) -> Vec<ToolDef> {
        // Deliberately not sorted: the client sorts them.
        ["slow", "find", "propose", "fail", "big"]
            .iter()
            .map(|n| ToolDef {
                name: n.to_string(),
                description: format!("The {n} tool."),
                parameters: json!({"type": "object", "properties": {"query": {"type": "string"}}}),
            })
            .collect()
    }
    fn call(&self, name: &str, arguments: &Value, cancel: &CancelToken) -> ToolOutput {
        self.calls
            .lock()
            .unwrap()
            .push((name.to_string(), arguments.clone()));
        match name {
            "find" => ToolOutput::ok(format!(
                "found {}",
                arguments["query"].as_str().unwrap_or("")
            )),
            "fail" => ToolOutput::error("no such activity"),
            "big" => ToolOutput::ok("x".repeat(self.big_len)),
            "slow" => {
                cancel.sleep(Duration::from_secs(5));
                ToolOutput::ok("slow done")
            }
            "propose" => ToolOutput::proposed(
                "Proposed 1 change.",
                ProposedChanges {
                    summary: "Change A1010 duration to 5d".into(),
                    payload: json!({"changes": [{"activity": "A1010", "duration": 5}]}),
                },
            ),
            _ => ToolOutput::error("unknown tool"),
        }
    }
}

pub struct Chat {
    pub fx: Fixture,
    pub client: Arc<ChatClient>,
}

pub fn chat() -> Chat {
    chat_with(|_| {})
}

pub fn chat_with(tune: impl FnOnce(&mut ApiConfig)) -> Chat {
    let fx = Fixture::new().signed_in();
    let mut api = ApiConfig {
        base_url: fx.server.url("/v1"),
        http: HttpConfig {
            connect_timeout: Duration::from_secs(2),
            read_timeout: Duration::from_secs(10),
        },
        retry_delays: vec![Duration::from_millis(10), Duration::from_millis(10)],
        max_retry_after: Duration::from_millis(50),
        ..ApiConfig::openai()
    };
    tune(&mut api);
    let client = Arc::new(ChatClient::new(fx.auth.clone(), api));
    Chat { fx, client }
}

pub fn request(text: &str) -> TurnRequest {
    TurnRequest {
        model: "model-from-list".into(),
        context: "Project RIVERBEND, data date 2026-03-02.".into(),
        user_text: text.into(),
        reasoning_effort: None,
    }
}

impl Chat {
    pub fn turn(
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

    pub fn requests(&self) -> Vec<Seen> {
        self.fx.server.seen("POST", "/v1/responses")
    }
}

#[test]
fn a_plain_turn_streams_text_and_reports_usage() {
    let c = chat();
    c.fx.server
        .route_seq("POST", "/v1/responses", vec![text_reply("Hello world")]);
    let mut conv = Conversation::new();
    let (r, events) = c.turn(&mut conv, "Hi", FakeTools::new());
    let usage = r.unwrap();
    assert_eq!(
        usage,
        Usage {
            input_tokens: 100,
            cached_tokens: 40,
            output_tokens: 10,
            reasoning_tokens: 2,
            requests: 1
        }
    );
    assert_eq!(
        events,
        vec![
            ChatEvent::TextDelta("Hello".into()),
            ChatEvent::TextDelta(" world".into()),
            ChatEvent::Message("Hello world".into()),
            ChatEvent::TurnFinished(usage),
        ]
    );
    assert_eq!(
        conv.messages(),
        vec![
            (Role::User, "Hi".into()),
            (Role::Assistant, "Hello world".into())
        ]
    );
    let req = &c.requests()[0];
    assert_eq!(req.header("Authorization"), Some("Bearer AT-SECRET-1"));
    let b = req.json();
    assert_eq!(b["store"], false);
    assert_eq!(b["stream"], true);
    assert_eq!(b["model"], "model-from-list");
    assert!(b["instructions"]
        .as_str()
        .unwrap()
        .starts_with(BASE_INSTRUCTIONS));
    assert!(b["instructions"]
        .as_str()
        .unwrap()
        .ends_with("data date 2026-03-02."));
    assert_eq!(b["prompt_cache_key"], conv.cache_key());
    assert_eq!(b["include"], json!(["reasoning.encrypted_content"]));
    assert!(b.get("previous_response_id").is_none());
    assert!(b.get("tool_choice").is_none());
    for field in [
        "max_output_tokens",
        "temperature",
        "top_p",
        "metadata",
        "user",
        "truncation",
    ] {
        assert!(
            b.get(field).is_none(),
            "{field} is not supported on this route"
        );
    }
    assert_eq!(b["input"], json!([{"role": "user", "content": "Hi"}]));
    // Tools are grouped in one namespace, sorted by name.
    let ns = &b["tools"][0];
    assert_eq!(ns["type"], "namespace");
    assert_eq!(ns["name"], "openxer");
    let names: Vec<&str> = ns["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["big", "fail", "find", "propose", "slow"]);
    assert_eq!(ns["tools"][0]["type"], "function");
}

#[test]
fn the_tool_loop_runs_tools_and_sends_results_back() {
    let c = chat();
    c.fx.server.route_seq(
        "POST",
        "/v1/responses",
        vec![
            calls_reply(&[("call_1", "find", r#"{"query":"A1010"}"#)]),
            text_reply("A1010 is late."),
        ],
    );
    let tools = FakeTools::new();
    let mut conv = Conversation::new();
    let (r, events) = c.turn(&mut conv, "Is A1010 late?", tools.clone());
    let usage = r.unwrap();
    assert_eq!(usage.requests, 2);
    assert_eq!(usage.input_tokens, 250);
    assert_eq!(usage.output_tokens, 30);
    assert_eq!(
        tools.calls.lock().unwrap()[0],
        ("find".into(), json!({"query": "A1010"}))
    );
    assert!(
        matches!(&events[0], ChatEvent::ToolStarted { id, name, target } if id == "call_1" && name == "find" && target == "A1010")
    );
    assert!(matches!(&events[1], ChatEvent::ToolFinished { ok: true, name, .. } if name == "find"));
    let second = c.requests()[1].json();
    let input = second["input"].as_array().unwrap();
    assert_eq!(
        input[0],
        json!({"role": "user", "content": "Is A1010 late?"})
    );
    // The reasoning item is resent within the turn, without its id.
    assert_eq!(
        input[1],
        json!({"type": "reasoning", "summary": [], "encrypted_content": "ENC"})
    );
    assert_eq!(
        input[2],
        json!({"type": "function_call", "call_id": "call_1", "name": "find", "namespace": "openxer",
            "arguments": "{\"query\":\"A1010\"}"})
    );
    assert_eq!(
        input[3],
        json!({"type": "function_call_output", "call_id": "call_1", "output": "found A1010"})
    );
    // After the turn the reasoning item is gone from the history.
    assert!(!conv.input_items().iter().any(|i| i["type"] == "reasoning"));
}

#[test]
fn several_tool_calls_in_one_response_run_in_order() {
    let c = chat();
    c.fx.server.route_seq(
        "POST",
        "/v1/responses",
        vec![
            calls_reply(&[
                ("call_a", "find", r#"{"query":"one"}"#),
                ("call_b", "fail", r#"{"query":"two"}"#),
            ]),
            text_reply("Done."),
        ],
    );
    let tools = FakeTools::new();
    let mut conv = Conversation::new();
    let (r, events) = c.turn(&mut conv, "Go", tools.clone());
    r.unwrap();
    let names: Vec<String> = tools
        .calls
        .lock()
        .unwrap()
        .iter()
        .map(|c| c.0.clone())
        .collect();
    assert_eq!(names, ["find", "fail"]);
    let finished: Vec<(String, bool)> = events
        .iter()
        .filter_map(|e| match e {
            ChatEvent::ToolFinished { id, ok, .. } => Some((id.clone(), *ok)),
            _ => None,
        })
        .collect();
    assert_eq!(
        finished,
        [("call_a".into(), true), ("call_b".into(), false)]
    );
    let input = c.requests()[1].json()["input"].as_array().unwrap().clone();
    let outputs: Vec<&Value> = input
        .iter()
        .filter(|i| i["type"] == "function_call_output")
        .collect();
    assert_eq!(outputs[0]["output"], "found one");
    assert_eq!(outputs[1]["output"], "Error: no such activity");
}

#[test]
fn bad_arguments_go_back_to_the_model_as_an_error() {
    let c = chat();
    c.fx.server.route_seq(
        "POST",
        "/v1/responses",
        vec![
            calls_reply(&[("call_1", "find", "{not json")]),
            text_reply("Sorry."),
        ],
    );
    let tools = FakeTools::new();
    let mut conv = Conversation::new();
    c.turn(&mut conv, "Go", tools.clone()).0.unwrap();
    assert!(tools.calls.lock().unwrap().is_empty());
    let input = c.requests()[1].json()["input"].as_array().unwrap().clone();
    assert!(input.iter().any(|i| i["type"] == "function_call_output"
        && i["output"].as_str().unwrap().contains("not valid JSON")));
}

#[test]
fn proposed_changes_are_reported() {
    let c = chat();
    c.fx.server.route_seq(
        "POST",
        "/v1/responses",
        vec![
            calls_reply(&[("call_p", "propose", "{}")]),
            text_reply("Review the proposal."),
        ],
    );
    let mut conv = Conversation::new();
    let (r, events) = c.turn(&mut conv, "Shorten A1010", FakeTools::new());
    r.unwrap();
    let p = events
        .iter()
        .find_map(|e| match e {
            ChatEvent::ProposedChanges { id, changes } => Some((id.clone(), changes.clone())),
            _ => None,
        })
        .expect("a proposal event");
    assert_eq!(p.0, "call_p");
    assert_eq!(p.1.summary, "Change A1010 duration to 5d");
    assert_eq!(p.1.payload["changes"][0]["duration"], 5);
}

#[test]
fn the_round_cap_stops_a_tool_loop_and_rolls_back() {
    let c = chat();
    c.fx.server.route_seq(
        "POST",
        "/v1/responses",
        vec![calls_reply(&[("call_x", "find", r#"{"query":"again"}"#)])],
    );
    let mut conv = Conversation::new().with_budgets(crate::Budgets {
        max_rounds: 3,
        ..Default::default()
    });
    let (r, events) = c.turn(&mut conv, "Loop", FakeTools::new());
    assert_eq!(r.unwrap_err(), ChatError::RoundCap { rounds: 3 });
    assert_eq!(
        events.last(),
        Some(&ChatEvent::Error(ChatError::RoundCap { rounds: 3 }))
    );
    let reqs = c.requests();
    assert_eq!(reqs.len(), 3);
    assert!(reqs[0].json().get("tool_choice").is_none());
    assert_eq!(
        reqs[2].json()["tool_choice"],
        "none",
        "the last round must answer"
    );
    assert!(
        conv.is_empty(),
        "a failed turn leaves the conversation unchanged"
    );
}

#[test]
fn cancel_during_streaming_stops_promptly() {
    let c = chat();
    let mut events: Vec<String> = message_events(0, "a long answer that keeps going");
    events.push(completed(1, 0, 1));
    let mut reply = Reply::sse(&events);
    reply.delay = Duration::from_millis(400);
    c.fx.server.route_seq("POST", "/v1/responses", vec![reply]);
    let conv = Conversation::new();
    let h = c.client.start_turn(conv, request("Go"), FakeTools::new());
    let t = Instant::now();
    let first = h.events.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(matches!(first, ChatEvent::TextDelta(_)), "{first:?}");
    let cancelled_at = Instant::now();
    h.cancel();
    let rest: Vec<ChatEvent> = h.events.iter().collect();
    assert_eq!(rest.last(), Some(&ChatEvent::Cancelled));
    assert!(
        cancelled_at.elapsed() < Duration::from_millis(300),
        "{:?}",
        cancelled_at.elapsed()
    );
    assert!(t.elapsed() < Duration::from_secs(3));
    assert!(h.join().is_empty());
}

#[test]
fn cancel_during_a_tool_call_stops_promptly() {
    let c = chat();
    c.fx.server.route_seq(
        "POST",
        "/v1/responses",
        vec![
            calls_reply(&[("call_s", "slow", "{}")]),
            text_reply("never"),
        ],
    );
    let h = c
        .client
        .start_turn(Conversation::new(), request("Go"), FakeTools::new());
    loop {
        let e = h.events.recv_timeout(Duration::from_secs(5)).unwrap();
        if matches!(e, ChatEvent::ToolStarted { .. }) {
            break;
        }
    }
    let t = Instant::now();
    h.cancel();
    let rest: Vec<ChatEvent> = h.events.iter().collect();
    assert_eq!(rest.last(), Some(&ChatEvent::Cancelled));
    assert!(
        t.elapsed() < Duration::from_millis(500),
        "{:?}",
        t.elapsed()
    );
    assert_eq!(c.requests().len(), 1);
}

fn error_json(code: &str, message: &str) -> Value {
    json!({"error": {"type": "invalid_request_error", "code": code, "message": message, "param": null}})
}

#[test]
fn a_401_refreshes_once_and_resends() {
    let c = chat();
    c.fx.server.route_seq(
        "POST",
        "/v1/responses",
        vec![
            Reply::json(401, &json!({"detail": "token rejected"})),
            text_reply("ok"),
        ],
    );
    let mut conv = Conversation::new();
    c.turn(&mut conv, "Hi", FakeTools::new()).0.unwrap();
    assert_eq!(c.fx.auth.refresh_count(), 1);
    let reqs = c.requests();
    assert_eq!(reqs[0].header("Authorization"), Some("Bearer AT-SECRET-1"));
    assert_eq!(reqs[1].header("Authorization"), Some("Bearer AT-SECRET-2"));
}

#[test]
fn a_second_401_means_signed_out() {
    let c = chat();
    c.fx.server.route_seq(
        "POST",
        "/v1/responses",
        vec![Reply::json(
            401,
            &error_json("subscription_sharing_invalid_user", "no"),
        )],
    );
    let mut conv = Conversation::new();
    let (r, _) = c.turn(&mut conv, "Hi", FakeTools::new());
    assert_eq!(r.unwrap_err(), ChatError::SignedOut);
    assert_eq!(c.fx.auth.refresh_count(), 1);
    assert_eq!(c.requests().len(), 2);
}

#[test]
fn a_rejected_refresh_during_a_turn_signs_out() {
    let c = chat();
    c.fx.state.lock().unwrap().refresh_override = Some((400, json!({"error": "invalid_grant"})));
    c.fx.server
        .route_seq("POST", "/v1/responses", vec![Reply::json(401, &json!({}))]);
    let mut conv = Conversation::new();
    assert_eq!(
        c.turn(&mut conv, "Hi", FakeTools::new()).0.unwrap_err(),
        ChatError::SignedOut
    );
    assert_eq!(c.fx.auth.status(), crate::SignInStatus::SignedOut);
}

#[test]
fn the_usage_limit_is_reported_and_not_retried() {
    let c = chat();
    c.fx.server.route_seq(
        "POST",
        "/v1/responses",
        vec![Reply::json(
            429,
            &error_json(
                "subscription_sharing_usage_limit_exceeded",
                "You have hit your limit.",
            ),
        )
        .header("Retry-After", "1")],
    );
    let mut conv = Conversation::new();
    let (r, events) = c.turn(&mut conv, "Hi", FakeTools::new());
    let e = r.unwrap_err();
    assert_eq!(
        e,
        ChatError::UsageLimit {
            message: "You have hit your limit.".into()
        }
    );
    assert!(e.to_string().contains("ChatGPT settings"));
    assert_eq!(events, vec![ChatEvent::Error(e)]);
    assert_eq!(c.requests().len(), 1, "no retry and no other billing path");
}

#[test]
fn a_usage_limit_after_streaming_starts_is_reported() {
    let c = chat();
    let mut e = message_events(0, "partial");
    e.truncate(2);
    e.push(ev(
        json!({"type": "response.failed", "response": {"status": "failed",
        "error": {"code": "subscription_sharing_usage_limit_exceeded", "message": "limit"}}}),
    ));
    c.fx.server
        .route_seq("POST", "/v1/responses", vec![Reply::sse(&e)]);
    let mut conv = Conversation::new();
    let (r, _) = c.turn(&mut conv, "Hi", FakeTools::new());
    assert!(matches!(r.unwrap_err(), ChatError::UsageLimit { .. }));
    assert_eq!(c.requests().len(), 1);
}

#[test]
fn other_429s_and_5xx_are_retried_with_a_bound() {
    let c = chat();
    c.fx.server.route_seq(
        "POST",
        "/v1/responses",
        vec![
            Reply::json(429, &error_json("rate_limit_exceeded", "slow down"))
                .header("Retry-After", "30"),
            Reply::json(
                503,
                &error_json("subscription_sharing_usage_unavailable", "later"),
            ),
            text_reply("ok"),
        ],
    );
    let mut conv = Conversation::new();
    let t = Instant::now();
    c.turn(&mut conv, "Hi", FakeTools::new()).0.unwrap();
    assert_eq!(c.requests().len(), 3);
    assert!(
        t.elapsed() < Duration::from_secs(2),
        "Retry-After is capped"
    );

    let c = chat();
    c.fx.server.route_seq(
        "POST",
        "/v1/responses",
        vec![Reply::json(500, &json!({"detail": "boom"}))],
    );
    let mut conv = Conversation::new();
    let e = c.turn(&mut conv, "Hi", FakeTools::new()).0.unwrap_err();
    assert_eq!(
        e,
        ChatError::Api {
            status: 500,
            code: "http_500".into(),
            message: "boom".into()
        }
    );
    assert_eq!(c.requests().len(), 3, "one try and two retries");
}

#[test]
fn forbidden_and_bad_requests_are_not_retried() {
    let c = chat();
    c.fx.server.route_seq(
        "POST",
        "/v1/responses",
        vec![Reply::json(
            403,
            &error_json("subscription_sharing_user_not_eligible", "not eligible"),
        )],
    );
    let mut conv = Conversation::new();
    assert_eq!(
        c.turn(&mut conv, "Hi", FakeTools::new()).0.unwrap_err(),
        ChatError::NotAllowed {
            code: "subscription_sharing_user_not_eligible".into(),
            message: "not eligible".into()
        }
    );
    assert_eq!(c.requests().len(), 1);
    let c = chat();
    c.fx.server.route_seq(
        "POST",
        "/v1/responses",
        vec![Reply::json(400, &error_json("invalid_value", "bad"))],
    );
    assert!(matches!(
        c.turn(&mut Conversation::new(), "Hi", FakeTools::new())
            .0
            .unwrap_err(),
        ChatError::Api { status: 400, .. }
    ));
    assert_eq!(c.requests().len(), 1);
}

#[test]
fn an_unsupported_optional_field_is_dropped_and_stays_dropped() {
    let c = chat();
    let mut unsupported = error_json(
        "subscription_sharing_unsupported_capability",
        "no caching key",
    );
    unsupported["error"]["param"] = json!("prompt_cache_key");
    c.fx.server.route_seq(
        "POST",
        "/v1/responses",
        vec![
            Reply::json(400, &unsupported),
            text_reply("ok"),
            text_reply("ok again"),
        ],
    );
    let mut conv = Conversation::new();
    c.turn(&mut conv, "Hi", FakeTools::new()).0.unwrap();
    c.turn(&mut conv, "Again", FakeTools::new()).0.unwrap();
    let reqs = c.requests();
    assert_eq!(reqs.len(), 3);
    assert!(reqs[0].json().get("prompt_cache_key").is_some());
    assert!(reqs[1].json().get("prompt_cache_key").is_none());
    assert!(reqs[2].json().get("prompt_cache_key").is_none());
    assert!(reqs[2].json().get("include").is_some());
    // A required part named as unsupported is an error, not a loop.
    let c = chat();
    let mut unsupported = error_json("subscription_sharing_unsupported_capability", "model");
    unsupported["error"]["param"] = json!("model");
    c.fx.server.route_seq(
        "POST",
        "/v1/responses",
        vec![Reply::json(400, &unsupported)],
    );
    assert!(c
        .turn(&mut Conversation::new(), "Hi", FakeTools::new())
        .0
        .is_err());
    assert_eq!(c.requests().len(), 1);
}

#[test]
fn stream_errors_and_cut_off_streams() {
    let c = chat();
    c.fx.server.route_seq(
        "POST",
        "/v1/responses",
        vec![Reply::sse(&[ev(
            json!({"type": "error", "code": "server_error", "message": "oops"}),
        )])],
    );
    assert_eq!(
        c.turn(&mut Conversation::new(), "Hi", FakeTools::new())
            .0
            .unwrap_err(),
        ChatError::Api {
            status: 200,
            code: "server_error".into(),
            message: "oops".into()
        }
    );
    // Cut in the middle of an event.
    let c = chat();
    let mut e = message_events(0, "partial answer");
    let half = e[2].len() / 2;
    e[2].truncate(half);
    let mut reply = Reply::sse(&e[..3]);
    reply.cut = true;
    c.fx.server.route_seq("POST", "/v1/responses", vec![reply]);
    let err = c
        .turn(&mut Conversation::new(), "Hi", FakeTools::new())
        .0
        .unwrap_err();
    assert!(
        matches!(&err, ChatError::BadResponse(m) | ChatError::Network(m) if !m.is_empty()),
        "{err:?}"
    );
    // A stream that ends cleanly without response.completed.
    let c = chat();
    c.fx.server.route_seq(
        "POST",
        "/v1/responses",
        vec![Reply::sse(&message_events(0, "no end"))],
    );
    assert_eq!(
        c.turn(&mut Conversation::new(), "Hi", FakeTools::new())
            .0
            .unwrap_err(),
        ChatError::BadResponse("the stream ended before the answer was complete".into())
    );
    // response.incomplete.
    let c = chat();
    let mut e = message_events(0, "stopped");
    e.push(ev(
        json!({"type": "response.incomplete", "response": {"status": "incomplete",
        "incomplete_details": {"reason": "content_filter"}, "usage": {}}}),
    ));
    c.fx.server
        .route_seq("POST", "/v1/responses", vec![Reply::sse(&e)]);
    assert_eq!(
        c.turn(&mut Conversation::new(), "Hi", FakeTools::new())
            .0
            .unwrap_err(),
        ChatError::Incomplete {
            reason: "content_filter".into()
        }
    );
}

#[test]
fn network_down_is_reported_after_retries() {
    let gone = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = gone.local_addr().unwrap().port();
    drop(gone);
    let c = chat_with(|api| {
        api.base_url = crate::http::Url::parse(&format!("http://127.0.0.1:{port}/v1")).unwrap()
    });
    crate::http::FAKE_PROXY.with(|f| f.set(Some(false)));
    let e = c
        .turn(&mut Conversation::new(), "Hi", FakeTools::new())
        .0
        .unwrap_err();
    crate::http::FAKE_PROXY.with(|f| f.set(None));
    assert!(matches!(e, ChatError::Network(_)), "{e:?}");
}

#[test]
fn chat_needs_a_sign_in_with_plan_usage() {
    let fx = Fixture::new();
    let client = ChatClient::new(
        fx.auth.clone(),
        ApiConfig {
            base_url: fx.server.url("/v1"),
            ..ApiConfig::openai()
        },
    );
    let (tx, _rx) = mpsc::channel();
    let tools: Arc<dyn Tools> = FakeTools::new();
    let r = client.run_turn(
        &mut Conversation::new(),
        &request("Hi"),
        &tools,
        &tx,
        &CancelToken::new(),
    );
    assert_eq!(r.unwrap_err(), ChatError::SignedOut);
    fx.auth
        .sign_in(
            &fx.browser(Browser::Approve {
                client_id: Some(ISSUED_CLIENT.into()),
                scope: "openid profile email offline_access".into(),
            }),
            &CancelToken::new(),
        )
        .unwrap();
    let r = client.run_turn(
        &mut Conversation::new(),
        &request("Hi"),
        &tools,
        &tx,
        &CancelToken::new(),
    );
    assert_eq!(r.unwrap_err(), ChatError::PlanUsageNotEnabled);
    assert_eq!(fx.server.count("POST", "/v1/responses"), 0);
}

#[test]
fn the_model_list_keeps_listed_models_in_order() {
    let c = chat();
    c.fx.server.route_seq(
        "GET",
        "/v1/models",
        vec![
            Reply::json(401, &json!({})),
            Reply::json(
                200,
                &json!({"models": [
                    {"slug": "model-b", "display_name": "Model B", "visibility": "list"},
                    {"slug": "model-hidden", "display_name": "Hidden", "visibility": "hide"},
                    {"slug": "model-a", "display_name": "Model A", "visibility": "list"}]}),
            ),
        ],
    );
    let models = c.client.list_models(&CancelToken::new()).unwrap();
    let slugs: Vec<&str> = models.iter().map(|m| m.slug.as_str()).collect();
    assert_eq!(slugs, ["model-b", "model-a"]);
    assert_eq!(
        crate::default_model(&models).unwrap().display_name,
        "Model B"
    );
    assert_eq!(c.fx.auth.refresh_count(), 1);
    let seen = c.fx.server.seen("GET", "/v1/models");
    assert_eq!(seen[1].header("Authorization"), Some("Bearer AT-SECRET-2"));
    // The plain API list shape is read too.
    c.fx.server
        .route_json("GET", "/v1/models", 200, json!({"data": [{"id": "m1"}]}));
    let models = c.client.list_models(&CancelToken::new()).unwrap();
    assert_eq!(models[0].slug, "m1");
    assert_eq!(models[0].display_name, "m1");
    assert!(crate::default_model(&[]).is_none());
}

/// A recorded-style stream: reasoning, text, two calls with arguments in
/// pieces, usage. Split into two pushes at every byte boundary, it decodes
/// to the same items and events as in one piece.
#[test]
fn the_decoder_gives_the_same_result_at_every_byte_boundary() {
    let mut e = reasoning_events(0);
    e.extend(message_events(1, "Checking two activities: é and ü."));
    e.extend(call_events(
        2,
        "call_1",
        "find",
        r#"{"query":"A1010","fields":["start","finish"]}"#,
    ));
    e.extend(call_events(3, "call_2", "find", r#"{"query":"A2020"}"#));
    e.push(completed(321, 300, 45));
    let bytes: Vec<u8> = e.concat().into_bytes();
    let run = |parts: &[&[u8]]| {
        let mut p = SseParser::new();
        let mut d = Decoder::new();
        let mut emits = Vec::new();
        for part in parts {
            for ev in p.push(part) {
                emits.extend(d.handle(&ev));
            }
        }
        (d.items(), emits, d.end.clone(), p.has_partial())
    };
    let whole = run(&[&bytes[..]]);
    assert_eq!(whole.0.len(), 4);
    assert!(
        matches!(&whole.0[1], OutputItem::Message(m) if m == "Checking two activities: é and ü.")
    );
    assert!(
        matches!(&whole.0[2], OutputItem::Call { arguments, namespace, .. }
        if arguments == r#"{"query":"A1010","fields":["start","finish"]}"# && namespace.as_deref() == Some("openxer"))
    );
    assert!(matches!(&whole.0[3], OutputItem::Call { call_id, .. } if call_id == "call_2"));
    assert!(matches!(whole.2, Some(crate::stream::End::Completed(u)) if u.cached_tokens == 300));
    assert!(!whole.3);
    for i in 0..=bytes.len() {
        assert_eq!(run(&[&bytes[..i], &bytes[i..]]), whole, "split at byte {i}");
    }
}

#[test]
fn arguments_built_from_deltas_when_the_done_events_are_missing() {
    let mut d = Decoder::new();
    let mut p = SseParser::new();
    let events = [
        ev(
            json!({"type": "response.output_item.added", "output_index": 0,
            "item": {"type": "function_call", "call_id": "c1", "name": "find", "arguments": ""}}),
        ),
        ev(
            json!({"type": "response.function_call_arguments.delta", "output_index": 0, "delta": "{\"query\":"}),
        ),
        ev(
            json!({"type": "response.function_call_arguments.delta", "output_index": 0, "delta": "\"x\"}"}),
        ),
    ];
    for e in &events {
        for s in p.push(e.as_bytes()) {
            d.handle(&s);
        }
    }
    assert_eq!(
        d.items(),
        vec![OutputItem::Call {
            call_id: "c1".into(),
            name: "find".into(),
            namespace: None,
            arguments: "{\"query\":\"x\"}".into()
        }]
    );
}

#[test]
fn chat_errors_carry_no_secrets() {
    let c = chat();
    c.fx.server
        .route_seq("POST", "/v1/responses", vec![Reply::json(401, &json!({}))]);
    let (r, events) = c.turn(&mut Conversation::new(), "Hi", FakeTools::new());
    let e = r.unwrap_err();
    assert_no_secrets(&format!("{e:?} {e} {events:?} {:?}", c.client));
    let errors = [
        ChatError::SignedOut,
        ChatError::PlanUsageNotEnabled,
        ChatError::UsageLimit {
            message: "m".into(),
        },
        ChatError::Network("n".into()),
        ChatError::RoundCap { rounds: 2 },
    ];
    for e in errors {
        assert!(!e.to_string().is_empty());
    }
}

#[test]
fn a_failed_connection_behind_a_proxy_is_not_retried() {
    let gone = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = gone.local_addr().unwrap().port();
    drop(gone);
    let c = chat_with(|api| {
        api.base_url = crate::http::Url::parse(&format!("http://127.0.0.1:{port}/v1")).unwrap();
        api.retry_delays = vec![Duration::from_secs(5)];
    });
    crate::http::FAKE_PROXY.with(|f| f.set(Some(true)));
    let t = Instant::now();
    let e = c
        .turn(&mut Conversation::new(), "Hi", FakeTools::new())
        .0
        .unwrap_err();
    crate::http::FAKE_PROXY.with(|f| f.set(None));
    assert_eq!(e, ChatError::ProxyNotSupported);
    assert!(e.to_string().contains("proxy"));
    assert!(t.elapsed() < Duration::from_secs(3), "no retry wait");
}

#[test]
fn an_oversized_event_or_stream_is_a_bad_response() {
    let c = chat_with(|api| api.max_event_bytes = 200);
    c.fx.server
        .route_seq("POST", "/v1/responses", vec![text_reply(&"y".repeat(500))]);
    assert_eq!(
        c.turn(&mut Conversation::new(), "Hi", FakeTools::new())
            .0
            .unwrap_err(),
        ChatError::BadResponse("an event in the reply was too large".into())
    );
    let c = chat_with(|api| api.max_stream_bytes = 1_000);
    let mut e = Vec::new();
    for _ in 0..10 {
        e.push(ev(
            json!({"type": "response.output_text.delta", "output_index": 0,
            "delta": "z".repeat(150)}),
        ));
    }
    c.fx.server
        .route_seq("POST", "/v1/responses", vec![Reply::sse(&e)]);
    assert_eq!(
        c.turn(&mut Conversation::new(), "Hi", FakeTools::new())
            .0
            .unwrap_err(),
        ChatError::BadResponse("the reply was too large".into())
    );
}

#[test]
fn streams_with_content_length_or_close_framing_work() {
    use crate::testkit::Framing;
    for framing in [Framing::Length, Framing::Close] {
        let c = chat();
        c.fx.server.route_seq(
            "POST",
            "/v1/responses",
            vec![
                calls_reply(&[("call_1", "find", r#"{"query":"A1"}"#)]).framed(framing),
                text_reply("Done.").framed(framing),
            ],
        );
        let mut conv = Conversation::new();
        let (r, events) = c.turn(&mut conv, "Go", FakeTools::new());
        assert_eq!(r.unwrap().requests, 2, "{framing:?}");
        assert!(events.contains(&ChatEvent::Message("Done.".into())));
        // Errors framed the same way.
        let c = chat();
        c.fx.server.route_seq(
            "POST",
            "/v1/responses",
            vec![Reply::json(
                403,
                &error_json("subscription_sharing_user_not_eligible", "no"),
            )
            .framed(framing)],
        );
        assert!(matches!(
            c.turn(&mut Conversation::new(), "Hi", FakeTools::new())
                .0
                .unwrap_err(),
            ChatError::NotAllowed { .. }
        ));
    }
}

#[test]
fn an_echoed_token_is_taken_out_of_error_text() {
    let c = chat();
    let mut e = message_events(0, "partial");
    e.truncate(2);
    e.push(ev(json!({"type": "response.failed", "response": {"error":
        {"code": "server_error", "message": "bad token AT-SECRET-1 in request"}}})));
    c.fx.server.route_seq(
        "POST",
        "/v1/responses",
        vec![
            Reply::json(
                400,
                &error_json("invalid_value", "Bearer AT-SECRET-1 is wrong"),
            ),
            Reply::sse(&e),
        ],
    );
    let mut texts = Vec::new();
    for _ in 0..2 {
        let (r, events) = c.turn(&mut Conversation::new(), "Hi", FakeTools::new());
        let err = r.unwrap_err();
        texts.push(format!("{err:?} {err} {events:?}"));
    }
    for t in &texts {
        assert_no_secrets(t);
        assert!(t.contains("<redacted>"), "{t}");
    }
}

#[test]
fn token_and_model_requests_leave_no_sockets_registered() {
    let c = chat();
    let cancel = CancelToken::new();
    c.fx.server.route_json("GET", "/v1/models", 500, json!({}));
    assert!(c.client.list_models(&cancel).is_err());
    assert_eq!(cancel.watched(), 0);
    c.fx.set_time(T0 + 4000);
    c.fx.auth.access_token(&cancel).unwrap();
    c.fx.auth.sign_out(&cancel).unwrap();
    assert_eq!(cancel.watched(), 0);
}
