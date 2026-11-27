//! Additional scheduling capabilities: resource leveling, general level-of-
//! effort spans and multiple float paths. Leveling is checked against saved
//! probe outputs (LVL-1). General LOE shapes and float-path tie breaking have
//! no independent reference outputs (FEAT-1, FEAT-2 in `engine/RULES.md`).
use crate::{
    calendar::{CalIndex, CalRef, CalendarAdapter, WorkCalendar, DAY},
    model::*,
    options::LagCalendar,
    scheduler::Scheduler,
};
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

#[derive(Clone, Debug)]
pub struct ResourceDemand {
    pub resource_id: i64,
    pub rate: f64,
    pub remaining_units: f64,
    pub duration: i64,
    pub calendar: Option<usize>,
}
#[derive(Clone, Debug)]
pub enum PriorityField {
    ActivityPriority,
    TotalFloat,
    EarlyStart,
    LateStart,
    RemainingDuration,
    Wbs,
    ActivityCode(i64),
}
#[derive(Clone, Debug)]
pub struct LevelPriority {
    pub field: PriorityField,
    pub ascending: bool,
}
#[derive(Clone, Debug, Default)]
pub struct LevelingInputs {
    pub limits: BTreeMap<i64, Vec<(i64, f64)>>,
    pub demands: BTreeMap<i64, Vec<ResourceDemand>>,
    pub priorities: Vec<LevelPriority>,
    pub activity_priority: BTreeMap<i64, i64>,
    pub wbs_paths: BTreeMap<i64, Vec<(i64, i64)>>,
    pub activity_codes: BTreeMap<(i64, i64), String>,
    pub resource_dependent: BTreeSet<i64>,
    /// Resources the leveler places. `None` places every resource that has a
    /// limit (LVL-4); a resource outside the set is booked against no limit.
    pub selected: Option<BTreeSet<i64>>,
    /// Delay an activity only while its float allows (LVL-3).
    pub within_float: bool,
    /// Float, in working minutes of the activity's calendar, that a delay
    /// must leave. Read only with `within_float` (LVL-3).
    pub preserve_float: i64,
    pub horizon: i64,
    pub unsupported: Vec<String>,
}
/// FEAT-1: a level-of-effort activity not dated by the base rule (PASS-6)
/// spans its tied activities' earliest start to latest finish. Its late
/// dates sit on its own calendar (`tests/loe_rules.rs`). No reference output
/// exists for the shapes beyond PASS-6.
pub fn set_additional_loe_dates(model: &mut Model) {
    additional_loe_dates(&mut Scheduler::new(model));
}
/// `set_additional_loe_dates` on a scheduler, so a partition's per-project
/// data dates and limits apply.
pub(crate) fn additional_loe_dates(sched: &mut Scheduler) {
    for i in 0..sched.model.activities.len() {
        let t = &sched.model.activities[i];
        if t.task_type != TaskType::Loe || t.is_completed() || t.early_start.is_some() {
            continue;
        }
        let early = |a: &Activity, _: usize| (a.early_start, a.early_finish);
        let [esv, efv, lsv, lfv] = loe_partner_dates(sched, i, &early);
        if esv.is_empty() && efv.is_empty() {
            continue;
        }
        let dd = sched.data_date_of(i).unwrap_or(0);
        let c = sched.cal_ref(i);
        let es = c.snap_start(dd.max(esv.iter().copied().min().unwrap_or(dd)));
        let ef = es.max(efv.iter().copied().max().unwrap_or(es));
        let limit = sched.late_limit_of(i);
        let mut lf = lfv.into_iter().max().unwrap_or(limit.unwrap_or(ef));
        if let Some(l) = limit {
            lf = lf.min(l);
        }
        let duration = c.between(es, ef);
        let ls = lsv.into_iter().min().unwrap_or_else(|| {
            if duration != 0 {
                c.sub(lf, duration)
            } else {
                lf
            }
        });
        // The late dates sit on the LOE's own calendar: the finish rolls back
        // to the last working finish, the start forward to the next working
        // start, and the start never passes the finish (a completed
        // predecessor with no later successor carries the project limit).
        let lf = c.snap_finish(lf);
        let ls = c.snap_start(ls).min(lf);
        let t = &mut sched.model.activities[i];
        t.restart = Some(es);
        t.early_start = Some(es);
        t.reend = Some(ef);
        t.early_finish = Some(ef);
        t.late_start = Some(ls);
        t.late_finish = Some(lf);
        t.rem_late_start = Some(ls);
        t.rem_late_finish = Some(lf);
        t.rem = 0;
        t.free_float = Some(0);
        sched.model.activities[i].total_float = sched.calc_total_float(i);
    }
}
/// A partner's (start, finish) as a level-of-effort activity reads it.
type PartnerDates<'a> = dyn Fn(&Activity, usize) -> (Option<i64>, Option<i64>) + 'a;
/// FEAT-1: the candidate dates a level-of-effort activity takes from the
/// activities it is tied to, whatever the direction of the tie: early start,
/// early finish, late start and late finish candidates, in that order. A
/// partner's early dates come from `early` (the scheduled early dates, or the
/// leveled dates after leveling); actual dates replace them.
fn loe_partner_dates(sched: &Scheduler, i: usize, early: &PartnerDates<'_>) -> [Vec<i64>; 4] {
    let t = &sched.model.activities[i];
    let mut esv = vec![];
    let mut efv = vec![];
    let mut lsv = vec![];
    let mut lfv = vec![];
    for (outgoing, rels) in [(false, &t.preds), (true, &t.succs)] {
        for &ri in rels {
            let l = &sched.model.links[ri];
            let oi = if outgoing { l.succ } else { l.pred };
            let other = &sched.model.activities[oi];
            if oi == i || Scheduler::skip_type(other.task_type) {
                continue;
            }
            let dep = sched.get_logical_dep_type(ri);
            let other_finish = if outgoing {
                matches!(dep, LinkType::FF | LinkType::SF)
            } else {
                matches!(dep, LinkType::FS | LinkType::FF)
            };
            let own_finish = if outgoing {
                matches!(dep, LinkType::FS | LinkType::FF)
            } else {
                matches!(dep, LinkType::FF | LinkType::SF)
            };
            let c = sched.lag_cal_ref(ri);
            let (other_start, other_end) = early(other, oi);
            for early in [true, false] {
                let mut date = match (early, other_finish) {
                    (true, true) => other_end,
                    (true, false) => other_start,
                    (false, true) => other.late_finish,
                    (false, false) => other.late_start,
                };
                if other_finish && other.act_finish.is_some() {
                    date = other.act_finish;
                } else if early && !other_finish && other.act_start.is_some() {
                    date = other.act_start;
                }
                if let Some(mut d) = date {
                    if l.lag != 0 {
                        d = if outgoing {
                            c.sub(d, l.lag)
                        } else {
                            c.add_start(d, l.lag)
                        };
                    }
                    match (early, own_finish) {
                        (true, true) => efv.push(d),
                        (true, false) => esv.push(d),
                        (false, true) => lfv.push(d),
                        (false, false) => lsv.push(d),
                    }
                }
            }
        }
    }
    [esv, efv, lsv, lfv]
}
fn path_score(m: &Model, ri: usize, ranks: &EdgeRanks) -> i64 {
    let l = &m.links[ri];
    let p = &m.activities[l.pred];
    let s = &m.activities[l.succ];
    let src = if matches!(l.link_type, LinkType::FS | LinkType::FF) {
        p.early_finish
    } else {
        p.early_start
    };
    let target = match (
        m.options.float_paths_use_total,
        matches!(l.link_type, LinkType::FS | LinkType::SS),
    ) {
        (true, true) => s.late_start,
        (true, false) => s.late_finish,
        (false, true) => s.early_start,
        (false, false) => s.early_finish,
    };
    let (Some(src), Some(mut target)) = (src, target) else {
        return 1_i64 << 62;
    };
    if l.lag != 0 {
        let c = ranks.lag_cal(m, l);
        target = c.start_at(c.work_at(target) - l.lag);
    }
    let v = ranks.cal(m, p.cal).between(src, target);
    if m.options.float_paths_use_total {
        v
    } else {
        v.max(0)
    }
}
/// FEAT-2: the ranking of every relationship for one float-path run, each
/// worked out once on first use. The ranking reads only dates, lags and IDs,
/// which a run never changes, so every path of the run ranks a relationship
/// the same way; asking again only repeats the calendar arithmetic, which made
/// a run cost paths times relationships calendar evaluations.
struct EdgeRanks {
    c24: CalendarAdapter,
    c24_ix: CalIndex,
    cal_indices: Vec<CalIndex>,
    scores: Vec<std::cell::OnceCell<i64>>,
}
impl EdgeRanks {
    fn new(model: &Model) -> Self {
        let c24 = CalendarAdapter::new(WorkCalendar::continuous(model.origin_serial));
        let c24_ix = CalIndex::new(&c24.cal);
        let cal_indices = model
            .calendars
            .iter()
            .map(|c| CalIndex::new(&c.cal))
            .collect();
        Self {
            c24,
            c24_ix,
            cal_indices,
            scores: vec![std::cell::OnceCell::new(); model.links.len()],
        }
    }
    fn cal<'a>(&'a self, m: &'a Model, idx: usize) -> CalRef<'a> {
        CalRef::new(&m.calendars[idx].cal, &self.cal_indices[idx])
    }
    fn lag_cal<'a>(&'a self, m: &'a Model, l: &Link) -> CalRef<'a> {
        match m.options.lag_calendar {
            LagCalendar::PredCal => self.cal(m, m.activities[l.pred].cal),
            LagCalendar::SuccCal => self.cal(m, m.activities[l.succ].cal),
            LagCalendar::Hour24 => CalRef::new(&self.c24.cal, &self.c24_ix),
            LagCalendar::ProjCal => self.cal(m, m.project_calendar),
        }
    }
    /// Relationship `r`'s score in `m`, the model the ranks were made for.
    fn score(&self, m: &Model, r: usize) -> i64 {
        *self.scores[r].get_or_init(|| path_score(m, r, self))
    }
    fn cmp_edges(&self, m: &Model, a: usize, b: usize) -> std::cmp::Ordering {
        let sa = self.score(m, a);
        let sb = self.score(m, b);
        if sa != sb {
            return sa.cmp(&sb);
        }
        let la = &m.links[a];
        let lb = &m.links[b];
        let pa = &m.activities[la.pred];
        let pb = &m.activities[lb.pred];
        let suc_a = &m.activities[la.succ];
        let suc_b = &m.activities[lb.succ];
        pa.task_code
            .cmp(&pb.task_code)
            .then_with(|| pa.task_id.cmp(&pb.task_id))
            .then_with(|| suc_a.task_code.cmp(&suc_b.task_code))
            .then_with(|| suc_a.task_id.cmp(&suc_b.task_id))
    }
}
fn trace_path(
    m: &Model,
    seed: usize,
    back: bool,
    remaining: &[bool],
    ranks: &EdgeRanks,
    seen_epoch: &mut [u32],
    epoch: u32,
) -> Vec<usize> {
    let mut chain = vec![];
    let mut i = seed;
    while remaining[i] && seen_epoch[i] != epoch {
        seen_epoch[i] = epoch;
        chain.push(i);
        let links = if back {
            &m.activities[i].preds
        } else {
            &m.activities[i].succs
        };
        let mut best: Option<usize> = None;
        for &r in links {
            let l = &m.links[r];
            let n = if back { l.pred } else { l.succ };
            if !remaining[n] || seen_epoch[n] == epoch {
                continue;
            }
            if best.is_none_or(|b| ranks.cmp_edges(m, r, b) == std::cmp::Ordering::Less) {
                best = Some(r);
            }
        }
        let Some(r) = best else {
            break;
        };
        i = if back {
            m.links[r].pred
        } else {
            m.links[r].succ
        };
    }
    chain
}
/// FEAT-2: distinct float paths in a fixed order; this never changes
/// schedule dates or critical flags. The tie-break order has no reference
/// output.
pub fn calculate_multiple_float_paths(model: &mut Model) -> Result<(), String> {
    for a in &mut model.activities {
        a.float_path = 0;
        a.float_path_order = 0;
    }
    if !model.options.multiple_float_paths {
        return Ok(());
    }
    if model.options.max_float_paths == 0 {
        return Err("max_float_paths must be positive".into());
    }
    let n = model.activities.len();
    let mut remaining = vec![false; n];
    let mut remaining_count = 0usize;
    for (i, a) in model.activities.iter().enumerate() {
        if !Scheduler::skip_type(a.task_type) && a.early_start.is_some() && a.early_finish.is_some()
        {
            remaining[i] = true;
            remaining_count += 1;
        }
    }
    if remaining_count == 0 {
        return Ok(());
    }
    let endpoint = if let Some(id) = model.options.float_paths_end_activity {
        Some(
            (0..n)
                .find(|&i| remaining[i] && model.activities[i].task_id == id)
                .ok_or("The float-path ending activity is missing or undated")?,
        )
    } else {
        None
    };
    if let Some(end) = endpoint {
        let mut allowed = vec![false; n];
        allowed[end] = true;
        let mut allowed_count = 1usize;
        let mut stack = vec![end];
        while let Some(i) = stack.pop() {
            for &r in &model.activities[i].preds {
                let p = model.links[r].pred;
                if remaining[p] && !allowed[p] {
                    allowed[p] = true;
                    allowed_count += 1;
                    stack.push(p);
                }
            }
        }
        remaining = allowed;
        remaining_count = allowed_count;
    }
    let mut edges: Vec<usize> = model
        .links
        .iter()
        .enumerate()
        .filter_map(|(r, l)| (remaining[l.pred] && remaining[l.succ]).then_some(r))
        .collect();
    let ranks = EdgeRanks::new(model);
    if model.options.float_paths_use_total {
        edges.sort_by(|&a, &b| ranks.cmp_edges(model, a, b));
    }
    let mut edge_cursor = 0usize;
    let mut seen_epoch = vec![0u32; n];
    let mut chain_epoch = vec![0u32; n];
    let mut epoch = 0u32;
    for path in 1..=model.options.max_float_paths {
        if remaining_count == 0 {
            break;
        }
        let mut chain;
        if model.options.float_paths_use_total {
            while edge_cursor < edges.len() {
                let l = &model.links[edges[edge_cursor]];
                if remaining[l.pred] && remaining[l.succ] {
                    break;
                }
                edge_cursor += 1;
            }
            if edge_cursor < edges.len() {
                let l = &model.links[edges[edge_cursor]];
                epoch += 1;
                chain = trace_path(
                    model,
                    l.pred,
                    true,
                    &remaining,
                    &ranks,
                    &mut seen_epoch,
                    epoch,
                );
                chain.reverse();
                epoch += 1;
                for &x in &chain {
                    chain_epoch[x] = epoch;
                }
                let right = trace_path(
                    model,
                    l.succ,
                    false,
                    &remaining,
                    &ranks,
                    &mut seen_epoch,
                    epoch,
                );
                for i in right {
                    if chain_epoch[i] != epoch {
                        chain_epoch[i] = epoch;
                        chain.push(i);
                    }
                }
            } else {
                let i = (0..n)
                    .filter(|&i| remaining[i])
                    .min_by(|&a, &b| {
                        let ta = &model.activities[a];
                        let tb = &model.activities[b];
                        ta.total_float
                            .unwrap_or(1_i64 << 62)
                            .cmp(&tb.total_float.unwrap_or(1_i64 << 62))
                            .then_with(|| ta.task_code.cmp(&tb.task_code))
                            .then_with(|| ta.task_id.cmp(&tb.task_id))
                    })
                    .ok_or("No float-path candidate")?;
                chain = vec![i];
            }
        } else {
            let mut boundary: Option<usize> = None;
            for &r in &edges {
                let l = &model.links[r];
                if remaining[l.pred]
                    && !remaining[l.succ]
                    && boundary
                        .is_none_or(|b| ranks.cmp_edges(model, r, b) == std::cmp::Ordering::Less)
                {
                    boundary = Some(r);
                }
            }
            let seed = if path == 1 {
                endpoint
            } else {
                boundary.map(|r| model.links[r].pred)
            };
            let seed = seed
                .or_else(|| {
                    (0..n).filter(|&i| remaining[i]).min_by(|&a, &b| {
                        let ta = &model.activities[a];
                        let tb = &model.activities[b];
                        (-ta.early_finish.unwrap_or(0))
                            .cmp(&(-tb.early_finish.unwrap_or(0)))
                            .then_with(|| ta.task_code.cmp(&tb.task_code))
                            .then_with(|| ta.task_id.cmp(&tb.task_id))
                    })
                })
                .ok_or("No float-path candidate")?;
            epoch += 1;
            chain = trace_path(
                model,
                seed,
                true,
                &remaining,
                &ranks,
                &mut seen_epoch,
                epoch,
            );
            chain.reverse();
        }
        for (order, i) in chain.into_iter().enumerate() {
            model.activities[i].float_path = path as i64;
            model.activities[i].float_path_order = order as i64 + 1;
            if remaining[i] {
                remaining[i] = false;
                remaining_count -= 1;
            }
        }
    }
    Ok(())
}

/// Activity critical paths through one activity, most critical first, each
/// in path order. Backward paths end at `seed`: they are the multiple float
/// paths the scheduler computes with `seed` as the ending activity, so path 1
/// is the Float Path 1 column for that ending activity. Forward paths start
/// at `seed` and follow the most critical successor relationships through the
/// activities `seed` leads to, using the same relationship ranking. Dates and
/// floats are read from the model as scheduled; nothing in it changes.
pub fn activity_paths(
    model: &Model,
    seed: usize,
    forward: bool,
    count: usize,
) -> Result<Vec<Vec<usize>>, String> {
    if seed >= model.activities.len() {
        return Err("The selected activity is missing".into());
    }
    if count == 0 {
        return Err("The path count must be positive".into());
    }
    if !forward {
        let mut m = model.clone();
        m.rebuild_links()?;
        m.options.multiple_float_paths = true;
        m.options.float_paths_end_activity = Some(m.activities[seed].task_id);
        m.options.max_float_paths = count;
        calculate_multiple_float_paths(&mut m)?;
        let mut paths: Vec<Vec<(i64, usize)>> = vec![vec![]; count];
        for (i, a) in m.activities.iter().enumerate() {
            if a.float_path >= 1 && a.float_path as usize <= count {
                paths[a.float_path as usize - 1].push((a.float_path_order, i));
            }
        }
        return Ok(paths
            .into_iter()
            .filter(|p| !p.is_empty())
            .map(|mut p| {
                p.sort();
                p.into_iter().map(|(_, i)| i).collect()
            })
            .collect());
    }
    let dated = |i: usize| {
        let a = &model.activities[i];
        !Scheduler::skip_type(a.task_type) && a.early_start.is_some() && a.early_finish.is_some()
    };
    if !dated(seed) {
        return Err("The selected activity has no scheduled dates".into());
    }
    let n = model.activities.len();
    let mut succs: Vec<Vec<usize>> = vec![vec![]; n];
    for (r, l) in model.links.iter().enumerate() {
        if l.pred >= n || l.succ >= n {
            return Err(format!("Link {r} references a missing activity"));
        }
        succs[l.pred].push(r);
    }
    // The seed and every dated activity it leads to.
    let mut remaining = vec![false; n];
    remaining[seed] = true;
    let mut remaining_count = 1usize;
    let mut stack = vec![seed];
    while let Some(i) = stack.pop() {
        for &r in &succs[i] {
            let s = model.links[r].succ;
            if dated(s) && !remaining[s] {
                remaining[s] = true;
                remaining_count += 1;
                stack.push(s);
            }
        }
    }
    let ranks = EdgeRanks::new(model);
    let mut done = vec![false; n];
    let mut paths = vec![];
    let mut seen_epoch = vec![0u32; n];
    let mut epoch = 0u32;
    while paths.len() < count && remaining_count > 0 {
        // Path 1 starts at the seed; later paths branch off the paths
        // already taken through their most critical relationship.
        let start = if paths.is_empty() {
            seed
        } else {
            let mut branch: Option<usize> = None;
            for (r, l) in model.links.iter().enumerate() {
                if done[l.pred]
                    && remaining[l.succ]
                    && branch
                        .is_none_or(|b| ranks.cmp_edges(model, r, b) == std::cmp::Ordering::Less)
                {
                    branch = Some(r);
                }
            }
            match branch {
                Some(r) => model.links[r].succ,
                None => break,
            }
        };
        epoch += 1;
        let mut chain = vec![];
        let mut cur = start;
        while remaining[cur] && seen_epoch[cur] != epoch {
            seen_epoch[cur] = epoch;
            chain.push(cur);
            let mut best: Option<usize> = None;
            for &r in &succs[cur] {
                let next_node = model.links[r].succ;
                if !remaining[next_node] || seen_epoch[next_node] == epoch {
                    continue;
                }
                if best.is_none_or(|b| ranks.cmp_edges(model, r, b) == std::cmp::Ordering::Less) {
                    best = Some(r);
                }
            }
            let Some(r) = best else {
                break;
            };
            cur = model.links[r].succ;
        }
        for &i in &chain {
            if remaining[i] {
                remaining[i] = false;
                remaining_count -= 1;
            }
            done[i] = true;
        }
        paths.push(chain);
    }
    Ok(paths)
}

// LVL-1: the resource leveler works on a merged interval view of each
// calendar that starts two days before the origin. This arithmetic is
// deliberately separate from the CPM calendar arithmetic; the leveling
// probes 801-817 and the leveled corpus files pin it.
#[derive(Clone, Debug)]
struct LevelCalendar {
    s: Vec<i64>,
    e: Vec<i64>,
    cum: Vec<i64>,
    horizon: i64,
}
impl LevelCalendar {
    fn empty(horizon: i64) -> Self {
        Self {
            s: vec![],
            e: vec![],
            cum: vec![0],
            horizon,
        }
    }
    fn new(c: &CalendarAdapter, horizon: i64) -> Self {
        let mut raw = vec![];
        for d in -2..(horizon.div_euclid(DAY) + 3) {
            for &(s, e) in c.cal.shifts_on(d) {
                raw.push((d * DAY + s, d * DAY + e));
            }
        }
        raw.sort_unstable();
        let mut out = Self {
            s: vec![],
            e: vec![],
            cum: vec![0],
            horizon,
        };
        for (s, e) in raw {
            if let Some(last) = out.e.last_mut() {
                if s <= *last {
                    if e > *last {
                        let delta = e - *last;
                        *last = e;
                        if let Some(v) = out.cum.last_mut() {
                            *v += delta;
                        }
                    }
                    continue;
                }
            }
            out.s.push(s);
            out.e.push(e);
            out.cum.push(out.cum.last().copied().unwrap_or(0) + e - s);
        }
        out
    }
    fn work_at(&self, t: i64) -> i64 {
        let p = self.s.partition_point(|&s| s <= t);
        if p == 0 {
            0
        } else {
            let i = p - 1;
            self.cum[i] + t.min(self.e[i]) - self.s[i]
        }
    }
    fn start_at(&self, w: i64) -> i64 {
        let p = self.cum.partition_point(|&v| v <= w);
        if p == 0 {
            return self.s.last().copied().unwrap_or(self.horizon) + w
                - self.cum.last().copied().unwrap_or(0);
        }
        let i = p - 1;
        if i >= self.s.len() {
            self.horizon + DAY
        } else {
            self.s[i] + w - self.cum[i]
        }
    }
    fn finish_at(&self, w: i64) -> i64 {
        if w <= 0 {
            return self.s.first().copied().unwrap_or(self.horizon + DAY);
        }
        let p = self.cum.partition_point(|&v| v < w);
        let i = p.saturating_sub(1);
        if i >= self.s.len() {
            self.horizon + DAY
        } else {
            self.s[i] + w - self.cum[i]
        }
    }
    fn snap_start(&self, t: i64) -> i64 {
        self.start_at(self.work_at(t))
    }
    fn snap_finish(&self, t: i64) -> i64 {
        self.finish_at(self.work_at(t))
    }
    fn is_work(&self, t: i64) -> bool {
        let p = self.s.partition_point(|&s| s <= t);
        p > 0 && t < self.e[p - 1]
    }
    fn add(&self, t: i64, d: i64) -> i64 {
        if d == 0 {
            self.snap_start(t)
        } else if d < 0 {
            self.sub(t, -d)
        } else {
            self.finish_at(self.work_at(t) + d)
        }
    }
    fn sub(&self, t: i64, d: i64) -> i64 {
        if d == 0 {
            self.snap_finish(t)
        } else if d < 0 {
            self.add(t, -d)
        } else {
            self.start_at((self.work_at(t) - d).max(0))
        }
    }
    fn between(&self, a: i64, b: i64) -> i64 {
        self.work_at(b) - self.work_at(a)
    }
    fn reach(&self, t: i64, lag: i64) -> i64 {
        if lag == 0 {
            t
        } else {
            self.finish_at((self.work_at(t) + lag).max(0))
        }
    }
    fn intervals(&self, s: i64, f: i64) -> impl Iterator<Item = (i64, i64)> + '_ {
        let i = self.e.partition_point(|&e| e <= s);
        self.s[i..]
            .iter()
            .zip(&self.e[i..])
            .take_while(move |&(&a, _)| a < f)
            .map(move |(&a, &b)| (a.max(s), b.min(f)))
    }
}
#[derive(Clone, Debug)]
struct Profile {
    limits: BTreeMap<i64, f64>,
    levels: BTreeMap<i64, f64>,
}
impl Profile {
    fn capacity(&self, t: i64) -> f64 {
        if self.limits.is_empty() {
            f64::INFINITY
        } else {
            self.limits.range(..=t).next_back().map_or(0.0, |(_, v)| *v)
        }
    }
    fn used(&self, t: i64) -> f64 {
        self.levels.range(..=t).next_back().map_or(0.0, |(_, &v)| v)
    }
    fn fits(&self, t: i64, rate: f64) -> bool {
        self.used(t) + rate <= self.capacity(t) + 1e-9
    }
    fn next_event(&self, t: i64) -> Option<i64> {
        use std::ops::Bound::{Excluded, Unbounded};
        self.limits
            .range((Excluded(t), Unbounded))
            .next()
            .map(|(&t, _)| t)
            .into_iter()
            .chain(
                self.levels
                    .range((Excluded(t), Unbounded))
                    .next()
                    .map(|(&t, _)| t),
            )
            .min()
    }
    fn conflict(&self, c: &LevelCalendar, s: i64, f: i64, rate: f64) -> Option<i64> {
        for (mut t, end) in c.intervals(s.max(0), f) {
            while t < end {
                if !self.fits(t, rate) {
                    return Some(t);
                }
                let next = self.next_event(t).unwrap_or(end);
                t = next.min(end);
            }
        }
        None
    }
    fn prune_knot(&mut self, t: i64) {
        if let Some(&val) = self.levels.get(&t) {
            let prev = self.levels.range(..t).next_back().map_or(0.0, |(_, &v)| v);
            if (val - prev).abs() <= 1e-12 {
                self.levels.remove(&t);
            }
        }
    }
    fn book(&mut self, c: &LevelCalendar, s: i64, f: i64, rate: f64) {
        for (a, b) in c.intervals(s.max(0), f) {
            if a >= b {
                continue;
            }
            let va = self.used(a);
            let vb = self.used(b);
            self.levels.entry(a).or_insert(va);
            self.levels.entry(b).or_insert(vb);
            for (_, v) in self.levels.range_mut(a..b) {
                *v += rate;
            }
            self.prune_knot(a);
            self.prune_knot(b);
        }
    }
}
fn first_room(
    c: &LevelCalendar,
    profiles: &BTreeMap<i64, Profile>,
    need: &[(i64, f64)],
    start: i64,
    horizon: i64,
) -> Option<i64> {
    let mut t = start.max(0);
    while t < horizon {
        t = c.snap_start(t);
        if t >= horizon {
            return None;
        }
        let mut next = None;
        let mut conflict = false;
        for &(r, rate) in need {
            if let Some(p) = profiles.get(&r) {
                if !p.fits(t, rate) {
                    conflict = true;
                    let event = p.next_event(t)?;
                    next = Some(next.map_or(event, |x: i64| x.max(event)));
                }
            }
        }
        if !conflict {
            return Some(t);
        }
        t = next?;
    }
    None
}
fn milestone(a: &Activity) -> bool {
    matches!(a.task_type, TaskType::Mile | TaskType::FinMile)
}
fn mandatory_dates(a: &Activity, c: &LevelCalendar, dd: i64) -> Option<(i64, i64)> {
    // CON-9: started work is placed by a mandatory date only when the passes
    // pin it too.
    if !(a.status == StatusType::NotStarted
        || (a.status == StatusType::InProgress && a.mandatory_pinned()))
    {
        return None;
    }
    let dur = if milestone(a) { 0 } else { a.rem };
    for (ct, cd) in [(a.cstr, a.cstr_date), (a.cstr2, a.cstr_date2)] {
        let Some(cd) = cd else {
            continue;
        };
        if ct == CstrType::MandatoryStart && a.task_type != TaskType::FinMile {
            let s = c.snap_start(cd);
            return Some((s, if dur != 0 { c.add(s, dur) } else { s }));
        }
        if ct == CstrType::MandatoryFinish && a.task_type != TaskType::Mile {
            let back = if c.is_work(cd - 1) {
                cd
            } else {
                c.snap_finish(cd)
            };
            if a.task_type == TaskType::FinMile {
                let f = if c.is_work(cd - 1) {
                    cd
                } else {
                    c.snap_start(cd)
                };
                return Some((f, f));
            }
            let es = c
                .snap_start(dd)
                .max(if dur != 0 { c.sub(back, dur) } else { back });
            return Some((es, if dur != 0 { c.add(es, dur) } else { es }));
        }
    }
    None
}
fn level_lag_calendar<'a>(
    m: &Model,
    l: &Link,
    cals: &'a [LevelCalendar],
    c24: &'a LevelCalendar,
) -> &'a LevelCalendar {
    match m.options.lag_calendar {
        LagCalendar::PredCal => &cals[m.activities[l.pred].cal],
        LagCalendar::SuccCal => &cals[m.activities[l.succ].cal],
        LagCalendar::Hour24 => c24,
        LagCalendar::ProjCal => &cals[m.project_calendar],
    }
}
fn level_earliest(
    m: &Model,
    i: usize,
    cals: &[LevelCalendar],
    c24: &LevelCalendar,
    dates: &BTreeMap<usize, (i64, i64)>,
) -> Result<(i64, Option<i64>), String> {
    let a = &m.activities[i];
    let c = &cals[a.cal];
    let dd = m.project.data_date.ok_or("Leveling requires a data date")?;
    let dur = if milestone(a) || a.is_completed() {
        0
    } else {
        a.rem
    };
    let mut lo_s = dd;
    let mut lo_f = None;
    let mut floor = dd;
    for &r in &a.preds {
        let l = &m.links[r];
        let pa = &m.activities[l.pred];
        if Scheduler::skip_type(pa.task_type) {
            continue;
        }
        let lc = level_lag_calendar(m, l, cals, c24);
        let (mut ps, pf) = *dates
            .get(&l.pred)
            .ok_or("Unresolved leveling predecessor")?;
        if pa.status == StatusType::InProgress && !m.options.lag_from_early_start {
            ps = pa
                .act_start
                .ok_or("Started predecessor is missing its actual start")?;
        }
        let mut lag = l.lag;
        if pa.is_completed() {
            let mut fs = ps;
            if lag != 0 {
                let actual = if matches!(l.link_type, LinkType::SS | LinkType::SF) {
                    pa.act_start
                } else {
                    pa.act_finish
                };
                fs = fs.max(lc.reach(
                    actual.ok_or("Completed predecessor is missing actual dates")?,
                    lag,
                ));
            }
            if matches!(l.link_type, LinkType::FS | LinkType::SS) {
                lo_s = lo_s.max(fs);
            } else {
                lo_f = Some(lo_f.map_or(fs, |x: i64| x.max(fs)));
            }
            continue;
        }
        if l.link_type == LinkType::SS
            && lag > 0
            && pa.status == StatusType::InProgress
            && m.options.lag_from_early_start
        {
            if let Some(actual) = pa.act_start {
                lag = (lag - lc.between(actual, dd).max(0)).max(0);
            }
        }
        floor = floor.max(lc.reach(
            if matches!(l.link_type, LinkType::FS | LinkType::FF) {
                pf
            } else {
                ps
            },
            lag,
        ));
        match l.link_type {
            LinkType::FS => lo_s = lo_s.max(lc.reach(pf, lag)),
            LinkType::SS => lo_s = lo_s.max(lc.reach(ps, lag)),
            LinkType::FF | LinkType::SF => {
                let base = if l.link_type == LinkType::FF { pf } else { ps };
                let v = if lag != 0 { lc.add(base, lag) } else { base };
                lo_f = Some(lo_f.map_or(v, |x| x.max(v)));
            }
            LinkType::None => {}
        }
    }
    if a.status == StatusType::NotStarted {
        for (ct, cd) in [(a.cstr, a.cstr_date), (a.cstr2, a.cstr_date2)] {
            if let Some(d) = cd {
                if matches!(ct, CstrType::StartOnAfter | CstrType::StartOn)
                    && a.task_type != TaskType::FinMile
                {
                    lo_s = lo_s.max(d);
                } else if matches!(ct, CstrType::FinishOnAfter | CstrType::FinishOn) {
                    lo_f = Some(lo_f.map_or(d, |x| x.max(d)));
                    if a.task_type == TaskType::FinMile {
                        floor = floor.max(d);
                    }
                }
            }
        }
    }
    // The model's resume field is the remaining-work gate; imported suspended
    // activities and completed resume dates both wait for it (405,815,817).
    if a.status != StatusType::NotStarted {
        if let Some(resume) = a.resume {
            lo_s = lo_s.max(resume);
        }
    }
    if a.is_completed() {
        return Ok((lo_s.max(lo_f.unwrap_or(lo_s)), None));
    }
    if a.task_type == TaskType::FinMile {
        let mut f = lo_s.max(lo_f.unwrap_or(lo_s));
        if !c.is_work(f - 1) {
            let b = c.snap_finish(f);
            f = if b >= floor { b } else { c.snap_start(f) };
        }
        return Ok((f.max(dd), None));
    }
    let mut s = c.snap_start(lo_s);
    if let Some(f) = lo_f {
        s = s.max(if dur != 0 {
            c.sub(f, dur)
        } else {
            c.snap_start(f)
        });
    }
    Ok((s, lo_f))
}
fn compare_ready(m: &Model, inputs: &LevelingInputs, a: usize, b: usize) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let aa = &m.activities[a];
    let bb = &m.activities[b];
    let crew = |a: &Activity| {
        inputs
            .demands
            .get(&a.task_id)
            .is_some_and(|v| v.iter().any(|d| d.rate > 0.0 && d.remaining_units > 0.0))
            && !milestone(a)
            && a.rem != 0
    };
    let man = |a: &Activity| {
        (a.status == StatusType::NotStarted
            || (a.status == StatusType::InProgress && a.mandatory_pinned()))
            && [a.cstr, a.cstr2]
                .iter()
                .any(|c| matches!(c, CstrType::MandatoryStart | CstrType::MandatoryFinish))
    };
    let rank = |a: &Activity| (!a.is_completed(), !man(a), crew(a));
    let r = rank(aa).cmp(&rank(bb));
    if r != Ordering::Equal {
        return r;
    }
    for p in &inputs.priorities {
        let mut order = match p.field {
            PriorityField::ActivityPriority => inputs
                .activity_priority
                .get(&aa.task_id)
                .copied()
                .unwrap_or(2)
                .cmp(
                    &inputs
                        .activity_priority
                        .get(&bb.task_id)
                        .copied()
                        .unwrap_or(2),
                ),
            PriorityField::TotalFloat => aa
                .total_float
                .unwrap_or(i64::MAX)
                .cmp(&bb.total_float.unwrap_or(i64::MAX)),
            PriorityField::EarlyStart => aa
                .early_start
                .unwrap_or(i64::MAX)
                .cmp(&bb.early_start.unwrap_or(i64::MAX)),
            PriorityField::LateStart => aa
                .late_start
                .unwrap_or(i64::MAX)
                .cmp(&bb.late_start.unwrap_or(i64::MAX)),
            PriorityField::RemainingDuration => aa.rem.cmp(&bb.rem),
            PriorityField::Wbs => inputs
                .wbs_paths
                .get(&aa.task_id)
                .map(Vec::as_slice)
                .unwrap_or(&[])
                .cmp(
                    inputs
                        .wbs_paths
                        .get(&bb.task_id)
                        .map(Vec::as_slice)
                        .unwrap_or(&[]),
                ),
            PriorityField::ActivityCode(code) => {
                let av = inputs.activity_codes.get(&(aa.task_id, code));
                let bv = inputs.activity_codes.get(&(bb.task_id, code));
                let missing = av.is_none().cmp(&bv.is_none());
                if missing == Ordering::Equal {
                    av.cmp(&bv)
                } else if p.ascending {
                    missing
                } else {
                    missing.reverse()
                }
            }
        };
        if !p.ascending && !matches!(p.field, PriorityField::ActivityCode(_)) {
            order = order.reverse();
        }
        if order != Ordering::Equal {
            return order;
        }
    }
    // LVL-5: the last tie-break is the Activity ID; leveling within float
    // breaks it by the activity's internal id (creation order) instead
    // (two private daily exports leveled within float).
    if inputs.within_float {
        return aa.task_id.cmp(&bb.task_id);
    }
    aa.task_code
        .cmp(&bb.task_code)
        .then(aa.task_id.cmp(&bb.task_id))
}
/// LVL-3: the latest finish a delay may reach, as the activity's late finish
/// and the float to keep, when leveling is limited to float.
fn float_limit(inputs: &LevelingInputs, a: &Activity) -> Option<(i64, i64)> {
    if !inputs.within_float {
        return None;
    }
    let late = a.rem_late_finish.or(a.late_finish)?;
    Some((late, inputs.preserve_float.max(0)))
}
/// LVL-3: a finish keeps its float when the working time left between it and
/// the late finish is at least the float to keep.
fn float_allows(c: &LevelCalendar, limit: Option<(i64, i64)>, finish: i64) -> bool {
    limit.is_none_or(|(late, keep)| c.between(finish, late) >= keep)
}
#[allow(clippy::too_many_arguments)]
fn place_task(
    m: &Model,
    inputs: &LevelingInputs,
    i: usize,
    earliest: (i64, Option<i64>),
    cals: &[LevelCalendar],
    profiles: &mut BTreeMap<i64, Profile>,
    horizon: i64,
    held: &mut Vec<i64>,
) -> Result<(i64, i64), String> {
    let (s, lo_f) = earliest;
    let a = &m.activities[i];
    let c = &cals[a.cal];
    if a.is_completed() || a.task_type == TaskType::FinMile {
        return Ok((s, s));
    }
    let dur = if milestone(a) { 0 } else { a.rem };
    let demands = inputs
        .demands
        .get(&a.task_id)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let need: Vec<_> = demands
        .iter()
        .filter(|d| {
            profiles.contains_key(&d.resource_id) && d.rate > 0.0 && d.remaining_units > 0.0
        })
        .map(|d| (d.resource_id, d.rate))
        .collect();
    let dd = m.project.data_date.unwrap_or(0);
    if let Some((t, f)) = mandatory_dates(a, c, dd) {
        for &(r, rate) in &need {
            if let Some(p) = profiles.get_mut(&r) {
                p.book(c, t, f, rate);
            }
        }
        return Ok((t, f));
    }
    let limit = float_limit(inputs, a);
    if inputs.resource_dependent.contains(&a.task_id) {
        let (placed, blocked) =
            place_resource_dependent(demands, s, lo_f, cals, profiles, horizon, (c, limit))?;
        if blocked {
            held.push(a.task_id);
        }
        return Ok(placed);
    }
    if dur == 0 || need.is_empty() {
        let f = if dur != 0 { c.add(s, dur) } else { s };
        return Ok((
            s,
            if dur != 0 {
                f.max(lo_f.unwrap_or(f))
            } else {
                f
            },
        ));
    }
    let mut t = s;
    let mut f;
    let mut guard = 0;
    loop {
        guard += 1;
        f = c.add(t, dur);
        if f >= horizon {
            return Err(format!(
                "Resource leveling horizon exceeded at {}",
                a.task_code
            ));
        }
        let bad = need
            .iter()
            .filter_map(|&(r, rate)| profiles[&r].conflict(c, t, f, rate))
            .min();
        let Some(bad) = bad else {
            break;
        };
        let next = first_room(c, profiles, &need, bad + 1, horizon);
        if next.is_none() || guard > 100000 {
            t = s;
            f = c.add(t, dur);
            break;
        }
        t = next.unwrap_or(s);
        // LVL-3: no later slot can keep the float either, so the activity
        // stays where logic puts it and the over-allocation remains.
        let placed_end = c.add(t, dur);
        if !float_allows(c, limit, placed_end.max(lo_f.unwrap_or(placed_end))) {
            held.push(a.task_id);
            t = s;
            f = c.add(t, dur);
            break;
        }
    }
    for &(r, rate) in &need {
        if let Some(p) = profiles.get_mut(&r) {
            p.book(c, t, f, rate);
        }
    }
    Ok((t, f.max(lo_f.unwrap_or(f))))
}
fn place_resource_dependent(
    demands: &[ResourceDemand],
    s: i64,
    lo_f: Option<i64>,
    cals: &[LevelCalendar],
    profiles: &mut BTreeMap<i64, Profile>,
    horizon: i64,
    float: (&LevelCalendar, Option<(i64, i64)>),
) -> Result<((i64, i64), bool), String> {
    let rd: Vec<_> = demands
        .iter()
        .filter(|d| d.rate > 0.0 && d.remaining_units > 0.0)
        .collect();
    if rd.is_empty() {
        return Err("Resource Dependent activity needs an assignment with remaining units".into());
    }
    for d in &rd {
        if d.calendar.is_none_or(|i| i >= cals.len()) {
            return Err("Resource Dependent assignment is missing its resource calendar".into());
        }
    }
    let dates = |t| -> Vec<_> {
        rd.iter()
            .map(|d| {
                let c = &cals[d.calendar.unwrap_or(0)];
                let st = c.snap_start(t);
                (st, c.add(st, d.duration))
            })
            .collect()
    };
    let mut t = s;
    let mut result = dates(t);
    let mut blocked = false;
    let mut settled = false;
    for _ in 0..100000 {
        result = dates(t);
        if result.iter().any(|&(_, f)| f >= horizon) {
            return Err("Resource leveling horizon exceeded".into());
        }
        // LVL-3: a delay that no longer keeps the float is not made.
        if t > s {
            let end = result.iter().map(|&(_, f)| f).max().unwrap_or(t);
            if !float_allows(float.0, float.1, end.max(lo_f.unwrap_or(end))) {
                blocked = true;
                result = dates(s);
                break;
            }
        }
        let mut next = None;
        let mut impossible = false;
        for (d, &(x, f)) in rd.iter().zip(&result) {
            let c = &cals[d.calendar.unwrap_or(0)];
            if let Some(p) = profiles.get(&d.resource_id) {
                if let Some(bad) = p.conflict(c, x, f, d.rate) {
                    if let Some(free) =
                        first_room(c, profiles, &[(d.resource_id, d.rate)], bad + 1, horizon)
                    {
                        let tk = if c.is_work(free - 1) {
                            free
                        } else {
                            c.snap_finish(free)
                        };
                        next = Some(next.unwrap_or(t + 1).max(tk).max(t + 1));
                    } else {
                        impossible = true;
                        break;
                    }
                }
            }
        }
        if impossible {
            result = dates(s);
            settled = true;
            break;
        }
        if let Some(nt) = next {
            t = nt;
        } else {
            settled = true;
            break;
        }
    }
    if !settled && !blocked {
        result = dates(s);
    }
    for (d, &(x, f)) in rd.iter().zip(&result) {
        if let Some(p) = profiles.get_mut(&d.resource_id) {
            p.book(&cals[d.calendar.unwrap_or(0)], x, f, d.rate);
        }
    }
    let start = result
        .iter()
        .map(|&(s, _)| s)
        .min()
        .ok_or("Missing resource start")?;
    let end = result
        .iter()
        .map(|&(_, f)| f)
        .max()
        .ok_or("Missing resource finish")?;
    Ok(((start, end.max(lo_f.unwrap_or(end))), blocked))
}
/// Serial, nonsplitting resource leveling, preserving CPM early/late dates.
/// LVL-1: behavioral evidence is the saved leveling probes 801-817 and the
/// leveled corpus files.
/// Inputs and outputs are in-memory only; callers decide when to save.
pub fn level_resources(model: &mut Model, inputs: &LevelingInputs) -> Result<(), String> {
    level_resources_report(model, inputs).map(|_| ())
}
/// Level as `level_resources` does and return the IDs of the activities left
/// over-allocated because their float would not allow the delay (LVL-3), in
/// the order they were placed.
pub fn level_resources_report(
    model: &mut Model,
    inputs: &LevelingInputs,
) -> Result<Vec<i64>, String> {
    if model.activities.is_empty() {
        return Ok(vec![]);
    }
    if !inputs.unsupported.is_empty() {
        return Err(format!(
            "Resource leveling is not validated for: {}",
            inputs.unsupported.join("; ")
        ));
    }
    if !model.options.retained_logic || model.options.progress_override {
        return Err("Resource leveling is validated for Retained Logic only".into());
    }
    if model.options.open_end_uses_early
        && model.options.use_must_finish
        && model.project.plan_end.is_some()
    {
        return Err("Open-ended critical leveling with Must Finish By is not validated".into());
    }
    model.validate()?;
    let maxdate = model
        .activities
        .iter()
        .flat_map(|a| [a.early_start, a.early_finish, a.late_start, a.late_finish])
        .flatten()
        .max()
        .unwrap_or(0);
    let horizon = if inputs.horizon > 0 {
        inputs.horizon
    } else {
        maxdate
            .checked_add(4000 * DAY)
            .ok_or("Leveling horizon overflows")?
    };
    validate_minute(horizon, model.origin_serial)?;
    for values in inputs.limits.values() {
        if values.iter().any(|&(_, v)| !v.is_finite() || v < 0.0) {
            return Err("Resource capacity must be finite and nonnegative".into());
        }
    }
    for ds in inputs.demands.values() {
        if ds
            .iter()
            .any(|d| !d.rate.is_finite() || !d.remaining_units.is_finite() || d.duration < 0)
        {
            return Err("Resource demands must be finite with nonnegative durations".into());
        }
        for index in ds.iter().filter_map(|d| d.calendar) {
            let calendar = model
                .calendars
                .get(index)
                .ok_or("Resource calendar index is invalid")?;
            validate_calendar(&calendar.cal)?;
            if calendar.cal.origin_serial != model.origin_serial {
                return Err(format!(
                    "Resource calendar {} has a different date origin",
                    calendar.id
                ));
            }
            if !has_recurring_work(&calendar.cal) {
                return Err(format!(
                    "Resource calendar {} has no recurring work",
                    calendar.id
                ));
            }
        }
    }
    let mut used_cal = vec![false; model.calendars.len()];
    if model.project_calendar < used_cal.len() {
        used_cal[model.project_calendar] = true;
    }
    for a in &model.activities {
        if a.cal < used_cal.len() {
            used_cal[a.cal] = true;
        }
    }
    for ds in inputs.demands.values() {
        for c in ds.iter().filter_map(|d| d.calendar) {
            if c < used_cal.len() {
                used_cal[c] = true;
            }
        }
    }
    let cals: Vec<_> = model
        .calendars
        .iter()
        .enumerate()
        .map(|(i, c)| {
            if used_cal[i] {
                LevelCalendar::new(c, horizon)
            } else {
                LevelCalendar::empty(horizon)
            }
        })
        .collect();
    let c24 = LevelCalendar::new(
        &CalendarAdapter::new(WorkCalendar::continuous(model.origin_serial)),
        horizon,
    );
    let mut profiles: BTreeMap<_, _> = inputs
        .limits
        .iter()
        .filter(|(r, _)| inputs.selected.as_ref().is_none_or(|s| s.contains(r)))
        .map(|(&r, rows)| {
            let mut rows = rows.clone();
            rows.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
            (
                r,
                Profile {
                    limits: rows.into_iter().collect(),
                    levels: BTreeMap::new(),
                },
            )
        })
        .collect();
    let ids: BTreeSet<_> = model
        .activities
        .iter()
        .enumerate()
        .filter_map(|(i, a)| (!Scheduler::skip_type(a.task_type)).then_some(i))
        .collect();
    let mut degrees: BTreeMap<_, _> = ids
        .iter()
        .map(|&i| {
            (
                i,
                model.activities[i]
                    .preds
                    .iter()
                    .filter(|&&r| ids.contains(&model.links[r].pred))
                    .count(),
            )
        })
        .collect();
    struct ReadyEntry<'a> {
        model: &'a Model,
        inputs: &'a LevelingInputs,
        idx: usize,
    }
    impl PartialEq for ReadyEntry<'_> {
        fn eq(&self, other: &Self) -> bool {
            self.idx == other.idx
        }
    }
    impl Eq for ReadyEntry<'_> {}
    impl Ord for ReadyEntry<'_> {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            // BinaryHeap pops the max element, so reverse compare_ready to pop the best.
            compare_ready(self.model, self.inputs, other.idx, self.idx)
        }
    }
    impl PartialOrd for ReadyEntry<'_> {
        fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(other))
        }
    }
    let mut ready: BinaryHeap<ReadyEntry<'_>> = degrees
        .iter()
        .filter_map(|(&i, &n)| {
            (n == 0).then_some(ReadyEntry {
                model,
                inputs,
                idx: i,
            })
        })
        .collect();
    let mut dates = BTreeMap::new();
    let mut held = vec![];
    while let Some(ReadyEntry { idx: i, .. }) = ready.pop() {
        let (s, lo_f) = level_earliest(model, i, &cals, &c24, &dates)?;
        let pair = place_task(
            model,
            inputs,
            i,
            (s, lo_f),
            &cals,
            &mut profiles,
            horizon,
            &mut held,
        )?;
        dates.insert(i, pair);
        for &r in &model.activities[i].succs {
            let s = model.links[r].succ;
            if let Some(n) = degrees.get_mut(&s) {
                *n -= 1;
                if *n == 0 {
                    ready.push(ReadyEntry {
                        model,
                        inputs,
                        idx: s,
                    });
                }
            }
        }
    }
    if dates.len() != ids.len() {
        return Err("Resource leveling network contains a circular relationship".into());
    }
    for (&i, &(s, f)) in &dates {
        let a = &mut model.activities[i];
        // Keep completed virtual remaining dates for relationship getters.
        // Final output cleanup clears restart/reend, while rem_* survives.
        a.restart = Some(s);
        a.reend = Some(f);
        a.rem_restart = Some(s);
        a.rem_reend = Some(f);
        if a.status == StatusType::NotStarted {
            a.target_start = Some(s);
            a.target_finish = Some(f);
        }
    }
    // LOEs book no resources. The proven SS/FF shape spans leveled partners.
    let dd = model.project.data_date.unwrap_or(0);
    let mut general = vec![];
    for i in 0..model.activities.len() {
        let a = &model.activities[i];
        if a.is_completed() {
            continue;
        }
        if a.task_type == TaskType::Wbs {
            let pair = (a.early_start, a.early_finish);
            let a = &mut model.activities[i];
            a.restart = pair.0;
            a.reend = pair.1;
            a.rem_restart = pair.0;
            a.rem_reend = pair.1;
            continue;
        }
        if a.task_type != TaskType::Loe {
            continue;
        }
        let mut starts = vec![];
        let mut finishes = vec![];
        let mut other = 0;
        for &r in a.preds.iter().chain(&a.succs) {
            let l = &model.links[r];
            let neighbor = if l.succ == i { l.pred } else { l.succ };
            if neighbor == i
                || l.lag != 0
                || Scheduler::skip_type(model.activities[neighbor].task_type)
            {
                other += 1;
            } else if l.succ == i && l.link_type == LinkType::SS {
                starts.push(neighbor);
            } else if l.link_type == LinkType::FF {
                finishes.push(neighbor);
            } else {
                other += 1;
            }
        }
        if other != 0 || starts.len() != 1 || finishes.len() != 1 {
            general.push(i);
            continue;
        }
        let p = &model.activities[starts[0]];
        if !((p.status == StatusType::NotStarted && a.status == StatusType::NotStarted)
            || (p.is_completed() && a.status == StatusType::InProgress))
        {
            continue;
        }
        if let (Some(&(s, _)), Some(&(_, f))) = (dates.get(&starts[0]), dates.get(&finishes[0])) {
            let s = cals[a.cal].snap_start(dd.max(s));
            let f = f.max(s);
            let a = &mut model.activities[i];
            a.restart = Some(s);
            a.reend = Some(f);
            a.rem_restart = Some(s);
            a.rem_reend = Some(f);
            if a.status == StatusType::NotStarted {
                a.target_start = Some(s);
                a.target_finish = Some(f);
            }
        }
    }
    // FEAT-1 after leveling: any other open LOE spans its partners' leveled
    // dates by the same endpoint rule as its early dates (private daily
    // exports: an LOE tied backwards, and one with a finish lag).
    let spans: Vec<(usize, i64, i64)> = {
        let sched = Scheduler::new(model);
        let leveled = |a: &Activity, oi: usize| {
            dates
                .get(&oi)
                .map_or((a.early_start, a.early_finish), |&(s, f)| {
                    (Some(s), Some(f))
                })
        };
        general
            .iter()
            .filter_map(|&i| {
                let [esv, efv, _, _] = loe_partner_dates(&sched, i, &leveled);
                if esv.is_empty() && efv.is_empty() {
                    return None;
                }
                let c = sched.cal(i);
                let es = c.snap_start(dd.max(esv.iter().copied().min().unwrap_or(dd)));
                let ef = es.max(efv.iter().copied().max().unwrap_or(es));
                Some((i, es, ef))
            })
            .collect()
    };
    for (i, s, f) in spans {
        let a = &mut model.activities[i];
        a.restart = Some(s);
        a.reend = Some(f);
        a.rem_restart = Some(s);
        a.rem_reend = Some(f);
        if a.status == StatusType::NotStarted {
            a.target_start = Some(s);
            a.target_finish = Some(f);
        }
    }
    Ok(held)
}

/// The leveled-float calendars last built on this thread, with what they were
/// built from (origin, horizon, calendars).
type FloatCalendars = std::rc::Rc<(Vec<LevelCalendar>, LevelCalendar)>;
type FloatCalendarsEntry = (i64, i64, Vec<CalendarAdapter>, FloatCalendars);
thread_local! {
    static FLOAT_CALENDARS: std::cell::RefCell<Option<FloatCalendarsEntry>> =
        const { std::cell::RefCell::new(None) };
}
/// LVL-2: the interval calendars the leveled relationship float reads. They
/// depend only on the origin, the calendars and the horizon, so a call with
/// the same three (the Gantt asks once per relationship) reuses the last
/// build instead of rebuilding every calendar out to the horizon.
fn float_calendars(model: &Model) -> FloatCalendars {
    let horizon = float_horizon(model);
    FLOAT_CALENDARS.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some((origin, h, cals, built)) = cache.as_ref() {
            if *origin == model.origin_serial && *h == horizon && *cals == model.calendars {
                return built.clone();
            }
        }
        let built = std::rc::Rc::new(build_float_calendars(model, horizon));
        *cache = Some((
            model.origin_serial,
            horizon,
            model.calendars.clone(),
            built.clone(),
        ));
        built
    })
}
fn float_horizon(model: &Model) -> i64 {
    model
        .activities
        .iter()
        .flat_map(|a| {
            [
                a.early_start,
                a.early_finish,
                a.rem_restart,
                a.rem_reend,
                a.act_start,
                a.act_finish,
            ]
        })
        .flatten()
        .max()
        .unwrap_or(0)
        .max(model.project.data_date.unwrap_or(0))
        .saturating_add(100 * DAY)
}
fn build_float_calendars(model: &Model, horizon: i64) -> (Vec<LevelCalendar>, LevelCalendar) {
    let cals = model
        .calendars
        .iter()
        .map(|c| LevelCalendar::new(c, horizon))
        .collect();
    let c24 = LevelCalendar::new(
        &CalendarAdapter::new(WorkCalendar::continuous(model.origin_serial)),
        horizon,
    );
    (cals, c24)
}
fn level_relationship_gap(
    model: &Model,
    ri: usize,
    cals: &[LevelCalendar],
    c24: &LevelCalendar,
) -> Option<i64> {
    let l = model.links.get(ri)?;
    let p = model.activities.get(l.pred)?;
    let s = model.activities.get(l.succ)?;
    if p.task_type == TaskType::Wbs || s.task_type == TaskType::Wbs {
        return None;
    }
    let dd = model.project.data_date?;
    let lc = level_lag_calendar(model, l, cals, c24);
    let start_link = matches!(l.link_type, LinkType::SS | LinkType::SF);
    let source = if start_link {
        p.rem_restart.or(p.early_start)
    } else {
        p.rem_reend.or(p.early_finish)
    }?;
    let reach = if start_link
        && p.status == StatusType::InProgress
        && p.act_start.is_some()
        && !model.options.lag_from_early_start
    {
        lc.reach(p.act_start?, l.lag).max(dd)
    } else {
        lc.reach(source, l.lag)
    };
    let target = if matches!(l.link_type, LinkType::FS | LinkType::SS) {
        s.rem_restart.or(s.early_start)
    } else {
        s.rem_reend.or(s.early_finish)
    }?;
    Some(cals[s.cal].between(reach, target))
}
fn level_driving_allowed(model: &Model, ri: usize, cals: &[LevelCalendar]) -> bool {
    let l = &model.links[ri];
    let p = &model.activities[l.pred];
    let s = &model.activities[l.succ];
    if mandatory_dates(s, &cals[s.cal], model.project.data_date.unwrap_or(0)).is_some() {
        return false;
    }
    !(matches!(l.link_type, LinkType::SS | LinkType::SF)
        && p.status == StatusType::InProgress
        && p.act_start.is_some()
        && (!model.options.lag_from_early_start
            || model.options.progress_override
            || model.options.neither_oos()))
}
/// LVL-2: leveled tie free float (saved probes 809-813): full lag at leveled
/// dates.
/// None means the original completed/WBS handling remains applicable.
pub fn relationship_free_float_leveled(model: &Model, ri: usize) -> Option<i64> {
    let l = model.links.get(ri)?;
    if model.activities.get(l.pred)?.is_completed() || model.activities.get(l.succ)?.is_completed()
    {
        return None;
    }
    let built = float_calendars(model);
    let (cals, c24) = (built.0.as_slice(), &built.1);
    level_relationship_gap(model, ri, cals, c24).map(|v| v.max(0))
}
pub fn relationship_driving_leveled(model: &Model, ri: usize) -> Option<bool> {
    let l = model.links.get(ri)?;
    if model.activities.get(l.pred)?.is_completed() || model.activities.get(l.succ)?.is_completed()
    {
        return None;
    }
    let built = float_calendars(model);
    let (cals, c24) = (built.0.as_slice(), &built.1);
    let gap = level_relationship_gap(model, ri, cals, c24)?;
    Some(gap <= 0 && level_driving_allowed(model, ri, cals))
}
pub(crate) fn relationship_driving_all_leveled(sched: &Scheduler<'_>) -> Vec<Option<bool>> {
    let model = &*sched.model;
    let built = float_calendars(model);
    let (cals, c24) = (built.0.as_slice(), &built.1);
    (0..model.links.len())
        .map(|r| {
            let l = &model.links[r];
            let p = &model.activities[l.pred];
            let s = &model.activities[l.succ];
            if p.task_type == TaskType::Wbs || s.task_type == TaskType::Wbs {
                return Some(false);
            }
            if !p.is_completed() && !s.is_completed() {
                if let Some(gap) = level_relationship_gap(model, r, cals, c24) {
                    return Some(gap <= 0 && level_driving_allowed(model, r, cals));
                }
            }
            let (_, drive) = sched.relationship_float_and_flag_base(r);
            if s.uses_mandatory_constraint() {
                return Some(false);
            }
            Some(drive)
        })
        .collect()
}
/// The leveled longest path ends at the latest EARLY finish, and traces the
/// leveled driving ties, including zero-gap chains through completed work.
pub fn calculate_driving_path_after_level(model: &mut Model) {
    let built = float_calendars(model);
    let (cals, c24) = (built.0.as_slice(), &built.1);
    let last = model
        .activities
        .iter()
        .filter(|a| !a.is_completed() && !Scheduler::skip_type(a.task_type))
        .filter_map(|a| a.early_finish)
        .max()
        // LVL-6: when leveling moved the project finish (within float), the
        // path ends where an early finish meets it, if anywhere.
        .max(model.project.scd_end);
    let mut on: BTreeSet<_> = model
        .activities
        .iter()
        .enumerate()
        .filter_map(|(i, a)| {
            (!a.is_completed()
                && !Scheduler::skip_type(a.task_type)
                && a.early_finish.is_some()
                && a.early_finish == last)
                .then_some(i)
        })
        .collect();
    let mut stack: Vec<_> = on.iter().copied().collect();
    while let Some(i) = stack.pop() {
        for &r in &model.activities[i].preds {
            let l = &model.links[r];
            let p = &model.activities[l.pred];
            let s = &model.activities[l.succ];
            let Some(gap) = level_relationship_gap(model, r, cals, c24) else {
                continue;
            };
            let drives = if p.is_completed() || s.is_completed() {
                gap == 0
            } else {
                gap <= 0
            };
            if drives && level_driving_allowed(model, r, cals) && on.insert(l.pred) {
                stack.push(l.pred);
            }
        }
    }
    for (i, a) in model.activities.iter_mut().enumerate() {
        a.driving_path = on.contains(&i);
    }
}
