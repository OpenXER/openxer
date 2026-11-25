//! The per-activity CPM rules: relationship dates, lags, constraints, boundary
//! adjustments and float. Each rule carries an ID that `engine/RULES.md`
//! explains with its test and evidence.
use crate::{calendar::CalendarAdapter, model::*, options::*, scheduler::Scheduler};
impl Scheduler<'_> {
    /// REL-1: a milestone has only one date, so a tie into or out of one uses
    /// the end it has (a start milestone's start, a finish milestone's finish).
    pub fn get_logical_dep_type(&self, r: usize) -> LinkType {
        use LinkType::*;
        use TaskType::*;
        let l = &self.model.links[r];
        let p = self.model.activities[l.pred].task_type;
        let s = self.model.activities[l.succ].task_type;
        let raw = l.link_type;
        match (p, s, raw) {
            (Mile, Mile, _) => SS,
            (Mile, FinMile, _) => SF,
            (Mile, _, FF) => SF,
            (Mile, _, FS | None) => SS,
            (Mile, _, _) => raw,
            (FinMile, Mile, _) => FS,
            (FinMile, FinMile, _) => FF,
            (FinMile, _, SF) => FF,
            (FinMile, _, SS) => FS,
            (FinMile, _, _) => raw,
            (_, Mile, SF) => SS,
            (_, Mile, FF) => FS,
            (_, FinMile, SS) => SF,
            (_, FinMile, FS | None) => FF,
            _ => raw,
        }
    }
    /// REL-2: the calendar that counts a tie's lag: the predecessor's, the
    /// successor's, the project's, or round the clock, by the lag calendar
    /// option.
    pub fn lag_calendar(&self, r: usize) -> &CalendarAdapter {
        let l = &self.model.links[r];
        match self.model.options.lag_calendar {
            LagCalendar::SuccCal => self.cal(l.succ),
            LagCalendar::Hour24 => &self.cal24,
            LagCalendar::PredCal => self.cal(l.pred),
            LagCalendar::ProjCal => &self.model.calendars[self.model.project_calendar],
        }
    }
    /// REL-3 to REL-5: the forward date a tie gives its successor: the
    /// predecessor's remaining start or finish plus the lag on the lag
    /// calendar. A started predecessor's start lag, and a completed
    /// predecessor's finish lag, are used up by the working time already
    /// elapsed up to the data date.
    pub fn apply_lag_forward(&self, r: usize) -> Option<i64> {
        let l = &self.model.links[r];
        let p = &self.model.activities[l.pred];
        let dep = self.get_logical_dep_type(r);
        let base = if matches!(dep, LinkType::FS | LinkType::FF) {
            p.reend
        } else {
            p.restart
        }?;
        self.apply_lag_forward_from(r, base)
    }
    /// The same effective-lag calculation also evaluates a relationship's
    /// candidate start against the durable remaining-date snapshot.
    fn apply_lag_forward_from(&self, r: usize, base: i64) -> Option<i64> {
        let l = &self.model.links[r];
        let p = &self.model.activities[l.pred];
        let dep = self.get_logical_dep_type(r);
        let lag = l.lag;
        if lag == 0 {
            if p.act_start.is_some()
                && self.model.options.progress_override
                && matches!(dep, LinkType::SS | LinkType::SF)
            {
                return self.data_date_of(l.pred);
            }
            return Some(base);
        }
        let cal = self.lag_calendar(r);
        let dd = self.data_date_of(l.pred)?;
        // REL-3: a positive lag ends on a finish moment of the lag calendar;
        // the successor's own calendar then places its start, and a completed
        // successor keeps the moment itself (corpus 4815, 5790, 6031, 6183;
        // saved probes 501, 503, 701).
        let forward = |t: i64, m: i64| cal.add(t, m);
        if p.act_start.is_none() || !matches!(dep, LinkType::SS | LinkType::SF) {
            if p.act_finish.is_none() || !matches!(dep, LinkType::FS | LinkType::FF) {
                return Some(if lag >= 0 {
                    forward(base, lag)
                } else {
                    cal.sub(base, -lag)
                });
            }
            let elapsed = cal.between(p.act_finish?, dd);
            let remaining_lag = (lag - elapsed).max(0);
            // REL-5: a lag used up entirely adds nothing. Adding zero work would
            // move a completed node into the next shift. REL-14: the part not
            // used up does not move a completed successor either (a private
            // weekly export: a finish-to-finish lag of 50 h with 40 h left).
            let succ_done = self.model.activities[l.succ].is_completed();
            return Some(if remaining_lag == 0 || succ_done {
                base
            } else {
                forward(base, remaining_lag)
            });
        }
        let elapsed = cal.between(p.act_start?, dd);
        let remaining_lag = (lag - elapsed).max(0);
        Some(if remaining_lag == 0 {
            base
        } else {
            forward(base, remaining_lag)
        })
    }
    /// REL-6: backward lag. Every lag counts in full, except that a started
    /// predecessor's own SS lag is used up by its work elapsed to the data
    /// date, as in the forward pass; the successor's progress does not
    /// shorten it (corpus 5772, 6031; saved probes 301, 304, 504, 505). The
    /// same holds under Progress Override (probes 302, 508); under Actual
    /// Dates every lag is counted back in full (probes 303, 509). A lag used
    /// up entirely leaves the date as it is (corpus 6035, 6321).
    pub fn apply_lag(&self, r: usize, date: i64) -> i64 {
        let l = &self.model.links[r];
        let p = &self.model.activities[l.pred];
        let lag = l.lag;
        if lag == 0 {
            return date;
        }
        let cal = self.lag_calendar(r);
        let dep = self.get_logical_dep_type(r);
        let dd = self.data_date_of(l.pred).unwrap_or(0);
        if !self.model.options.neither_oos() && dep == LinkType::SS && lag > 0 {
            if let Some(start) = p.act_start {
                let left = (lag - cal.between(start, dd).max(0)).max(0);
                return if left == 0 { date } else { cal.sub(date, left) };
            }
        }
        if lag >= 0 {
            cal.sub(date, lag)
        } else {
            cal.add_start(date, -lag)
        }
    }
    /// BWD-6: backward, a completed predecessor's positive finish-to-start or
    /// finish-to-finish lag counts only the part not yet used up: the working
    /// time on the lag calendar between its actual finish and the data date
    /// is taken off, as in the forward pass (REL-5), whichever way
    /// start-to-start lag is measured (saved probes 304, 1205, 1206). For
    /// start-side ties, negative lags and lags used up entirely the date is
    /// left as it is (BWD-1).
    pub fn apply_lag_completed(&self, r: usize, date: i64) -> i64 {
        let l = &self.model.links[r];
        let p = &self.model.activities[l.pred];
        if l.lag <= 0 {
            return date;
        }
        if !matches!(self.get_logical_dep_type(r), LinkType::FS | LinkType::FF) {
            return date;
        }
        let (Some(from), Some(dd)) = (p.act_finish, self.data_date_of(l.pred)) else {
            return date;
        };
        let cal = self.lag_calendar(r);
        let left = (l.lag - cal.between(from, dd).max(0)).max(0);
        if left == 0 {
            date
        } else {
            cal.sub(date, left)
        }
    }
    /// REL-7 to REL-10: the forward candidate one tie gives a successor, as a
    /// (start, finish) pair. Level-of-effort predecessors give none; under
    /// Progress Override a started successor's start-side ties are replaced by
    /// its progress; a completed predecessor drives from its actual dates when
    /// logic is not retained; a successor with an Expected Finish ignores
    /// finish-side ties.
    pub fn rel_succ_restart_date(
        &self,
        i: usize,
        r: usize,
        dep: LinkType,
        check: bool,
    ) -> (Option<i64>, Option<i64>) {
        let opts = &self.model.options;
        let p = &self.model.activities[self.model.links[r].pred];
        let t = &self.model.activities[i];
        if check && p.task_type == TaskType::Loe {
            return (None, None);
        }
        // REL-8: under Progress Override a started activity's start-side ties
        // are replaced by its progress; finish-side ties still hold (saved
        // probes 302, 508).
        if opts.progress_override
            && matches!(dep, LinkType::FS | LinkType::SS)
            && !t.is_completed()
            && t.act_start.is_some()
        {
            return (None, None);
        }
        let date = if p.is_completed() && !opts.retained_logic {
            // REL-9: without retained logic a completed predecessor still
            // drives, from its actual finish (FS/FF) or actual start (SS/SF)
            // plus the whole lag (saved probes 302, 303, 508, 509).
            let base = if matches!(dep, LinkType::FS | LinkType::FF) {
                p.act_finish
            } else {
                p.act_start
            };
            let Some(base) = base else {
                return (None, None);
            };
            let lag = self.model.links[r].lag;
            let cal = self.lag_calendar(r);
            match lag {
                0 => base,
                l if l > 0 => cal.add(base, l),
                l => cal.sub(base, -l),
            }
        } else {
            let Some(date) = self.apply_lag_forward(r) else {
                return (None, None);
            };
            date
        };
        if matches!(dep, LinkType::FS | LinkType::SS) {
            return (Some(date), None);
        }
        if self.model.activities[i]
            .uses_expected_finish(&self.model.options, self.model.options.do_level)
        {
            (None, None)
        } else {
            (None, Some(date))
        }
    }
    /// REL-11, REL-12: the earliest start a successor's ties allow: the latest
    /// of the start-side candidates and of the finish-side candidates minus
    /// the successor's duration. Also returns the driving tie type and the
    /// latest finish-side candidate.
    pub fn get_succ_restart_date(&self, i: usize) -> (Option<i64>, LinkType, Option<i64>) {
        self.succ_restart_over(i, &self.model.activities[i].preds)
    }
    /// `get_succ_restart_date` over the given predecessor relationships only.
    pub fn succ_restart_over(
        &self,
        i: usize,
        preds: &[usize],
    ) -> (Option<i64>, LinkType, Option<i64>) {
        let mut restart = None;
        let mut reend = None;
        let mut drive = LinkType::None;
        let mut reend_dep = LinkType::None;
        for &r in preds {
            let dep = self.get_logical_dep_type(r);
            let (rs, re) = self.rel_succ_restart_date(i, r, dep, true);
            if let Some(rs) = rs {
                if restart.is_none_or(|v| rs > v) {
                    restart = Some(rs);
                    drive = dep;
                }
            }
            if let Some(re) = re {
                let mut re = self.edge_adjust_early_reend(i, re);
                let lag = self.model.links[r].lag;
                // ADM-1: the finish-on-valid-time administrator setting is not
                // retained logic. Only the day's first work start qualifies.
                if self.model.options.finish_on_valid_time
                    && ((dep == LinkType::FF && lag < 0) || (dep == LinkType::SF && lag <= 0))
                {
                    let cal = self.scheduling_cal(i);
                    if re == cal.snap_start(re.div_euclid(1440) * 1440) {
                        re = cal.snap_finish(re);
                    }
                }
                if reend.is_none_or(|v| re > v) {
                    reend = Some(re);
                    reend_dep = dep;
                }
            }
        }
        if let Some(re) = reend {
            let conv = self.get_rsrc_restart_date(i, re);
            if restart.is_none_or(|rs| conv > rs) {
                return (Some(conv), reend_dep, reend);
            }
        }
        (restart, drive, reend)
    }
    /// BWD-3: the finish a successor offers its predecessors. In Actual Dates
    /// mode an activity holding an actual finish offers that; under Progress
    /// Override it offers none; otherwise its remaining finish.
    pub fn get_task_end_date(&self, i: usize) -> Option<i64> {
        let t = &self.model.activities[i];
        if !t.is_completed() && t.act_finish.is_some() && !self.model.options.retained_logic {
            if self.model.options.progress_override {
                None
            } else {
                t.act_finish
            }
        } else {
            t.reend
        }
    }
    /// BWD-3: the same for starts (a started successor's actual start limits
    /// its predecessors in Actual Dates mode).
    pub fn get_task_start_date(&self, i: usize) -> Option<i64> {
        let t = &self.model.activities[i];
        if !t.is_completed() && t.act_start.is_some() && !self.model.options.retained_logic {
            if self.model.options.progress_override {
                None
            } else {
                t.act_start
            }
        } else {
            t.restart
        }
    }
    /// BWD-2: the limit one tie places on its predecessor, as a (start limit,
    /// finish limit) pair by tie type. Summaries never limit; level-of-effort
    /// successors do not when `check` is set; a completed successor does not
    /// unless logic is retained; Progress Override gives none.
    pub fn rel_pred_reend_date(
        &self,
        r: usize,
        dep: LinkType,
        check: bool,
    ) -> (Option<i64>, Option<i64>) {
        let l = &self.model.links[r];
        let s = &self.model.activities[l.succ];
        if (s.is_completed() && !self.model.options.retained_logic)
            || (check && s.task_type == TaskType::Loe)
            || s.task_type == TaskType::Wbs
        {
            return (None, None);
        }
        let val = |end: bool| {
            if self.model.options.progress_override {
                return None;
            }
            let d = if end {
                self.get_task_end_date(l.succ)
            } else {
                self.get_task_start_date(l.succ)
            }?;
            Some(if l.lag != 0 { self.apply_lag(r, d) } else { d })
        };
        match dep {
            LinkType::FF => (None, val(true)),
            LinkType::FS | LinkType::None => (None, val(false)),
            LinkType::SF => (val(true), None),
            LinkType::SS => (val(false), None),
        }
    }
    /// BWD-1: the earliest limit the successors place on a predecessor's
    /// finish. A completed predecessor ignores the tie lags (corpus
    /// 369). Also returns the start-side limit, which the nonwork start
    /// rule receives (saved probe 701; FWD-5).
    pub fn get_pred_reend_date(&self, i: usize) -> (Option<i64>, LinkType, Option<i64>) {
        self.pred_reend_over(i, &self.model.activities[i].succs)
    }
    /// `get_pred_reend_date` over the given successor relationships only.
    pub fn pred_reend_over(
        &self,
        i: usize,
        succs: &[usize],
    ) -> (Option<i64>, LinkType, Option<i64>) {
        let mut finish = None;
        let mut start = None;
        let mut fin_dep = LinkType::None;
        let mut st_dep = LinkType::None;
        let t = &self.model.activities[i];
        for &r in succs {
            let l = &self.model.links[r];
            let s = &self.model.activities[l.succ];
            if (s.is_completed() && !self.model.options.retained_logic)
                || Self::skip_type(s.task_type)
            {
                continue;
            }
            let dep = self.get_logical_dep_type(r);
            let c = match dep {
                LinkType::FF => self.get_task_end_date(l.succ),
                LinkType::FS | LinkType::None => self.get_task_start_date(l.succ),
                _ => None,
            };
            if let Some(mut c) = c {
                if l.lag != 0 {
                    c = if t.is_completed() {
                        self.apply_lag_completed(r, c)
                    } else {
                        self.apply_lag(r, c)
                    };
                }
                if finish.is_none_or(|v| c < v) {
                    finish = Some(c);
                    fin_dep = dep;
                }
            }
            let c = match dep {
                LinkType::SF => self.get_task_end_date(l.succ),
                LinkType::SS => self.get_task_start_date(l.succ),
                _ => None,
            };
            if let Some(mut c) = c {
                if l.lag != 0 {
                    c = if t.is_completed() {
                        self.apply_lag_completed(r, c)
                    } else {
                        self.apply_lag(r, c)
                    };
                }
                c = self.edge_adjust_late_restart(i, c);
                if start.is_none_or(|v| c < v) {
                    start = Some(c);
                    st_dep = dep;
                }
            }
        }
        if let Some(s) = start {
            let conv = self.get_rsrc_reend_date(i, s);
            if finish.is_none_or(|f| conv < f) {
                return (Some(conv), st_dep, start);
            }
        }
        (finish, fin_dep, start)
    }
    /// RES-4: an assignment's exact hours (units / units per hour) when its
    /// completed activity is counted on round-the-clock time (PASS-3) and it
    /// has no lag; such dates are shifted as day numbers (`clock_shift`).
    fn clock_hours(&self, i: usize, a: &Assignment, lag: i64) -> Option<f64> {
        let t = &self.model.activities[i];
        let on_clock =
            t.is_completed() && t.task_type != TaskType::Wbs && !self.model.options.neither_oos();
        if !on_clock || lag != 0 {
            return None;
        }
        a.rem_hours_bits.map(f64::from_bits)
    }
    /// RES-4: a moment moved by `hours` the way the reference stores it: as a
    /// day number (days since 1899-12-30 plus the fraction of the day) plus
    /// hours / 24, rounded to the nearest minute with ties to even. An exact
    /// half minute can land a hair above or below the half in binary, so the
    /// same duration rounds up from some moments and down from others (a
    /// private weekly export: 142.5 minutes after Monday 07:00 gives 09:22, after Friday
    /// 17:00 gives 19:23; saved probe 1203).
    pub fn clock_shift(&self, date: i64, hours: f64) -> i64 {
        let origin = self.model.origin_serial;
        let day = (origin + date.div_euclid(1440)) as f64;
        let serial = day + date.rem_euclid(1440) as f64 / 1440.0;
        let minutes = ((serial + hours / 24.0) * 1440.0).round_ties_even() as i64;
        minutes - origin * 1440
    }
    /// RES-1: the start for a given finish: the earliest over the
    /// date-driving assignments of the finish minus their remaining work and
    /// lag; with none, minus the activity's remaining duration.
    pub fn get_rsrc_restart_date(&self, i: usize, reend: i64) -> i64 {
        let t = &self.model.activities[i];
        t.assignments
            .iter()
            .filter(|a| a.rollup)
            .map(|a| match self.clock_hours(i, a, a.remain_lag) {
                Some(h) => self.clock_shift(reend, -h),
                None => self.scheduling_cal(i).sub(reend, a.rem + a.remain_lag),
            })
            .min()
            .unwrap_or_else(|| {
                if t.rem == 0 {
                    reend
                } else {
                    self.scheduling_cal(i).sub(reend, t.rem)
                }
            })
    }
    /// RES-1: the finish for a given start: the latest over the date-driving
    /// assignments of the start plus their remaining work and lag; with none,
    /// plus the activity's remaining duration.
    pub fn get_rsrc_reend_date(&self, i: usize, restart: i64) -> i64 {
        let t = &self.model.activities[i];
        t.assignments
            .iter()
            .filter(|a| a.rollup)
            .map(|a| match self.clock_hours(i, a, a.relag) {
                Some(h) => self.clock_shift(restart, h),
                None => self.scheduling_cal(i).add(restart, a.rem + a.relag),
            })
            .max()
            .unwrap_or_else(|| {
                if t.rem == 0 {
                    restart
                } else {
                    self.scheduling_cal(i).add(restart, t.rem)
                }
            })
    }
    /// Forward pass for one activity (FWD-1 to FWD-3, REL-11, REL-12): the
    /// early start is the latest of the data date, the constraint date, the
    /// ties' candidate, the resume date and (Actual Dates) the actual start.
    pub fn schedule_forward_task(&mut self, i: usize) {
        let t = &self.model.activities[i];
        let opt = &self.model.options;
        let mut cstr_date = if !matches!(t.cstr, CstrType::None | CstrType::Alap) {
            self.constraint_early_restart(i, true)
        } else if !matches!(t.cstr2, CstrType::None | CstrType::Alap) {
            self.constraint_early_restart(i, false)
        } else {
            None
        };
        let (rs_drive, drive_dep, re_drive) = self.get_succ_restart_date(i);
        let dd = self.data_date_of(i).unwrap_or(0);
        let mut es = dd;
        // EXP-1: with Expected Finish dates in use, an actual start before the
        // expected finish drops the constraint date.
        if t.uses_expected_finish(opt, opt.do_level)
            && matches!((t.act_start,t.expect_end),(Some(s),Some(e))if e>s)
        {
            cstr_date = None;
        }
        for c in [cstr_date, rs_drive].into_iter().flatten() {
            es = es.max(c);
        }
        if opt.out_of_sequence != OutOfSequence::None {
            if let Some(c) = t.ext_early_start {
                es = es.max(c);
            }
        }
        es = es.max(dd);
        if opt.neither_oos() {
            if let Some(c) = t.act_start {
                es = es.max(c);
            }
        }
        if let Some(c) = t.resume {
            es = es.max(c);
        }
        // FWD-1: zero duration keeps the raw date; a driven start milestone
        // snaps forward to a start moment.
        let (mut rs, re) = if t.rem == 0 {
            let rs = if t.task_type == TaskType::Mile && (rs_drive.is_some() || cstr_date.is_some())
            {
                self.scheduling_cal(i).snap_start(es)
            } else if t.task_type == TaskType::FinMile && !t.is_completed() {
                self.edge_adjust_early_reend(i, es)
            } else {
                es
            };
            (rs, rs)
        } else {
            let rs = self.scheduling_cal(i).snap_start(es);
            (rs, self.scheduling_cal(i).add(rs, t.rem))
        };
        self.model.activities[i].restart = Some(rs);
        self.model.activities[i].reend = Some(re);
        if self.model.activities[i].rem != 0 {
            self.place_resource_dependent(i, false);
            rs = self.model.activities[i].restart.unwrap_or(rs);
        }
        self.adjust_for_non_work_finish_time_from_dependency(i, drive_dep, re_drive);
        let t = &self.model.activities[i];
        if t.rem == 0 && matches!(t.task_type, TaskType::Mile | TaskType::FinMile) {
            rs = t.reend.unwrap_or(rs);
            self.model.activities[i].restart = Some(rs);
        }
        self.rem_finish(i, drive_dep);
        // FWD-1: an open activity with no remaining work, other than a finish
        // milestone, starts at the next working moment whether or not a tie
        // drives it (saved probes 000, 506-507, 510-512, 702-704, 1001-1006;
        // corpus 4815). A completed node keeps its raw date.
        let t = &self.model.activities[i];
        if t.rem == 0
            && !t.is_completed()
            && matches!(
                t.task_type,
                TaskType::Mile | TaskType::Task | TaskType::Resource
            )
        {
            if let Some(rs) = t.restart {
                let start = self.scheduling_cal(i).snap_start(rs);
                if start != rs {
                    let re = if t.task_type == TaskType::Mile {
                        start
                    } else {
                        t.reend.map_or(start, |e| e.max(start))
                    };
                    self.model.activities[i].restart = Some(start);
                    self.model.activities[i].reend = Some(re);
                }
            }
        }
    }
    /// Backward pass for one activity (BWD-1 to BWD-5): the late finish is the
    /// earliest of the project limit, the open-end early finish, the
    /// constraint, the successors' limits and an outside late finish. There is
    /// no data-date clamp (BWD-5), so a late date may fall before the data
    /// date.
    pub fn schedule_backward_task(&mut self, i: usize) {
        let t = &self.model.activities[i];
        let opt = &self.model.options;
        let cstr = if !matches!(t.cstr, CstrType::None | CstrType::Alap) {
            self.constraint_late_reend(i, true)
        } else if !matches!(t.cstr2, CstrType::None | CstrType::Alap) {
            self.constraint_late_reend(i, false)
        } else {
            None
        };
        let (succ_limit, succ_dep, start_limit) = self.get_pred_reend_date(i);
        let mut end = self.late_limit_of(i);
        if let Some(mut e) = end {
            if t.succs.is_empty() && opt.open_end_uses_early {
                if let Some(r) = t.reend {
                    e = e.min(r);
                }
            }
            for c in [cstr, succ_limit].into_iter().flatten() {
                e = e.min(c);
            }
            if opt.out_of_sequence != OutOfSequence::None {
                if let Some(c) = t.ext_late_end {
                    e = e.min(c);
                }
            }
            end = Some(self.edge_adjust_late_reend(i, e));
        }
        let start = end.map(|e| {
            if t.rem == 0 {
                e
            } else {
                self.scheduling_cal(i).sub(e, t.rem)
            }
        });
        self.model.activities[i].reend = end;
        self.model.activities[i].restart = start;
        // RES-2: Resource Dependent work sits on its resources' calendars
        // (saved probes 814, 816), then the nonwork start rule sees the start
        // limit (FWD-5).
        self.place_resource_dependent(i, true);
        self.adjust_for_non_work_start_time_from_dependency(i, succ_dep, start_limit);
    }
    /// FWD-4: extend a finish driven by an FF/SF tie across a gap containing
    /// no work: when no working time lies between the finish and the tie's
    /// limit, the finish is the limit.
    pub fn adjust_for_non_work_finish_time_from_dependency(
        &mut self,
        i: usize,
        dep: LinkType,
        limit: Option<i64>,
    ) {
        let Some(limit) = limit else {
            return;
        };
        let t = &self.model.activities[i];
        if !matches!(dep, LinkType::SF | LinkType::FF)
            || t.reend.is_none_or(|finish| finish >= limit)
        {
            return;
        }
        let cal = self.scheduling_cal(i);
        if cal.between(t.reend.unwrap(), limit) == 0 {
            let moved = if self.model.options.finish_on_valid_time
                && limit == cal.snap_start(limit.div_euclid(1440) * 1440)
            {
                cal.snap_finish(limit)
            } else {
                limit
            };
            self.model.activities[i].reend = Some(moved);
        }
    }
    /// FWD-5: the backward mirror of the finish rule: when the late start lies
    /// after the start-side limit of the driving SS/SF tie with no working
    /// time between them, the late start is the limit itself (a start a tie
    /// places at the close of a shift stays there; saved probe 701).
    pub fn adjust_for_non_work_start_time_from_dependency(
        &mut self,
        i: usize,
        dep: LinkType,
        limit: Option<i64>,
    ) {
        let Some(limit) = limit else {
            return;
        };
        if !matches!(dep, LinkType::SF | LinkType::SS) {
            return;
        }
        let Some(restart) = self.model.activities[i].restart else {
            return;
        };
        if restart > limit && self.scheduling_cal(i).between(limit, restart) == 0 {
            self.model.activities[i].restart = Some(limit);
        }
    }
    /// RES-2: an open Resource Dependent activity's work: one part per
    /// assignment with units whose resource calendar is known, else none.
    pub fn resource_dependent_parts(&self, i: usize) -> Vec<(usize, i64)> {
        let t = &self.model.activities[i];
        if t.task_type != TaskType::Resource || t.is_completed() {
            return vec![];
        }
        t.assignments
            .iter()
            .filter_map(|a| a.cal.filter(|_| a.rem > 0).map(|c| (c, a.rem)))
            .collect()
    }
    /// RES-2: a Resource Dependent activity works on its resources' calendars.
    /// Forward, each assignment starts at the activity's start moved on to
    /// its resource's next working moment and runs its own remaining
    /// duration there; backward, each finishes at the activity's late finish
    /// moved back to its resource's previous working finish. The activity
    /// spans the earliest start to the latest finish; float stays on the
    /// activity's own calendar (saved probes 814, 816).
    pub fn place_resource_dependent(&mut self, i: usize, backward: bool) {
        let parts = self.resource_dependent_parts(i);
        if parts.is_empty() {
            return;
        }
        let cals = &self.model.calendars;
        let t = &self.model.activities[i];
        let (rs, re) = if backward {
            let Some(end) = t.reend else {
                return;
            };
            let ends: Vec<i64> = parts
                .iter()
                .map(|&(c, _)| cals[c].snap_finish(end))
                .collect();
            let rs = parts
                .iter()
                .zip(&ends)
                .map(|(&(c, d), &f)| cals[c].sub(f, d))
                .min();
            (rs, ends.iter().copied().max())
        } else {
            let Some(start) = t.restart else {
                return;
            };
            let starts: Vec<i64> = parts
                .iter()
                .map(|&(c, _)| cals[c].snap_start(start))
                .collect();
            let re = parts
                .iter()
                .zip(&starts)
                .map(|(&(c, d), &s)| cals[c].add(s, d))
                .max();
            (starts.iter().copied().min(), re)
        };
        self.model.activities[i].restart = rs;
        self.model.activities[i].reend = re;
    }
    /// EDGE-1: an early finish moment stays when the minute before it is
    /// working time (a finish at a shift's close stays); otherwise it moves on
    /// to the next working start.
    pub fn edge_adjust_early_reend(&self, i: usize, d: i64) -> i64 {
        if !self.scheduling_cal(i).is_work(d - 1) {
            self.scheduling_cal(i).snap_start(d)
        } else {
            d
        }
    }
    /// EDGE-1, EDGE-2: an early start moment is the next working start; a
    /// finish milestone's early date follows the finish rule instead.
    pub fn edge_adjust_early_restart(&self, i: usize, d: i64) -> i64 {
        if self.model.activities[i].task_type == TaskType::FinMile {
            self.edge_adjust_early_reend(i, d)
        } else {
            self.scheduling_cal(i).snap_start(d)
        }
    }
    /// EDGE-1, EDGE-2: a late finish moment is the previous working finish; a
    /// start milestone's late date follows the late start rule instead.
    pub fn edge_adjust_late_reend(&self, i: usize, d: i64) -> i64 {
        if self.model.activities[i].task_type == TaskType::Mile {
            self.edge_adjust_late_restart(i, d)
        } else {
            self.scheduling_cal(i).snap_finish(d)
        }
    }
    /// EDGE-1: a late start moment stays when it is working time, otherwise it
    /// is the previous working finish.
    pub fn edge_adjust_late_restart(&self, i: usize, d: i64) -> i64 {
        if !self.scheduling_cal(i).is_work(d) {
            self.scheduling_cal(i).snap_finish(d)
        } else {
            d
        }
    }
    /// CON-1, CON-3 to CON-5: the early start a constraint demands. Start On,
    /// Start On or After, Finish On, Finish On or After and the mandatory
    /// types demand one; a finish constraint's date is first converted to a
    /// start through the duration; an Expected Finish replaces a finish
    /// constraint's date; milestones ignore constraints on the end they lack;
    /// only work not yet started is bound (and started work under a pinning
    /// Mandatory constraint, CON-9).
    pub fn constraint_early_restart(&self, i: usize, primary: bool) -> Option<i64> {
        use CstrType::*;
        let t = &self.model.activities[i];
        let (ty, mut date) = if primary {
            (t.cstr, t.cstr_date)
        } else {
            (t.cstr2, t.cstr_date2)
        };
        if !ty.early_gate() {
            return Option::None;
        }
        if t.expect_end.is_some() && ty.ef_replaces() {
            date = t.expect_end;
        }
        let date = date?;
        if t.task_type == TaskType::Mile && matches!(ty, FinishOn | FinishOnAfter | MandatoryFinish)
        {
            return Option::None;
        }
        if t.task_type == TaskType::FinMile && matches!(ty, StartOn | StartOnAfter | MandatoryStart)
        {
            return Option::None;
        }
        if !t.unstarted_constraint_binds(primary) {
            return Option::None;
        }
        let result = if matches!(ty, StartOn | StartOnAfter | MandatoryStart) {
            date
        } else {
            self.get_rsrc_restart_date(i, self.edge_adjust_early_reend(i, date))
        };
        Some(self.edge_adjust_early_restart(i, result))
    }
    /// CON-2 to CON-6: the late finish a constraint caps. Start On, Start On
    /// or Before, Finish On, Finish On or Before and the mandatory types cap
    /// one; a start constraint is converted to a finish through the duration;
    /// a zero-duration start milestone keeps the rolled start (corpus
    /// 6215); a start constraint binds only unstarted work (and started work
    /// under a pinning Mandatory Start, CON-9), a finish constraint anything
    /// not completed.
    pub fn constraint_late_reend(&self, i: usize, primary: bool) -> Option<i64> {
        use CstrType::*;
        let t = &self.model.activities[i];
        let (ty, mut date) = if primary {
            (t.cstr, t.cstr_date)
        } else {
            (t.cstr2, t.cstr_date2)
        };
        if !ty.late_gate() {
            return Option::None;
        }
        if t.expect_end.is_some() && ty.ef_replaces() {
            date = t.expect_end;
        }
        let date = date?;
        if t.task_type == TaskType::Mile
            && matches!(ty, FinishOn | FinishOnBefore | MandatoryFinish)
        {
            return Option::None;
        }
        if t.task_type == TaskType::FinMile
            && matches!(ty, StartOn | StartOnBefore | MandatoryStart)
        {
            return Option::None;
        }
        let result = if matches!(ty, StartOn | StartOnBefore | MandatoryStart) {
            if !t.unstarted_constraint_binds(primary) {
                return Option::None;
            }
            let restart = self.edge_adjust_late_restart(i, date);
            if t.rem == 0 {
                restart
            } else {
                self.get_rsrc_reend_date(i, restart)
            }
        } else {
            if t.is_completed() {
                return Option::None;
            }
            date
        };
        Some(self.edge_adjust_late_reend(i, result))
    }
    /// EXP-1: with Expected Finish dates in use, the remaining duration runs
    /// from the start to the expected finish. A finish-side tie driving the
    /// activity leaves its duration alone; an expected finish at or before the
    /// start leaves no remaining work unless an assignment drives dates.
    pub fn rem_finish(&mut self, i: usize, dep: LinkType) {
        let t = &self.model.activities[i];
        if matches!(dep, LinkType::FF | LinkType::SF)
            || !t.uses_expected_finish(&self.model.options, self.model.options.do_level)
        {
            return;
        }
        let (Some(e), Some(s)) = (t.expect_end, t.restart) else {
            return;
        };
        // EXP-2: an Expected Finish is a finish moment: one at a shift's start
        // or in nonworking time rolls back to the previous working finish
        // (saved probe 402).
        let e = self.scheduling_cal(i).snap_finish(e);
        if e > s {
            let rem = self.cal(i).between(s, e);
            let t = &mut self.model.activities[i];
            t.reend = Some(e);
            t.rem = rem;
        } else if !t.assignments.iter().any(|a| a.rollup) {
            let t = &mut self.model.activities[i];
            t.reend = t.restart;
            t.rem = 0;
        }
    }
    /// FLT-2: store the activity's free float.
    pub fn calculate_free_float_task(&mut self, i: usize) {
        self.model.activities[i].free_float = Some(self.calc_free_float(i));
    }
    /// FLT-2: free float: the smallest gap to a successor, using the raw tie
    /// type, measured on the activity's own calendar after taking the lag off
    /// on the lag calendar. With no successor it runs to the project finish.
    /// Level-of-effort work, Expected Finish work and mandatory-constrained
    /// work have none.
    pub fn calc_free_float(&self, i: usize) -> i64 {
        let t = &self.model.activities[i];
        if t.task_type == TaskType::Wbs {
            // PASS-1: a summary's ties are dropped both ways, so it has no
            // successor: free float runs to the project finish (probe 601).
            return match (t.early_finish, self.scd_end_of(i)) {
                (Some(e), Some(p)) => self.cal(i).between(e, p),
                _ => 0,
            };
        }
        if t.task_type == TaskType::Loe
            || t.uses_expected_finish(&self.model.options, self.model.options.do_level)
            || t.uses_mandatory_constraint()
        {
            return 0;
        }
        let mut best = None;
        for &r in &t.succs {
            let l = &self.model.links[r];
            let s = &self.model.activities[l.succ];
            if Self::skip_type(s.task_type) {
                continue;
            }
            // FLT-5: a tie that progress replaces is not counted (saved probes
            // 302, 508).
            if self.model.options.progress_override
                && !self.model.options.retained_logic
                && self.progress_replaces_tie(r)
            {
                continue;
            }
            let src = if matches!(l.link_type, LinkType::FS | LinkType::FF) {
                t.early_finish
            } else {
                t.early_start
            };
            let tgt = if matches!(l.link_type, LinkType::FS | LinkType::SS) {
                s.early_start
            } else {
                s.early_finish
            };
            let (Some(src), Some(mut tgt)) = (src, tgt) else {
                continue;
            };
            let opt = &self.model.options;
            let lc = self.lag_calendar(r);
            let dd = self.data_date_of(i).unwrap_or(0);
            // FLT-2: a started activity's SS/SF ties count the lag as the
            // forward pass does (corpus 5772; saved probes 302, 303).
            let started = t
                .act_start
                .filter(|_| matches!(l.link_type, LinkType::SS | LinkType::SF));
            if started.is_some() && (opt.progress_override || opt.neither_oos()) {
                continue;
            }
            let ff = match started {
                Some(act) if !opt.lag_from_early_start => {
                    let reach = if l.lag != 0 {
                        lc.finish_at(crate::calendar::note_position(lc.work_at(act) + l.lag).max(0))
                    } else {
                        act
                    };
                    lc.between(reach.max(dd), tgt).max(0)
                }
                _ => {
                    let mut lag = l.lag;
                    if let Some(act) = started {
                        if lag > 0 {
                            lag = (lag - lc.between(act, dd).max(0)).max(0);
                        }
                    }
                    if lag != 0 {
                        tgt = lc.start_at(lc.work_at(tgt) - lag);
                    }
                    self.cal(i).between(src, tgt).max(0)
                }
            };
            if best.is_none_or(|v| ff < v) {
                best = Some(ff);
            }
        }
        best.unwrap_or_else(|| match (t.early_finish, self.scd_end_of(i)) {
            (Some(e), Some(p)) => self.cal(i).between(e, p),
            _ => 0,
        })
    }
    /// A relationship's free float at the current virtual remaining dates.
    pub fn relationship_free_float(&self, r: usize) -> Option<i64> {
        Some(self.relationship_float_and_flag(r).0)
    }
    /// FLT-3, FLT-4: a relationship's driving flag: the float calculation's
    /// flag, suppressed for a mandatory primary successor constraint and for
    /// ties of summaries.
    pub fn relationship_driving(&self, r: usize) -> Option<bool> {
        let l = &self.model.links[r];
        if self.model.activities[l.pred].task_type == TaskType::Wbs
            || self.model.activities[l.succ].task_type == TaskType::Wbs
        {
            // FLT-4: a summary's ties are dropped (probe 601).
            return Some(false);
        }
        if self.model.options.do_level {
            if let Some(value) = crate::features::relationship_driving_leveled(self.model, r) {
                return Some(value);
            }
        }
        let (_, drive) = self.relationship_float_and_flag(r);
        if self.model.activities[l.succ].uses_mandatory_constraint() {
            return Some(false);
        }
        Some(drive)
    }
    /// FLT-5: a tie is replaced by the progress of its ends: FS by an actual
    /// finish of the predecessor or an actual start of the successor, FF by
    /// either actual finish, SF by the predecessor's actual start or the
    /// successor's actual finish, SS by either actual start (saved probes
    /// 302, 508).
    pub fn progress_replaces_tie(&self, r: usize) -> bool {
        let l = &self.model.links[r];
        let p = &self.model.activities[l.pred];
        let s = &self.model.activities[l.succ];
        match l.link_type {
            LinkType::FS => p.act_finish.is_some() || s.act_start.is_some(),
            LinkType::FF => p.act_finish.is_some() || s.act_finish.is_some(),
            LinkType::SF => p.act_start.is_some() || s.act_finish.is_some(),
            LinkType::SS => s.act_start.is_some() || p.act_start.is_some(),
            LinkType::None => false,
        }
    }
    /// (free float, driving flag before the mandatory-successor gate).
    ///
    /// Retained logic: from the predecessor's virtual remaining date (its CPM
    /// early date when completed) plus the effective lag to the successor's
    /// virtual remaining start; a finish-side candidate is first converted to
    /// a start on the successor's calendar.
    ///
    /// FLT-9: by scheduling mode (saved probes 201, 301-304, 508, 509; the
    /// independent relationship answers):
    /// Actual Dates measures from the predecessor's actual finish/start (else
    /// its remaining date) plus the whole lag to the successor's actual
    /// start/finish (else its remaining date), and a mandatory predecessor
    /// gives zero and does not drive; a completed end without remaining
    /// dates (always so under Progress Override) gives zero, driving only
    /// under retained logic; under Progress Override a tie the ends'
    /// progress replaces gives zero and does not drive; missing dates give
    /// zero, not driving; with retained logic and lag from the actual start
    /// an SS tie from a started predecessor gives zero and does not drive,
    /// and one from unstarted work into a started successor drives.
    pub fn relationship_float_and_flag(&self, r: usize) -> (i64, bool) {
        if self.model.options.do_level {
            if let Some(value) = crate::features::relationship_free_float_leveled(self.model, r) {
                return (value, value == 0);
            }
        }
        let opt = &self.model.options;
        let l = &self.model.links[r];
        let p = &self.model.activities[l.pred];
        let s = &self.model.activities[l.succ];
        if p.task_type == TaskType::Loe || s.task_type == TaskType::Loe {
            return (0, true);
        }
        let dep = self.get_logical_dep_type(r);
        let has_dates = |t: &Activity| t.rem_reend.is_some() || t.rem_restart.is_some();
        // Remaining dates count as available when either of the activity's two
        // remaining dates exists, not each side on its own (FLT-9).
        let get = |t: &Activity, end: bool| {
            if has_dates(t) {
                if end {
                    t.rem_reend
                } else {
                    t.rem_restart
                }
            } else if end {
                t.early_finish
            } else {
                t.early_start
            }
        };
        let pred_finish = matches!(dep, LinkType::FS | LinkType::FF);
        if opt.neither_oos() {
            if p.uses_mandatory_constraint() {
                return (0, false);
            }
            let actual = if pred_finish {
                p.act_finish
            } else {
                p.act_start
            };
            let Some(mut base) = actual.or_else(|| get(p, pred_finish)) else {
                return (0, false);
            };
            if l.lag != 0 {
                let lc = self.lag_calendar(r);
                base = if l.lag > 0 {
                    lc.add(base, l.lag)
                } else {
                    lc.sub(base, -l.lag)
                };
            }
            let target = if matches!(dep, LinkType::FS | LinkType::SS) {
                s.act_start.or_else(|| get(s, false))
            } else {
                s.act_finish.or_else(|| get(s, true))
            };
            let Some(target) = target else {
                return (0, false);
            };
            let ff = self.scheduling_cal(l.succ).between(base, target).max(0);
            return (ff, ff == 0);
        }
        let retained = opt.retained_logic;
        for t in [p, s] {
            if t.is_completed() && (!retained || !has_dates(t)) {
                return (0, retained);
            }
        }
        if !retained && self.progress_replaces_tie(r) {
            return (0, false);
        }
        let base = if p.is_completed() {
            if pred_finish {
                p.early_finish
            } else {
                p.early_start
            }
        } else {
            get(p, pred_finish)
        };
        let Some(mut candidate) = base.and_then(|b| self.apply_lag_forward_from(r, b)) else {
            return (0, false);
        };
        let cal = self.scheduling_cal(l.succ);
        if matches!(dep, LinkType::FF | LinkType::SF) {
            candidate = cal.sub(candidate, s.rem);
        }
        let Some(target) = get(s, false) else {
            return (0, false);
        };
        let ff = cal.between(candidate, target).max(0);
        if retained && l.link_type == LinkType::SS && !opt.lag_from_early_start {
            if p.act_start.is_some() {
                return (0, false);
            }
            if s.act_start.is_some() {
                return (ff, true);
            }
        }
        (ff, ff == 0)
    }
    /// FLT-6: start float: working time from early start to late start.
    pub fn calc_start_float(&self, i: usize) -> Option<i64> {
        let t = &self.model.activities[i];
        Some(self.cal(i).between(t.early_start?, t.late_start?))
    }
    /// FLT-6: finish float: working time from early finish to late finish.
    pub fn calc_finish_float(&self, i: usize) -> Option<i64> {
        let t = &self.model.activities[i];
        let (e, l) = (t.early_finish?, t.late_finish?);
        if (self.model.options.neither_oos() || self.model.options.progress_override) && e == l {
            return Some(0);
        }
        Some(self.cal(i).between(e, l))
    }
    /// FLT-6: total float by the float type option: start float, finish
    /// float, or the smaller. In Actual Dates and Progress Override modes an
    /// activity with an expected finish reports its finish float.
    pub fn calc_total_float(&self, i: usize) -> Option<i64> {
        let sf = self.calc_start_float(i);
        let ff = self.calc_finish_float(i);
        let opt = &self.model.options;
        match opt.float_type {
            FloatType::Start => sf.or(ff),
            FloatType::Finish => ff.or(sf),
            FloatType::Min => {
                if (opt.neither_oos() || opt.progress_override)
                    && self.model.activities[i].expect_end.is_some()
                {
                    ff.or(sf)
                } else {
                    match (sf, ff) {
                        (Some(a), Some(b)) => Some(a.min(b)),
                        _ => sf.or(ff),
                    }
                }
            }
        }
    }
}
