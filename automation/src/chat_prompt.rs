//! The instructions every chat assistant gets, whichever agent or model
//! answers: one scheduling core, the user's own standing instructions when
//! there are any, and the mode. The methods for a build, an update, an
//! audit and the rest are playbooks the assistant loads when it needs one
//! (`openxer_playbook`), so they cost nothing up front.
use crate::chat_tools::ChatMode;
use std::path::{Path, PathBuf};

/// The core, sent with every chat. Kept short on purpose: every character
/// is paid for on every turn.
pub const CORE: &str = "You are a senior CPM scheduler working inside OpenXER on the user's open schedule. You build, update and analyze schedules for construction and industrial projects.
Work from the schedule, not from memory: read the data with the tools before you state a date, duration, float value or logic tie. Never invent an activity, a date or a number. If the data does not answer the question, say so.
Think like a scheduler: logic drives dates. Look at the driving path, float, constraints, calendars, open ends, lags and out-of-sequence progress before you explain why something moved. Separate what the schedule says from your opinion of it.
When asked to change the schedule, propose the smallest set of edits that does it, explain the effect on the finish date and float, and wait for approval. Never change logic or durations just to hit a date without saying that is what you are doing.
Write for people in the field: plain words, Activity IDs with names, dates as the project shows them. Be brief. For a build, an update, an audit or a schedule check, load the matching playbook first with openxer_playbook.
Use only the OpenXER tools; they read the open project, so never pass a file. In openxer_load pick fields, filters and sort, and keep pages small. When asked to filter, group, sort, show columns, select, zoom, trace paths, switch views, open a dialog or export, call openxer_ui directly. When a request is ambiguous or has choices, call openxer_ask_user with short options and wait.";

/// The line for each mode.
pub fn mode_line(mode: ChatMode) -> &'static str {
    match mode {
        ChatMode::Ask => "Ask mode: read only. If asked to change the schedule, say the chat can be switched to Edit.",
        ChatMode::Edit => "Edit mode: propose changes with one openxer_apply call. It changes nothing until the user approves it in OpenXER, so never say it is done.",
    }
}

/// The complete instructions of a chat: the core, the user's standing
/// instructions (from [`read_user_instructions`]) and the mode line. Both
/// hosted agents and an in-process provider send exactly this.
pub fn system_prompt(mode: ChatMode, user: Option<&str>) -> String {
    system_prompt_with(mode, user, None)
}

/// Most characters of per-project standing instructions sent.
pub const PROJECT_LIMIT: usize = 2000;

/// The complete instructions of a chat with optional per-project rules:
/// core, global user standing instructions, project-specific instructions,
/// and the mode line.
pub fn system_prompt_with(mode: ChatMode, user: Option<&str>, project: Option<&str>) -> String {
    let mut text = String::from(CORE);
    if let Some(user) = user.map(str::trim).filter(|u| !u.is_empty()) {
        text.push_str("\nThe user's standing instructions (follow them unless they conflict with the above):\n");
        text.push_str(user);
    }
    if let Some(project) = project.map(str::trim).filter(|p| !p.is_empty()) {
        let trimmed: String = project.chars().take(PROJECT_LIMIT).collect();
        text.push_str("\nProject-specific rules for this schedule (follow them unless they conflict with the above):\n");
        text.push_str(&trimmed);
    }
    text.push('\n');
    text.push_str(mode_line(mode));
    text
}

/// Decode UTF-8/UTF-16 text, strip HTML comments and normalize CRLF to LF.
pub(crate) fn clean_markdown(bytes: &[u8]) -> String {
    without_comments(&decode(bytes)).replace("\r\n", "\n")
}

/// The most characters of the user's file that are sent.
pub const USER_LIMIT: usize = 4000;

/// The user's standing instructions as read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserInstructions {
    pub text: String,
    /// The file was longer than [`USER_LIMIT`] and was cut there.
    pub cut: bool,
    pub path: PathBuf,
}

/// The OpenXER data folder: where the workspace is (`OPENXER_DATA_DIR`,
/// else as `locate::default_workspace_in` decides for a program in
/// `exe_dir`).
pub fn data_folder_in(
    exe_dir: Option<&Path>,
    data_dir: Option<std::ffi::OsString>,
    local_appdata: Option<std::ffi::OsString>,
) -> PathBuf {
    let workspace = crate::locate::default_workspace_in(exe_dir, data_dir, local_appdata);
    workspace
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// [`data_folder_in`] for this process.
pub fn data_folder() -> PathBuf {
    let exe = std::env::current_exe().ok();
    data_folder_in(
        exe.as_deref().and_then(Path::parent),
        std::env::var_os(crate::locate::DATA_DIR_VAR),
        std::env::var_os("LOCALAPPDATA"),
    )
}

/// The user's instructions file in `data_folder`: `chat\AGENTS.md`.
pub fn instructions_path(data_folder: &Path) -> PathBuf {
    data_folder.join("chat").join("AGENTS.md")
}

/// Text in UTF-8 (with or without a byte order mark) or UTF-16 with its
/// mark; anything else is read as UTF-8 with bad bytes replaced.
fn decode(bytes: &[u8]) -> String {
    let utf16 = |little: bool| {
        let units: Vec<u16> = bytes[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| {
                if little {
                    u16::from_le_bytes([c[0], c[1]])
                } else {
                    u16::from_be_bytes([c[0], c[1]])
                }
            })
            .collect();
        String::from_utf16_lossy(&units)
    };
    match bytes {
        [0xEF, 0xBB, 0xBF, rest @ ..] => String::from_utf8_lossy(rest).into_owned(),
        [0xFF, 0xFE, ..] => utf16(true),
        [0xFE, 0xFF, ..] => utf16(false),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// `text` without `<!-- ... -->` comments (an unclosed one runs to the end).
fn without_comments(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("<!--") {
        out.push_str(&rest[..start]);
        match rest[start..].find("-->") {
            Some(end) => rest = &rest[start + end + 3..],
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The user's standing instructions in the file at `path` (see
/// [`instructions_path`]), comments left out, at most [`USER_LIMIT`]
/// characters. `None` when the file is missing, unreadable or holds
/// nothing but comments and blank lines.
pub fn read_user_instructions(path: &Path) -> Option<UserInstructions> {
    let bytes = std::fs::read(path).ok()?;
    let text = without_comments(&decode(&bytes)).replace("\r\n", "\n");
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let cut = text.chars().count() > USER_LIMIT;
    let text: String = text.chars().take(USER_LIMIT).collect();
    Some(UserInstructions {
        text,
        cut,
        path: path.to_path_buf(),
    })
}

/// What a new instructions file holds: only comments, so it sends nothing
/// until the user writes their own lines.
pub const STARTER: &str = "<!--
Your standing instructions for OpenXER's Chat. Every assistant reads the
lines you write below this note (it ignores this note itself), at most 4000
characters. Write plain sentences, for example:

Write dates as 12-Mar-27.
Name activities by Activity ID, then name.
Treat total float under 10 days as near critical.
Our crews work 10-hour days, Monday to Thursday.
-->
";

/// Create the instructions file at `path` with [`STARTER`] if it does not
/// exist yet (an existing file is never changed); returns its path.
pub fn create_user_instructions(path: &Path) -> std::io::Result<PathBuf> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(mut file) => {
            use std::io::Write;
            file.write_all(STARTER.as_bytes())?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e),
    }
    Ok(path.to_path_buf())
}
