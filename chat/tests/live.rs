//! The live check: a real sign-in in your browser and one question about
//! the sample project. Ignored, and also skipped unless
//! `OPENXER_CHAT_LIVE=1`, so no automated run ever reaches OpenAI:
//!
//! ```text
//! $env:OPENXER_CHAT_LIVE = "1"
//! cargo test -p openxer-chat --test live -- --ignored --nocapture
//! ```

#[path = "../examples/live_chat.rs"]
mod live_chat;

#[test]
#[ignore = "signs in to ChatGPT in a real browser; run by hand"]
fn live_sign_in_and_one_question() {
    if std::env::var("OPENXER_CHAT_LIVE").as_deref() != Ok("1") {
        println!("skipped: set OPENXER_CHAT_LIVE=1 to sign in for real");
        return;
    }
    let answer = live_chat::run(
        "How many activities are in progress? Answer with the number and their IDs.",
        false,
    )
    .expect("a live answer");
    assert!(!answer.trim().is_empty());
}
