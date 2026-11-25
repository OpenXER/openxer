//! Turns the Responses API event stream into output items and UI events.

use crate::sse::SseEvent;
use serde_json::Value;
use std::collections::BTreeMap;

/// Token counts the API reported.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Usage {
    pub input_tokens: u64,
    /// Input tokens served from the prompt cache (part of `input_tokens`).
    pub cached_tokens: u64,
    pub output_tokens: u64,
    /// Reasoning tokens (part of `output_tokens`).
    pub reasoning_tokens: u64,
    /// Model requests whose token use the server reported. `0` means the
    /// use is unknown (a compatible server that does not report it): the
    /// counts are then all zero, never a guess. See [`Usage::is_known`].
    pub requests: u32,
}

impl Usage {
    /// Whether the server reported the token use.
    pub fn is_known(&self) -> bool {
        self.requests > 0
    }

    pub fn add(&mut self, o: &Usage) {
        self.input_tokens += o.input_tokens;
        self.cached_tokens += o.cached_tokens;
        self.output_tokens += o.output_tokens;
        self.reasoning_tokens += o.reasoning_tokens;
        self.requests += o.requests;
    }

    fn from_json(u: &Value) -> Usage {
        Usage {
            input_tokens: u["input_tokens"].as_u64().unwrap_or(0),
            cached_tokens: u["input_tokens_details"]["cached_tokens"]
                .as_u64()
                .unwrap_or(0),
            output_tokens: u["output_tokens"].as_u64().unwrap_or(0),
            reasoning_tokens: u["output_tokens_details"]["reasoning_tokens"]
                .as_u64()
                .unwrap_or(0),
            requests: 1,
        }
    }
}

/// One finished output item.
#[derive(Clone, Debug, PartialEq)]
pub enum OutputItem {
    Message(String),
    Call {
        call_id: String,
        name: String,
        namespace: Option<String>,
        arguments: String,
    },
    /// Kept as sent, to resend within the same turn.
    Reasoning(Value),
}

/// How the stream ended.
#[derive(Clone, Debug, PartialEq)]
pub enum End {
    Completed(Usage),
    /// `response.failed` or an `error` event.
    Failed {
        code: String,
        message: String,
        param: Option<String>,
    },
    Incomplete {
        reason: String,
        usage: Usage,
    },
}

/// What one event produced for the UI.
#[derive(Clone, Debug, PartialEq)]
pub enum Emit {
    TextDelta(String),
    Message(String),
}

#[derive(Debug)]
enum Building {
    Message(String),
    Call {
        call_id: String,
        name: String,
        namespace: Option<String>,
        arguments: String,
    },
    Reasoning,
    Other,
}

/// Feed it SSE events in order; read `items` and `end` when done.
#[derive(Debug, Default)]
pub struct Decoder {
    building: BTreeMap<u64, Building>,
    finished: BTreeMap<u64, OutputItem>,
    pub end: Option<End>,
    /// Text deltas or items have been emitted.
    pub emitted: bool,
}

fn text_of(v: &Value) -> String {
    v.as_str().unwrap_or("").to_string()
}

fn message_text(item: &Value) -> Option<String> {
    let parts = item["content"].as_array()?;
    let mut s = String::new();
    for p in parts {
        if p["type"] == "output_text" {
            s.push_str(p["text"].as_str().unwrap_or(""));
        }
    }
    Some(s)
}

fn failure(err: &Value) -> End {
    End::Failed {
        code: crate::util::safe_code(
            err["code"]
                .as_str()
                .or(err["type"].as_str())
                .unwrap_or("error"),
        ),
        message: crate::util::cut(err["message"].as_str().unwrap_or(""), 300).to_string(),
        param: err["param"].as_str().map(str::to_string),
    }
}

impl Decoder {
    pub fn new() -> Decoder {
        Decoder::default()
    }

    /// Output items in output order (finished ones, then any still being
    /// built that carry a whole function call).
    pub fn items(&self) -> Vec<OutputItem> {
        let mut out: BTreeMap<u64, OutputItem> = self.finished.clone();
        for (i, b) in &self.building {
            if out.contains_key(i) {
                continue;
            }
            match b {
                Building::Message(t) if !t.is_empty() => {
                    out.insert(*i, OutputItem::Message(t.clone()));
                }
                Building::Call {
                    call_id,
                    name,
                    namespace,
                    arguments,
                } => {
                    out.insert(
                        *i,
                        OutputItem::Call {
                            call_id: call_id.clone(),
                            name: name.clone(),
                            namespace: namespace.clone(),
                            arguments: arguments.clone(),
                        },
                    );
                }
                _ => {}
            }
        }
        out.into_values().collect()
    }

    /// Handle one event. Unknown events are ignored.
    pub fn handle(&mut self, ev: &SseEvent) -> Vec<Emit> {
        if ev.data.trim() == "[DONE]" {
            return Vec::new();
        }
        let Ok(v) = serde_json::from_str::<Value>(&ev.data) else {
            return Vec::new();
        };
        let kind = v["type"]
            .as_str()
            .map(str::to_string)
            .or_else(|| ev.event.clone())
            .unwrap_or_default();
        let index = v["output_index"].as_u64().unwrap_or(0);
        let mut out = Vec::new();
        match kind.as_str() {
            "response.output_item.added" => {
                let item = &v["item"];
                let b = match item["type"].as_str() {
                    Some("message") => Building::Message(String::new()),
                    Some("function_call") => Building::Call {
                        call_id: text_of(&item["call_id"]),
                        name: text_of(&item["name"]),
                        namespace: item["namespace"].as_str().map(str::to_string),
                        arguments: text_of(&item["arguments"]),
                    },
                    Some("reasoning") => Building::Reasoning,
                    _ => Building::Other,
                };
                self.building.insert(index, b);
            }
            "response.output_text.delta" => {
                let d = text_of(&v["delta"]);
                if let Building::Message(t) = self
                    .building
                    .entry(index)
                    .or_insert_with(|| Building::Message(String::new()))
                {
                    t.push_str(&d);
                }
                if !d.is_empty() {
                    self.emitted = true;
                    out.push(Emit::TextDelta(d));
                }
            }
            "response.output_text.done" => {
                if let (Some(Building::Message(t)), Some(full)) =
                    (self.building.get_mut(&index), v["text"].as_str())
                {
                    *t = full.to_string();
                }
            }
            "response.function_call_arguments.delta" => {
                if let Some(Building::Call { arguments, .. }) = self.building.get_mut(&index) {
                    arguments.push_str(v["delta"].as_str().unwrap_or(""));
                }
            }
            "response.function_call_arguments.done" => {
                if let (Some(Building::Call { arguments, .. }), Some(full)) =
                    (self.building.get_mut(&index), v["arguments"].as_str())
                {
                    *arguments = full.to_string();
                }
            }
            "response.output_item.done" => {
                let item = &v["item"];
                match item["type"].as_str() {
                    Some("message") => {
                        let built = match self.building.get(&index) {
                            Some(Building::Message(t)) => t.clone(),
                            _ => String::new(),
                        };
                        let text = message_text(item)
                            .filter(|t| !t.is_empty())
                            .unwrap_or(built);
                        self.emitted = true;
                        out.push(Emit::Message(text.clone()));
                        self.finished.insert(index, OutputItem::Message(text));
                    }
                    Some("function_call") => {
                        let (b_id, b_name, b_ns, b_args) = match self.building.get(&index) {
                            Some(Building::Call {
                                call_id,
                                name,
                                namespace,
                                arguments,
                            }) => (
                                call_id.clone(),
                                name.clone(),
                                namespace.clone(),
                                arguments.clone(),
                            ),
                            _ => Default::default(),
                        };
                        let pick = |k: &str, built: String| {
                            item[k]
                                .as_str()
                                .filter(|s| !s.is_empty())
                                .map(str::to_string)
                                .unwrap_or(built)
                        };
                        self.emitted = true;
                        self.finished.insert(
                            index,
                            OutputItem::Call {
                                call_id: pick("call_id", b_id),
                                name: pick("name", b_name),
                                namespace: item["namespace"].as_str().map(str::to_string).or(b_ns),
                                arguments: pick("arguments", b_args),
                            },
                        );
                    }
                    Some("reasoning") => {
                        let mut r = item.clone();
                        if let Some(o) = r.as_object_mut() {
                            // Not stored server side, so ids refer to nothing.
                            o.remove("id");
                            o.remove("status");
                        }
                        self.finished.insert(index, OutputItem::Reasoning(r));
                    }
                    _ => {}
                }
            }
            "response.completed" => {
                self.end = Some(End::Completed(Usage::from_json(&v["response"]["usage"])));
            }
            "response.incomplete" => {
                let r = &v["response"];
                self.end = Some(End::Incomplete {
                    reason: crate::util::safe_code(
                        r["incomplete_details"]["reason"]
                            .as_str()
                            .unwrap_or("unknown"),
                    ),
                    usage: Usage::from_json(&r["usage"]),
                });
            }
            "response.failed" => {
                self.end = Some(failure(&v["response"]["error"]));
            }
            "error" => {
                let e = if v["error"].is_object() {
                    &v["error"]
                } else {
                    &v
                };
                self.end = Some(failure(e));
            }
            _ => {}
        }
        out
    }
}
