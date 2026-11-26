//! Scheduling dialogs: Schedule (F9), Schedule Options, the out-of-sequence
//! prompt and its detail list, Check Schedule with its results, and the
//! schedule log viewer.
//!
//! Layout is absolute, in dialog client pixels at 96 DPI. Buttons are
//! text-only.
use super::*;
use crate::dialog_kit::Entry;
use crate::schedule_check::{self as check, Check, CheckParams, CheckResult, Item, CHECKS};
#[path = "progress_tools.rs"]
pub(crate) mod progress_tools;

// The dialog palette is the kit's.
use crate::dialog_kit::FACE;

/// Which modal dialog is on top, used for Enter and Esc.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Top {
    /// The open projects' scheduling options differ: use the Default
    /// Project's for all? (`options_conflict_dialog`)
    OptionsConflict,
    /// Close on Schedule Options after a change: save the changes?
    /// (`options_save_dialog`)
    OptionsSave,
    /// Default on Schedule Options: reset every option?
    /// (`options_reset_dialog`)
    OptionsReset,
    Schedule,
    Options,
    Check,
    OutOfSequence,
    OutOfSequenceDetail,
}

/// A pending out-of-sequence confirmation for a change already applied.
#[derive(Clone, Debug)]
pub(crate) struct SequencePrompt {
    /// The activity that would be out of sequence.
    pub activity: usize,
    /// The relationship whose change triggered the prompt. Answering No
    /// removes a new relationship (`previous` is `None`) or restores the
    /// previous type and lag of an edited one.
    pub link: usize,
    pub previous: Option<(LinkType, i64)>,
    /// Set when a status change (Started/Finished) triggered the prompt:
    /// answering No restores these values instead of touching the relationship.
    pub status_revert: Option<StatusRevert>,
}
/// Activity progress fields restored when a status change is not kept.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct StatusRevert {
    pub status: StatusType,
    pub act_start: Option<i64>,
    pub act_finish: Option<i64>,
    pub rem: i64,
}

pub(crate) struct ScheduleDialogs {
    pub schedule_open: bool,
    pub own_data_date: bool,
    pub data_date_text: String,
    pub plan_to_forecast: bool,
    pub log_to_file: bool,
    pub log_html: bool,
    pub log_append: bool,
    pub log_path: String,
    pub options_advanced: bool,
    pub critical_longest: bool,
    pub critical_text: String,
    /// Recalculate assignment costs after scheduling, and its draft in
    /// Schedule Options.
    pub recalc_costs: bool,
    pub recalc_costs_draft: bool,
    pub sequence: Option<SequencePrompt>,
    /// New relationships still to check for out-of-sequence progress after
    /// the open prompt is answered (checked from the last one down).
    pub sequence_queue: Vec<usize>,
    pub sequence_detail: bool,
    /// The prompt's "turn this warning off" box: answering with it ticked
    /// clears User Preferences, Assistance, out-of-sequence warning.
    pub sequence_quiet_draft: bool,
    /// A typed Start waiting for its answer, and the saved answer
    /// (`start_hold`).
    pub start_hold: Option<crate::start_hold::Prompt>,
    pub start_hold_prefs: crate::start_hold::Prefs,
    pub check_open: bool,
    pub check_params: CheckParams,
    pub check_draft: CheckParams,
    pub check_text: Vec<String>,
    pub check_hours_text: [String; 3],
    pub check_report_path: String,
    pub check_overwrite: bool,
    pub check_results: Option<Vec<CheckResult>>,
    pub check_selected: usize,
    pub check_report_written: Option<String>,
    pub progress: progress_tools::ProgressTools,
    /// Schedule Options, "Schedule automatically when a change affects
    /// dates" (off by default), for the project named by `auto_key`; the
    /// dialog edits `auto_draft`. Kept in the workspace, as export files do
    /// not carry it.
    pub auto_schedule: bool,
    pub auto_draft: bool,
    pub auto_key: Option<String>,
    /// Schedule was pressed with open projects whose options differ: the
    /// Default Project ("ID-name") the question names.
    pub options_conflict: Option<String>,
    /// What Schedule Options showed when it opened, so Close can tell
    /// whether there is anything to save.
    pub options_shown: Option<OptionsShown>,
    /// Close was pressed on Schedule Options with changes: the save
    /// question is up.
    pub options_save: bool,
    /// Default was pressed on Schedule Options: the reset question is up.
    pub options_reset: bool,
    /// Cached `(task_id, "code - name")` list for the float-path ending
    /// activity drop-down.
    pub options_activities_cache: Option<ActivityListCache>,
}

/// Cached activity labels for Schedule Options' float-path drop-down.
pub(crate) struct ActivityListCache {
    pub key: (u64, u64, i64, usize),
    pub items: Vec<(i64, String)>,
}

/// The values Schedule Options shows, compared on Close.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct OptionsShown {
    pub options: ScheduleOptions,
    pub critical_longest: bool,
    pub critical_text: String,
    pub recalc_costs: bool,
    pub auto_schedule: bool,
}

fn documents() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .map(|p| PathBuf::from(p).join("Documents"))
        .unwrap_or_else(std::env::temp_dir)
}

impl Default for ScheduleDialogs {
    fn default() -> Self {
        let params = CheckParams::default();
        Self {
            schedule_open: false,
            own_data_date: true,
            data_date_text: String::new(),
            plan_to_forecast: false,
            log_to_file: false,
            log_html: false,
            log_append: false,
            log_path: documents().join("SCHEDLOG.TXT").display().to_string(),
            options_advanced: false,
            critical_longest: false,
            critical_text: "0h".into(),
            recalc_costs: false,
            recalc_costs_draft: false,
            sequence: None,
            sequence_queue: vec![],
            sequence_detail: false,
            sequence_quiet_draft: false,
            start_hold: None,
            start_hold_prefs: Default::default(),
            check_open: false,
            check_text: target_texts(&params),
            check_hours_text: hour_texts(&params),
            check_draft: params.clone(),
            check_params: params,
            check_report_path: documents()
                .join("ScheduleCheckReport.html")
                .display()
                .to_string(),
            check_overwrite: true,
            check_results: None,
            check_selected: 0,
            check_report_written: None,
            progress: Default::default(),
            auto_schedule: false,
            auto_draft: false,
            auto_key: None,
            options_conflict: None,
            options_shown: None,
            options_save: false,
            options_reset: false,
            options_activities_cache: None,
        }
    }
}

fn number(value: f64) -> String {
    crate::dialog_kit::format_number(value)
}
fn target_texts(p: &CheckParams) -> Vec<String> {
    p.targets
        .iter()
        .map(|t| t.map_or(String::new(), number))
        .collect()
}
fn hour_texts(p: &CheckParams) -> [String; 3] {
    [
        number(p.long_lag_hours),
        number(p.large_float_hours),
        number(p.large_duration_hours),
    ]
}

// ------------------------------------------------------------------ dates

fn epoch() -> chrono::NaiveDateTime {
    chrono::NaiveDate::from_ymd_opt(1899, 12, 30)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap()
}

/// The reference date display: `30-Oct-26 06:00`.
/// Workspace preference for Schedule Options' "Recalculate assignment costs
/// after scheduling".
pub(crate) const RECALC_COSTS_KEY: &str = "schedule_recalc_costs_v1";
/// Workspace preference for Level Resources' "Recalculate assignment costs
/// after leveling". XER files carry no column for it.
pub(crate) const LEVEL_RECALC_COSTS_KEY: &str = "level_recalc_costs_v1";

pub(crate) fn display_date(origin: i64, value: Option<i64>) -> String {
    format::date(origin, value)
}

/// Parse a date in the user's format (or ISO) into model minutes.
pub(crate) fn parse_display_date(origin: i64, text: &str) -> Result<i64, String> {
    format::parse_date(origin, text, None)?.ok_or_else(|| "Enter a data date".to_string())
}

// ------------------------------------------------------------------ canvas

// The fixed-layout canvas, its controls and the modal shell live in the
// dialog kit; these names stay here for the files that import them.
pub(crate) use crate::dialog_kit::{dialog_font, modal, modal_at, paint_sunken, Canvas};

/// A project's critical-activity definition from its PROJECT row:
/// (longest path, total float threshold in hours).
pub(crate) fn critical_definition_of(p: &ProjectData) -> (bool, f64) {
    openxer_xer::critical_definition(p)
}

/// The activities critical by the project's own definition (the status bar
/// count after a schedule run).
pub(crate) fn critical_count(p: &ProjectData) -> usize {
    let definition = critical_definition_of(p);
    p.model
        .activities
        .iter()
        .filter(|a| crate::grid::columns::is_critical(a, definition))
        .count()
}

// ------------------------------------------------------------------ App hooks

impl App {
    pub(crate) fn top_schedule_dialog(&self) -> Option<Top> {
        if self.sched.options_conflict.is_some() {
            Some(Top::OptionsConflict)
        } else if self.show_schedule && self.sched.options_save {
            Some(Top::OptionsSave)
        } else if self.show_schedule && self.sched.options_reset {
            Some(Top::OptionsReset)
        } else if self.sched.sequence_detail {
            Some(Top::OutOfSequenceDetail)
        } else if self.sched.sequence.is_some() {
            Some(Top::OutOfSequence)
        } else if self.show_schedule {
            Some(Top::Options)
        } else if self.sched.schedule_open {
            Some(Top::Schedule)
        } else if self.sched.check_open {
            Some(Top::Check)
        } else {
            None
        }
    }

    /// Esc cancels the top dialog. Returns whether a dialog consumed it.
    pub(crate) fn cancel_top_schedule_dialog(&mut self) -> bool {
        match self.top_schedule_dialog() {
            Some(Top::OptionsConflict) => {
                self.diag_log().line("options_conflict choice=Escape");
                self.sched.options_conflict = None;
            }
            Some(Top::OutOfSequenceDetail) => self.sched.sequence_detail = false,
            // Escape on the save or reset question goes back to Schedule
            // Options; on Schedule Options itself it does nothing, as
            // observed in the reference.
            Some(Top::OptionsSave) => self.sched.options_save = false,
            Some(Top::OptionsReset) => self.sched.options_reset = false,
            Some(Top::OutOfSequence) => self.answer_sequence(false),
            Some(Top::Options) => {}
            Some(Top::Schedule) => self.sched.schedule_open = false,
            Some(Top::Check) => self.sched.check_open = false,
            None => return false,
        }
        true
    }

    /// F9 and Tools > Schedule: open the Schedule dialog.
    pub(crate) fn open_schedule_dialog(&mut self) {
        if self.open_set.empty {
            self.status = "Open or create a project before scheduling.".into();
            return;
        }
        if let Some((activity, field)) = self.edit_cell.take() {
            if !self.apply_cell(activity, field, &self.edit_text.clone()) {
                return;
            }
        }
        let origin = self.project.model.origin_serial;
        self.sched.data_date_text = display_date(origin, self.project.model.project.data_date);
        self.sched.own_data_date = true;
        self.sched.plan_to_forecast = false;
        self.sched.schedule_open = true;
    }

    pub(crate) fn open_check_schedule(&mut self) {
        self.sched.check_draft = self.sched.check_params.clone();
        self.sched.check_text = target_texts(&self.sched.check_draft);
        self.sched.check_hours_text = hour_texts(&self.sched.check_draft);
        self.sched.check_open = true;
    }

    fn project_row(&self) -> Option<usize> {
        let pid = self.project.model.project.proj_id.to_string();
        let table = self.project.document.table("PROJECT")?;
        table
            .rows
            .iter()
            .position(|r| table.value(r, "proj_id") == pid)
    }
    fn project_value(&self, key: &str) -> String {
        self.project_row()
            .and_then(|i| {
                let table = self.project.document.table("PROJECT")?;
                Some(table.value(&table.rows[i], key).to_string())
            })
            .unwrap_or_default()
    }
    fn set_project_value(&mut self, key: &str, value: String) {
        if let Some(i) = self.project_row() {
            self.project
                .document
                .table_mut("PROJECT")
                .set(i, key, value);
        }
    }
    /// The project's critical-activity definition: (longest path, hours).
    pub(crate) fn critical_definition(&self) -> (bool, f64) {
        critical_definition_of(&self.project)
    }
    pub(crate) fn forecast_start(&self) -> Option<i64> {
        openxer_xer::parse_relative(
            self.project.model.origin_serial,
            &self.project_value("fcst_start_date"),
        )
        .ok()
        .flatten()
    }
    fn project_title(&self) -> String {
        let pid = self.project.model.project.proj_id.to_string();
        self.project
            .document
            .table("PROJWBS")
            .and_then(|t| {
                t.rows
                    .iter()
                    .find(|r| t.value(r, "proj_id") == pid && t.value(r, "proj_node_flag") == "Y")
                    .map(|r| t.value(r, "wbs_name").to_string())
            })
            .unwrap_or_else(|| self.project.model.project.name.clone())
    }
    fn expense_tasks(&self) -> BTreeSet<i64> {
        check::expense_tasks(&self.project)
    }

    /// Prepare Schedule Options' extra drafts alongside `draft_options`.
    fn auto_schedule_key(&self) -> String {
        format!("auto_schedule_v1:{}", self.project.model.project.name)
    }

    /// Reads the automatic scheduling choice of the current project when
    /// the project changed.
    pub(crate) fn refresh_auto_schedule(&mut self) {
        let key = self.auto_schedule_key();
        if self.sched.auto_key.as_deref() != Some(key.as_str()) {
            self.sched.auto_schedule =
                matches!(self.store.preference(&key), Ok(Some(v)) if v == "1");
            self.sched.auto_key = Some(key);
        }
    }

    /// Turns automatic scheduling on or off for the current project.
    pub(crate) fn set_auto_schedule(&mut self, on: bool) {
        self.refresh_auto_schedule();
        if self.sched.auto_schedule == on {
            return;
        }
        let key = self.auto_schedule_key();
        match self.store.set_preference(&key, if on { "1" } else { "0" }) {
            Ok(()) => self.sched.auto_schedule = on,
            Err(error) => self.error(format!("Cannot save the scheduling option: {error}")),
        }
    }

    pub(crate) fn open_options_drafts(&mut self) {
        self.refresh_auto_schedule();
        self.sched.auto_draft = self.sched.auto_schedule;
        let (longest, hours) = self.critical_definition();
        self.sched.critical_longest = longest;
        self.sched.recalc_costs = self
            .store
            .preference(RECALC_COSTS_KEY)
            .ok()
            .flatten()
            .as_deref()
            == Some("Y");
        self.sched.recalc_costs_draft = self.sched.recalc_costs;
        self.sched.critical_text = format::duration(
            (hours * 60.).round() as i64,
            format::project_periods(&self.project),
        );
        self.sched.options_save = false;
        self.sched.options_reset = false;
        self.sched.options_shown = self.options_on_screen();
    }

    /// The values Schedule Options shows now (its drafts).
    fn options_on_screen(&self) -> Option<OptionsShown> {
        Some(OptionsShown {
            options: self.draft_options.clone()?,
            critical_longest: self.sched.critical_longest,
            critical_text: self.sched.critical_text.clone(),
            recalc_costs: self.sched.recalc_costs_draft,
            auto_schedule: self.sched.auto_draft,
        })
    }

    /// Close, Cancel, Enter and the close box on Schedule Options: with a
    /// value that differs from what the dialog opened with it asks whether
    /// to save, otherwise it just closes. Escape does nothing there.
    pub(crate) fn press_options_close(&mut self) {
        if self.options_on_screen() != self.sched.options_shown {
            self.sched.options_save = true;
        } else {
            self.keep_repaired_float_paths();
            self.drop_options_drafts();
        }
    }

    /// Close Schedule Options without saving its drafts.
    fn drop_options_drafts(&mut self) {
        self.show_schedule = false;
        self.draft_options = None;
        self.sched.options_save = false;
        self.sched.options_reset = false;
        self.sched.options_shown = None;
        self.sched.options_activities_cache = None;
    }

    /// Commit the Schedule Options drafts (Close).
    pub(crate) fn apply_options_drafts(&mut self) -> Result<(), String> {
        let periods = format::project_periods(&self.project);
        let hours = format::parse_duration(&self.sched.critical_text, periods).map_err(|_| {
            format!(
                "Enter the critical total float, for example {}.",
                format::duration(0, periods)
            )
        })? as f64
            / 60.;
        self.set_project_value(
            "critical_path_type",
            if self.sched.critical_longest {
                "CT_DrivPath"
            } else {
                "CT_TotFloat"
            }
            .into(),
        );
        self.set_project_value("critical_drtn_hr_cnt", number(hours));
        if self.sched.recalc_costs_draft != self.sched.recalc_costs {
            self.sched.recalc_costs = self.sched.recalc_costs_draft;
            let text = if self.sched.recalc_costs { "Y" } else { "N" };
            if let Err(e) = self.store.set_preference(RECALC_COSTS_KEY, text) {
                self.error(format!("Cannot save the schedule options: {e}"));
            }
        }
        self.set_auto_schedule(self.sched.auto_draft);
        Ok(())
    }

    /// Whether F9 recalculates assignment costs after scheduling (Schedule
    /// Options). XER files do not carry the option, so it is a workspace
    /// preference.
    pub(crate) fn recalc_costs_after_scheduling(&self) -> bool {
        self.store
            .preference(RECALC_COSTS_KEY)
            .ok()
            .flatten()
            .as_deref()
            == Some("Y")
    }

    /// Whether a run that levels recalculates assignment costs afterwards
    /// (Level Resources).
    pub(crate) fn recalc_costs_after_leveling(&self) -> bool {
        self.store
            .preference(LEVEL_RECALC_COSTS_KEY)
            .ok()
            .flatten()
            .as_deref()
            == Some("Y")
    }

    /// The Schedule button: apply the dialog's data date, run, log, close.
    #[cfg(test)]
    pub(crate) fn run_schedule_dialog(&mut self) {
        let origin = self.project.model.origin_serial;
        let before = self.project.model.project.clone();
        if self.sched.plan_to_forecast {
            if let Some(forecast) = self.forecast_start() {
                self.project.model.project.data_date = Some(forecast);
                self.project.model.project.plan_start = Some(forecast);
            }
        } else if !self.sched.own_data_date {
            match parse_display_date(origin, &self.sched.data_date_text) {
                Ok(date) => {
                    self.project.model.project.data_date = Some(date);
                    if self.open_set.multi() {
                        self.open_set.apply_data_date = Some(date + origin * DAY);
                    }
                }
                Err(error) => {
                    self.error(error);
                    return;
                }
            }
        }
        // Several open projects keeping data dates that differ are scheduled
        // each from its own date, with no question, as the reference does.
        let runs = self.schedule_runs;
        self.schedule();
        self.open_set.apply_data_date = None;
        if self.schedule_runs == runs {
            self.project.model.project = before;
            return;
        }
        self.sched.schedule_open = false;
        if self.sched.log_to_file {
            if let Err(error) = write_log(
                Path::new(&self.sched.log_path),
                &self.log,
                self.sched.log_html,
                self.sched.log_append,
            ) {
                self.status = format!("Fail to create log file: {error}  Log disabled.");
            }
        }
    }

    /// Build the schedule log for the run that just finished.
    pub(crate) fn schedule_log(&self, elapsed: f64) -> Vec<String> {
        let (longest, hours) = self.critical_definition();
        let started = chrono::Local::now().naive_local();
        // With several projects open the run takes its options from the
        // Default Project; with one, that project is the default.
        let default = self
            .default_project_name()
            .unwrap_or_else(|| self.project.model.project.name.clone());
        let auto = matches!(
            self.store.preference(&self.auto_schedule_key()),
            Ok(Some(v)) if v == "1"
        );
        let mut log = build_log(
            &self.project,
            &self.project_title(),
            &default,
            (longest, hours),
            (self.recalc_costs_after_scheduling(), auto),
            started,
        );
        if !self.project.warnings.is_empty() {
            log.push("Import notes:".into());
            log.push("-------------".into());
            for warning in &self.project.warnings {
                log.push(format!("\t{warning}"));
            }
        }
        log.push(format!("\tCalculation time: {elapsed:.3} s"));
        log
    }

    /// User Preferences, Assistance: whether progress or a relationship
    /// change that leaves an activity out of sequence asks first (off by
    /// default; the schedule log and View > Check Schedule list such
    /// activities either way).
    pub(crate) fn warns_out_of_sequence(&self) -> bool {
        self.editing.options.warn_out_of_sequence
    }

    /// After a relationship change, ask whether to keep a change that leaves
    /// the successor out of sequence.
    pub(crate) fn check_relationship_sequence(
        &mut self,
        link: usize,
        previous: Option<(LinkType, i64)>,
    ) {
        if !self.warns_out_of_sequence() || !check::link_out_of_sequence(&self.project.model, link)
        {
            return;
        }
        let activity = self.project.model.links[link].succ;
        self.sched.sequence = Some(SequencePrompt {
            activity,
            link,
            previous,
            status_revert: None,
        });
        self.sched.sequence_quiet_draft = false;
    }

    /// After several relationships were added (`links`), ask about each one
    /// that leaves its successor out of sequence, one prompt at a time. The
    /// last is asked first, so removing one keeps the others' positions.
    pub(crate) fn check_relationships_sequence(&mut self, links: std::ops::Range<usize>) {
        self.sched.sequence_queue = links.collect();
        self.next_sequence_prompt();
    }

    fn next_sequence_prompt(&mut self) {
        while self.sched.sequence.is_none() {
            let Some(link) = self.sched.sequence_queue.pop() else {
                return;
            };
            if link < self.project.model.links.len() {
                self.check_relationship_sequence(link, None);
            }
        }
    }

    /// After a progress change on `activity`, ask whether to keep it when one of
    /// its predecessor relationships is now out of sequence.
    pub(crate) fn check_status_sequence(&mut self, activity: usize, revert: StatusRevert) {
        if !self.warns_out_of_sequence() {
            return;
        }
        let model = &self.project.model;
        let Some(link) = (0..model.links.len())
            .find(|&r| model.links[r].succ == activity && check::link_out_of_sequence(model, r))
        else {
            return;
        };
        self.sched.sequence = Some(SequencePrompt {
            activity,
            link,
            previous: None,
            status_revert: Some(revert),
        });
        self.sched.sequence_quiet_draft = false;
    }

    fn answer_sequence(&mut self, keep: bool) {
        self.answer_one_sequence(keep);
        self.next_sequence_prompt();
    }

    fn answer_one_sequence(&mut self, keep: bool) {
        let Some(prompt) = self.sched.sequence.take() else {
            return;
        };
        self.sched.sequence_detail = false;
        if std::mem::take(&mut self.sched.sequence_quiet_draft) {
            self.editing.options.warn_out_of_sequence = false;
            self.save_editing_options();
        }
        if keep {
            return;
        }
        if let Some(revert) = prompt.status_revert {
            if let Some(a) = self.project.model.activities.get_mut(prompt.activity) {
                a.status = revert.status;
                a.act_start = revert.act_start;
                a.act_finish = revert.act_finish;
                a.rem = revert.rem;
                self.date_edits.clear();
                self.status = "Status change cancelled.".into();
            }
            return;
        }
        let result = match prompt.previous {
            None => relationships::remove(&mut self.project.model, prompt.link),
            Some((kind, lag)) => {
                relationships::edit(&mut self.project.model, prompt.link, kind, lag as f64 / 60.)
            }
        };
        self.status = match result {
            Ok(()) => "Relationship change cancelled.".into(),
            Err(error) => error,
        };
    }

    /// Draw every scheduling dialog. Called from `dialogs()`.
    pub(crate) fn schedule_dialogs(&mut self, ctx: &egui::Context) {
        // Enter presses the default button of the top dialog.
        let any_combo_open = ctx.data(|d| {
            [
                ("options", "float_type"),
                ("options", "lag_calendar"),
                ("options", "project"),
                ("options", "mlp_end"),
                ("check", "project"),
            ]
            .iter()
            .any(|key| d.get_temp::<bool>(egui::Id::new(*key)).unwrap_or(false))
        });
        let enter = self.error.is_none()
            && !self.busy_active()
            && !self.help.open
            && !crate::dialog_keys::enter_handled(ctx)
            && ctx.input(|i| i.key_pressed(Key::Enter))
            && !any_combo_open;
        if enter {
            match self.top_schedule_dialog() {
                // The message box presses its own default.
                Some(Top::OptionsConflict | Top::OptionsSave | Top::OptionsReset) => {}
                Some(Top::Schedule) => self.press_schedule(ctx),
                Some(Top::Options) => self.press_options_close(),
                Some(Top::Check) => self.run_check_schedule(),
                Some(Top::OutOfSequence) => self.answer_sequence(true),
                Some(Top::OutOfSequenceDetail) => self.sched.sequence_detail = false,
                None => {}
            }
        }
        if self.sched.check_open {
            self.check_dialog(ctx);
        }
        if self.sched.schedule_open {
            self.schedule_dialog(ctx);
        }
        if self.show_schedule {
            self.options_dialog(ctx);
        }
        if self.sched.sequence.is_some() {
            self.sequence_dialog(ctx);
        }
        if self.sched.sequence_detail {
            self.sequence_detail_dialog(ctx);
        }
        self.start_hold_dialog(ctx);
        if self.sched.check_results.is_some() {
            self.check_results_window(ctx);
        }
        self.progress_dialogs(ctx);
        if self.show_log {
            self.log_window(ctx);
        }
        self.options_conflict_dialog(ctx);
        self.options_save_dialog(ctx);
        self.options_reset_dialog(ctx);
    }

    /// Default on Schedule Options asks first: Yes puts the default values
    /// in the dialog (nothing is saved until Close and Yes), No and Escape
    /// leave the values as they are. Default from
    /// `kit_window::Question::ResetScheduleOptions`.
    fn options_reset_dialog(&mut self, ctx: &egui::Context) {
        if !(self.show_schedule && self.sched.options_reset) {
            return;
        }
        let text = format!(
            "Put every scheduling option of {} - {} back to its default value?",
            self.project.model.project.name,
            self.project_title()
        );
        let buttons = ["Yes", "No"];
        let mut rects = vec![];
        let clicked = crate::kit_window::message_box(
            ctx,
            "options_reset",
            "Confirmation",
            crate::kit_window::MessageIcon::Question,
            &text,
            &buttons,
            crate::kit_window::Question::ResetScheduleOptions.default_in(&buttons),
            &mut rects,
        );
        self.record(rects);
        match clicked {
            Some(0) => {
                self.sched.options_reset = false;
                self.reset_options_drafts();
            }
            Some(_) => self.sched.options_reset = false,
            None => {}
        }
    }

    /// The values Default puts in Schedule Options, as observed in the
    /// reference: every option of the General and Advanced tabs and
    /// critical activities by a total float of 0. The two workspace check
    /// boxes (automatic scheduling, recalculating assignment costs) stay as
    /// they are: the reference was not seen changing them.
    fn reset_options_drafts(&mut self) {
        let current = self
            .draft_options
            .clone()
            .unwrap_or_else(|| self.project.model.options.clone());
        self.draft_options = Some(default_options(&current));
        self.sched.critical_longest = false;
        self.sched.critical_text = format::duration(0, format::project_periods(&self.project));
    }

    /// Close on Schedule Options after a change: Yes saves the options and
    /// closes, No closes without saving, Cancel and Escape go back to the
    /// dialog. Default from `kit_window::Question::SaveScheduleOptions`.
    fn options_save_dialog(&mut self, ctx: &egui::Context) {
        if !(self.show_schedule && self.sched.options_save) {
            return;
        }
        let text = format!(
            "Save the changed scheduling options of {} - {}?",
            self.project.model.project.name,
            self.project_title()
        );
        let buttons = ["Yes", "No", "Cancel"];
        let mut rects = vec![];
        let clicked = crate::kit_window::message_box(
            ctx,
            "options_save",
            "Confirmation",
            crate::kit_window::MessageIcon::Question,
            &text,
            &buttons,
            crate::kit_window::Question::SaveScheduleOptions.default_in(&buttons),
            &mut rects,
        );
        self.record(rects);
        match clicked {
            Some(0) => {
                self.sched.options_save = false;
                self.close_options();
            }
            Some(1) => self.drop_options_drafts(),
            Some(_) => self.sched.options_save = false,
            None => {}
        }
    }

    /// The Schedule button (or Enter): with open projects whose scheduling
    /// options differ it asks first, otherwise it runs.
    fn press_schedule(&mut self, ctx: &egui::Context) {
        match self.conflicting_options() {
            Some(default) => {
                self.diag_log().line(&format!(
                    "options_conflict shown open_projects={}",
                    self.open_ids().len()
                ));
                self.sched.options_conflict = Some(default);
            }
            None => self.begin_busy(ctx, crate::busy::Busy::Schedule),
        }
    }

    /// Yes schedules every open project with the Default Project's options;
    /// No and Escape go back to the Schedule dialog. Default from
    /// `kit_window::Question::ConflictingScheduleOptions`.
    fn options_conflict_dialog(&mut self, ctx: &egui::Context) {
        let Some(default) = self.sched.options_conflict.clone() else {
            return;
        };
        let text = format!(
            "The open projects do not share the same scheduling options. \
             Schedule every open project with the options of {default}?"
        );
        let buttons = ["Yes", "No"];
        let mut rects = vec![];
        let clicked = crate::kit_window::message_box(
            ctx,
            "options_conflict",
            "Confirmation",
            crate::kit_window::MessageIcon::Question,
            &text,
            &buttons,
            crate::kit_window::Question::ConflictingScheduleOptions.default_in(&buttons),
            &mut rects,
        );
        self.record(rects);
        match clicked {
            Some(0) => {
                self.diag_log().line("options_conflict choice=Yes");
                self.sched.options_conflict = None;
                self.open_set.default_options_for_all = true;
                self.begin_busy(ctx, crate::busy::Busy::Schedule);
            }
            Some(_) => {
                self.diag_log().line("options_conflict choice=No");
                self.sched.options_conflict = None;
            }
            None => {}
        }
    }

    fn record(&mut self, actions: Vec<(String, Rect)>) {
        for (name, r) in actions {
            self.action_rects
                .insert(name, [r.left(), r.top(), r.right(), r.bottom()]);
        }
    }

    /// Save the Schedule Options drafts and close (Yes on the save
    /// question).
    fn close_options(&mut self) {
        match self.apply_options_drafts() {
            Ok(()) => {
                self.apply_options();
                self.sched.options_shown = None;
                self.sched.options_activities_cache = None;
            }
            Err(error) => self.error(error),
        }
    }

    // -------------------------------------------------------- Schedule (F9)

    fn schedule_dialog(&mut self, ctx: &egui::Context) {
        #[derive(Default)]
        struct Clicks {
            cancel: bool,
            run: bool,
            log: bool,
            help: bool,
            options: bool,
        }
        let origin = self.project.model.origin_serial;
        let forecast = self.forecast_start();
        let forecast_text = display_date(origin, forecast);
        let open_count = self.open_ids().len().max(1);
        let state = &mut self.sched;
        let (clicks, closed, actions) =
            modal(ctx, "schedule", "Schedule", Vec2::new(533., 273.), |c| {
                let mut k = Clicks::default();
                c.fill(5., 5., 411., 263., FACE);
                c.lowered(5., 5., 411., 263.);
                c.lowered(11., 11., 399., 163.);
                c.lowered(11., 174., 399., 88.);
                c.label(17., 23., 16., "Project(s) to schedule", true);
                let mut count = open_count.to_string();
                c.edit(
                    "schedule_projects",
                    268.,
                    20.,
                    137.,
                    21.,
                    &mut count,
                    false,
                    Align::Center,
                );
                if c.radio(
                    "schedule_own_date",
                    17.,
                    62.,
                    370.,
                    17.,
                    "All Projects use their own data date",
                    state.own_data_date,
                    true,
                ) {
                    state.own_data_date = true;
                }
                if c.radio(
                    "schedule_apply_date",
                    17.,
                    82.,
                    260.,
                    19.,
                    "Apply selected data date to all open projects",
                    !state.own_data_date,
                    true,
                ) {
                    state.own_data_date = false;
                }
                let date_enabled = !state.own_data_date;
                // The shared date edit: the `...` inside the edit's frame.
                crate::views::sheet::date_text_field(
                    c,
                    "schedule_data_date",
                    "schedule_date_picker",
                    (268., 82., 137., 21.),
                    &mut state.data_date_text,
                    origin,
                    date_enabled,
                );
                c.label(20., 111., 16., "Project Forecast Start Date", true);
                let mut forecast_shown = forecast_text.clone();
                c.edit(
                    "schedule_forecast",
                    268.,
                    108.,
                    137.,
                    21.,
                    &mut forecast_shown,
                    false,
                    Align::Center,
                );
                let response = c.checkbox(
                    "schedule_plan_to_forecast",
                    20.,
                    148.,
                    386.,
                    20.,
                    "Set Data Date and Planned Start to Project Forecast Start during scheduling",
                    &mut state.plan_to_forecast,
                    forecast.is_some(),
                );
                if forecast.is_none() {
                    response.on_hover_text("The project has no forecast start date.");
                }
                c.checkbox(
                    "schedule_log_to_file",
                    21.,
                    183.,
                    190.,
                    20.,
                    "Log to file",
                    &mut state.log_to_file,
                    true,
                );
                let log_enabled = state.log_to_file;
                c.checkbox(
                    "schedule_log_html",
                    211.,
                    183.,
                    190.,
                    20.,
                    "As Html",
                    &mut state.log_html,
                    log_enabled,
                );
                c.edit(
                    "schedule_log_path",
                    39.,
                    206.,
                    329.,
                    21.,
                    &mut state.log_path,
                    log_enabled,
                    Align::Min,
                );
                let frame = c.r(39., 206., 350., 21.);
                c.sunken(
                    Rect::from_min_max(Pos2::new(frame.right() - 21., frame.top()), frame.max),
                    if log_enabled { Color32::WHITE } else { FACE },
                );
                if c.ellipsis("schedule_log_browse", frame, log_enabled)
                    .clicked()
                {
                    if let Some(path) = rfd::FileDialog::new()
                        .set_file_name("SCHEDLOG.TXT")
                        .save_file()
                    {
                        state.log_path = path.display().to_string();
                    }
                }
                if c.radio(
                    "schedule_log_overwrite",
                    39.,
                    238.,
                    113.,
                    17.,
                    "Overwrite existing",
                    !state.log_append,
                    log_enabled,
                ) {
                    state.log_append = false;
                }
                if c.radio(
                    "schedule_log_append",
                    158.,
                    238.,
                    113.,
                    17.,
                    "Append to existing",
                    state.log_append,
                    log_enabled,
                ) {
                    state.log_append = true;
                }
                // The order of the reference's own Schedule dialog: Cancel,
                // then Schedule (the default), View Log, Help, Options....
                let column = crate::dialog_kit::observed_button_column(
                    c,
                    533.,
                    &[
                        Entry::cancel("schedule_cancel"),
                        Entry::Button("schedule_run", "Schedule", true, true),
                        Entry::Button("schedule_view_log", "View Log", true, false),
                        Entry::Button("schedule_help", "Help", true, false),
                        Entry::Button("schedule_options", "Options...", true, false),
                    ],
                );
                crate::dialog_kit::mark_default(c, "schedule_run");
                let clicked = |key: &str| column.clicked.as_deref() == Some(key);
                k.run = clicked("schedule_run");
                k.cancel = clicked("schedule_cancel");
                k.log = clicked("schedule_view_log");
                k.options = clicked("schedule_options");
                k.help = clicked("schedule_help");
                k
            });
        self.record(actions);
        if clicks.cancel || closed {
            self.sched.schedule_open = false;
        } else if clicks.run {
            self.press_schedule(ctx);
        } else if clicks.log {
            self.show_log = true;
        } else if clicks.help {
            self.open_help_for("schedule");
        } else if clicks.options {
            self.open_options();
        }
    }

    // ------------------------------------------------------ Schedule Options

    fn options_dialog(&mut self, ctx: &egui::Context) {
        #[derive(Default)]
        struct Clicks {
            close: bool,
            cancel: bool,
            default: bool,
            help: bool,
        }
        let mut options = self
            .draft_options
            .take()
            .unwrap_or_else(|| self.project.model.options.clone());
        let project_label = format!(
            "{} - {}",
            self.project.model.project.name,
            self.project_title()
        );
        // Only build the activity list when the Advanced tab is shown, and
        // cache it across frames until project records change.
        if self.sched.options_advanced {
            let cache_key = (
                self.records_revision,
                self.history.changes(),
                self.project.model.project.proj_id,
                self.project.model.activities.len(),
            );
            if self
                .sched
                .options_activities_cache
                .as_ref()
                .is_none_or(|c| c.key != cache_key)
            {
                let built: Vec<(i64, String)> = self
                    .project
                    .model
                    .activities
                    .iter()
                    .enumerate()
                    .map(|(i, a)| {
                        (
                            a.task_id,
                            format!("{} - {}", a.task_code, self.project.name(i)),
                        )
                    })
                    .collect();
                self.sched.options_activities_cache = Some(ActivityListCache {
                    key: cache_key,
                    items: built,
                });
            }
        }
        // Escape does nothing on this dialog; the shell reports it as a
        // close, so only the close box closes it.
        let escape = ctx.input(|i| i.key_pressed(Key::Escape));
        let state = &mut self.sched;
        let (clicks, closed, actions) = modal(
            ctx,
            "options",
            "Schedule Options",
            Vec2::new(617., 630.),
            |c| {
                let mut k = Clicks::default();
                c.label(13., 11., 14., "Use scheduling options from", true);
                let mut project = 0;
                c.combo(
                    "project",
                    13.,
                    31.,
                    400.,
                    22.,
                    &[(project_label.as_str(), true)],
                    &mut project,
                    true,
                );
                let tab = usize::from(state.options_advanced);
                let tabs = [("General", "0"), ("Advanced", "1")].map(|(label, key)| {
                    crate::dialog_kit::VTab {
                        label,
                        key,
                        unavailable: None,
                    }
                });
                if let Some(i) =
                    crate::dialog_kit::tabs(c, "options_tab_", (10., 82., 485., 538.), &tabs, tab)
                {
                    state.options_advanced = i == 1;
                }
                c.lowered(15., 87., 475., 528.);
                if state.options_advanced {
                    c.checkbox(
                        "options_mlp",
                        28.,
                        99.,
                        281.,
                        17.,
                        "Calculate multiple float paths",
                        &mut options.multiple_float_paths,
                        true,
                    );
                    let on = options.multiple_float_paths;
                    c.label(46., 131., 13., "Calculate multiple paths using", on);
                    if c.radio(
                        "options_mlp_total",
                        46.,
                        151.,
                        113.,
                        17.,
                        "Total Float",
                        options.float_paths_use_total,
                        on,
                    ) {
                        options.float_paths_use_total = true;
                    }
                    if c.radio(
                        "options_mlp_free",
                        164.,
                        151.,
                        113.,
                        17.,
                        "Free Float",
                        !options.float_paths_use_total,
                        on,
                    ) {
                        options.float_paths_use_total = false;
                    }
                    c.label(
                        46.,
                        183.,
                        13.,
                        "Display multiple float paths ending with activity",
                        on,
                    );
                    let activities: &[(i64, String)] = state
                        .options_activities_cache
                        .as_ref()
                        .map_or(&[], |c| c.items.as_slice());
                    let mut items: Vec<(&str, bool)> = Vec::with_capacity(activities.len() + 1);
                    items.push(("", true));
                    items.extend(activities.iter().map(|(_, s)| (s.as_str(), true)));
                    let mut selected = options
                        .float_paths_end_activity
                        .and_then(|id| activities.iter().position(|(t, _)| *t == id))
                        .map_or(0, |i| i + 1);
                    if c.combo("mlp_end", 46., 203., 185., 21., &items, &mut selected, on) {
                        options.float_paths_end_activity =
                            selected.checked_sub(1).map(|i| activities[i].0);
                    }
                    c.label(
                        46.,
                        243.,
                        13.,
                        "Specify the number of paths to calculate",
                        on,
                    );
                    let mut text = options.max_float_paths.to_string();
                    c.spin(
                        "options_mlp_paths",
                        276.,
                        240.,
                        73.,
                        23.,
                        &mut text,
                        1.,
                        1.0..=1000.,
                        on,
                    );
                    if let Ok(n) = text.trim().parse::<f64>() {
                        options.max_float_paths = n.clamp(1., 1000.) as usize;
                    }
                } else {
                    let mut ignore = options.out_of_sequence == OutOfSequence::None;
                    // Written only when clicked, so OK with nothing touched
                    // leaves the stored choice exactly as it was.
                    if c.checkbox(
                        "options_ignore_external",
                        26.,
                        92.,
                        468.,
                        20.,
                        "Ignore relationships to and from other projects",
                        &mut ignore,
                        true,
                    )
                    .clicked()
                    {
                        options.out_of_sequence = if ignore {
                            OutOfSequence::None
                        } else {
                            OutOfSequence::Both
                        };
                    }
                    c.checkbox(
                        "options_open_critical",
                        26.,
                        117.,
                        465.,
                        20.,
                        "Make open-ended activities critical",
                        &mut options.open_end_uses_early,
                        true,
                    );
                    c.checkbox(
                        "options_expected_finish",
                        26.,
                        142.,
                        465.,
                        20.,
                        "Use Expected Finish Dates",
                        &mut options.use_expected_finish,
                        true,
                    );
                    c.checkbox(
                        "options_auto_schedule",
                        26.,
                        167.,
                        465.,
                        17.,
                        "Schedule automatically when a change affects dates",
                        &mut state.auto_draft,
                        true,
                    );
                    c.checkbox(
                        "options_level",
                        26.,
                        192.,
                        465.,
                        17.,
                        "Level resources during scheduling",
                        &mut options.do_level,
                        true,
                    );
                    c.checkbox(
                        "options_recalc_costs",
                        26.,
                        217.,
                        465.,
                        17.,
                        "Recalculate assignment costs after scheduling",
                        &mut state.recalc_costs_draft,
                        true,
                    );
                    for y in [238., 294., 353., 451., 507.] {
                        c.etched(17., y, 471.);
                    }
                    c.label(
                        25.,
                        246.,
                        14.,
                        "When scheduling progressed activities use",
                        true,
                    );
                    let mode = if options.retained_logic {
                        0
                    } else if options.progress_override {
                        1
                    } else {
                        2
                    };
                    for (i, (key, x, w, text)) in [
                        ("options_retained_logic", 27., 151., "Retained Logic"),
                        ("options_progress_override", 177., 185., "Progress Override"),
                        ("options_actual_dates", 361., 113., "Actual Dates"),
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        if c.radio(key, x, 268., w, 17., text, mode == i, true) {
                            options.retained_logic = i == 0;
                            options.progress_override = i == 1;
                        }
                    }
                    c.label(25., 302., 14., "Calculate start-to-start lag from", true);
                    if c.radio(
                        "options_lag_early",
                        27.,
                        324.,
                        151.,
                        17.,
                        "Early Start",
                        options.lag_from_early_start,
                        true,
                    ) {
                        options.lag_from_early_start = true;
                    }
                    if c.radio(
                        "options_lag_actual",
                        177.,
                        324.,
                        177.,
                        17.,
                        "Actual Start",
                        !options.lag_from_early_start,
                        true,
                    ) {
                        options.lag_from_early_start = false;
                    }
                    c.label(25., 361., 14., "Define critical activities as", true);
                    if c.radio(
                        "options_critical_float",
                        25.,
                        377.,
                        465.,
                        17.,
                        "Total Float less than or equal to",
                        !state.critical_longest,
                        true,
                    ) {
                        state.critical_longest = false;
                    }
                    c.edit(
                        "options_critical_hours",
                        49.,
                        401.,
                        70.,
                        21.,
                        &mut state.critical_text,
                        !state.critical_longest,
                        Align::Max,
                    );
                    if c.radio(
                        "options_critical_longest",
                        25.,
                        425.,
                        465.,
                        17.,
                        "Longest Path",
                        state.critical_longest,
                        true,
                    ) {
                        state.critical_longest = true;
                    }
                    c.label(
                        25.,
                        459.,
                        14.,
                        "Calculate float based on finish date of",
                        true,
                    );
                    c.radio(
                        "options_float_each",
                        27.,
                        481.,
                        151.,
                        17.,
                        "Each project",
                        true,
                        true,
                    );
                    c.radio(
                        "options_float_opened",
                        177.,
                        481.,
                        233.,
                        19.,
                        "Opened projects",
                        false,
                        false,
                    );
                    let rect = c.r(177., 481., 233., 19.);
                    c.ui.interact(rect, c.id("options_float_opened_tip"), Sense::hover())
                        .on_hover_text("One project is open; float uses its own finish date.");
                    c.label(25., 515., 14., "Compute Total Float as", true);
                    let mut float = match options.float_type {
                        FloatType::Start => 0,
                        FloatType::Finish => 1,
                        FloatType::Min => 2,
                    };
                    if c.combo(
                        "float_type",
                        26.,
                        535.,
                        295.,
                        22.,
                        &[
                            ("Start Float = Late Start - Early Start", true),
                            ("Finish Float = Late Finish - Early Finish", true),
                            ("Smallest of Start Float and Finish Float", true),
                        ],
                        &mut float,
                        true,
                    ) {
                        options.float_type =
                            [FloatType::Start, FloatType::Finish, FloatType::Min][float];
                    }
                    c.label(
                        25.,
                        563.,
                        14.,
                        "Calendar for scheduling Relationship Lag",
                        true,
                    );
                    let mut lag = match options.lag_calendar {
                        LagCalendar::PredCal => 0,
                        LagCalendar::SuccCal => 1,
                        LagCalendar::Hour24 => 2,
                        LagCalendar::ProjCal => 3,
                    };
                    if c.combo(
                        "lag_calendar",
                        25.,
                        579.,
                        295.,
                        22.,
                        &[
                            ("Predecessor Activity Calendar", true),
                            ("Successor Activity Calendar", true),
                            ("24 Hour Calendar", true),
                            ("Project Default Calendar", true),
                        ],
                        &mut lag,
                        true,
                    ) {
                        options.lag_calendar = [
                            LagCalendar::PredCal,
                            LagCalendar::SuccCal,
                            LagCalendar::Hour24,
                            LagCalendar::ProjCal,
                        ][lag.min(3)];
                    }
                }
                // As the reference has it: Close (the default) and Cancel
                // (both ask to save a change), then Default and Help
                // together after a gap.
                let column = crate::dialog_kit::button_column(
                    c,
                    617.,
                    &[
                        Entry::close("options_close"),
                        Entry::cancel("options_cancel"),
                        Entry::Button("options_default", "Default", true, true),
                        Entry::Button("options_help", "Help", true, false),
                    ],
                );
                let clicked = |key: &str| column.clicked.as_deref() == Some(key);
                k.close = clicked("options_close");
                k.cancel = clicked("options_cancel");
                k.default = clicked("options_default");
                k.help = clicked("options_help");
                k
            },
        );
        self.record(actions);
        self.draft_options = Some(options);
        let close_box = closed && !escape;
        if self.sched.options_save || self.sched.options_reset {
            // A question is up: the dialog under it waits.
        } else if clicks.close || clicks.cancel || close_box {
            self.press_options_close();
        } else if clicks.default {
            // Asks first; Yes puts the defaults in the dialog, saved only
            // by Close (or Cancel) and Yes.
            self.sched.options_reset = true;
        } else if clicks.help {
            self.open_help_for("options");
        }
    }

    // ---------------------------------------------------- Out of sequence

    fn sequence_dialog(&mut self, ctx: &egui::Context) {
        let Some(prompt) = self.sched.sequence.clone() else {
            return;
        };
        let state = &mut self.sched;
        let ((yes, no, view), closed, actions) = modal(
            ctx,
            "sequence",
            "Out of Sequence",
            Vec2::new(496., 118.),
            |c| {
                c.fill(0., 0., 403., 118., FACE);
                // Our own question glyph (not a reference bitmap).
                let center = c.origin + Vec2::new(33., 36.);
                c.ui.painter()
                    .circle_filled(center, 16., Color32::from_rgb(0, 84, 166));
                c.ui.painter().text(
                    center,
                    Align2::CENTER_CENTER,
                    "?",
                    FontId::new(22., egui::FontFamily::Name("Arial".into())),
                    Color32::WHITE,
                );
                let message = if prompt.status_revert.is_some() {
                    "This activity has predecessors that are not finished yet, so this \
                     progress may leave it out of sequence. View lists them. Keep the \
                     change?"
                } else {
                    "The predecessor of this relationship is not finished yet, so the \
                     change may leave the activity out of sequence. View lists its \
                     unfinished predecessors. Keep the change?"
                };
                let galley =
                    c.ui.painter()
                        .layout(message.into(), dialog_font(), INK, 300.);
                c.ui.painter()
                    .galley(c.origin + Vec2::new(68., 8.), galley, INK);
                c.checkbox(
                    "sequence_quiet",
                    21.,
                    92.,
                    318.,
                    22.,
                    "Turn this warning off",
                    &mut state.sequence_quiet_draft,
                    true,
                );
                crate::dialog_kit::strip(c, 496., 118.);
                let column = crate::dialog_kit::button_column(
                    c,
                    496.,
                    &[
                        Entry::Button("sequence_yes", "Yes", true, false),
                        Entry::Button("sequence_no", "No", true, false),
                        Entry::Button("sequence_view", "View", true, false),
                    ],
                );
                for (key, caption) in [("sequence_yes", "Yes"), ("sequence_no", "No")] {
                    if crate::kit_window::Question::OutOfSequence.is_default(caption) {
                        crate::dialog_kit::mark_default(c, key);
                    }
                }
                let clicked = |key: &str| column.clicked.as_deref() == Some(key);
                (
                    clicked("sequence_yes"),
                    clicked("sequence_no"),
                    clicked("sequence_view"),
                )
            },
        );
        self.record(actions);
        if yes {
            self.answer_sequence(true);
        } else if no || closed {
            self.answer_sequence(false);
        } else if view {
            self.sched.sequence_detail = true;
        }
    }

    fn sequence_detail_dialog(&mut self, ctx: &egui::Context) {
        let Some(prompt) = self.sched.sequence.clone() else {
            self.sched.sequence_detail = false;
            return;
        };
        let m = &self.project.model;
        // The predecessors that are not finished.
        let rows: Vec<[String; 5]> = m.activities[prompt.activity]
            .preds
            .iter()
            .filter(|&&l| m.activities[m.links[l].pred].status != StatusType::Completed)
            .map(|&l| {
                let link = &m.links[l];
                [
                    m.activities[link.pred].task_code.clone(),
                    self.project.name(link.pred).to_string(),
                    link.link_type.as_str()[3..].to_string(),
                    format::duration(
                        link.lag,
                        format::lag_periods(&self.project, link.pred, link.succ),
                    ),
                    status_label(m.activities[link.pred].status).to_string(),
                ]
            })
            .collect();
        let title = format!(
            "Predecessors of {}",
            m.activities[prompt.activity].task_code
        );
        let (close, closed, actions) =
            modal(ctx, "sequence_detail", &title, Vec2::new(638., 452.), |c| {
                c.fill(0., 0., 638., 452., FACE);
                let cols = [
                    ("Activity ID", 100.),
                    ("Activity Name", 230.),
                    ("Relationship Type", 100.),
                    ("Lag", 50.),
                    ("Predecessor Activity Status", 146.),
                ]
                .map(|(title, width)| crate::dialog_kit::Col { title, width });
                crate::dialog_kit::list(
                    c,
                    "sequence_detail_rows",
                    (4., 4., 630., 403.),
                    None,
                    &cols,
                    rows.len(),
                    None,
                    None,
                    &mut |c, r, k, rect, sel| {
                        crate::dialog_kit::cell_text(c, rect, &rows[r][k], sel, 3.);
                    },
                );
                crate::dialog_kit::button_row(
                    c,
                    634.,
                    417.,
                    &[Entry::close("sequence_detail_close")],
                )
                .clicked
                .is_some()
            });
        self.record(actions);
        if close || closed {
            self.sched.sequence_detail = false;
        }
    }

    // ------------------------------------------------------ Check Schedule

    /// The Check Schedule draft with the typed targets and hour limits
    /// committed (an hour text that is not a number keeps its last value).
    pub(crate) fn commit_check_texts(&mut self) {
        let draft = &mut self.sched.check_draft;
        for (i, text) in self.sched.check_text.iter().enumerate() {
            draft.targets[i] = text.trim().parse::<f64>().ok();
        }
        for (i, text) in self.sched.check_hours_text.iter().enumerate() {
            if let Ok(v) = text.trim().parse::<f64>() {
                match i {
                    0 => draft.long_lag_hours = v,
                    1 => draft.large_float_hours = v,
                    _ => draft.large_duration_hours = v,
                }
            }
        }
    }

    fn run_check_schedule(&mut self) {
        // Commit the parameter texts.
        let expenses = self.expense_tasks();
        let finishes = self.baseline_compare().project_finishes(&self.project);
        self.commit_check_texts();
        let draft = &self.sched.check_draft;
        let results =
            check::run_checks_with(&self.project.model, draft, &expenses, Some(&finishes));
        let loop_lines = if self.has_loops() {
            let report = self.loop_report();
            self.loop_text(&report)
        } else {
            vec![]
        };
        self.sched.check_report_written = None;
        let path = self.sched.check_report_path.trim().to_string();
        if !path.is_empty() {
            let path = PathBuf::from(&path);
            if path.exists() && !self.sched.check_overwrite {
                self.status = format!("Report not saved: {} already exists.", path.display());
            } else {
                let html = check_report_html(&self.project, &results, &loop_lines);
                match openxer_xer::write_file_atomic(&path, html.as_bytes()) {
                    Ok(()) => self.sched.check_report_written = Some(path.display().to_string()),
                    Err(error) => self.status = format!("Report not saved: {error}"),
                }
            }
        }
        self.sched.check_results = Some(results);
        self.sched.check_selected = 0;
        self.sched.check_open = false;
        if !loop_lines.is_empty() {
            self.open_loops();
        }
        self.loops.check_lines = loop_lines;
    }

    fn check_dialog(&mut self, ctx: &egui::Context) {
        #[derive(Default)]
        struct Clicks {
            cancel: bool,
            save: bool,
            run: bool,
            default: bool,
            help: bool,
        }
        let project_label = format!(
            "{} - {}",
            self.project.model.project.name,
            self.project_title()
        );
        let state = &mut self.sched;
        let (clicks, closed, actions) =
            modal(ctx, "check", "Check Schedule", Vec2::new(869., 678.), |c| {
                let mut k = Clicks::default();
                c.fill(5., 5., 740., 668., FACE);
                c.lowered(5., 5., 740., 668.);
                c.lowered(11., 11., 728., 656.);
                c.bold(27., 24., 22., "Use Check Schedule Parameters from");
                let mut project = 0;
                c.combo(
                    "project",
                    371.,
                    26.,
                    255.,
                    22.,
                    &[(project_label.as_str(), true)],
                    &mut project,
                    true,
                );
                c.bold(640., 26., 22., "Target");
                c.lowered(12., 57., 726., 550.);
                let list = c.r(13., 58., 724., 548.);
                let origin = c.origin;
                let mut actions = vec![];
                c.ui.scope_builder(egui::UiBuilder::new().max_rect(list), |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("check_rows")
                        .max_height(list.height())
                        .show(ui, |ui| {
                            let (area, _) = ui.allocate_exact_size(
                                Vec2::new(list.width() - 14., 36. * 15. + 12.),
                                Sense::hover(),
                            );
                            let mut inner = Canvas {
                                ui,
                                origin: area.min - Vec2::new(13., 58.),
                                salt: "check",
                                actions: vec![],
                            };
                            for (row, check) in CHECKS.iter().enumerate() {
                                let y = 76. - 58. + 58. + 36. * row as f32;
                                let i = *check as usize;
                                if row > 0 {
                                    inner.hline(14., y - 12., 700., Color32::WHITE);
                                }
                                let supported = check.unsupported().is_none();
                                let caption_width = if check.has_hours() { 451. } else { 585. };
                                let mut enabled = state.check_draft.enabled[i] && supported;
                                let response = inner.checkbox(
                                    &format!("check_{i}"),
                                    27.,
                                    y,
                                    caption_width,
                                    17.,
                                    check.caption(),
                                    &mut enabled,
                                    supported,
                                );
                                if supported {
                                    state.check_draft.enabled[i] = enabled;
                                } else if let Some(note) = check.unsupported() {
                                    response.on_hover_text(note);
                                }
                                if check.has_hours() {
                                    let h = match check {
                                        Check::LongLags => 0,
                                        Check::LargeFloat => 1,
                                        _ => 2,
                                    };
                                    inner.edit(
                                        &format!("check_hours_{i}"),
                                        495.,
                                        y - 1.,
                                        91.,
                                        22.,
                                        &mut state.check_hours_text[h],
                                        supported,
                                        Align::Max,
                                    );
                                    inner.label(596., y, 17., "h", supported);
                                }
                                let sign = if check.target_is_minimum() { ">" } else { "<" };
                                inner.label(618., y - 2., 17., sign, supported);
                                let (step, max) = if *check == Check::Bei {
                                    (0.01, 10.)
                                } else {
                                    (1., 100.)
                                };
                                inner.spin(
                                    &format!("check_target_{i}"),
                                    643.,
                                    y - 2.,
                                    57.,
                                    24.,
                                    &mut state.check_text[i],
                                    step,
                                    0.0..=max,
                                    supported,
                                );
                                if *check != Check::Bei {
                                    inner.label(711., y + 2., 17., "%", supported);
                                }
                            }
                            actions = inner.actions;
                        });
                });
                let _ = origin;
                c.actions.extend(actions);
                c.label(27., 617., 17., "Save Report To", true);
                c.edit(
                    "check_report_path",
                    270.,
                    614.,
                    408.,
                    22.,
                    &mut state.check_report_path,
                    true,
                    Align::Min,
                );
                let frame = c.r(270., 614., 429., 22.);
                c.sunken(
                    Rect::from_min_max(Pos2::new(frame.right() - 21., frame.top()), frame.max),
                    Color32::WHITE,
                );
                if c.ellipsis("check_report_browse", frame, true).clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("HTML", &["html", "htm"])
                        .set_file_name("ScheduleCheckReport.html")
                        .save_file()
                    {
                        state.check_report_path = path.display().to_string();
                    }
                }
                c.checkbox(
                    "check_overwrite",
                    27.,
                    640.,
                    434.,
                    17.,
                    "Overwrite Existing",
                    &mut state.check_overwrite,
                    true,
                );
                // The standard column: the committing verb, Cancel, the
                // others as a group, Help last.
                let column = crate::dialog_kit::button_column(
                    c,
                    869.,
                    &[
                        Entry::Button("check_run", "Check Schedule", true, false),
                        Entry::cancel("check_cancel"),
                        Entry::Button("check_save", "Save", true, true),
                        Entry::Button("check_default", "Default", true, false),
                        Entry::help("check_help"),
                    ],
                );
                crate::dialog_kit::mark_default(c, "check_run");
                let clicked = |key: &str| column.clicked.as_deref() == Some(key);
                k.run = clicked("check_run");
                k.cancel = clicked("check_cancel");
                k.save = clicked("check_save");
                k.default = clicked("check_default");
                k.help = clicked("check_help");
                k
            });
        self.record(actions);
        if clicks.cancel || closed {
            self.sched.check_open = false;
        } else if clicks.run {
            self.run_check_schedule();
        } else if clicks.save {
            self.commit_check_texts();
            self.sched.check_params = self.sched.check_draft.clone();
            self.status = "Check Schedule parameters kept for this session.".into();
        } else if clicks.default {
            let params = CheckParams::default();
            self.sched.check_text = target_texts(&params);
            self.sched.check_hours_text = hour_texts(&params);
            self.sched.check_draft = params;
        } else if clicks.help {
            self.open_help_for("check");
        }
    }

    /// Modeless results list with jump-to.
    fn check_results_window(&mut self, ctx: &egui::Context) {
        let Some(results) = self.sched.check_results.clone() else {
            return;
        };
        let mut open = true;
        let mut jump = None;
        let m = &self.project.model;
        crate::chrome::plain_window("Check Schedule Results")
            .id(egui::Id::new("check_results"))
            .open(&mut open)
            .default_size([760., 460.])
            .collapsible(false)
            .show(ctx, |ui| {
                if let Some(path) = &self.sched.check_report_written {
                    ui.label(format!("Report saved to {path}"));
                }
                egui::Grid::new("check_summary")
                    .striped(false)
                    .num_columns(5)
                    .spacing([14., 2.])
                    .show(ui, |ui| {
                        for header in ["Check", "Count", "Of", "Result", "Target"] {
                            ui.label(RichText::new(header).font(chrome::bold_font()));
                        }
                        ui.end_row();
                        for (i, r) in results.iter().enumerate() {
                            let label =
                                ui.selectable_label(self.sched.check_selected == i, r.check.name());
                            let r_rect = label.rect;
                            self.action_rects.insert(
                                format!("check_result_{i}"),
                                [r_rect.left(), r_rect.top(), r_rect.right(), r_rect.bottom()],
                            );
                            if label.clicked() {
                                self.sched.check_selected = i;
                            }
                            ui.label(r.items.len().to_string());
                            ui.label(r.population.to_string());
                            let value = match (r.check, r.value) {
                                (_, None) => r.note.unwrap_or("").to_string(),
                                (Check::RelationshipTypes, Some(v)) => {
                                    format!("{} % Finish to Start", number(v))
                                }
                                (_, Some(v)) => format!("{} %", number(v)),
                            };
                            let color = match r.passed {
                                Some(false) => CRITICAL,
                                Some(true) => Color32::from_rgb(0, 128, 0),
                                None => INK,
                            };
                            ui.label(RichText::new(value).color(color));
                            let sign = if r.check.target_is_minimum() {
                                ">"
                            } else {
                                "<"
                            };
                            ui.label(
                                r.target
                                    .map_or(String::new(), |t| format!("{sign} {}", number(t))),
                            );
                            ui.end_row();
                        }
                    });
                if !self.loops.check_lines.is_empty() {
                    ui.separator();
                    ui.label(RichText::new("Circular Relationships").font(chrome::bold_font()));
                    egui::ScrollArea::vertical()
                        .id_salt("check_loops")
                        .max_height(110.)
                        .show(ui, |ui| {
                            for line in &self.loops.check_lines {
                                ui.label(line);
                            }
                        });
                }
                ui.separator();
                if let Some(r) = results.get(self.sched.check_selected) {
                    ui.label(RichText::new(r.check.caption()).font(chrome::bold_font()));
                    egui::ScrollArea::vertical()
                        .id_salt("check_items")
                        .max_height(220.)
                        .show(ui, |ui| {
                            for (n, item) in r.items.iter().enumerate() {
                                let (activity, text) = match *item {
                                    Item::Activity(a) => (
                                        a,
                                        format!(
                                            "{}  {}",
                                            m.activities[a].task_code,
                                            self.project.name(a)
                                        ),
                                    ),
                                    Item::Relationship(l) => {
                                        let link = &m.links[l];
                                        (
                                            link.succ,
                                            format!(
                                                "{} → {}  {}  lag {}",
                                                m.activities[link.pred].task_code,
                                                m.activities[link.succ].task_code,
                                                &link.link_type.as_str()[3..],
                                                format::duration(
                                                    link.lag,
                                                    format::lag_periods(
                                                        &self.project,
                                                        link.pred,
                                                        link.succ
                                                    )
                                                )
                                            ),
                                        )
                                    }
                                };
                                let row =
                                    ui.selectable_label(self.selected == Some(activity), text);
                                let rr = row.rect;
                                self.action_rects.insert(
                                    format!("check_item_{n}"),
                                    [rr.left(), rr.top(), rr.right(), rr.bottom()],
                                );
                                if row.clicked() {
                                    jump = Some(activity);
                                }
                            }
                        });
                }
            });
        if let Some(activity) = jump {
            self.jump_to(activity);
        }
        if !open {
            self.sched.check_results = None;
        }
    }

    fn log_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_log;
        crate::chrome::plain_window("Schedule Log")
            .id(egui::Id::new("schedule_log_window"))
            .open(&mut open)
            .default_size([640., 480.])
            .show(ctx, |ui| {
                ui.set_min_size(Vec2::new(520., 300.));
                if self.log.is_empty() {
                    ui.label("No schedule has been run in this session.");
                }
                egui::ScrollArea::both()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let mono = FontId::monospace(11.);
                        for line in &self.log {
                            ui.label(RichText::new(expand_tabs(line)).font(mono.clone()));
                        }
                    });
            });
        self.show_log = open;
    }
}

fn default_options(current: &ScheduleOptions) -> ScheduleOptions {
    ScheduleOptions {
        retained_logic: true,
        progress_override: false,
        lag_from_early_start: true,
        use_expected_finish: true,
        do_level: false,
        lag_calendar: LagCalendar::PredCal,
        float_type: FloatType::Finish,
        out_of_sequence: OutOfSequence::Both,
        open_end_uses_early: false,
        multiple_float_paths: false,
        float_paths_use_total: true,
        max_float_paths: 10,
        float_paths_end_activity: None,
        ..current.clone()
    }
}

fn expand_tabs(line: &str) -> String {
    let mut out = String::new();
    for ch in line.chars() {
        if ch == '\t' {
            let pad = 8 - out.chars().count() % 8;
            out.extend(std::iter::repeat_n(' ', pad));
        } else {
            out.push(ch);
        }
    }
    out
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Write the log as text (CRLF, UTF-8 with BOM) or as simple HTML. The
/// file is written whole through a temporary file (an append rewrites the
/// old text with the new after it), so a failed write leaves the log that
/// was there.
pub(crate) fn write_log(
    path: &Path,
    log: &[String],
    html: bool,
    append: bool,
) -> Result<(), String> {
    let body = if html {
        let mut s = String::from(
            "<!DOCTYPE html>\r\n<html><head><meta charset=\"utf-8\"><title>Schedule Log</title></head><body><pre>\r\n",
        );
        for line in log {
            s.push_str(&html_escape(&expand_tabs(line)));
            s.push_str("\r\n");
        }
        s.push_str("</pre></body></html>\r\n");
        s
    } else {
        let mut s = String::new();
        for line in log {
            s.push_str(line);
            s.push_str("\r\n");
        }
        s
    };
    let mut bytes = if append {
        match std::fs::read(path) {
            Ok(old) => old,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => vec![],
            Err(e) => return Err(format!("{}: {e}", path.display())),
        }
    } else {
        vec![]
    };
    if !html && bytes.is_empty() {
        bytes.extend_from_slice("\u{feff}".as_bytes());
    }
    bytes.extend_from_slice(body.as_bytes());
    openxer_xer::write_file_atomic(path, &bytes).map_err(|e| format!("{}: {e}", path.display()))
}

fn leader(label: &str, value: impl std::fmt::Display) -> String {
    let dots = 65usize.saturating_sub(label.chars().count()).max(1);
    format!("\t{label}{}{value}", ".".repeat(dots))
}

fn yes_no(v: bool) -> &'static str {
    if v {
        "Yes"
    } else {
        "No"
    }
}

/// The schedule log in the reference report layout.
pub(crate) fn build_log(
    project: &ProjectData,
    title: &str,
    default_project: &str,
    (critical_longest, critical_hours): (bool, f64),
    (recalc_costs, auto_schedule): (bool, bool),
    started: chrono::NaiveDateTime,
) -> Vec<String> {
    let m = &project.model;
    let o = &m.options;
    let origin = m.origin_serial;
    let short = &m.project.name;
    let mut log = vec![];
    let heading = format!(
        "Scheduling/Leveling Report - {} - OpenXER",
        started.format("%Y-%m-%d %H:%M:%S")
    );
    log.push(heading.clone());
    log.push("=".repeat(heading.chars().count()));
    log.push(String::new());
    log.push(leader("Default Project", default_project));
    log.push(leader("Use scheduling options from", default_project));
    log.push(String::new());
    log.push("\tProjects: ".into());
    log.push(format!(
        "\t\t{short}{}{title}",
        ".".repeat(65usize.saturating_sub(short.chars().count()).max(1))
    ));
    log.push(String::new());
    log.push("Scheduling/Leveling Settings:".into());
    log.push("-----------------------------".into());
    log.push("\tGeneral".into());
    log.push("\t-------".into());
    log.push(leader("Scheduling ", "Yes"));
    log.push(leader("Leveling ", yes_no(o.do_level)));
    log.push(leader(
        "Ignore relationships to and from other projects ",
        yes_no(o.out_of_sequence == OutOfSequence::None),
    ));
    log.push(leader(
        "Make open-ended activities critical ",
        yes_no(o.open_end_uses_early),
    ));
    log.push(leader(
        "Use Expected Finish Dates ",
        yes_no(o.use_expected_finish),
    ));
    log.push(leader(
        "Schedule automatically when a change affects dates ",
        yes_no(auto_schedule),
    ));
    log.push(leader(
        "Level resources during scheduling ",
        yes_no(o.do_level),
    ));
    log.push(leader(
        "Recalculate assignment costs after scheduling ",
        yes_no(recalc_costs),
    ));
    log.push(leader(
        "When scheduling progressed activities use ",
        if o.retained_logic {
            "Retained Logic"
        } else if o.progress_override {
            "Progress Override"
        } else {
            "Actual Dates"
        },
    ));
    log.push(leader(
        "Calculate start-to-start lag from ",
        if o.lag_from_early_start {
            "Early Start"
        } else {
            "Actual Start"
        },
    ));
    if critical_longest {
        log.push(leader("Define critical activities as ", "Longest Path"));
    } else {
        log.push(leader(
            "Define critical activities as Total Float less than or equal to ",
            number(critical_hours),
        ));
    }
    log.push(leader(
        "Compute Total Float As ",
        match o.float_type {
            FloatType::Start => "Start Float",
            FloatType::Finish => "Finish Float",
            FloatType::Min => "Smallest of Start Float and Finish Float",
        },
    ));
    log.push(leader(
        "Calculate float based on finish date of ",
        "Each project",
    ));
    log.push(leader(
        "Calendar for scheduling Relationship Lag ",
        match o.lag_calendar {
            LagCalendar::PredCal => "Predecessor Activity Calendar",
            LagCalendar::SuccCal => "Successor Activity Calendar",
            LagCalendar::Hour24 => "24 Hour Calendar",
            LagCalendar::ProjCal => "Project Default Calendar",
        },
    ));
    log.push(String::new());
    log.push("\tAdvanced".into());
    log.push("\t--------".into());
    log.push(leader(
        "Calculate multiple float paths",
        yes_no(o.multiple_float_paths),
    ));
    if o.multiple_float_paths {
        log.push(leader(
            "Calculate multiple paths using",
            if o.float_paths_use_total {
                "Total Float"
            } else {
                "Free Float"
            },
        ));
        let end = o
            .float_paths_end_activity
            .and_then(|id| m.activities.iter().find(|a| a.task_id == id))
            .map_or(String::new(), |a| a.task_code.clone());
        log.push(leader(
            "Display multiple float paths ending with activity",
            end,
        ));
        log.push(leader("Number of paths to calculate", o.max_float_paths));
    }
    log.push(String::new());
    let entry = |i: usize| {
        format!(
            "\t\tProject:\t{short}\tActivity:\t{}\t{}",
            m.activities[i].task_code,
            project.name(i)
        )
    };
    let count = |s: StatusType| m.activities.iter().filter(|a| a.status == s).count();
    log.push("Statistics:".into());
    log.push("-----------".into());
    log.push(leader("# Projects", 1));
    log.push(leader("# Activities", m.activities.len()));
    log.push(leader("# Not Started", count(StatusType::NotStarted)));
    log.push(leader("# In Progress", count(StatusType::InProgress)));
    log.push(leader("# Completed", count(StatusType::Completed)));
    log.push(leader("# Relationships", m.links.len()));
    let constrained = check::constrained(m);
    log.push(leader("# Activities with Constraint", constrained.len()));
    log.extend(constrained.into_iter().map(entry));
    log.push(String::new());
    log.push("Errors:".into());
    log.push("-------".into());
    log.push("Warnings:".into());
    log.push("---------".into());
    let section = |log: &mut Vec<String>, label: &str, items: Vec<usize>| {
        log.push(leader(label, items.len()));
        log.extend(items.into_iter().map(entry));
        log.push(String::new());
    };
    section(
        &mut log,
        "Activities without predecessors",
        check::without_predecessors(m),
    );
    section(
        &mut log,
        "Activities without successors",
        check::without_successors(m),
    );
    section(
        &mut log,
        "Out-of-sequence activities",
        check::out_of_sequence_activities(m),
    );
    section(
        &mut log,
        "Activities with Actual Dates > Data Date",
        check::actuals_after_data_date(m),
    );
    section(
        &mut log,
        "Milestone Activities with atypical relationships",
        check::atypical_milestones(m),
    );
    let pairs = check::finish_milestone_calendar_mismatches(m);
    log.push(leader(
        "Finish milestone and predecessors have different calendars",
        pairs.len(),
    ));
    let calendar = |i: usize| {
        project
            .calendar_names
            .get(m.activities[i].cal)
            .cloned()
            .unwrap_or_default()
    };
    for l in pairs {
        let link = &m.links[l];
        log.push(format!(
            "\t\tProject:\t{short}\tActivity:\t{}\t{}\tCalendar:\t{}\tPredecessor Project:\t{short}\tPredecessor Activity:\t{}\t{}\tPredecessor Activity Calendar:\t{}",
            m.activities[link.succ].task_code,
            project.name(link.succ),
            calendar(link.succ),
            m.activities[link.pred].task_code,
            project.name(link.pred),
            calendar(link.pred),
        ));
    }
    log.push(String::new());
    log.push("Scheduling/Leveling Results:".into());
    log.push("----------------------------".into());
    log.push(leader("# Projects Scheduled/Leveled", 1));
    log.push(leader("# Activities Scheduled/Leveled", m.activities.len()));
    log.push(leader(
        "Data Date",
        display_date(origin, m.project.data_date),
    ));
    log.push(leader(
        "Earliest Early Start Date",
        display_date(
            origin,
            m.activities.iter().filter_map(|a| a.early_start).min(),
        ),
    ));
    log.push(leader(
        "Latest Early Finish Date",
        display_date(
            origin,
            m.activities.iter().filter_map(|a| a.early_finish).max(),
        ),
    ));
    log.push("Exceptions:".into());
    log.push("-----------".into());
    let threshold = (critical_hours * 60.).round() as i64;
    section(
        &mut log,
        "Critical Activities",
        check::critical_activities(m, critical_longest, threshold),
    );
    log
}

/// A self-contained HTML report of the Check Schedule results.
/// The report with the circular relationships (one text line each) listed
/// after the checks.
pub(crate) fn check_report_html(
    project: &ProjectData,
    results: &[CheckResult],
    loops: &[String],
) -> String {
    let m = &project.model;
    let mut s = String::from(
        "<!DOCTYPE html>\n<html><head><meta charset=\"utf-8\"><title>Check Schedule</title>\
         <style>body{font:13px Arial,sans-serif}table{border-collapse:collapse}\
         td,th{border:1px solid #a0a0a0;padding:2px 6px;text-align:left}\
         .fail{color:#c00}.pass{color:#080}</style></head><body>\n",
    );
    s.push_str(&format!(
        "<h1>Check Schedule - {}</h1>\n<p>Data Date: {}</p>\n<table><tr><th>Check</th><th>Count</th><th>Of</th><th>Result</th><th>Target</th></tr>\n",
        html_escape(&m.project.name),
        display_date(m.origin_serial, m.project.data_date)
    ));
    for r in results {
        let class = match r.passed {
            Some(true) => "pass",
            Some(false) => "fail",
            None => "",
        };
        let sign = if r.check.target_is_minimum() {
            "&gt;"
        } else {
            "&lt;"
        };
        s.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td class=\"{class}\">{}</td><td>{}</td></tr>\n",
            html_escape(r.check.caption()),
            r.items.len(),
            r.population,
            r.value.map_or_else(
                || html_escape(r.note.unwrap_or("")),
                |v| format!("{} %", number(v))
            ),
            r.target
                .map_or(String::new(), |t| format!("{sign} {}", number(t)))
        ));
    }
    s.push_str("</table>\n");
    for r in results.iter().filter(|r| !r.items.is_empty()) {
        s.push_str(&format!("<h2>{}</h2>\n<ul>\n", html_escape(r.check.name())));
        for item in &r.items {
            let text = match *item {
                Item::Activity(a) => format!("{} {}", m.activities[a].task_code, project.name(a)),
                Item::Relationship(l) => {
                    let link = &m.links[l];
                    format!(
                        "{} to {} ({}, lag {})",
                        m.activities[link.pred].task_code,
                        m.activities[link.succ].task_code,
                        &link.link_type.as_str()[3..],
                        format::duration(
                            link.lag,
                            format::lag_periods(project, link.pred, link.succ)
                        )
                    )
                }
            };
            s.push_str(&format!("<li>{}</li>\n", html_escape(&text)));
        }
        s.push_str("</ul>\n");
    }
    if !loops.is_empty() {
        s.push_str("<h2>Circular Relationships</h2>\n<pre>\n");
        for line in loops {
            s.push_str(&html_escape(line));
            s.push('\n');
        }
        s.push_str("</pre>\n");
    }
    s.push_str("</body></html>\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_dates_round_trip() {
        let origin = 46_000;
        let minutes = 40 * DAY + 6 * 60;
        let text = display_date(origin, Some(minutes));
        assert_eq!(parse_display_date(origin, &text).unwrap(), minutes);
        assert_eq!(
            parse_display_date(origin, &openxer_xer::date_string(origin, Some(minutes))).unwrap(),
            minutes
        );
        assert!(parse_display_date(origin, "not a date").is_err());
    }

    #[test]
    fn log_uses_dot_leaders_and_lists_warnings() {
        let mut project = openxer_xer::starter();
        schedule_model(&mut project.model, &project.resources).unwrap();
        let started = chrono::NaiveDate::from_ymd_opt(2026, 11, 4)
            .unwrap()
            .and_hms_opt(9, 30, 0)
            .unwrap();
        let log = build_log(
            &project,
            "Riverbend",
            &project.model.project.name,
            (false, 0.),
            (false, false),
            started,
        );
        assert_eq!(
            log[0],
            "Scheduling/Leveling Report - 2026-11-04 09:30:00 - OpenXER"
        );
        let line = log
            .iter()
            .find(|l| l.starts_with("\tDefault Project"))
            .unwrap();
        assert_eq!(line.find(&project.model.project.name).unwrap(), 66);
        assert!(log.iter().any(|l| l
            .starts_with("\tDefine critical activities as Total Float less than or equal to .0")));
        for heading in [
            "Statistics:",
            "Warnings:",
            "Scheduling/Leveling Results:",
            "Exceptions:",
        ] {
            assert!(log.iter().any(|l| l == heading), "{heading}");
        }
        let missing = check::without_predecessors(&project.model).len();
        assert!(log
            .iter()
            .any(|l| *l == leader("Activities without predecessors", missing)));
    }

    fn single_app(name: &str) -> (App, egui::Context) {
        let dir = std::env::temp_dir().join(format!("openxer-sched-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let store = crate::store::Store::open(dir.join("workspace.db")).unwrap();
        let ctx = egui::Context::default();
        ctx.set_pixels_per_point(1.);
        let mut project = openxer_xer::starter();
        schedule_model(&mut project.model, &project.resources).unwrap();
        (App::new(&ctx, store, project), ctx)
    }

    /// The status bar count after F9 follows the project's own critical
    /// definition (total float threshold or longest path), not float <= 0.
    #[test]
    fn status_critical_count_uses_the_projects_definition() {
        let (mut app, _ctx) = single_app("critical");
        let open = |a: &Activity| !a.is_completed();
        app.set_project_value("critical_drtn_hr_cnt", "80".into());
        app.schedule();
        let m = &app.project.model;
        let by_float = |limit: i64| {
            m.activities
                .iter()
                .filter(|a| open(a) && a.total_float.is_some_and(|f| f <= limit))
                .count()
        };
        let zero = by_float(0);
        let eighty = by_float(80 * 60);
        assert_ne!(zero, eighty, "the starter needs float between 0 and 80h");
        assert!(
            app.status
                .starts_with(&format!("Schedule complete · {eighty} critical activities")),
            "{}",
            app.status
        );
        app.set_project_value("critical_path_type", "CT_DrivPath".into());
        app.schedule();
        let m = &app.project.model;
        let longest = m
            .activities
            .iter()
            .filter(|a| open(a) && a.driving_path)
            .count();
        assert!(
            app.status.starts_with(&format!(
                "Schedule complete · {longest} critical activities"
            )),
            "{}",
            app.status
        );
    }

    /// The log reports the real "Schedule automatically" setting.
    #[test]
    fn log_reports_the_automatic_scheduling_setting() {
        let (mut app, _ctx) = single_app("auto");
        let line = |app: &App, v: &str| {
            app.log
                .iter()
                .any(|l| *l == leader("Schedule automatically when a change affects dates ", v))
        };
        app.schedule();
        assert!(line(&app, "No"), "{:?}", app.log);
        app.set_auto_schedule(true);
        app.schedule();
        assert!(line(&app, "Yes"), "{:?}", app.log);
        app.set_auto_schedule(false);
    }

    /// Check Schedule > Save keeps edited hour limits as well as targets.
    #[test]
    fn check_schedule_save_keeps_hour_limits() {
        let (mut app, ctx) = single_app("check_save");
        let mut painter = crate::headless_render::SoftwarePainter::default();
        let mut time = 0.;
        for _ in 0..2 {
            let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
        }
        app.open_check_schedule();
        for _ in 0..3 {
            let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
        }
        app.sched.check_hours_text = ["99".into(), "123".into(), "456".into()];
        app.sched.check_text[0] = "7".into();
        painter.click_action(&mut app, &ctx, &mut time, "check_save");
        assert_eq!(
            app.status,
            "Check Schedule parameters kept for this session."
        );
        assert_eq!(app.sched.check_hours_text[0], "99");
        let kept = &app.sched.check_params;
        assert_eq!(kept.long_lag_hours, 99.);
        assert_eq!(kept.large_float_hours, 123.);
        assert_eq!(kept.large_duration_hours, 456.);
        assert_eq!(kept.targets[0], Some(7.));
        // Reopening shows the saved limits.
        painter.click_action(&mut app, &ctx, &mut time, "check_cancel");
        app.open_check_schedule();
        assert_eq!(app.sched.check_hours_text[0], "99");
    }

    /// Bulk Assign checks every new relationship for out-of-sequence
    /// progress, not only the last one, and asks about each in turn.
    #[test]
    fn bulk_links_check_every_new_link_for_out_of_sequence() {
        let (mut app, _ctx) = single_app("bulk_sequence");
        app.editing.options.warn_out_of_sequence = true;
        let n = app.project.model.activities.len();
        let (started, plain, other) = (n - 3, n - 2, 0);
        {
            let dd = app.project.model.project.data_date;
            let a = &mut app.project.model.activities[started];
            a.act_start = dd;
            a.status = StatusType::InProgress;
            for i in [plain, other] {
                let a = &app.project.model.activities[i];
                assert!(a.act_start.is_none() && a.act_finish.is_none(), "{i}");
            }
        }
        let first_new = app.project.model.links.len();
        // `other` becomes the predecessor of both; only the first new link
        // (to the started activity) is out of sequence.
        app.link_bulk(&[started, plain], other, true, LinkType::FS, 0, 1.);
        assert_eq!(
            app.project.model.links.len(),
            first_new + 2,
            "{}",
            app.status
        );
        let prompt = app
            .sched
            .sequence
            .as_ref()
            .expect("asked about the first link");
        assert_eq!(prompt.activity, started);
        assert_eq!(prompt.link, first_new);
        // No removes that link; nothing else is left to ask.
        app.answer_sequence(false);
        assert!(app.sched.sequence.is_none());
        assert_eq!(app.project.model.links.len(), first_new + 1);
        assert_eq!(app.project.model.links[first_new].succ, plain);
    }

    #[test]
    fn log_files_overwrite_append_and_html() {
        let dir = std::env::temp_dir().join(format!("openxer-log-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let text = dir.join("log.txt");
        let lines = vec!["one\t<two>".to_string()];
        write_log(&text, &lines, false, false).unwrap();
        write_log(&text, &lines, false, true).unwrap();
        let content = std::fs::read_to_string(&text).unwrap();
        assert_eq!(content, "\u{feff}one\t<two>\r\none\t<two>\r\n");
        write_log(&text, &lines, false, false).unwrap();
        assert_eq!(
            std::fs::read_to_string(&text).unwrap(),
            "\u{feff}one\t<two>\r\n"
        );
        let html = dir.join("log.html");
        write_log(&html, &lines, true, false).unwrap();
        assert!(std::fs::read_to_string(&html)
            .unwrap()
            .contains("one     &lt;two&gt;"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
