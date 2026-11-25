//! Playbook slash commands (`/check`, `/audit`, `/lookahead`, custom `.md`
//! playbooks), saved user prompts, and `@` Activity ID autocomplete in the
//! Chat composer.
use super::backend::ChatContext;
use super::history::{expand_template, SavedPrompt};
use openxer_automation::playbooks::{self, PlaybookEntry};
use std::path::Path;

/// Short command aliases for built-in playbooks (both the alias and the full
/// playbook name match `/...`).
const ALIASES: &[(&str, &str)] = &[
    ("check", "schedule-check"),
    ("critical", "critical-path"),
    ("recover", "delay-recovery"),
    ("ev", "earned-value"),
    ("resources", "resource-review"),
];

/// One candidate shown in the `/` slash popup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SlashItem {
    /// Command token without leading `/` (e.g. `"schedule-check"` or `"saved:weekly"`).
    pub command: String,
    pub label: String,
    pub detail: String,
    pub kind: SlashKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SlashKind {
    Playbook { name: String, custom: bool },
    SavedPrompt { text: String },
}

/// All playbooks available right now: built-in plus any `.md` files in the
/// user's `chat/playbooks/` folder.
pub(crate) fn available_playbooks(instructions_path: &Path) -> Vec<PlaybookEntry> {
    let dir = instructions_path.parent().map(|p| p.join("playbooks"));
    playbooks::all_playbooks(dir.as_deref())
}

/// If `draft` is an in-progress `/command` (starts with `/`, single line, no
/// space yet), returns the typed filter text after `/`.
pub(crate) fn slash_query(draft: &str) -> Option<&str> {
    let trimmed = draft.trim_start();
    if !trimmed.starts_with('/') || trimmed.contains('\n') {
        return None;
    }
    let after = &trimmed[1..];
    if after.contains(char::is_whitespace) {
        return None;
    }
    Some(after)
}

/// Matching slash items for `query` (empty lists everything, built-ins first,
/// then custom playbooks, then saved prompts; non-empty ranks name/alias
/// prefix matches ahead of description matches).
pub(crate) fn slash_matches(
    query: &str,
    playbooks: &[PlaybookEntry],
    saved: &[SavedPrompt],
) -> Vec<SlashItem> {
    let q = query.trim().to_ascii_lowercase();
    let mut ranked: Vec<(u8, usize, SlashItem)> = vec![];
    let mut order = 0usize;
    for p in playbooks {
        let alias = ALIASES
            .iter()
            .find(|(_, target)| *target == p.name)
            .map(|(a, _)| *a);
        let name_l = p.name.to_ascii_lowercase();
        let rank =
            if q.is_empty() || name_l.starts_with(&q) || alias.is_some_and(|a| a.starts_with(&q)) {
                Some(0)
            } else if name_l.contains(&q) || alias.is_some_and(|a| a.contains(&q)) {
                Some(1)
            } else if p.about.to_ascii_lowercase().contains(&q) {
                Some(2)
            } else {
                None
            };
        if let Some(r) = rank {
            let label = if p.custom {
                format!("/{} (custom)", p.name)
            } else {
                format!("/{}", p.name)
            };
            ranked.push((
                r,
                order,
                SlashItem {
                    command: p.name.clone(),
                    label,
                    detail: p.about.clone(),
                    kind: SlashKind::Playbook {
                        name: p.name.clone(),
                        custom: p.custom,
                    },
                },
            ));
            order += 1;
        }
    }
    for s in saved {
        let slug = slug_of(&s.name);
        let name_l = s.name.to_ascii_lowercase();
        let rank = if q.is_empty() || slug.starts_with(&q) || name_l.starts_with(&q) {
            Some(0)
        } else if slug.contains(&q) || name_l.contains(&q) {
            Some(1)
        } else if s.text.to_ascii_lowercase().contains(&q) {
            Some(2)
        } else {
            None
        };
        if let Some(r) = rank {
            ranked.push((
                r,
                order,
                SlashItem {
                    command: slug,
                    label: format!("/{} (saved)", s.name),
                    detail: s.text.chars().take(80).collect(),
                    kind: SlashKind::SavedPrompt {
                        text: s.text.clone(),
                    },
                },
            ));
            order += 1;
        }
    }
    ranked.sort_by_key(|(r, o, _)| (*r, *o));
    ranked.into_iter().map(|(_, _, item)| item).collect()
}

/// Default prompt sent when a user runs a playbook from `/` or the Prompts menu.
pub(crate) fn playbook_prompt(name: &str, extra: &str) -> String {
    let extra = extra.trim();
    if !extra.is_empty() {
        return format!("Load and follow the {name} playbook with openxer_playbook. {extra}");
    }
    let body = match name {
        "schedule-check" => "Run Check Schedule and explain the failing checks and next steps.",
        "audit" => "Audit this schedule's logic, constraints, lags, float and health.",
        "critical-path" => {
            "Explain the driving critical path, near-critical paths and key float drivers."
        }
        "delay-recovery" => {
            "Identify what drives any delay and rank recovery options with cost and risk."
        }
        "update" => {
            "Walk through updating progress to the current data date and explain what moves."
        }
        "build" => "Help build out WBS, activities, durations and logic for this schedule.",
        "lookahead" => "Build a 3-week field lookahead grouped by WBS with readiness risks.",
        "narrative" => "Draft an executive schedule narrative ready to copy into a status report.",
        "earned-value" => {
            "Analyze earned value (SPI, CPI, EAC, VAC) and list the top variance drivers."
        }
        "resource-review" => "Review time-phased resource peaks, bottlenecks and leveling options.",
        "what-if" => {
            "Help test a what-if scenario and measure its impact on finish date and float."
        }
        _ => "Run this playbook on the open schedule.",
    };
    format!("Load and follow the {name} playbook with openxer_playbook. {body}")
}

/// If `text` begins with `/command [extra]` matching a known playbook (or alias)
/// or saved prompt, expands it into the full prompt to send.
pub(crate) fn expand_slash_send(
    text: &str,
    playbooks: &[PlaybookEntry],
    saved: &[SavedPrompt],
    ctx: Option<&ChatContext>,
) -> Option<String> {
    let trimmed = text.trim();
    let rest = trimmed.strip_prefix('/')?;
    let (cmd, extra) = match rest.split_once(char::is_whitespace) {
        Some((c, e)) => (c.trim(), e.trim()),
        None => (rest.trim(), ""),
    };
    if cmd.is_empty() {
        return None;
    }
    let lower = cmd.to_ascii_lowercase();
    let resolved = ALIASES
        .iter()
        .find(|(a, _)| *a == lower)
        .map(|(_, target)| *target)
        .unwrap_or(lower.as_str());
    if let Some(p) = playbooks
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case(resolved))
    {
        return Some(playbook_prompt(&p.name, extra));
    }
    if let Some(s) = saved
        .iter()
        .find(|s| slug_of(&s.name) == lower || s.name.eq_ignore_ascii_case(cmd))
    {
        let expanded = expand_template(&s.text, ctx);
        return Some(if extra.is_empty() {
            expanded
        } else {
            format!("{expanded}\n{extra}")
        });
    }
    None
}

/// If the draft ends with an `@token` mention (preceded by start of string or
/// whitespace, with no whitespace after `@`), returns `(byte_index_of_at, token)`.
pub(crate) fn at_mention_query(draft: &str) -> Option<(usize, &str)> {
    let at = draft.rfind('@')?;
    if at > 0 {
        let prev = draft[..at].chars().last()?;
        if !prev.is_whitespace() && !"([{\"',;:".contains(prev) {
            return None;
        }
    }
    let token = &draft[at + 1..];
    if token.contains(char::is_whitespace) {
        return None;
    }
    Some((at, token))
}

/// Replace the active `@token` starting at `at` with `code` followed by a space.
pub(crate) fn insert_at_mention(draft: &mut String, at: usize, code: &str) {
    if at <= draft.len() && draft.is_char_boundary(at) {
        draft.truncate(at);
        draft.push_str(code);
        draft.push(' ');
    }
}

fn slug_of(name: &str) -> String {
    name.trim()
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slash_commands_match_aliases_and_expand() {
        let pbs = playbooks::all_playbooks(None);
        let saved = vec![SavedPrompt {
            name: "Friday Update".into(),
            text: "Summarize {selection} for Friday.".into(),
        }];
        assert_eq!(slash_query("/aud"), Some("aud"));
        assert_eq!(slash_query("/audit extra"), None);
        let m = slash_matches("check", &pbs, &saved);
        assert!(m.iter().any(|i| i.command == "schedule-check"));
        let expanded = expand_slash_send("/check focus on piping", &pbs, &saved, None).unwrap();
        assert!(expanded.contains("schedule-check"));
        assert!(expanded.contains("focus on piping"));
        let ctx = ChatContext {
            project_id: "P1".into(),
            selection: super::super::backend::Selection::One("A100".into()),
            data_date: "01-Jan-26".into(),
            ..Default::default()
        };
        let from_saved = expand_slash_send("/friday-update", &pbs, &saved, Some(&ctx)).unwrap();
        assert_eq!(from_saved, "Summarize A100 for Friday.");
    }

    #[test]
    fn at_mentions_parse_and_insert() {
        assert_eq!(at_mention_query("Why is @PIP"), Some((7, "PIP")));
        assert_eq!(at_mention_query("@"), Some((0, "")));
        assert_eq!(at_mention_query("email@example.com"), None);
        assert_eq!(at_mention_query("Why is @PIP late"), None);
        let mut draft = "Why is @PIP".to_string();
        insert_at_mention(&mut draft, 7, "PIP-3110");
        assert_eq!(draft, "Why is PIP-3110 ");
    }
}
