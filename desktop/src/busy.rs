//! Off-UI-thread scheduling and leveling worker with a modal progress dialog
//! and clean cancellation.
//!
//! Pressing Schedule (F9) or Level validates inputs on the UI thread, checks
//! for circular relationships, logs structural counts to the workspace
//! diagnostic log, and spawns a background thread over cloned project data.
//! While the worker runs, the UI thread stays responsive and paints a modal
//! progress dialog with phase label, elapsed time, progress bar and Cancel.
//! On completion the UI thread commits the result as one undo step; Cancel or
//! Escape sets the cancel flag and discards the result without touching
//! `App.project`.
use super::*;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, TryRecvError},
        Arc, Mutex,
    },
    time::Duration,
};

/// A long operation run on a background worker thread.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Busy {
    /// Schedule (F9): the Schedule dialog's Schedule button.
    Schedule,
    /// Level Resources: the Level button, at the input time it was pressed.
    Level(f64),
}

impl Busy {
    fn message(self) -> &'static str {
        match self {
            Busy::Schedule => "Scheduling...",
            Busy::Level(_) => "Leveling resources...",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Progress {
    pub phase: String,
    pub fraction: Option<f32>,
}

enum JobKind {
    Schedule,
    Level {
        settings: crate::schedule_dialogs::progress_tools::LevelSettings,
        time: f64,
    },
}

enum Work {
    Single(Box<crate::scheduling::SingleScheduleJob>),
    Multi(crate::open_set::MultiScheduleJob),
}

enum JobOutput {
    Single(Result<Box<crate::scheduling::SingleScheduleOut>, String>),
    Multi(Result<crate::open_set::MultiScheduleOut, String>),
    Cancelled,
}

struct ActiveJob {
    kind: JobKind,
    title: &'static str,
    op: &'static str,
    started: Instant,
    progress: Arc<Mutex<Progress>>,
    cancel: Arc<AtomicBool>,
    rx: mpsc::Receiver<JobOutput>,
    _worker: std::thread::JoinHandle<()>,
}

#[derive(Default)]
pub(crate) struct BusyState {
    active: Option<ActiveJob>,
}

impl App {
    /// Whether a background scheduling or leveling worker is currently active.
    pub(crate) fn busy_active(&self) -> bool {
        self.busy.active.is_some()
    }

    /// Start `op` on a background worker thread after UI-thread validation.
    pub(crate) fn begin_busy(&mut self, ctx: &egui::Context, op: Busy) {
        if self.busy.active.is_some() {
            return;
        }
        match op {
            Busy::Schedule => self.begin_schedule_busy(ctx),
            Busy::Level(time) => self.begin_level_busy(ctx, time),
        }
    }

    fn begin_schedule_busy(&mut self, ctx: &egui::Context) {
        if let Some((activity, field)) = self.edit_cell.take() {
            if !self.apply_cell(activity, field, &self.edit_text.clone()) {
                return;
            }
        }
        let origin = self.project.model.origin_serial;
        let mut data_date = self.project.model.project.data_date;
        let mut plan_start = self.project.model.project.plan_start;
        let mut apply_data_date = None;
        if self.sched.plan_to_forecast {
            if let Some(forecast) = self.forecast_start() {
                data_date = Some(forecast);
                plan_start = Some(forecast);
            }
        } else if !self.sched.own_data_date {
            match crate::schedule_dialogs::parse_display_date(origin, &self.sched.data_date_text) {
                Ok(date) => {
                    data_date = Some(date);
                    if self.open_set.multi() {
                        apply_data_date = Some(date + origin * DAY);
                    }
                }
                Err(error) => {
                    self.diag_log().error("schedule", &error);
                    self.error(error);
                    return;
                }
            }
        }
        let date_text = date_string(origin, data_date);
        let finish_text = date_string(origin, self.project.model.project.plan_end);
        let date = match openxer_xer::parse_relative(origin, &date_text) {
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
        let finish = match openxer_xer::parse_relative(origin, &finish_text) {
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
        let work = if self.open_set.multi() {
            let all_default = std::mem::take(&mut self.open_set.default_options_for_all);
            let mut job = self.prepare_multi_schedule_job(
                date,
                finish,
                apply_data_date,
                all_default,
                "schedule",
            );
            job.projects[0].model.project.plan_start = plan_start;
            crate::open_set::log_start_multi(&diag, "schedule", &job.projects, job.default);
            Work::Multi(job)
        } else {
            let mut next = self.project.clone();
            if self.sched.progress.manual_level {
                next.model.options.do_level = false;
            }
            next.model.project.data_date = Some(date);
            next.model.project.plan_start = plan_start;
            next.model.project.plan_end = finish;
            let recalc = self.recalc_costs_after_scheduling()
                || (next.model.options.do_level && self.recalc_costs_after_leveling());
            crate::scheduling::log_start_single(&diag, "schedule", &next);
            Work::Single(Box::new(crate::scheduling::SingleScheduleJob {
                project: next,
                recalc,
                op: "schedule",
            }))
        };
        self.spawn_busy(
            ctx,
            JobKind::Schedule,
            "Scheduling...",
            Busy::Schedule.message(),
            "schedule",
            work,
            diag,
        );
    }

    fn begin_level_busy(&mut self, ctx: &egui::Context, time: f64) {
        if let Some((activity, field)) = self.edit_cell.take() {
            if !self.apply_cell(activity, field, &self.edit_text.clone()) {
                return;
            }
        }
        let Some((settings, mut next)) = self.prepare_level_run() else {
            return;
        };
        let origin = next.model.origin_serial;
        let date_text = date_string(origin, next.model.project.data_date);
        let finish_text = date_string(origin, next.model.project.plan_end);
        let date = match openxer_xer::parse_relative(origin, &date_text) {
            Ok(Some(d)) => d,
            Ok(None) => {
                self.diag_log().error("level", "Enter a data date");
                self.error("Leveling canceled; no changes were made. Enter a data date".into());
                return;
            }
            Err(e) => {
                self.diag_log().error("level", &e);
                self.error(format!("Leveling canceled; no changes were made. {e}"));
                return;
            }
        };
        let finish = match openxer_xer::parse_relative(origin, &finish_text) {
            Ok(d) => d,
            Err(e) => {
                self.diag_log().error("level", &e);
                self.error(format!("Leveling canceled; no changes were made. {e}"));
                return;
            }
        };
        let diag = self.diag_log();
        diag.phase_start("level", "intercept_loops");
        let t = Instant::now();
        let has_loops = self.intercept_loops();
        diag.phase_done("level", "intercept_loops", t.elapsed().as_millis());
        if has_loops {
            self.status =
                "Leveling canceled; no changes were made. Fix the circular relationships first."
                    .into();
            return;
        }
        next.model.project.data_date = Some(date);
        next.model.project.plan_end = finish;
        let work = if self.open_set.multi() {
            let mut job = self.prepare_multi_schedule_job(date, finish, None, false, "level");
            job.projects[0] = next;
            crate::open_set::log_start_multi(&diag, "level", &job.projects, job.default);
            Work::Multi(job)
        } else {
            let recalc = self.recalc_costs_after_scheduling() || settings.recalc_costs;
            crate::scheduling::log_start_single(&diag, "level", &next);
            Work::Single(Box::new(crate::scheduling::SingleScheduleJob {
                project: next,
                recalc,
                op: "level",
            }))
        };
        self.spawn_busy(
            ctx,
            JobKind::Level { settings, time },
            "Leveling...",
            Busy::Level(time).message(),
            "level",
            work,
            diag,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn spawn_busy(
        &mut self,
        ctx: &egui::Context,
        kind: JobKind,
        title: &'static str,
        status: &'static str,
        op: &'static str,
        work: Work,
        diag: crate::diag_log::DiagLog,
    ) {
        let progress = Arc::new(Mutex::new(Progress {
            phase: "Preparing schedule...".into(),
            fraction: Some(0.05),
        }));
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        let worker_progress = Arc::clone(&progress);
        let worker_cancel = Arc::clone(&cancel);
        let worker = std::thread::spawn(move || {
            let out = match work {
                Work::Single(job) => match crate::scheduling::run_single_schedule_job(
                    *job,
                    Some(&worker_progress),
                    &worker_cancel,
                    &diag,
                ) {
                    Some(res) => JobOutput::Single(res.map(Box::new)),
                    None => JobOutput::Cancelled,
                },
                Work::Multi(job) => match crate::open_set::run_multi_schedule_job(
                    job,
                    Some(&worker_progress),
                    &worker_cancel,
                    &diag,
                ) {
                    Some(res) => JobOutput::Multi(res),
                    None => JobOutput::Cancelled,
                },
            };
            let _ = tx.send(out);
        });
        self.busy.active = Some(ActiveJob {
            kind,
            title,
            op,
            started: Instant::now(),
            progress,
            cancel,
            rx,
            _worker: worker,
        });
        self.status = status.into();
        ctx.set_cursor_icon(egui::CursorIcon::Wait);
        ctx.request_repaint();
        ctx.request_repaint_after(Duration::from_millis(50));
    }

    /// Cancel the running background worker and discard its result without
    /// mutating `App.project`.
    pub(crate) fn cancel_busy(&mut self) {
        let Some(job) = self.busy.active.take() else {
            return;
        };
        job.cancel.store(true, Ordering::Relaxed);
        self.diag_log().line(&format!(
            "{} state=cancelled elapsed_ms={}",
            job.op,
            job.started.elapsed().as_millis()
        ));
        self.status = match job.kind {
            JobKind::Schedule => "Schedule cancelled.",
            JobKind::Level { .. } => "Leveling cancelled.",
        }
        .into();
    }

    /// Poll a running background job. Called at the start of every frame.
    pub(crate) fn busy_frame(&mut self, ctx: &egui::Context) {
        let Some(active) = self.busy.active.as_ref() else {
            return;
        };
        ctx.set_cursor_icon(egui::CursorIcon::Wait);
        match active.rx.try_recv() {
            Err(TryRecvError::Empty) => {
                ctx.request_repaint_after(Duration::from_millis(50));
            }
            Ok(output) => {
                let job = self.busy.active.take().unwrap();
                if job.cancel.load(Ordering::Relaxed) {
                    return;
                }
                match (job.kind, output) {
                    (JobKind::Schedule, JobOutput::Single(Ok(out))) => {
                        self.commit_single_schedule_output(*out, "schedule", true, true);
                    }
                    (JobKind::Schedule, JobOutput::Multi(Ok(out))) => {
                        self.commit_multi_schedule_output(out, "schedule", true, true);
                    }
                    (JobKind::Schedule, JobOutput::Single(Err(e)) | JobOutput::Multi(Err(e))) => {
                        self.error(e);
                    }
                    (JobKind::Schedule, JobOutput::Cancelled) => {
                        self.status = "Schedule cancelled.".into();
                    }
                    (JobKind::Level { settings, time }, JobOutput::Single(Ok(out))) => {
                        let before = self.project.clone();
                        let revision = self.records_revision;
                        self.commit_single_schedule_output(*out, "level", false, false);
                        self.finish_level_run(settings, before, revision, time);
                    }
                    (JobKind::Level { settings, time }, JobOutput::Multi(Ok(out))) => {
                        let before = self.project.clone();
                        let revision = self.records_revision;
                        self.commit_multi_schedule_output(out, "level", false, false);
                        self.finish_level_run(settings, before, revision, time);
                    }
                    (
                        JobKind::Level { .. },
                        JobOutput::Single(Err(e)) | JobOutput::Multi(Err(e)),
                    ) => {
                        self.error(format!("Leveling canceled; no changes were made. {e}"));
                    }
                    (JobKind::Level { .. }, JobOutput::Cancelled) => {
                        self.status = "Leveling cancelled.".into();
                    }
                }
            }
            Err(TryRecvError::Disconnected) => {
                let job = self.busy.active.take().unwrap();
                self.diag_log().error(job.op, "worker disconnected");
                self.error("Scheduling stopped unexpectedly.".into());
            }
        }
    }

    /// Modal progress dialog drawn while a background schedule/level job runs.
    pub(crate) fn busy_dialog(&mut self, ctx: &egui::Context) {
        let Some(active) = self.busy.active.as_ref() else {
            return;
        };
        let title = active.title;
        let elapsed_text = format!("{:.1} s", active.started.elapsed().as_secs_f64());
        let (phase, frac) = active
            .progress
            .lock()
            .map(|p| (p.phase.clone(), p.fraction.unwrap_or(0.25)))
            .unwrap_or_else(|_| ("Working...".into(), 0.25));
        const W: f32 = 380.;
        const H: f32 = 122.;
        let (cancel_clicked, closed, actions) =
            crate::dialog_kit::modal(ctx, "busy_progress", title, Vec2::new(W, H), |c| {
                c.fill(0., 0., W, H, crate::dialog_kit::FACE);
                let phase_rect = c.r(14., 14., 270., 16.);
                c.ui.painter().with_clip_rect(phase_rect).text(
                    phase_rect.left_center(),
                    Align2::LEFT_CENTER,
                    &phase,
                    crate::dialog_kit::body_font(),
                    INK,
                );
                let elapsed_rect = c.r(290., 14., 76., 16.);
                c.ui.painter().with_clip_rect(elapsed_rect).text(
                    elapsed_rect.right_center(),
                    Align2::RIGHT_CENTER,
                    &elapsed_text,
                    crate::dialog_kit::body_font(),
                    INK,
                );
                let bar = c.r(14., 42., 352., 18.);
                c.sunken(bar, Color32::WHITE);
                let inner = bar.shrink(2.);
                let fill_w = (inner.width() * frac.clamp(0.08, 1.0)).round();
                let filled = Rect::from_min_size(inner.min, Vec2::new(fill_w, inner.height()));
                c.ui.painter().rect_filled(filled, 0., BLUE);
                crate::dialog_kit::button_row(
                    c,
                    W,
                    83.,
                    &[crate::dialog_kit::Entry::cancel("busy_cancel")],
                )
                .clicked
                .is_some()
            });
        self.record_dialog_actions(actions);
        if cancel_clicked || closed {
            self.cancel_busy();
        }
    }
}
