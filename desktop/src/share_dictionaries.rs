//! Admin > Share Dictionaries Across Projects: move a workspace that keeps
//! calendars, resources, codes and the other dictionaries inside each
//! project (workspace schema 1) to one shared workspace dictionary
//! (schema 3). The dialog shows what the conversion would do, per project,
//! before anything changes; Apply first copies the workspace into
//! `backups\`, converts it in one transaction and checks that every
//! project's dates and float are the same as before (`Store::migrate_dictionary`).
use super::*;
use crate::dialog_kit::{self, Col};
use openxer_automation::workspace_db;

pub const SALT: &str = "share_dictionaries";
pub const TITLE: &str = "Share Dictionaries Across Projects";
/// Workspace preference noting that the startup notice was shown.
const NOTICE_KEY: &str = "share_dictionaries_notice_v1";
/// Client size: the lists and the standard button column beside them.
const W: f32 = 744.;
const H: f32 = 470.;

/// One project's line of the preview.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProjectLine {
    pub project: String,
    pub shared: usize,
    pub added: usize,
    pub renumbered: usize,
    pub copies: usize,
}

/// The open dialog.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShareDialog {
    pub projects: Vec<ProjectLine>,
    /// What changes, one line each (renamed copies with the fields that
    /// differ, renumbered rows).
    pub details: Vec<String>,
    /// The dictionary rows the workspace would share, by table.
    pub summary: String,
    pub selected: Option<usize>,
    /// After Apply: what was done.
    pub done: Option<String>,
    pub error: Option<String>,
}

/// What kind of row a report's table holds.
fn kind(table: &serde_json::Value) -> &'static str {
    workspace_db::kind_label(table.as_str().unwrap_or_default())
}

/// The preview (or the result) of a conversion, from its JSON report.
pub fn read_report(report: &serde_json::Value) -> ShareDialog {
    let mut d = ShareDialog::default();
    let count = |v: &serde_json::Value| v.as_array().map_or(0, Vec::len);
    for p in report["projects"].as_array().into_iter().flatten() {
        let name = p["project"].as_str().unwrap_or_default().to_string();
        d.projects.push(ProjectLine {
            project: name.clone(),
            shared: p["shared"].as_u64().unwrap_or(0) as usize,
            added: p["added"].as_u64().unwrap_or(0) as usize,
            renumbered: count(&p["renumbered"]),
            copies: count(&p["copies"]),
        });
        for c in p["copies"].as_array().into_iter().flatten() {
            let differences: Vec<&str> = c["differences"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str())
                .collect();
            d.details.push(format!(
                "{name}: {} \"{}\" is kept as \"{}\" (it differs in {})",
                kind(&c["table"]),
                c["name"].as_str().unwrap_or_default(),
                c["renamed"].as_str().unwrap_or_default(),
                if differences.is_empty() {
                    "its child rows".to_string()
                } else {
                    differences.join(", ")
                }
            ));
        }
        for r in p["renamed_to_match"].as_array().into_iter().flatten() {
            d.details.push(format!(
                "{name}: {} \"{}\" is the same as \"{}\" and takes that name",
                kind(&r["table"]),
                r["name"].as_str().unwrap_or_default(),
                r["renamed"].as_str().unwrap_or_default()
            ));
        }
        for r in p["renumbered"].as_array().into_iter().flatten() {
            let label = r["name"].as_str().unwrap_or_default();
            d.details.push(format!(
                "{name}: {} {}{} gets id {} (its id is taken)",
                kind(&r["table"]),
                r["from_id"],
                if label.is_empty() {
                    String::new()
                } else {
                    format!(" \"{label}\"")
                },
                r["to_id"]
            ));
        }
    }
    let rows: Vec<String> = report["dictionary_rows"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(t, n)| format!("{} {n}", workspace_db::kind_label(t)))
        .collect();
    let total: u64 = report["dictionary_rows"]
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(_, n)| n.as_u64())
        .sum();
    d.summary = format!(
        "{} project(s) would share {total} dictionary row(s): {}.",
        d.projects.len(),
        if rows.is_empty() {
            "none".to_string()
        } else {
            rows.join(", ")
        }
    );
    if d.details.is_empty() {
        d.details
            .push("No dictionary row is renamed or renumbered.".into());
    }
    d
}

impl App {
    /// Admin > Share Dictionaries Across Projects: preview the conversion.
    pub(crate) fn open_share_dictionaries(&mut self) {
        match self.store.shared() {
            Ok(false) => {}
            Ok(true) => {
                self.status =
                    "This workspace already shares its dictionaries across projects".into();
                return;
            }
            Err(e) => {
                self.error(e);
                return;
            }
        }
        // Unsaved changes go into the preview.
        if self.any_dirty() {
            self.save();
            if self.any_dirty() {
                return;
            }
        }
        match self.store.migrate_dictionary(true) {
            Ok(report) => self.share_dictionaries = Some(read_report(&report)),
            Err(e) => {
                self.share_dictionaries = Some(ShareDialog {
                    error: Some(format!("The workspace cannot be converted: {e}")),
                    ..Default::default()
                })
            }
        }
    }

    /// Apply: back up, convert, check, then reopen the open projects.
    fn apply_share_dictionaries(&mut self) -> Result<String, String> {
        if self.any_dirty() {
            self.save();
            if self.any_dirty() {
                return Err("The open projects could not be saved first.".into());
            }
        }
        let ids = self.open_ids();
        let active = self.open_set.active;
        let report = self.store.migrate_dictionary(false)?;
        // From here the conversion is committed: whatever follows, the
        // message says so.
        self.reset_dictionary_sync();
        // The open projects come back as the dictionary holds them; the
        // conversion is not an undo step (the backup keeps the workspace as
        // it was).
        let reopened = match active {
            Some(a) => self.store.load(a).map(|project| {
                self.load(project);
                if ids.len() > 1 {
                    self.open_project_set(&ids);
                    self.activate_id(a);
                }
            }),
            None => Ok(()),
        };
        Ok(conversion_message(&report, reopened))
    }

    /// On opening a workspace that keeps dictionaries per project: say once,
    /// in the status bar, where to share them. Nothing is converted.
    pub(crate) fn note_dictionary_sharing(&mut self) {
        if self.store.shared() != Ok(false) || self.saved_projects.len() < 2 {
            return;
        }
        if self.store.preference(NOTICE_KEY).ok().flatten().is_some() {
            return;
        }
        let _ = self.store.set_preference(NOTICE_KEY, "shown");
        self.status = format!(
            "Calendars, resources and codes are kept in each project of this workspace. Admin > {TITLE}... shares them; nothing changes until you apply it."
        );
    }

    pub(crate) fn share_dictionaries_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.share_dictionaries.clone() else {
            return;
        };
        let cols = [
            Col {
                title: "Project",
                width: 200.,
            },
            Col {
                title: "Shared",
                width: 70.,
            },
            Col {
                title: "Added",
                width: 70.,
            },
            Col {
                title: "Renumbered",
                width: 80.,
            },
            Col {
                title: "Renamed copies",
                width: 96.,
            },
        ];
        let detail_cols = [Col {
            title: "What changes",
            width: 614.,
        }];
        let finished = d.done.is_some() || (d.error.is_some() && d.projects.is_empty());
        let ((apply, cancel, help, picked), closed, actions) = dialog_kit::window(
            ctx,
            SALT,
            TITLE,
            Vec2::new(W, H),
            |c| {
                let intro = [
                    "Keep calendars, resources, roles, global and EPS codes, user defined fields and the",
                    "other dictionaries once for the whole workspace, so every project offers the same ones.",
                    "Nothing changes until you click Apply. Apply copies the workspace into backups first,",
                    "then checks that every project's dates and float are the same as before.",
                ];
                dialog_kit::lines(c, 12., 8., 15., &intro, true);
                c.label(12., 72., 17., &d.summary, true);
                let projects = d.projects.clone();
                dialog_kit::list(
                    c,
                    "share_dictionaries_projects",
                    (12., 94., 520., 150.),
                    None,
                    &cols,
                    projects.len(),
                    None,
                    None,
                    &mut |c, row, col, rect, selected| {
                        let p = &projects[row];
                        let text = match col {
                            0 => p.project.clone(),
                            1 => p.shared.to_string(),
                            2 => p.added.to_string(),
                            3 => p.renumbered.to_string(),
                            _ => p.copies.to_string(),
                        };
                        dialog_kit::cell_text(c, rect, &text, selected, 3.);
                    },
                );
                let details = d.details.clone();
                let out = dialog_kit::list(
                    c,
                    "share_dictionaries_details",
                    (12., 252., 616., 150.),
                    None,
                    &detail_cols,
                    details.len(),
                    d.selected,
                    None,
                    &mut |c, row, _, rect, selected| {
                        dialog_kit::cell_text(c, rect, &details[row], selected, 3.);
                    },
                );
                let message = d.done.as_deref().or(d.error.as_deref()).unwrap_or(
                    "Edit > Undo does not take a conversion back; the backup copy keeps the workspace as it was.",
                );
                let wrapped = wrap(message, 104);
                let shown: Vec<&str> = wrapped.iter().map(String::as_str).take(3).collect();
                dialog_kit::lines(c, 12., 410., 15., &shown, true);
                // Apply commits (it heads the column in OK's place).
                dialog_kit::strip(c, W, H);
                let column = dialog_kit::button_column(
                    c,
                    W,
                    &[
                        dialog_kit::Entry::Button(
                            "share_dictionaries_apply",
                            "Apply",
                            !finished && d.error.is_none(),
                            false,
                        ),
                        dialog_kit::Entry::Button(
                            "share_dictionaries_cancel",
                            if finished { "Close" } else { "Cancel" },
                            true,
                            false,
                        ),
                        dialog_kit::Entry::help("share_dictionaries_help"),
                    ],
                );
                let clicked = |key: &str| column.clicked.as_deref() == Some(key);
                (
                    clicked("share_dictionaries_apply"),
                    clicked("share_dictionaries_cancel"),
                    clicked("share_dictionaries_help"),
                    out.clicked,
                )
            },
        );
        self.record_dialog_actions(actions);
        if let Some(i) = picked {
            d.selected = Some(i);
        }
        if help {
            self.open_help_for(SALT);
        }
        if apply {
            match self.apply_share_dictionaries() {
                Ok(text) => {
                    self.status = "The workspace shares its dictionaries across projects".into();
                    d.done = Some(text);
                }
                Err(e) => d.error = Some(format!("Nothing was changed: {e}")),
            }
        }
        if cancel || closed || ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.share_dictionaries = None;
        } else {
            self.share_dictionaries = Some(d);
        }
    }
}

/// What the dialog says after a conversion `report` was committed, when
/// reopening the open project then gave `reopened`.
pub(crate) fn conversion_message(
    report: &serde_json::Value,
    reopened: Result<(), String>,
) -> String {
    let backup = report["backup"].as_str().unwrap_or_default();
    let file = std::path::Path::new(backup)
        .file_name()
        .map_or(String::new(), |f| f.to_string_lossy().into_owned());
    let done = format!(
        "The workspace now shares its dictionaries across {} project(s). Every project's dates and float were checked and are the same as before. The workspace as it was is kept in backups\\{file} beside it.",
        report["projects"].as_array().map_or(0, Vec::len)
    );
    match reopened {
        Ok(()) => done,
        Err(e) => format!(
            "{done} The open project could not be opened again ({e}); open it again from the list of workspace projects."
        ),
    }
}

/// Splits text at spaces into lines of at most `width` characters.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut out = vec![];
    let mut current = String::new();
    for word in text.split(' ') {
        if !current.is_empty() && current.chars().count() + 1 + word.chars().count() > width {
            out.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    out.push(current);
    out
}

#[cfg(test)]
mod tests;
