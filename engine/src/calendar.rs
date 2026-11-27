//! Work calendars: minute-exact arithmetic over weekday shift patterns and
//! dated exceptions. Rules CAL-1 to CAL-10 in `engine/RULES.md`, tests in
//! `engine/tests/blackbox_rules.rs`.
use std::{borrow::Cow, cell::Cell, collections::BTreeMap};
pub const DAY: i64 = 1440;
thread_local! {
    static BEFORE_ORIGIN: Cell<bool> = const { Cell::new(false) };
}
/// CAL-10, BWD-5: work positions are counted from the origin day and are not
/// meaningful before it. Every position function marks, on this thread, a
/// request that reaches before the origin (a moment or a work position
/// below zero, or a step back clamped at zero), so a scheduling run can tell
/// that its dates may be clamped and run again from an earlier origin.
fn mark_before_origin() {
    BEFORE_ORIGIN.with(|mark| mark.set(true));
}
/// Clear this thread's before-origin mark.
pub fn clear_before_origin() {
    BEFORE_ORIGIN.with(|mark| mark.set(false));
}
/// This thread's before-origin mark, cleared by the call: true when a
/// position before the origin was asked for since the last clear.
pub fn take_before_origin() -> bool {
    BEFORE_ORIGIN.with(|mark| mark.replace(false))
}
/// Mark a position before the origin when `w` is negative; returns `w`.
pub(crate) fn note_position(w: i64) -> i64 {
    if w < 0 {
        mark_before_origin();
    }
    w
}
/// CAL-1: a day's working view from the stored shifts of the day before
/// (`prev`) and of the day itself (`own`); see [`WorkCalendar::work_shifts`].
fn day_view<'s>(prev: &[(i64, i64)], own: &'s [(i64, i64)]) -> Cow<'s, [(i64, i64)]> {
    let spills = prev.iter().any(|&(_, e)| e > DAY);
    let clean = own.iter().all(|&(_, e)| e <= DAY) && own.windows(2).all(|w| w[0].1 <= w[1].0);
    if clean && !spills {
        return Cow::Borrowed(own);
    }
    let mut raw: Vec<(i64, i64)> = prev
        .iter()
        .filter(|&&(_, e)| e > DAY)
        .map(|&(s, e)| ((s - DAY).max(0), e - DAY))
        .chain(own.iter().map(|&(s, e)| (s, e.min(DAY))))
        .filter(|&(s, e)| s < e)
        .collect();
    raw.sort_unstable();
    let mut out: Vec<(i64, i64)> = Vec::with_capacity(raw.len());
    for (s, e) in raw {
        match out.last_mut() {
            Some(last) if s < last.1 => last.1 = last.1.max(e),
            _ => out.push((s, e)),
        }
    }
    Cow::Owned(out)
}
/// Most shift periods one day may carry (informational; the arithmetic does
/// not enforce it).
pub const MAX_SHIFTS: usize = 48;
pub type Shifts = Vec<(i64, i64)>;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkCalendar {
    pub name: String,
    pub continuous: bool,
    pub origin_serial: i64,
    pub template: [Shifts; 7],
    pub exc: BTreeMap<i64, Shifts>,
}
impl WorkCalendar {
    pub fn new(
        name: String,
        continuous: bool,
        origin_serial: i64,
        mut template: [Shifts; 7],
        mut exc: BTreeMap<i64, Shifts>,
    ) -> Self {
        for shifts in template.iter_mut().chain(exc.values_mut()) {
            for (s, e) in shifts {
                if *e <= *s {
                    *e += DAY;
                }
            }
        }
        Self {
            name,
            continuous,
            origin_serial,
            template,
            exc,
        }
    }
    pub fn continuous(origin: i64) -> Self {
        Self::new(
            "24x7".into(),
            true,
            origin,
            std::array::from_fn(|_| vec![(0, DAY)]),
            BTreeMap::new(),
        )
    }
    /// The same calendar counted from another origin: day `d` of the result is
    /// day `d - (self.origin_serial - origin)` of this one, so every absolute
    /// moment keeps its work time. Exceptions are keyed by absolute day.
    pub fn rebased(&self, origin: i64) -> Self {
        let delta = self.origin_serial - origin;
        let mut out = self.clone();
        out.origin_serial = origin;
        out.template = std::array::from_fn(|i| {
            self.template[(i as i64 - delta).rem_euclid(7) as usize].clone()
        });
        out
    }
    /// The shifts stored for `day`: its dated exception, else its weekday's
    /// pattern, exactly as stored (in stored order, possibly overlapping, an
    /// overnight shift ending after midnight). The arithmetic reads
    /// [`Self::work_shifts`] instead.
    pub fn shifts_on(&self, day: i64) -> &[(i64, i64)] {
        if self.continuous {
            return &[(0, DAY)];
        }
        self.exc
            .get(&(self.origin_serial + day))
            .unwrap_or(&self.template[day.rem_euclid(7) as usize])
    }
    /// CAL-1: the working time of `day` as the arithmetic reads it: the
    /// day's own shifts up to midnight, plus the part after midnight of an
    /// overnight shift that started the day before, sorted by start with
    /// overlapping shifts merged (shifts that only touch stay apart). A shift
    /// belongs to the day it starts on, so a dated exception replaces the
    /// shifts starting that day and the next day still gets the rest of the
    /// previous night's shift. The stored shifts are not changed.
    pub fn work_shifts(&self, day: i64) -> Cow<'_, [(i64, i64)]> {
        if self.continuous {
            return Cow::Borrowed(&[(0, DAY)]);
        }
        day_view(self.shifts_on(day - 1), self.shifts_on(day))
    }
    /// The work minutes of a day of weekday `i` (0 to 6) when neither it nor
    /// the day before has a dated exception, for each weekday.
    fn pattern_totals(&self) -> [i64; 7] {
        std::array::from_fn(|i| {
            day_view(&self.template[(i + 6) % 7], &self.template[i])
                .iter()
                .map(|(s, e)| e - s)
                .sum()
        })
    }
    /// The sum of [`Self::total_hours_on_date`] over days `a..b` (`0` when
    /// `b <= a`), without walking the days: whole weeks of the weekday pattern,
    /// then the days a dated exception touches (its own day, and the next day,
    /// which may carry the end of the exception's overnight shift) corrected
    /// to their actual totals. Same integers as the day-by-day sum.
    fn work_in_days(&self, a: i64, b: i64, pattern: &[i64; 7]) -> i64 {
        if b <= a {
            return 0;
        }
        if self.continuous {
            return (b - a) * DAY;
        }
        let n = b - a;
        let week: i64 = pattern.iter().sum();
        let first = a.rem_euclid(7);
        let mut total = n.div_euclid(7) * week
            + (0..n.rem_euclid(7))
                .map(|k| pattern[((first + k) % 7) as usize])
                .sum::<i64>();
        let mut last = i64::MIN;
        let keys = (self.origin_serial + a - 1)..=(self.origin_serial + b - 1);
        for &key in self.exc.range(keys).map(|(k, _)| k) {
            let x = key - self.origin_serial;
            for d in [x, x + 1] {
                if d > last && (a..b).contains(&d) {
                    total += self.total_hours_on_date(d) - pattern[d.rem_euclid(7) as usize];
                    last = d;
                }
            }
        }
        total
    }
    /// The day a work position falls on and the position within that day:
    /// the first day `d >= 0` whose end of day reaches past `w` (`>= w` when
    /// `at_close`, so a position on a day's last close stays on that day),
    /// with `w` less the work of days `0..d`. The answer of walking the days
    /// from day 0, found by whole-week steps and then single days.
    fn locate(&self, w: i64, at_close: bool) -> (i64, i64) {
        let past = |c: i64| if at_close { c >= w } else { c > w };
        let pattern = self.pattern_totals();
        let week: i64 = pattern.iter().sum();
        if week <= 0 && !self.continuous {
            // No recurring work: only exceptions can carry the position, so
            // walk the days as before.
            let (mut day, mut w) = (0, w);
            loop {
                let tot = self.total_hours_on_date(day);
                if (at_close && w <= tot) || (!at_close && w < tot) {
                    return (day, w);
                }
                w -= tot;
                day += 1;
            }
        }
        let week = if self.continuous { 7 * DAY } else { week };
        // m is a day count and c the work of days 0..m.
        let mut m = ((w.max(0) / week) * 7).max(1);
        let mut c = self.work_in_days(0, m, &pattern);
        while !past(c) && (w - c) / week >= 1 {
            let step = 7 * ((w - c) / week);
            c += self.work_in_days(m, m + step, &pattern);
            m += step;
        }
        while !past(c) {
            c += self.total_hours_on_date(m);
            m += 1;
        }
        while m > 1 && past(c - self.total_hours_on_date(m - 1)) {
            m -= 1;
            c -= self.total_hours_on_date(m);
        }
        let day = m - 1;
        (day, w - (c - self.total_hours_on_date(day)))
    }
    /// CAL-1: the day's total work minutes. A dated exception replaces the
    /// weekday pattern for that day.
    pub fn total_hours_on_date(&self, day: i64) -> i64 {
        self.work_shifts(day).iter().map(|(s, e)| e - s).sum()
    }
    pub fn in_work(&self, t: i64, day: i64) -> bool {
        let tod = t - day * DAY;
        self.work_shifts(day)
            .iter()
            .any(|&(s, e)| s <= tod && tod < e)
    }
    pub fn work_end(&self, t: i64, day: i64) -> Option<i64> {
        let tod = t - day * DAY;
        self.work_shifts(day)
            .iter()
            .find(|&&(s, e)| s <= tod && tod < e)
            .map(|&(_, e)| day * DAY + e)
    }
    /// CAL-2: this minute when it is working time, else the next shift start
    /// (today first, then later days), never past `bound`. A round-the-clock
    /// calendar answers `min(t, bound)`.
    pub fn next_work_time(&self, t: i64, bound: i64) -> i64 {
        if self.continuous {
            return t.min(bound);
        }
        let day = t.div_euclid(DAY);
        if self.in_work(t, day) {
            return t;
        }
        for &(s, _) in self.work_shifts(day).iter() {
            if s >= t - day * DAY {
                return (day * DAY + s).min(bound);
            }
        }
        let mut d = day + 1;
        while d * DAY < bound {
            if let Some(&(s, _)) = self.work_shifts(d).first() {
                return (d * DAY + s).min(bound);
            }
            d += 1;
        }
        bound
    }
    /// CAL-3: the close of the shift containing `t`, capped by `bound`; `t`
    /// itself outside any shift.
    pub fn next_non_work_time(&self, t: i64, bound: i64) -> i64 {
        if self.continuous {
            return t.min(bound);
        }
        self.work_end(t, t.div_euclid(DAY))
            .map_or(t, |w| w.min(bound))
    }
    /// CAL-4: the latest shift close at or before `t` (not the current working
    /// minute), looking back over earlier days; `bound` is a floor.
    pub fn prev_work_time(&self, t: i64, bound: i64) -> i64 {
        if self.continuous {
            return t.max(bound);
        }
        let day = t.div_euclid(DAY);
        let tod = t - day * DAY;
        if let Some(e) = self
            .work_shifts(day)
            .iter()
            .filter_map(|&(_, e)| (e <= tod).then_some(e))
            .max()
        {
            return (day * DAY + e).max(bound);
        }
        let mut d = day - 1;
        while d * DAY + DAY > bound {
            if let Some(&(_, e)) = self.work_shifts(d).last() {
                return (d * DAY + e).max(bound);
            }
            d -= 1;
        }
        bound
    }
    /// CAL-5: the start of the shift containing `t` (a shift's opening and
    /// closing minute both count as inside it); `t` itself outside any shift.
    /// A round-the-clock calendar answers `min(t, bound)`.
    pub fn prev_non_work_time(&self, t: i64, bound: i64) -> i64 {
        if self.continuous {
            return t.min(bound);
        }
        let day = t.div_euclid(DAY);
        let tod = t - day * DAY;
        for &(s, e) in self.work_shifts(day).iter() {
            if (s < tod && tod <= e) || e == tod || (s <= tod && tod < e) {
                return (day * DAY + s).max(bound);
            }
        }
        t
    }
    /// CAL-6: working minutes between two moments, negative when reversed.
    pub fn duration(&self, mut a: i64, mut b: i64) -> i64 {
        let mut sign = 1;
        if b < a {
            std::mem::swap(&mut a, &mut b);
            sign = -1;
        }
        if self.continuous {
            return sign * (b - a);
        }
        let mut total = 0;
        let mut t = self.next_work_time(a, b);
        if t >= b {
            return 0;
        }
        let seg_end = ((t.div_euclid(DAY) + 1) * DAY).min(b);
        while t < seg_end {
            let nw = self.next_non_work_time(t, seg_end);
            total += nw - t;
            t = self.next_work_time(nw, seg_end);
        }
        let d = (t + DAY - 1).div_euclid(DAY);
        let end_day = b.div_euclid(DAY);
        if d < end_day {
            total += self.work_in_days(d, end_day, &self.pattern_totals());
        }
        t = t.max(end_day * DAY);
        if t < b {
            let mut t2 = self.next_work_time(t, b);
            while t2 < b {
                let nw = self.next_non_work_time(t2, b);
                total += nw - t2;
                t2 = self.next_work_time(nw, b);
            }
        }
        sign * total
    }
    /// CAL-7: the moment after `minutes` of work from `start`. Work that
    /// exactly fills a shift finishes at its close, not at the next opening.
    pub fn get_finish_date(&self, start: i64, minutes: i64) -> i64 {
        if self.continuous {
            return start + minutes;
        }
        let mut day = start.div_euclid(DAY);
        let mut rem = minutes;
        let mut cur = start;
        let mut dvar = self.duration(start, (day + 1) * DAY);
        while rem > dvar {
            rem -= dvar;
            day += 1;
            dvar = self.total_hours_on_date(day);
            cur = day * DAY;
        }
        let mut t = self.next_work_time(cur, (day + 1) * DAY);
        loop {
            let nw = self.next_non_work_time(t, (day + 1) * DAY);
            if nw - t >= rem {
                return t + rem;
            }
            rem -= nw - t;
            t = self.next_work_time(nw, (day + 1) * DAY);
        }
    }
    /// CAL-10: cumulative working minutes from day 0 up to `t` (half-open).
    /// Whole days before day 0 add nothing.
    pub fn work_at(&self, t: i64) -> i64 {
        if t < 0 {
            mark_before_origin();
        }
        let day = t.div_euclid(DAY);
        let tod = t - day * DAY;
        self.work_in_days(0, day, &self.pattern_totals())
            + self
                .work_shifts(day)
                .iter()
                .map(|&(s, e)| (e.min(tod) - s).max(0))
                .sum::<i64>()
    }
    pub fn start_at(&self, w: i64) -> i64 {
        note_position(w);
        let (day, mut w) = self.locate(w, false);
        for &(s, e) in self.work_shifts(day).iter() {
            if w < e - s {
                return day * DAY + s + w;
            }
            w -= e - s;
        }
        // CAL-10: a work position that lands exactly on a shift's close is the
        // next shift's opening.
        let shifts = self.work_shifts(day);
        let last = shifts.last().map_or(0, |&(_, e)| e);
        for &(s, _) in shifts.iter() {
            if s >= last {
                return day * DAY + s;
            }
        }
        (day + 1) * DAY
    }
    pub fn finish_at(&self, w: i64) -> i64 {
        note_position(w);
        let (day, mut w) = self.locate(w, true);
        for &(s, e) in self.work_shifts(day).iter() {
            if w <= e - s {
                return day * DAY + s + w;
            }
            w -= e - s;
        }
        (day + 1) * DAY
    }
    /// CAL-8: the moment `minutes` of work before `finish`, as a start inside
    /// a shift.
    pub fn get_start_date(&self, finish: i64, minutes: i64) -> i64 {
        if self.continuous {
            finish - minutes
        } else {
            self.start_at(note_position(self.work_at(finish) - minutes).max(0))
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CalendarAdapter {
    pub cal: WorkCalendar,
    pub id: String,
}
impl CalendarAdapter {
    pub fn new(cal: WorkCalendar) -> Self {
        Self {
            id: cal.name.clone(),
            cal,
        }
    }
    /// CAL-9: move `t` forward by `m` working minutes. Zero snaps to a start
    /// moment; a negative `m` moves back.
    pub fn add(&self, t: i64, m: i64) -> i64 {
        if m == 0 {
            self.snap_start(t)
        } else if m < 0 {
            self.sub(t, -m)
        } else {
            self.finish_at(self.work_at(t) + m)
        }
    }
    /// CAL-9: move `t` back by `m` working minutes. Zero snaps to a finish
    /// moment; a negative `m` moves forward.
    pub fn sub(&self, t: i64, m: i64) -> i64 {
        if m == 0 {
            self.snap_finish(t)
        } else if m < 0 {
            self.add(t, -m)
        } else {
            self.start_at(note_position(self.work_at(t) - m).max(0))
        }
    }
    /// CAL-9: signed working minutes from `a` to `b`.
    pub fn between(&self, a: i64, b: i64) -> i64 {
        self.work_at(b) - self.work_at(a)
    }
    pub fn work_at(&self, t: i64) -> i64 {
        self.cal.work_at(t)
    }
    pub fn start_at(&self, w: i64) -> i64 {
        self.cal.start_at(w)
    }
    pub fn finish_at(&self, w: i64) -> i64 {
        self.cal.finish_at(w)
    }
    pub fn add_start(&self, t: i64, m: i64) -> i64 {
        self.start_at(self.work_at(t) + m)
    }
    pub fn snap_start(&self, t: i64) -> i64 {
        self.start_at(self.work_at(t))
    }
    pub fn snap_finish(&self, t: i64) -> i64 {
        self.finish_at(self.work_at(t))
    }
    pub fn is_work(&self, t: i64) -> bool {
        self.cal.in_work(t, t.div_euclid(DAY))
    }
}

/// Precomputed exception prefix-sums for O(log E) calendar position arithmetic
/// over an immutable [`WorkCalendar`]. Produces identical results and
/// before-origin marks as [`CalendarAdapter`].
#[derive(Clone, Debug)]
pub(crate) struct CalIndex {
    continuous: bool,
    pattern: [i64; 7],
    week: i64,
    exc_days: Vec<i64>,
    exc_prefix: Vec<i64>,
    exc_totals: Vec<i64>,
}

impl CalIndex {
    pub(crate) fn new(cal: &WorkCalendar) -> Self {
        if cal.continuous {
            return Self {
                continuous: true,
                pattern: [DAY; 7],
                week: 7 * DAY,
                exc_days: Vec::new(),
                exc_prefix: vec![0],
                exc_totals: Vec::new(),
            };
        }
        let pattern = cal.pattern_totals();
        let week = pattern.iter().sum();
        let mut exc_days = Vec::with_capacity(cal.exc.len() * 2);
        let mut exc_totals = Vec::with_capacity(cal.exc.len() * 2);
        let mut exc_prefix = Vec::with_capacity(cal.exc.len() * 2 + 1);
        exc_prefix.push(0);
        let mut last = i64::MIN;
        let mut running = 0_i64;
        for &key in cal.exc.keys() {
            let x = key - cal.origin_serial;
            for d in [x, x + 1] {
                if d > last {
                    let tot = cal.total_hours_on_date(d);
                    running += tot - pattern[d.rem_euclid(7) as usize];
                    exc_days.push(d);
                    exc_totals.push(tot);
                    exc_prefix.push(running);
                    last = d;
                }
            }
        }
        Self {
            continuous: false,
            pattern,
            week,
            exc_days,
            exc_prefix,
            exc_totals,
        }
    }

    fn total_hours_on_date(&self, day: i64) -> i64 {
        if self.continuous {
            return DAY;
        }
        match self.exc_days.binary_search(&day) {
            Ok(idx) => self.exc_totals[idx],
            Err(_) => self.pattern[day.rem_euclid(7) as usize],
        }
    }

    fn work_in_days(&self, a: i64, b: i64) -> i64 {
        if b <= a {
            return 0;
        }
        if self.continuous {
            return (b - a) * DAY;
        }
        let n = b - a;
        let first = a.rem_euclid(7);
        let mut total = n.div_euclid(7) * self.week
            + (0..n.rem_euclid(7))
                .map(|k| self.pattern[((first + k) % 7) as usize])
                .sum::<i64>();
        if !self.exc_days.is_empty() {
            let lo = self.exc_days.partition_point(|&d| d < a);
            let hi = self.exc_days.partition_point(|&d| d < b);
            total += self.exc_prefix[hi] - self.exc_prefix[lo];
        }
        total
    }

    fn locate(&self, cal: &WorkCalendar, w: i64, at_close: bool) -> (i64, i64) {
        let past = |c: i64| if at_close { c >= w } else { c > w };
        if self.week <= 0 && !self.continuous {
            let (mut day, mut rem) = (0, w);
            loop {
                let tot = self.total_hours_on_date(day);
                if (at_close && rem <= tot) || (!at_close && rem < tot) {
                    return (day, rem);
                }
                rem -= tot;
                day += 1;
            }
        }
        let _ = cal;
        let c1 = self.work_in_days(0, 1);
        let m = if past(c1) {
            1
        } else {
            let m_init = ((w.max(0) / self.week).saturating_mul(7)).max(2);
            let (mut lo, mut hi) = if past(self.work_in_days(0, m_init)) {
                (2_i64, m_init)
            } else {
                let mut lo = m_init;
                let mut step = 7_i64;
                let mut hi = lo.saturating_add(step);
                while !past(self.work_in_days(0, hi)) {
                    lo = hi;
                    step = step.saturating_mul(2);
                    hi = lo.saturating_add(step);
                }
                (lo + 1, hi)
            };
            while lo < hi {
                let mid = lo + (hi - lo) / 2;
                if past(self.work_in_days(0, mid)) {
                    hi = mid;
                } else {
                    lo = mid + 1;
                }
            }
            lo
        };
        let day = m - 1;
        (day, w - self.work_in_days(0, day))
    }

    pub(crate) fn work_at(&self, cal: &WorkCalendar, t: i64) -> i64 {
        if t < 0 {
            mark_before_origin();
        }
        let day = t.div_euclid(DAY);
        let tod = t - day * DAY;
        self.work_in_days(0, day)
            + cal
                .work_shifts(day)
                .iter()
                .map(|&(s, e)| (e.min(tod) - s).max(0))
                .sum::<i64>()
    }

    pub(crate) fn start_at(&self, cal: &WorkCalendar, w: i64) -> i64 {
        note_position(w);
        let (day, mut w) = self.locate(cal, w, false);
        let shifts = cal.work_shifts(day);
        for &(s, e) in shifts.iter() {
            if w < e - s {
                return day * DAY + s + w;
            }
            w -= e - s;
        }
        let last = shifts.last().map_or(0, |&(_, e)| e);
        for &(s, _) in shifts.iter() {
            if s >= last {
                return day * DAY + s;
            }
        }
        (day + 1) * DAY
    }

    pub(crate) fn finish_at(&self, cal: &WorkCalendar, w: i64) -> i64 {
        note_position(w);
        let (day, mut w) = self.locate(cal, w, true);
        for &(s, e) in cal.work_shifts(day).iter() {
            if w <= e - s {
                return day * DAY + s + w;
            }
            w -= e - s;
        }
        (day + 1) * DAY
    }
}

#[derive(Clone, Copy)]
pub(crate) struct CalRef<'a> {
    cal: &'a WorkCalendar,
    ix: &'a CalIndex,
}

impl<'a> CalRef<'a> {
    pub(crate) fn new(cal: &'a WorkCalendar, ix: &'a CalIndex) -> Self {
        Self { cal, ix }
    }
    pub(crate) fn add(self, t: i64, m: i64) -> i64 {
        if m == 0 {
            self.snap_start(t)
        } else if m < 0 {
            self.sub(t, -m)
        } else {
            self.finish_at(self.work_at(t) + m)
        }
    }
    pub(crate) fn sub(self, t: i64, m: i64) -> i64 {
        if m == 0 {
            self.snap_finish(t)
        } else if m < 0 {
            self.add(t, -m)
        } else {
            self.start_at(note_position(self.work_at(t) - m).max(0))
        }
    }
    pub(crate) fn between(self, a: i64, b: i64) -> i64 {
        self.work_at(b) - self.work_at(a)
    }
    pub(crate) fn work_at(self, t: i64) -> i64 {
        self.ix.work_at(self.cal, t)
    }
    pub(crate) fn start_at(self, w: i64) -> i64 {
        self.ix.start_at(self.cal, w)
    }
    pub(crate) fn finish_at(self, w: i64) -> i64 {
        self.ix.finish_at(self.cal, w)
    }
    pub(crate) fn add_start(self, t: i64, m: i64) -> i64 {
        self.start_at(self.work_at(t) + m)
    }
    pub(crate) fn snap_start(self, t: i64) -> i64 {
        self.start_at(self.work_at(t))
    }
    pub(crate) fn snap_finish(self, t: i64) -> i64 {
        self.finish_at(self.work_at(t))
    }
    pub(crate) fn is_work(self, t: i64) -> bool {
        self.cal.in_work(t, t.div_euclid(DAY))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn cal() -> CalendarAdapter {
        CalendarAdapter::new(WorkCalendar::new(
            "5x8".into(),
            false,
            45000,
            std::array::from_fn(|i| {
                if i < 5 {
                    vec![(480, 720), (780, 1020)]
                } else {
                    vec![]
                }
            }),
            BTreeMap::new(),
        ))
    }
    #[test]
    fn split_shift_boundaries() {
        let c = cal();
        assert_eq!(c.start_at(240), 780);
        assert_eq!(c.finish_at(240), 720);
        assert_eq!(c.add(480, 480), 1020);
        assert_eq!(c.sub(1020, 480), 480);
        assert_eq!(c.work_at(-1), 0);
    }
    /// CAL-10: the position functions as they were, walking every day from
    /// day 0. The arithmetic must give exactly these answers.
    fn walk_work_at(c: &WorkCalendar, t: i64) -> i64 {
        let day = t.div_euclid(DAY);
        let tod = t - day * DAY;
        (0..day).map(|d| c.total_hours_on_date(d)).sum::<i64>()
            + c.work_shifts(day)
                .iter()
                .map(|&(s, e)| (e.min(tod) - s).max(0))
                .sum::<i64>()
    }
    fn walk_locate(c: &WorkCalendar, mut w: i64, at_close: bool) -> (i64, i64) {
        let mut day = 0;
        loop {
            let tot = c.total_hours_on_date(day);
            if (at_close && w <= tot) || (!at_close && w < tot) {
                return (day, w);
            }
            w -= tot;
            day += 1;
        }
    }
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self, n: u64) -> i64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((self.0 >> 33) % n) as i64
        }
        fn shifts(&mut self) -> Shifts {
            match self.next(9) {
                0 | 1 => vec![],
                2 => vec![(480, 1020)],
                3 => vec![(480, 720), (780, 1020)],
                4 => vec![(780, 1020), (480, 900)],
                5 => vec![(1200, 120)],
                6 => vec![(0, 0)],
                7 => vec![(360, 600), (1320, 360)],
                _ => {
                    let s = self.next(1400);
                    vec![(s, (s + 1 + self.next(1439)) % DAY)]
                }
            }
        }
    }
    /// CAL-10: work_at, start_at and finish_at no longer walk from day 0 but give
    /// the same answers as the walk on every calendar shape the engine
    /// accepts (split, unsorted, overlapping, overnight, 24-hour, exceptions
    /// in runs and on consecutive days, long closures), at every minute of
    /// the first days and at sampled positions out to several years.
    #[test]
    fn positions_match_the_day_walk() {
        let mut rng = Lcg(7);
        for case in 0..40 {
            let origin = 45000 + rng.next(7);
            let template: [Shifts; 7] = std::array::from_fn(|_| rng.shifts());
            let mut exc = BTreeMap::new();
            for _ in 0..rng.next(40) {
                let at = origin - 3 + rng.next(800);
                for k in 0..1 + rng.next(4) * rng.next(30) {
                    exc.insert(at + k, rng.shifts());
                }
            }
            let c = WorkCalendar::new("r".into(), case % 17 == 0, origin, template, exc);
            if c.template.iter().all(|s| s.is_empty()) && !c.continuous {
                continue;
            }
            let mut ts: Vec<i64> = (-2 * DAY..9 * DAY).step_by(7).collect();
            ts.extend((0..80).map(|_| rng.next(1200) * DAY + rng.next(DAY as u64)));
            for t in ts {
                assert_eq!(
                    c.work_at(t),
                    walk_work_at(&c, t),
                    "case {case} work_at({t})"
                );
            }
            let year = walk_work_at(&c, 3 * 365 * DAY);
            let mut ws: Vec<i64> = (-30..4 * DAY).step_by(3).collect();
            ws.extend((0..80).map(|_| rng.next(year as u64 + 1)));
            ws.extend((0..20).map(|_| walk_work_at(&c, rng.next(1100) * DAY)));
            for w in ws {
                for at_close in [false, true] {
                    assert_eq!(
                        c.locate(w, at_close),
                        walk_locate(&c, w, at_close),
                        "case {case} locate({w}, {at_close})"
                    );
                }
            }
        }
    }
    /// CAL-10: a position far from the origin (about the year 9999 counted from a
    /// 2023 origin) is answered without walking millions of days.
    #[test]
    fn far_positions_answer_quickly() {
        let mut c = cal().cal;
        for k in 0..2000 {
            c.exc.insert(45000 + 3 * k, vec![]);
        }
        let far = (2_958_465 - 45000) * DAY;
        let started = std::time::Instant::now();
        for i in 0..200 {
            let t = far - i * 997;
            let w = c.work_at(t);
            assert!(c.start_at(w) <= t + 3 * DAY && c.finish_at(w) <= t);
        }
        assert!(started.elapsed() < std::time::Duration::from_secs(20));
    }
    #[test]
    fn midnight_and_negative_floor() {
        let c = cal();
        assert_eq!(c.cal.next_work_time(-1440, 1440), 480);
        assert_eq!(c.cal.duration(480, 1020), 480);
        assert_eq!(c.cal.duration(1020, 480), -480);
    }

    #[test]
    fn cal_index_matches_work_calendar_on_random_and_overtime_calendars() {
        let mut rng = Lcg(99);
        for case in 0..40 {
            let origin = 45000 + rng.next(7);
            let template: [Shifts; 7] = std::array::from_fn(|_| rng.shifts());
            let mut exc = BTreeMap::new();
            for _ in 0..rng.next(45) {
                let at = origin - 3 + rng.next(900);
                for k in 0..1 + rng.next(4) * rng.next(25) {
                    // Mix empty holidays and overtime 16-hour / 24-hour exceptions.
                    let shifts = if rng.next(3) == 0 {
                        vec![(360, 1320)]
                    } else {
                        rng.shifts()
                    };
                    exc.insert(at + k, shifts);
                }
            }
            let wc = WorkCalendar::new("r".into(), case % 17 == 0, origin, template, exc);
            if wc.template.iter().all(|s| s.is_empty()) && !wc.continuous {
                continue;
            }
            let adapter = CalendarAdapter::new(wc);
            let ix = CalIndex::new(&adapter.cal);
            let cref = CalRef::new(&adapter.cal, &ix);

            let mut ts: Vec<i64> = (-2 * DAY..9 * DAY).step_by(19).collect();
            ts.extend((0..60).map(|_| rng.next(1500) * DAY + rng.next(DAY as u64)));
            ts.push((2_958_464 - origin) * DAY + 480);
            for t in ts {
                clear_before_origin();
                let expected = adapter.work_at(t);
                let exp_mark = take_before_origin();
                clear_before_origin();
                let got = cref.work_at(t);
                let got_mark = take_before_origin();
                assert_eq!((got, got_mark), (expected, exp_mark), "case {case} t={t}");
                assert_eq!(cref.snap_start(t), adapter.snap_start(t));
                assert_eq!(cref.snap_finish(t), adapter.snap_finish(t));
            }
            let year = adapter.work_at(3 * 365 * DAY);
            let mut ws: Vec<i64> = (-30..3 * DAY).step_by(7).collect();
            ws.extend((0..60).map(|_| rng.next(year as u64 + 1)));
            for w in ws {
                clear_before_origin();
                let es = adapter.start_at(w);
                let ms = take_before_origin();
                clear_before_origin();
                let gs = cref.start_at(w);
                let mgs = take_before_origin();
                assert_eq!((gs, mgs), (es, ms), "case {case} start_at({w})");

                clear_before_origin();
                let ef = adapter.finish_at(w);
                let mf = take_before_origin();
                clear_before_origin();
                let gf = cref.finish_at(w);
                let mgf = take_before_origin();
                assert_eq!((gf, mgf), (ef, mf), "case {case} finish_at({w})");
            }
        }
    }
}
