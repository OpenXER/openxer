//! Level of Effort results as a fresh F9 stores them: remaining duration from
//! the dates, late dates on the LOE's own calendar.
use openxer_engine::{
    calendar::{CalendarAdapter, WorkCalendar},
    model::{Activity, CstrType, Link, LinkType, Model, ProjectSettings, StatusType, TaskType},
    options::ScheduleOptions,
    scheduler,
};
use std::collections::BTreeMap;

const DAY: i64 = 1440;
// Serial 44998 is a Monday, so template index 0 is Monday.
const ORIGIN: i64 = 44998;
const OPEN: i64 = 420;
const CLOSE: i64 = 1020;

fn calendar(name: &str, days: usize) -> CalendarAdapter {
    let mut week: [Vec<(i64, i64)>; 7] = Default::default();
    for day in week.iter_mut().take(days) {
        *day = vec![(OPEN, CLOSE)];
    }
    CalendarAdapter::new(WorkCalendar::new(
        name.into(),
        false,
        ORIGIN,
        week,
        BTreeMap::new(),
    ))
}

/// Calendar 0 works Monday to Friday 07:00-17:00, calendar 1 every day.
/// The data date is Monday 07:00 and the project must finish in week four
/// (the must-finish date bounds the late dates).
fn model(activities: Vec<Activity>, links: Vec<Link>) -> Model {
    let mut m = Model {
        origin_serial: ORIGIN,
        project_calendar: 0,
        calendars: vec![calendar("5x10", 5), calendar("7x10", 7)],
        activities,
        links,
        project: ProjectSettings {
            data_date: Some(OPEN),
            plan_end: Some(24 * DAY + CLOSE),
            ..ProjectSettings::default()
        },
        options: ScheduleOptions {
            use_must_finish: true,
            retained_logic: true,
            open_end_uses_early: false,
            ..ScheduleOptions::default()
        },
    };
    m.rebuild_links().unwrap();
    m
}

fn task(id: i64, code: &str, rem: i64) -> Activity {
    Activity {
        task_id: id,
        task_code: code.into(),
        rem,
        ..Activity::default()
    }
}

fn loe(id: i64, code: &str) -> Activity {
    Activity {
        task_type: TaskType::Loe,
        ..task(id, code, 0)
    }
}

fn link(pred: usize, succ: usize, link_type: LinkType) -> Link {
    Link {
        pred,
        succ,
        link_type,
        lag: 0,
        driving_path: false,
    }
}

#[test]
fn loe_remaining_duration_is_the_span_of_its_dates() {
    // The LOE starts with the first task (SS) and finishes with the second
    // (FF): Monday 07:00 to Thursday 17:00 is four working days.
    let first = task(1, "A", 600);
    let second = task(2, "B", 1800);
    let mut m = model(
        vec![first, second, loe(3, "LOE")],
        vec![
            link(0, 1, LinkType::FS),
            link(0, 2, LinkType::SS),
            link(1, 2, LinkType::FF),
        ],
    );
    scheduler::schedule(&mut m).unwrap();
    let l = &m.activities[2];
    assert_eq!((l.restart, l.reend), (Some(OPEN), Some(3 * DAY + CLOSE)));
    assert_eq!(l.rem, 4 * 600);
}

#[test]
fn loe_on_finish_to_start_ties_takes_its_span_too() {
    // A started LOE between a started task and a later one (FS both ways):
    // Tuesday 07:00 to Friday 07:00 is three working days.
    let mut a = task(1, "A", 600);
    a.status = StatusType::InProgress;
    a.act_start = Some(OPEN - DAY);
    let c = task(2, "C", 1800);
    let b = task(3, "B", 600);
    let mut open = loe(4, "LOE");
    open.status = StatusType::InProgress;
    open.act_start = Some(OPEN - 7 * DAY);
    let mut m = model(
        vec![a, c, b, open],
        vec![
            link(0, 1, LinkType::FS),
            link(1, 2, LinkType::FS),
            link(0, 3, LinkType::FS),
            link(3, 2, LinkType::FS),
        ],
    );
    scheduler::schedule(&mut m).unwrap();
    let l = &m.activities[3];
    assert_eq!(
        (l.restart, l.reend),
        (Some(DAY + OPEN), Some(4 * DAY + OPEN))
    );
    assert_eq!(l.rem, 3 * 600);
}

#[test]
fn completed_loe_keeps_no_remaining_duration() {
    let mut done = loe(1, "LOE");
    done.status = StatusType::Completed;
    done.act_start = Some(OPEN);
    done.act_finish = Some(CLOSE);
    let mut m = model(vec![task(2, "A", 600), done], vec![]);
    scheduler::schedule(&mut m).unwrap();
    assert_eq!(m.activities[1].rem, 0);
}

#[test]
fn scheduling_twice_gives_the_same_loe_duration() {
    let mut m = model(
        vec![task(1, "A", 600), task(2, "B", 1800), loe(3, "LOE")],
        vec![
            link(0, 1, LinkType::FS),
            link(0, 2, LinkType::SS),
            link(1, 2, LinkType::FF),
        ],
    );
    scheduler::schedule(&mut m).unwrap();
    let first = m.activities[2].clone();
    scheduler::schedule(&mut m).unwrap();
    assert_eq!(m.activities[2], first);
}

/// A finish milestone on the seven-day calendar, held to a finish moment.
fn milestone(id: i64, code: &str, constraint_date: i64) -> Activity {
    Activity {
        task_type: TaskType::FinMile,
        cal: 1,
        cstr: CstrType::FinishOnBefore,
        cstr_date: Some(constraint_date),
        ..task(id, code, 0)
    }
}

#[test]
fn loe_late_dates_sit_on_working_moments_of_its_calendar() {
    // The predecessor milestone must finish Saturday 17:00 and the successor
    // milestone Saturday of the next week, both on the seven-day calendar.
    // The LOE works Monday to Friday: it starts the next Monday morning and
    // finishes the Friday before the successor's Saturday.
    let before = milestone(1, "P", 5 * DAY + CLOSE);
    let after = milestone(2, "S", 12 * DAY + CLOSE);
    let mut m = model(
        vec![before, loe(3, "LOE"), after],
        vec![link(0, 1, LinkType::FS), link(1, 2, LinkType::FS)],
    );
    scheduler::schedule(&mut m).unwrap();
    let l = &m.activities[1];
    assert_eq!(l.late_finish, Some(11 * DAY + CLOSE));
    assert_eq!(l.late_start, Some(7 * DAY + OPEN));
    assert_eq!(
        (l.rem_late_start, l.rem_late_finish),
        (l.late_start, l.late_finish)
    );
}

#[test]
fn loe_late_start_never_passes_its_late_finish() {
    // The completed start milestone has no later successor, so its late
    // dates are the project limit, long after the successor's constraint.
    let mut done = task(1, "P", 0);
    done.task_type = TaskType::Mile;
    done.status = StatusType::Completed;
    done.act_start = Some(0);
    done.act_finish = Some(0);
    let mut open = loe(2, "LOE");
    open.status = StatusType::InProgress;
    open.act_start = Some(0);
    let after = milestone(3, "S", 11 * DAY + CLOSE);
    let mut m = model(
        vec![done, open, after],
        vec![link(0, 1, LinkType::FS), link(1, 2, LinkType::FS)],
    );
    scheduler::schedule(&mut m).unwrap();
    let l = &m.activities[1];
    assert_eq!(l.late_finish, Some(11 * DAY + CLOSE));
    assert_eq!(l.late_start, l.late_finish);
}
