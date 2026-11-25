//! The live check of the compatible-endpoint route: one question about the
//! sample project to a real server (Ollama on this PC by default, or a
//! provider with a key from the environment; see examples/compat_chat.rs).
//! Ignored, and also skipped unless `OPENXER_COMPAT_LIVE=1`:
//!
//! ```text
//! $env:OPENXER_COMPAT_LIVE = "1"
//! $env:OPENXER_COMPAT_MODEL = "<model>"
//! cargo test -p openxer-chat --test compat_live -- --ignored --nocapture
//! ```

#[allow(dead_code)]
#[path = "../examples/compat_chat.rs"]
mod compat_chat;

#[test]
#[ignore = "talks to a real server; run by hand"]
fn live_compatible_endpoint_answers_one_question() {
    if std::env::var("OPENXER_COMPAT_LIVE").as_deref() != Ok("1") {
        println!("skipped: set OPENXER_COMPAT_LIVE=1 to ask a real server");
        return;
    }
    let answer = compat_chat::run("How many activities are in progress? Answer with the number.")
        .expect("a live answer");
    assert!(!answer.trim().is_empty());
}
