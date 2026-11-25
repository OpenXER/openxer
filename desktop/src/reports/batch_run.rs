//! Run Batch: pick a saved batch and write its reports, in the order the
//! batch lists them, to a folder as HTML, text or PDF files. A batch has no
//! Print Preview. A global batch runs on any project; a project batch only
//! on the project it belongs to.
use super::doc::Output;
use super::rpt::store::{self, Batch};
use super::rpt_entries::{Entry, Which};
use crate::dialog_kit as kit;
use crate::App;
use eframe::egui::{self, Align, Vec2};
use std::path::PathBuf;

/// The formats a batch can write: (caption, output).
pub const FORMATS: [(&str, Output); 3] = [
    ("HTML file", Output::Html),
    ("ASCII text file", Output::Csv),
    ("PDF file", Output::Pdf),
];

pub struct RunBatch {
    /// The batch picked in the list.
    pub selected: Option<usize>,
    pub folder: String,
    pub format: usize,
    pub notes: String,
    pub error: Option<String>,
}

impl App {
    /// The saved batches this project can run: the global ones and its own.
    pub(crate) fn runnable_batches(&self) -> Vec<Batch> {
        let open = self.open_project_id();
        store::batches(self.store.connection())
            .unwrap_or_default()
            .into_iter()
            .filter(|b| b.scope.runs_on(&open))
            .collect()
    }

    pub(crate) fn open_run_batch(&mut self) {
        let folder = self
            .store
            .preference(super::groups::BATCH_FOLDER_KEY)
            .ok()
            .flatten()
            .unwrap_or_else(|| super::doc::documents().display().to_string());
        self.reports.run_batch = Some(RunBatch {
            selected: None,
            folder,
            format: 0,
            notes: String::new(),
            error: None,
        });
    }

    /// Runs a batch: its reports in order into `folder`. Returns the files
    /// written; every report that ran gets its last run time.
    pub(crate) fn run_saved_batch(
        &mut self,
        batch: &Batch,
        folder: &std::path::Path,
        format: Output,
        notes: &str,
    ) -> Result<Vec<PathBuf>, String> {
        if !batch.scope.runs_on(&self.open_project_id()) {
            return Err(format!(
                "{} is a project batch for another project.",
                batch.name
            ));
        }
        let all = self.report_entries();
        let mut picked: Vec<Entry> = vec![];
        for id in &batch.reports {
            if let Some(e) = all.iter().find(|e| e.which == Which::Stored(*id)) {
                if !self.can_run_entry(e) {
                    return Err(self.scope_refusal(e));
                }
                picked.push(e.clone());
            }
        }
        if picked.is_empty() {
            return Err(format!("{} has no reports.", batch.name));
        }
        let refs: Vec<&Entry> = picked.iter().collect();
        let files = self.run_batch_entries(&refs, folder, format, notes)?;
        for e in &picked {
            self.mark_run(&e.which);
        }
        Ok(files)
    }

    /// Run Batch.
    pub(crate) fn run_batch_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.reports.run_batch.take() else {
            return;
        };
        if self.reports.take_escape() {
            return;
        }
        const W: f32 = 560.;
        const H: f32 = 372.;
        #[derive(Default)]
        struct Clicks {
            cancel: bool,
            run: bool,
            browse: bool,
            help: bool,
            clicked: Option<usize>,
        }
        let batches = self.runnable_batches();
        if d.selected.is_some_and(|i| i >= batches.len()) {
            d.selected = None;
        }
        let formats: Vec<(&str, bool)> = FORMATS.iter().map(|(c, _)| (*c, true)).collect();
        let (k, closed, actions) =
            kit::window(ctx, "run_batch", "Run Batch", Vec2::new(W, H), |c| {
                let mut k = Clicks::default();
                c.fill(0., 0., W, H, kit::FACE);
                let out = kit::list(
                    c,
                    "run_batch_rows",
                    (6., 6., W - 128., 182.),
                    None,
                    &[
                        kit::Col {
                            title: "Batch Name",
                            width: 240.,
                        },
                        kit::Col {
                            title: "Report Scope",
                            width: 90.,
                        },
                        kit::Col {
                            title: "Reports",
                            width: 60.,
                        },
                    ],
                    batches.len(),
                    d.selected,
                    Some(0),
                    &mut |c, i, col, rect, s| {
                        let b = &batches[i];
                        let text = match col {
                            0 => b.name.clone(),
                            1 => b.scope.caption().to_string(),
                            _ => b.reports.len().to_string(),
                        };
                        kit::cell_text(c, rect, &text, s, 3.);
                    },
                );
                k.clicked = out.clicked;
                kit::text(c, 8., 200., "Output Folder", true);
                c.edit(
                    "run_batch_folder",
                    8.,
                    216.,
                    W - 134.,
                    21.,
                    &mut d.folder,
                    true,
                    Align::Min,
                );
                let field = c.r(8., 216., W - 134., 21.);
                k.browse = c.ellipsis("run_batch_browse", field, true).clicked();
                kit::text(c, 8., 248., "Format", true);
                c.combo(
                    "run_batch_format",
                    8.,
                    264.,
                    200.,
                    21.,
                    &formats,
                    &mut d.format,
                    true,
                );
                kit::text(c, 8., 292., "User's Notes", true);
                c.edit(
                    "run_batch_notes",
                    8.,
                    308.,
                    W - 134.,
                    21.,
                    &mut d.notes,
                    true,
                    Align::Min,
                );
                if let Some(e) = &d.error {
                    kit::text(c, 8., 340., e, true);
                }
                kit::strip(c, W, H);
                let ready = d.selected.is_some() && !d.folder.trim().is_empty();
                let col = kit::button_column(
                    c,
                    W,
                    &[
                        kit::Entry::Button("run_batch_run", "Run", ready, false),
                        kit::Entry::cancel("run_batch_cancel"),
                        kit::Entry::help("run_batch_help"),
                    ],
                );
                match col.clicked.as_deref() {
                    Some("run_batch_run") => k.run = true,
                    Some("run_batch_cancel") => k.cancel = true,
                    Some("run_batch_help") => k.help = true,
                    _ => {}
                }
                k
            });
        self.record_dialog_actions(actions);
        if k.cancel || closed {
            return;
        }
        if let Some(i) = k.clicked {
            d.selected = Some(i);
        }
        if k.browse {
            if let Some(folder) = rfd::FileDialog::new()
                .set_directory(d.folder.trim())
                .pick_folder()
            {
                d.folder = folder.display().to_string();
            }
        }
        if k.help {
            self.open_help_for("run_batch");
        }
        if k.run {
            if let Some(batch) = d.selected.and_then(|i| batches.get(i)) {
                let (_, format) = FORMATS[d.format.min(FORMATS.len() - 1)];
                let folder = PathBuf::from(d.folder.trim());
                match self.run_saved_batch(batch, &folder, format, &d.notes) {
                    Ok(files) => {
                        self.status = format!(
                            "{} report{} written to {}",
                            files.len(),
                            if files.len() == 1 { "" } else { "s" },
                            folder.display()
                        );
                        if let Err(e) = self
                            .store
                            .set_preference(super::groups::BATCH_FOLDER_KEY, d.folder.trim())
                        {
                            self.error(e);
                        }
                        return;
                    }
                    Err(e) => d.error = Some(e),
                }
            }
        }
        self.reports.run_batch = Some(d);
    }
}
