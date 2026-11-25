//! Per-project conversation history, per-project standing rules and global
//! saved prompt snippets, stored in OpenXER's preferences table.
use super::backend::{ChatContext, Selection};
use super::model::Conversation;

/// Maximum archived conversations kept per project.
pub(crate) const MAX_HISTORY: usize = 15;
/// Preference key for global saved user prompt snippets.
pub(crate) const SAVED_PROMPTS_KEY: &str = "chat_saved_prompts_v1";

/// One archived conversation for a project.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct SavedChat {
    pub id: u64,
    pub title: String,
    #[serde(default)]
    pub agent: Option<String>,
    pub conv: Conversation,
}

/// One user-saved reusable prompt snippet.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct SavedPrompt {
    pub name: String,
    pub text: String,
}

fn hash_key(project_key: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    project_key.hash(&mut h);
    format!("{:016x}", h.finish())
}

pub(crate) fn history_pref_key(project_key: &str) -> String {
    format!("chat_history_v1_{}", hash_key(project_key))
}

pub(crate) fn project_rules_pref_key(project_key: &str) -> String {
    format!("chat_project_rules_v1_{}", hash_key(project_key))
}

/// Archive `conv` into `history` (deduplicating by `active_id` if already stored,
/// placing the newest entry first, capped at [`MAX_HISTORY`]).
pub(crate) fn archive(
    history: &mut Vec<SavedChat>,
    active_id: Option<u64>,
    agent: Option<String>,
    conv: &Conversation,
) -> Option<u64> {
    if conv.is_empty() {
        return active_id;
    }
    let mut clean = conv.clone();
    clean.sanitize_restored();
    let title = clean.title();
    let id = active_id.unwrap_or_else(|| history.iter().map(|s| s.id).max().unwrap_or(0) + 1);
    history.retain(|s| s.id != id);
    history.insert(
        0,
        SavedChat {
            id,
            title,
            agent,
            conv: clean,
        },
    );
    history.truncate(MAX_HISTORY);
    Some(id)
}

/// Expand lightweight template tokens (`{selection}`, `{project}`, `{data_date}`)
/// in a saved prompt snippet using the active schedule context.
pub(crate) fn expand_template(template: &str, ctx: Option<&ChatContext>) -> String {
    let Some(ctx) = ctx else {
        return template.to_string();
    };
    let sel = match &ctx.selection {
        Selection::None => "the open schedule".to_string(),
        Selection::One(id) => id.clone(),
        Selection::Many(ids) => ids.iter().take(10).cloned().collect::<Vec<_>>().join(", "),
    };
    template
        .replace("{selection}", &sel)
        .replace("{project}", &ctx.project_id)
        .replace("{data_date}", &ctx.data_date)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::backend::AgentEvent;

    #[test]
    fn archive_deduplicates_and_caps_history() {
        let mut history = vec![];
        let mut c = Conversation::default();
        c.send("First question");
        c.apply(AgentEvent::Message("Answer 1".into()));
        c.apply(AgentEvent::TurnFinished { usage: None });
        let id = archive(&mut history, None, Some("chatgpt".into()), &c).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].title, "First question");
        // Updating the same chat replaces it in place at the front.
        c.send("Second turn");
        let id2 = archive(&mut history, Some(id), Some("chatgpt".into()), &c).unwrap();
        assert_eq!(id, id2);
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].conv.items.len(), 3);
        // Cap at MAX_HISTORY.
        for n in 0..(MAX_HISTORY + 5) {
            let mut other = Conversation::default();
            other.send(&format!("Question {n}"));
            archive(&mut history, None, None, &other);
        }
        assert_eq!(history.len(), MAX_HISTORY);
    }

    #[test]
    fn template_tokens_expand_from_context() {
        let ctx = ChatContext {
            project_id: "UNIT4".into(),
            selection: Selection::One("PIP-3110".into()),
            data_date: "30-Mar-26".into(),
            ..Default::default()
        };
        assert_eq!(
            expand_template(
                "Review {selection} in {project} as of {data_date}.",
                Some(&ctx)
            ),
            "Review PIP-3110 in UNIT4 as of 30-Mar-26."
        );
    }
}
