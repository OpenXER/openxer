//! Running a definition report to a file (the Run Report to File dialog:
//! an HTML file, a text file or a PDF file) and the page preview of the
//! report editor. A report run from the Reports view goes to a tab
//! instead (see `report_tab`).
use super::doc::documents;
use super::groups;
use super::rpt::engine::{Generated, Options};
use super::rpt::store;
use super::rpt::stream::Stream;
use super::rpt::writers::{self, TextOptions, DELIMITERS, QUALIFIERS};
use super::rpt_entries::Which;
use crate::dialog_kit as kit;
use crate::print::ops::Page;
use crate::App;
use eframe::egui::{self, Align, Vec2};
use std::path::{Path, PathBuf};

/// The file a report is sent to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Html,
    Text,
    Pdf,
}

impl Target {
    pub const ALL: [Target; 3] = [Target::Html, Target::Text, Target::Pdf];
    pub fn caption(self) -> &'static str {
        match self {
            Target::Html => "&HTML File",
            Target::Text => "&ASCII Text File",
            Target::Pdf => "P&DF File",
        }
    }
    pub fn extension(self) -> &'static str {
        match self {
            Target::Html => "html",
            Target::Text => "txt",
            Target::Pdf => "pdf",
        }
    }
}

pub struct RunDialog {
    pub which: Which,
    pub name: String,
    pub target: Target,
    pub path: String,
    /// The path was typed or picked, so a change of target keeps its folder
    /// and name.
    pub delimiter: usize,
    pub qualifier: usize,
    pub open_after: bool,
    pub notes: String,
    pub error: Option<String>,
}

/// The pages of a report on screen.
pub struct PagePreview {
    pub title: String,
    pub pages: Vec<Page>,
    pub page: usize,
    /// Screen pixels per page pixel; `None` fits the page to the window.
    pub zoom: Option<f32>,
    /// Where the pages are scrolled to.
    pub nav: crate::print::viewer::Nav,
    /// Fit mode, page strip and Find.
    pub view: crate::print::viewer_ui::ViewState,
    pub note: Option<String>,
    pub warnings: Vec<String>,
    /// The report as an HTML file and as a text file, for Export.
    pub html: String,
    pub text: String,
}

impl PagePreview {
    /// A preview of a generated report, opened as the last viewer left its
    /// zoom and strip (`start`).
    pub fn new(g: &Generated, start: (Option<f32>, crate::print::viewer_ui::ViewState)) -> Self {
        Self::from_pages(
            g.title.clone(),
            writers::print_pages(g),
            g.warnings.clone(),
            (writers::html(g), writers::text(g, &TextOptions::default())),
            start,
        )
    }
    pub fn from_pages(
        title: String,
        pages: Vec<Page>,
        warnings: Vec<String>,
        (html, text): (String, String),
        (zoom, view): (Option<f32>, crate::print::viewer_ui::ViewState),
    ) -> Self {
        Self {
            title,
            pages,
            page: 0,
            zoom,
            nav: Default::default(),
            view,
            note: None,
            warnings,
            html,
            text,
        }
    }
}

fn default_path(name: &str, target: Target) -> String {
    let stem = groups::stem_of(name);
    documents()
        .join(format!("{stem}.{}", target.extension()))
        .display()
        .to_string()
}

/// Writes `bytes` whole or not at all: through a temporary file of its own
/// (flushed to disk) renamed over `path`, so a failed write leaves the file
/// that was there and no partial temporary file.
pub(crate) fn write_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    openxer_xer::write_file_atomic(path, bytes)
        .map_err(|e| format!("{} could not be written: {e}", path.display()))
}

impl App {
    /// The options of a run now: the clock, the user and their notes.
    pub(crate) fn run_options(&self, notes: &str) -> Options {
        let mut o = Options::new(chrono::Local::now().naive_local());
        o.user = super::doc::user_name();
        o.notes = notes.to_string();
        o
    }

    /// Generates a stored report on the open project.
    pub(crate) fn generate_stored(&self, id: i64, notes: &str) -> Result<Generated, String> {
        let bytes = store::load(self.store.connection(), id)?;
        let stream =
            Stream::parse_bytes(&bytes).map_err(|e| format!("This report cannot be run: {e}"))?;
        let ix = self.grid_index();
        super::rpt::generate(&stream, &self.project, &ix, &self.run_options(notes))
    }

    /// Writes a stored report in a batch format: HTML, PDF or comma
    /// separated text.
    pub(crate) fn write_stored_as(
        &self,
        id: i64,
        format: super::doc::Output,
        path: &Path,
        notes: &str,
    ) -> Result<(), String> {
        let g = self.generate_stored(id, notes)?;
        let bytes = match format {
            super::doc::Output::Html => writers::html(&g).into_bytes(),
            super::doc::Output::Pdf => writers::pdf(&g),
            super::doc::Output::Csv => super::csv::file_bytes(writers::text(
                &g,
                &TextOptions {
                    delimiter: ",".into(),
                    qualifier: "\"".into(),
                },
            )),
            super::doc::Output::Excel => writers::excel(&g)?,
        };
        write_file(path, &bytes)
    }

    /// Runs a stored report to a file.
    pub(crate) fn run_stored_report(
        &mut self,
        id: i64,
        target: Target,
        path: &Path,
        text: &TextOptions,
        notes: &str,
    ) -> Result<PathBuf, String> {
        let g = self.generate_stored(id, notes)?;
        let bytes = match target {
            Target::Html => writers::html(&g).into_bytes(),
            Target::Text => super::csv::file_bytes(writers::text(&g, text)),
            Target::Pdf => writers::pdf(&g),
        };
        if path.as_os_str().is_empty() {
            return Err("Choose a file for the report.".into());
        }
        write_file(path, &bytes)?;
        self.status = if g.warnings.is_empty() {
            format!("Report saved to {}", path.display())
        } else {
            format!(
                "Report saved to {} with warnings: {}",
                path.display(),
                g.warnings.join("; ")
            )
        };
        Ok(path.to_path_buf())
    }

    /// Runs a saved report (wizard or definition) to a file on the open
    /// project and records the run. A project report runs only on its own
    /// project.
    pub(crate) fn run_report_entry(
        &mut self,
        which: &Which,
        target: Target,
        path: &Path,
        text: &TextOptions,
        notes: &str,
    ) -> Result<PathBuf, String> {
        let entry = self
            .report_entries()
            .into_iter()
            .find(|e| &e.which == which)
            .ok_or("That report is no longer in the list.")?;
        if !self.can_run_entry(&entry) {
            return Err(self.scope_refusal(&entry));
        }
        let outcome = match which {
            Which::Stored(id) => self.run_stored_report(*id, target, path, text, notes)?,
            Which::Wizard(i) => self.run_wizard_report(*i, target, path, text, notes)?,
            Which::Builtin(_) => return Err("That report runs from its own dialog.".into()),
        };
        self.mark_run(which);
        Ok(outcome)
    }

    /// Runs a wizard report to a file.
    fn run_wizard_report(
        &mut self,
        index: usize,
        target: Target,
        path: &Path,
        text: &TextOptions,
        notes: &str,
    ) -> Result<PathBuf, String> {
        let def = self
            .report_definitions()
            .get(index)
            .cloned()
            .ok_or("That report is no longer in the list.")?;
        let report = self.wizard_report(&def, notes);
        let bytes = match target {
            Target::Html => super::html::write(&report).into_bytes(),
            Target::Text => super::csv::file_bytes(super::csv::write_with(
                &report,
                &text.delimiter,
                &text.qualifier,
            )),
            Target::Pdf => super::pdf::write(&report),
        };
        if path.as_os_str().is_empty() {
            return Err("Choose a file for the report.".into());
        }
        write_file(path, &bytes)?;
        self.status = format!("Report saved to {}", path.display());
        Ok(path.to_path_buf())
    }

    /// Opens the Run Report to File dialog.
    pub(crate) fn open_run_report(&mut self, which: Which, name: &str) {
        self.reports.run = Some(RunDialog {
            which,
            name: name.to_string(),
            target: Target::Html,
            path: default_path(name, Target::Html),
            delimiter: 0,
            qualifier: 0,
            open_after: false,
            notes: String::new(),
            error: None,
        });
    }

    /// Run Report to File.
    pub(crate) fn run_report_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.reports.run.take() else {
            return;
        };
        if self.reports.take_escape() {
            return;
        }
        // The fields end left of the button strip.
        const W: f32 = 600.;
        const H: f32 = 328.;
        #[derive(Default)]
        struct Clicks {
            cancel: bool,
            run: bool,
            browse: bool,
            help: bool,
            target: Option<Target>,
        }
        let delimiters: Vec<(&str, bool)> = DELIMITERS.iter().map(|(c, _)| (*c, true)).collect();
        let qualifiers: Vec<(&str, bool)> = QUALIFIERS.iter().map(|(c, _)| (*c, true)).collect();
        let (k, closed, actions) = kit::window(
            ctx,
            "run_report",
            "Run Report to File",
            Vec2::new(W, H),
            |c| {
                let mut k = Clicks::default();
                c.fill(0., 0., W, H, kit::FACE);
                kit::text(c, 14., 12., &format!("Report: {}", d.name), true);
                kit::section(c, 12., 36., 260., 96., "Send Report To");
                for (i, t) in Target::ALL.iter().enumerate() {
                    if kit::radio(
                        c,
                        &format!("run_report_target_{i}"),
                        24.,
                        60. + 24. * i as f32,
                        236.,
                        20.,
                        t.caption(),
                        d.target == *t,
                        true,
                    ) {
                        k.target = Some(*t);
                    }
                }
                kit::text(c, 14., 144., "Output File", true);
                c.edit(
                    "run_report_path",
                    14.,
                    160.,
                    W - kit::STRIP_WIDTH - 22.,
                    21.,
                    &mut d.path,
                    true,
                    Align::Min,
                );
                let field = c.r(14., 160., W - kit::STRIP_WIDTH - 22., 21.);
                k.browse = c.ellipsis("run_report_browse", field, true).clicked();
                kit::text(c, 14., 192., "Field Delimiter", d.target == Target::Text);
                c.combo(
                    "run_report_delimiter",
                    14.,
                    208.,
                    150.,
                    21.,
                    &delimiters,
                    &mut d.delimiter,
                    d.target == Target::Text,
                );
                kit::text(c, 184., 192., "Text Qualifier", d.target == Target::Text);
                c.combo(
                    "run_report_qualifier",
                    184.,
                    208.,
                    150.,
                    21.,
                    &qualifiers,
                    &mut d.qualifier,
                    d.target == Target::Text,
                );
                c.checkbox(
                    "run_report_open",
                    14.,
                    238.,
                    300.,
                    18.,
                    "View file when done",
                    &mut d.open_after,
                    true,
                );
                kit::text(c, 14., 262., "User's Notes", true);
                c.edit(
                    "run_report_notes",
                    14.,
                    278.,
                    290.,
                    21.,
                    &mut d.notes,
                    true,
                    Align::Min,
                );
                if let Some(e) = &d.error {
                    kit::text(c, 14., 306., e, true);
                }
                kit::strip(c, W, H);
                let col = kit::button_column(
                    c,
                    W,
                    &[
                        kit::Entry::Button("run_report_run", "Run", true, false),
                        kit::Entry::cancel("run_report_cancel"),
                        kit::Entry::help("run_report_help"),
                    ],
                );
                match col.clicked.as_deref() {
                    Some("run_report_run") => k.run = true,
                    Some("run_report_cancel") => k.cancel = true,
                    Some("run_report_help") => k.help = true,
                    _ => {}
                }
                k
            },
        );
        self.record_dialog_actions(actions);
        if k.cancel || closed {
            return;
        }
        if let Some(t) = k.target {
            if t != d.target {
                // Keep the folder and name, change the extension.
                let p = Path::new(&d.path);
                d.path = if d.path.trim().is_empty() || p.extension().is_none() {
                    d.path.clone()
                } else {
                    p.with_extension(t.extension()).display().to_string()
                };
                d.target = t;
            }
        }
        if k.browse {
            let mut dialog = rfd::FileDialog::new()
                .add_filter(d.target.caption().replace('&', ""), &[d.target.extension()]);
            if let Some(parent) = Path::new(&d.path).parent().filter(|p| p.is_dir()) {
                dialog = dialog.set_directory(parent);
            }
            if let Some(f) = dialog.save_file() {
                d.path = f.display().to_string();
            }
        }
        if k.help {
            self.open_help_for("run_report");
        }
        if k.run {
            let text = TextOptions {
                delimiter: DELIMITERS[d.delimiter.min(DELIMITERS.len() - 1)]
                    .1
                    .to_string(),
                qualifier: QUALIFIERS[d.qualifier.min(QUALIFIERS.len() - 1)]
                    .1
                    .to_string(),
            };
            let path = PathBuf::from(d.path.trim());
            match self.run_report_entry(&d.which, d.target, &path, &text, &d.notes) {
                Ok(path) => {
                    if d.open_after {
                        if let Err(e) = crate::util::open_in_default_app(&path) {
                            self.error(e);
                        }
                    }
                    return;
                }
                Err(e) => d.error = Some(e),
            }
        }
        self.reports.run = Some(d);
    }

    /// Writes the HTML or the text of the report in the preview to `path`.
    pub(crate) fn export_report_preview(&mut self, p: &mut PagePreview, html: bool, path: &Path) {
        let bytes = if html {
            p.html.clone().into_bytes()
        } else {
            super::csv::file_bytes(p.text.clone())
        };
        match write_file(path, &bytes) {
            Ok(()) => p.note = Some(format!("Saved {}", path.display())),
            Err(e) => self.error(e),
        }
    }

    /// The print preview of a report's pages.
    pub(crate) fn report_page_preview(&mut self, ctx: &egui::Context) {
        use crate::print::viewer_ui::{self, text_button};
        let Some(mut p) = self.reports.page_preview.take() else {
            return;
        };
        if self.reports.take_escape() {
            // Find closes first, then the preview.
            if p.view.find.open {
                p.view.find.open = false;
                self.reports.page_preview = Some(p);
            } else {
                self.save_viewer_prefs(p.zoom, &p.view);
            }
            return;
        }
        // 1180 x 720 where it fits; smaller screens get a smaller window
        // (the pages scroll and zoom inside it).
        let screen = ctx.content_rect().size();
        let w_max = (screen.x - 16.).max(480.);
        let h_max = (screen.y - 56.).max(320.);
        let (w, h) = (1180_f32.min(w_max), 720_f32.min(h_max));
        const STATUS: f32 = 22.;
        #[derive(Default)]
        struct Clicks {
            close: bool,
            save: bool,
            html: bool,
            text: bool,
            save_prefs: bool,
        }
        let count = p.pages.len();
        // Every page of a report has the size of the largest.
        let page_size = p
            .pages
            .iter()
            .fold(Vec2::ZERO, |size, pg| size.max(pg.size));
        let PagePreview {
            pages,
            page,
            zoom,
            nav,
            view,
            warnings,
            note,
            ..
        } = &mut p;
        let (k, closed, actions) = kit::window(
            ctx,
            "report_preview_pages",
            "Report Preview",
            Vec2::new(w, h),
            |c| {
                let mut k = Clicks::default();
                c.fill(0., 0., w, h, kit::FACE);
                let area = c.r(0., 0., w, h - STATUS);
                crate::print::viewer::cover(c.ui.ctx(), c.r(0., 0., w, h));
                let mut host = |ui: &mut egui::Ui, acts: &mut Vec<(String, eframe::egui::Rect)>| {
                    k.save |= text_button(
                        ui,
                        acts,
                        "report_preview_save",
                        "Save PDF...",
                        count > 0,
                        false,
                    );
                    k.html |= text_button(
                        ui,
                        acts,
                        "report_preview_html",
                        "Export HTML...",
                        count > 0,
                        false,
                    );
                    k.text |= text_button(
                        ui,
                        acts,
                        "report_preview_text",
                        "Export Text...",
                        count > 0,
                        false,
                    );
                };
                let frame = viewer_ui::begin(
                    &mut *c.ui,
                    viewer_ui::Chrome {
                        names: viewer_ui::REPORT,
                        area,
                        page_size,
                        count,
                        across: 1,
                        page: &mut *page,
                        zoom: &mut *zoom,
                        nav: &mut *nav,
                        view: &mut *view,
                        active: true,
                    },
                    &mut host,
                );
                k.close = frame.close;
                k.save_prefs = frame.save_prefs;
                for (name, rect) in &frame.actions {
                    c.act(name, *rect);
                }
                for &(i, origin) in &frame.cells {
                    crate::print::preview::paint_page(
                        &frame.painter,
                        &pages[i],
                        origin,
                        frame.zoom,
                    );
                    viewer_ui::paint_matches(&frame.painter, &view.find, i, origin, frame.zoom);
                }
                for &(i, rect) in &frame.thumbs {
                    viewer_ui::paint_thumb(&frame.thumb_painter, rect, pages.get(i));
                }
                // Every page is built, so the search is done at once.
                while let Some(i) = view.find.next_scan(count) {
                    view.find.scan(i, &pages[i]);
                }
                let status = format!(
                    "Page {} of {}    Zoom {:.0}%{}{}",
                    *page + 1,
                    count.max(1),
                    frame.zoom * 100.,
                    note.as_ref().map_or(String::new(), |n| format!("    {n}")),
                    if warnings.is_empty() {
                        String::new()
                    } else {
                        format!("    {} warning(s): {}", warnings.len(), warnings.join("; "))
                    }
                );
                kit::text(c, 8., h - 19., &status, true);
                k
            },
        );
        self.record_dialog_actions(actions);
        if k.close || closed {
            self.save_viewer_prefs(p.zoom, &p.view);
            return;
        }
        if k.save_prefs {
            self.save_viewer_prefs(p.zoom, &p.view);
            p.view.take_dirty();
        }
        if k.save {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("PDF", &["pdf"])
                .set_file_name(format!("{}.pdf", groups::stem_of(&p.title)))
                .save_file()
            {
                let bytes = crate::print::pdf::write(&p.pages, &p.title);
                match write_file(&path, &bytes) {
                    Ok(()) => p.note = Some(format!("Saved {}", path.display())),
                    Err(e) => self.error(e),
                }
            }
        }
        for (wanted, html) in [(k.html, true), (k.text, false)] {
            if !wanted {
                continue;
            }
            let (name, ext) = if html {
                ("HTML", "html")
            } else {
                ("Text", "txt")
            };
            if let Some(path) = rfd::FileDialog::new()
                .add_filter(name, &[ext])
                .set_file_name(format!("{}.{ext}", groups::stem_of(&p.title)))
                .save_file()
            {
                self.export_report_preview(&mut p, html, &path);
            }
        }
        self.reports.page_preview = Some(p);
    }
}
