//! The conversation and the token budgets. The endpoint keeps nothing
//! between requests (`store: false`, no `previous_response_id`), so the
//! whole history is resent every turn. To keep that small:
//!
//! - a tool result is capped at [`Budgets::max_tool_result_chars`], with a
//!   marker telling the model to ask for less;
//! - once a turn is over, its large tool results are replaced by a one-line
//!   summary (the model's answer already says what mattered);
//! - reasoning items are kept only within the turn that made them;
//! - when the history is still over [`Budgets::max_history_chars`], the
//!   oldest turns are dropped whole.
//!
//! The request starts with what does not change (instructions, then the
//! tools in a fixed order), then the history oldest first, so the prompt
//! cache can reuse the prefix; [`Conversation::cache_key`] is sent as
//! `prompt_cache_key`.

use serde_json::{json, Value};

/// The fixed part of the instructions. Short on purpose: it is resent with
/// every request.
pub const BASE_INSTRUCTIONS: &str = "You are the assistant in OpenXER, a CPM scheduling app. \
Answer questions about the open schedule. Read data with the tools; never guess IDs, dates \
or values. Ask tools for only what you need. Answer briefly in plain words. You cannot change \
the schedule: changes are only proposed through a tool and the user decides.";

/// Limits that keep each request small. Pinned by tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Budgets {
    /// A single tool result longer than this is cut, with a marker.
    pub max_tool_result_chars: usize,
    /// Tool results of finished turns longer than this become a summary.
    pub old_result_keep_chars: usize,
    /// How much of an old result the summary quotes.
    pub summary_quote_chars: usize,
    /// Model requests per turn (each tool round is one); the last one is
    /// asked to answer without tools.
    pub max_rounds: u32,
    /// The history is trimmed, oldest turns first, to about this size.
    pub max_history_chars: usize,
    /// The caller's context text (appended to the instructions) is cut to this.
    pub max_context_chars: usize,
}

impl Default for Budgets {
    fn default() -> Self {
        Budgets {
            max_tool_result_chars: 12_000,
            old_result_keep_chars: 400,
            summary_quote_chars: 120,
            max_rounds: 8,
            max_history_chars: 80_000,
            max_context_chars: 4_000,
        }
    }
}

/// Cut a tool result to the budget, ending with a marker that says so.
pub fn cap_tool_result(text: &str, budgets: &Budgets) -> String {
    let max = budgets.max_tool_result_chars;
    if text.chars().count() <= max {
        return text.to_string();
    }
    let total = text.chars().count();
    let head: String = text.chars().take(max).collect();
    format!(
        "{head}\n[Truncated: the result was {total} characters and only the first {max} are shown. \
Ask for less: narrow the filter, request fewer fields, or ask for the next part.]"
    )
}

/// The summary line that replaces a large tool result of a finished turn.
pub fn summarize_result(name: &str, target: &str, text: &str, budgets: &Budgets) -> String {
    let total = text.chars().count();
    let first: String = text
        .lines()
        .next()
        .unwrap_or("")
        .chars()
        .take(budgets.summary_quote_chars)
        .collect();
    let target = if target.is_empty() {
        String::new()
    } else {
        format!(" ({target})")
    };
    format!(
        "[Earlier result of {name}{target}, {total} characters, removed to save space; it began: \
\"{first}\". Call the tool again if you need it.]"
    )
}

/// Who said a message, for display.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Entry {
    User {
        turn: u32,
        text: String,
    },
    Assistant {
        turn: u32,
        text: String,
    },
    Call {
        turn: u32,
        call_id: String,
        name: String,
        namespace: Option<String>,
        arguments: String,
    },
    Output {
        turn: u32,
        call_id: String,
        text: String,
        summary: String,
    },
    Reasoning {
        turn: u32,
        item: Value,
    },
}

impl Entry {
    fn turn(&self) -> u32 {
        match self {
            Entry::User { turn, .. }
            | Entry::Assistant { turn, .. }
            | Entry::Call { turn, .. }
            | Entry::Output { turn, .. }
            | Entry::Reasoning { turn, .. } => *turn,
        }
    }
}

/// A chat: the history the model sees, plus the budgets. Plain data;
/// clone it freely.
#[derive(Clone, Debug)]
pub struct Conversation {
    pub(crate) entries: Vec<Entry>,
    /// Turns completed so far; the next turn has this number.
    pub(crate) turn: u32,
    cache_key: String,
    pub budgets: Budgets,
    /// The fixed instructions, when the app gives its own in place of
    /// [`BASE_INSTRUCTIONS`].
    base: Option<String>,
}

impl Default for Conversation {
    fn default() -> Self {
        Conversation::new()
    }
}

impl Conversation {
    pub fn new() -> Conversation {
        Conversation {
            entries: Vec::new(),
            turn: 0,
            cache_key: format!("openxer-{}", crate::util::random_token(12)),
            budgets: Budgets::default(),
            base: None,
        }
    }

    /// Use `text` as the fixed instructions in place of
    /// [`BASE_INSTRUCTIONS`] (the app's one chat prompt). Keep it the same
    /// from turn to turn so the prompt cache holds.
    pub fn set_instructions(&mut self, text: impl Into<String>) {
        self.base = Some(text.into());
    }

    pub fn with_budgets(mut self, budgets: Budgets) -> Conversation {
        self.budgets = budgets;
        self
    }

    /// A stable key for this conversation's prompt cache.
    pub fn cache_key(&self) -> &str {
        &self.cache_key
    }

    /// The user and assistant messages so far, for display.
    pub fn messages(&self) -> Vec<(Role, String)> {
        self.entries
            .iter()
            .filter_map(|e| match e {
                Entry::User { text, .. } => Some((Role::User, text.clone())),
                Entry::Assistant { text, .. } => Some((Role::Assistant, text.clone())),
                _ => None,
            })
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Forget everything (a new chat); the cache key changes too.
    pub fn clear(&mut self) {
        let base = self.base.take();
        *self = Conversation::new().with_budgets(self.budgets.clone());
        self.base = base;
    }

    /// Full instructions: the fixed part, then the caller's context.
    pub fn instructions(&self, context: &str) -> String {
        let base = self.base.as_deref().unwrap_or(BASE_INSTRUCTIONS);
        let context = context.trim();
        if context.is_empty() {
            return base.to_string();
        }
        let cut = crate::util::cut(context, self.budgets.max_context_chars);
        format!("{base}\n\n{cut}")
    }

    pub(crate) fn push(&mut self, e: Entry) {
        self.entries.push(e);
    }

    /// End a successful turn: drop its reasoning items, then trim.
    pub(crate) fn finish_turn(&mut self) {
        let t = self.turn;
        self.entries
            .retain(|e| !matches!(e, Entry::Reasoning { turn, .. } if *turn == t));
        self.turn += 1;
        self.trim_history();
    }

    /// Drop whole oldest turns while the history is over budget (the
    /// newest turn is always kept).
    fn trim_history(&mut self) {
        while self.history_chars() > self.budgets.max_history_chars {
            let Some(oldest) = self.entries.first().map(Entry::turn) else {
                return;
            };
            if self.entries.iter().all(|e| e.turn() == oldest) {
                return;
            }
            self.entries.retain(|e| e.turn() != oldest);
        }
    }

    /// Characters the history sends now (a proxy for its token count).
    pub fn history_chars(&self) -> usize {
        self.input_items().iter().map(|v| v.to_string().len()).sum()
    }

    /// The history as Chat Completions `messages` (after the system
    /// message), with the same trimming as [`Conversation::input_items`]:
    /// an assistant text and the tool calls right after it make one
    /// assistant message; reasoning items are not sent.
    pub(crate) fn chat_messages(&self) -> Vec<Value> {
        let current = self.turn;
        let keep = self.budgets.old_result_keep_chars;
        let mut out: Vec<Value> = Vec::with_capacity(self.entries.len());
        // Whether the last message pushed is an assistant message that may
        // take more tool calls.
        let mut open_assistant = false;
        for e in &self.entries {
            match e {
                Entry::User { text, .. } => {
                    out.push(json!({"role": "user", "content": text}));
                    open_assistant = false;
                }
                Entry::Assistant { text, .. } => {
                    out.push(json!({"role": "assistant", "content": text}));
                    open_assistant = true;
                }
                Entry::Call {
                    call_id,
                    name,
                    arguments,
                    ..
                } => {
                    let call = json!({"id": call_id, "type": "function",
                        "function": {"name": name, "arguments": arguments}});
                    match out.last_mut() {
                        Some(last) if open_assistant => {
                            if !last["tool_calls"].is_array() {
                                last["tool_calls"] = json!([]);
                            }
                            if let Some(a) = last["tool_calls"].as_array_mut() {
                                a.push(call);
                            }
                        }
                        _ => {
                            out.push(json!({"role": "assistant", "content": null,
                                "tool_calls": [call]}));
                            open_assistant = true;
                        }
                    }
                }
                Entry::Output {
                    turn,
                    call_id,
                    text,
                    summary,
                } => {
                    let content = if *turn == current || text.chars().count() <= keep {
                        text
                    } else {
                        summary
                    };
                    out.push(json!({"role": "tool", "tool_call_id": call_id,
                        "content": content}));
                    open_assistant = false;
                }
                Entry::Reasoning { .. } => {}
            }
        }
        out
    }

    /// The `input` items for the next request.
    pub(crate) fn input_items(&self) -> Vec<Value> {
        let current = self.turn;
        let keep = self.budgets.old_result_keep_chars;
        let mut out = Vec::with_capacity(self.entries.len());
        for e in &self.entries {
            match e {
                Entry::User { text, .. } => {
                    out.push(json!({"role": "user", "content": text}));
                }
                Entry::Assistant { text, .. } => {
                    out.push(json!({"role": "assistant", "content": text}));
                }
                Entry::Call {
                    call_id,
                    name,
                    namespace,
                    arguments,
                    ..
                } => {
                    let mut v = json!({"type": "function_call", "call_id": call_id,
                        "name": name, "arguments": arguments});
                    if let Some(ns) = namespace {
                        v["namespace"] = json!(ns);
                    }
                    out.push(v);
                }
                Entry::Output {
                    turn,
                    call_id,
                    text,
                    summary,
                } => {
                    let output = if *turn == current || text.chars().count() <= keep {
                        text
                    } else {
                        summary
                    };
                    out.push(json!({"type": "function_call_output", "call_id": call_id,
                        "output": output}));
                }
                Entry::Reasoning { turn, item } => {
                    if *turn == current {
                        out.push(item.clone());
                    }
                }
            }
        }
        out
    }
}
