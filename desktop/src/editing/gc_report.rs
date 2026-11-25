//! The Global Change Report window: a header (change name, projects,
//! date), Select All and Clear All, one line per change with a Selected
//! check box, and the Save Report options that write the report to a text
//! or HTML file when the changes are committed.
use super::gc::Change;
use super::ops;
use super::*;
use crate::dialog_kit::{self as kit, Col, Entry};
use std::path::{Path, PathBuf};

/// Preference key of the Save Report options.
pub const REPORT_FILE_KEY: &str = "global_change_report_v1";

/// File name of the default report, in the user's Documents folder.
const DEFAULT_FILE: &str = "GlobalChangeReport.txt";

/// The report's columns, in the order the window and the files show them.
pub const COLUMNS: [&str; 8] = [
    "Selected",
    "Project ID",
    "WBS Code",
    "Activity ID",
    "Field",
    "Old Value",
    "New Value",
    "Exception",
];

/// Save Report: whether the report is written when the changes are
/// committed, where, as HTML or text, and whether an existing file is
/// replaced (otherwise the next free numbered name is used).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ReportFile {
    pub save: bool,
    pub path: String,
    pub html: bool,
    pub overwrite: bool,
}

impl Default for ReportFile {
    fn default() -> Self {
        Self {
            save: true,
            path: default_report_path().display().to_string(),
            html: false,
            overwrite: true,
        }
    }
}

/// The user's Documents folder (the one under OneDrive when Documents is
/// kept there), else the home folder. Test builds use a temporary folder
/// so no test ever writes to the user's own folders.
fn documents() -> PathBuf {
    if cfg!(test) {
        let dir = std::env::temp_dir().join("openxer-gc-report-tests");
        let _ = std::fs::create_dir_all(&dir);
        return dir;
    }
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    [
        home.join("OneDrive").join("Documents"),
        home.join("Documents"),
    ]
    .into_iter()
    .find(|p| p.is_dir())
    .unwrap_or(home)
}

pub fn default_report_path() -> PathBuf {
    documents().join(DEFAULT_FILE)
}

/// `path` with the extension the format needs: .html for HTML, .txt for
/// text, when it has one of the two (any other extension is kept).
pub fn with_format_extension(path: &str, html: bool) -> String {
    let p = Path::new(path);
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    match ext.as_deref() {
        Some("txt" | "htm" | "html") | None if !path.trim().is_empty() => p
            .with_extension(if html { "html" } else { "txt" })
            .display()
            .to_string(),
        _ => path.to_string(),
    }
}

/// Where the report goes: `path` itself when it may be replaced or does
/// not exist yet, else the first of `name1.ext`, `name2.ext` ... that is
/// free.
pub fn target_path(path: &Path, overwrite: bool) -> PathBuf {
    if overwrite || !path.exists() {
        return path.to_path_buf();
    }
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    (1u32..)
        .map(|k| path.with_file_name(format!("{stem}{k}{ext}")))
        .find(|p| !p.exists())
        .unwrap_or_else(|| path.to_path_buf())
}

pub struct GcReport {
    pub name: String,
    pub changes: Vec<Change>,
    /// The header's Projects and Date lines.
    pub projects: String,
    pub date: String,
    pub file: ReportFile,
}

impl GcReport {
    /// A report of `changes` dated today, with the default Save Report
    /// options and no project line.
    pub fn new(name: &str, changes: Vec<Change>) -> Self {
        Self {
            name: name.to_string(),
            changes,
            projects: String::new(),
            date: chrono::Local::now().format("%A, %B %-d, %Y").to_string(),
            file: ReportFile::default(),
        }
    }

    /// One line's cells after the Selected column.
    fn cells(c: &Change) -> [&str; 7] {
        [
            &c.project,
            &c.wbs,
            &c.activity,
            &c.title,
            &c.old,
            &c.new,
            c.error.as_deref().unwrap_or(""),
        ]
    }

    /// The header lines: (caption, value).
    fn header(&self) -> [(&str, &str); 3] {
        [
            ("Global Change", &self.name),
            ("Projects", &self.projects),
            ("Date", &self.date),
        ]
    }

    /// The report as tab-separated text: the header lines, a blank line,
    /// then the column titles and one row per line.
    pub fn text(&self) -> String {
        let mut rows: Vec<Vec<String>> = self
            .header()
            .iter()
            .map(|(k, v)| vec![k.to_string(), v.to_string()])
            .collect();
        rows.push(vec![]);
        rows.push(COLUMNS.map(String::from).to_vec());
        for c in &self.changes {
            let mut row = vec![if c.apply { "Yes" } else { "No" }.to_string()];
            row.extend(Self::cells(c).map(String::from));
            rows.push(row);
        }
        ops::to_tsv(&rows)
    }

    /// The report as one HTML page with the same header and rows.
    pub fn html(&self) -> String {
        use crate::reports::html::escape;
        let mut out = String::from(
            "<!DOCTYPE html>\n<html><head><meta charset=\"utf-8\"><title>Global Change Report</title>\n\
             <style>body{font-family:Arial,sans-serif;font-size:10pt}\
             table{border-collapse:collapse}th,td{border:1px solid #999;padding:2px 6px;text-align:left}\
             th{background:#ddd}</style></head><body>\n<table class=\"header\">\n",
        );
        for (k, v) in self.header() {
            out.push_str(&format!(
                "<tr><th>{}</th><td>{}</td></tr>\n",
                escape(k),
                escape(v)
            ));
        }
        out.push_str("</table>\n<br>\n<table class=\"changes\">\n<tr>");
        for title in COLUMNS {
            out.push_str(&format!("<th>{}</th>", escape(title)));
        }
        out.push_str("</tr>\n");
        for c in &self.changes {
            out.push_str("<tr>");
            out.push_str(&format!("<td>{}</td>", if c.apply { "Yes" } else { "No" }));
            for cell in Self::cells(c) {
                out.push_str(&format!("<td>{}</td>", escape(cell)));
            }
            out.push_str("</tr>\n");
        }
        out.push_str("</table>\n</body></html>\n");
        out
    }
}

/// Write the Global Change report `r` to `path` as tab-separated text, or
/// as HTML with `html`, whole or not at all.
pub(crate) fn write_gc_report(path: &Path, r: &GcReport, html: bool) -> Result<(), String> {
    let body = if html { r.html() } else { r.text() };
    openxer_xer::write_file_atomic(path, body.as_bytes())
        .map_err(|e| format!("Cannot write {}: {e}", path.display()))
}

impl App {
    /// The Save Report options last used, or the defaults.
    pub(crate) fn gc_report_file(&self) -> ReportFile {
        self.store
            .preference(REPORT_FILE_KEY)
            .ok()
            .flatten()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    fn save_gc_report_file(&mut self, file: &ReportFile) {
        let text = serde_json::to_string(file).unwrap_or_default();
        if let Err(e) = self.store.set_preference(REPORT_FILE_KEY, &text) {
            self.error(format!("Cannot save the report options: {e}"));
        }
    }

    /// The header's Projects line: the names of the projects the listed
    /// activities belong to, else the open project's.
    pub(crate) fn gc_report_projects(&self, changes: &[Change]) -> String {
        let catalog = columns::catalog(&self.project);
        let Some(column) = columns::find(&catalog, "project_name") else {
            return String::new();
        };
        let ix = self.grid_index();
        let name = |i: usize| {
            columns::display(
                &self.project,
                Some(i),
                &columns::value(&self.project, &ix, column, i),
            )
        };
        let mut names: Vec<String> = vec![];
        for c in changes {
            if let Some(i) = self
                .project
                .model
                .activities
                .iter()
                .position(|a| a.task_id == c.task_id)
            {
                let n = name(i);
                if !n.is_empty() && !names.contains(&n) {
                    names.push(n);
                }
            }
        }
        if names.is_empty() && !self.project.model.activities.is_empty() {
            names.push(name(0));
        }
        names.join(", ")
    }

    /// Write the report where Save Report says, after a commit.
    fn write_report_on_commit(&mut self, r: &GcReport) {
        let path = target_path(Path::new(r.file.path.trim()), r.file.overwrite);
        match write_gc_report(&path, r, r.file.html) {
            Ok(()) => self.status = format!("{}; report saved to {}", self.status, path.display()),
            Err(e) => self.error(e),
        }
    }

    pub(crate) fn gc_report_dialog(&mut self, ctx: &egui::Context, time: f64) {
        let Some(mut r) = self.editing.dialogs.gc_report.take() else {
            return;
        };
        let mut done = None;
        let mut save = false;
        let mut help = false;
        let mut browse = false;
        // 1040 x 520 px inside the frame: the header block, Select All and
        // Clear All, the lines, then Save Report and the buttons along the
        // bottom.
        const W: f32 = 1040.;
        const H: f32 = 520.;
        const LIST_TOP: f32 = 112.;
        const BOTTOM: f32 = H - 36.;
        let any = r.changes.iter().any(|c| c.apply && c.error.is_none());
        let ((), closed, actions) = kit::window(
            ctx,
            "global_change_report",
            "Global Change Report",
            Vec2::new(W, H),
            |c| {
                // Header block.
                c.fill(6., 6., W - 12., 66., kit::STRIP);
                for (k, (caption, value)) in r.header().iter().enumerate() {
                    let y = 12. + 18. * k as f32;
                    c.bold(20., y, 16., caption);
                    kit::text(c, 120., y + 2., value, true);
                }
                // Select All, Clear All.
                if c.button(
                    "gc_report_select_all",
                    6.,
                    80.,
                    108.,
                    25.,
                    "Select All",
                    false,
                    true,
                ) {
                    for ch in r.changes.iter_mut().filter(|ch| ch.error.is_none()) {
                        ch.apply = true;
                    }
                }
                if c.button(
                    "gc_report_clear_all",
                    119.,
                    80.,
                    108.,
                    25.,
                    "Clear All",
                    false,
                    true,
                ) {
                    for ch in r.changes.iter_mut() {
                        ch.apply = false;
                    }
                }
                let widths = [56., 90., 120., 100., 140., 120., 120., 270.];
                let cols: Vec<Col> = COLUMNS
                    .iter()
                    .zip(widths)
                    .map(|(&title, width)| Col { title, width })
                    .collect();
                let changes = &mut r.changes;
                kit::list(
                    c,
                    "gc_report_rows",
                    (6., LIST_TOP, W - 12., BOTTOM - LIST_TOP - 8.),
                    None,
                    &cols,
                    changes.len(),
                    None,
                    None,
                    &mut |c, k, col, rect, sel| {
                        let change = &mut changes[k];
                        let at = rect.min - c.origin.to_vec2();
                        match col {
                            0 => {
                                let ok = change.error.is_none();
                                let x = (rect.center().x - 6.5).floor() - c.origin.x;
                                let y = at.y + 2.;
                                let hit =
                                    c.hit(&format!("gc_report_check_{k}"), c.r(x, y, 13., 13.), ok);
                                if ok && hit.clicked() {
                                    change.apply = !change.apply;
                                }
                                kit::check_glyph(c, x, y, change.apply, ok);
                            }
                            7 => {
                                if let Some(error) = &change.error {
                                    kit::error_text(
                                        c,
                                        at.x + 3.,
                                        at.y + 3.,
                                        rect.width() - 6.,
                                        error,
                                    );
                                }
                            }
                            n => kit::cell_text(c, rect, GcReport::cells(change)[n - 1], sel, 3.),
                        }
                    },
                );
                // Save Report with its file, As HTML, Overwrite Existing.
                let y = BOTTOM;
                let f = &mut r.file;
                c.checkbox(
                    "gc_report_save",
                    8.,
                    y + 2.,
                    90.,
                    21.,
                    "Save Report",
                    &mut f.save,
                    true,
                );
                let on = f.save;
                c.edit(
                    "gc_report_path",
                    100.,
                    y + 2.,
                    262.,
                    21.,
                    &mut f.path,
                    on,
                    egui::Align::Min,
                );
                let edit = c.r(100., y + 2., 262., 21.);
                browse = c.ellipsis("gc_report_browse", edit, on).clicked() && on;
                let was_html = f.html;
                c.checkbox(
                    "gc_report_html",
                    374.,
                    y + 2.,
                    68.,
                    21.,
                    "As HTML",
                    &mut f.html,
                    on,
                );
                if f.html != was_html {
                    f.path = with_format_extension(&f.path, f.html);
                }
                c.checkbox(
                    "gc_report_overwrite",
                    446.,
                    y + 2.,
                    120.,
                    21.,
                    "Overwrite Existing",
                    &mut f.overwrite,
                    on,
                );
                let out = kit::button_row(
                    c,
                    W - 6.,
                    y,
                    &[
                        Entry::Button("gc_save_report", "Save to File...", true, false),
                        Entry::Button("gc_cancel_changes", "Cancel Changes", true, false),
                        Entry::Button("gc_commit", "Commit Changes", any, false),
                        Entry::help("gc_report_help"),
                    ],
                );
                match out.clicked.as_deref() {
                    Some("gc_commit") => done = Some(true),
                    Some("gc_cancel_changes") => done = Some(false),
                    Some("gc_save_report") => save = true,
                    Some(_) => help = true,
                    None => {}
                }
            },
        );
        self.record_dialog_actions(actions);
        if browse {
            let (name, filter) = if r.file.html {
                ("HTML", "html")
            } else {
                ("Text", "txt")
            };
            if let Some(path) = rfd::FileDialog::new()
                .add_filter(name, &[filter])
                .set_file_name(DEFAULT_FILE)
                .save_file()
            {
                r.file.path = path.display().to_string();
            }
        }
        if save {
            self.save_gc_report(&r);
        }
        if help {
            self.open_help_for("global_change_report");
        }
        match done {
            Some(true) => {
                self.commit_global_change(&r.changes, time);
                if r.file.save {
                    self.write_report_on_commit(&r);
                }
                let file = r.file.clone();
                self.save_gc_report_file(&file);
            }
            Some(false) => self.status = "Global change cancelled".into(),
            None if closed || self.escape_for(ctx, "gc_report") => {}
            None => self.editing.dialogs.gc_report = Some(r),
        }
    }

    fn save_gc_report(&mut self, r: &GcReport) {
        let (name, ext) = if r.file.html {
            ("HTML", "html")
        } else {
            ("Text", "txt")
        };
        let Some(path) = rfd::FileDialog::new()
            .add_filter(name, &[ext])
            .set_file_name(format!("{}.{ext}", r.name))
            .save_file()
        else {
            return;
        };
        if let Err(e) = write_gc_report(&path, r, r.file.html) {
            self.error(e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_html_switches_the_extension_and_a_kept_file_gets_a_number() {
        assert_eq!(
            with_format_extension("a/Report.txt", true),
            Path::new("a/Report.html").display().to_string()
        );
        assert_eq!(
            with_format_extension("a/Report.html", false),
            Path::new("a/Report.txt").display().to_string()
        );
        assert_eq!(with_format_extension("a/Report.csv", true), "a/Report.csv");
        assert_eq!(with_format_extension("", true), "");
        let dir = std::env::temp_dir().join(format!("openxer-gc-target-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("GlobalChangeReport.txt");
        assert_eq!(target_path(&path, false), path, "a new file keeps its name");
        std::fs::write(&path, "old").unwrap();
        assert_eq!(target_path(&path, true), path, "Overwrite Existing");
        assert_eq!(
            target_path(&path, false),
            dir.join("GlobalChangeReport1.txt")
        );
        std::fs::write(dir.join("GlobalChangeReport1.txt"), "old").unwrap();
        assert_eq!(
            target_path(&path, false),
            dir.join("GlobalChangeReport2.txt")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_default_report_file_is_in_a_test_folder_here() {
        let path = default_report_path();
        assert!(path.starts_with(std::env::temp_dir()), "{}", path.display());
        assert!(path.ends_with(DEFAULT_FILE));
        assert!(ReportFile::default().save && ReportFile::default().overwrite);
    }
}
