//! The token budgets: what each request carries, pinned.

use super::turns::*;
use super::*;
use crate::conversation::{cap_tool_result, Budgets, Conversation, BASE_INSTRUCTIONS};

#[test]
fn the_default_budgets_are_pinned() {
    assert_eq!(
        Budgets::default(),
        Budgets {
            max_tool_result_chars: 12_000,
            old_result_keep_chars: 400,
            summary_quote_chars: 120,
            max_rounds: 8,
            max_history_chars: 80_000,
            max_context_chars: 4_000,
        }
    );
}

#[test]
fn the_fixed_instructions_stay_short() {
    assert!(
        BASE_INSTRUCTIONS.len() <= 450,
        "{} chars",
        BASE_INSTRUCTIONS.len()
    );
    let conv = Conversation::new();
    assert_eq!(conv.instructions("  "), BASE_INSTRUCTIONS);
    let long = "c".repeat(10_000);
    let full = conv.instructions(&long);
    assert_eq!(full.len(), BASE_INSTRUCTIONS.len() + 2 + 4_000);
}

#[test]
fn a_large_tool_result_is_cut_with_a_marker() {
    let b = Budgets::default();
    let small = "y".repeat(12_000);
    assert_eq!(cap_tool_result(&small, &b), small);
    let out = cap_tool_result(&"x".repeat(50_000), &b);
    assert!(out.starts_with(&"x".repeat(12_000)));
    assert!(out.contains(
        "[Truncated: the result was 50000 characters and only the first 12000 are shown."
    ));
    assert!(out.contains("Ask for less"));
    assert!(out.chars().count() <= 12_000 + 250);
    // Character based, so multibyte text is cut cleanly.
    let out = cap_tool_result(&"é".repeat(20_000), &b);
    assert!(out.starts_with(&"é".repeat(12_000)));
}

#[test]
fn the_capped_result_is_what_the_model_gets() {
    let c = chat();
    c.fx.server.route_seq(
        "POST",
        "/v1/responses",
        vec![
            calls_reply(&[("call_big", "big", "{}")]),
            text_reply("Too much."),
        ],
    );
    let mut conv = Conversation::new();
    c.turn(&mut conv, "All of it", FakeTools::new()).0.unwrap();
    let input = c.requests()[1].json()["input"].as_array().unwrap().clone();
    let out = input
        .iter()
        .find(|i| i["type"] == "function_call_output")
        .unwrap();
    let text = out["output"].as_str().unwrap();
    assert!(text.len() <= 12_000 + 250);
    assert!(text.contains("[Truncated"));
}

#[test]
fn old_tool_results_shrink_to_a_summary_line() {
    let c = chat();
    c.fx.server.route_seq(
        "POST",
        "/v1/responses",
        vec![
            calls_reply(&[("call_big", "big", r#"{"query":"all"}"#)]),
            text_reply("Here is the summary."),
            calls_reply(&[("call_f", "find", r#"{"query":"A1010"}"#)]),
            text_reply("Found."),
        ],
    );
    let tools = FakeTools::new();
    let mut conv = Conversation::new();
    c.turn(&mut conv, "Turn one", tools.clone()).0.unwrap();
    c.turn(&mut conv, "Turn two", tools.clone()).0.unwrap();
    let reqs = c.requests();
    // Request 4 is turn two's second round: turn one's large result is a
    // one-line summary; turn two's own result is whole.
    let input = reqs[3].json()["input"].as_array().unwrap().clone();
    let outputs: Vec<&str> = input
        .iter()
        .filter(|i| i["type"] == "function_call_output")
        .map(|i| i["output"].as_str().unwrap())
        .collect();
    assert_eq!(outputs.len(), 2);
    assert!(
        outputs[0].starts_with("[Earlier result of big (all), 12"),
        "{}",
        outputs[0]
    );
    assert!(outputs[0].len() < 400, "{} chars", outputs[0].len());
    assert!(outputs[0].contains("Call the tool again"));
    assert_eq!(outputs[1], "found A1010");
    // The whole request stays small.
    assert!(reqs[3].body.len() < 3_000, "{} bytes", reqs[3].body.len());
    // Small old results are kept as they are.
    let c2 = chat();
    c2.fx.server.route_seq(
        "POST",
        "/v1/responses",
        vec![
            calls_reply(&[("call_f", "find", r#"{"query":"A1"}"#)]),
            text_reply("one"),
            text_reply("two"),
        ],
    );
    let mut conv = Conversation::new();
    c2.turn(&mut conv, "a", tools.clone()).0.unwrap();
    c2.turn(&mut conv, "b", tools).0.unwrap();
    let input = c2.requests()[2].json()["input"].as_array().unwrap().clone();
    assert!(input.iter().any(|i| i["output"] == "found A1"));
}

#[test]
fn the_request_prefix_is_stable_from_turn_to_turn() {
    let c = chat();
    c.fx.server.route_seq(
        "POST",
        "/v1/responses",
        vec![text_reply("First answer."), text_reply("Second answer.")],
    );
    let mut conv = Conversation::new();
    c.turn(&mut conv, "First", FakeTools::new()).0.unwrap();
    c.turn(&mut conv, "Second", FakeTools::new()).0.unwrap();
    let reqs = c.requests();
    let (a, b) = (reqs[0].json(), reqs[1].json());
    assert_eq!(a["instructions"], b["instructions"]);
    assert_eq!(a["tools"], b["tools"]);
    assert_eq!(a["prompt_cache_key"], b["prompt_cache_key"]);
    let (ia, ib) = (
        a["input"].as_array().unwrap(),
        b["input"].as_array().unwrap(),
    );
    assert_eq!(
        &ib[..ia.len()],
        &ia[..],
        "turn two starts with turn one's input"
    );
    assert_eq!(
        ib[1],
        json!({"role": "assistant", "content": "First answer."})
    );
    assert_eq!(ib[2], json!({"role": "user", "content": "Second"}));
    // A new conversation gets a new cache key.
    assert_ne!(Conversation::new().cache_key(), conv.cache_key());
}

#[test]
fn the_oldest_turns_go_when_the_history_is_over_budget() {
    let c = chat();
    let replies: Vec<Reply> = (0..4)
        .map(|i| text_reply(&format!("answer {i} {}", "z".repeat(400))))
        .collect();
    c.fx.server.route_seq("POST", "/v1/responses", replies);
    let mut conv = Conversation::new().with_budgets(Budgets {
        max_history_chars: 1_200,
        ..Default::default()
    });
    for i in 0..4 {
        c.turn(&mut conv, &format!("question {i}"), FakeTools::new())
            .0
            .unwrap();
        assert!(conv.history_chars() <= 1_200, "{}", conv.history_chars());
    }
    let msgs = conv.messages();
    assert_eq!(msgs.first().unwrap().1, "question 2");
    assert!(msgs.last().unwrap().1.starts_with("answer 3"));
}

#[test]
fn usage_is_reported_per_turn() {
    let c = chat();
    c.fx.server.route_seq(
        "POST",
        "/v1/responses",
        vec![
            calls_reply(&[("c1", "find", r#"{"query":"x"}"#)]),
            text_reply("one"),
            text_reply("two"),
        ],
    );
    let mut conv = Conversation::new();
    let (r1, e1) = c.turn(&mut conv, "a", FakeTools::new());
    let (r2, e2) = c.turn(&mut conv, "b", FakeTools::new());
    let (u1, u2) = (r1.unwrap(), r2.unwrap());
    assert_eq!(
        (
            u1.input_tokens,
            u1.cached_tokens,
            u1.output_tokens,
            u1.requests
        ),
        (250, 40, 30, 2)
    );
    assert_eq!(
        (
            u2.input_tokens,
            u2.cached_tokens,
            u2.output_tokens,
            u2.requests
        ),
        (100, 40, 10, 1)
    );
    assert_eq!(e1.last(), Some(&crate::ChatEvent::TurnFinished(u1)));
    assert_eq!(e2.last(), Some(&crate::ChatEvent::TurnFinished(u2)));
}

#[test]
fn the_app_can_give_its_own_fixed_instructions() {
    let mut conv = Conversation::new();
    assert!(conv.instructions("ctx").starts_with(BASE_INSTRUCTIONS));
    conv.set_instructions("The app's own prompt.");
    assert_eq!(conv.instructions(""), "The app's own prompt.");
    assert_eq!(conv.instructions("ctx"), "The app's own prompt.\n\nctx");
    conv.clear();
    assert_eq!(
        conv.instructions(""),
        "The app's own prompt.",
        "kept by a new chat"
    );
}
