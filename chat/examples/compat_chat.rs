//! One question about the synthetic sample project over an OpenAI-compatible
//! endpoint: a model on this PC (Ollama, LM Studio) or a provider with an API
//! key taken from an environment variable. For the maintainer to run by
//! hand; tests never run it.
//!
//! ```text
//! # Ollama on this PC (default), with a model already pulled:
//! $env:OPENXER_COMPAT_MODEL = "<model name from ollama list>"
//! cargo run -p openxer-chat --example compat_chat -- "Which activities are in progress?"
//!
//! # A provider with a key (never stored; read from the environment):
//! $env:OPENXER_COMPAT_PRESET = "openai"   # openai, openrouter, anthropic, ollama, lmstudio, custom
//! $env:OPENXER_COMPAT_KEY = "<your key>"
//! $env:OPENXER_COMPAT_MODEL = "<a model the key may use>"
//! # optional: $env:OPENXER_COMPAT_BASE_URL = "https://..."
//! cargo run -p openxer-chat --example compat_chat
//! ```

#[allow(dead_code)]
#[path = "live_chat.rs"]
mod live_chat;

use openxer_chat::compat::{
    privacy_text, store_key, test_connection, CompatClient, Connection, Preset, KEY_CAUTION,
};
use openxer_chat::{CancelToken, ChatEvent, MemoryStore, Secret, Tools, TurnRequest};
use openxer_xer::date_string;
use openxer_xer::sample_project::sample_project;
use std::io::Write;
use std::sync::Arc;

/// Ask `question`; returns the answer text.
pub fn run(question: &str) -> Result<String, String> {
    let preset_id = std::env::var("OPENXER_COMPAT_PRESET").unwrap_or_else(|_| "ollama".into());
    let preset =
        Preset::from_id(preset_id.trim()).ok_or_else(|| format!("unknown preset {preset_id}"))?;
    let mut conn = Connection::new(preset);
    if let Ok(b) = std::env::var("OPENXER_COMPAT_BASE_URL") {
        if !b.trim().is_empty() {
            conn.base_url = b.trim().to_string();
        }
    }
    conn.model = std::env::var("OPENXER_COMPAT_MODEL").unwrap_or_default();
    // The key lives only in memory for this run.
    let store = Arc::new(MemoryStore::new());
    if let Ok(k) = std::env::var("OPENXER_COMPAT_KEY") {
        if !k.trim().is_empty() {
            store_key(&*store, &conn, &Secret::new(k)).map_err(|e| e.to_string())?;
        }
    }
    println!("{}\n", privacy_text(&conn));
    if !conn.is_local() {
        println!("{KEY_CAUTION}\n");
    }
    let cancel = CancelToken::new();
    let check = test_connection(&conn, &*store, &cancel);
    println!("Check: {check}");
    let client = Arc::new(CompatClient::new(conn.clone(), store).map_err(|e| e.to_string())?);
    if conn.model.is_empty() {
        let models = client.list_models(&cancel).map_err(|e| e.to_string())?;
        conn.model = models
            .first()
            .map(|m| m.slug.clone())
            .ok_or("set OPENXER_COMPAT_MODEL: the server lists no models")?;
    }
    println!("Using {}.\n", conn.model);
    let sample = sample_project();
    let p = &sample.current;
    let context = format!(
        "Open project: {} (synthetic sample), {} activities, data date {}.",
        p.model.project.name,
        p.model.activities.len(),
        date_string(p.model.origin_serial, p.model.project.data_date)
    );
    let tools: Arc<dyn Tools> = Arc::new(live_chat::SampleTools {
        project: sample.current.clone(),
    });
    let request = TurnRequest {
        model: conn.model.clone(),
        context,
        user_text: question.to_string(),
        reasoning_effort: None,
    };
    let handle = client.start_turn(conn.new_conversation(), request, tools);
    let mut answer = String::new();
    let mut failed = None;
    for e in handle.events.iter() {
        match e {
            ChatEvent::TextDelta(t) => {
                print!("{t}");
                let _ = std::io::stdout().flush();
            }
            ChatEvent::Message(m) => {
                answer.push_str(&m);
                println!();
            }
            ChatEvent::ToolStarted { name, target, .. } => println!("[tool {name} {target}]"),
            ChatEvent::ToolFinished { ok, .. } => {
                println!("[tool {}]", if ok { "ok" } else { "failed" })
            }
            ChatEvent::ProposedChanges { changes, .. } => {
                println!("[proposed: {}]", changes.summary)
            }
            ChatEvent::TurnFinished(u) if u.is_known() => println!(
                "\n[tokens: input {} (cached {}), output {}, {} request(s)]",
                u.input_tokens, u.cached_tokens, u.output_tokens, u.requests
            ),
            ChatEvent::TurnFinished(_) => println!("\n[tokens: not reported by the server]"),
            ChatEvent::Error(e) => failed = Some(e.to_string()),
            ChatEvent::Cancelled => failed = Some("cancelled".into()),
        }
    }
    let _ = handle.join();
    match failed {
        Some(e) => Err(e),
        None => Ok(answer),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let question = if args.is_empty() {
        "Which activities are in progress, and which of them has the least float?".to_string()
    } else {
        args.join(" ")
    };
    if let Err(e) = run(&question) {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
