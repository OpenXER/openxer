//! Claude Code, driven directly: print mode with JSON lines in and out
//! (`--input-format stream-json --output-format stream-json`), one process
//! for the whole conversation (each message is one more stdin line), and
//! `--resume` with its session id when the process had to be stopped.
use super::{
    classify_failure, tool_target, AgentError, Event, ModelChoice, SessionOptions, ToolCall, Usage,
};
use crate::chat_tools::{self, ChatMode};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// The prefix Claude Code gives the tools of the OpenXER server.
pub const TOOL_PREFIX: &str = "mcp__openxer__";

/// The model alias of the lower cost choice, as `claude --help` names
/// the aliases (fable, opus, sonnet).
pub const LOWER_COST_MODEL: &str = "sonnet";

/// The MCP config file of a session: only the OpenXER chat server.
pub fn mcp_config(options: &SessionOptions) -> Value {
    json!({"mcpServers":{"openxer":{
        "command": options.cli,
        "args": chat_tools::server_arguments(&options.chat_context()),
        "env": {"OPENXER_DATA_DIR": options.data_dir}
    }}})
}

/// The tools the agent may call: exactly the chat tools of the mode.
pub fn allowed_tools(mode: ChatMode) -> Vec<String> {
    chat_tools::tools_for(mode, true)
        .iter()
        .map(|t| format!("{TOOL_PREFIX}{}", t.name))
        .collect()
}

/// The arguments: no built-in tools, the user's settings, hooks and other
/// MCP servers ignored, only the OpenXER tools allowed, nothing asks for
/// permission, and OpenXER's rules as the whole system prompt.
pub fn arguments(
    options: &SessionOptions,
    config: &Path,
    instructions: &Path,
    resume: Option<&str>,
) -> Result<Vec<String>, AgentError> {
    let mut args: Vec<String> = [
        "-p",
        "--verbose",
        "--restricted",
        "--strict-mcp-config",
        "--disable-slash-commands",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--include-partial-messages",
        "--tools",
        "",
        "--permission-prompts",
        "none",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    args.push("--mcp-config".into());
    args.push(config.display().to_string());
    args.push("--allowedTools".into());
    args.push(allowed_tools(options.mode).join(","));
    // The instructions come from a file: they hold the user's own text,
    // which never goes on a command line.
    args.push("--system-prompt-file".into());
    args.push(instructions.display().to_string());
    match &options.model {
        ModelChoice::AgentDefault => {}
        ModelChoice::LowerCost => {
            args.push("--model".into());
            args.push(LOWER_COST_MODEL.into());
        }
        ModelChoice::Named(name) => {
            super::check_model_name(name)?;
            args.push("--model".into());
            args.push(name.clone());
        }
    }
    if let Some(id) = resume {
        if !super::codex::valid_thread_id(id) {
            return Err(AgentError::Failed {
                message: format!("Claude Code gave an unexpected session id {id:?}"),
            });
        }
        args.push("--resume".into());
        args.push(id.into());
    }
    Ok(args)
}

/// The environment the hosted Claude Code runs with: none of the user's
/// CLAUDE.md files (user, project or parent folders) and no auto memory
/// are loaded (`CLAUDE_CODE_DISABLE_CLAUDE_MDS`,
/// `CLAUDE_CODE_DISABLE_AUTO_MEMORY`, both documented for any mode).
pub const ENVIRONMENT: &[(&str, &str)] = &[
    ("CLAUDE_CODE_DISABLE_CLAUDE_MDS", "1"),
    ("CLAUDE_CODE_DISABLE_AUTO_MEMORY", "1"),
];

/// One user message as a stdin line.
pub fn user_line(text: &str) -> String {
    json!({"type":"user","message":{"role":"user","content":[{"type":"text","text":text}]}})
        .to_string()
}

/// Where the session writes its MCP config.
pub fn config_path(options: &SessionOptions) -> PathBuf {
    options.work_dir.join("openxer-chat-mcp.json")
}

/// Claude Code's output lines to events. A pure state machine: it holds
/// only what later lines refer to (open tool calls, the session id).
#[derive(Debug, Default)]
pub struct ClaudeParser {
    /// The conversation's session id, from the `system` `init` line.
    pub session_id: Option<String>,
    open: BTreeMap<String, ToolCall>,
    cost_so_far: f64,
}

fn text_of(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(items) => items
            .iter()
            .filter_map(|i| i["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

impl ClaudeParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// The events of one output line, `now` being when it arrived.
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
        match v["type"].as_str().unwrap_or("") {
            "system" if v["subtype"] == "init" => {
                if let Some(id) = v["session_id"].as_str() {
                    self.session_id = Some(id.to_string());
                }
                let mut events = vec![Event::Ready {
                    model: v["model"].as_str().unwrap_or("").to_string(),
                }];
                let server = v["mcp_servers"]
                    .as_array()
                    .and_then(|list| list.iter().find(|s| s["name"] == "openxer"));
                let status = server
                    .and_then(|s| s["status"].as_str())
                    .unwrap_or("missing");
                if status != "connected" {
                    events.push(Event::Error {
                        error: AgentError::Failed {
                            message: format!("The OpenXER tools did not start ({status})."),
                        },
                    });
                }
                events
            }
            "stream_event" => {
                let e = &v["event"];
                if e["type"] == "content_block_delta" && e["delta"]["type"] == "text_delta" {
                    let text = e["delta"]["text"].as_str().unwrap_or("");
                    if !text.is_empty() {
                        return vec![Event::TextDelta {
                            text: text.to_string(),
                        }];
                    }
                }
                vec![]
            }
            "assistant" => {
                let mut events = vec![];
                for block in v["message"]["content"].as_array().into_iter().flatten() {
                    match block["type"].as_str() {
                        Some("text") => {
                            let text = block["text"].as_str().unwrap_or("");
                            if !text.trim().is_empty() {
                                events.push(Event::Message {
                                    text: text.to_string(),
                                });
                            }
                        }
                        Some("tool_use") => {
                            let id = block["id"].as_str().unwrap_or("").to_string();
                            let name = block["name"].as_str().unwrap_or("");
                            let tool = name.strip_prefix(TOOL_PREFIX).unwrap_or(name).to_string();
                            let input = block["input"].clone();
                            events.push(Event::ToolStarted {
                                id: id.clone(),
                                tool: tool.clone(),
                                target: tool_target(&tool, &input),
                            });
                            self.open.insert(
                                id,
                                ToolCall {
                                    tool,
                                    input,
                                    started: now,
                                },
                            );
                        }
                        _ => {}
                    }
                }
                events
            }
            "user" => {
                let mut events = vec![];
                for block in v["message"]["content"].as_array().into_iter().flatten() {
                    if block["type"] != "tool_result" {
                        continue;
                    }
                    let id = block["tool_use_id"].as_str().unwrap_or("").to_string();
                    let call = self.open.remove(&id);
                    let text = text_of(&block["content"]);
                    events.extend(super::tool_finished(
                        id,
                        call,
                        block["is_error"] == true,
                        &text,
                        now,
                    ));
                }
                events
            }
            "result" => {
                let mut events = vec![];
                let failed = v["is_error"] == true
                    || v["subtype"]
                        .as_str()
                        .is_some_and(|s| s != "success" && !s.is_empty());
                if failed {
                    let message = v["result"]
                        .as_str()
                        .filter(|s| !s.is_empty())
                        .map(str::to_string)
                        .or_else(|| v["errors"].as_array().map(|e| text_of(&json!(e))))
                        .unwrap_or_else(|| {
                            format!("The reply failed ({})", v["subtype"].as_str().unwrap_or(""))
                        });
                    events.push(Event::Error {
                        error: classify_failure(super::Agent::ClaudeCode, &message),
                    });
                }
                let u = &v["usage"];
                let n = |k: &str| u[k].as_u64().unwrap_or(0);
                let cached = n("cache_read_input_tokens");
                // total_cost_usd adds up the whole process; the turn's own
                // cost is per model since the last result.
                let total = v["total_cost_usd"].as_f64();
                let turn = v["model_usage_since_last_result"]
                    .as_object()
                    .map(|models| models.values().filter_map(|m| m["costUSD"].as_f64()).sum())
                    .or_else(|| total.map(|t| (t - self.cost_so_far).max(0.0)));
                if let Some(total) = total {
                    self.cost_so_far = total;
                }
                events.push(Event::TurnFinished {
                    usage: Usage {
                        input_tokens: n("input_tokens") + n("cache_creation_input_tokens") + cached,
                        cached_input_tokens: cached,
                        output_tokens: n("output_tokens"),
                    },
                    cost_usd: turn,
                    cancelled: false,
                });
                // Tools still open when the turn ends never finished.
                self.open.clear();
                events
            }
            _ => vec![],
        }
    }
}
