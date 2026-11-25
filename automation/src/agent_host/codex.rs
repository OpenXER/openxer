//! Codex, driven directly: `codex exec --json`, one process per turn, and
//! `codex exec resume <thread id>` for each later turn of the conversation.
use super::{
    classify_failure, tool_target, Agent, AgentError, Event, ModelChoice, SessionOptions, ToolCall,
    Usage,
};
use crate::chat_tools;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

/// The lower cost choice: a model `codex debug models` lists as fast and
/// affordable, at low reasoning effort.
pub const LOWER_COST_MODEL: &str = "gpt-6-luna";
pub const LOWER_COST_EFFORT: &str = "low";

/// Built-in tools and extras switched off (`codex features list` names):
/// commands, sub-agents, plugins, apps, skills, browsing, images, hooks
/// and memories. What stays is the model's script tool that only calls
/// other tools (no file system, no network) and the OpenXER server.
pub const DISABLED_FEATURES: &[&str] = &[
    "shell_tool",
    "unified_exec",
    "multi_agent",
    "multi_agent_v2",
    "sleep_tool",
    "skill_search",
    "plugins",
    "apps",
    "goals",
    "hooks",
    "memories",
    "image_generation",
    "view_image",
    "browser_use",
    "browser_use_external",
    "computer_use",
    "in_app_browser",
    "tool_suggest",
];

/// A TOML string for a `-c key=value` override.
fn toml_string(text: &str) -> String {
    toml_edit::Value::from(text).decorated("", "").to_string()
}

/// Where the session writes the chat's instructions, which replace
/// Codex's own (`model_instructions_file`).
pub fn rules_path(options: &SessionOptions) -> PathBuf {
    super::instructions_file(options)
}

/// Whether `id` is a thread id as Codex gives them (a UUID): the only
/// value from the agent's output that goes back on a command line.
pub fn valid_thread_id(id: &str) -> bool {
    id.len() == 36
        && id.chars().enumerate().all(|(i, c)| {
            if [8, 13, 18, 23].contains(&i) {
                c == '-'
            } else {
                c.is_ascii_hexdigit()
            }
        })
}

/// The arguments of one turn: fixed flags and paths only. The prompt is
/// read from stdin (`-`), after `--` so nothing in it is ever a flag.
/// `resume` is the thread to continue.
pub fn arguments(
    options: &SessionOptions,
    resume: Option<&str>,
) -> Result<Vec<String>, AgentError> {
    let mut args: Vec<String> = vec!["exec".into()];
    if let Some(thread) = resume {
        if !valid_thread_id(thread) {
            return Err(AgentError::Failed {
                message: format!("Codex gave an unexpected thread id {thread:?}"),
            });
        }
        args.push("resume".into());
    }
    for a in [
        "--json",
        "--skip-git-repo-check",
        "--ignore-user-config",
        "--ignore-rules",
    ] {
        args.push(a.into());
    }
    let mut config = |key: &str, value: String| {
        args.push("-c".into());
        args.push(format!("{key}={value}"));
    };
    config("sandbox_mode", toml_string("read-only"));
    config("approval_policy", toml_string("never"));
    config("web_search", toml_string("disabled"));
    config("skills.include_instructions", "false".into());
    config("include_permissions_instructions", "false".into());
    config("include_environment_context", "false".into());
    // No AGENTS.md from the work folder or above it.
    config("project_doc_max_bytes", "0".into());
    config(
        "model_instructions_file",
        toml_string(&rules_path(options).display().to_string()),
    );
    config(
        "mcp_servers.openxer.command",
        toml_string(&options.cli.display().to_string()),
    );
    let server_args: Vec<String> = chat_tools::server_arguments(&options.chat_context())
        .iter()
        .map(|a| toml_string(a))
        .collect();
    config(
        "mcp_servers.openxer.args",
        format!("[{}]", server_args.join(", ")),
    );
    // Codex's default wait for a server to start is short; a busy PC can
    // miss it, and then the chat has no tools at all.
    config("mcp_servers.openxer.startup_timeout_sec", "30".into());
    // No answer without the tools: the server must be ready at startup.
    config("mcp_servers.openxer.required", "true".into());
    config(
        "mcp_servers.openxer.env",
        format!(
            "{{OPENXER_DATA_DIR={}}}",
            toml_string(&options.data_dir.display().to_string())
        ),
    );
    match &options.model {
        ModelChoice::AgentDefault => {}
        ModelChoice::LowerCost => {
            config("model_reasoning_effort", toml_string(LOWER_COST_EFFORT));
            args.push("-m".into());
            args.push(LOWER_COST_MODEL.into());
        }
        ModelChoice::Named(name) => {
            super::check_model_name(name)?;
            args.push("-m".into());
            args.push(name.clone());
        }
    }
    for feature in DISABLED_FEATURES {
        args.push("--disable".into());
        args.push((*feature).into());
    }
    args.push("--".into());
    if let Some(thread) = resume {
        args.push(thread.into());
    }
    args.push("-".into());
    Ok(args)
}

/// Codex's output lines to events. A pure state machine like the Claude
/// parser; `thread_id` is the conversation to resume.
#[derive(Debug, Default)]
pub struct CodexParser {
    pub thread_id: Option<String>,
    /// The model label `Ready` reports (Codex does not name it).
    pub model: String,
    open: BTreeMap<String, ToolCall>,
}

/// A tool call's arguments: an object, or JSON text of one.
fn arguments_of(item: &Value) -> Value {
    match &item["arguments"] {
        Value::String(s) => serde_json::from_str(s).unwrap_or(Value::Null),
        other => other.clone(),
    }
}

fn result_text(item: &Value) -> String {
    let result = &item["result"];
    let content = result["content"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|i| i["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();
    if !content.is_empty() {
        return content;
    }
    if !result["structured_content"].is_null() {
        return result["structured_content"].to_string();
    }
    item["error"]["message"]
        .as_str()
        .map(str::to_string)
        .unwrap_or_default()
}

impl CodexParser {
    pub fn new(model: impl Into<String>) -> Self {
        CodexParser {
            model: model.into(),
            ..Default::default()
        }
    }

    pub fn parse_line(&mut self, line: &str, now: Instant) -> Vec<Event> {
        let line = line.trim();
        if line.is_empty() {
            return vec![];
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            return vec![Event::Error {
                error: AgentError::BadOutput {
                    line: super::clip(line, 200),
                },
            }];
        };
        let item = &v["item"];
        match v["type"].as_str().unwrap_or("") {
            "thread.started" => {
                self.thread_id = v["thread_id"].as_str().map(str::to_string);
                vec![Event::Ready {
                    model: self.model.clone(),
                }]
            }
            "item.started" if item["type"] == "mcp_tool_call" => {
                let id = item["id"].as_str().unwrap_or("").to_string();
                let tool = item["tool"].as_str().unwrap_or("").to_string();
                let input = arguments_of(item);
                let event = Event::ToolStarted {
                    id: id.clone(),
                    tool: tool.clone(),
                    target: tool_target(&tool, &input),
                };
                self.open.insert(
                    id,
                    ToolCall {
                        tool,
                        input,
                        started: now,
                    },
                );
                vec![event]
            }
            "item.completed" => match item["type"].as_str() {
                Some("agent_message") => {
                    let text = item["text"].as_str().unwrap_or("");
                    if text.trim().is_empty() {
                        vec![]
                    } else {
                        vec![Event::Message {
                            text: text.to_string(),
                        }]
                    }
                }
                Some("mcp_tool_call") => {
                    let id = item["id"].as_str().unwrap_or("").to_string();
                    let mut events = vec![];
                    let call = match self.open.remove(&id) {
                        Some(call) => call,
                        None => {
                            // Finished without a start line: report both.
                            let tool = item["tool"].as_str().unwrap_or("").to_string();
                            let input = arguments_of(item);
                            events.push(Event::ToolStarted {
                                id: id.clone(),
                                tool: tool.clone(),
                                target: tool_target(&tool, &input),
                            });
                            ToolCall {
                                tool,
                                input,
                                started: now,
                            }
                        }
                    };
                    let failed = item["status"] != "completed" || !item["error"].is_null();
                    events.extend(super::tool_finished(
                        id,
                        Some(call),
                        failed,
                        &result_text(item),
                        now,
                    ));
                    events
                }
                Some("error") => vec![Event::Error {
                    error: classify_failure(Agent::Codex, item["message"].as_str().unwrap_or("")),
                }],
                _ => vec![],
            },
            "turn.completed" => {
                let u = &v["usage"];
                let n = |k: &str| u[k].as_u64().unwrap_or(0);
                self.open.clear();
                vec![Event::TurnFinished {
                    usage: Usage {
                        input_tokens: n("input_tokens"),
                        cached_input_tokens: n("cached_input_tokens"),
                        output_tokens: n("output_tokens"),
                    },
                    cost_usd: None,
                    cancelled: false,
                }]
            }
            "turn.failed" => {
                self.open.clear();
                let message = v["error"]["message"].as_str().unwrap_or("The turn failed");
                vec![
                    Event::Error {
                        error: classify_failure(Agent::Codex, message),
                    },
                    Event::TurnFinished {
                        usage: Usage::default(),
                        cost_usd: None,
                        cancelled: false,
                    },
                ]
            }
            "error" => vec![Event::Error {
                error: classify_failure(Agent::Codex, v["message"].as_str().unwrap_or("")),
            }],
            _ => vec![],
        }
    }
}
