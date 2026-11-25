//! Report tabs: a report that has been run sits in a tab of the main view
//! area, in the shared document viewer. Where it goes (PDF, Excel, HTML,
//! text, CSV, the printer) is chosen afterwards, from the tab's toolbar.
//! Closing the tab frees the report; the tabs are not saved with the
//! layout.
use super::doc::{self, Report};
use super::groups;
use super::rpt::engine::Generated;
use super::rpt::writers::{self, TextOptions};
use super::rpt_entries::Which;
use super::wizard::Definition;
use crate::print::ops::{Op, Page};
use crate::print::viewer::Nav;
use crate::print::viewer_ui::ViewState;
use crate::views::MainView;
use crate::App;
use eframe::egui::Vec2;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Names one open report tab for as long as it is open.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ReportTabId(pub u32);

/// What a tab runs again on Refresh.
pub enum Source {
    /// A definition report in the workspace.
    Stored(i64),
    /// A wizard report: the definition it ran with.
    Wizard(Box<Definition>),
    /// A built-in report (S-Curve, Earned Hours): the options it ran with.
    Progress(super::progress_view::Spec),
}

/// The report document a tab holds, in the form its writers read.
pub enum Doc {
    Generated(Box<Generated>),
    Sheet(Box<Report>),
    /// A built-in report: its tables and chart, and the figures its
    /// workbook is written from.
    Progress(Box<super::progress_view::ProgressDoc>),
}

/// The files a report tab can write.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Export {
    Pdf,
    Excel,
    Html,
    Text,
    Csv,
}

impl Export {
    pub const ALL: [Export; 5] = [
        Export::Pdf,
        Export::Excel,
        Export::Html,
        Export::Text,
        Export::Csv,
    ];
    /// The item in the Export menu.
    pub fn caption(self) -> &'static str {
        match self {
            Export::Pdf => "PDF...",
            Export::Excel => "Excel (.xlsx)...",
            Export::Html => "HTML...",
            Export::Text => "Text (ASCII)...",
            Export::Csv => "CSV...",
        }
    }
    pub fn extension(self) -> &'static str {
        match self {
            Export::Pdf => "pdf",
            Export::Excel => "xlsx",
            Export::Html => "html",
            Export::Text => "txt",
            Export::Csv => "csv",
        }
    }
    /// The file type named in the Save As dialog.
    #[cfg_attr(test, allow(dead_code))]
    pub fn filter_name(self) -> &'static str {
        match self {
            Export::Pdf => "PDF",
            Export::Excel => "Excel workbook",
            Export::Html => "HTML",
            Export::Text => "Text",
            Export::Csv => "CSV",
        }
    }
    /// The action name of its Export menu item.
    pub fn key(self) -> &'static str {
        self.extension()
    }
}

pub struct ReportTab {
    pub id: ReportTabId,
    /// The report's name.
    pub name: String,
    /// The tab caption: the name, with " (2)" and so on for a report that is
    /// open more than once.
    pub caption: String,
    pub source: Source,
    pub notes: String,
    pub doc: Doc,
    pub pages: Vec<Page>,
    pub page: usize,
    pub zoom: Option<f32>,
    pub nav: Nav,
    pub view: ViewState,
    /// The project, its data date and the run time, for the header line.
    pub project: String,
    pub data_date: String,
    pub ran: String,
    pub warnings: Vec<String>,
    /// The last file written from this tab; the status line offers to open
    /// it or its folder.
    pub saved: Option<PathBuf>,
    /// The last thing done other than saving a file ("Copied ...").
    pub note: Option<String>,
    /// Does the report have rows to write as comma separated values?
    pub tabular: bool,
}

impl ReportTab {
    /// The size of the largest page; every page of a report has it.
    pub fn page_size(&self) -> Vec2 {
        self.pages
            .iter()
            .fold(Vec2::ZERO, |size, p| size.max(p.size))
    }

    /// Replaces the document and its pages, keeping the page, the zoom and
    /// the scroll.
    pub fn set_doc(&mut self, doc: Doc) {
        self.pages = match &doc {
            Doc::Generated(g) => writers::print_pages(g),
            Doc::Sheet(r) => super::pdf::pages(r),
            Doc::Progress(p) => p.pages(),
        };
        self.warnings = match &doc {
            Doc::Generated(g) => g.warnings.clone(),
            Doc::Sheet(_) => vec![],
            Doc::Progress(p) => p.warnings(),
        };
        self.tabular = match &doc {
            Doc::Generated(g) => !writers::text(g, &TextOptions::default()).is_empty(),
            Doc::Sheet(r) => r.tables().next().is_some(),
            Doc::Progress(p) => p.report.tables().next().is_some(),
        };
        self.doc = doc;
        self.page = self.page.min(self.pages.len().saturating_sub(1));
        self.view.find.invalidate();
    }

    /// The bytes of the report in `format`, from the same writers a run to a
    /// file uses.
    pub fn bytes(&self, format: Export, text: &TextOptions) -> Result<Vec<u8>, String> {
        let comma = TextOptions {
            delimiter: ",".into(),
            qualifier: "\"".into(),
        };
        Ok(match (&self.doc, format) {
            (Doc::Generated(g), Export::Pdf) => writers::pdf(g),
            (Doc::Generated(g), Export::Excel) => writers::excel(g)?,
            (Doc::Generated(g), Export::Html) => writers::html(g).into_bytes(),
            (Doc::Generated(g), Export::Text) => super::csv::file_bytes(writers::text(g, text)),
            (Doc::Generated(g), Export::Csv) => super::csv::file_bytes(writers::text(g, &comma)),
            (Doc::Sheet(r), Export::Pdf) => super::pdf::write(r),
            (Doc::Sheet(r), Export::Excel) => super::xlsx::bytes(r)?,
            (Doc::Sheet(r), Export::Html) => super::html::write(r).into_bytes(),
            (Doc::Sheet(r), Export::Text) => {
                super::csv::file_bytes(super::csv::write_with(r, &text.delimiter, &text.qualifier))
            }
            (Doc::Sheet(r), Export::Csv) => super::csv::file_bytes(super::csv::write(r)),
            // The workbook of the command line and MCP, and the pages shown.
            (Doc::Progress(p), Export::Excel) => p.workbook()?,
            (Doc::Progress(p), Export::Pdf) => {
                crate::print::pdf::write(&self.pages, &p.report.title)
            }
            (Doc::Progress(p), Export::Html) => super::html::write(&p.report).into_bytes(),
            (Doc::Progress(p), Export::Text) => super::csv::file_bytes(super::csv::write_with(
                &p.report,
                &text.delimiter,
                &text.qualifier,
            )),
            (Doc::Progress(p), Export::Csv) => super::csv::file_bytes(super::csv::write(&p.report)),
        })
    }

    /// The text of page `k`, line by line, the cells of a line separated by
    /// tabs so it pastes into a spreadsheet as columns.
    pub fn page_text(&self, k: usize) -> String {
        self.pages.get(k).map(page_text).unwrap_or_default()
    }

    /// The text of the whole report, page after page.
    pub fn report_text(&self) -> String {
        self.pages
            .iter()
            .map(page_text)
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// The text runs of a page in reading order: a line per row of text, the
/// runs of a line left to right.
fn page_text(page: &Page) -> String {
    let mut runs: Vec<(f32, f32, &str, f32)> = page
        .ops
        .iter()
        .filter_map(|c| match &c.op {
            Op::Text {
                pos, text, size, ..
            } if !text.trim().is_empty() => Some((pos.y, pos.x, text.as_str(), *size)),
            _ => None,
        })
        .collect();
    runs.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut lines: Vec<Vec<(f32, &str)>> = vec![];
    let mut line_y = f32::NEG_INFINITY;
    for (y, x, text, size) in runs {
        if lines.is_empty() || y - line_y > size * 0.5 {
            lines.push(vec![]);
            line_y = y;
        }
        if let Some(line) = lines.last_mut() {
            line.push((x, text));
        }
    }
    let mut out = String::new();
    for mut line in lines {
        line.sort_by(|a, b| a.0.total_cmp(&b.0));
        let cells: Vec<&str> = line.iter().map(|(_, t)| t.trim_end()).collect();
        out.push_str(&cells.join("\t"));
        out.push('\n');
    }
    out
}

/// The open report tabs.
#[derive(Default)]
pub struct ReportTabs {
    tabs: BTreeMap<ReportTabId, ReportTab>,
    next: u32,
}

impl ReportTabs {
    pub fn get(&self, id: ReportTabId) -> Option<&ReportTab> {
        self.tabs.get(&id)
    }
    pub fn get_mut(&mut self, id: ReportTabId) -> Option<&mut ReportTab> {
        self.tabs.get_mut(&id)
    }
    /// Takes a tab out while it is drawn; `put` returns it.
    pub fn take(&mut self, id: ReportTabId) -> Option<ReportTab> {
        self.tabs.remove(&id)
    }
    pub fn put(&mut self, tab: ReportTab) {
        self.tabs.insert(tab.id, tab);
    }
    pub fn ids(&self) -> Vec<ReportTabId> {
        self.tabs.keys().copied().collect()
    }
    pub fn fresh_id(&mut self) -> ReportTabId {
        self.next += 1;
        ReportTabId(self.next)
    }
    /// `name`, or `name (2)`, `name (3)`... when that caption is taken.
    pub fn free_caption(&self, name: &str) -> String {
        let taken = |c: &str| self.tabs.values().any(|t| t.caption == c);
        if !taken(name) {
            return name.to_string();
        }
        (2..)
            .map(|n| format!("{name} ({n})"))
            .find(|c| !taken(c))
            .unwrap_or_else(|| name.to_string())
    }
}

/// The folder the last Save As used.
const EXPORT_DIR_KEY: &str = "report_tab_folder_v1";

/// The file name Save As offers: the report, the project and the day.
pub fn default_file_name(name: &str, project: &str, day: &str, format: Export) -> String {
    let stem = groups::stem_of(&format!("{name} - {project} - {day}"));
    format!("{stem}.{}", format.extension())
}

#[cfg(test)]
pub(crate) fn pick_save_path(_format: Export, name: &str, dir: &Path) -> Option<PathBuf> {
    seam::OFFERED.with(|o| o.borrow_mut().push((dir.to_path_buf(), name.to_string())));
    seam::ANSWERS.with(|a| a.borrow_mut().pop_front().flatten())
}

impl App {
    /// Builds the document of a report again on the open project.
    fn build_tab_doc(&mut self, source: &Source, notes: &str) -> Result<Doc, String> {
        match source {
            Source::Stored(id) => Ok(Doc::Generated(Box::new(self.generate_stored(*id, notes)?))),
            Source::Wizard(def) => Ok(Doc::Sheet(Box::new(self.wizard_report(def, notes)))),
            Source::Progress(spec) => Ok(Doc::Progress(Box::new(self.progress_doc(*spec)?))),
        }
    }

    /// A wizard report with the user's notes under its header.
    pub(crate) fn wizard_report(&self, def: &Definition, notes: &str) -> Report {
        let mut report = self.build_report(def);
        if !notes.trim().is_empty() {
            report.header.push(format!("Notes: {}", notes.trim()));
        }
        report
    }

    /// Runs a saved report into a tab (Run Report in the Reports view). A
    /// project report runs only on its own project. The run is recorded.
    pub(crate) fn run_report_in_tab(
        &mut self,
        which: &Which,
        notes: &str,
    ) -> Result<ReportTabId, String> {
        let entry = self
            .report_entries()
            .into_iter()
            .find(|e| &e.which == which)
            .ok_or("That report is no longer in the list.")?;
        if !self.can_run_entry(&entry) {
            return Err(self.scope_refusal(&entry));
        }
        let source = match which {
            Which::Stored(id) => Source::Stored(*id),
            Which::Wizard(i) => Source::Wizard(Box::new(
                self.report_definitions()
                    .get(*i)
                    .cloned()
                    .ok_or("That report is no longer in the list.")?,
            )),
            Which::Builtin(_) => return Err("That report runs from its own dialog.".into()),
        };
        let doc = self.build_tab_doc(&source, notes)?;
        self.mark_run(which);
        Ok(self.open_report_tab(entry.name, source, notes, doc))
    }

    /// Runs a wizard definition into a tab (the wizard's Run and Finish).
    pub(crate) fn run_definition_in_tab(&mut self, def: &Definition) -> ReportTabId {
        let name = [&def.name, &def.title]
            .into_iter()
            .find(|n| !n.trim().is_empty())
            .cloned()
            .unwrap_or_else(|| "Report".into());
        let doc = Doc::Sheet(Box::new(self.wizard_report(def, "")));
        self.open_report_tab(name, Source::Wizard(Box::new(def.clone())), "", doc)
    }

    /// Opens a tab on a finished report and shows it.
    pub(crate) fn open_report_tab(
        &mut self,
        name: String,
        source: Source,
        notes: &str,
        doc: Doc,
    ) -> ReportTabId {
        let id = self.reports.tabs.fresh_id();
        let caption = self.reports.tabs.free_caption(&name);
        let (zoom, view) = self.viewer_start();
        let mut tab = ReportTab {
            id,
            name,
            caption,
            source,
            notes: notes.to_string(),
            doc: Doc::Sheet(Box::default()),
            pages: vec![],
            page: 0,
            zoom,
            nav: Nav::default(),
            view,
            project: String::new(),
            data_date: String::new(),
            ran: String::new(),
            warnings: vec![],
            saved: None,
            note: None,
            tabular: false,
        };
        tab.set_doc(doc);
        self.stamp_report_tab(&mut tab);
        if !tab.warnings.is_empty() {
            self.status = format!("Report run with warnings: {}", tab.warnings.join("; "));
        }
        self.reports.tabs.put(tab);
        self.show_view(MainView::Report(id));
        id
    }

    /// The project, data date and time of a run, for the tab's header line.
    fn stamp_report_tab(&self, tab: &mut ReportTab) {
        let project = &self.project.model.project;
        let title = crate::views::project_title(&self.project);
        tab.project = if title.is_empty() || title == project.name {
            project.name.clone()
        } else {
            format!("{} - {title}", project.name)
        };
        tab.data_date = crate::format::date(self.project.model.origin_serial, project.data_date);
        tab.ran = doc::run_stamp();
    }

    /// Refresh: runs the report again on the project as it is now, in the
    /// same tab, keeping the page and the zoom.
    pub(crate) fn refresh_report_tab(&mut self, id: ReportTabId) {
        let Some(mut tab) = self.reports.tabs.take(id) else {
            return;
        };
        match self.build_tab_doc(&tab.source, &tab.notes) {
            Ok(doc) => {
                tab.set_doc(doc);
                self.stamp_report_tab(&mut tab);
                tab.note = Some("Refreshed".into());
                tab.saved = None;
                match &tab.source {
                    Source::Stored(sid) => self.mark_run(&Which::Stored(*sid)),
                    Source::Wizard(def) => {
                        let at = self
                            .report_definitions()
                            .iter()
                            .position(|d| d.name == def.name && !def.name.is_empty());
                        if let Some(i) = at {
                            self.mark_run(&Which::Wizard(i));
                        }
                    }
                    Source::Progress(spec) => self.mark_run(&Which::Builtin(spec.builtin())),
                }
                if !tab.warnings.is_empty() {
                    self.status = format!("Report run with warnings: {}", tab.warnings.join("; "));
                }
            }
            Err(e) => self.error(e),
        }
        self.reports.tabs.put(tab);
    }

    /// Frees the report of a tab that has been closed, and remembers its
    /// zoom for the next viewer.
    pub(crate) fn free_report_tab(&mut self, id: ReportTabId) {
        if let Some(tab) = self.reports.tabs.take(id) {
            self.save_viewer_prefs(tab.zoom, &tab.view);
        }
    }

    /// The report tab being shown, if the focused tab group shows one.
    pub(crate) fn current_report_tab(&self) -> Option<ReportTabId> {
        match self.views.groups.current() {
            MainView::Report(id) => Some(id),
            _ => None,
        }
    }

    /// Closes every report tab (the project they were run on is gone).
    pub(crate) fn close_report_tabs(&mut self) {
        for id in self.reports.tabs.ids() {
            self.close_tab(MainView::Report(id));
        }
    }

    /// The folder Save As opens in: the last one used.
    pub(crate) fn export_folder(&self) -> PathBuf {
        self.store
            .preference(EXPORT_DIR_KEY)
            .ok()
            .flatten()
            .map(PathBuf::from)
            .filter(|p| p.is_dir())
            .unwrap_or_else(doc::documents)
    }

    /// Export > a format: asks where to save (text asks for its delimiter
    /// and qualifier first).
    pub(crate) fn export_report_tab(&mut self, id: ReportTabId, format: Export) {
        if format == Export::Text {
            self.reports.text_export = Some(TextExport {
                id,
                delimiter: self.reports.text_choice.0,
                qualifier: self.reports.text_choice.1,
            });
        } else {
            self.save_report_tab(id, format, &TextOptions::default());
        }
    }

    /// Asks where to save the report of a tab and writes it.
    pub(crate) fn save_report_tab(&mut self, id: ReportTabId, format: Export, text: &TextOptions) {
        let Some(tab) = self.reports.tabs.get(id) else {
            return;
        };
        let day = chrono::Local::now().format("%Y-%m-%d").to_string();
        let name = match (&tab.doc, format) {
            // A built-in report's workbook keeps the name its export had.
            (Doc::Progress(_), Export::Excel) => self.report_file_name(&tab.name),
            _ => default_file_name(&tab.name, &self.project.model.project.name, &day, format),
        };
        let Some(path) = pick_save_path(format, &name, &self.export_folder()) else {
            return;
        };
        if let Err(e) = self.write_report_tab(id, format, text, &path) {
            self.error(e);
        }
    }

    /// Remembers the folder of a file just saved, where Save As opens next.
    pub(crate) fn remember_export_folder(&self, path: &Path) {
        if let Some(dir) = path.parent().filter(|d| d.is_dir()) {
            let _ = self
                .store
                .set_preference(EXPORT_DIR_KEY, &dir.display().to_string());
        }
    }

    /// Writes the report of a tab to `path`, remembers the folder and notes
    /// the file on the tab's status line.
    pub(crate) fn write_report_tab(
        &mut self,
        id: ReportTabId,
        format: Export,
        text: &TextOptions,
        path: &Path,
    ) -> Result<(), String> {
        let tab = self
            .reports
            .tabs
            .get(id)
            .ok_or("That report tab is closed.")?;
        let bytes = tab.bytes(format, text)?;
        super::rpt_run::write_file(path, &bytes)?;
        self.remember_export_folder(path);
        if let Some(tab) = self.reports.tabs.get_mut(id) {
            tab.saved = Some(path.to_path_buf());
            tab.note = None;
        }
        self.status = format!("Report saved to {}", path.display());
        Ok(())
    }

    /// Print: the report as a PDF in the viewer that prints it.
    pub(crate) fn print_report_tab(&mut self, id: ReportTabId) {
        let Some(tab) = self.reports.tabs.get(id) else {
            return;
        };
        let count = tab.pages.len();
        let sent = tab
            .bytes(Export::Pdf, &TextOptions::default())
            .and_then(|bytes| crate::print::send_pdf_to_viewer(&bytes));
        match sent {
            Ok(path) => {
                self.status = format!(
                    "{count} page(s) sent to the PDF viewer for printing: {}",
                    path.display()
                );
            }
            Err(e) => self.error(e),
        }
    }

    /// Copy: the text of the selected page (the page highlighted in the
    /// page strip), or of the whole report when no page is selected.
    pub(crate) fn copy_report_tab(&mut self, ctx: &eframe::egui::Context, id: ReportTabId) {
        let Some(tab) = self.reports.tabs.get_mut(id) else {
            return;
        };
        let (text, note) = if tab.view.thumbs {
            (
                tab.page_text(tab.page),
                format!("Copied the text of page {}", tab.page + 1),
            )
        } else {
            (tab.report_text(), "Copied the text of the report".into())
        };
        ctx.copy_text(text);
        tab.note = Some(note);
    }

    /// Open: the file last written from the tab, or its folder.
    pub(crate) fn open_saved_report(&mut self, id: ReportTabId, folder: bool) {
        let Some(path) = self.reports.tabs.get(id).and_then(|t| t.saved.clone()) else {
            return;
        };
        let target = if folder {
            path.parent().map(Path::to_path_buf).unwrap_or(path)
        } else {
            path
        };
        if let Err(e) = crate::util::open_in_default_app(&target) {
            self.error(e);
        }
    }

    /// The right-click menu of a report tab.
    pub(crate) fn report_tab_menu(&mut self, m: &mut crate::context_menu::Menu, view: MainView) {
        use crate::chrome::Item;
        use crate::views::tab_groups::Split;
        if m.item("close", Item::new("Close")) {
            self.request_close(view);
        }
        let others = self
            .reports
            .tabs
            .ids()
            .into_iter()
            .any(|id| MainView::Report(id) != view);
        if m.item(
            "close_others",
            Item::new("Close Other Report Tabs").enabled(others),
        ) {
            for id in self.reports.tabs.ids() {
                if MainView::Report(id) != view {
                    self.request_close(MainView::Report(id));
                }
            }
        }
        if m.item("close_all", Item::new("Close All Report Tabs")) {
            for id in self.reports.tabs.ids() {
                self.request_close(MainView::Report(id));
            }
        }
        m.separator();
        let groups = &self.views.groups;
        let (horizontal, vertical, merge) = (
            groups.can_split(Split::Horizontal),
            groups.can_split(Split::Vertical),
            groups.can_merge(),
        );
        if m.item(
            "horizontal",
            Item::new("New Horizontal Tab Group").enabled(horizontal),
        ) {
            self.show_view(view);
            self.new_tab_group(Split::Horizontal);
        }
        if m.item(
            "vertical",
            Item::new("New Vertical Tab Group").enabled(vertical),
        ) {
            self.show_view(view);
            self.new_tab_group(Split::Vertical);
        }
        if m.item("merge", Item::new("Merge All Tab Groups").enabled(merge)) {
            self.merge_tab_groups();
        }
    }
}

/// The Text export's delimiter and qualifier, asked before Save As.
pub struct TextExport {
    pub id: ReportTabId,
    pub delimiter: usize,
    pub qualifier: usize,
}

#[cfg(not(test))]
pub(crate) fn pick_save_path(format: Export, name: &str, dir: &Path) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter(format.filter_name(), &[format.extension()])
        .set_directory(dir)
        .set_file_name(name)
        .save_file()
}

/// Tests answer Save As from a queue and note what was offered.
#[cfg(test)]
pub(crate) mod seam {
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::path::PathBuf;

    thread_local! {
        pub static ANSWERS: RefCell<VecDeque<Option<PathBuf>>> = const { RefCell::new(VecDeque::new()) };
        pub static OFFERED: RefCell<Vec<(PathBuf, String)>> = const { RefCell::new(Vec::new()) };
    }
    pub fn answer(path: Option<PathBuf>) {
        ANSWERS.with(|a| a.borrow_mut().push_back(path));
    }
    pub fn offered() -> Vec<(PathBuf, String)> {
        OFFERED.with(|o| std::mem::take(&mut *o.borrow_mut()))
    }
}
