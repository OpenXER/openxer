//! Enterprise > External Applications and View > Launch. The user keeps a
//! short list of programs (a name, a command line and arguments) in the
//! workspace. Launching one starts it directly, without a shell; an
//! argument that contains `%xer%` is given a fresh XER export of the open
//! project, written to a temporary file just before the program starts.
use super::*;
use serde::{Deserialize, Serialize};

/// Workspace preference holding the list.
pub const PREF_KEY: &str = "external_apps_v1";
/// The argument token replaced by the path of a fresh XER export.
pub const XER_TOKEN: &str = "%xer%";
/// Most entries the list holds.
pub const MAX_APPS: usize = 30;
const MAX_NAME: usize = 60;
const MAX_TEXT: usize = 520;
/// Name of the private folder that holds the exports, and what every
/// exported file name starts with.
const FOLDER: &str = "launch";
const EXPORT_PREFIX: &str = "OpenXER-";
/// Exports older than this many seconds are deleted at the next launch.
const KEEP_SECONDS: u64 = 24 * 3600;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalApp {
    pub name: String,
    /// The program: a path, or a name found on the search path.
    pub command: String,
    /// Arguments, separated by spaces; double quotes group words.
    pub arguments: String,
}

impl ExternalApp {
    /// The entry takes the XER export of the open project.
    pub fn uses_xer(&self) -> bool {
        self.arguments
            .to_ascii_lowercase()
            .contains(&XER_TOKEN.to_ascii_lowercase())
    }

    /// The program with any quotes around it removed.
    pub fn program(&self) -> &str {
        self.command.trim().trim_matches('"').trim()
    }
}

fn has_control(text: &str) -> bool {
    text.chars().any(char::is_control)
}

/// Does the program text name a place (a path) rather than a bare name?
fn is_path(program: &str) -> bool {
    program.contains(['\\', '/']) || program.as_bytes().get(1) == Some(&b':')
}

/// Splits arguments at spaces; a double quote starts and ends a word that
/// may hold spaces. Nothing else is interpreted: no variables, no pipes.
pub fn split_arguments(text: &str) -> Result<Vec<String>, String> {
    let mut words = vec![];
    let mut word = String::new();
    let mut quoted = false;
    let mut started = false;
    for ch in text.chars() {
        match ch {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            c if c.is_whitespace() && !quoted => {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            c => {
                word.push(c);
                started = true;
            }
        }
    }
    if quoted {
        return Err("The arguments have a quote mark that is never closed.".into());
    }
    if started {
        words.push(word);
    }
    Ok(words)
}

/// The arguments with every `%xer%` replaced by `xer` (any letter case).
pub fn expand_arguments(words: &[String], xer: &str) -> Vec<String> {
    let lower = XER_TOKEN.to_ascii_lowercase();
    words
        .iter()
        .map(|w| {
            let mut out = String::new();
            let mut rest = w.as_str();
            while let Some(at) = rest.to_ascii_lowercase().find(&lower) {
                out.push_str(&rest[..at]);
                out.push_str(xer);
                rest = &rest[at + lower.len()..];
            }
            out.push_str(rest);
            out
        })
        .collect()
}

/// Checks an entry before it joins the list; `others` are the other
/// entries' names.
pub fn validate(app: &ExternalApp, others: &[&str]) -> Result<(), String> {
    check(app, others, true)
}

/// The entry's shape; with `files` also that a program given as a path is
/// a file on this machine.
fn check(app: &ExternalApp, others: &[&str], files: bool) -> Result<(), String> {
    let name = app.name.trim();
    if name.is_empty() {
        return Err("Enter a name for the application.".into());
    }
    if name.chars().count() > MAX_NAME {
        return Err(format!("The name is longer than {MAX_NAME} characters."));
    }
    if has_control(name) {
        return Err("The name cannot hold control characters.".into());
    }
    if others.iter().any(|o| o.trim().eq_ignore_ascii_case(name)) {
        return Err(format!("An application named {name} is already listed."));
    }
    let program = app.program();
    if program.is_empty() {
        return Err("Enter the command line of the application.".into());
    }
    if program.len() > MAX_TEXT || has_control(program) || program.contains('"') {
        return Err("The command line is not a valid program name or path.".into());
    }
    if program.contains(['<', '>', '|', '*', '?']) {
        return Err("The command line cannot hold any of < > | * ?".into());
    }
    if files && is_path(program) && !Path::new(program).is_file() {
        return Err(format!("{program} is not a file."));
    }
    if app.arguments.len() > MAX_TEXT || app.arguments.chars().any(|c| c.is_control()) {
        return Err("The arguments hold characters that cannot be passed.".into());
    }
    split_arguments(&app.arguments)?;
    Ok(())
}

/// The workspace list from its preference text; unreadable or invalid
/// entries are left out. A program path is not looked up here: it may be
/// on a drive that is not connected until it is launched.
pub fn from_json(text: &str) -> Vec<ExternalApp> {
    let Ok(list) = serde_json::from_str::<Vec<ExternalApp>>(text) else {
        return vec![];
    };
    let mut out: Vec<ExternalApp> = vec![];
    for app in list {
        let entry = ExternalApp {
            name: app.name.trim().to_string(),
            command: app.command.trim().to_string(),
            arguments: app.arguments.trim().to_string(),
        };
        let names: Vec<&str> = out.iter().map(|a| a.name.as_str()).collect();
        if out.len() < MAX_APPS && check(&entry, &names, false).is_ok() {
            out.push(entry);
        }
    }
    out
}

pub fn to_json(apps: &[ExternalApp]) -> String {
    serde_json::to_string(apps).unwrap_or_else(|_| "[]".into())
}

/// Starts the program with `xer` as the export path when the arguments ask
/// for one. The child is not waited for and has no console window.
pub fn start(app: &ExternalApp, xer: Option<&Path>) -> Result<std::process::Child, String> {
    let program = app.program();
    if program.is_empty() || has_control(program) {
        return Err("The command line is not a valid program name or path.".into());
    }
    if is_path(program) && !Path::new(program).is_file() {
        return Err(format!("{} was not found: {program}", app.name));
    }
    let words = split_arguments(&app.arguments)?;
    let words = match xer {
        Some(path) => expand_arguments(&words, &path.display().to_string()),
        None => words,
    };
    let mut command = std::process::Command::new(program);
    command
        .args(words)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: a console program does not open a console.
        command.creation_flags(0x0800_0000);
    }
    command
        .spawn()
        .map_err(|e| format!("{} could not be started: {e}", app.name))
}

/// A file name made of the characters every file system accepts.
fn safe_name(text: &str) -> String {
    let name: String = text
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .take(40)
        .collect();
    if name.trim_matches('.').is_empty() {
        "project".into()
    } else {
        name
    }
}

/// Deletes exports left by earlier launches once they are a day old. Only
/// plain files of ours inside the private folder are touched; a link is
/// never followed or removed.
fn purge_old_exports(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !(name.starts_with(EXPORT_PREFIX) && name.ends_with(".xer")) {
            continue;
        }
        let Ok(meta) = std::fs::symlink_metadata(entry.path()) else {
            continue;
        };
        let old = meta
            .modified()
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age.as_secs() > KEEP_SECONDS);
        if old && meta.file_type().is_file() {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Writes the export into the private folder `dir` as a new file with an
/// unpredictable name and returns its path.
pub fn write_export(dir: &Path, project_id: &str, bytes: &[u8]) -> Result<PathBuf, String> {
    let dir = crate::util::refuse_link(dir.to_path_buf())?;
    purge_old_exports(&dir);
    crate::util::write_new_file(
        &dir,
        &format!("{EXPORT_PREFIX}{}", safe_name(project_id)),
        "xer",
        bytes,
    )
}

/// The Enterprise > External Applications dialog while open.
#[derive(Clone, Debug, Default)]
pub struct Dialog {
    pub selected: Option<usize>,
    pub name: String,
    pub command: String,
    pub arguments: String,
    pub error: Option<String>,
}

impl Dialog {
    fn entry(&self) -> ExternalApp {
        ExternalApp {
            name: self.name.trim().to_string(),
            command: self.command.trim().to_string(),
            arguments: self.arguments.trim().to_string(),
        }
    }
    fn load(&mut self, index: usize, app: &ExternalApp) {
        self.selected = Some(index);
        self.name = app.name.clone();
        self.command = app.command.clone();
        self.arguments = app.arguments.clone();
        self.error = None;
    }
}

#[derive(Default)]
pub struct State {
    pub apps: Vec<ExternalApp>,
    pub dialog: Option<Dialog>,
}

impl App {
    pub(crate) fn restore_external_apps(&mut self) {
        if let Ok(Some(text)) = self.store.preference(PREF_KEY) {
            self.external.apps = from_json(&text);
        }
    }

    fn save_external_apps(&mut self) {
        let text = to_json(&self.external.apps);
        if let Err(e) = self.store.set_preference(PREF_KEY, &text) {
            self.error(format!("Cannot save the application list: {e}"));
        }
    }

    pub(crate) fn open_external_apps(&mut self) {
        self.external.dialog = Some(Dialog::default());
    }

    /// Adds, or with `replace` changes the selected entry. The list stays
    /// as it was when the entry is refused.
    pub(crate) fn external_apps_commit(&mut self, replace: bool) -> Result<(), String> {
        let Some(d) = self.external.dialog.as_ref() else {
            return Ok(());
        };
        let entry = d.entry();
        let target = d.selected.filter(|_| replace);
        if replace && target.is_none() {
            return Err("Select an application to change.".into());
        }
        if target.is_none() && self.external.apps.len() >= MAX_APPS {
            return Err(format!("The list holds at most {MAX_APPS} applications."));
        }
        let others: Vec<&str> = self
            .external
            .apps
            .iter()
            .enumerate()
            .filter(|(i, _)| Some(*i) != target)
            .map(|(_, a)| a.name.as_str())
            .collect();
        validate(&entry, &others)?;
        let index = match target {
            Some(i) => {
                self.external.apps[i] = entry;
                i
            }
            None => {
                self.external.apps.push(entry);
                self.external.apps.len() - 1
            }
        };
        self.save_external_apps();
        let app = self.external.apps[index].clone();
        if let Some(d) = self.external.dialog.as_mut() {
            d.load(index, &app);
        }
        Ok(())
    }

    pub(crate) fn external_apps_delete(&mut self) {
        let Some(i) = self.external.dialog.as_ref().and_then(|d| d.selected) else {
            return;
        };
        if i < self.external.apps.len() {
            self.external.apps.remove(i);
            self.save_external_apps();
        }
        self.external.dialog = Some(Dialog::default());
    }

    /// View > Launch: starts entry `index`, with a fresh export of the open
    /// project when its arguments ask for one.
    pub(crate) fn launch_external(&mut self, index: usize) -> Result<std::process::Child, String> {
        let app = self
            .external
            .apps
            .get(index)
            .cloned()
            .ok_or("That application is no longer listed.")?;
        let export = if app.uses_xer() {
            let bytes = self.project.updated_document().to_bytes()?;
            let dir = crate::util::private_folder(FOLDER, "OpenXER-launch")?;
            Some(write_export(
                &dir,
                &self.project.model.project.name,
                &bytes,
            )?)
        } else {
            None
        };
        let child = start(&app, export.as_deref())?;
        self.status = format!("Started {}", app.name);
        Ok(child)
    }

    /// The View > Launch submenu: one item per listed application.
    pub(crate) fn launch_items(&mut self, ui: &mut egui::Ui) {
        if self.external.apps.is_empty() {
            self.item(
                ui,
                "menu_launch_none",
                chrome::Item::new("No applications listed").enabled(false),
            );
            return;
        }
        for i in 0..self.external.apps.len() {
            let label = grid::presets::menu_text(&self.external.apps[i].name);
            if self.item(ui, &format!("menu_launch_{i}"), chrome::Item::new(&label)) {
                match self.launch_external(i) {
                    Ok(child) => {
                        // Nobody waits for the program; reap it in the background.
                        std::thread::spawn(move || {
                            let mut child = child;
                            let _ = child.wait();
                        });
                    }
                    Err(e) => self.error(e),
                }
            }
        }
    }

    /// External Applications: a kit dialog with the list and the fields on
    /// the left and Add, Change, Delete, Close and Help in the button column.
    /// The Name field has the keyboard when it opens, Enter and Escape close
    /// it (Close is the default button), and a refusal shows in red under the field it is about.
    pub(crate) fn external_apps_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.external.dialog.take() else {
            return;
        };
        use crate::dialog_kit::{self as kit, Col, Entry};
        const W: f32 = 648.;
        const H: f32 = 300.;
        let window = egui::Id::new(("dialog_window", "External Applications"));
        let opening = !ctx.memory(|m| {
            m.areas()
                .is_visible(&egui::LayerId::new(egui::Order::Middle, window))
        });
        let mut pick = None;
        let apps = self.external.apps.clone();
        let error_field = d.error.as_deref().map(field_of);
        let error_text = d.error.clone().unwrap_or_default();
        let (clicked, closed, actions) = kit::window(
            ctx,
            "External Applications",
            "External Applications",
            Vec2::new(W, H),
            |c| {
                let selected = d.selected.is_some();
                let out = kit::list(
                    c,
                    "external_apps",
                    (8., 8., 520., 140.),
                    None,
                    &[
                        Col {
                            title: "Name",
                            width: 160.,
                        },
                        Col {
                            title: "Command Line",
                            width: 341.,
                        },
                    ],
                    apps.len(),
                    d.selected,
                    None,
                    &mut |c, i, k, rect, sel| {
                        let app = &apps[i];
                        let text = if k == 0 { &app.name } else { &app.command };
                        kit::cell_text(c, rect, text, sel, 4.);
                    },
                );
                pick = out.clicked;
                if apps.is_empty() {
                    kit::text(c, 14., 48., "No applications are listed.", true);
                }
                let mut y = 158.;
                for (key, label, field, limit) in [
                    ("external_apps_name", "Name", Field::Name, MAX_NAME),
                    (
                        "external_apps_command",
                        "Command Line",
                        Field::Command,
                        MAX_TEXT,
                    ),
                    (
                        "external_apps_arguments",
                        "Arguments",
                        Field::Arguments,
                        MAX_TEXT,
                    ),
                ] {
                    kit::text(c, 8., y + 4., label, true);
                    let text = match field {
                        Field::Name => &mut d.name,
                        Field::Command => &mut d.command,
                        Field::Arguments => &mut d.arguments,
                    };
                    let r = c.edit(key, 100., y, 428., 21., text, true, Align::Min);
                    if text.chars().count() > limit {
                        *text = text.chars().take(limit).collect();
                    }
                    if opening && field == Field::Name {
                        r.request_focus();
                    }
                    y += 27.;
                    if error_field == Some(Some(field)) {
                        kit::error_text(c, 100., y - 4., 428., &error_text);
                        y += 16.;
                    }
                }
                if error_field == Some(None) {
                    kit::error_text(c, 8., y, 520., &error_text);
                    y += 16.;
                }
                kit::text(
                    c,
                    8.,
                    y + 2.,
                    &format!(
                        "{XER_TOKEN} in the arguments becomes a fresh XER export of the open project."
                    ),
                    true,
                );
                kit::strip(c, W, H);
                let column = kit::button_column(
                    c,
                    W,
                    &[
                        Entry::Button("external_apps_add", "Add", true, false),
                        Entry::Button("external_apps_change", "Change", selected, false),
                        Entry::Button("external_apps_delete", "Delete", selected, false),
                        Entry::Button("external_apps_close", "Close", true, true),
                        Entry::help("external_apps_help"),
                    ],
                );
                // Enter closes, as in other list dialogs; Add is a click.
                kit::mark_default(c, "external_apps_close");
                column.clicked
            },
        );
        self.record_dialog_actions(actions);
        let clicked = match clicked.as_deref() {
            Some("external_apps_add") => Some(0),
            Some("external_apps_change") => Some(1),
            Some("external_apps_delete") => Some(2),
            Some("external_apps_close") => Some(3),
            Some("external_apps_help") => Some(4),
            _ => None,
        };
        let open = !closed;
        if let Some(i) = pick {
            if let Some(app) = apps.get(i) {
                d.load(i, app);
            }
        }
        self.external.dialog = Some(d);
        match clicked {
            Some(i @ (0 | 1)) => {
                let result = self.external_apps_commit(i == 1);
                if let Some(d) = self.external.dialog.as_mut() {
                    d.error = result.err();
                }
            }
            Some(2) => self.external_apps_delete(),
            Some(4) => self.open_help_for("External Applications"),
            _ => {}
        }
        if !open || clicked == Some(3) {
            self.external.dialog = None;
        }
    }
}

/// A field of the External Applications dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Field {
    Name,
    Command,
    Arguments,
}

/// The field a refusal from [`validate`] is about; `None` for one about the
/// whole entry.
fn field_of(error: &str) -> Option<Field> {
    let starts = |prefixes: &[&str]| prefixes.iter().any(|p| error.starts_with(p));
    if starts(&["Enter a name", "The name", "An application named"]) {
        Some(Field::Name)
    } else if starts(&["Enter the command line", "The command line"])
        || error.ends_with(" is not a file.")
    {
        Some(Field::Command)
    } else if starts(&["The arguments"]) {
        Some(Field::Arguments)
    } else {
        None
    }
}

#[cfg(test)]
mod tests;
