//! A real sign-in with ChatGPT in your own browser, then one question about
//! the synthetic sample project. For the maintainer to run by hand; tests
//! never run it.
//!
//! ```text
//! cargo run -p openxer-chat --example live_chat -- "Which activities are in progress?"
//! cargo run -p openxer-chat --example live_chat -- --sign-out
//! ```
//!
//! The sign-in is kept in Windows Credential Manager under
//! `OpenXER/chat-example/` (apart from the app's own entries). Set
//! `OPENXER_CHAT_MODEL` to pick a model; otherwise the first listed is used.

use openxer_chat::{
    default_model, ApiConfig, Auth, AuthConfig, CancelToken, ChatClient, ChatEvent, Conversation,
    SignInStatus, ToolDef, ToolOutput, Tools, TurnRequest, WindowsCredentialStore, CONSENT_TEXT,
};
use openxer_engine::model::StatusType;
use openxer_xer::sample_project::sample_project;
use openxer_xer::{date_string, ProjectData};
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::sync::Arc;

const PREFIX: &str = "OpenXER/chat-example/";

/// Read-only tools over the sample project.
pub struct SampleTools {
    pub project: ProjectData,
}

impl SampleTools {
    fn line(&self, i: usize) -> String {
        let a = &self.project.model.activities[i];
        let name = self
            .project
            .details
            .get(&a.task_id)
            .map(|d| d.name.as_str())
            .unwrap_or("");
        let pct = self
            .project
            .details
            .get(&a.task_id)
            .map(|d| d.percent)
            .unwrap_or(0.0);
        let o = self.project.model.origin_serial;
        let float = a
            .total_float
            .map(|f| format!("{:.0}h", f as f64 / 60.0))
            .unwrap_or_default();
        format!(
            "{} | {} | {:?} | {:.0}% | start {} | finish {} | float {}",
            a.task_code,
            name,
            a.status,
            pct,
            date_string(o, a.early_start.or(a.act_start)),
            date_string(o, a.early_finish.or(a.act_finish)),
            float
        )
    }
}

impl Tools for SampleTools {
    fn definitions(&self) -> Vec<ToolDef> {
        vec![ToolDef {
            name: "find_activities".into(),
            description: "Find activities whose ID or name contains the text, or by status \
(not_started, in_progress, completed). Returns at most 40 lines."
                .into(),
            parameters: json!({"type": "object", "properties": {
                "text": {"type": "string"},
                "status": {"type": "string", "enum": ["not_started", "in_progress", "completed"]}}}),
        }]
    }

    fn call(&self, _name: &str, args: &Value, _cancel: &CancelToken) -> ToolOutput {
        let text = args["text"].as_str().unwrap_or("").to_lowercase();
        let status = args["status"].as_str();
        let mut out = Vec::new();
        for (i, a) in self.project.model.activities.iter().enumerate() {
            let name = self
                .project
                .details
                .get(&a.task_id)
                .map(|d| d.name.to_lowercase())
                .unwrap_or_default();
            let st = match a.status {
                StatusType::NotStarted => "not_started",
                StatusType::InProgress => "in_progress",
                StatusType::Completed => "completed",
            };
            if status.is_some_and(|s| s != st) {
                continue;
            }
            if !text.is_empty()
                && !a.task_code.to_lowercase().contains(&text)
                && !name.contains(&text)
            {
                continue;
            }
            out.push(self.line(i));
            if out.len() == 40 {
                out.push("[more not shown; narrow the search]".into());
                break;
            }
        }
        if out.is_empty() {
            ToolOutput::ok("No matching activities.")
        } else {
            ToolOutput::ok(out.join("\n"))
        }
    }
}

fn open_browser(url: &str) -> Result<(), String> {
    // The URL is handed to the default browser; it is not printed, since
    // a returning sign-in carries an ID token hint.
    std::process::Command::new("rundll32.exe")
        .arg("url.dll,FileProtocolHandler")
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn ask_yes(prompt: &str) -> bool {
    print!("{prompt} ");
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    let _ = std::io::stdin().lock().read_line(&mut line);
    line.trim().eq_ignore_ascii_case("yes")
}

/// Sign in if needed, then ask `question`. Returns the answer text. With
/// `ask_consent` false the consent text is shown but not asked about (the
/// live test, where setting OPENXER_CHAT_LIVE=1 was the consent).
pub fn run(question: &str, ask_consent: bool) -> Result<String, String> {
    let store = Arc::new(WindowsCredentialStore::new(PREFIX));
    let auth = Arc::new(Auth::new(AuthConfig::openai(), store));
    let cancel = CancelToken::new();
    match auth.status() {
        SignInStatus::SignedIn {
            account,
            plan_usage: true,
        } => println!("Signed in as {}.", account.label()),
        _ => {
            println!("{CONSENT_TEXT}\n");
            if ask_consent && !ask_yes("Type yes to sign in with ChatGPT in your browser:") {
                return Err("not signed in".into());
            }
            println!("Waiting for the browser (5 minutes)...");
            let account = auth
                .sign_in(&open_browser, &cancel)
                .map_err(|e| e.to_string())?;
            println!("Signed in as {}.", account.label());
            if !auth.plan_usage_enabled() {
                return Err("signed in, but plan usage was not allowed".into());
            }
        }
    }
    let client = ChatClient::new(auth.clone(), ApiConfig::openai());
    let models = client.list_models(&cancel).map_err(|e| e.to_string())?;
    println!(
        "Models: {}",
        models
            .iter()
            .map(|m| m.slug.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
    let model = match std::env::var("OPENXER_CHAT_MODEL") {
        Ok(m) if !m.is_empty() => m,
        _ => default_model(&models)
            .ok_or("the account lists no models")?
            .slug
            .clone(),
    };
    println!("Using {model}.\n");
    let sample = sample_project();
    let p = &sample.current;
    let context = format!(
        "Open project: {} (synthetic sample), {} activities, data date {}.",
        p.model.project.name,
        p.model.activities.len(),
        date_string(p.model.origin_serial, p.model.project.data_date)
    );
    let tools: Arc<dyn Tools> = Arc::new(SampleTools {
        project: sample.current.clone(),
    });
    let (tx, rx) = std::sync::mpsc::channel();
    let mut conv = Conversation::new();
    let request = TurnRequest {
        model,
        context,
        user_text: question.to_string(),
        reasoning_effort: None,
    };
    let printer = std::thread::spawn(move || {
        let mut answer = String::new();
        for e in rx {
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
                ChatEvent::ToolFinished { ok, duration, .. } => {
                    println!(
                        "[tool {} in {} ms]",
                        if ok { "ok" } else { "failed" },
                        duration.as_millis()
                    )
                }
                ChatEvent::ProposedChanges { changes, .. } => {
                    println!("[proposed: {}]", changes.summary)
                }
                ChatEvent::TurnFinished(u) => println!(
                    "\n[tokens: input {} (cached {}), output {} (reasoning {}), {} request(s)]",
                    u.input_tokens,
                    u.cached_tokens,
                    u.output_tokens,
                    u.reasoning_tokens,
                    u.requests
                ),
                ChatEvent::Error(e) => println!("\n[error: {e}]"),
                ChatEvent::Cancelled => println!("\n[cancelled]"),
            }
        }
        answer
    });
    let result = client.run_turn(&mut conv, &request, &tools, &tx, &cancel);
    drop(tx);
    let answer = printer.join().unwrap_or_default();
    result.map(|_| answer).map_err(|e| e.to_string())
}

#[allow(dead_code)]
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--sign-out") {
        let store = Arc::new(WindowsCredentialStore::new(PREFIX));
        let auth = Auth::new(AuthConfig::openai(), store);
        match auth.sign_out(&CancelToken::new()) {
            Ok(o) if o.revoked => println!("Signed out; the sign-in was revoked."),
            Ok(_) => println!(
                "Signed out on this computer; revocation was not confirmed. Disconnect OpenXER in ChatGPT settings, Security and login."
            ),
            Err(e) => println!("Sign-out failed: {e}"),
        }
        return;
    }
    let question = if args.is_empty() {
        "Which activities are in progress, and which of them has the least float?".to_string()
    } else {
        args.join(" ")
    };
    if let Err(e) = run(&question, true) {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
