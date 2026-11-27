use super::*;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};

pub(crate) struct SingleScheduleJob {
    pub project: ProjectData,
    pub recalc: bool,
    pub op: &'static str,
}

pub(crate) struct SingleScheduleOut {
    pub project: ProjectData,
    pub held: Vec<i64>,
    pub recalculated: Option<crate::schedule_dialogs::progress_tools::RecalcReport>,
    pub elapsed: f64,
}

pub(crate) fn log_start_single(diag: &crate::diag_log::DiagLog, op: &str, p: &ProjectData) {
    let o = &p.model.options;
    let rsrcs = p.document.table("RSRC").map_or(0, |t| t.rows.len());
    diag.line(&format!(
        "{op} state=start open_projects=1 multi=false do_level={} multiple_float_paths={} float_paths_use_total={} max_float_paths={} open_end_uses_early={} has_plan_end={} activities={} links={} resources={}",
        o.do_level,
        o.multiple_float_paths,
        o.float_paths_use_total,
        o.max_float_paths,
        o.open_end_uses_early,
        p.model.project.plan_end.is_some(),
        p.model.activities.len(),
        p.model.links.len(),
        rsrcs,
    ));
}

pub(crate) fn run_single_schedule_job(
    mut job: SingleScheduleJob,
    progress: Option<&Mutex<crate::busy::Progress>>,
    cancel: &AtomicBool,
    diag: &crate::diag_log::DiagLog,
) -> Option<Result<SingleScheduleOut, String>> {
    let op = job.op;
    let total_start = Instant::now();
    let cancelled = || cancel.load(Ordering::Relaxed);
    let report = |phase: &str, fraction: f32| {
        if let Some(p) = progress {
            if let Ok(mut g) = p.lock() {
                g.phase = phase.to_string();
                g.fraction = Some(fraction);
            }
        }
    };

    if cancelled() {
        return None;
    }
    report("Refreshing derived durations...", 0.10);
    diag.phase_start(op, "refresh_derived_durations");
    let t = Instant::now();
    job.project.refresh_derived_durations();
    diag.phase_done(op, "refresh_derived_durations", t.elapsed().as_millis());
    if cancelled() {
        return None;
    }

    report("Validating schedule...", 0.22);
    diag.phase_start(op, "validate");
    let t = Instant::now();
    if let Err(e) = job.project.model.validate() {
        diag.phase_done(op, "validate", t.elapsed().as_millis());
        diag.error(op, &e);
        return Some(Err(e));
    }
    diag.phase_done(op, "validate", t.elapsed().as_millis());
    if cancelled() {
        return None;
    }

    let phase_label = if job.project.model.options.do_level {
        "Scheduling and leveling..."
    } else {
        "Scheduling activities..."
    };
    report(phase_label, 0.38);
    diag.phase_start(op, "schedule_model_report");
    let t = Instant::now();
    let held = match scheduler::schedule_with_resources_report(
        &mut job.project.model,
        &job.project.resources,
    ) {
        Ok(held) => held,
        Err(e) => {
            diag.phase_done(op, "schedule_model_report", t.elapsed().as_millis());
            diag.error(op, &e);
            return Some(Err(e));
        }
    };
    diag.phase_done(op, "schedule_model_report", t.elapsed().as_millis());
    if cancelled() {
        return None;
    }

    report("Updating assignment dates...", 0.78);
    diag.phase_start(op, "write_assignment_schedule");
    let t = Instant::now();
    job.project.write_assignment_schedule();
    diag.phase_done(op, "write_assignment_schedule", t.elapsed().as_millis());
    if cancelled() {
        return None;
    }

    let recalculated = if job.recalc {
        report("Recalculating assignment costs...", 0.88);
        diag.phase_start(op, "recalculate_assignment_costs");
        let t = Instant::now();
        let pid = job.project.model.project.proj_id;
        let rep = crate::schedule_dialogs::progress_tools::recalculate_assignment_costs(
            &mut job.project.document,
            pid,
            false,
            false,
        );
        diag.phase_done(op, "recalculate_assignment_costs", t.elapsed().as_millis());
        if cancelled() {
            return None;
        }
        Some(rep)
    } else {
        None
    };

    report("Finishing...", 0.96);
    let origin = job.project.model.origin_serial;
    job.project.model.project.last_schedule = Some(
        chrono::Local::now()
            .naive_local()
            .signed_duration_since(
                chrono::NaiveDate::from_ymd_opt(1899, 12, 30)
                    .unwrap()
                    .and_hms_opt(0, 0, 0)
                    .unwrap(),
            )
            .num_minutes()
            - origin * DAY,
    );

    Some(Ok(SingleScheduleOut {
        project: job.project,
        held,
        recalculated,
        elapsed: total_start.elapsed().as_secs_f64(),
    }))
}

impl App {
    pub(crate) fn commit_single_schedule_output(
        &mut self,
        out: SingleScheduleOut,
        op: &str,
        close_schedule_dialog: bool,
        write_log_file: bool,
    ) {
        let diag = self.diag_log();
        self.project = out.project;
        self.sched.progress.manual_level = false;
        self.sched.progress.left_over = out.held;
        note_scheduled(&self.project.model);
        // TASKRSRC dates changed: a new records revision keeps undo/redo
        // snapshots of the assignment rows in step with the schedule.
        self.touch_records();
        self.schedule_runs += 1;
        self.dirty = true;
        self.date_edits.clear();
        self.data_date = date_string(
            self.project.model.origin_serial,
            self.project.model.project.data_date,
        );
        self.must_finish = date_string(
            self.project.model.origin_serial,
            self.project.model.project.plan_end,
        );
        let critical = crate::schedule_dialogs::critical_count(&self.project);
        diag.phase_start(op, "schedule_log");
        let t = Instant::now();
        self.log = self.schedule_log(out.elapsed);
        if let Some(report) = out.recalculated {
            self.log.push(format!(
                "\tAssignment costs recalculated: {} updated, {} unchanged, {} skipped",
                report.updated,
                report.unchanged,
                report.skipped.len()
            ));
        }
        diag.phase_done(op, "schedule_log", t.elapsed().as_millis());
        self.status = format!("Schedule complete · {critical} critical activities");
        if close_schedule_dialog {
            self.sched.schedule_open = false;
        }
        self.show_schedule = false;
        self.draft_options = None;
        if write_log_file && self.sched.log_to_file {
            if let Err(error) = crate::schedule_dialogs::write_log(
                Path::new(&self.sched.log_path),
                &self.log,
                self.sched.log_html,
                self.sched.log_append,
            ) {
                self.status = format!("Fail to create log file: {error}  Log disabled.");
            }
        }
        diag.phase_start(op, "save");
        let t = Instant::now();
        self.save();
        diag.phase_done(op, "save", t.elapsed().as_millis());
        if !self.dirty && !self.status.starts_with("Fail to create log file") {
            self.status = format!("Schedule complete · {critical} critical activities · Saved");
        }
        diag.line(&format!(
            "{op} state=done elapsed_ms={} critical={critical}",
            (out.elapsed * 1000.0).round() as u128
        ));
    }

    #[cfg(test)]
    pub(super) fn schedule(&mut self) {
        if let Some((activity, field)) = self.edit_cell.take() {
            if !self.apply_cell(activity, field, &self.edit_text.clone()) {
                return;
            }
        }
        self.data_date = date_string(
            self.project.model.origin_serial,
            self.project.model.project.data_date,
        );
        self.must_finish = date_string(
            self.project.model.origin_serial,
            self.project.model.project.plan_end,
        );
        let origin = self.project.model.origin_serial;
        let date = match openxer_xer::parse_relative(origin, &self.data_date) {
            Ok(Some(d)) => d,
            Ok(None) => {
                self.diag_log().error("schedule", "Enter a data date");
                self.error("Enter a data date".into());
                return;
            }
            Err(e) => {
                self.diag_log().error("schedule", &e);
                self.error(e);
                return;
            }
        };
        let finish = match openxer_xer::parse_relative(origin, &self.must_finish) {
            Ok(d) => d,
            Err(e) => {
                self.diag_log().error("schedule", &e);
                self.error(e);
                return;
            }
        };
        let diag = self.diag_log();
        diag.phase_start("schedule", "intercept_loops");
        let t = Instant::now();
        let has_loops = self.intercept_loops();
        diag.phase_done("schedule", "intercept_loops", t.elapsed().as_millis());
        if has_loops {
            return;
        }
        self.level_hook_before_schedule();
        if self.open_set.multi() {
            self.schedule_open_projects(date, finish);
            return;
        }
        let mut next = self.project.clone();
        next.model.project.data_date = Some(date);
        next.model.project.plan_end = finish;
        let recalc = self.recalc_costs_after_scheduling()
            || (next.model.options.do_level && self.recalc_costs_after_leveling());
        log_start_single(&diag, "schedule", &next);
        let cancel = AtomicBool::new(false);
        let job = SingleScheduleJob {
            project: next,
            recalc,
            op: "schedule",
        };
        match run_single_schedule_job(job, None, &cancel, &diag) {
            Some(Ok(out)) => self.commit_single_schedule_output(out, "schedule", false, false),
            Some(Err(e)) => self.error(e),
            None => {}
        }
    }

    /// One run of the engine on the project with its stored options: the
    /// data date and must finish date given (or kept), assignment dates
    /// written, the last scheduled date stamped. Returns the number of
    /// critical activities. F9 and automatic scheduling both come here, so
    /// they give the same dates.
    fn run_engine(
        &mut self,
        date: Option<i64>,
        finish: Option<i64>,
    ) -> Result<
        (
            usize,
            Option<crate::schedule_dialogs::progress_tools::RecalcReport>,
        ),
        String,
    > {
        let origin = self.project.model.origin_serial;
        let mut next = self.project.clone();
        if date.is_some() {
            next.model.project.data_date = date;
        }
        next.model.project.plan_end = finish;
        next.refresh_derived_durations();
        let held = next
            .model
            .validate()
            .and_then(|_| schedule_model_report(&mut next.model, &next.resources))?;
        next.write_assignment_schedule();
        // Schedule Options: recalculate assignment costs after scheduling;
        // Level Resources: and after a run that leveled.
        let recalc = self.recalc_costs_after_scheduling()
            || (next.model.options.do_level && self.recalc_costs_after_leveling());
        let recalculated = recalc.then(|| {
            let pid = next.model.project.proj_id;
            crate::schedule_dialogs::progress_tools::recalculate_assignment_costs(
                &mut next.document,
                pid,
                false,
                false,
            )
        });
        next.model.project.last_schedule = Some(
            chrono::Local::now()
                .naive_local()
                .signed_duration_since(
                    chrono::NaiveDate::from_ymd_opt(1899, 12, 30)
                        .unwrap()
                        .and_hms_opt(0, 0, 0)
                        .unwrap(),
                )
                .num_minutes()
                - origin * DAY,
        );
        self.project = next;
        self.sched.progress.left_over = held;
        // TASKRSRC dates changed: a new records revision keeps undo/redo
        // snapshots of the assignment rows in step with the schedule.
        self.touch_records();
        self.schedule_runs += 1;
        self.dirty = true;
        self.date_edits.clear();
        let critical = crate::schedule_dialogs::critical_count(&self.project);
        Ok((critical, recalculated))
    }

    /// Schedule Options, "Schedule automatically when a change affects
    /// dates": called once per frame before the undo history records it.
    /// When this frame changed something the engine reads (durations,
    /// logic, progress, constraints, calendars, options, data date), the
    /// stored schedule runs once, so the edit and its new dates are one undo
    /// step. Undo and redo restore a recorded state and do not trigger it.
    /// Row order follows Disable Auto-Reorganization like any edit.
    pub(super) fn schedule_after_edit(&mut self) -> bool {
        self.refresh_auto_schedule();
        if !self.sched.auto_schedule || self.open_set.multi() {
            return false;
        }
        if self
            .history
            .is_current(&self.project, self.records_revision)
        {
            return true;
        }
        if same_schedule_inputs(self.history.latest_model(), &self.project.model) {
            return false;
        }
        self.schedule_automatically();
        false
    }

    /// A bulk edit (paste, fill down, link, delete...) about to be recorded
    /// as one undo step: with automatic scheduling on and the engine's
    /// inputs changed from `before`, the schedule runs first so it is part
    /// of the same step.
    pub(super) fn schedule_bulk_edit(&mut self, before: &Model) {
        self.refresh_auto_schedule();
        if self.sched.auto_schedule
            && !self.open_set.multi()
            && !same_schedule_inputs(before, &self.project.model)
        {
            self.schedule_automatically();
        }
    }

    fn schedule_automatically(&mut self) {
        self.level_hook_before_schedule();
        let finish = self.project.model.project.plan_end;
        if let Err(e) = self.run_engine(None, finish) {
            self.status = format!("Not scheduled automatically: {e}");
        }
    }

    pub(super) fn open_options(&mut self) {
        // With several projects open, F9 uses the Default Project's options.
        if self.open_set.multi() {
            self.activate_default();
        }
        let mut options = self.project.model.options.clone();
        repair_float_paths(&mut options, &self.project.model);
        self.draft_options = Some(options);
        self.level_hook_options_opened();
        self.open_options_drafts();
        self.show_schedule = true;
    }
    pub(super) fn apply_options(&mut self) {
        if let Some(mut options) = self.draft_options.take() {
            repair_float_paths(&mut options, &self.project.model);
            self.project.model.options = options;
            self.level_hook_options_applied();
        }
        self.dirty = true;
        self.save();
        self.show_schedule = false;
    }
    /// Close on Schedule Options with nothing changed: a stored float-path
    /// setting the engine refuses, which the dialog showed repaired, is
    /// written back as shown; nothing else is saved.
    pub(super) fn keep_repaired_float_paths(&mut self) {
        let mut options = self.project.model.options.clone();
        repair_float_paths(&mut options, &self.project.model);
        if options != self.project.model.options {
            self.project.model.options = options;
            self.dirty = true;
            self.save();
        }
    }
}
/// Schedule Options never keeps a float-path setting the engine refuses: an
/// ending activity the project does not hold is blank and the number of
/// paths is at least one, whichever tab was drawn.
fn repair_float_paths(options: &mut ScheduleOptions, model: &Model) {
    if options
        .float_paths_end_activity
        .is_some_and(|id| !model.activities.iter().any(|a| a.task_id == id))
    {
        options.float_paths_end_activity = None;
    }
    options.max_float_paths = options.max_float_paths.max(1);
}

/// Whether two models give the same schedule: every field the engine
/// reads is equal (results such as early dates and float are not compared).
pub(super) fn same_schedule_inputs(a: &Model, b: &Model) -> bool {
    fn activity(a: &Activity) -> impl PartialEq + '_ {
        (
            (a.task_id, a.task_type, a.status, a.act_start, a.act_finish),
            (
                a.resume,
                a.expect_end,
                a.cstr,
                a.cstr_date,
                a.cstr2,
                a.cstr_date2,
            ),
            (
                a.rem,
                a.cal,
                a.ext_early_start,
                a.ext_late_end,
                &a.preds,
                &a.succs,
            ),
            a.assignments
                .iter()
                .map(|s| (s.rsrc_id, s.rollup, s.rem, s.remain_lag, s.relag, s.cal))
                .collect::<Vec<_>>(),
        )
    }
    a.origin_serial == b.origin_serial
        && a.project_calendar == b.project_calendar
        && a.options == b.options
        && a.calendars == b.calendars
        && (
            a.project.data_date,
            a.project.plan_start,
            a.project.plan_end,
        ) == (
            b.project.data_date,
            b.project.plan_start,
            b.project.plan_end,
        )
        && a.links.len() == b.links.len()
        && a.links.iter().zip(&b.links).all(|(x, y)| {
            (x.pred, x.succ, x.link_type, x.lag) == (y.pred, y.succ, y.link_type, y.lag)
        })
        && a.activities.len() == b.activities.len()
        && a.activities
            .iter()
            .zip(&b.activities)
            .all(|(x, y)| activity(x) == activity(y))
}

pub(super) fn schedule_model(
    model: &mut Model,
    inputs: &openxer_engine::features::LevelingInputs,
) -> Result<(), String> {
    schedule_model_report(model, inputs).map(|_| ())
}

/// Schedule, leveling when the options say so. Returns the IDs of the
/// activities leveling left over-allocated because their float would not
/// allow a delay. Resource Dependent activities work on their resources'
/// calendars with or without leveling.
pub(super) fn schedule_model_report(
    model: &mut Model,
    inputs: &openxer_engine::features::LevelingInputs,
) -> Result<Vec<i64>, String> {
    let held = scheduler::schedule_with_resources_report(model, inputs)?;
    note_scheduled(model);
    Ok(held)
}

/// The schedule inputs each project had when the engine last ran on it,
/// keyed by project ID and name, for the "schedule needed" status panel.
/// Every desktop schedule run (F9, automatic, leveling, several projects)
/// comes through `schedule_model_report`, which records them here.
#[derive(Default)]
struct LastRuns {
    models: std::collections::HashMap<(i64, String), Model>,
    /// Bumped on every recorded run, so a cached answer is not reused.
    generation: u64,
    /// The last answer: project key, history changes, generation, needed.
    answer: Option<((i64, String), u64, u64, bool)>,
}

thread_local! {
    static LAST_RUNS: std::cell::RefCell<LastRuns> = Default::default();
}

fn run_key(model: &Model) -> (i64, String) {
    (model.project.proj_id, model.project.name.clone())
}

pub(crate) fn note_scheduled(model: &Model) {
    LAST_RUNS.with(|runs| {
        let mut runs = runs.borrow_mut();
        runs.models.insert(run_key(model), model.clone());
        runs.generation += 1;
    });
}

impl App {
    /// The status bar panel saying the dates are out of date: shown when
    /// automatic scheduling is off and something the engine reads changed
    /// since the engine last ran on the project (a project first seen counts
    /// as scheduled). Worked out again only when the recorded project
    /// changes or the engine runs.
    pub(crate) fn schedule_needed_text(&self) -> Option<String> {
        if self.sched.auto_schedule {
            return None;
        }
        let latest = self.history.latest_model();
        let key = run_key(latest);
        let changes = self.history.changes();
        let needed = LAST_RUNS.with(|runs| {
            let mut runs = runs.borrow_mut();
            let generation = runs.generation;
            if let Some((k, c, g, needed)) = &runs.answer {
                if *k == key && *c == changes && *g == generation {
                    return *needed;
                }
            }
            let needed = match runs.models.get(&key) {
                Some(last) => !same_schedule_inputs(last, latest),
                None => {
                    runs.models.insert(key.clone(), latest.clone());
                    false
                }
            };
            runs.answer = Some((key, changes, generation, needed));
            needed
        });
        needed.then(|| "Schedule needed: press F9".to_string())
    }
}
