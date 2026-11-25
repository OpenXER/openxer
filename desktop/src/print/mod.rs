//! Printing: Page Setup, Print Preview, PDF output and the `export-pdf`
//! command. Pages are drawn by the production table, Gantt and network
//! painters (see `compose`), so the printout matches the screen.
//!
//! File > Print writes the pages to a PDF in the temp folder and opens it in
//! the system's default viewer, which prints with the platform dialog; the
//! app itself never talks to a printer.
use super::*;
pub mod cli;
pub mod compose;
mod dialog;
pub mod ops;
mod other_views;
pub mod paginate;
pub mod pdf;
pub(crate) mod preview;
#[cfg(test)]
mod screen_tests;
#[cfg(test)]
mod scroll_tests;
pub mod setup;
#[cfg(test)]
mod speed_tests;
#[cfg(test)]
mod tests;
mod ts_picker;
#[cfg(test)]
mod ts_picker_tests;
pub(crate) mod viewer;
pub(crate) mod viewer_ui;

pub use compose::Printed;
pub use setup::PageSetup;

/// Print state kept on the app.
#[derive(Default)]
pub struct PrintState {
    pub dialog: Option<dialog::SetupDialog>,
    pub preview: Option<preview::Preview>,
    /// The painted canvas of the last job and what it was painted for, kept
    /// while a preview or Page Setup is open so a new setup that does not
    /// change the canvas (header, footer, margins, paper) reuses it.
    canvas: Option<(compose::CanvasKey, std::rc::Rc<ops::Indexed>)>,
    /// Keep and reuse the canvas (a preview or Page Setup is open).
    keep_canvas: bool,
    /// Canvases painted so far (tests count them).
    #[cfg(test)]
    pub paints: usize,
    /// The private layout context (created on first use).
    context: Option<egui::Context>,
    /// The last PDF written by Print or Publish.
    pub last_pdf: Option<PathBuf>,
    /// True while a view paints into a printout (no selection is shown).
    pub painting: bool,
    /// Is the Activities chart on screen? Set by the top pane each frame;
    /// Same as screen has nothing to copy from a window that never drew.
    pub screen_seen: bool,
}

/// Preference holding the paper size chosen last, anywhere: layouts with
/// no Page Setup of their own start from it.
pub const LAST_PAPER_KEY: &str = "print_last_paper_v1";

impl App {
    /// Where the current view's Page Setup is kept: the preference and the
    /// name in it. The Activities view keeps one per layout, the other
    /// views one each.
    fn page_setup_slot(&self) -> (&'static str, String) {
        match self.print_view() {
            Some((id, _)) => (setup::VIEWS_PREF_KEY, id.to_string()),
            None => (setup::PREF_KEY, self.grid.layout.name.clone()),
        }
    }

    /// The Page Setup of the current layout or view (defaults when none is
    /// saved).
    pub(crate) fn page_setup(&self) -> PageSetup {
        let (key, name) = self.page_setup_slot();
        let mut s = setup::load_slot(|k| self.store.preference(k).ok().flatten(), key, &name)
            .unwrap_or_else(|| self.starting_page_setup());
        s.sanitize();
        s
    }

    /// The Page Setup a layout starts from: the defaults, the paper last
    /// chosen anywhere, and relationship lines when the chart shows them.
    pub(crate) fn starting_page_setup(&self) -> PageSetup {
        let mut s = PageSetup::default();
        s.options.relationships = self.gantt.options.show_relationships;
        if let Some(name) = self.store.preference(LAST_PAPER_KEY).ok().flatten() {
            if let Some(p) = setup::PAPERS.iter().find(|p| p.name == name) {
                s.paper = p.name.into();
            }
        }
        s
    }

    /// Saves `setup` for the current layout or view. Only this layout's
    /// entry is written; the setups of other layouts are not read or
    /// rewritten.
    pub(crate) fn save_page_setup(&mut self, setup: &PageSetup) -> Result<(), String> {
        let (key, name) = self.page_setup_slot();
        let text = serde_json::to_string(setup).map_err(|e| e.to_string())?;
        self.store
            .set_preference(&setup::slot_key(key, &name), &text)?;
        self.store.set_preference(LAST_PAPER_KEY, &setup.paper)
    }

    /// Is printing available in the current view? The Activities views and
    /// the WBS, Projects, Resources and Resource Assignments views print.
    pub(crate) fn can_print(&self) -> bool {
        self.views.main.is_activities() || self.print_view().is_some()
    }

    /// The pages of the current layout with `setup`.
    pub(crate) fn print_with(&mut self, setup: &PageSetup) -> Result<Printed, String> {
        let ctx = self.print_context();
        self.compose_print(&ctx, setup)
    }

    /// The PDF bytes of the current layout and what was printed.
    pub(crate) fn render_pdf(&mut self, setup: &PageSetup) -> Result<(Vec<u8>, Printed), String> {
        let printed = self.print_with(setup)?;
        let title = format!(
            "{} - {}",
            self.project.model.project.name,
            self.print_field("layout_name", 0, 1).unwrap_or_default()
        );
        Ok((pdf::write(&printed.pages, &title), printed))
    }

    /// Writes the current layout to a PDF file the user chose (Publish);
    /// returns the page count.
    pub(crate) fn write_pdf(&mut self, path: &Path, setup: &PageSetup) -> Result<usize, String> {
        let (bytes, printed) = self.render_pdf(setup)?;
        // Whole or not at all: a failed write leaves the file that was there.
        openxer_xer::write_file_atomic(path, &bytes)
            .map_err(|e| format!("Cannot write {}: {e}", path.display()))?;
        self.print.last_pdf = Some(path.to_path_buf());
        Ok(printed.pages.len())
    }

    pub(crate) fn open_page_setup(&mut self) {
        if !self.can_print() {
            return;
        }
        let s = self.page_setup();
        self.print.dialog = Some(dialog::SetupDialog::new(s));
    }

    /// Is the open preview (if any) laid out, with no new setup waiting to
    /// be painted?
    #[cfg(test)]
    pub(crate) fn print_preview_ready(&self) -> bool {
        self.print.preview.as_ref().is_none_or(|p| p.ready())
    }

    /// The private layout context of the printouts.
    pub(crate) fn print_context(&mut self) -> egui::Context {
        // Free functions format in the user's date and duration units.
        format::install(&self.formats);
        self.print
            .context
            .get_or_insert_with(compose::print_context)
            .clone()
    }

    /// Lays `preview` out with `setup`: at once when the canvas is kept,
    /// else on the frame after the next (the old pages stay shown meanwhile).
    /// False when the setup cannot print (the error is reported).
    fn lay_out_preview(&mut self, preview: &mut preview::Preview, setup: PageSetup) -> bool {
        self.print.keep_canvas = true;
        let ctx = self.print_context();
        match self.compose_job(&ctx, &setup, false) {
            Ok(Some(job)) => preview.set_job(job),
            Ok(None) => preview.wait_for(setup),
            Err(e) => {
                self.error(e);
                return false;
            }
        }
        true
    }

    /// Print Preview from a menu or toolbar: the page layout now, the
    /// canvas painted on a later frame so the click itself stays quick.
    pub(crate) fn show_print_preview(&mut self) {
        if !self.can_print() {
            return;
        }
        let s = self.page_setup();
        let mut preview = preview::Preview::empty();
        (preview.zoom, preview.view) = self.viewer_start();
        if self.lay_out_preview(&mut preview, s) {
            self.print.preview = Some(preview);
        }
    }

    /// Print Preview with every page's canvas painted before it returns
    /// (the menus use `show_print_preview`; the tests use this).
    #[cfg(test)]
    pub(crate) fn open_print_preview(&mut self) {
        if !self.can_print() {
            return;
        }
        let s = self.page_setup();
        self.print.keep_canvas = true;
        let ctx = self.print_context();
        match self.compose_job(&ctx, &s, true) {
            Ok(Some(job)) => {
                let mut preview = preview::Preview::empty();
                (preview.zoom, preview.view) = self.viewer_start();
                preview.set_job(job);
                self.print.preview = Some(preview);
            }
            Ok(None) => {}
            Err(e) => self.error(e),
        }
    }

    /// File > Print: a temp PDF opened in the default viewer.
    pub(crate) fn print_command(&mut self) {
        // A report tab prints its own report.
        if let Some(id) = self.current_report_tab() {
            self.print_report_tab(id);
            return;
        }
        if !self.can_print() {
            return;
        }
        let s = self.page_setup();
        let written = self
            .render_pdf(&s)
            .and_then(|(bytes, printed)| Ok((write_new(&print_folder()?, &bytes)?, printed)));
        match written {
            Ok((path, printed)) => match open_in_viewer(&path) {
                Ok(()) => {
                    self.status = sent_status(&printed, &path);
                    self.print.last_pdf = Some(path);
                }
                Err(e) => self.error(e),
            },
            Err(e) => self.error(e),
        }
    }

    /// Publish to PDF: asks for a file name, then writes the pages.
    pub(crate) fn publish_pdf(&mut self) {
        let name = format!("{}.pdf", self.project.model.project.name);
        let Some(path) = save_dialog(&name) else {
            return;
        };
        let s = self.page_setup();
        match self.write_pdf(&path, &s) {
            Ok(pages) => {
                self.status = format!(
                    "{} written to {}",
                    crate::util::count(pages, "page", "pages"),
                    path.display()
                )
            }
            Err(e) => self.error(e),
        }
    }

    /// Page Setup and Print Preview windows.
    pub(crate) fn print_dialogs(&mut self, ctx: &egui::Context) {
        if self.print.preview.is_none() && self.print.dialog.is_none() {
            // Nothing reuses the canvas any more, and the project may change.
            self.print.keep_canvas = false;
            self.print.canvas = None;
        }
        if self.print.preview.is_some() {
            self.print_preview_window(ctx);
        }
        if self.print.dialog.is_some() {
            self.page_setup_dialog(ctx);
        }
    }

    /// Escape closes Page Setup, then the preview. True when consumed.
    pub(crate) fn cancel_print_dialog(&mut self) -> bool {
        // Escape closes the timescale date picker before Page Setup.
        if let Some(d) = self.print.dialog.as_mut() {
            if d.picker.take().is_some() {
                return true;
            }
        }
        if self.print.dialog.take().is_some() {
            return true;
        }
        // Find closes first, then the preview.
        if let Some(p) = self.print.preview.as_mut().filter(|p| p.view.find.open) {
            p.view.find.open = false;
            return true;
        }
        let Some(p) = self.print.preview.take() else {
            return false;
        };
        self.save_viewer_prefs(p.zoom, &p.view);
        true
    }
}

/// The status line after Print: pages, paper, orientation and scale.
fn sent_status(printed: &Printed, path: &Path) -> String {
    format!(
        "{} page(s), {} {} at {:.0}%: sent to the PDF viewer for printing: {}",
        printed.pages.len(),
        printed.paper,
        match printed.orientation {
            setup::Orientation::Portrait => "portrait",
            _ => "landscape",
        },
        printed.scale * 100.,
        path.display()
    )
}

/// PDFs sent to the viewer that stay in the private print folder.
pub(crate) const KEEP_PRINTS: usize = 20;

/// Per-user folder for the PDFs Print hands to the viewer: the user's local
/// application data folder, or, if that is unavailable, a fresh folder with
/// a random name created inside the temp directory. A folder that is a link
/// is refused, so another account cannot redirect the output.
fn print_folder() -> Result<PathBuf, String> {
    crate::util::private_folder("print", "OpenXER-print")
}

/// Writes `bytes` to a new file with an unpredictable name in `dir`. The
/// file is created exclusively (an existing path or link is never opened or
/// truncated); a taken name is retried with another suffix.
pub(crate) fn write_new(dir: &Path, bytes: &[u8]) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
    // Files sent to the viewer are not kept forever: the newest stay.
    crate::util::prune_old(dir, "OpenXER-print-", "pdf", KEEP_PRINTS - 1);
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    crate::util::write_new_file(dir, &format!("OpenXER-print-{stamp}"), "pdf", bytes)
}

/// Writes `bytes` (a PDF) to a new file in the private print folder and
/// opens it in the PDF viewer for printing; the file's path.
pub(crate) fn send_pdf_to_viewer(bytes: &[u8]) -> Result<PathBuf, String> {
    let path = write_new(&print_folder()?, bytes)?;
    open_in_viewer(&path)?;
    Ok(path)
}

#[cfg(not(test))]
fn open_in_viewer(path: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        std::process::Command::new("cmd")
            .args(["/C", "start", ""])
            .arg(path)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("Cannot open the PDF viewer: {e}"))
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("Cannot open the PDF viewer: {e}"))
    }
}

/// Tests never start a viewer or a printer.
#[cfg(test)]
fn open_in_viewer(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(not(test))]
fn save_dialog(name: &str) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("PDF", &["pdf"])
        .set_file_name(name)
        .save_file()
}

#[cfg(test)]
fn save_dialog(_name: &str) -> Option<PathBuf> {
    None
}
