//! Shift shapes the stored calendar may hold that the position arithmetic
//! must still read correctly: shifts stored out of order or overlapping, and
//! overnight shifts. The stored shifts are kept exactly as stored (they
//! round-trip unchanged); the arithmetic reads their working time (CAL-1,
//! CAL-10 in `engine/RULES.md`).
//!
//! Each calendar is checked against an independent reading: every stored
//! shift placed on the absolute time line at the day it starts on, the union
//! of those intervals counted from the origin's midnight.
//!
//! Days are counted from a Monday origin: 0 Monday ... 6 Sunday.
use openxer_engine::{
    calendar::{CalendarAdapter, WorkCalendar},
    model::{Activity, Model, ProjectSettings},
    options::ScheduleOptions,
    scheduler,
};
use std::collections::BTreeMap;

const DAY: i64 = 1440;
const ORIGIN: i64 = 44998;
const SPAN: i64 = 21;

fn at(day: i64, hour: i64, minute: i64) -> i64 {
    day * DAY + hour * 60 + minute
}

fn calendar(week: [Vec<(i64, i64)>; 7], exc: BTreeMap<i64, Vec<(i64, i64)>>) -> CalendarAdapter {
    CalendarAdapter::new(WorkCalendar::new("shapes".into(), false, ORIGIN, week, exc))
}

fn weekdays(shifts: &[(i64, i64)]) -> CalendarAdapter {
    calendar(
        std::array::from_fn(|i| if i < 5 { shifts.to_vec() } else { vec![] }),
        BTreeMap::new(),
    )
}

/// The independent reading: the union of the stored shifts on the absolute
/// time line, from the day before the origin on.
struct Union(Vec<(i64, i64)>);

impl Union {
    fn of(c: &CalendarAdapter) -> Self {
        let mut raw = vec![];
        for d in -1..SPAN + 2 {
            for &(s, e) in c.cal.shifts_on(d) {
                raw.push((d * DAY + s, d * DAY + e));
            }
        }
        raw.sort_unstable();
        let mut out: Vec<(i64, i64)> = vec![];
        for (s, e) in raw {
            match out.last_mut() {
                Some(last) if s < last.1 => last.1 = last.1.max(e),
                _ => out.push((s, e)),
            }
        }
        // Counted from the origin's midnight.
        Self(
            out.into_iter()
                .filter(|&(_, e)| e > 0)
                .map(|(s, e)| (s.max(0), e))
                .collect(),
        )
    }
    fn work_at(&self, t: i64) -> i64 {
        self.0.iter().map(|&(s, e)| (e.min(t) - s).max(0)).sum()
    }
    fn start_at(&self, mut w: i64) -> i64 {
        for &(s, e) in &self.0 {
            if w < e - s {
                return s + w;
            }
            w -= e - s;
        }
        unreachable!("position past the span")
    }
    fn finish_at(&self, mut w: i64) -> i64 {
        if w == 0 {
            return self.0[0].0;
        }
        for &(s, e) in &self.0 {
            if w <= e - s {
                return s + w;
            }
            w -= e - s;
        }
        unreachable!("position past the span")
    }
}

/// Every position function agrees with the independent reading over three
/// weeks, minute positions every 5 minutes.
fn agrees_with_the_union(c: &CalendarAdapter) {
    let u = Union::of(c);
    let end = (SPAN - 1) * DAY;
    for t in (0..end).step_by(5) {
        assert_eq!(c.work_at(t), u.work_at(t), "work_at({t})");
        assert_eq!(
            c.is_work(t),
            u.work_at(t + 1) > u.work_at(t),
            "is_work({t})"
        );
    }
    for w in (0..u.work_at(end - 2 * DAY)).step_by(5) {
        assert_eq!(c.start_at(w), u.start_at(w), "start_at({w})");
        assert_eq!(c.finish_at(w), u.finish_at(w), "finish_at({w})");
    }
}

fn one_task(c: CalendarAdapter, data_date: i64, rem: i64) -> Model {
    let mut m = Model {
        origin_serial: ORIGIN,
        project_calendar: 0,
        calendars: vec![c],
        activities: vec![Activity {
            task_id: 1,
            task_code: "A".into(),
            rem,
            ..Activity::default()
        }],
        links: vec![],
        project: ProjectSettings {
            data_date: Some(data_date),
            ..ProjectSettings::default()
        },
        options: ScheduleOptions::default(),
    };
    m.rebuild_links().unwrap();
    m
}

/// Rule CAL-1, CAL-10 in `engine/RULES.md`: overlapping shifts stored out of
/// order are read as their sorted union (13:00-17:00 and 08:00-15:00 is
/// 08:00-17:00, nine hours), and the stored shifts stay as stored.
#[test]
fn overlapping_shifts_stored_out_of_order_are_read_as_their_union() {
    let stored = vec![(780, 1020), (480, 900)];
    let c = weekdays(&stored);
    assert_eq!(c.start_at(0), at(0, 8, 0));
    assert_eq!(c.cal.total_hours_on_date(0), 540);
    assert_eq!(c.work_at(DAY), 540);
    assert_eq!(c.add(at(0, 8, 0), 540), at(0, 17, 0));
    assert_eq!(c.cal.template[0], stored);
    agrees_with_the_union(&c);
    // A nine-hour activity fills Monday.
    let mut m = one_task(c, at(0, 8, 0), 540);
    scheduler::schedule(&mut m).unwrap();
    let a = &m.activities[0];
    assert_eq!(
        (a.early_start, a.early_finish),
        (Some(at(0, 8, 0)), Some(at(0, 17, 0)))
    );
    assert_eq!(m.calendars[0].cal.template[0], stored);
}

/// Rule CAL-1, CAL-10 in `engine/RULES.md`: disjoint shifts stored out of
/// order give the same arithmetic as the same shifts in order, the
/// shift-close reading included.
#[test]
fn disjoint_shifts_stored_out_of_order_read_as_in_order() {
    let unsorted = weekdays(&[(780, 1020), (480, 720)]);
    let sorted = weekdays(&[(480, 720), (780, 1020)]);
    for t in (0..SPAN * DAY).step_by(5) {
        assert_eq!(unsorted.work_at(t), sorted.work_at(t), "work_at({t})");
        assert_eq!(
            unsorted.snap_start(t),
            sorted.snap_start(t),
            "snap_start({t})"
        );
        assert_eq!(
            unsorted.snap_finish(t),
            sorted.snap_finish(t),
            "snap_finish({t})"
        );
        assert_eq!(unsorted.add(t, 300), sorted.add(t, 300), "add({t})");
        assert_eq!(unsorted.sub(t, 300), sorted.sub(t, 300), "sub({t})");
        assert_eq!(
            unsorted.cal.next_non_work_time(t, t + DAY),
            sorted.cal.next_non_work_time(t, t + DAY),
            "next_non_work_time({t})"
        );
    }
    // A work position on the morning shift's close is the afternoon opening
    // as a start and the close as a finish.
    assert_eq!(unsorted.start_at(240), at(0, 13, 0));
    assert_eq!(unsorted.finish_at(240), at(0, 12, 0));
    agrees_with_the_union(&unsorted);
}

/// Monday to Friday nights, 20:00 to 02:00 (stored as one shift that ends
/// after midnight).
fn nights() -> CalendarAdapter {
    calendar(
        std::array::from_fn(|i| if i < 5 { vec![(1200, 120)] } else { vec![] }),
        BTreeMap::new(),
    )
}

/// Rule CAL-1, CAL-10 in `engine/RULES.md`: an overnight shift belongs to the
/// day it starts on and its part after midnight is working time of the next
/// day, so work at 01:00 starts at 01:00, not at 20:00 that evening.
#[test]
fn an_overnight_shift_continues_past_midnight() {
    let c = nights();
    assert_eq!(c.cal.template[0], vec![(1200, 1560)]);
    // Tuesday 01:00 is inside Monday night's shift.
    assert!(c.is_work(at(1, 1, 0)));
    assert_eq!(c.snap_start(at(1, 1, 0)), at(1, 1, 0));
    assert_eq!(c.add(at(1, 1, 0), 60), at(1, 2, 0));
    assert_eq!(c.between(at(0, 20, 0), at(1, 2, 0)), 360);
    // Saturday 01:00 finishes Friday night; Sunday 01:00 is off.
    assert!(c.is_work(at(5, 1, 0)));
    assert!(!c.is_work(at(6, 1, 0)));
    // Monday 00:00-02:00 belongs to Sunday, which has no shift.
    assert!(!c.is_work(at(0, 1, 0)));
    agrees_with_the_union(&c);
    // An hour of work from Tuesday 01:00 runs 01:00 to 02:00.
    let mut m = one_task(c, at(1, 1, 0), 60);
    scheduler::schedule(&mut m).unwrap();
    let a = &m.activities[0];
    assert_eq!(
        (a.early_start, a.early_finish),
        (Some(at(1, 1, 0)), Some(at(1, 2, 0)))
    );
    assert_eq!(m.calendars[0].cal.template[0], vec![(1200, 1560)]);
}

/// Rule CAL-1 in `engine/RULES.md`: a dated exception replaces the shifts
/// that start on its day. The rest of the previous night's shift still
/// works on it, its own overnight shift runs into the next day, and an
/// overnight end overlapping the next day's own shift counts once.
#[test]
fn exceptions_and_the_next_days_shifts_combine_with_an_overnight_shift() {
    let mut exc = BTreeMap::new();
    // Tuesday off.
    exc.insert(ORIGIN + 1, vec![]);
    // Saturday: one night shift, 22:00 to 04:00 Sunday.
    exc.insert(ORIGIN + 5, vec![(1320, 240)]);
    let mut week: [Vec<(i64, i64)>; 7] =
        std::array::from_fn(|i| if i < 5 { vec![(1200, 120)] } else { vec![] });
    // Thursday also has a day shift 01:00-09:00, overlapping Wednesday
    // night's end.
    week[3] = vec![(60, 540), (1200, 120)];
    let c = calendar(
        week,
        exc.into_iter()
            .map(|(d, v)| {
                let v = v
                    .into_iter()
                    .map(|(s, e): (i64, i64)| if e <= s { (s, e + DAY) } else { (s, e) })
                    .collect();
                (d, v)
            })
            .collect(),
    );
    // Monday night's shift still runs to Tuesday 02:00.
    assert!(c.is_work(at(1, 1, 0)));
    assert!(!c.is_work(at(1, 21, 0)));
    // Tuesday had no night shift, so Wednesday 01:00 is off.
    assert!(!c.is_work(at(2, 1, 0)));
    // Thursday: 00:00-02:00 from Wednesday night and 01:00-09:00 count once.
    assert_eq!(c.cal.total_hours_on_date(3), 540 + 240);
    assert_eq!(c.between(at(3, 0, 0), at(3, 9, 0)), 540);
    // Saturday's exception night shift runs to Sunday 04:00.
    assert!(c.is_work(at(6, 3, 0)));
    assert!(!c.is_work(at(6, 4, 0)));
    agrees_with_the_union(&c);
}
