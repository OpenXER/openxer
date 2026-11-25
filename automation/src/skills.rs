//! Skills: instructions that teach any connected AI agent how to build and
//! analyze schedules with OpenXER's tools.
//!
//! A skill is one Markdown file: a header between two `---` lines, one
//! `key: value` per line (list keys take `- item` lines, `#` starts a
//! comment line), then the instructions.
//!
//! ```text
//! ---
//! id: logic-health-check        # the file name without .md: a-z, 0-9, hyphens
//! name: Logic health check      # the label in lists, at most 40 characters
//! description: One line, at most 160 characters.
//! request: Run the logic health check.   # the sentence a user pastes to the agent
//! use_when: The plain words that ask for it.
//! examples:                     # optional: requests that should find it
//! - Check the logic
//! cites:                        # optional: standards by number and title
//! - GAO-16-89G Schedule Assessment Guide
//! tools:                        # the OpenXER tools its instructions run
//! - openxer_check_schedule
//! order: 10                     # optional: place in the list, 0 to 999
//! kind: check                   # check (read-only) or workflow (builds or changes)
//! ---
//! The instructions, in Markdown.
//! ```
//!
//! Built-in skills are the files in `assets/skills/`, compiled in. The
//! user's skills are `.md` files in `<data folder>\skills\` (see
//! [`crate::locate`]). A user file whose id equals a built-in's replaces it
//! (customized); deleting it brings the built-in back. A user file that does
//! not parse is left out and reported, never breaking the others. The folder
//! is read on every call, so edits show up without a restart.
//!
//! Skills are generic: no company's codes, gates or calendars. The agent
//! never computes schedule metrics itself: every count, date, float and
//! percent comes from a tool result.
use serde_json::{json, Value};
use std::fmt;
use std::path::{Path, PathBuf};

/// Keys every skill must give.
pub const REQUIRED: [&str; 5] = ["id", "name", "description", "request", "use_when"];
/// Keys whose value is a list of `- item` lines.
pub const LISTS: [&str; 3] = ["examples", "cites", "tools"];
/// Optional single-value keys.
pub const OPTIONAL: [&str; 3] = ["order", "kind", "compare"];
/// The only value `compare` takes (kept for files written for the earlier
/// format; it asks the agent to settle a reference before comparing).
pub const COMPARE: &str = "baseline-or-file";
/// A skill without an order goes after those with one.
pub const ORDER_LAST: u32 = 1000;
pub const NAME_MAX: usize = 40;
pub const DESCRIPTION_MAX: usize = 160;
pub const ID_MAX: usize = 64;

/// Check (read-only analysis) or workflow (builds or changes a schedule).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    #[default]
    Check,
    Workflow,
}

impl Kind {
    pub fn key(self) -> &'static str {
        match self {
            Kind::Check => "check",
            Kind::Workflow => "workflow",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Kind::Check => "Check",
            Kind::Workflow => "Workflow",
        }
    }
}

/// Where a skill in the catalog comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// Shipped with OpenXER.
    BuiltIn,
    /// A file of the user's own.
    Yours,
    /// The user's file that replaces a built-in of the same id.
    Customized,
}

impl Source {
    pub fn key(self) -> &'static str {
        match self {
            Source::BuiltIn => "built_in",
            Source::Yours => "yours",
            Source::Customized => "customized",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Source::BuiltIn => "Built-in",
            Source::Yours => "Yours",
            Source::Customized => "Customized",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Skill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub request: String,
    pub use_when: String,
    pub instructions: String,
    pub examples: Vec<String>,
    pub cites: Vec<String>,
    pub tools: Vec<String>,
    pub order: u32,
    pub kind: Kind,
    pub compare: Option<String>,
}

/// Why a skill file does not parse, with the line when one is to blame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkillError {
    pub line: Option<usize>,
    pub message: String,
}

impl SkillError {
    fn at(line: usize, message: impl Into<String>) -> Self {
        Self {
            line: Some(line),
            message: message.into(),
        }
    }
    fn whole(message: impl Into<String>) -> Self {
        Self {
            line: None,
            message: message.into(),
        }
    }
}

impl fmt::Display for SkillError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            Some(n) => write!(f, "line {n}: {}", self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for SkillError {}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= ID_MAX
        && id.split('-').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        })
}

/// Parse a skill file's text. `stem`, when given, is the file name without
/// `.md`; the id must equal it. A byte order mark and CRLF line ends are
/// accepted.
pub fn parse(text: &str, stem: Option<&str>) -> Result<Skill, SkillError> {
    let text = text.trim_start_matches('\u{feff}').replace("\r\n", "\n");
    let lines: Vec<&str> = text.split('\n').collect();
    if lines.first().map(|l| l.trim()) != Some("---") {
        return Err(SkillError::at(
            1,
            "a skill file starts with a --- header line",
        ));
    }
    let Some(end) = (1..lines.len()).find(|&i| lines[i].trim() == "---") else {
        return Err(SkillError::whole("the header has no closing --- line"));
    };
    let mut single: Vec<(String, String, usize)> = vec![];
    let mut lists: Vec<(String, Vec<String>, usize)> = vec![];
    let mut current: Option<String> = None;
    for (i, line) in lines[1..end].iter().enumerate() {
        let n = i + 2;
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        if let Some(item) = line.strip_prefix("- ") {
            let Some(key) = current.as_ref().filter(|k| LISTS.contains(&k.as_str())) else {
                return Err(SkillError::at(
                    n,
                    format!("a list item outside a list key ({})", LISTS.join(", ")),
                ));
            };
            let item = strip_comment(item).trim().to_string();
            if item.is_empty() {
                return Err(SkillError::at(n, "an empty list item"));
            }
            lists
                .iter_mut()
                .find(|(k, _, _)| k == key)
                .expect("list key was opened")
                .1
                .push(item);
            continue;
        }
        let Some((key, value)) = line.split_once(':').filter(|(k, _)| {
            !k.is_empty() && k.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')
        }) else {
            return Err(SkillError::at(
                n,
                format!("expected 'key: value', got {:?}", line.trim()),
            ));
        };
        let value = strip_comment(value).trim().to_string();
        if single.iter().any(|(k, _, _)| k == key) || lists.iter().any(|(k, _, _)| k == key) {
            return Err(SkillError::at(n, format!("{key} is given twice")));
        }
        if LISTS.contains(&key) {
            if !value.is_empty() {
                return Err(SkillError::at(
                    n,
                    format!("{key} is a list: put each item on its own '- ' line below it"),
                ));
            }
            lists.push((key.to_string(), vec![], n));
        } else if REQUIRED.contains(&key) || OPTIONAL.contains(&key) {
            single.push((key.to_string(), value, n));
        } else {
            let mut keys: Vec<&str> = REQUIRED.to_vec();
            keys.extend(LISTS);
            keys.extend(OPTIONAL);
            return Err(SkillError::at(
                n,
                format!("unknown key {key:?} (keys: {})", keys.join(", ")),
            ));
        }
        current = Some(key.to_string());
    }
    let get = |key: &str| {
        single
            .iter()
            .find(|(k, _, _)| k == key)
            .map(|(_, v, n)| (v.as_str(), *n))
    };
    for key in REQUIRED {
        match get(key) {
            None => return Err(SkillError::whole(format!("the header has no {key}"))),
            Some(("", n)) => return Err(SkillError::at(n, format!("{key} is empty"))),
            Some(_) => {}
        }
    }
    let (id, id_line) = get("id").expect("required");
    if !valid_id(id) {
        return Err(SkillError::at(
            id_line,
            format!("id {id:?}: use lower-case letters, digits and single hyphens, at most {ID_MAX} characters"),
        ));
    }
    if let Some(stem) = stem {
        if id != stem {
            return Err(SkillError::at(
                id_line,
                format!("id {id:?} differs from its file name {stem:?}.md"),
            ));
        }
    }
    let (name, name_line) = get("name").expect("required");
    let count = name.chars().count();
    if count > NAME_MAX {
        return Err(SkillError::at(
            name_line,
            format!("name is {count} characters; keep it to {NAME_MAX}"),
        ));
    }
    let (description, description_line) = get("description").expect("required");
    let count = description.chars().count();
    if count > DESCRIPTION_MAX {
        return Err(SkillError::at(
            description_line,
            format!("description is {count} characters; keep it to one line of at most {DESCRIPTION_MAX}"),
        ));
    }
    let order = match get("order") {
        None | Some(("", _)) => ORDER_LAST,
        Some((value, n)) => {
            if value.is_empty() || value.len() > 3 || !value.bytes().all(|b| b.is_ascii_digit()) {
                return Err(SkillError::at(
                    n,
                    format!("order {value:?}: give a whole number from 0 to 999"),
                ));
            }
            value.parse().expect("digits")
        }
    };
    let kind = match get("kind") {
        None | Some(("", _)) => Kind::Check,
        Some(("check", _)) => Kind::Check,
        Some(("workflow", _)) => Kind::Workflow,
        Some((value, n)) => {
            return Err(SkillError::at(
                n,
                format!("kind {value:?}: use check or workflow"),
            ))
        }
    };
    let compare = match get("compare") {
        None | Some(("", _)) => None,
        Some((COMPARE, _)) => Some(COMPARE.to_string()),
        Some((value, n)) => {
            return Err(SkillError::at(
                n,
                format!("compare {value:?}: the only value is {COMPARE}"),
            ))
        }
    };
    let instructions = lines[end + 1..].join("\n").trim().to_string();
    if instructions.is_empty() {
        return Err(SkillError::at(
            end + 1,
            "the skill has no instructions after its header",
        ));
    }
    let list = |key: &str| {
        lists
            .iter()
            .find(|(k, _, _)| k == key)
            .map(|(_, v, _)| v.clone())
            .unwrap_or_default()
    };
    Ok(Skill {
        id: id.to_string(),
        name: name.to_string(),
        description: description.to_string(),
        request: get("request").expect("required").0.to_string(),
        use_when: get("use_when").expect("required").0.to_string(),
        instructions,
        examples: list("examples"),
        cites: list("cites"),
        tools: list("tools"),
        order,
        kind,
        compare,
    })
}

/// A value's trailing `  # comment`, as the header's own comments are
/// written; a `#` inside the text (no space before it) stays.
fn strip_comment(value: &str) -> &str {
    match value.find(" #") {
        Some(i) => &value[..i],
        None => value,
    }
}

impl Skill {
    /// The file text for this skill: header, then the instructions.
    pub fn to_text(&self) -> String {
        let mut out = String::from("---\n");
        out.push_str(&format!("id: {}\n", self.id));
        out.push_str(&format!("name: {}\n", self.name));
        out.push_str(&format!("description: {}\n", self.description));
        out.push_str(&format!("request: {}\n", self.request));
        out.push_str(&format!("use_when: {}\n", self.use_when));
        out.push_str(&format!("kind: {}\n", self.kind.key()));
        if self.order != ORDER_LAST {
            out.push_str(&format!("order: {}\n", self.order));
        }
        if let Some(compare) = &self.compare {
            out.push_str(&format!("compare: {compare}\n"));
        }
        for (key, items) in [
            ("examples", &self.examples),
            ("cites", &self.cites),
            ("tools", &self.tools),
        ] {
            if !items.is_empty() {
                out.push_str(&format!("{key}:\n"));
                for item in items {
                    out.push_str(&format!("- {item}\n"));
                }
            }
        }
        out.push_str("---\n");
        out.push_str(self.instructions.trim());
        out.push('\n');
        out
    }
}

/// The built-in skills' files, compiled in, in file name order. A test
/// checks this list against the folder.
macro_rules! built_in {
    ($($id:literal),* $(,)?) => {
        &[$(($id, include_str!(concat!("../../assets/skills/", $id, ".md")))),*]
    };
}
pub const BUILT_IN: &[(&str, &str)] = built_in![
    "activity-definition-review",
    "add-scope",
    "baseline-review",
    "baseline-variance",
    "build-schedule",
    "calendar-duration-check",
    "change-interview",
    "compare-files",
    "contingency-float",
    "critical-path-explained",
    "duration-review",
    "export-pack",
    "float-constraint-review",
    "levels-of-detail",
    "logic-health-check",
    "look-ahead",
    "milestone-status",
    "near-critical-paths",
    "one-page-schedule",
    "progress-update",
    "recovery-schedule",
    "resource-load",
    "resource-loading-review",
    "risk-read",
    "schedule-basis",
    "schedule-maturity",
    "schedule-review",
    "sequence-review",
    "time-impact",
    "update-review",
    "weather-nonwork",
    "what-if",
];

/// The built-in skills, in catalog order. A built-in that does not parse is
/// a build mistake the tests catch; it is left out here.
pub fn built_in() -> Vec<Skill> {
    let mut list: Vec<Skill> = BUILT_IN
        .iter()
        .filter_map(|(id, text)| parse(text, Some(id)).ok())
        .collect();
    sort(&mut list);
    list
}

fn sort(list: &mut [Skill]) {
    list.sort_by(|a, b| a.order.cmp(&b.order).then_with(|| a.id.cmp(&b.id)));
}

/// The folder of the user's skills: `skills` in the data folder.
pub fn user_dir() -> PathBuf {
    crate::locate::data_folder().join("skills")
}

/// The names of every tool the MCP server offers, the skill tools included.
pub fn known_tools() -> Vec<String> {
    crate::mcp::tool_definitions()
        .iter()
        .filter_map(|t| t["name"].as_str().map(String::from))
        .collect()
}

/// The tools a skill names that the server does not offer.
pub fn unknown_tools(skill: &Skill, known: &[String]) -> Vec<String> {
    skill
        .tools
        .iter()
        .filter(|t| !known.contains(t))
        .cloned()
        .collect()
}

/// One skill in the catalog.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub skill: Skill,
    pub source: Source,
    /// The user's file, for Yours and Customized.
    pub path: Option<PathBuf>,
    /// Problems that do not stop the skill from loading (a tool it names
    /// that does not exist).
    pub warnings: Vec<String>,
}

/// A user file left out, and why.
#[derive(Clone, Debug, PartialEq)]
pub struct Problem {
    pub path: PathBuf,
    pub error: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Catalog {
    pub entries: Vec<Entry>,
    pub problems: Vec<Problem>,
}

impl Catalog {
    pub fn get(&self, id: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.skill.id == id)
    }
}

/// The catalog: the built-ins with the user's skills from `dir` (none when
/// `dir` is None or missing), in order, then by id.
pub fn catalog_in(dir: Option<&Path>) -> Catalog {
    let mut entries: Vec<Entry> = built_in()
        .into_iter()
        .map(|skill| Entry {
            skill,
            source: Source::BuiltIn,
            path: None,
            warnings: vec![],
        })
        .collect();
    let mut problems = vec![];
    let mut files: Vec<PathBuf> = dir
        .and_then(|d| std::fs::read_dir(d).ok())
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| {
                    p.is_file() && p.extension().is_some_and(|x| x.eq_ignore_ascii_case("md"))
                })
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    let known = known_tools();
    for path in files {
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let parsed = std::fs::read(&path)
            .map_err(|e| format!("cannot read it: {e}"))
            .and_then(|bytes| {
                String::from_utf8(bytes).map_err(|_| "it is not UTF-8 text".to_string())
            })
            .and_then(|text| parse(&text, Some(&stem)).map_err(|e| e.to_string()));
        let skill = match parsed {
            Ok(skill) => skill,
            Err(error) => {
                problems.push(Problem { path, error });
                continue;
            }
        };
        let warnings = unknown_tools(&skill, &known)
            .into_iter()
            .map(|t| format!("tools: {t} is not an OpenXER tool"))
            .collect();
        let entry = |source| Entry {
            skill: skill.clone(),
            source,
            path: Some(path.clone()),
            warnings,
        };
        match entries.iter_mut().find(|e| e.skill.id == skill.id) {
            Some(existing) if existing.source == Source::BuiltIn => {
                *existing = entry(Source::Customized);
            }
            Some(_) => problems.push(Problem {
                path: path.clone(),
                error: format!("another file already has id {:?}", skill.id),
            }),
            None => entries.push(entry(Source::Yours)),
        }
    }
    entries.sort_by(|a, b| {
        a.skill
            .order
            .cmp(&b.skill.order)
            .then_with(|| a.skill.id.cmp(&b.skill.id))
    });
    Catalog { entries, problems }
}

/// The catalog with the user's skills from [`user_dir`].
pub fn catalog() -> Catalog {
    catalog_in(Some(&user_dir()))
}

/// Check a skill's text: the parse error, or the skill with its warnings
/// (tools that do not exist).
pub fn validate(text: &str, stem: Option<&str>) -> Result<(Skill, Vec<String>), SkillError> {
    let skill = parse(text, stem)?;
    let known = known_tools();
    let warnings = unknown_tools(&skill, &known)
        .into_iter()
        .map(|t| format!("tools: {t} is not an OpenXER tool"))
        .collect();
    Ok((skill, warnings))
}

/// The text of a new skill with id `id`: a short template that explains
/// itself and parses as it is.
pub fn template(id: &str) -> String {
    format!(
        "---
# A skill teaches an AI agent one way to analyze or change a schedule with
# OpenXER's tools. Lines starting with # are notes and are ignored.
id: {id}
name: My schedule check
# One line, shown in lists (at most 160 characters):
description: What this skill checks or does, in one line.
# The sentence a user pastes to the agent to run it:
request: Run my schedule check.
# The plain words that should make an agent pick this skill:
use_when: The user asks for ...
# check reads only; workflow builds or changes a schedule:
kind: check
# The OpenXER tools the steps run (see Tools you can name):
tools:
- openxer_load
---
# My schedule check

Say in one or two lines what this skill is for.

## Steps

1. Run `openxer_load` on the user's file and take ... from its result.
2. Every count, date, float and percent comes from a tool result. Never work
   one out yourself.
3. Rank what you find by its consequence to the key dates, worst first.

## Ask

When something is unclear, ask the user one short question at a time.

## Do not

Do not change the schedule. Do not guess numbers.

## Answer

Two lines first: the finding that matters most. Then one section per finding.
"
    )
}

/// A file name (without .md) that is a valid id and not yet used by a
/// user file in `dir` or a built-in: `base`, else `base-2`, `base-3` ...
pub fn free_id(dir: &Path, base: &str) -> String {
    let taken =
        |id: &str| dir.join(format!("{id}.md")).exists() || BUILT_IN.iter().any(|(b, _)| *b == id);
    if valid_id(base) && !taken(base) {
        return base.to_string();
    }
    let base = if valid_id(base) { base } else { "my-skill" };
    (2..)
        .map(|n| format!("{base}-{n}"))
        .find(|id| !taken(id))
        .expect("a free id")
}

/// Write a user skill file `<dir>/<id>.md`, creating the folder. `replace`
/// false refuses an existing file.
pub fn write_user(dir: &Path, id: &str, text: &str, replace: bool) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("{id}.md"));
    if !replace && path.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("{} already exists", path.display()),
        ));
    }
    let tmp = dir.join(format!(".{id}.md.tmp"));
    std::fs::write(&tmp, text.replace("\r\n", "\n"))?;
    std::fs::rename(&tmp, &path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })?;
    Ok(path)
}

/// One entry as the list tool and `skills list` give it.
pub fn summary_json(e: &Entry) -> Value {
    let mut v = json!({
        "id": e.skill.id,
        "name": e.skill.name,
        "description": e.skill.description,
        "use_when": e.skill.use_when,
        "request": e.skill.request,
        "kind": e.skill.kind.key(),
        "source": e.source.key(),
    });
    if !e.warnings.is_empty() {
        v["warnings"] = json!(e.warnings);
    }
    v
}

/// One entry with its instructions, as the get tool and `skills show` give
/// it. A skill's `cites` stay in its file for the person who wrote it and
/// are never sent to an agent: answers do not say where a method came from.
pub fn detail_json(e: &Entry) -> Value {
    let mut v = summary_json(e);
    v["examples"] = json!(e.skill.examples);
    v["tools"] = json!(e.skill.tools);
    v["compare"] = json!(e.skill.compare);
    v["path"] = json!(e.path);
    v["instructions"] = json!(e.skill.instructions);
    v
}

/// The catalog as JSON: skills and the user files left out.
pub fn catalog_json(c: &Catalog, dir: Option<&Path>) -> Value {
    json!({
        "skills": c.entries.iter().map(summary_json).collect::<Vec<_>>(),
        "problems": c.problems.iter().map(|p| json!({"path": p.path, "error": p.error})).collect::<Vec<_>>(),
        "folder": dir,
        "rules": RULES,
    })
}

/// What every agent is told with the skill list and each skill.
pub const RULES: &str = "Before analyzing or building a schedule, find the skill that matches the request and follow its instructions. Every count, date, float and percent you report comes from an OpenXER tool result; never compute schedule metrics yourself. Anything that writes is dry run first and goes to a new file.";

/// The text an agent receives for one skill: the rules, then the skill.
pub fn prompt_text(s: &Skill) -> String {
    format!(
        "{RULES}\n\nSkill: {} ({}). {}\n\n{}",
        s.name,
        s.kind.label().to_lowercase(),
        s.request,
        s.instructions
    )
}

#[cfg(test)]
#[path = "skills_tests.rs"]
mod tests;
