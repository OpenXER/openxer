//! Pass drivers and the full schedule run (forward pass, backward pass,
//! float, ALAP, level of effort, summaries, longest path). Rules are listed
//! in `engine/RULES.md`.
use crate::{
    calendar::{CalIndex, CalRef, CalendarAdapter, WorkCalendar, DAY},
    model::*,
};

pub struct Scheduler<'a> {
    pub model: &'a mut Model,
    pub tasks_to_schedule: Vec<usize>,
    pub tasks_for_alap_loe: Vec<usize>,
    pub all_tasks: Vec<usize>,
    pub constrained_tasks: Vec<usize>,
    pub earliest_start: Option<i64>,
    pub latest_end: Option<i64>,
    pub latest_end_start: Option<i64>,
    pub(crate) cal24: CalendarAdapter,
    pub(crate) cal_indices: Vec<CalIndex>,
    pub(crate) cal24_index: CalIndex,
    /// Set when several projects are scheduled as one network.
    pub(crate) partition: Option<Partition>,
    /// Activities leveling left over-allocated because their float would not
    /// allow a delay (LVL-3), by activity ID.
    pub left_over_allocated: Vec<i64>,
}
/// Several projects scheduled as one network. Each activity keeps its own
/// project's data date, late limit and finish; `Model.project` and
/// `Model.options` hold the run's shared values.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Partition {
    /// Project index of each activity (into `projects`).
    pub of: Vec<usize>,
    /// Each project's settings; `scd_end` is written by the run.
    pub projects: Vec<ProjectSettings>,
    /// Per activity after the run: the start limit its predecessors in other
    /// projects gave it (the external early start to store).
    pub external_start: Vec<Option<i64>>,
    /// Per activity after the run: the finish limit its successors in other
    /// projects gave it (the external late finish to store).
    pub external_finish: Vec<Option<i64>>,
}
impl Partition {
    /// Is relationship `r` between activities of different projects?
    pub fn crosses(&self, model: &Model, r: usize) -> bool {
        let l = &model.links[r];
        self.of[l.pred] != self.of[l.succ]
    }
}
/// PASS-7: the work lists of one pass. `ready` is the queue of activities
/// whose dependencies are all scheduled, taken from the front in the order
/// they became ready; `in_pass` and `in_degree` track active pass members and
/// remaining unscheduled dependency edges for O(V + E) Kahn traversal.
#[derive(Clone, Debug, Default)]
pub struct PassLists {
    pub ready: std::collections::VecDeque<usize>,
    in_pass: Vec<bool>,
    in_degree: Vec<usize>,
}
impl PassLists {
    pub fn new(activities: usize) -> Self {
        Self {
            ready: Default::default(),
            in_pass: vec![false; activities],
            in_degree: vec![0; activities],
        }
    }
    /// Move an activity to the back of the ready queue.
    fn enqueue(&mut self, i: usize) {
        self.ready.push_back(i);
    }
}
impl<'a> Scheduler<'a> {
    pub fn new(model: &'a mut Model) -> Self {
        let cal24 = CalendarAdapter::new(WorkCalendar::continuous(model.origin_serial));
        let cal24_index = CalIndex::new(&cal24.cal);
        let cal_indices = model
            .calendars
            .iter()
            .map(|c| CalIndex::new(&c.cal))
            .collect();
        Self {
            model,
            tasks_to_schedule: vec![],
            tasks_for_alap_loe: vec![],
            all_tasks: vec![],
            constrained_tasks: vec![],
            earliest_start: None,
            latest_end: None,
            latest_end_start: None,
            cal24,
            cal_indices,
            cal24_index,
            partition: None,
            left_over_allocated: vec![],
        }
    }
    /// The data date that applies to activity `i`.
    pub(crate) fn data_date_of(&self, i: usize) -> Option<i64> {
        match &self.partition {
            Some(p) => p.projects[p.of[i]].data_date,
            None => self.model.project.data_date,
        }
    }
    /// The late limit (project finish) that applies to activity `i`.
    pub(crate) fn late_limit_of(&self, i: usize) -> Option<i64> {
        let use_must_finish = self.model.options.use_must_finish;
        match &self.partition {
            Some(p) => p.projects[p.of[i]].late_limit(use_must_finish),
            None => self.model.project.late_limit(use_must_finish),
        }
    }
    /// The scheduled finish of activity `i`'s project.
    pub(crate) fn scd_end_of(&self, i: usize) -> Option<i64> {
        match &self.partition {
            Some(p) => p.projects[p.of[i]].scd_end,
            None => self.model.project.scd_end,
        }
    }
    pub(crate) fn cal(&self, i: usize) -> &CalendarAdapter {
        &self.model.calendars[self.model.activities[i].cal]
    }
    pub(crate) fn cal_by_index(&self, idx: usize) -> CalRef<'_> {
        CalRef::new(&self.model.calendars[idx].cal, &self.cal_indices[idx])
    }
    pub(crate) fn cal_ref(&self, i: usize) -> CalRef<'_> {
        self.cal_by_index(self.model.activities[i].cal)
    }
    /// PASS-3: under Retained Logic or Progress Override a completed activity
    /// (other than a summary) is placed on round-the-clock time. Lags and
    /// float keep the activity's own calendar.
    pub(crate) fn scheduling_cal_ref(&self, i: usize) -> CalRef<'_> {
        let task = &self.model.activities[i];
        if task.is_completed()
            && task.task_type != TaskType::Wbs
            && !self.model.options.neither_oos()
        {
            CalRef::new(&self.cal24.cal, &self.cal24_index)
        } else {
            self.cal_ref(i)
        }
    }
    /// PASS-5: level-of-effort and summary activities take no part in the
    /// forward and backward passes, nor in the float of the work around them.
    pub fn skip_type(ty: TaskType) -> bool {
        matches!(ty, TaskType::Loe | TaskType::Wbs)
    }
    /// PASS-4: open work is scheduled; completed work only when logic is
    /// retained. Level-of-effort and as-late-as-possible handling covers open
    /// work only.
    pub fn load_task_lists(&mut self) {
        self.tasks_to_schedule.clear();
        self.tasks_for_alap_loe.clear();
        self.all_tasks.clear();
        self.constrained_tasks.clear();
        for (i, t) in self.model.activities.iter().enumerate() {
            self.all_tasks.push(i);
            if !t.is_completed() {
                self.tasks_to_schedule.push(i);
                self.tasks_for_alap_loe.push(i);
            } else if self.model.options.retained_logic {
                self.tasks_to_schedule.push(i);
            }
            if t.cstr != CstrType::None || t.cstr2 != CstrType::None {
                self.constrained_tasks.push(i);
            }
        }
    }
    /// PASS-7: seed the pass with the activities that have no dependency left
    /// to wait for. The order is not observable in any date (input_order test).
    /// `remaining` lists the pass's activities, each once.
    pub fn create_open_end_list(&self, lists: &mut PassLists, remaining: &[usize], backward: bool) {
        for &i in remaining {
            let t = &self.model.activities[i];
            if !Self::skip_type(t.task_type) {
                lists.in_pass[i] = true;
            }
        }
        for &i in remaining {
            if !lists.in_pass[i] {
                continue;
            }
            let t = &self.model.activities[i];
            let deps = if backward { &t.succs } else { &t.preds };
            let mut deg = 0usize;
            for &r in deps {
                let l = &self.model.links[r];
                let dep = if backward { l.succ } else { l.pred };
                if lists.in_pass[dep] {
                    deg += 1;
                }
            }
            lists.in_degree[i] = deg;
        }
        for &i in remaining.iter().rev() {
            if lists.in_pass[i] && lists.in_degree[i] == 0 {
                lists.enqueue(i);
            }
        }
    }
    /// PASS-7: decrement each dependent's remaining in-degree and enqueue it
    /// once its last dependency has been scheduled. `just` is the activity at
    /// the front of the ready queue, taken off it here.
    pub fn create_todo_next_list(&self, just: usize, lists: &mut PassLists, backward: bool) {
        if lists.ready.front() == Some(&just) {
            lists.ready.pop_front();
        } else {
            lists.ready.retain(|&i| i != just);
        }
        let t = &self.model.activities[just];
        let rels = if backward { &t.preds } else { &t.succs };
        for &r in rels {
            let rel = &self.model.links[r];
            let nxt = if backward { rel.pred } else { rel.succ };
            if lists.in_pass[nxt] && lists.in_degree[nxt] > 0 {
                lists.in_degree[nxt] -= 1;
                if lists.in_degree[nxt] == 0 {
                    lists.enqueue(nxt);
                }
            }
        }
    }
    /// CON-7: mandatory pre-pass, forward. The date comes from the
    /// constraint's early reader, so a Mandatory Finish converts to a start
    /// through the duration and a date outside working time moves on (saved
    /// probe 201). With no reader date (a Mandatory Start on a finish milestone, a
    /// Mandatory Finish on a start milestone, or completed work) the activity
    /// gets no dates from the pass. Work in progress is placed here only when
    /// it is pinned (CON-9); otherwise the passes schedule it by its logic.
    pub fn set_mandatory_dates_forward(&mut self, list: &[usize]) {
        for &i in list {
            let t = &self.model.activities[i];
            if !t.mandatory_pinned() {
                continue;
            }
            let Some(cstr) = self.constraint_early_restart(i, true) else {
                self.model.activities[i].restart = None;
                self.model.activities[i].reend = None;
                continue;
            };
            let t = &self.model.activities[i];
            let restart = cstr.max(self.data_date_of(i).unwrap_or(cstr));
            let (rs, re) = if t.rem == 0 {
                let d = if t.is_completed() {
                    restart
                } else if t.task_type == TaskType::FinMile {
                    self.edge_adjust_early_reend(i, restart)
                } else {
                    self.scheduling_cal_ref(i).snap_start(restart)
                };
                (d, d)
            } else {
                let cal = self.scheduling_cal_ref(i);
                let rs = cal.snap_start(restart);
                (rs, cal.add(rs, t.rem))
            };
            self.model.activities[i].restart = Some(rs);
            self.model.activities[i].reend = Some(re);
            self.rem_finish(i, LinkType::None);
        }
    }
    /// CON-7: mandatory pre-pass, backward: the constraint's late reader
    /// converts a Mandatory Start to a finish through the duration (probe 201); no
    /// reader date leaves the activity without late dates. Work in progress
    /// that is not pinned (CON-9) is left to the backward pass.
    pub fn set_mandatory_dates_backward(&mut self, list: &[usize]) {
        for &i in list {
            let t = &self.model.activities[i];
            if !t.mandatory_pinned() {
                continue;
            }
            let Some(cstr) = self.constraint_late_reend(i, true) else {
                self.model.activities[i].restart = None;
                self.model.activities[i].reend = None;
                continue;
            };
            let limit = match self.late_limit_of(i) {
                Some(l) if l <= cstr => l,
                _ => cstr,
            };
            let limit = self.edge_adjust_late_reend(i, limit);
            let rem = self.model.activities[i].rem;
            let rs = if rem == 0 {
                limit
            } else {
                self.scheduling_cal_ref(i).sub(limit, rem)
            };
            self.model.activities[i].reend = Some(limit);
            self.model.activities[i].restart = Some(rs);
        }
    }
    /// PASS-7: the forward pass over the listed activities, in dependency
    /// order, after the mandatory pre-pass.
    pub fn schedule_forward(&mut self, list: &[usize]) {
        if list.is_empty() {
            return;
        }
        let remaining: Vec<_> = list
            .iter()
            .copied()
            .filter(|&i| {
                let t = &self.model.activities[i];
                !Self::skip_type(t.task_type) && !t.mandatory_pinned()
            })
            .collect();
        self.set_mandatory_dates_forward(list);
        // CON-8: even when every listed activity is mandatory (nothing left
        // for the pass), the pre-pass dates are saved as the early dates.
        let mut lists = PassLists::new(self.model.activities.len());
        self.create_open_end_list(&mut lists, &remaining, false);
        while let Some(&i) = lists.ready.front() {
            self.schedule_forward_task(i);
            self.record_external_start(i);
            self.create_todo_next_list(i, &mut lists, false);
        }
        self.save_early_dates(list);
    }
    /// PASS-7: the backward pass over the listed activities, in dependency
    /// order, after the mandatory pre-pass.
    pub fn schedule_backward(&mut self, list: &[usize]) {
        if list.is_empty() {
            return;
        }
        let remaining: Vec<_> = list
            .iter()
            .copied()
            .filter(|&i| {
                let t = &self.model.activities[i];
                !Self::skip_type(t.task_type) && !t.mandatory_pinned()
            })
            .collect();
        self.set_mandatory_dates_backward(list);
        // CON-8: the same for the late dates.
        let mut lists = PassLists::new(self.model.activities.len());
        self.create_open_end_list(&mut lists, &remaining, true);
        while let Some(&i) = lists.ready.front() {
            self.schedule_backward_task(i);
            self.record_external_finish(i);
            self.create_todo_next_list(i, &mut lists, true);
        }
        self.save_late_dates(list);
    }
    /// PASS-8: the forward pass results become the early dates, and the latest
    /// early finish becomes the scheduled finish of the project. Stored
    /// remaining dates are left as they are here.
    pub fn save_early_dates(&mut self, list: &[usize]) {
        self.earliest_start = None;
        self.latest_end = None;
        self.latest_end_start = None;
        for &i in list {
            let t = &mut self.model.activities[i];
            t.early_start = t.restart;
            t.early_finish = t.reend;
            if !Self::skip_type(t.task_type) {
                for a in &mut t.assignments {
                    a.rem_late_start = t.restart;
                    a.rem_late_finish = t.reend;
                }
            }
            if let Some(s) = t.early_start {
                if self.earliest_start.is_none_or(|x| s < x) {
                    self.earliest_start = Some(s);
                }
            }
            if let Some(e) = t.early_finish {
                if self.latest_end.is_none_or(|x| e > x) {
                    self.latest_end = Some(e);
                    self.latest_end_start = t.early_start;
                }
            }
        }
        self.model.project.scd_end = list
            .iter()
            .filter_map(|&i| self.model.activities[i].early_finish)
            .max();
        if let Some(p) = &mut self.partition {
            for project in &mut p.projects {
                project.scd_end = None;
            }
            for &i in list {
                let finish = self.model.activities[i].early_finish;
                let end = &mut p.projects[p.of[i]].scd_end;
                if let Some(f) = finish {
                    if end.is_none_or(|e| f > e) {
                        *end = Some(f);
                    }
                }
            }
        }
    }
    /// With a partition, keep the start limit set by predecessors in other
    /// projects (no effect on the activity's own dates).
    fn record_external_start(&mut self, i: usize) {
        let Some(p) = &self.partition else {
            return;
        };
        let cross: Vec<usize> = self.model.activities[i]
            .preds
            .iter()
            .copied()
            .filter(|&r| p.crosses(self.model, r))
            .collect();
        if cross.is_empty() {
            return;
        }
        let limit = self.succ_restart_over(i, &cross).0;
        if let Some(p) = &mut self.partition {
            p.external_start[i] = limit;
        }
    }
    /// With a partition, keep the finish limit set by successors in other
    /// projects.
    fn record_external_finish(&mut self, i: usize) {
        let Some(p) = &self.partition else {
            return;
        };
        let cross: Vec<usize> = self.model.activities[i]
            .succs
            .iter()
            .copied()
            .filter(|&r| p.crosses(self.model, r))
            .collect();
        if cross.is_empty() {
            return;
        }
        let limit = self.pred_reend_over(i, &cross).0;
        if let Some(p) = &mut self.partition {
            p.external_finish[i] = limit;
        }
    }
    /// PASS-8: the backward pass results become the late dates and the
    /// remaining late dates; the working dates return to the forward pass
    /// results.
    pub fn save_late_dates(&mut self, list: &[usize]) {
        for &i in list {
            let t = &mut self.model.activities[i];
            t.late_start = t.restart;
            t.late_finish = t.reend;
            t.rem_late_start = t.restart;
            t.rem_late_finish = t.reend;
            for a in &mut t.assignments {
                a.rem_late_start = t.restart;
                a.rem_late_finish = t.reend;
            }
            // The forward working dates return before ALAP, float calculation
            // and leveling. Completed remaining work is zero.
            t.restart = t.early_start;
            t.reend = if t.is_completed() {
                t.early_start
            } else {
                t.early_finish
            };
        }
    }
    /// FLT-8: an as-late-as-possible activity moves later by its free float
    /// and then has none; the free float of its predecessors is recomputed.
    pub fn set_alap_dates(&mut self, list: &[usize], recompute: bool) {
        for &i in list {
            let t = &self.model.activities[i];
            if t.cstr != CstrType::Alap {
                continue;
            }
            let ff = if recompute {
                Some(self.calc_free_float(i))
            } else {
                t.free_float
            };
            let Some(ff) = ff.filter(|&v| v != 0) else {
                continue;
            };
            let Some(re) = t.reend else {
                continue;
            };
            // FLT-8: a start milestone's one date is a start moment, so ALAP
            // moves it to the start edge (corpus 6183).
            let (s, e) = if t.task_type == TaskType::Mile {
                let s = self.cal_ref(i).add_start(re, ff);
                (s, s)
            } else {
                let e = self.cal_ref(i).add(re, ff);
                (self.cal_ref(i).sub(e, t.rem), e)
            };
            let t = &mut self.model.activities[i];
            t.reend = Some(e);
            t.restart = Some(s);
            t.early_start = Some(s);
            t.early_finish = Some(e);
            self.model.activities[i].total_float = self.calc_total_float(i);
            if !recompute {
                let preds = self.model.activities[i].preds.clone();
                for r in preds {
                    let p = self.model.links[r].pred;
                    if !self.model.activities[p].is_completed() {
                        self.calculate_free_float_task(p);
                    }
                }
            }
            self.model.activities[i].free_float = Some(0);
        }
    }
    /// PASS-6: a level-of-effort activity tied start-to-start to one activity
    /// (no lag) and finish-to-finish to another spans from the first's early
    /// start to the second's early finish (corpus 4815, 6031, 369).
    pub fn set_loe_task_dates(&mut self, list: &[usize]) {
        for &i in list {
            let dd = self.data_date_of(i).unwrap_or(0);
            let loe = &self.model.activities[i];
            if loe.task_type != TaskType::Loe || loe.is_completed() {
                continue;
            }
            let mut start = None;
            let mut finish = None;
            let mut other = 0;
            for &r in &loe.preds {
                let l = &self.model.links[r];
                let p = &self.model.activities[l.pred];
                if Self::skip_type(p.task_type) || l.lag != 0 {
                    other += 1;
                } else if l.link_type == LinkType::SS {
                    start = Some(l.pred);
                } else if l.link_type != LinkType::FF {
                    other += 1;
                }
            }
            for &r in &loe.succs {
                let l = &self.model.links[r];
                let s = &self.model.activities[l.succ];
                if Self::skip_type(s.task_type) || l.lag != 0 {
                    other += 1;
                } else if l.link_type == LinkType::FF {
                    finish = Some(l.succ);
                } else {
                    other += 1;
                }
            }
            for &r in &loe.preds {
                let l = &self.model.links[r];
                if l.link_type == LinkType::FF
                    && l.lag == 0
                    && !Self::skip_type(self.model.activities[l.pred].task_type)
                {
                    finish = Some(l.pred);
                }
            }
            let start_count = loe
                .preds
                .iter()
                .filter(|&&r| self.model.links[r].link_type == LinkType::SS)
                .count();
            let finish_count = loe
                .preds
                .iter()
                .chain(&loe.succs)
                .filter(|&&r| self.model.links[r].link_type == LinkType::FF)
                .count();
            let (Some(s), Some(f)) = (start, finish) else {
                continue;
            };
            if other != 0 || start_count != 1 || finish_count != 1 {
                continue;
            }
            let st = &self.model.activities[s];
            let ft = &self.model.activities[f];
            if !((st.status == StatusType::NotStarted && loe.status == StatusType::NotStarted)
                || (st.is_completed() && loe.status == StatusType::InProgress))
            {
                continue;
            }
            let (Some(es), Some(ef), Some(lf)) = (st.early_start, ft.early_finish, ft.late_finish)
            else {
                continue;
            };
            let es = self.cal_ref(i).snap_start(dd.max(es));
            let ef = es.max(ef);
            let dur = self.cal_ref(i).between(es, ef);
            let mut ls = if dur != 0 {
                self.cal_ref(i).sub(lf, dur)
            } else {
                lf
            };
            if st.is_completed() {
                let has_other = st.succs.iter().any(|&r| {
                    let n = self.model.links[r].succ;
                    n != i && !Self::skip_type(self.model.activities[n].task_type)
                });
                if has_other {
                    if let Some(s) = st.late_start {
                        ls = s;
                    }
                }
            } else if let Some(s) = st.late_start {
                ls = s;
            }
            let t = &mut self.model.activities[i];
            t.restart = Some(es);
            t.reend = Some(ef);
            t.early_start = Some(es);
            t.early_finish = Some(ef);
            t.late_start = Some(ls);
            t.late_finish = Some(lf);
            t.rem = 0;
        }
    }
    /// PASS-2: not-started activities take their planned (target) dates from
    /// the scheduled dates.
    pub fn set_target_dates(&mut self, list: &[usize]) {
        for &i in list {
            let t = &mut self.model.activities[i];
            if t.status == StatusType::NotStarted {
                t.target_start = t.restart;
                t.target_finish = t.reend;
            }
        }
    }
    /// PASS-1: a summary's dates are one point, the data date moved to a start
    /// moment, not the span of the activities beneath it (probes 601, 815).
    pub fn set_wbs_dates(&mut self) {
        for i in 0..self.model.activities.len() {
            let t = &self.model.activities[i];
            if t.task_type != TaskType::Wbs || t.is_completed() {
                continue;
            }
            let d = self.data_date_of(i).map(|d| self.cal_ref(i).snap_start(d));
            let t = &mut self.model.activities[i];
            t.early_start = d;
            t.early_finish = d;
            t.late_start = d;
            t.late_finish = d;
            t.restart = d;
            t.reend = d;
            // Its remaining late dates sit at the same point (probe 601).
            t.rem_late_start = d;
            t.rem_late_finish = d;
            t.rem = 0;
        }
    }
    /// FLT-1, FLT-2, PASS-1: total and free float of the listed activities,
    /// either all but summaries and level-of-effort work, or one type only.
    pub fn calculate_float(&mut self, list: &[usize], filter: Option<TaskType>) {
        for &i in list {
            let ty = self.model.activities[i].task_type;
            if filter.map_or(Self::skip_type(ty), |f| f != ty) {
                continue;
            }
            self.model.activities[i].total_float = self.calc_total_float(i);
            self.calculate_free_float_task(i);
        }
    }
    /// The longest path (activity driving flag). It ends at the open
    /// activities (LOE included, WBS summaries excluded) whose early finish
    /// equals the latest early finish of the open non-LOE work, and runs back
    /// through driving relationships only, completed predecessors included.
    /// Matches every stored flag of the 14 unleveled corpus projects.
    pub fn calculate_driving_path(&mut self) {
        for t in &mut self.model.activities {
            t.driving_path = false;
        }
        let open: Vec<usize> = self
            .all_tasks
            .iter()
            .copied()
            .filter(|&i| {
                let t = &self.model.activities[i];
                !t.is_completed() && t.early_finish.is_some() && t.task_type != TaskType::Wbs
            })
            .collect();
        // Each project of a partition has its own longest path.
        let part = |i: usize| self.partition.as_ref().map_or(0, |p| p.of[i]);
        let parts = self.partition.as_ref().map_or(1, |p| p.projects.len());
        let mut last = vec![None; parts];
        for &i in &open {
            let t = &self.model.activities[i];
            if t.task_type == TaskType::Loe {
                continue;
            }
            if let Some(f) = t.early_finish {
                let l = &mut last[part(i)];
                if l.is_none_or(|v| f > v) {
                    *l = Some(f);
                }
            }
        }
        let mut stack: Vec<usize> = open
            .into_iter()
            .filter(|&i| {
                let l = last[part(i)];
                l.is_some() && self.model.activities[i].early_finish == l
            })
            .collect();
        for &i in &stack {
            self.model.activities[i].driving_path = true;
        }
        while let Some(i) = stack.pop() {
            let preds = self.model.activities[i].preds.clone();
            for r in preds {
                let p = self.model.links[r].pred;
                let t = &self.model.activities[p];
                if t.driving_path || t.task_type == TaskType::Wbs {
                    continue;
                }
                if self.relationship_driving(r) == Some(true) {
                    self.model.activities[p].driving_path = true;
                    stack.push(p);
                }
            }
        }
    }
    /// PASS-3: a completed activity keeps no remaining dates, expected
    /// finish or float; unless logic is retained it keeps no early or late
    /// dates either.
    pub fn clear_completed_dates(&mut self) {
        for t in &mut self.model.activities {
            if !t.is_completed() {
                continue;
            }
            t.restart = None;
            t.reend = None;
            t.rem_late_start = None;
            t.rem_late_finish = None;
            t.expect_end = None;
            t.total_float = None;
            t.free_float = None;
            if !self.model.options.retained_logic {
                t.early_start = None;
                t.early_finish = None;
                t.late_start = None;
                t.late_finish = None;
            }
        }
    }
    /// PASS-8: a fresh schedule, in this order: forward pass, backward pass,
    /// as-late-as-possible, float, level of effort, planned dates, summaries,
    /// leveling, longest path, float paths, completed-work cleanup.
    pub fn schedule_now(&mut self) -> Result<(), String> {
        self.schedule_now_with_resources(None)
    }
    pub fn schedule_now_with_resources(
        &mut self,
        resources: Option<&crate::features::LevelingInputs>,
    ) -> Result<(), String> {
        // Imported remaining dates describe a previous run, not input anchors.
        for t in &mut self.model.activities {
            t.rem_restart = None;
            t.rem_reend = None;
        }
        self.load_task_lists();
        if self.tasks_to_schedule.is_empty() {
            return Ok(());
        }
        let scheduled = self.tasks_to_schedule.clone();
        let alap_loe = self.tasks_for_alap_loe.clone();
        self.schedule_forward(&scheduled);
        self.schedule_backward(&scheduled);
        if !self.model.options.do_level {
            self.set_alap_dates(&alap_loe, true);
        }
        self.calculate_float(&scheduled, None);
        if !self.model.options.do_level {
            self.set_alap_dates(&alap_loe, false);
        }
        self.set_loe_task_dates(&alap_loe);
        // PASS-6: an open LOE carries remaining late dates equal to its late
        // dates and a total float computed as for any activity (corpus 369,
        // 4815, 6031; saved probes 510-512).
        for &i in &alap_loe {
            let t = &self.model.activities[i];
            if t.task_type == TaskType::Loe && !t.is_completed() && t.late_start.is_some() {
                let (ls, lf) = (t.late_start, t.late_finish);
                self.model.activities[i].rem_late_start = ls;
                self.model.activities[i].rem_late_finish = lf;
                self.model.activities[i].total_float = self.calc_total_float(i);
            }
        }
        crate::features::additional_loe_dates(self);
        // PASS-6: an open LOE's free float is zero, dated or not.
        for t in &mut self.model.activities {
            if t.task_type == TaskType::Loe && !t.is_completed() {
                t.free_float = Some(0);
            }
        }
        self.set_target_dates(&alap_loe);
        self.set_wbs_dates();
        self.calculate_float(&self.all_tasks.clone(), Some(TaskType::Wbs));
        // Completed output dates are cleared at the end, but relationship
        // calculations still need their internal remaining state.
        for t in &mut self.model.activities {
            t.rem_restart = t.restart;
            t.rem_reend = t.reend;
        }
        if self.model.options.do_level {
            let inputs = resources.ok_or("Resource leveling requires resource inputs")?;
            self.left_over_allocated = crate::features::level_resources_report(self.model, inputs)?;
            // LVL-6: leveled only within float, the project finish is the
            // latest leveled finish of the open work (a private daily export).
            if inputs.within_float {
                let leveled = self
                    .model
                    .activities
                    .iter()
                    .filter(|a| !a.is_completed() && !Self::skip_type(a.task_type))
                    .filter_map(|a| a.rem_reend)
                    .max();
                if leveled.is_some() {
                    self.model.project.scd_end = leveled;
                }
            }
        }
        if self.model.options.do_level {
            crate::features::calculate_driving_path_after_level(self.model);
        } else {
            self.calculate_driving_path();
        }
        crate::features::calculate_multiple_float_paths(self.model)?;
        self.clear_completed_dates();
        Ok(())
    }
}
/// Remaining duration of every open LOE: the span of its remaining dates on
/// its own calendar. The scheduling passes treat an LOE as having no work of
/// its own; the stored duration follows the dates the passes gave it.
pub fn set_loe_remaining_durations(model: &mut Model) {
    let mut indices: Vec<Option<CalIndex>> = vec![None; model.calendars.len()];
    for i in 0..model.activities.len() {
        let t = &model.activities[i];
        if t.task_type != TaskType::Loe || t.is_completed() {
            continue;
        }
        if let (Some(start), Some(end)) = (t.restart, t.reend) {
            let cal = &model.calendars[t.cal].cal;
            let ix = indices[t.cal].get_or_insert_with(|| CalIndex::new(cal));
            let span = CalRef::new(cal, ix).between(start, end);
            model.activities[i].rem = span.max(0);
        }
    }
}
/// Validated application entry point. Differential harness invokes Scheduler directly.
pub fn schedule(model: &mut Model) -> Result<(), String> {
    model.validate()?;
    if model.options.do_level {
        return Err(
            "Resource leveling needs resource limits and assignments; use schedule_with_resources"
                .into(),
        );
    }
    let (working, ()) = run_clear_of_origin(model, |working, _| {
        Scheduler::new(working).schedule_now()?;
        set_loe_remaining_durations(working);
        Ok(())
    })?;
    *model = working;
    Ok(())
}
/// How many times a run that reached before the origin is repeated from an
/// earlier one; the last attempt counts back about 128 years.
const ORIGIN_ATTEMPTS: u32 = 8;
/// BWD-5, CAL-10: the position arithmetic counts work from the model's origin
/// day and cannot reach before it, so a late date before the origin would be
/// clamped at it. `run` schedules a fresh copy of `model`; when the run
/// reached before the origin (the calendar's before-origin mark), it is run
/// again on a copy counted from an earlier origin and its results are moved
/// back to the model's origin, where a date before it is a negative minute.
/// `run` also gets the minutes every date was moved forward, for inputs it
/// holds outside the model. A run that stays after the origin runs once, as
/// before.
fn run_clear_of_origin<T>(
    model: &Model,
    mut run: impl FnMut(&mut Model, i64) -> Result<T, String>,
) -> Result<(Model, T), String> {
    for attempt in 0..=ORIGIN_ATTEMPTS {
        let mut working = model.clone();
        working.rebuild_links()?;
        clear_calculated(&mut working);
        let back = days_before_origin(&mut working, attempt);
        if back > 0 {
            working.rebase(model.origin_serial - back);
        }
        crate::calendar::clear_before_origin();
        let out = run(&mut working, back * DAY);
        let reached = crate::calendar::take_before_origin();
        let out = out?;
        if !reached {
            if back > 0 {
                working.rebase(model.origin_serial);
            }
            return Ok((working, out));
        }
    }
    Err(format!(
        "The schedule reaches more than {} years before its dates (check constraint dates, lags and durations)",
        (366_i64 << (ORIGIN_ATTEMPTS - 1)) / 366
    ))
}
/// Days to move the origin back on `attempt` (none on the first): enough to
/// bring every stored date to or after the origin, plus a year doubled on
/// each further attempt.
fn days_before_origin(model: &mut Model, attempt: u32) -> i64 {
    if attempt == 0 {
        return 0;
    }
    let mut earliest = 0_i64;
    for a in &mut model.activities {
        for date in a.dates_mut().into_iter().flatten() {
            earliest = earliest.min(*date);
        }
    }
    let p = &model.project;
    for date in [
        p.data_date,
        p.scd_end,
        p.plan_start,
        p.plan_end,
        p.last_schedule,
    ]
    .into_iter()
    .flatten()
    {
        earliest = earliest.min(date);
    }
    -earliest.div_euclid(DAY) + (366_i64 << (attempt - 1))
}
fn clear_calculated(model: &mut Model) {
    model.project.scd_end = None;
    for a in &mut model.activities {
        a.restart = None;
        a.reend = None;
        a.early_start = None;
        a.early_finish = None;
        a.late_start = None;
        a.late_finish = None;
        a.rem_late_start = None;
        a.rem_late_finish = None;
        a.total_float = None;
        a.free_float = None;
        a.driving_path = false;
        a.float_path = 0;
        a.float_path_order = 0;
    }
}

/// Schedule several projects as one network (see `Partition`). `model`
/// holds every activity and link, cross-project links included; its
/// `project` supplies the shared origin and `options` the run's options.
/// Returns the partition with each project's finish and the external
/// limits filled in. Resource leveling is not supported here.
pub fn schedule_together(model: &mut Model, mut partition: Partition) -> Result<Partition, String> {
    if partition.of.len() != model.activities.len()
        || partition.of.iter().any(|&p| p >= partition.projects.len())
    {
        return Err("Every activity needs a project of the partition".into());
    }
    if model.options.do_level {
        return Err("Leveling several projects scheduled together is not supported".into());
    }
    for (index, project) in partition.projects.iter().enumerate() {
        if project.data_date.is_none() && partition.of.contains(&index) {
            return Err(format!("Project {} has no data date", project.name));
        }
        for minute in [project.data_date, project.plan_end, project.scd_end]
            .into_iter()
            .flatten()
        {
            validate_minute(minute, model.origin_serial)?;
        }
    }
    model.validate()?;
    for project in &mut partition.projects {
        project.scd_end = None;
    }
    let n = model.activities.len();
    partition.external_start = vec![None; n];
    partition.external_finish = vec![None; n];
    let (working, partition) = run_clear_of_origin(model, |working, shift| {
        let mut part = partition.clone();
        for project in &mut part.projects {
            project.shift_dates(shift);
        }
        let mut scheduler = Scheduler::new(working);
        scheduler.partition = Some(part);
        scheduler.schedule_now()?;
        let mut part = scheduler.partition.take().expect("set above");
        set_loe_remaining_durations(working);
        // Back to the model's origin.
        for project in &mut part.projects {
            project.shift_dates(-shift);
        }
        for date in part
            .external_start
            .iter_mut()
            .chain(part.external_finish.iter_mut())
            .flatten()
        {
            *date -= shift;
        }
        Ok(part)
    })?;
    *model = working;
    Ok(partition)
}

/// Schedule and level atomically; failed validation/placement leaves the project unchanged.
pub fn schedule_with_resources(
    model: &mut Model,
    inputs: &crate::features::LevelingInputs,
) -> Result<(), String> {
    schedule_with_resources_report(model, inputs).map(|_| ())
}

/// As `schedule_with_resources`, returning the IDs of the activities that
/// leveling left over-allocated because their float would not allow a delay
/// (LVL-3).
pub fn schedule_with_resources_report(
    model: &mut Model,
    inputs: &crate::features::LevelingInputs,
) -> Result<Vec<i64>, String> {
    model.validate()?;
    let (working, held) = run_clear_of_origin(model, |working, shift| {
        let moved;
        let inputs = if shift == 0 {
            inputs
        } else {
            let mut copy = inputs.clone();
            for limits in copy.limits.values_mut() {
                for (at, _) in limits.iter_mut() {
                    *at += shift;
                }
            }
            if copy.horizon > 0 {
                copy.horizon += shift;
            }
            moved = copy;
            &moved
        };
        let mut scheduler = Scheduler::new(working);
        scheduler.schedule_now_with_resources(Some(inputs))?;
        let held = std::mem::take(&mut scheduler.left_over_allocated);
        set_loe_remaining_durations(working);
        Ok(held)
    })?;
    *model = working;
    Ok(held)
}
