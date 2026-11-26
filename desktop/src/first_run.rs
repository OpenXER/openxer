//! First run and dropped files.
//!
//! On the first start of a new workspace OpenXER opens its sample project
//! (`openxer_xer::sample_project`: an invented, resource and cost loaded job
//! in progress, with its as-planned copy as the project baseline); the
//! status bar then says so once and how to bring in a schedule of your own.
//! The calendars, resources, notebook topics and other dictionary rows the
//! sample brings are noted as the sample's (`workspace_db::record_sample`),
//! so deleting the sample in the Projects view takes them out again. An `.xer` file dropped on the window opens File > Import with that
//! file chosen, as if picked with its Browse button.
use super::*;

const NOTED_KEY: &str = "first_run_noted";
/// Set once the workspace has been initialized (seeded with the sample or had
/// a project deleted), so emptying the workspace does not re-seed the sample
/// on the next start.
pub(crate) const WORKSPACE_SEEDED_KEY: &str = "workspace_seeded";

pub(crate) fn mark_seeded(store: &Store) {
    let _ = store.set_preference(WORKSPACE_SEEDED_KEY, "1");
}

pub(crate) fn was_seeded(store: &Store) -> bool {
    store
        .preference(WORKSPACE_SEEDED_KEY)
        .ok()
        .flatten()
        .is_some()
        || store.preference(NOTED_KEY).ok().flatten().is_some()
}
/// The status message of the first start. It stays short enough to fit in
/// the status bar's message panel beside the AI agents hint on a 1366 px
/// wide window (`ai_agents::tests::hint_fits_beside_the_first_run_note`):
/// the hint yields to a message it does not fit beside, and this message
/// is the one showing on the first start, when the hint matters most. How
/// to remove the sample is in Help (Getting started) and the Projects
/// view.
pub(crate) const SAMPLE_NOTE: &str =
    "RIVERBEND is a sample project. Use File > Import... or drop an XER file.";

/// Store the first-run sample's as-planned copy `plan` (Project and Primary
/// baseline) and mid-project copy `revision` (Secondary baseline), and note
/// which dictionary rows are the sample's: those the workspace dictionary has
/// now and had not `before` the sample was stored.
pub(crate) fn install_sample(
    store: &mut Store,
    project: &mut ProjectData,
    plan: &ProjectData,
    revision: &ProjectData,
    before: &openxer_xer::Document,
) -> Result<(), String> {
    let row = store
        .id_of(&project.source_name, project.model.project.proj_id)
        .ok_or("the sample is not in the workspace")?;
    let start = openxer_xer::date_string(plan.model.origin_serial, plan.model.project.plan_start);
    let primary = crate::baselines::add_plan_baseline(
        store.connection(),
        row,
        project,
        &plan.updated_document(),
        "As planned at notice to proceed",
        &start,
    )?;
    let rev_date = openxer_xer::date_string(
        revision.model.origin_serial,
        revision.model.project.data_date,
    );
    crate::baselines::add_secondary_baseline(
        store.connection(),
        row,
        project,
        primary,
        &revision.updated_document(),
        "Mid-August Rebaseline (Aug 2026)",
        "2",
        &rev_date,
    )?;
    if store.shared()? {
        openxer_automation::workspace_db::record_sample(store.connection(), row, before)?;
    }
    store.save(project)?;
    Ok(())
}

impl App {
    /// Per frame: the one-time first-run note and dropped XER files.
    pub(crate) fn first_run_and_drops(&mut self, ctx: &egui::Context) {
        let checked = egui::Id::new("openxer_first_run_checked");
        if !ctx.data(|d| d.get_temp::<bool>(checked).unwrap_or(false)) {
            ctx.data_mut(|d| d.insert_temp(checked, true));
            self.note_first_run();
        }
        let dropped: Vec<std::path::PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        if !dropped.is_empty() {
            let modal = self.modal_open(ctx);
            self.open_dropped(&dropped, modal);
        }
    }

    /// The sample is the only project in a workspace that has not shown the
    /// note yet: say what it is, once.
    fn note_first_run(&mut self) {
        if !self.has_open_project() {
            return;
        }
        if self.store.preference(NOTED_KEY).ok().flatten().is_some() {
            return;
        }
        let _ = self.store.set_preference(NOTED_KEY, "1");
        let sample = openxer_xer::sample_project::SAMPLE_PROJECT_ID;
        let first = self.saved_projects.len() <= 1 && self.project.model.project.name == sample;
        // Only over the starting message (or the save of the sample at
        // start), never over an error or another message.
        let starting = matches!(self.status.as_str(), "Ready" | "Saved to workspace");
        if first && starting && self.error.is_none() {
            self.status = SAMPLE_NOTE.into();
        }
    }

    /// Dropped files: the XER files open File > Import with them chosen;
    /// anything else is refused in the status bar.
    pub(crate) fn open_dropped(&mut self, paths: &[std::path::PathBuf], modal: bool) {
        let xer: Vec<String> = paths
            .iter()
            .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("xer")))
            .map(|p| p.display().to_string())
            .collect();
        if xer.is_empty() {
            self.status = "Only XER files can be dropped here.".into();
            return;
        }
        if modal {
            self.status = "Close the open dialog, then drop the file again.".into();
            return;
        }
        self.open_import_wizard();
        if let Some(w) = self.interchange.xer.as_mut() {
            w.path_text = xer.join("; ");
        }
    }
}
