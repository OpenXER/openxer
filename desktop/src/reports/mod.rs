//! Reports: the shared report document and its HTML, PDF, CSV and Excel
//! writers, the Project Checker, the Report Wizard, report groups and
//! batch runs.
pub mod batch_run;
pub mod checker;
#[cfg(test)]
mod checker_tests;
pub mod checker_ui;
pub mod csv;
pub mod doc;
pub mod groups;
pub mod groups_ui;
pub mod html;
#[cfg(test)]
mod injection_tests;
pub mod pdf;
#[cfg(test)]
mod progress_tests;
pub mod progress_ui;
pub mod progress_view;
pub mod report_tab;
#[cfg(test)]
mod report_tab_tests;
pub mod report_tab_ui;
pub mod reports_view;
#[cfg(test)]
mod reports_view_tests;
pub mod rpt;
pub mod rpt_editor;
pub mod rpt_entries;
pub mod rpt_io;
pub mod rpt_props;
pub mod rpt_run;
#[cfg(test)]
mod rpt_tests;
#[cfg(test)]
pub(crate) mod test_support;
pub mod wizard;
#[cfg(test)]
mod wizard_tests;
pub mod wizard_ui;
pub mod xlsx;

/// Open report dialogs and windows.
#[derive(Default)]
pub struct ReportsState {
    pub checker: Option<checker_ui::CheckerDialog>,
    pub checker_results: Option<checker_ui::CheckerResults>,
    pub wizard: Option<wizard_ui::WizardState>,
    pub groups: Option<groups_ui::GroupsDialog>,
    pub batch: Option<groups_ui::BatchDialog>,
    /// Run Batch: pick a saved batch and run its reports in order.
    pub run_batch: Option<batch_run::RunBatch>,
    /// Select Report Group: the group lookup of the selected report.
    pub pick_group: Option<reports_view::GroupPick>,
    pub run: Option<rpt_run::RunDialog>,
    pub page_preview: Option<rpt_run::PagePreview>,
    /// Reports run into tabs of the main view area.
    pub tabs: report_tab::ReportTabs,
    /// The Text export's delimiter and qualifier dialog.
    pub text_export: Option<report_tab::TextExport>,
    /// The delimiter and qualifier last chosen there (indexes).
    pub text_choice: (usize, usize),
    pub import: Option<rpt_io::ImportDialog>,
    pub export: Option<rpt_io::ExportDialog>,
    pub editor: Option<rpt_editor::Editor>,
    /// Tools > Reports > S-Curve and Earned Hours.
    pub scurve: Option<progress_ui::ScurveDialog>,
    pub earned_hours: Option<progress_ui::EarnedDialog>,
    /// A report cut or copied in the Reports view.
    pub clipboard: Option<rpt_entries::Clip>,
    /// Escape was pressed this frame and no dialog has used it yet; the
    /// topmost open dialog takes it.
    pub escape: bool,
}

impl ReportsState {
    /// A modal report dialog is open (keyboard shortcuts stay off).
    pub fn modal_open(&self) -> bool {
        self.checker.is_some()
            || self.wizard.is_some()
            || self.groups.is_some()
            || self.batch.is_some()
            || self.run_batch.is_some()
            || self.pick_group.is_some()
            || self.run.is_some()
            || self.page_preview.is_some()
            || self.text_export.is_some()
            || self.import.is_some()
            || self.export.is_some()
            || self.editor.is_some()
            || self.scurve.is_some()
            || self.earned_hours.is_some()
    }

    /// True once per Escape press, for the topmost dialog that asks first.
    pub fn take_escape(&mut self) -> bool {
        std::mem::take(&mut self.escape)
    }
}

/// Cell text like `dialog_kit::cell_text`, clipped at `bottom` (absolute),
/// the last pixel row of the list body, so a partly visible last row does
/// not paint below its list.
pub(crate) fn cell_text(
    c: &crate::schedule_dialogs::Canvas,
    rect: eframe::egui::Rect,
    s: &str,
    selected: bool,
    pad: f32,
    bottom: f32,
) {
    use eframe::egui::{Color32, Pos2, Vec2};
    if rect.top() >= bottom {
        return;
    }
    let color = if selected {
        Color32::WHITE
    } else {
        crate::chrome::INK
    };
    let galley =
        c.ui.painter()
            .layout_no_wrap(s.to_owned(), crate::schedule_dialogs::dialog_font(), color);
    let pos = Pos2::new(
        rect.left() + pad,
        (rect.center().y - galley.size().y / 2.).round(),
    );
    let mut clip = rect.shrink2(Vec2::new(1., 0.));
    clip.max.y = clip.max.y.min(bottom);
    c.ui.painter()
        .with_clip_rect(clip)
        .galley(pos, galley, color);
}
