//! Connecting MCP clients (Claude Desktop, ChatGPT desktop and Codex, Claude
//! Code) to `openxer-cli mcp`. The CLI (`openxer-cli connect ...`) and the
//! desktop's AI Agents menu share this module.
//!
//! The first half is pure functions over config text: read the `openxer`
//! entry's state, merge it in, take it out. They never touch the file
//! system, so every case is unit tested on strings. The second half is a
//! thin file layer over an injected [`Env`]: it finds the config files,
//! backs a file up before changing it, writes atomically, re-reads and
//! validates, and puts the original back when validation fails. Only the
//! `openxer` entry is ever added, changed or removed.
use crate::{AutomationError, Result};
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    ffi::OsString,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// The server name every client config uses for OpenXER.
pub const SERVER: &str = "openxer";
/// The CLI executable's file name.
pub const CLI_NAME: &str = if cfg!(windows) {
    "openxer-cli.exe"
} else {
    "openxer-cli"
};
/// Claude Desktop's config file name.
pub const CLAUDE_DESKTOP_FILE: &str = "claude_desktop_config.json";
/// How long Test Connection waits for the server.
pub const TEST_TIMEOUT: Duration = Duration::from_secs(15);

/// The file an installer puts beside the executables of an installed copy
/// (a portable copy has none). Keep in step with the desktop's workspace
/// rule, which uses the same marker.
pub const INSTALLED_MARKER: &str = "openxer.installed";
/// The first request to try in a newly connected app.
pub const FIRST_PROMPT: &str = "Use OpenXER to inspect my schedule.";

/// The clients this module knows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Client {
    ClaudeDesktop,
    /// The ChatGPT desktop app and Codex (its app and CLI): they share one
    /// config file, `%USERPROFILE%\.codex\config.toml`.
    Codex,
    ClaudeCode,
}

impl Client {
    pub const ALL: [Client; 3] = [Client::ClaudeDesktop, Client::Codex, Client::ClaudeCode];

    /// The id the CLI takes (`claude-desktop`, `codex`, `claude-code`).
    pub fn id(self) -> &'static str {
        match self {
            Client::ClaudeDesktop => "claude-desktop",
            Client::Codex => "codex",
            Client::ClaudeCode => "claude-code",
        }
    }

    /// The app's display name.
    pub fn name(self) -> &'static str {
        match self {
            Client::ClaudeDesktop => "Claude Desktop",
            Client::Codex => "ChatGPT desktop and Codex",
            Client::ClaudeCode => "Claude Code",
        }
    }

    /// The client a CLI id names; `chatgpt` is another name for `codex`.
    pub fn parse(id: &str) -> Option<Client> {
        if id == "chatgpt" {
            return Some(Client::Codex);
        }
        Client::ALL.into_iter().find(|c| c.id() == id)
    }

    /// What to do in the app after it is connected, in plain words, with
    /// the first thing to ask.
    pub fn next_step(self) -> String {
        match self {
            Client::ClaudeDesktop => format!(
                "Quit Claude Desktop from its tray icon (closing the window is not enough), open it again, then ask: \"{FIRST_PROMPT}\""
            ),
            Client::Codex => format!(
                "Restart the app, then ask: \"{FIRST_PROMPT}\" In ChatGPT desktop use Work or Codex mode: for now, plain Chat may not use tools on this PC."
            ),
            Client::ClaudeCode => format!(
                "Paste the command into a terminal, restart Claude Code, then ask: \"{FIRST_PROMPT}\""
            ),
        }
    }

    /// Claude Code is connected by its own command, never by editing files.
    pub fn edits_config(self) -> bool {
        self != Client::ClaudeCode
    }
}

/// The state of the `openxer` entry in one config.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Entry {
    /// No `openxer` entry (or no file).
    NotConnected,
    /// The entry runs this OpenXER's `openxer-cli mcp`.
    Connected,
    /// The entry runs `mcp` with another executable that exists: another
    /// copy of OpenXER (`installed` when the installer's marker is beside
    /// it). It works, so nothing repoints it unless asked ("Use this copy").
    OtherCopy { command: String, installed: bool },
    /// The entry runs an executable that no longer exists, or other
    /// arguments: it cannot work and needs repair.
    Stale { command: String },
    /// The config cannot be read as JSON or TOML; nothing is written to it.
    Malformed { message: String },
}

impl Entry {
    /// Ranks entries so the worst of several files names a client's state.
    fn rank(&self) -> u8 {
        match self {
            Entry::Connected => 0,
            Entry::NotConnected => 1,
            Entry::OtherCopy { .. } => 2,
            Entry::Stale { .. } => 3,
            Entry::Malformed { .. } => 4,
        }
    }

    /// The short words a status column shows.
    pub fn label(&self) -> &'static str {
        match self {
            Entry::NotConnected => "Not connected",
            Entry::Connected => "Connected",
            Entry::OtherCopy { .. } => "Connected to another copy of OpenXER",
            Entry::Stale { .. } => "Needs repair",
            Entry::Malformed { .. } => "Its settings file cannot be read",
        }
    }

    /// Whether the app can reach an OpenXER server through this entry.
    pub fn works(&self) -> bool {
        matches!(self, Entry::Connected | Entry::OtherCopy { .. })
    }
}

/// The one thing the Connect dialog offers for an app, worked out from its
/// state. The CLI reports it too, so both say the same.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// Found and not connected: Connect.
    Connect,
    /// Connected to this copy: nothing to do (Test, Disconnect).
    Connected,
    /// Connected to another copy that exists: Use This Copy switches it.
    UseThisCopy,
    /// The entry points at a missing file or runs other arguments: Repair.
    Repair,
    /// The app is not on this PC and has no entry: plain text, no button.
    NotInstalled,
    /// Its settings file cannot be read: OpenXER leaves it alone.
    FixFile,
    /// Claude Code: copy its own command.
    CopyCommand,
}

impl Action {
    /// Whether "Connect all found apps" connects an app in this state:
    /// never one that works, so a working connection is not repointed.
    pub fn in_connect_all(self) -> bool {
        matches!(self, Action::Connect | Action::Repair)
    }
}

// ------------------------------------------------------------ pure: paths

/// Whether a config's `command` names `cli`: separators and, on Windows,
/// letter case are not significant; surrounding quotes are ignored.
pub fn same_path(command: &str, cli: &Path) -> bool {
    let norm = |s: &str| {
        let s = s.trim().trim_matches('"');
        if cfg!(windows) {
            s.replace('/', "\\").to_lowercase()
        } else {
            s.to_string()
        }
    };
    norm(command) == norm(&cli.display().to_string())
}

fn classify(
    command: Option<&str>,
    args: Option<Vec<String>>,
    cli: &Path,
    exists: &dyn Fn(&Path) -> bool,
) -> Entry {
    let command_text = command.unwrap_or_default().to_string();
    let runs_mcp = args.as_deref() == Some(&["mcp".to_string()][..]);
    match command {
        Some(c) if same_path(c, cli) && exists(cli) && runs_mcp => Entry::Connected,
        Some(c) if runs_mcp && !same_path(c, cli) => {
            let other = PathBuf::from(c.trim().trim_matches('"'));
            if other.is_absolute() && exists(&other) {
                Entry::OtherCopy {
                    command: command_text,
                    installed: is_installed_copy(&other, exists),
                }
            } else {
                Entry::Stale {
                    command: command_text,
                }
            }
        }
        _ => Entry::Stale {
            command: command_text,
        },
    }
}

/// Whether `cli` belongs to an installed copy: the installer's marker file
/// is beside it.
pub fn is_installed_copy(cli: &Path, exists: &dyn Fn(&Path) -> bool) -> bool {
    cli.parent()
        .is_some_and(|dir| exists(&dir.join(INSTALLED_MARKER)))
}

/// The command a user pastes into a terminal to add OpenXER to Claude Code.
/// The path is always quoted, so spaces in it are safe.
pub fn claude_code_command(cli: &Path) -> String {
    format!(
        "claude mcp add --scope user {SERVER} -- \"{}\" mcp",
        cli.display()
    )
}

/// A prompt a user pastes into any AI tool that can run commands, so it
/// sets itself up with this copy of OpenXER: where `openxer-cli` is (quoted,
/// as in [`claude_code_command`]), how to learn its commands, the MCP
/// server to add or the CLI to call, a real command that proves it works
/// (`connect test`), the safety rules and a first request. The desktop's
/// Copy Setup Prompt and `openxer-cli connect prompt` both print this.
pub fn setup_prompt(cli: &Path) -> String {
    let cli = format!("\"{}\"", cli.display());
    format!(
        "Set yourself up to work with OpenXER, a CPM scheduler on this PC, through its command-line tool:
{cli}

1. Run it with --help to see its commands.
2. If you can use MCP servers, add one named {SERVER} that runs that file with the single argument mcp. Otherwise call the file directly: every command prints JSON.
3. Check that it works: run it with the arguments connect test. It starts the MCP server, lists its tools and prints \"Working: N tools\".

Rules: OpenXER never changes an input file; every edit is written to a new file (--output). Use --dry-run to preview an edit before anything is written, and show me the changes first.

Then: {FIRST_PROMPT}
"
    )
}

/// The generic MCP JSON setup most clients accept.
pub fn setup_snippet(cli: &Path) -> String {
    let value =
        json!({"mcpServers": {SERVER: {"command": cli.display().to_string(), "args": ["mcp"]}}});
    serde_json::to_string_pretty(&value).expect("JSON value serializes") + "\n"
}

// ------------------------------------------------------- pure: JSON configs

/// A JSON value that keeps its keys in file order, so a rewritten config
/// differs from the original only where the `openxer` entry is.
#[derive(Clone, Debug, PartialEq)]
enum J {
    Null,
    Bool(bool),
    Number(serde_json::Number),
    String(String),
    Array(Vec<J>),
    Object(Vec<(String, J)>),
}

impl<'de> serde::Deserialize<'de> for J {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<J, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = J;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a JSON value")
            }
            fn visit_unit<E>(self) -> std::result::Result<J, E> {
                Ok(J::Null)
            }
            fn visit_none<E>(self) -> std::result::Result<J, E> {
                Ok(J::Null)
            }
            fn visit_bool<E>(self, v: bool) -> std::result::Result<J, E> {
                Ok(J::Bool(v))
            }
            fn visit_i64<E>(self, v: i64) -> std::result::Result<J, E> {
                Ok(J::Number(v.into()))
            }
            fn visit_u64<E>(self, v: u64) -> std::result::Result<J, E> {
                Ok(J::Number(v.into()))
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> std::result::Result<J, E> {
                serde_json::Number::from_f64(v)
                    .map(J::Number)
                    .ok_or_else(|| E::custom("number out of range"))
            }
            fn visit_str<E>(self, v: &str) -> std::result::Result<J, E> {
                Ok(J::String(v.to_string()))
            }
            fn visit_string<E>(self, v: String) -> std::result::Result<J, E> {
                Ok(J::String(v))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut a: A,
            ) -> std::result::Result<J, A::Error> {
                let mut out = vec![];
                while let Some(v) = a.next_element()? {
                    out.push(v);
                }
                Ok(J::Array(out))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut a: A,
            ) -> std::result::Result<J, A::Error> {
                let mut out: Vec<(String, J)> = vec![];
                while let Some((k, v)) = a.next_entry::<String, J>()? {
                    out.push((k, v));
                }
                Ok(J::Object(out))
            }
        }
        d.deserialize_any(V)
    }
}

impl Serialize for J {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::{SerializeMap, SerializeSeq};
        match self {
            J::Null => s.serialize_unit(),
            J::Bool(b) => s.serialize_bool(*b),
            J::Number(n) => n.serialize(s),
            J::String(t) => s.serialize_str(t),
            J::Array(items) => {
                let mut seq = s.serialize_seq(Some(items.len()))?;
                for item in items {
                    seq.serialize_element(item)?;
                }
                seq.end()
            }
            J::Object(fields) => {
                let mut map = s.serialize_map(Some(fields.len()))?;
                for (k, v) in fields {
                    map.serialize_entry(k, v)?;
                }
                map.end()
            }
        }
    }
}

impl J {
    fn get(&self, key: &str) -> Option<&J> {
        match self {
            J::Object(fields) => fields.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    fn get_mut(&mut self, key: &str) -> Option<&mut J> {
        match self {
            J::Object(fields) => fields.iter_mut().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    fn remove(&mut self, key: &str) -> bool {
        if let J::Object(fields) = self {
            let before = fields.len();
            fields.retain(|(k, _)| k != key);
            return fields.len() != before;
        }
        false
    }
    fn str(&self) -> Option<&str> {
        match self {
            J::String(s) => Some(s),
            _ => None,
        }
    }
    fn strings(&self) -> Option<Vec<String>> {
        match self {
            J::Array(items) => items.iter().map(|i| i.str().map(str::to_string)).collect(),
            _ => None,
        }
    }
}

const JSON_SERVERS: &str = "mcpServers";

fn strip_bom(text: &str) -> &str {
    text.strip_prefix('\u{feff}').unwrap_or(text)
}

/// Parses a JSON config: empty text is an empty object; anything that is
/// not an object whose `mcpServers` (when present) is an object is refused.
fn json_root(text: &str) -> std::result::Result<J, String> {
    let text = strip_bom(text);
    if text.trim().is_empty() {
        return Ok(J::Object(vec![]));
    }
    let root: J = serde_json::from_str(text).map_err(|e| format!("not valid JSON ({e})"))?;
    if !matches!(root, J::Object(_)) {
        return Err("the file is not a JSON object".into());
    }
    if root
        .get(JSON_SERVERS)
        .is_some_and(|s| !matches!(s, J::Object(_)))
    {
        return Err(format!("\"{JSON_SERVERS}\" is not an object"));
    }
    Ok(root)
}

/// The `openxer` entry's state in a JSON (`mcpServers`) config.
pub fn json_entry(text: &str, cli: &Path, exists: &dyn Fn(&Path) -> bool) -> Entry {
    let root = match json_root(text) {
        Ok(root) => root,
        Err(message) => return Entry::Malformed { message },
    };
    match root.get(JSON_SERVERS).and_then(|s| s.get(SERVER)) {
        None => Entry::NotConnected,
        Some(entry) => classify(
            entry.get("command").and_then(J::str),
            entry.get("args").and_then(J::strings),
            cli,
            exists,
        ),
    }
}

// The JSON configs are edited as text: the parsed value says what to
// change, and byte ranges found by a small scanner say where, so every byte
// outside the `openxer` entry (other servers, settings, spacing, key order)
// stays exactly as the file had it.

/// One `"key": value` of a JSON object, as byte offsets into the text.
struct Member {
    key: String,
    key_start: usize,
    key_end: usize,
    value: (usize, usize),
}

/// A JSON object's braces and members, as byte offsets into the text.
struct Obj {
    open: usize,
    close: usize,
    members: Vec<Member>,
}

impl Obj {
    fn member(&self, key: &str) -> Option<&Member> {
        self.members.iter().find(|m| m.key == key)
    }
}

fn skip_ws(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && matches!(b[i], b' ' | b'\t' | b'\r' | b'\n') {
        i += 1;
    }
    i
}

/// The end (exclusive) of the string starting at `i` (its opening quote).
fn string_end(b: &[u8], mut i: usize) -> Option<usize> {
    i += 1;
    while i < b.len() {
        match b[i] {
            b'\\' => i += 2,
            b'"' => return Some(i + 1),
            _ => i += 1,
        }
    }
    None
}

/// The end (exclusive) of the value starting at `i`.
fn value_end(b: &[u8], i: usize) -> Option<usize> {
    match *b.get(i)? {
        b'"' => string_end(b, i),
        b'{' | b'[' => {
            let mut depth = 0usize;
            let mut j = i;
            while j < b.len() {
                match b[j] {
                    b'"' => {
                        j = string_end(b, j)?;
                        continue;
                    }
                    b'{' | b'[' => depth += 1,
                    b'}' | b']' => {
                        depth -= 1;
                        if depth == 0 {
                            return Some(j + 1);
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
            None
        }
        _ => {
            let mut j = i;
            while j < b.len() && !matches!(b[j], b',' | b'}' | b']' | b' ' | b'\t' | b'\r' | b'\n')
            {
                j += 1;
            }
            Some(j)
        }
    }
}

/// The object whose `{` is at `open` (text already validated as JSON).
fn object_at(text: &str, open: usize) -> Option<Obj> {
    let b = text.as_bytes();
    let mut members = vec![];
    let mut i = skip_ws(b, open + 1);
    loop {
        match *b.get(i)? {
            b'}' => {
                return Some(Obj {
                    open,
                    close: i,
                    members,
                })
            }
            b'"' => {
                let key_end = string_end(b, i)?;
                let key: String = serde_json::from_str(&text[i..key_end]).ok()?;
                let colon = skip_ws(b, key_end);
                let start = skip_ws(b, colon + 1);
                let end = value_end(b, start)?;
                members.push(Member {
                    key,
                    key_start: i,
                    key_end,
                    value: (start, end),
                });
                i = skip_ws(b, end);
                if b.get(i) == Some(&b',') {
                    i = skip_ws(b, i + 1);
                }
            }
            _ => return None,
        }
    }
}

/// How the file lays out JSON: one indent step (None: all on one line).
fn json_style(text: &str, root: &Obj) -> Option<String> {
    if !text.contains('\n') && !root.members.is_empty() {
        return None;
    }
    let unit = root
        .members
        .first()
        .and_then(|m| {
            let before = &text[root.open + 1..m.key_start];
            before.rfind('\n').map(|n| before[n + 1..].to_string())
        })
        .filter(|u| !u.is_empty() && u.chars().all(|c| c == ' ' || c == '\t'))
        .unwrap_or_else(|| "  ".into());
    Some(unit)
}

/// The spaces at the start of the line holding byte `at`.
fn line_indent(text: &str, at: usize) -> String {
    let start = text[..at].rfind('\n').map_or(0, |n| n + 1);
    text[start..]
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect()
}

/// A JSON string literal for `s`.
fn json_string(s: &str) -> String {
    serde_json::to_string(s).expect("a string serializes")
}

/// Our entry's object text with its members at `indent` + one step (or on
/// one line).
fn json_entry_text(cli: &Path, style: Option<&str>, indent: &str) -> String {
    let command = json_string(&cli.display().to_string());
    match style {
        None => format!("{{\"command\":{command},\"args\":[\"mcp\"]}}"),
        Some(unit) => format!(
            "{{\n{indent}{unit}\"command\": {command},\n{indent}{unit}\"args\": [\n{indent}{unit}{unit}\"mcp\"\n{indent}{unit}]\n{indent}}}"
        ),
    }
}

/// The edit that adds `"key": value` as the last member of `obj`; `value`
/// is made for the member indent it is given.
fn insert_member(
    text: &str,
    obj: &Obj,
    key: &str,
    style: Option<&str>,
    value: impl Fn(&str) -> String,
) -> (usize, usize, String) {
    let key = json_string(key);
    match obj.members.last() {
        Some(last) => {
            let b = text.as_bytes();
            let mut sep_start = last.key_start;
            while sep_start > 0 && matches!(b[sep_start - 1], b' ' | b'\t' | b'\r' | b'\n') {
                sep_start -= 1;
            }
            let sep = &text[sep_start..last.key_start];
            let colon = &text[last.key_end..last.value.0];
            let indent = sep.rfind('\n').map_or("", |n| &sep[n + 1..]);
            let at = last.value.1;
            (at, at, format!(",{sep}{key}{colon}{}", value(indent)))
        }
        None => {
            let outer = line_indent(text, obj.open);
            let body = match style {
                None => format!("{key}:{}", value("")),
                Some(unit) => {
                    let indent = format!("{outer}{unit}");
                    format!("\n{indent}{key}: {}\n{outer}", value(&indent))
                }
            };
            (obj.open + 1, obj.close, body)
        }
    }
}

/// Applies non-overlapping edits (start, end, new text) to `text`.
fn apply_edits(text: &str, mut edits: Vec<(usize, usize, String)>) -> String {
    edits.sort_by_key(|e| std::cmp::Reverse(e.0));
    let mut out = text.to_string();
    for (start, end, new) in edits {
        out.replace_range(start..end, &new);
    }
    out
}

/// The BOM (if any) and the JSON text after it.
fn split_bom(text: &str) -> (&str, &str) {
    match text.strip_prefix('\u{feff}') {
        Some(rest) => ("\u{feff}", rest),
        None => ("", text),
    }
}

fn scan_root(body: &str) -> std::result::Result<Obj, String> {
    let open = skip_ws(body.as_bytes(), 0);
    object_at(body, open).ok_or_else(|| "the file could not be scanned as JSON".to_string())
}

/// One edit at a time: the text after it, with offsets rescanned by the
/// caller. In a file with Windows line ends the new text gets them too.
fn edit_once(text: &str, mut edit: (usize, usize, String)) -> String {
    if text.contains("\r\n") {
        edit.2 = edit.2.replace("\r\n", "\n").replace('\n', "\r\n");
    }
    apply_edits(text, vec![edit])
}

/// The object a member's value is.
fn member_object(text: &str, root: &Obj, key: &str) -> Option<Obj> {
    let m = root.member(key)?;
    (text.as_bytes().get(m.value.0) == Some(&b'{'))
        .then(|| object_at(text, m.value.0))
        .flatten()
}

/// `text` with the `openxer` entry set to run `cli mcp`. Everything else in
/// the file, other fields of an existing `openxer` entry (such as `env`)
/// included, is kept byte for byte. Err is why the text was refused.
pub fn json_connect(text: &str, cli: &Path) -> std::result::Result<String, String> {
    json_root(text)?;
    let (bom, body) = split_bom(text);
    if body.trim().is_empty() {
        let value = json!({JSON_SERVERS: {SERVER: {"command": cli.display().to_string(), "args": ["mcp"]}}});
        return Ok(format!(
            "{bom}{}\n",
            serde_json::to_string_pretty(&value).expect("JSON value serializes")
        ));
    }
    let unscannable = || "the file could not be scanned as JSON".to_string();
    let style = json_style(body, &scan_root(body)?);
    let style = style.as_deref();
    let entry = |indent: &str| json_entry_text(cli, style, indent);
    let command = json_string(&cli.display().to_string());
    let mut out = body.to_string();
    // 1. An `mcpServers` object to put the entry in.
    let root = scan_root(&out)?;
    if root.member(JSON_SERVERS).is_none() {
        let edit = insert_member(&out, &root, JSON_SERVERS, style, |_| "{}".into());
        out = edit_once(&out, edit);
    }
    let root = scan_root(&out)?;
    let servers = member_object(&out, &root, JSON_SERVERS).ok_or_else(unscannable)?;
    match servers.member(SERVER) {
        // 2. No entry: add ours as the last server.
        None => {
            let edit = insert_member(&out, &servers, SERVER, style, entry);
            out = edit_once(&out, edit);
        }
        // 3. An entry that is not an object: replace it whole.
        Some(ours) if out.as_bytes()[ours.value.0] != b'{' => {
            let indent = line_indent(&out, ours.key_start);
            let edit = (ours.value.0, ours.value.1, entry(&indent));
            out = edit_once(&out, edit);
        }
        // 4. An entry object: set its command and args in place, keeping
        //    its other fields.
        Some(_) => {
            let wanted = [
                ("command", command, json!(cli.display().to_string())),
                ("args", "[\"mcp\"]".to_string(), json!(["mcp"])),
            ];
            for (key, value, parsed) in wanted {
                let root = scan_root(&out)?;
                let servers = member_object(&out, &root, JSON_SERVERS).ok_or_else(unscannable)?;
                let ours = member_object(&out, &servers, SERVER).ok_or_else(unscannable)?;
                let edit = match ours.member(key) {
                    // Already right, however it is laid out: left alone.
                    Some(m)
                        if serde_json::from_str::<Value>(&out[m.value.0..m.value.1]).ok()
                            == Some(parsed) =>
                    {
                        continue
                    }
                    Some(m) => (m.value.0, m.value.1, value),
                    None => insert_member(&out, &ours, key, style, |_| value.clone()),
                };
                out = edit_once(&out, edit);
            }
        }
    }
    Ok(format!("{bom}{out}"))
}

/// `text` without the `openxer` entry; every other byte is kept. Text
/// without one is returned as is.
pub fn json_disconnect(text: &str) -> std::result::Result<String, String> {
    json_root(text)?;
    let (bom, body) = split_bom(text);
    if body.trim().is_empty() {
        return Ok(text.to_string());
    }
    let root = scan_root(body)?;
    let Some(servers) = member_object(body, &root, JSON_SERVERS) else {
        return Ok(text.to_string());
    };
    let Some(index) = servers.members.iter().position(|m| m.key == SERVER) else {
        return Ok(text.to_string());
    };
    let members = &servers.members;
    let ours = &members[index];
    let span = if index > 0 {
        // From the end of the server before ours: its comma goes with ours.
        (members[index - 1].value.1, ours.value.1)
    } else if let Some(next) = members.get(1) {
        (ours.key_start, next.key_start)
    } else {
        (servers.open + 1, servers.close)
    };
    Ok(format!(
        "{bom}{}",
        apply_edits(body, vec![(span.0, span.1, String::new())])
    ))
}

/// Everything but the `openxer` entry (and an `mcpServers` left empty), for
/// checking a rewrite changed nothing else.
fn json_rest(text: &str) -> Option<J> {
    let mut root = json_root(text).ok()?;
    if let Some(servers) = root.get_mut(JSON_SERVERS) {
        servers.remove(SERVER);
        if matches!(servers, J::Object(f) if f.is_empty()) {
            root.remove(JSON_SERVERS);
        }
    }
    Some(root)
}

// ------------------------------------------------------- pure: TOML configs

const TOML_SERVERS: &str = "mcp_servers";

fn toml_doc(text: &str) -> std::result::Result<toml_edit::DocumentMut, String> {
    let doc: toml_edit::DocumentMut = strip_bom(text)
        .parse()
        .map_err(|e: toml_edit::TomlError| format!("not valid TOML ({})", e.message()))?;
    if doc.get(TOML_SERVERS).is_some_and(|s| !s.is_table_like()) {
        return Err(format!("\"{TOML_SERVERS}\" is not a table"));
    }
    Ok(doc)
}

/// The `openxer` entry's state in a Codex (`[mcp_servers.openxer]`) config.
pub fn toml_entry(text: &str, cli: &Path, exists: &dyn Fn(&Path) -> bool) -> Entry {
    let doc = match toml_doc(text) {
        Ok(doc) => doc,
        Err(message) => return Entry::Malformed { message },
    };
    let Some(entry) = doc
        .get(TOML_SERVERS)
        .and_then(|s| s.as_table_like())
        .and_then(|s| s.get(SERVER))
    else {
        return Entry::NotConnected;
    };
    let Some(entry) = entry.as_table_like() else {
        return Entry::Stale {
            command: String::new(),
        };
    };
    let args = entry.get("args").and_then(|a| a.as_array()).and_then(|a| {
        a.iter()
            .map(|v| v.as_str().map(str::to_string))
            .collect::<Option<Vec<_>>>()
    });
    classify(
        entry.get("command").and_then(|c| c.as_str()),
        args,
        cli,
        exists,
    )
}

/// `text` with `[mcp_servers.openxer]` set to run `cli mcp`. Comments,
/// formatting and every other table and key are kept as written.
pub fn toml_connect(text: &str, cli: &Path) -> std::result::Result<String, String> {
    use toml_edit::{value, Array, InlineTable, Item, Table};
    let mut doc = toml_doc(text)?;
    let root = doc.as_table_mut();
    if !root.contains_key(TOML_SERVERS) {
        let mut servers = Table::new();
        servers.set_implicit(true);
        root.insert(TOML_SERVERS, Item::Table(servers));
    }
    let servers_item = root.get_mut(TOML_SERVERS).expect("servers table");
    let inline = servers_item.is_inline_table();
    let servers = servers_item.as_table_like_mut().expect("validated table");
    let command = toml_path(&cli.display().to_string());
    let args = Array::from_iter(["mcp"]);
    match servers.get_mut(SERVER).and_then(|e| e.as_table_like_mut()) {
        Some(entry) => {
            set_keeping_decor(entry, "command", command);
            set_keeping_decor(entry, "args", args.into());
        }
        None => {
            servers.remove(SERVER);
            if inline {
                let mut entry = InlineTable::new();
                entry.insert("command", command);
                entry.insert("args", args.into());
                servers.insert(SERVER, value(entry));
            } else {
                let mut entry = Table::new();
                entry.insert("command", value(command));
                entry.insert("args", value(args));
                servers.insert(SERVER, Item::Table(entry));
            }
        }
    }
    let mut out = doc.to_string();
    if !out.ends_with('\n') {
        out.push('\n');
    }
    Ok(out)
}

/// A path as a TOML string. A literal string (single quotes) keeps every
/// backslash as written, so no escape in it can ever be wrong; a path with
/// a single quote or a control character is a basic string escaped by
/// `toml_edit` instead.
pub fn toml_path(path: &str) -> toml_edit::Value {
    let literal_ok = !path.contains('\'') && path.chars().all(|c| c == '\t' || !c.is_control());
    if literal_ok {
        if let Ok(mut value) = format!("'{path}'").parse::<toml_edit::Value>() {
            // The default spacing around it, as for any new value.
            value.decor_mut().clear();
            if value.as_str() == Some(path) {
                return value;
            }
        }
    }
    toml_edit::Value::from(path)
}

/// Sets `key` in `table`; an existing value is replaced in place, so the
/// key's comments and the value's spacing stay.
fn set_keeping_decor(table: &mut dyn toml_edit::TableLike, key: &str, mut new: toml_edit::Value) {
    match table.get_mut(key).and_then(|item| item.as_value_mut()) {
        Some(old) => {
            *new.decor_mut() = old.decor().clone();
            *old = new;
        }
        None => {
            table.insert(key, toml_edit::Item::Value(new));
        }
    }
}

/// `text` without `[mcp_servers.openxer]`. Text without one is returned as
/// is.
pub fn toml_disconnect(text: &str) -> std::result::Result<String, String> {
    let mut doc = toml_doc(text)?;
    let removed = doc
        .get_mut(TOML_SERVERS)
        .and_then(|s| s.as_table_like_mut())
        .is_some_and(|s| s.remove(SERVER).is_some());
    Ok(if removed {
        doc.to_string()
    } else {
        text.to_string()
    })
}

fn toml_value(item: &toml_edit::Item) -> Value {
    use toml_edit::Item;
    match item {
        Item::None => Value::Null,
        Item::Value(v) => toml_plain(v),
        Item::Table(t) => Value::Object(
            t.iter()
                .map(|(k, v)| (k.to_string(), toml_value(v)))
                .collect(),
        ),
        Item::ArrayOfTables(a) => Value::Array(
            a.iter()
                .map(|t| {
                    Value::Object(
                        t.iter()
                            .map(|(k, v)| (k.to_string(), toml_value(v)))
                            .collect(),
                    )
                })
                .collect(),
        ),
    }
}

fn toml_plain(v: &toml_edit::Value) -> Value {
    use toml_edit::Value as T;
    match v {
        T::String(s) => json!(s.value()),
        T::Integer(i) => json!(i.value()),
        T::Float(f) => json!(f.value().to_string()),
        T::Boolean(b) => json!(b.value()),
        T::Datetime(d) => json!(d.value().to_string()),
        T::Array(a) => Value::Array(a.iter().map(toml_plain).collect()),
        T::InlineTable(t) => Value::Object(
            t.iter()
                .map(|(k, v)| (k.to_string(), toml_plain(v)))
                .collect(),
        ),
    }
}

/// Everything but the `openxer` entry, as plain values, for checking a
/// rewrite changed nothing else.
fn toml_rest(text: &str) -> Option<Value> {
    let mut value = toml_value(toml_doc(text).ok()?.as_item());
    if let Some(servers) = value.get_mut(TOML_SERVERS).and_then(Value::as_object_mut) {
        servers.remove(SERVER);
        if servers.is_empty() {
            value.as_object_mut()?.remove(TOML_SERVERS);
        }
    }
    Some(value)
}

/// The config format of a client's files.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Json,
    Toml,
}

impl Format {
    pub fn entry(self, text: &str, cli: &Path, exists: &dyn Fn(&Path) -> bool) -> Entry {
        match self {
            Format::Json => json_entry(text, cli, exists),
            Format::Toml => toml_entry(text, cli, exists),
        }
    }
    /// The new text for a connect (or disconnect); Err is why the existing
    /// text was refused.
    pub fn rewrite(
        self,
        text: &str,
        cli: &Path,
        disconnect: bool,
    ) -> std::result::Result<String, String> {
        match (self, disconnect) {
            (Format::Json, false) => json_connect(text, cli),
            (Format::Json, true) => json_disconnect(text),
            (Format::Toml, false) => toml_connect(text, cli),
            (Format::Toml, true) => toml_disconnect(text),
        }
    }
    /// True when `after` differs from `before` only in the `openxer` entry.
    pub fn only_entry_changed(self, before: &str, after: &str) -> bool {
        match self {
            Format::Json => json_rest(before).is_some() && json_rest(before) == json_rest(after),
            Format::Toml => toml_rest(before).is_some() && toml_rest(before) == toml_rest(after),
        }
    }
}

// --------------------------------------------------------------- file layer

/// Where to look for clients. Every path is injected: the CLI and the app
/// fill it from the process environment, tests from temporary folders.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Env {
    /// `%APPDATA%` (roaming).
    pub appdata: Option<PathBuf>,
    /// `%LOCALAPPDATA%`.
    pub local_appdata: Option<PathBuf>,
    /// `%USERPROFILE%` (or `HOME`).
    pub home: Option<PathBuf>,
    /// `CODEX_HOME`, when set; else `<home>\.codex`.
    pub codex_home: Option<PathBuf>,
    /// The `PATH` searched for `claude` and `codex`.
    pub path: Option<OsString>,
    /// The `openxer-cli` executable clients should run.
    pub cli: Option<PathBuf>,
}

impl Env {
    /// The process environment, with `cli` as the executable to connect.
    pub fn from_process(cli: Option<PathBuf>) -> Env {
        let var = |name: &str| {
            std::env::var_os(name)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        Env {
            appdata: var("APPDATA"),
            local_appdata: var("LOCALAPPDATA"),
            home: var("USERPROFILE").or_else(|| var("HOME")),
            codex_home: var("CODEX_HOME"),
            path: std::env::var_os("PATH"),
            cli,
        }
    }

    /// The executable, or the error every connect action reports without it.
    pub fn cli(&self) -> Result<&Path> {
        self.cli.as_deref().filter(|p| p.is_file()).ok_or_else(|| {
            AutomationError::new(
                "cli_missing",
                format!(
                    "{CLI_NAME} was not found{}. It ships next to OpenXER.exe; reinstall or copy it there.",
                    self.cli
                        .as_ref()
                        .map(|p| format!(" at {}", p.display()))
                        .unwrap_or_default()
                ),
            )
        })
    }

    /// The Microsoft Store packages' folders whose names start with one of
    /// `prefixes` (`%LOCALAPPDATA%\Packages\<name>`).
    fn packages(&self, prefixes: &[&str]) -> Vec<PathBuf> {
        let Some(local) = &self.local_appdata else {
            return vec![];
        };
        let mut found: Vec<PathBuf> = std::fs::read_dir(local.join("Packages"))
            .map(|dir| {
                dir.filter_map(|e| e.ok())
                    .filter(|e| {
                        let name = e.file_name().to_string_lossy().to_string();
                        prefixes.iter().any(|p| name.starts_with(p))
                    })
                    .map(|e| e.path())
                    .filter(|p| p.is_dir())
                    .collect()
            })
            .unwrap_or_default();
        found.sort();
        found
    }

    /// The Microsoft Store installs' Claude folders (`Packages\Claude_*`).
    fn claude_packages(&self) -> Vec<PathBuf> {
        self.packages(&["Claude_"])
    }

    /// Whether the ChatGPT desktop app or the Codex app is installed: a
    /// Store package (`OpenAI.ChatGPT*`, `OpenAI.Codex_*`; the Codex
    /// runtime packages are not the app) or `%LOCALAPPDATA%\OpenAI\Codex`.
    fn openai_app_installed(&self) -> bool {
        !self
            .packages(&["OpenAI.ChatGPT", "OpenAI.Codex_"])
            .is_empty()
            || self
                .local_appdata
                .as_ref()
                .is_some_and(|l| l.join("OpenAI").join("Codex").is_dir())
    }

    fn claude_classic_dir(&self) -> Option<PathBuf> {
        self.appdata.as_ref().map(|a| a.join("Claude"))
    }

    fn claude_classic_installed(&self) -> bool {
        self.claude_classic_dir().is_some_and(|d| d.is_dir())
            || self
                .local_appdata
                .as_ref()
                .is_some_and(|l| l.join("AnthropicClaude").is_dir())
    }

    /// The Claude Desktop config files to keep in step: each Store install's
    /// own copy, and the classic `%APPDATA%\Claude` file when that install
    /// is present or nothing else is.
    pub fn claude_desktop_files(&self) -> Vec<PathBuf> {
        let mut files: Vec<PathBuf> = self
            .claude_packages()
            .into_iter()
            .map(|p| {
                p.join("LocalCache")
                    .join("Roaming")
                    .join("Claude")
                    .join(CLAUDE_DESKTOP_FILE)
            })
            .collect();
        if self.claude_classic_installed() || files.is_empty() {
            if let Some(dir) = self.claude_classic_dir() {
                files.push(dir.join(CLAUDE_DESKTOP_FILE));
            }
        }
        files
    }

    /// Codex's config, shared by the Codex app and CLI and, as reported, by
    /// the ChatGPT desktop app.
    pub fn codex_file(&self) -> Option<PathBuf> {
        self.codex_home
            .clone()
            .or_else(|| self.home.as_ref().map(|h| h.join(".codex")))
            .map(|d| d.join("config.toml"))
    }

    /// The first `name` executable on `PATH`.
    pub fn on_path(&self, name: &str) -> Option<PathBuf> {
        let path = self.path.as_ref()?;
        let names: Vec<String> = if cfg!(windows) {
            ["exe", "cmd", "bat", "ps1"]
                .iter()
                .map(|e| format!("{name}.{e}"))
                .collect()
        } else {
            vec![name.to_string()]
        };
        std::env::split_paths(path)
            .flat_map(|dir| names.iter().map(move |n| dir.join(n)))
            .find(|p| p.is_file())
    }

    /// Whether `client` looks installed on this PC.
    pub fn found(&self, client: Client) -> bool {
        match client {
            Client::ClaudeDesktop => {
                self.claude_classic_installed() || !self.claude_packages().is_empty()
            }
            Client::Codex => {
                self.codex_file()
                    .and_then(|f| f.parent().map(Path::is_dir))
                    .unwrap_or(false)
                    || self.openai_app_installed()
                    || self.on_path("codex").is_some()
            }
            Client::ClaudeCode => self.on_path("claude").is_some(),
        }
    }

    /// The config files and format of a client that is connected by editing
    /// files.
    pub fn files(&self, client: Client) -> (Format, Vec<PathBuf>) {
        match client {
            Client::ClaudeDesktop => (Format::Json, self.claude_desktop_files()),
            Client::Codex => (Format::Toml, self.codex_file().into_iter().collect()),
            Client::ClaudeCode => (Format::Json, vec![]),
        }
    }
}

/// One config file's state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FileStatus {
    pub path: PathBuf,
    pub exists: bool,
    pub entry: Entry,
}

/// One client's state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ClientStatus {
    pub client: Client,
    pub name: &'static str,
    pub found: bool,
    /// The worst state of its files (Claude Code: always not connected,
    /// since its own command does the connecting).
    pub entry: Entry,
    /// The one thing to offer for it (the dialog's button).
    pub action: Action,
    pub files: Vec<FileStatus>,
    /// Claude Code's command, when the executable is known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
}

/// The one thing to offer for a client in `entry` state.
pub fn action(client: Client, found: bool, entry: &Entry) -> Action {
    if !client.edits_config() {
        return if found {
            Action::CopyCommand
        } else {
            Action::NotInstalled
        };
    }
    match entry {
        Entry::Connected => Action::Connected,
        Entry::OtherCopy { .. } => Action::UseThisCopy,
        Entry::Stale { .. } => Action::Repair,
        Entry::Malformed { .. } => Action::FixFile,
        Entry::NotConnected if found => Action::Connect,
        Entry::NotConnected => Action::NotInstalled,
    }
}

fn read_config(path: &Path) -> Result<Option<String>> {
    match std::fs::read(path) {
        Ok(bytes) => String::from_utf8(bytes).map(Some).map_err(|_| {
            AutomationError::new(
                "config_malformed",
                format!("{} is not UTF-8 text", path.display()),
            )
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(io_error(path, "read", e)),
    }
}

fn io_error(path: &Path, what: &str, e: std::io::Error) -> AutomationError {
    AutomationError::new("io_error", format!("Cannot {what} {}: {e}", path.display()))
}

/// The state of one client.
pub fn client_status(env: &Env, client: Client) -> ClientStatus {
    let cli = env.cli.clone().unwrap_or_default();
    let exists = |p: &Path| p.is_file();
    let (format, paths) = env.files(client);
    let files: Vec<FileStatus> = paths
        .into_iter()
        .map(|path| {
            let (exists_now, entry) = match read_config(&path) {
                Ok(None) => (false, Entry::NotConnected),
                Ok(Some(text)) => (true, format.entry(&text, &cli, &exists)),
                Err(e) => (true, Entry::Malformed { message: e.message }),
            };
            FileStatus {
                path,
                exists: exists_now,
                entry,
            }
        })
        .collect();
    let entry = files
        .iter()
        .map(|f| f.entry.clone())
        .max_by_key(Entry::rank)
        .unwrap_or(Entry::NotConnected);
    let found = env.found(client);
    ClientStatus {
        client,
        name: client.name(),
        found,
        action: action(client, found, &entry),
        entry,
        files,
        command: (client == Client::ClaudeCode)
            .then(|| env.cli.as_deref().map(claude_code_command))
            .flatten(),
    }
}

/// The state of every client.
pub fn status(env: &Env) -> Vec<ClientStatus> {
    Client::ALL.iter().map(|c| client_status(env, *c)).collect()
}

/// What a connect or disconnect did (or, in a dry run, would do) to one file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Change {
    pub path: PathBuf,
    /// The file is new.
    pub created: bool,
    /// The text changes.
    pub changed: bool,
    /// The copy made before the change.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup: Option<PathBuf>,
    pub before: Entry,
    pub after: Entry,
    /// The new text (dry run only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
}

/// `YYYYMMDD-HHMMSS` in UTC.
pub fn timestamp(at: SystemTime) -> String {
    let secs = at
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) as i64;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}{month:02}{day:02}-{:02}{:02}{:02}",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// The backup path for `file` at `stamp`, numbered when that name is taken.
pub fn backup_path(file: &Path, stamp: &str) -> PathBuf {
    let base = format!("{}.openxer-backup-{stamp}", file.display());
    let mut candidate = PathBuf::from(&base);
    let mut n = 2;
    while candidate.exists() {
        candidate = PathBuf::from(format!("{base}-{n}"));
        n += 1;
    }
    candidate
}

/// Writes `text` to `path` through a temporary file in the same folder, so
/// a reader never sees half a file.
fn write_atomic(path: &Path, text: &str) -> Result<()> {
    let dir = path.parent().ok_or_else(|| {
        AutomationError::new("io_error", format!("{} has no folder", path.display()))
    })?;
    std::fs::create_dir_all(dir).map_err(|e| io_error(dir, "create", e))?;
    let mut tmp = tempfile::Builder::new()
        .prefix(".openxer-")
        .suffix(".tmp")
        .tempfile_in(dir)
        .map_err(|e| io_error(dir, "write in", e))?;
    tmp.write_all(text.as_bytes())
        .and_then(|()| tmp.as_file().sync_all())
        .map_err(|e| io_error(path, "write", e))?;
    tmp.persist(path)
        .map_err(|e| io_error(path, "replace", e.error))?;
    Ok(())
}

/// Connects (or with `disconnect`, disconnects) `client` by editing its
/// config files. Every file is read and its new text worked out before any
/// is written, so a malformed file stops the whole change. Each changed
/// file is backed up first, written atomically, re-read and checked: the
/// entry must be in its new state and everything else unchanged, or the
/// original is put back. `dry_run` returns the changes and new text without
/// writing anything.
pub fn apply(env: &Env, client: Client, disconnect: bool, dry_run: bool) -> Result<Vec<Change>> {
    if !client.edits_config() {
        return Err(AutomationError::new(
            "unsupported",
            "Claude Code is connected with its own command; run `openxer-cli connect claude-code` to print it",
        ));
    }
    let cli = env.cli()?.to_path_buf();
    let exists = |p: &Path| p.is_file();
    let (format, paths) = env.files(client);
    if paths.is_empty() {
        return Err(AutomationError::new(
            "config_unknown",
            format!("No config location is known for {}", client.name()),
        ));
    }
    // A Store install that has no config of its own yet sees the classic
    // file through its virtual AppData; its new copy starts from that one,
    // so the other servers listed there are not hidden.
    let classic = env
        .claude_classic_dir()
        .map(|d| d.join(CLAUDE_DESKTOP_FILE));
    let classic_text = match (&classic, client) {
        (Some(path), Client::ClaudeDesktop) => read_config(path)?,
        _ => None,
    };
    let mut plans = vec![];
    for path in paths {
        let current = read_config(&path)?;
        let base = match (&current, &classic_text) {
            (Some(text), _) => text.clone(),
            (None, Some(text)) if Some(&path) != classic.as_ref() => text.clone(),
            (None, _) => String::new(),
        };
        let before = match &current {
            Some(text) => format.entry(text, &cli, &exists),
            None => Entry::NotConnected,
        };
        let new_text = format.rewrite(&base, &cli, disconnect).map_err(|message| {
            AutomationError::new(
                "config_malformed",
                format!(
                    "{} was not changed: {message}. Fix the file by hand, or use Copy Setup and add the entry yourself.",
                    path.display()
                ),
            )
        })?;
        let after = if disconnect {
            Entry::NotConnected
        } else {
            Entry::Connected
        };
        // An entry already in its wanted state is left alone, so a file
        // formatted by hand is not rewritten for nothing.
        let changed = before != after && !(current.is_none() && disconnect);
        let read_only =
            current.is_some() && std::fs::metadata(&path).is_ok_and(|m| m.permissions().readonly());
        if changed && read_only && !dry_run {
            return Err(AutomationError::new(
                "config_read_only",
                format!(
                    "{} is read-only, so nothing was changed. Clear Read-only in the file's Properties, then try again.",
                    path.display()
                ),
            ));
        }
        plans.push((path, current, base, new_text, before, after, changed));
    }
    let stamp = timestamp(SystemTime::now());
    let mut changes = vec![];
    for (path, current, base, new_text, before, after, changed) in plans {
        let mut change = Change {
            path: path.clone(),
            created: current.is_none() && changed,
            changed,
            backup: None,
            before,
            after: after.clone(),
            preview: None,
        };
        if dry_run {
            change.preview = changed.then(|| new_text.clone());
            changes.push(change);
            continue;
        }
        if !changed {
            changes.push(change);
            continue;
        }
        if current.is_some() {
            let backup = backup_path(&path, &stamp);
            std::fs::copy(&path, &backup).map_err(|e| io_error(&backup, "back up to", e))?;
            change.backup = Some(backup);
        }
        write_atomic(&path, &new_text)?;
        let reread = read_config(&path)?.unwrap_or_default();
        let entry_ok = format.entry(&reread, &cli, &exists) == after;
        if !(entry_ok && format.only_entry_changed(&base, &reread)) {
            let restored = match &current {
                Some(text) => write_atomic(&path, text),
                None => std::fs::remove_file(&path).map_err(|e| io_error(&path, "remove", e)),
            };
            return Err(AutomationError::new(
                "validation_failed",
                format!(
                    "{} did not read back as expected after the change; the original was {}",
                    path.display(),
                    if restored.is_ok() {
                        "put back"
                    } else {
                        "kept in the backup"
                    }
                ),
            ));
        }
        changes.push(change);
    }
    Ok(changes)
}

/// The message a connect or disconnect ends with.
pub fn finish_message(
    client: Client,
    disconnect: bool,
    changes: &[Change],
    dry_run: bool,
) -> String {
    if dry_run {
        return "Dry run: nothing was written.".into();
    }
    if !changes.iter().any(|c| c.changed) {
        return if disconnect {
            format!("{} was not connected.", client.name())
        } else {
            format!("{} is already connected.", client.name())
        };
    }
    if disconnect {
        format!("Disconnected. Restart {} to finish.", client.name())
    } else {
        format!("Connected. {}", client.next_step())
    }
}

/// What "Connect all found apps" did for one client.
#[derive(Clone, Debug, Serialize)]
pub struct AllResult {
    pub client: Client,
    pub ok: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<Change>,
}

/// The clients "Connect all found apps" connects: found, config-editing,
/// and not working (Connect or Repair). One that works, even through
/// another copy of OpenXER, is never repointed.
pub fn connect_all_targets(statuses: &[ClientStatus]) -> Vec<Client> {
    statuses
        .iter()
        .filter(|s| s.found && s.action.in_connect_all())
        .map(|s| s.client)
        .collect()
}

/// Connects every client [`connect_all_targets`] names; one failing does
/// not stop the others.
pub fn connect_all(env: &Env, dry_run: bool) -> Vec<AllResult> {
    connect_all_targets(&status(env))
        .into_iter()
        .map(|client| match apply(env, client, false, dry_run) {
            Ok(files) => AllResult {
                client,
                ok: true,
                message: finish_message(client, false, &files, dry_run),
                files,
            },
            Err(e) => AllResult {
                client,
                ok: false,
                message: e.message,
                files: vec![],
            },
        })
        .collect()
}

/// The `openxer-cli` next to `exe` (a test runner's `deps` folder also looks
/// one folder up). `OPENXER_CLI` overrides it.
pub fn resolve_cli(exe: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("OPENXER_CLI").filter(|v| !v.is_empty()) {
        let path = PathBuf::from(path);
        return Some(std::path::absolute(&path).unwrap_or(path));
    }
    let dir = exe?.parent()?;
    let beside = dir.join(CLI_NAME);
    if beside.is_file() || dir.file_name().is_none_or(|n| n != "deps") {
        return Some(beside);
    }
    Some(dir.parent()?.join(CLI_NAME))
}

/// Starts `cli mcp`, initializes a session and lists its tools; returns how
/// many there are. The process is always stopped before this returns.
pub fn test_connection(cli: &Path, timeout: Duration) -> Result<usize> {
    if !cli.is_file() {
        return Err(AutomationError::new(
            "cli_missing",
            format!("{} was not found", cli.display()),
        ));
    }
    let mut command = Command::new(cli);
    command
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command.spawn().map_err(|e| {
        AutomationError::new(
            "test_failed",
            format!("Cannot start {}: {e}", cli.display()),
        )
    })?;
    let result = (|| {
        let mut stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        let (send, receive) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if send.send(line).is_err() {
                    break;
                }
            }
        });
        let requests = [
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"openxer-connect-test","version":crate::VERSION}}}),
            json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}),
        ];
        for request in requests {
            writeln!(stdin, "{request}")
                .and_then(|()| stdin.flush())
                .map_err(|e| {
                    AutomationError::new("test_failed", format!("The server stopped: {e}"))
                })?;
        }
        let deadline = std::time::Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            let line = receive.recv_timeout(left).map_err(|e| {
                AutomationError::new(
                    "test_failed",
                    match e {
                        mpsc::RecvTimeoutError::Timeout => {
                            format!("No answer within {} seconds", timeout.as_secs())
                        }
                        mpsc::RecvTimeoutError::Disconnected => {
                            "The server stopped without answering".into()
                        }
                    },
                )
            })?;
            let Ok(reply) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if let Some(error) = reply.get("error") {
                return Err(AutomationError::new(
                    "test_failed",
                    format!(
                        "The server refused the request: {}",
                        error["message"].as_str().unwrap_or("error")
                    ),
                ));
            }
            if reply["id"] == json!(2) {
                return reply["result"]["tools"]
                    .as_array()
                    .map(Vec::len)
                    .ok_or_else(|| {
                        AutomationError::new("test_failed", "The tool list was missing")
                    });
            }
        }
    })();
    let _ = child.kill();
    let _ = child.wait();
    result
}

// ---------------------------------------------------------------- CLI glue

/// What `openxer-cli connect` was asked to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    Status,
    Apply {
        client: Client,
        disconnect: bool,
        dry_run: bool,
    },
    /// Connect every found app that is not connected (or needs repair).
    All {
        dry_run: bool,
    },
    ClaudeCode,
    Test,
    /// The setup prompt for any AI tool that can run commands.
    Prompt,
}

/// Runs a connect request; the result goes in the CLI's JSON envelope.
pub fn run(request: &Request, env: &Env) -> Result<Value> {
    match request {
        Request::Status => Ok(json!({
            "cli": env.cli,
            "cli_found": env.cli.as_deref().is_some_and(Path::is_file),
            "cli_installed": env.cli.as_deref().is_some_and(|c| is_installed_copy(c, &|p: &Path| p.is_file())),
            "clients": status(env),
        })),
        Request::All { dry_run } => {
            env.cli()?;
            let results = connect_all(env, *dry_run);
            let message = if results.is_empty() {
                "Nothing to connect: every app found is already connected, or none was found."
                    .to_string()
            } else {
                results
                    .iter()
                    .map(|r| format!("{}: {}", r.client.name(), r.message))
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            Ok(json!({
                "action": "connect_all",
                "dry_run": dry_run,
                "ok": results.iter().all(|r| r.ok),
                "clients": results,
                "message": message,
            }))
        }
        Request::Apply {
            client,
            disconnect,
            dry_run,
        } => {
            let changes = apply(env, *client, *disconnect, *dry_run)?;
            Ok(json!({
                "client": client,
                "action": if *disconnect { "disconnect" } else { "connect" },
                "dry_run": dry_run,
                "changed": changes.iter().any(|c| c.changed),
                "files": changes,
                "message": finish_message(*client, *disconnect, &changes, *dry_run),
                "next_step": (!*disconnect).then(|| client.next_step()),
            }))
        }
        Request::ClaudeCode => {
            let cli = env.cli()?;
            Ok(json!({
                "client": Client::ClaudeCode,
                "found": env.found(Client::ClaudeCode),
                "command": claude_code_command(cli),
                "message": "Run this command in a terminal; OpenXER does not run it for you.",
            }))
        }
        Request::Prompt => {
            let cli = env.cli()?;
            Ok(json!({
                "cli": cli,
                "prompt": setup_prompt(cli),
                "message": "Paste this prompt into an AI tool that can run commands.",
            }))
        }
        Request::Test => {
            let cli = env.cli()?;
            let tools = test_connection(cli, TEST_TIMEOUT)?;
            Ok(json!({"cli": cli, "tools": tools, "message": format!("Working: {tools} tools")}))
        }
    }
}

#[cfg(test)]
mod tests;
