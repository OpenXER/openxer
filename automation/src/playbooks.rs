//! `playbook`: step-by-step scheduling methods an agent loads when it
//! needs one, instead of carrying them in every prompt. Built-in texts are
//! assets compiled into the binary (`assets/playbooks/`); user-authored
//! Markdown files in `chat/playbooks/` are discovered beside them.
use crate::{AutomationError, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// Most characters read from one user-authored playbook file.
pub const USER_PLAYBOOK_LIMIT: usize = 6_000;

/// Each built-in playbook: name, what it is for, text.
pub const PLAYBOOKS: &[(&str, &str, &str)] = &[
    (
        "build",
        "Build a new schedule: WBS, activities, durations, logic, calendars, resources, quality check",
        include_str!("../../assets/playbooks/build.md"),
    ),
    (
        "update",
        "Update progress to a new data date, reschedule and explain what moved",
        include_str!("../../assets/playbooks/update.md"),
    ),
    (
        "audit",
        "Audit logic and health: open ends, constraints, lags, durations, float, relationship mix, out of sequence",
        include_str!("../../assets/playbooks/audit.md"),
    ),
    (
        "critical-path",
        "Read the driving path, near-critical paths and what consumes float",
        include_str!("../../assets/playbooks/critical-path.md"),
    ),
    (
        "delay-recovery",
        "Find what drives a delay and list options to pull time back",
        include_str!("../../assets/playbooks/delay-recovery.md"),
    ),
    (
        "schedule-check",
        "Run Check Schedule and explain each result and what to look at next",
        include_str!("../../assets/playbooks/schedule-check.md"),
    ),
    (
        "lookahead",
        "Build a 2, 3 or 6 week field lookahead grouped by WBS with readiness flags",
        include_str!("../../assets/playbooks/lookahead.md"),
    ),
    (
        "narrative",
        "Draft a copy-ready executive weekly or monthly schedule narrative",
        include_str!("../../assets/playbooks/narrative.md"),
    ),
    (
        "earned-value",
        "Analyze SPI, CPI, EAC and VAC and isolate the activities driving variance",
        include_str!("../../assets/playbooks/earned-value.md"),
    ),
    (
        "resource-review",
        "Inspect time-phased resource peaks, crew bottlenecks and leveling options",
        include_str!("../../assets/playbooks/resource-review.md"),
    ),
    (
        "what-if",
        "Test a single change in Edit mode and measure its finish date and float impact",
        include_str!("../../assets/playbooks/what-if.md"),
    ),
];

/// One playbook entry (built-in or discovered from the user's folder).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaybookEntry {
    pub name: String,
    pub about: String,
    pub text: String,
    pub custom: bool,
    pub path: Option<PathBuf>,
}

/// The user's custom playbooks folder inside `data_folder`: `chat\playbooks`.
pub fn playbooks_dir(data_folder: &Path) -> PathBuf {
    data_folder.join("chat").join("playbooks")
}

/// Starter README dropped once when the user opens the custom playbooks folder
/// for the first time (only HTML comments, ignored by playbook discovery).
pub const CUSTOM_PLAYBOOK_SAMPLE: &str = "<!--
Drop any Markdown (.md) file into this folder to add your own scheduling
playbook to OpenXER Chat. The file name (without .md) becomes its /command
name, the first heading or line becomes its description, and HTML comments
like this one are ignored. Example file: weekly-owner-update.md

# Weekly owner update
Summarize critical path changes, completed milestones, and 3-week lookahead
risks using plain dates and Activity IDs.
-->
";

/// Ensure `dir` exists and contains a starter example comment file if empty.
pub fn ensure_playbooks_dir(dir: &Path) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let sample = dir.join("README-custom-playbooks.txt");
    if !sample.exists() {
        let _ = std::fs::write(&sample, CUSTOM_PLAYBOOK_SAMPLE);
    }
    Ok(dir.to_path_buf())
}

fn normalize_name(stem: &str) -> Option<String> {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in stem.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if matches!(ch, '-' | '_' | ' ') && !out.is_empty() && !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    (!out.is_empty()).then_some(out)
}

fn extract_about(text: &str, fallback_name: &str) -> String {
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let clean = trimmed
            .trim_start_matches('#')
            .trim()
            .trim_start_matches("Goal:")
            .trim();
        if !clean.is_empty() {
            let mut s: String = clean.chars().take(110).collect();
            if clean.chars().count() > 110 {
                s.push_str("...");
            }
            return s;
        }
    }
    format!("Custom playbook: {fallback_name}")
}

/// Discover user-authored `.md` playbooks in `dir` (sorted by name).
/// Empty or comment-only files are skipped.
pub fn discover_user_playbooks(dir: &Path) -> Vec<PlaybookEntry> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return vec![];
    };
    let mut found = vec![];
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let is_md = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("md"));
        if !is_md {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Some(name) = normalize_name(stem) else {
            continue;
        };
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let cleaned = crate::chat_prompt::clean_markdown(&bytes);
        let text = cleaned.trim();
        if text.is_empty() {
            continue;
        }
        let text: String = text.chars().take(USER_PLAYBOOK_LIMIT).collect();
        let about = extract_about(&text, &name);
        found.push(PlaybookEntry {
            name,
            about,
            text,
            custom: true,
            path: Some(path),
        });
    }
    found.sort_by(|a, b| a.name.cmp(&b.name));
    found
}

/// Built-in playbooks combined with any custom playbooks in `dir`.
/// A custom file with the same name as a built-in overrides it.
pub fn all_playbooks(dir: Option<&Path>) -> Vec<PlaybookEntry> {
    let mut list: Vec<PlaybookEntry> = PLAYBOOKS
        .iter()
        .map(|(n, about, text)| PlaybookEntry {
            name: (*n).to_string(),
            about: (*about).to_string(),
            text: text.replace("\r\n", "\n"),
            custom: false,
            path: None,
        })
        .collect();
    if let Some(dir) = dir {
        for custom in discover_user_playbooks(dir) {
            if let Some(slot) = list
                .iter_mut()
                .find(|e| e.name.eq_ignore_ascii_case(&custom.name))
            {
                *slot = custom;
            } else {
                list.push(custom);
            }
        }
    }
    list
}

/// Without `name` the list of built-in playbooks; with it, that playbook's text.
pub fn playbook(name: Option<&str>) -> Result<Value> {
    playbook_in(name, None)
}

/// Same as [`playbook`], also including custom `.md` playbooks in `dir`.
pub fn playbook_in(name: Option<&str>, dir: Option<&Path>) -> Result<Value> {
    let list = all_playbooks(dir);
    match name {
        None => Ok(json!({
            "playbooks": list
                .iter()
                .map(|p| {
                    if p.custom {
                        json!({"name": p.name, "about": p.about, "custom": true})
                    } else {
                        json!({"name": p.name, "about": p.about})
                    }
                })
                .collect::<Vec<_>>()
        })),
        Some(name) => list
            .into_iter()
            .find(|p| p.name.eq_ignore_ascii_case(name.trim()))
            .map(|p| json!({"name": p.name, "text": p.text}))
            .ok_or_else(|| {
                AutomationError::new(
                    "not_found",
                    format!(
                        "No playbook named {name:?}; playbooks are {}",
                        all_playbooks(dir)
                            .iter()
                            .map(|p| p.name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                )
            }),
    }
}
