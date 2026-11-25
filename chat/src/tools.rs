//! The seam to the schedule: a set of tools the model may call. The app
//! passes an implementation (the automation crate's chat tool set); tests
//! pass fakes.

use crate::util::CancelToken;
use serde_json::Value;

/// One tool the model may call: a function with a JSON Schema for its
/// arguments.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolDef {
    pub name: String,
    /// One or two short sentences; every word is resent each turn.
    pub description: String,
    /// JSON Schema of the arguments object.
    pub parameters: Value,
}

/// Changes a propose-only tool worked out but did not make. The window
/// shows them and applies them only when the user says so.
#[derive(Clone, Debug, PartialEq)]
pub struct ProposedChanges {
    /// One line for the user ("Add 2 links, change 1 duration").
    pub summary: String,
    /// Whatever the apply step needs, as the tool produced it.
    pub payload: Value,
}

/// What a tool call returned.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolOutput {
    pub ok: bool,
    /// The text sent back to the model (capped by the conversation budget).
    pub text: String,
    pub proposed: Option<ProposedChanges>,
}

impl ToolOutput {
    pub fn ok(text: impl Into<String>) -> ToolOutput {
        ToolOutput {
            ok: true,
            text: text.into(),
            proposed: None,
        }
    }
    pub fn error(text: impl Into<String>) -> ToolOutput {
        ToolOutput {
            ok: false,
            text: text.into(),
            proposed: None,
        }
    }
    pub fn proposed(text: impl Into<String>, changes: ProposedChanges) -> ToolOutput {
        ToolOutput {
            ok: true,
            text: text.into(),
            proposed: Some(changes),
        }
    }
}

/// A set of tools. `call` runs on a worker thread of its own; when the user
/// cancels, the turn stops waiting at once and the result is dropped, so a
/// long tool should also check `cancel` and return early.
pub trait Tools: Send + Sync {
    /// The tools, in a stable order (they lead every request, so a stable
    /// list keeps the prompt cache warm).
    fn definitions(&self) -> Vec<ToolDef>;
    /// Run tool `name` with its parsed arguments.
    fn call(&self, name: &str, arguments: &Value, cancel: &CancelToken) -> ToolOutput;
    /// A short target for the progress line ("A1010", "WBS 1.2"). The
    /// default takes the first string argument.
    fn target(&self, _name: &str, arguments: &Value) -> String {
        default_target(arguments)
    }
}

/// The first short string or number among the arguments, at most 40 chars.
pub fn default_target(arguments: &Value) -> String {
    const KEYS: &[&str] = &[
        "id",
        "activity",
        "code",
        "activity_id",
        "wbs",
        "query",
        "name",
    ];
    let Some(obj) = arguments.as_object() else {
        return String::new();
    };
    let pick = |v: &Value| match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    };
    let found = KEYS
        .iter()
        .find_map(|k| obj.get(*k).and_then(pick))
        .or_else(|| obj.values().find_map(pick))
        .unwrap_or_default();
    crate::util::cut(&found, 40).to_string()
}

/// No tools: plain chat.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoTools;

impl Tools for NoTools {
    fn definitions(&self) -> Vec<ToolDef> {
        Vec::new()
    }
    fn call(&self, name: &str, _arguments: &Value, _cancel: &CancelToken) -> ToolOutput {
        ToolOutput::error(format!("There is no tool named {name}."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn default_target_prefers_identifying_keys() {
        assert_eq!(
            default_target(&json!({"fields": ["a"], "id": "A1010"})),
            "A1010"
        );
        assert_eq!(default_target(&json!({"limit": 5})), "5");
        assert_eq!(default_target(&json!("x")), "");
        assert_eq!(default_target(&json!({"query": "x".repeat(100)})).len(), 40);
    }
}
