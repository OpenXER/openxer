//! Rule CON-9 in `engine/RULES.md`: an activity in progress whose primary
//! constraint is Mandatory Start or Mandatory Finish.
//!
//! The settled part holds for every CPM scheduler and is asserted for both
//! readings: work not complete gets early, late and remaining dates and a total
//! float, and a finish-to-start successor waits for its in-progress
//! predecessor's remaining work. Where the mandatory date places the started
//! activity is NOT settled (no reference run yet; probes 1401 to 1404 are
//! pending), so the expected dates are written for both readings and the
//! switch `MANDATORY_PINS_STARTED_WORK` chooses which set applies.
//!
//! Calendar Monday to Friday 08:00-16:00. Day 0 is Monday 2026-02-23; the
//! data date is day 7 (Monday 2026-03-02) 08:00. A started day 0 08:00 with
//! 40 h left; A finish-to-start B (24 h), B finish-to-start C (16 h).
use openxer_engine::{
    calendar::{CalendarAdapter, WorkCalendar},
    model::{
        Activity, CstrType, Link, LinkType, Model, ProjectSettings, StatusType,
        MANDATORY_PINS_STARTED_WORK,
    },
    options::ScheduleOptions,
    scheduler,
};
use std::collections::BTreeMap;

const DAY: i64 = 1440;
// Serial 46076 is Monday 2026-02-23.
const ORIGIN: i64 = 46076;
const HOUR: i64 = 60;

fn at(day: i64, hour: i64) -> i64 {
    day * DAY + hour * HOUR
}

fn task(id: i64, code: &str, rem: i64) -> Activity {
    Activity {
        task_id: id,
        task_code: code.into(),
        rem,
        ..Activity::default()
    }
}

fn tie(pred: usize, succ: usize) -> Link {
    Link {
        pred,
        succ,
        link_type: LinkType::FS,
        lag: 0,
        driving_path: false,
    }
}

fn options(progress_override: bool) -> ScheduleOptions {
    ScheduleOptions {
        retained_logic: !progress_override,
        progress_override,
        ..ScheduleOptions::default()
    }
}

/// The reported schedule with A's primary constraint `cstr` on `date`.
fn schedule(cstr: CstrType, date: i64, progress_override: bool) -> Model {
    let mut week: [Vec<(i64, i64)>; 7] = Default::default();
    for day in week.iter_mut().take(5) {
        *day = vec![(8 * HOUR, 16 * HOUR)];
    }
    let cal = CalendarAdapter::new(WorkCalendar::new(
        "8h".into(),
        false,
        ORIGIN,
        week,
        BTreeMap::new(),
    ));
    let mut a = task(1, "A", 40 * HOUR);
    a.status = StatusType::InProgress;
    a.act_start = Some(at(0, 8));
    a.cstr = cstr;
    a.cstr_date = Some(date);
    let mut m = Model {
        origin_serial: ORIGIN,
        project_calendar: 0,
        calendars: vec![cal],
        activities: vec![a, task(2, "B", 24 * HOUR), task(3, "C", 16 * HOUR)],
        links: vec![tie(0, 1), tie(1, 2)],
        project: ProjectSettings {
            data_date: Some(at(7, 8)),
            ..ProjectSettings::default()
        },
        options: options(progress_override),
    };
    m.rebuild_links().unwrap();
    scheduler::schedule(&mut m).unwrap();
    m
}

/// (early start, early finish, late start, late finish, total float) of
/// every activity, then the project finish.
type Dates = (i64, i64, i64, i64, i64);

fn dates(m: &Model) -> (Vec<Dates>, i64) {
    let rows = m
        .activities
        .iter()
        .map(|a| {
            let row = (
                a.early_start
                    .unwrap_or_else(|| panic!("{} has no early start", a.task_code)),
                a.early_finish
                    .unwrap_or_else(|| panic!("{} has no early finish", a.task_code)),
                a.late_start
                    .unwrap_or_else(|| panic!("{} has no late start", a.task_code)),
                a.late_finish
                    .unwrap_or_else(|| panic!("{} has no late finish", a.task_code)),
                a.total_float
                    .unwrap_or_else(|| panic!("{} has no total float", a.task_code)),
            );
            // PASS-8: remaining dates equal the early and late dates.
            assert_eq!(
                a.rem_restart,
                Some(row.0),
                "{} remaining start",
                a.task_code
            );
            assert_eq!(a.rem_reend, Some(row.1), "{} remaining finish", a.task_code);
            assert_eq!(
                a.rem_late_start,
                Some(row.2),
                "{} remaining late start",
                a.task_code
            );
            assert_eq!(
                a.rem_late_finish,
                Some(row.3),
                "{} remaining late finish",
                a.task_code
            );
            row
        })
        .collect::<Vec<_>>();
    (rows, m.project.scd_end.expect("project finish"))
}

/// The settled part: B starts no earlier than A's remaining finish (rolled to
/// the next working start) and C no earlier than B's finish.
fn assert_logic_holds(m: &Model) {
    let (rows, finish) = dates(m);
    let (a, b, c) = (rows[0], rows[1], rows[2]);
    assert!(b.0 >= a.1, "B starts {} before A finishes {}", b.0, a.1);
    assert!(c.0 >= b.1, "C starts {} before B finishes {}", c.0, b.1);
    assert_eq!(finish, c.1);
}

fn row(es: (i64, i64), ef: (i64, i64), ls: (i64, i64), lf: (i64, i64), tf: i64) -> Dates {
    (
        at(es.0, es.1),
        at(ef.0, ef.1),
        at(ls.0, ls.1),
        at(lf.0, lf.1),
        tf * HOUR,
    )
}

/// A runs from the data date over its 40 h (Mon 03-02 08:00 to Fri 03-06
/// 16:00), B Mon 03-09 to Wed 03-11, C Thu 03-12 to Fri 03-13 16:00: the
/// answer when the passes schedule started mandatory work by its logic.
fn by_logic() -> (Vec<Dates>, i64) {
    (
        vec![
            row((7, 8), (11, 16), (7, 8), (11, 16), 0),
            row((14, 8), (16, 16), (14, 8), (16, 16), 0),
            row((17, 8), (18, 16), (17, 8), (18, 16), 0),
        ],
        at(18, 16),
    )
}

/// Rule CON-9: Mandatory Finish Fri 2026-03-13 16:00 on the started A, under
/// Retained Logic and under Progress Override.
#[test]
fn a_started_activity_with_a_mandatory_finish_keeps_its_dates_and_its_successor_waits() {
    for progress_override in [false, true] {
        let m = schedule(CstrType::MandatoryFinish, at(18, 16), progress_override);
        assert_logic_holds(&m);
        let expected = if MANDATORY_PINS_STARTED_WORK {
            // Pinned as unstarted work is: A's 40 h end on the date (Mon 03-09
            // 08:00 to Fri 03-13 16:00), B Mon 03-16 to Wed 03-18, C Thu 03-19
            // to Fri 03-20.
            (
                vec![
                    row((14, 8), (18, 16), (14, 8), (18, 16), 0),
                    row((21, 8), (23, 16), (21, 8), (23, 16), 0),
                    row((24, 8), (25, 16), (24, 8), (25, 16), 0),
                ],
                at(25, 16),
            )
        } else {
            by_logic()
        };
        assert_eq!(dates(&m), expected, "progress override {progress_override}");
    }
}

/// Rule CON-9: Mandatory Start Wed 2026-03-04 08:00 on the started A, under
/// Retained Logic and under Progress Override.
#[test]
fn a_started_activity_with_a_mandatory_start_keeps_its_dates_and_its_successor_waits() {
    for progress_override in [false, true] {
        let m = schedule(CstrType::MandatoryStart, at(9, 8), progress_override);
        assert_logic_holds(&m);
        let expected = if MANDATORY_PINS_STARTED_WORK {
            // Pinned as unstarted work is: A Wed 03-04 08:00 to Tue 03-10
            // 16:00, B Wed 03-11 to Fri 03-13, C Mon 03-16 to Tue 03-17.
            (
                vec![
                    row((9, 8), (15, 16), (9, 8), (15, 16), 0),
                    row((16, 8), (18, 16), (16, 8), (18, 16), 0),
                    row((21, 8), (22, 16), (21, 8), (22, 16), 0),
                ],
                at(22, 16),
            )
        } else {
            by_logic()
        };
        assert_eq!(dates(&m), expected, "progress override {progress_override}");
    }
}

/// Rule CON-9 with CON-5: a Mandatory Finish earlier than the started A can
/// finish (Thu 2026-03-05 16:00). Both readings give the same answer here:
/// the date caps A's late finish (a late finish constraint binds unfinished
/// work, CON-5), A cannot start before the data date, and the slip shows as
/// negative float (late Fri 02-27 08:00 to Thu 03-05 16:00: one working day,
/// 8 h, behind at both ends).
#[test]
fn a_mandatory_finish_before_a_started_activity_can_finish_shows_in_its_float() {
    for progress_override in [false, true] {
        let m = schedule(CstrType::MandatoryFinish, at(10, 16), progress_override);
        assert_logic_holds(&m);
        let expected = (
            vec![
                row((7, 8), (11, 16), (4, 8), (10, 16), -8),
                row((14, 8), (16, 16), (14, 8), (16, 16), 0),
                row((17, 8), (18, 16), (17, 8), (18, 16), 0),
            ],
            at(18, 16),
        );
        assert_eq!(dates(&m), expected, "progress override {progress_override}");
    }
}
