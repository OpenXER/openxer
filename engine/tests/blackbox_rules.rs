//! Scheduling rules stated as observable behavior, each on a small synthetic
//! project whose expected result follows from the rule alone. `engine/RULES.md`
//! lists the evidence class of every rule and the test here that pins it.
//!
//! Days are counted from a Monday origin: 0 Monday ... 4 Friday, 5 Saturday,
//! 6 Sunday, 7 the next Monday. Dates are minutes from origin midnight.
use openxer_engine::{
    calendar::{CalendarAdapter, WorkCalendar},
    model::{
        Activity, Assignment, CstrType, Link, LinkType, Model, ProjectSettings, StatusType,
        TaskType,
    },
    options::{FloatType, LagCalendar, OutOfSequence, ScheduleOptions},
    scheduler::{self, Scheduler},
};
use std::collections::BTreeMap;

const DAY: i64 = 1440;
// Serial 44998 is a Monday, so template index 0 is Monday.
const ORIGIN: i64 = 44998;
const FAR: i64 = 100 * DAY;

fn at(day: i64, hour: i64, minute: i64) -> i64 {
    day * DAY + hour * 60 + minute
}

fn weekdays(name: &str, shifts: &[(i64, i64)]) -> CalendarAdapter {
    let mut week: [Vec<(i64, i64)>; 7] = Default::default();
    for day in week.iter_mut().take(5) {
        *day = shifts.to_vec();
    }
    CalendarAdapter::new(WorkCalendar::new(
        name.into(),
        false,
        ORIGIN,
        week,
        BTreeMap::new(),
    ))
}

/// Calendar 0 works Monday to Friday 07:00-17:00, calendar 1 07:00-12:00 and
/// 13:00-18:00, calendar 2 08:00-16:00. The data date is Monday 07:00.
fn model(activities: Vec<Activity>, links: Vec<Link>, options: ScheduleOptions) -> Model {
    let mut m = Model {
        origin_serial: ORIGIN,
        project_calendar: 2,
        calendars: vec![
            weekdays("5x10", &[(420, 1020)]),
            weekdays("lunch", &[(420, 720), (780, 1080)]),
            weekdays("short", &[(480, 960)]),
        ],
        activities,
        links,
        project: ProjectSettings {
            data_date: Some(at(0, 7, 0)),
            ..ProjectSettings::default()
        },
        options,
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

fn tie(pred: usize, succ: usize, link_type: LinkType, lag: i64) -> Link {
    Link {
        pred,
        succ,
        link_type,
        lag,
        driving_path: false,
    }
}

fn plain() -> ScheduleOptions {
    ScheduleOptions {
        open_end_uses_early: false,
        ..ScheduleOptions::default()
    }
}

fn retained() -> ScheduleOptions {
    ScheduleOptions {
        retained_logic: true,
        ..plain()
    }
}

fn run(m: &mut Model) {
    scheduler::schedule(m).unwrap();
}

fn early(a: &Activity) -> (Option<i64>, Option<i64>) {
    (a.early_start, a.early_finish)
}

fn late(a: &Activity) -> (Option<i64>, Option<i64>) {
    (a.late_start, a.late_finish)
}

// ---------------------------------------------------------------------------
// Work calendars
// ---------------------------------------------------------------------------

/// Monday to Friday 08:00-12:00 and 13:00-17:00.
fn split() -> WorkCalendar {
    WorkCalendar::new(
        "split".into(),
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
    )
}

/// Rule CAL-1 in `engine/RULES.md`.
#[test]
fn a_days_work_is_the_sum_of_its_shifts_and_exceptions_replace_the_weekday() {
    let c = split();
    assert_eq!(c.total_hours_on_date(0), 480);
    assert_eq!(c.total_hours_on_date(4), 480);
    assert_eq!(c.total_hours_on_date(5), 0);
    assert_eq!(c.total_hours_on_date(7), 480);
    // The weekday pattern repeats backward too: day -3 is a Friday.
    assert_eq!(c.total_hours_on_date(-3), 480);
    assert_eq!(c.total_hours_on_date(-1), 0);
    let mut exc = BTreeMap::new();
    exc.insert(45003, vec![]); // Thursday off
    exc.insert(45005, vec![(480, 720)]); // Saturday morning on
    let c = WorkCalendar::new(
        "exceptions".into(),
        false,
        45000,
        std::array::from_fn(|i| if i < 5 { vec![(480, 1020)] } else { vec![] }),
        exc,
    );
    assert_eq!(c.total_hours_on_date(3), 0);
    assert_eq!(c.total_hours_on_date(5), 240);
    assert_eq!(c.total_hours_on_date(2), 540);
    assert_eq!(WorkCalendar::continuous(45000).total_hours_on_date(9), DAY);
}

/// Rule CAL-2 in `engine/RULES.md`.
#[test]
fn the_next_working_moment_is_this_minute_or_the_next_shift_start() {
    let c = split();
    let far = FAR;
    assert_eq!(c.next_work_time(500, far), 500);
    assert_eq!(c.next_work_time(300, far), 480);
    // Lunch, and the minute a shift closes, are not working time.
    assert_eq!(c.next_work_time(720, far), 780);
    assert_eq!(c.next_work_time(1020, far), at(1, 8, 0));
    // Friday evening goes to Monday morning.
    assert_eq!(c.next_work_time(at(4, 18, 0), far), at(7, 8, 0));
    // A search bound caps the answer.
    assert_eq!(c.next_work_time(720, 750), 750);
    assert_eq!(c.next_work_time(1020, DAY + 100), DAY + 100);
    // Round-the-clock work is always on.
    let all = WorkCalendar::continuous(45000);
    assert_eq!(all.next_work_time(5000, far), 5000);
    assert_eq!(all.next_work_time(5000, 4000), 4000);
}

/// Rule CAL-3 in `engine/RULES.md`.
#[test]
fn the_next_non_working_moment_is_the_close_of_the_shift_worked_in() {
    let c = split();
    assert_eq!(c.next_non_work_time(500, FAR), 720);
    assert_eq!(c.next_non_work_time(720, FAR), 720);
    assert_eq!(c.next_non_work_time(1100, FAR), 1100);
    assert_eq!(c.next_non_work_time(500, 600), 600);
    assert_eq!(
        WorkCalendar::continuous(45000).next_non_work_time(5000, 6000),
        5000
    );
}

/// Rule CAL-4 in `engine/RULES.md`.
#[test]
fn the_previous_working_moment_is_the_latest_shift_close_not_after_the_minute() {
    let c = split();
    assert_eq!(c.prev_work_time(800, -FAR), 720);
    assert_eq!(c.prev_work_time(1020, -FAR), 1020);
    // Before the first shift, the previous working day's last close.
    assert_eq!(c.prev_work_time(DAY + 400, -FAR), 1020);
    // Monday morning looks back over the weekend to Friday's close.
    assert_eq!(c.prev_work_time(400, -FAR), -3 * DAY + 1020);
    // The bound is a floor.
    assert_eq!(c.prev_work_time(400, -2 * DAY), -2 * DAY);
    assert_eq!(
        WorkCalendar::continuous(45000).prev_work_time(5000, 4000),
        5000
    );
}

/// Rule CAL-5 in `engine/RULES.md`.
#[test]
fn the_previous_non_working_moment_is_the_start_of_the_shift_containing_the_minute() {
    let c = split();
    assert_eq!(c.prev_non_work_time(600, 0), 480);
    // A shift's closing minute belongs to that shift, its opening minute too.
    assert_eq!(c.prev_non_work_time(720, 0), 480);
    assert_eq!(c.prev_non_work_time(780, 0), 780);
    assert_eq!(c.prev_non_work_time(1020, 0), 780);
    // Outside any shift the minute itself.
    assert_eq!(c.prev_non_work_time(750, 0), 750);
    // Round-the-clock work answers with the earlier of the minute and bound.
    let all = WorkCalendar::continuous(45000);
    assert_eq!(all.prev_non_work_time(5000, 4000), 4000);
    assert_eq!(all.prev_non_work_time(5000, 6000), 5000);
}

/// Rule CAL-6 in `engine/RULES.md`.
#[test]
fn duration_counts_working_minutes_and_is_negative_when_reversed() {
    let c = split();
    assert_eq!(c.duration(0, 2 * DAY), 960);
    assert_eq!(c.duration(2 * DAY, 0), -960);
    assert_eq!(c.duration(500, 1100), 460);
    assert_eq!(c.duration(4 * DAY, 7 * DAY), 480);
    assert_eq!(c.duration(at(5, 1, 40), at(5, 15, 0)), 0);
    let all = WorkCalendar::continuous(45000);
    assert_eq!(all.duration(100, 5000), 4900);
    assert_eq!(all.duration(5000, 100), -4900);
}

/// Rule CAL-7 in `engine/RULES.md`.
#[test]
fn a_finish_that_fills_a_shift_stays_at_its_close() {
    let c = split();
    assert_eq!(c.get_finish_date(480, 240), 720);
    assert_eq!(c.get_finish_date(480, 480), 1020);
    assert_eq!(c.get_finish_date(480, 481), DAY + 481);
    assert_eq!(c.get_finish_date(480, 960), DAY + 1020);
    // Friday work that spills over goes to Monday, not Saturday.
    assert_eq!(c.get_finish_date(at(4, 8, 0), 481), at(7, 8, 1));
    assert_eq!(
        WorkCalendar::continuous(45000).get_finish_date(100, 50),
        150
    );
}

/// Rule CAL-8 in `engine/RULES.md`.
#[test]
fn a_start_found_by_counting_back_lands_inside_a_shift() {
    let c = split();
    assert_eq!(c.get_start_date(1020, 240), 780);
    assert_eq!(c.get_start_date(1020, 480), 480);
    assert_eq!(c.get_start_date(780, 240), 480);
    assert_eq!(c.get_start_date(DAY + 480, 60), 960);
    // Counting back past the first moment of work stops at the first start.
    assert_eq!(c.get_start_date(500, 1000), 480);
    assert_eq!(WorkCalendar::continuous(45000).get_start_date(150, 50), 100);
}

/// Rule CAL-9 in `engine/RULES.md`.
#[test]
fn adding_zero_work_snaps_to_a_start_or_a_finish_moment() {
    let c = CalendarAdapter::new(split());
    // A start moment at a shift's close is the next shift's opening.
    assert_eq!(c.add(720, 0), 780);
    assert_eq!(c.snap_start(720), 780);
    // A finish moment at a shift's opening is the previous shift's close.
    assert_eq!(c.sub(780, 0), 720);
    assert_eq!(c.snap_finish(780), 720);
    assert_eq!(c.add(480, 480), 1020);
    assert_eq!(c.add(1020, -480), 480);
    assert_eq!(c.sub(480, -480), 1020);
    assert_eq!(c.add(1020, 1), DAY + 481);
    assert_eq!(c.sub(DAY + 480, 1), 1019);
    assert_eq!(c.between(480, DAY + 480), 480);
    assert_eq!(c.between(DAY + 480, 480), -480);
}

// ---------------------------------------------------------------------------
// Relationship types and lags
// ---------------------------------------------------------------------------

/// Rule REL-2 in `engine/RULES.md`.
#[test]
fn the_lag_calendar_option_chooses_the_calendar_that_counts_the_lag() {
    // A works the 5x10 calendar, B the lunch calendar, the project the short
    // one. Lag of 1440 minutes after A finishes Monday 17:00.
    let expected = [
        (LagCalendar::PredCal, "5x10", at(3, 11, 0)),
        (LagCalendar::SuccCal, "lunch", at(3, 10, 0)),
        (LagCalendar::Hour24, "24x7", at(1, 17, 0)),
        (LagCalendar::ProjCal, "short", at(3, 16, 0)),
    ];
    for (option, id, start) in expected {
        let mut b = task(2, "B", 600);
        b.cal = 1;
        let mut m = model(
            vec![task(1, "A", 600), b],
            vec![tie(0, 1, LinkType::FS, 1440)],
            ScheduleOptions {
                lag_calendar: option,
                ..plain()
            },
        );
        assert_eq!(Scheduler::new(&mut m).lag_calendar(0).id, id);
        run(&mut m);
        assert_eq!(m.activities[1].early_start, Some(start), "{option:?}");
    }
}

/// Rule REL-3 in `engine/RULES.md`.
#[test]
fn a_finish_to_start_lag_is_working_time_after_the_predecessor_finishes() {
    let build = |lag| {
        let mut m = model(
            vec![task(1, "A", 600), task(2, "B", 600)],
            vec![tie(0, 1, LinkType::FS, lag)],
            plain(),
        );
        run(&mut m);
        m
    };
    // One working day (480 minutes of the 600 in a day) after Monday 17:00.
    let m = build(480);
    assert_eq!(
        early(&m.activities[1]),
        (Some(at(1, 15, 0)), Some(at(2, 15, 0)))
    );
    // A negative lag starts the successor inside the predecessor.
    let m = build(-480);
    assert_eq!(
        early(&m.activities[1]),
        (Some(at(0, 9, 0)), Some(at(1, 9, 0)))
    );
    // No lag: the successor starts when the predecessor finishes, which on a
    // working calendar is the next morning.
    let m = build(0);
    assert_eq!(
        early(&m.activities[1]),
        (Some(at(1, 7, 0)), Some(at(1, 17, 0)))
    );
}

/// Rule REL-4 in `engine/RULES.md`.
#[test]
fn a_started_predecessors_start_lag_is_used_up_by_its_elapsed_work() {
    // A started Monday 07:00 and has one more day; the data date is Tuesday
    // 07:00. B follows A's start by two working days (1200 minutes); one day
    // has already been worked, so one day is left after the data date.
    for options in [plain(), retained()] {
        let mut a = task(1, "A", 600);
        a.status = StatusType::InProgress;
        a.act_start = Some(at(0, 7, 0));
        let mut m = model(
            vec![a, task(2, "B", 600)],
            vec![tie(0, 1, LinkType::SS, 1200)],
            options,
        );
        m.project.data_date = Some(at(1, 7, 0));
        run(&mut m);
        assert_eq!(m.activities[1].early_start, Some(at(2, 7, 0)));
    }
}

/// Rule REL-13 in `engine/RULES.md`.
#[test]
fn under_progress_override_a_zero_lag_start_tie_from_started_work_follows_the_data_date() {
    // A started Monday and is suspended until Wednesday; the data date is
    // Tuesday 07:00. B starts with A (SS, no lag).
    let build = |options| {
        let mut m = pair(LinkType::SS, 0, options);
        let a = &mut m.activities[0];
        a.status = StatusType::InProgress;
        a.act_start = Some(at(0, 7, 0));
        a.restart = Some(at(2, 7, 0));
        a.reend = Some(at(2, 17, 0));
        m.project.data_date = Some(at(1, 7, 0));
        m
    };
    let mut m = build(plain());
    assert_eq!(
        Scheduler::new(&mut m).apply_lag_forward(0),
        Some(at(2, 7, 0))
    );
    let mut m = build(ScheduleOptions {
        progress_override: true,
        ..plain()
    });
    assert_eq!(
        Scheduler::new(&mut m).apply_lag_forward(0),
        Some(at(1, 7, 0))
    );
    // A finish tie is unaffected.
    let mut m = build(ScheduleOptions {
        progress_override: true,
        ..plain()
    });
    m.links[0].link_type = LinkType::FS;
    assert_eq!(
        Scheduler::new(&mut m).apply_lag_forward(0),
        Some(at(2, 17, 0))
    );
}

/// Rule REL-5 in `engine/RULES.md`.
#[test]
fn a_completed_predecessors_finish_lag_is_used_up_by_the_time_since_it_finished() {
    // A finished Monday 17:00; the data date is Wednesday 07:00 and the tie has
    // a one-day lag (600 minutes). One working day has already passed, so B
    // may start at the data date.
    let mut a = task(1, "A", 0);
    a.status = StatusType::Completed;
    a.act_start = Some(at(0, 7, 0));
    a.act_finish = Some(at(0, 17, 0));
    let mut m = model(
        vec![a, task(2, "B", 600)],
        vec![tie(0, 1, LinkType::FS, 600)],
        retained(),
    );
    m.project.data_date = Some(at(2, 7, 0));
    run(&mut m);
    assert_eq!(m.activities[1].early_start, Some(at(2, 7, 0)));
}

/// Rule REL-6 in `engine/RULES.md`.
#[test]
fn backward_lag_counts_in_full_except_a_started_start_ties_used_up_part() {
    // A started Monday 07:00, data date Tuesday 07:00: 600 minutes elapsed.
    let build = |options, kind, lag| {
        let mut a = task(1, "A", 600);
        a.status = StatusType::InProgress;
        a.act_start = Some(at(0, 7, 0));
        let mut m = model(
            vec![a, task(2, "B", 600)],
            vec![tie(0, 1, kind, lag)],
            options,
        );
        m.project.data_date = Some(at(1, 7, 0));
        m
    };
    let friday = at(4, 7, 0);
    // Retained logic and Progress Override: the used-up day is not counted.
    for options in [
        retained(),
        ScheduleOptions {
            progress_override: true,
            ..plain()
        },
    ] {
        let mut m = build(options, LinkType::SS, 1200);
        assert_eq!(Scheduler::new(&mut m).apply_lag(0, friday), at(3, 7, 0));
    }
    // Actual Dates: every lag is counted back in full.
    let mut m = build(plain(), LinkType::SS, 1200);
    assert_eq!(Scheduler::new(&mut m).apply_lag(0, friday), at(2, 7, 0));
    // A lag used up entirely leaves the date as it is.
    let mut m = build(retained(), LinkType::SS, 600);
    assert_eq!(Scheduler::new(&mut m).apply_lag(0, friday), friday);
    // Finish-to-start lags are never used up.
    let mut m = build(retained(), LinkType::FS, 600);
    assert_eq!(Scheduler::new(&mut m).apply_lag(0, friday), at(3, 7, 0));
    // A negative lag moves the date later, onto a start moment.
    let mut m = build(plain(), LinkType::FS, -600);
    assert_eq!(
        Scheduler::new(&mut m).apply_lag(0, at(0, 7, 0)),
        at(1, 7, 0)
    );
}

// ---------------------------------------------------------------------------
// Forward candidates from relationships
// ---------------------------------------------------------------------------

fn pair(kind: LinkType, lag: i64, options: ScheduleOptions) -> Model {
    model(
        vec![task(1, "A", 600), task(2, "B", 600)],
        vec![tie(0, 1, kind, lag)],
        options,
    )
}

/// Rule REL-7 in `engine/RULES.md`.
#[test]
fn level_of_effort_work_never_drives_its_successors() {
    let mut m = pair(LinkType::FS, 0, plain());
    m.activities[0].task_type = TaskType::Loe;
    m.activities[0].restart = Some(at(3, 7, 0));
    m.activities[0].reend = Some(at(4, 7, 0));
    let s = Scheduler::new(&mut m);
    assert_eq!(s.get_succ_restart_date(1), (None, LinkType::None, None));
}

/// Rule REL-8 in `engine/RULES.md`.
#[test]
fn progress_override_ignores_start_side_ties_into_started_work_only() {
    let build = |options, kind| {
        let mut m = pair(kind, 0, options);
        m.activities[0].restart = Some(at(1, 7, 0));
        m.activities[0].reend = Some(at(2, 7, 0));
        m.activities[1].status = StatusType::InProgress;
        m.activities[1].act_start = Some(at(0, 7, 0));
        m
    };
    let over = ScheduleOptions {
        progress_override: true,
        ..plain()
    };
    // Actual Dates: the started successor still takes the tie's date.
    let mut m = build(plain(), LinkType::FS);
    assert_eq!(
        Scheduler::new(&mut m).get_succ_restart_date(1).0,
        Some(at(2, 7, 0))
    );
    // Progress Override: its progress replaces FS and SS ties...
    for kind in [LinkType::FS, LinkType::SS] {
        let mut m = build(over.clone(), kind);
        assert_eq!(
            Scheduler::new(&mut m).get_succ_restart_date(1),
            (None, LinkType::None, None)
        );
    }
    // ...but a finish-side tie still holds.
    let mut m = build(over, LinkType::FF);
    let (restart, dep, reend) = Scheduler::new(&mut m).get_succ_restart_date(1);
    assert_eq!((dep, reend), (LinkType::FF, Some(at(2, 7, 0))));
    assert!(restart.is_some());
}

/// Rule REL-9 in `engine/RULES.md`.
#[test]
fn a_completed_predecessor_still_drives_from_its_actual_dates_without_retained_logic() {
    let build = |options, kind, lag| {
        let mut m = pair(kind, lag, options);
        let a = &mut m.activities[0];
        a.status = StatusType::Completed;
        a.act_start = Some(at(0, 7, 0));
        a.act_finish = Some(at(0, 17, 0));
        a.restart = Some(at(3, 7, 0));
        a.reend = Some(at(3, 17, 0));
        m.project.data_date = Some(at(0, 7, 0));
        m
    };
    let candidate = |m: &mut Model| Scheduler::new(m).get_succ_restart_date(1).0;
    // From the actual finish (finish ties) or the actual start (start ties),
    // plus the whole lag.
    assert_eq!(
        candidate(&mut build(plain(), LinkType::FS, 0)),
        Some(at(0, 17, 0))
    );
    assert_eq!(
        candidate(&mut build(plain(), LinkType::SS, 0)),
        Some(at(0, 7, 0))
    );
    assert_eq!(
        candidate(&mut build(plain(), LinkType::FS, 480)),
        Some(at(1, 15, 0))
    );
    // Retained logic uses the stored remaining dates instead.
    assert_eq!(
        candidate(&mut build(retained(), LinkType::FS, 0)),
        Some(at(3, 17, 0))
    );
}

/// Rule REL-10 in `engine/RULES.md`.
#[test]
fn expected_finish_discards_finish_side_ties() {
    let build = |use_expected| {
        let mut m = pair(
            LinkType::FF,
            0,
            ScheduleOptions {
                use_expected_finish: use_expected,
                ..plain()
            },
        );
        m.activities[0].restart = Some(at(0, 7, 0));
        m.activities[0].reend = Some(at(2, 17, 0));
        m.activities[1].expect_end = Some(at(1, 17, 0));
        m
    };
    let mut m = build(false);
    let (restart, dep, reend) = Scheduler::new(&mut m).get_succ_restart_date(1);
    assert_eq!((dep, reend), (LinkType::FF, Some(at(2, 17, 0))));
    assert_eq!(restart, Some(at(2, 7, 0)));
    let mut m = build(true);
    assert_eq!(
        Scheduler::new(&mut m).get_succ_restart_date(1),
        (None, LinkType::None, None)
    );
}

/// Rule REL-11 in `engine/RULES.md`.
#[test]
fn a_finish_tie_sets_the_successors_finish_and_its_start_follows_from_the_duration() {
    // A works the whole week; B (one day) must finish with A.
    let mut m = model(
        vec![task(1, "A", 3000), task(2, "B", 600)],
        vec![tie(0, 1, LinkType::FF, 0)],
        plain(),
    );
    run(&mut m);
    assert_eq!(
        early(&m.activities[1]),
        (Some(at(4, 7, 0)), Some(at(4, 17, 0)))
    );
    // Ten hours of lag push the finish into the next week.
    let mut m = model(
        vec![task(1, "A", 3000), task(2, "B", 600)],
        vec![tie(0, 1, LinkType::FF, 600)],
        plain(),
    );
    run(&mut m);
    assert_eq!(
        early(&m.activities[1]),
        (Some(at(7, 7, 0)), Some(at(7, 17, 0)))
    );
}

/// Rule REL-12 in `engine/RULES.md`.
#[test]
fn an_activity_starts_at_the_latest_of_its_start_and_finish_driven_candidates() {
    // B is held by P1 (finish to start, one day) and Q (finish to finish,
    // a week, so B's one day ends Friday).
    let mut m = model(
        vec![task(1, "P1", 600), task(2, "Q", 3000), task(3, "B", 600)],
        vec![tie(0, 2, LinkType::FS, 0), tie(1, 2, LinkType::FF, 0)],
        plain(),
    );
    run(&mut m);
    assert_eq!(m.activities[2].early_start, Some(at(4, 7, 0)));
    // With a longer first predecessor the start-driven candidate wins.
    let mut m = model(
        vec![task(1, "P1", 4200), task(2, "Q", 3000), task(3, "B", 600)],
        vec![tie(0, 2, LinkType::FS, 0), tie(1, 2, LinkType::FF, 0)],
        plain(),
    );
    run(&mut m);
    assert_eq!(m.activities[2].early_start, Some(at(9, 7, 0)));
}

// ---------------------------------------------------------------------------
// Milestone retyping, boundary rules, resource lag
// ---------------------------------------------------------------------------

/// Rule EDGE-1 in `engine/RULES.md`.
#[test]
fn a_finish_keeps_its_working_boundary_and_a_start_after_a_non_work_minute_moves_on() {
    let mut b = task(2, "B", 600);
    b.cal = 1;
    let mut m = model(vec![task(1, "A", 600), b], vec![], plain());
    let s = Scheduler::new(&mut m);
    // With a lunch break (07:00-12:00, 13:00-18:00), 13:00 is working time as
    // a late start but not a finish moment.
    assert_eq!(s.edge_adjust_late_restart(1, at(0, 13, 0)), at(0, 13, 0));
    assert_eq!(s.edge_adjust_late_reend(1, at(0, 13, 0)), at(0, 12, 0));
    // Early finish moment: kept when the minute before it is working time.
    assert_eq!(s.edge_adjust_early_reend(0, at(0, 17, 0)), at(0, 17, 0));
    assert_eq!(s.edge_adjust_early_reend(0, at(0, 12, 0)), at(0, 12, 0));
    assert_eq!(s.edge_adjust_early_reend(0, at(0, 18, 0)), at(1, 7, 0));
    assert_eq!(s.edge_adjust_early_reend(0, at(0, 7, 0)), at(0, 7, 0));
    assert_eq!(s.edge_adjust_early_reend(0, at(5, 12, 0)), at(7, 7, 0));
    // Early start moment: the next working start.
    assert_eq!(s.edge_adjust_early_restart(0, at(0, 17, 0)), at(1, 7, 0));
    assert_eq!(s.edge_adjust_early_restart(0, at(0, 9, 0)), at(0, 9, 0));
    // Late start moment: kept when it is working time, else the previous finish.
    assert_eq!(s.edge_adjust_late_restart(0, at(0, 7, 0)), at(0, 7, 0));
    assert_eq!(s.edge_adjust_late_restart(0, at(5, 12, 0)), at(4, 17, 0));
    assert_eq!(s.edge_adjust_late_restart(0, at(1, 5, 0)), at(0, 17, 0));
    // Late finish moment: the previous working finish.
    assert_eq!(s.edge_adjust_late_reend(0, at(5, 12, 0)), at(4, 17, 0));
    assert_eq!(s.edge_adjust_late_reend(0, at(0, 17, 0)), at(0, 17, 0));
    assert_eq!(s.edge_adjust_late_reend(0, at(0, 12, 0)), at(0, 12, 0));
}

/// Rule EDGE-2 in `engine/RULES.md`.
#[test]
fn a_milestone_takes_the_date_of_the_end_it_has_when_the_boundary_rules_apply() {
    let mut m = model(
        vec![task(1, "S", 0), task(2, "F", 0), task(3, "M", 0)],
        vec![],
        plain(),
    );
    m.activities[0].task_type = TaskType::Mile;
    m.activities[1].task_type = TaskType::FinMile;
    m.activities[2].task_type = TaskType::Mile;
    m.activities[2].cal = 1;
    let s = Scheduler::new(&mut m);
    // On a calendar with a lunch break a start milestone may sit at 13:00.
    assert_eq!(s.edge_adjust_late_reend(2, at(0, 13, 0)), at(0, 13, 0));
    // A start milestone's early date is a start moment, its late date a
    // start that must be working time; a finish milestone is the reverse.
    assert_eq!(s.edge_adjust_early_restart(0, at(0, 17, 0)), at(1, 7, 0));
    assert_eq!(s.edge_adjust_early_restart(1, at(0, 17, 0)), at(0, 17, 0));
    assert_eq!(s.edge_adjust_late_reend(0, at(0, 17, 0)), at(0, 17, 0));
    assert_eq!(s.edge_adjust_late_reend(0, at(5, 12, 0)), at(4, 17, 0));
    assert_eq!(s.edge_adjust_late_reend(1, at(5, 12, 0)), at(4, 17, 0));
}

/// Rule FWD-1 in `engine/RULES.md`.
#[test]
fn zero_duration_work_starts_at_the_next_working_start_but_a_finish_milestone_stays() {
    // Data date Friday 17:00, the close of the day.
    let dd = at(4, 17, 0);
    for kind in [TaskType::Task, TaskType::Mile] {
        let mut a = task(1, "Z", 0);
        a.task_type = kind;
        let mut m = model(vec![a], vec![], plain());
        m.project.data_date = Some(dd);
        Scheduler::new(&mut m).schedule_forward_task(0);
        let a = &m.activities[0];
        assert_eq!(
            (a.restart, a.reend),
            (Some(at(7, 7, 0)), Some(at(7, 7, 0))),
            "{kind:?}"
        );
    }
    let mut a = task(1, "Z", 0);
    a.task_type = TaskType::FinMile;
    let mut m = model(vec![a], vec![], plain());
    m.project.data_date = Some(dd);
    Scheduler::new(&mut m).schedule_forward_task(0);
    let a = &m.activities[0];
    assert_eq!((a.restart, a.reend), (Some(dd), Some(dd)));
    // A completed node keeps the raw date even outside working time.
    let mut a = task(1, "Z", 0);
    a.status = StatusType::Completed;
    a.act_start = Some(0);
    a.act_finish = Some(1);
    let mut m = model(vec![a], vec![], retained());
    m.project.data_date = Some(dd);
    Scheduler::new(&mut m).schedule_forward_task(0);
    let a = &m.activities[0];
    assert_eq!((a.restart, a.reend), (Some(dd), Some(dd)));
}

/// Rule FWD-2 in `engine/RULES.md`.
#[test]
fn work_never_starts_before_the_data_date_or_its_resume_date_or_a_start_imposed_from_another_project(
) {
    // Not started: the data date is the earliest start.
    let mut m = model(vec![task(1, "A", 600)], vec![], plain());
    m.project.data_date = Some(at(2, 7, 0));
    run(&mut m);
    assert_eq!(
        early(&m.activities[0]),
        (Some(at(2, 7, 0)), Some(at(2, 17, 0)))
    );
    // Suspended work resumes at its resume date.
    let mut a = task(1, "A", 600);
    a.status = StatusType::InProgress;
    a.act_start = Some(at(0, 7, 0));
    a.resume = Some(at(2, 7, 0));
    let mut m = model(vec![a], vec![], retained());
    Scheduler::new(&mut m).schedule_forward_task(0);
    assert_eq!(m.activities[0].restart, Some(at(2, 7, 0)));
    // An early start imposed by another project holds unless relationships to
    // and from other projects are ignored.
    for (sequence, want) in [
        (OutOfSequence::None, at(0, 7, 0)),
        (OutOfSequence::Both, at(3, 7, 0)),
    ] {
        let mut a = task(1, "A", 600);
        a.ext_early_start = Some(at(3, 7, 0));
        let mut m = model(
            vec![a],
            vec![],
            ScheduleOptions {
                out_of_sequence: sequence,
                ..plain()
            },
        );
        Scheduler::new(&mut m).schedule_forward_task(0);
        assert_eq!(m.activities[0].restart, Some(want), "{sequence:?}");
    }
}

/// Rule BWD-4 in `engine/RULES.md`.
#[test]
fn a_late_finish_imposed_by_another_project_holds_unless_external_relationships_are_ignored() {
    for (sequence, want) in [
        (OutOfSequence::None, at(11, 17, 0)),
        (OutOfSequence::Both, at(2, 17, 0)),
    ] {
        let mut a = task(1, "A", 600);
        a.ext_late_end = Some(at(2, 17, 0));
        let mut m = model(
            vec![a],
            vec![],
            ScheduleOptions {
                out_of_sequence: sequence,
                ..plain()
            },
        );
        m.project.plan_end = Some(at(11, 17, 0));
        Scheduler::new(&mut m).schedule_backward_task(0);
        assert_eq!(m.activities[0].reend, Some(want), "{sequence:?}");
    }
}

/// Rule FWD-3 in `engine/RULES.md`.
#[test]
fn an_actual_start_after_the_data_date_holds_the_start_in_actual_dates_mode_only() {
    let build = |options| {
        let mut a = task(1, "A", 600);
        a.act_start = Some(at(1, 7, 0));
        model(vec![a], vec![], options)
    };
    let mut m = build(plain());
    Scheduler::new(&mut m).schedule_forward_task(0);
    assert_eq!(m.activities[0].restart, Some(at(1, 7, 0)));
    let mut m = build(retained());
    Scheduler::new(&mut m).schedule_forward_task(0);
    assert_eq!(m.activities[0].restart, Some(at(0, 7, 0)));
}

/// Rule FWD-4 in `engine/RULES.md`.
#[test]
fn a_finish_in_a_gap_is_extended_to_the_driving_finish_ties_limit() {
    let build = |dep| {
        let mut a = task(1, "A", 600);
        a.reend = Some(at(0, 17, 0));
        (model(vec![a], vec![], plain()), dep)
    };
    // No working time between the finish and the limit: the finish is the limit.
    let (mut m, dep) = build(LinkType::FF);
    Scheduler::new(&mut m).adjust_for_non_work_finish_time_from_dependency(
        0,
        dep,
        Some(at(1, 5, 0)),
    );
    assert_eq!(m.activities[0].reend, Some(at(1, 5, 0)));
    // Working time in between: unchanged.
    let (mut m, dep) = build(LinkType::SF);
    Scheduler::new(&mut m).adjust_for_non_work_finish_time_from_dependency(
        0,
        dep,
        Some(at(1, 8, 0)),
    );
    assert_eq!(m.activities[0].reend, Some(at(0, 17, 0)));
    // Only finish-side ties extend a finish.
    let (mut m, _) = build(LinkType::FS);
    Scheduler::new(&mut m).adjust_for_non_work_finish_time_from_dependency(
        0,
        LinkType::FS,
        Some(at(1, 5, 0)),
    );
    assert_eq!(m.activities[0].reend, Some(at(0, 17, 0)));
    // A finish already at or past the limit stays.
    let (mut m, dep) = build(LinkType::FF);
    Scheduler::new(&mut m).adjust_for_non_work_finish_time_from_dependency(
        0,
        dep,
        Some(at(0, 12, 0)),
    );
    assert_eq!(m.activities[0].reend, Some(at(0, 17, 0)));
}

// ---------------------------------------------------------------------------
// Backward readers
// ---------------------------------------------------------------------------

/// Rule BWD-1 in `engine/RULES.md`.
#[test]
fn a_successors_late_dates_limit_its_predecessor_and_a_completed_predecessor_ignores_the_lag() {
    let build = |completed: bool| {
        let mut a = task(1, "A", 600);
        let mut b = task(2, "B", 600);
        b.restart = Some(at(2, 7, 0));
        b.reend = Some(at(2, 17, 0));
        if completed {
            a.status = StatusType::Completed;
            a.act_start = Some(at(0, 7, 0));
            a.act_finish = Some(at(0, 17, 0));
        }
        model(vec![a, b], vec![tie(0, 1, LinkType::FS, 480)], retained())
    };
    // An open predecessor must finish one working day (480 minutes) before B's
    // late start.
    let mut m = build(false);
    assert_eq!(
        Scheduler::new(&mut m).get_pred_reend_date(0).0,
        Some(at(1, 9, 0))
    );
    // A completed one whose lag is used up (it finished Monday 17:00, the
    // data date is Wednesday 07:00: 600 working minutes since) is limited by
    // B's late start itself; an unused lag would count back (BWD-6).
    let mut m = build(true);
    m.project.data_date = Some(at(2, 7, 0));
    assert_eq!(
        Scheduler::new(&mut m).get_pred_reend_date(0).0,
        Some(at(2, 7, 0))
    );
}

/// Rule BWD-2 in `engine/RULES.md`.
#[test]
fn the_limit_a_tie_gives_depends_on_its_type_and_ignores_summaries_and_finished_successors() {
    let build = |kind| {
        let mut b = task(2, "B", 600);
        b.restart = Some(at(2, 7, 0));
        b.reend = Some(at(2, 17, 0));
        model(
            vec![task(1, "A", 600), b],
            vec![tie(0, 1, kind, 0)],
            plain(),
        )
    };
    let read = |m: &mut Model, kind, check| Scheduler::new(m).rel_pred_reend_date(0, kind, check);
    // FF and FS limit a finish; SS and SF limit a start.
    let mut m = build(LinkType::FF);
    assert_eq!(read(&mut m, LinkType::FF, true), (None, Some(at(2, 17, 0))));
    let mut m = build(LinkType::FS);
    assert_eq!(read(&mut m, LinkType::FS, true), (None, Some(at(2, 7, 0))));
    let mut m = build(LinkType::SS);
    assert_eq!(read(&mut m, LinkType::SS, true), (Some(at(2, 7, 0)), None));
    let mut m = build(LinkType::SF);
    assert_eq!(read(&mut m, LinkType::SF, true), (Some(at(2, 17, 0)), None));
    // A level-of-effort successor is ignored when asked to check, a summary always.
    let mut m = build(LinkType::FS);
    m.activities[1].task_type = TaskType::Loe;
    assert_eq!(read(&mut m, LinkType::FS, true), (None, None));
    assert_eq!(read(&mut m, LinkType::FS, false), (None, Some(at(2, 7, 0))));
    m.activities[1].task_type = TaskType::Wbs;
    assert_eq!(read(&mut m, LinkType::FS, false), (None, None));
    // A completed successor is ignored unless logic is retained.
    let mut m = build(LinkType::FS);
    m.activities[1].status = StatusType::Completed;
    assert_eq!(read(&mut m, LinkType::FS, true), (None, None));
    m.options.retained_logic = true;
    assert_eq!(read(&mut m, LinkType::FS, true), (None, Some(at(2, 7, 0))));
    // Progress Override ignores the successor's dates altogether.
    let mut m = build(LinkType::FS);
    m.options.progress_override = true;
    assert_eq!(read(&mut m, LinkType::FS, true), (None, None));
}

/// Rule BWD-3 in `engine/RULES.md`.
#[test]
fn a_started_successors_actual_start_stands_in_for_its_remaining_start() {
    let build = |options| {
        let mut a = task(1, "A", 600);
        a.status = StatusType::InProgress;
        a.act_start = Some(at(0, 7, 0));
        a.act_finish = Some(at(0, 12, 0));
        a.restart = Some(at(1, 7, 0));
        a.reend = Some(at(1, 17, 0));
        model(vec![a], vec![], options)
    };
    let mut m = build(plain());
    let s = Scheduler::new(&mut m);
    assert_eq!(s.get_task_start_date(0), Some(at(0, 7, 0)));
    assert_eq!(s.get_task_end_date(0), Some(at(0, 12, 0)));
    let mut m = build(ScheduleOptions {
        progress_override: true,
        ..plain()
    });
    let s = Scheduler::new(&mut m);
    assert_eq!(
        (s.get_task_start_date(0), s.get_task_end_date(0)),
        (None, None)
    );
    let mut m = build(retained());
    let s = Scheduler::new(&mut m);
    assert_eq!(s.get_task_start_date(0), Some(at(1, 7, 0)));
    assert_eq!(s.get_task_end_date(0), Some(at(1, 17, 0)));
}

/// Rule RES-1 in `engine/RULES.md`.
#[test]
fn resource_driven_dates_use_the_longest_or_earliest_driving_assignment() {
    let assign = |rollup, rem, remain_lag, relag| Assignment {
        rsrc_id: 1,
        rollup,
        rem,
        remain_lag,
        relag,
        rollup_present: true,
        ..Assignment::default()
    };
    let mut a = task(1, "A", 100);
    a.assignments = vec![
        assign(true, 300, 0, 0),
        assign(true, 600, 300, 300),
        assign(false, 6000, 0, 0),
    ];
    let mut m = model(vec![a], vec![], plain());
    let s = Scheduler::new(&mut m);
    // The finish is the latest of the driving assignments' finishes.
    assert_eq!(s.get_rsrc_reend_date(0, at(0, 7, 0)), at(1, 12, 0));
    // The start is the earliest of the driving assignments' starts.
    assert_eq!(s.get_rsrc_restart_date(0, at(2, 17, 0)), at(1, 12, 0));
    // Without driving assignments the activity's own remaining duration counts.
    let mut m = model(vec![task(1, "A", 100)], vec![], plain());
    let s = Scheduler::new(&mut m);
    assert_eq!(s.get_rsrc_reend_date(0, at(0, 7, 0)), at(0, 8, 40));
    assert_eq!(s.get_rsrc_restart_date(0, at(0, 17, 0)), at(0, 15, 20));
    // With no remaining duration the date is unchanged.
    let mut m = model(vec![task(1, "A", 0)], vec![], plain());
    let s = Scheduler::new(&mut m);
    assert_eq!(s.get_rsrc_reend_date(0, at(0, 7, 0)), at(0, 7, 0));
    assert_eq!(s.get_rsrc_restart_date(0, at(0, 17, 0)), at(0, 17, 0));
}

// ---------------------------------------------------------------------------
// Constraints
// ---------------------------------------------------------------------------

fn constrained(kind: CstrType, date: i64, rem: i64) -> Model {
    let mut a = task(1, "A", rem);
    a.cstr = kind;
    a.cstr_date = Some(date);
    model(vec![a], vec![], plain())
}

/// Rule CON-1 in `engine/RULES.md`.
#[test]
fn start_constraints_hold_the_early_start_and_finish_constraints_hold_the_early_finish() {
    // Start On / Start On or After: the early start is the constraint date.
    for kind in [CstrType::StartOn, CstrType::StartOnAfter] {
        let mut m = constrained(kind, at(2, 7, 0), 600);
        run(&mut m);
        assert_eq!(
            early(&m.activities[0]),
            (Some(at(2, 7, 0)), Some(at(2, 17, 0)))
        );
    }
    // Finish On / Finish On or After: the early finish is the constraint date.
    for kind in [CstrType::FinishOn, CstrType::FinishOnAfter] {
        let mut m = constrained(kind, at(2, 17, 0), 600);
        run(&mut m);
        assert_eq!(
            early(&m.activities[0]),
            (Some(at(2, 7, 0)), Some(at(2, 17, 0)))
        );
    }
    // "On or Before" constraints and As Late As Possible do not move the
    // early dates.
    for kind in [
        CstrType::StartOnBefore,
        CstrType::FinishOnBefore,
        CstrType::Alap,
    ] {
        let mut m = constrained(kind, at(2, 17, 0), 600);
        run(&mut m);
        assert_eq!(
            early(&m.activities[0]),
            (Some(at(0, 7, 0)), Some(at(0, 17, 0)))
        );
    }
}

/// Rule CON-2 in `engine/RULES.md`.
#[test]
fn before_constraints_cap_the_late_dates_and_after_constraints_do_not() {
    let late_of = |kind, date| {
        let mut m = constrained(kind, date, 600);
        m.project.plan_end = Some(at(11, 17, 0));
        run(&mut m);
        late(&m.activities[0])
    };
    assert_eq!(
        late_of(CstrType::StartOnBefore, at(2, 7, 0)),
        (Some(at(2, 7, 0)), Some(at(2, 17, 0)))
    );
    assert_eq!(
        late_of(CstrType::FinishOnBefore, at(2, 17, 0)),
        (Some(at(2, 7, 0)), Some(at(2, 17, 0)))
    );
    assert_eq!(
        late_of(CstrType::StartOn, at(2, 7, 0)),
        (Some(at(2, 7, 0)), Some(at(2, 17, 0)))
    );
    // Start or Finish On or After leave the late dates at the project limit.
    for kind in [CstrType::StartOnAfter, CstrType::FinishOnAfter] {
        assert_eq!(
            late_of(kind, at(2, 7, 0)),
            (Some(at(11, 7, 0)), Some(at(11, 17, 0)))
        );
    }
}

/// Rule CON-2, BWD-5 in `engine/RULES.md`.
#[test]
fn a_late_finish_before_the_data_date_gives_negative_float() {
    let mut m = constrained(CstrType::FinishOnBefore, at(0, 17, 0), 600);
    m.project.data_date = Some(at(2, 7, 0));
    m.project.plan_end = Some(at(11, 17, 0));
    run(&mut m);
    let a = &m.activities[0];
    assert_eq!(early(a), (Some(at(2, 7, 0)), Some(at(2, 17, 0))));
    assert_eq!(late(a), (Some(at(0, 7, 0)), Some(at(0, 17, 0))));
    assert_eq!(a.total_float, Some(-1200));
}

/// Rule BWD-5, CAL-10 in `engine/RULES.md`: late dates are not clamped at the
/// model's origin day. The origin sits 40 days before the data date (as an
/// imported file has it) and a Finish On or Before constraint lies 90 days
/// before the data date: the late dates are the constraint's, before the
/// origin, and the float is the whole 64 working days between.
#[test]
fn late_dates_reach_before_the_origin_without_a_clamp() {
    // Data date Monday of week 6 (day 42), constraint Tuesday day -48 17:00.
    let mut m = constrained(CstrType::FinishOnBefore, at(-48, 17, 0), 600);
    m.project.data_date = Some(at(42, 7, 0));
    run(&mut m);
    assert_eq!(m.origin_serial, ORIGIN);
    let a = &m.activities[0];
    assert_eq!(early(a), (Some(at(42, 7, 0)), Some(at(42, 17, 0))));
    assert_eq!(late(a), (Some(at(-48, 7, 0)), Some(at(-48, 17, 0))));
    assert_eq!(a.total_float, Some(-64 * 600));

    // A chain whose late dates pass the origin: B must finish by Tuesday
    // 17:00, so A (ten days) must start ten working days before Monday 17:00.
    let mut b = task(2, "B", 600);
    b.cstr = CstrType::FinishOnBefore;
    b.cstr_date = Some(at(1, 17, 0));
    let mut m = model(
        vec![task(1, "A", 6000), b],
        vec![tie(0, 1, LinkType::FS, 0)],
        plain(),
    );
    run(&mut m);
    let a = &m.activities[0];
    assert_eq!(late(a), (Some(at(-13, 7, 0)), Some(at(0, 17, 0))));
    assert_eq!(a.total_float, Some(-9 * 600));
    // The calendars keep the model's origin.
    assert!(m.calendars.iter().all(|c| c.cal.origin_serial == ORIGIN));
}

/// Rule CON-3 in `engine/RULES.md`.
#[test]
fn an_expected_finish_replaces_the_date_of_a_finish_constraint() {
    let build = |expect| {
        let mut m = constrained(CstrType::FinishOnAfter, at(2, 17, 0), 600);
        m.activities[0].expect_end = expect;
        m
    };
    let mut m = build(None);
    assert_eq!(
        Scheduler::new(&mut m).constraint_early_restart(0, true),
        Some(at(2, 7, 0))
    );
    let mut m = build(Some(at(3, 17, 0)));
    assert_eq!(
        Scheduler::new(&mut m).constraint_early_restart(0, true),
        Some(at(3, 7, 0))
    );
    // A Mandatory Finish takes it too.
    let mut m = constrained(CstrType::MandatoryFinish, at(2, 17, 0), 600);
    m.activities[0].expect_end = Some(at(3, 17, 0));
    assert_eq!(
        Scheduler::new(&mut m).constraint_early_restart(0, true),
        Some(at(3, 7, 0))
    );
    // The same for a late finish cap.
    let mut m = constrained(CstrType::FinishOnBefore, at(3, 17, 0), 600);
    m.activities[0].expect_end = Some(at(1, 17, 0));
    assert_eq!(
        Scheduler::new(&mut m).constraint_late_reend(0, true),
        Some(at(1, 17, 0))
    );
    // Not for a start constraint.
    let mut m = constrained(CstrType::StartOnAfter, at(2, 7, 0), 600);
    m.activities[0].expect_end = Some(at(3, 17, 0));
    assert_eq!(
        Scheduler::new(&mut m).constraint_early_restart(0, true),
        Some(at(2, 7, 0))
    );
}

/// Rule CON-4 in `engine/RULES.md`.
#[test]
fn a_milestone_ignores_constraints_on_the_end_it_does_not_have() {
    let early_of = |task_type, kind| {
        let mut m = constrained(kind, at(2, 12, 0), 0);
        m.activities[0].task_type = task_type;
        Scheduler::new(&mut m).constraint_early_restart(0, true)
    };
    let late_of = |task_type, kind| {
        let mut m = constrained(kind, at(2, 12, 0), 0);
        m.activities[0].task_type = task_type;
        Scheduler::new(&mut m).constraint_late_reend(0, true)
    };
    for kind in [
        CstrType::FinishOn,
        CstrType::FinishOnAfter,
        CstrType::MandatoryFinish,
    ] {
        assert_eq!(early_of(TaskType::Mile, kind), None, "{kind:?}");
        assert!(early_of(TaskType::Task, kind).is_some(), "{kind:?}");
        assert!(early_of(TaskType::FinMile, kind).is_some(), "{kind:?}");
    }
    for kind in [
        CstrType::StartOn,
        CstrType::StartOnAfter,
        CstrType::MandatoryStart,
    ] {
        assert_eq!(early_of(TaskType::FinMile, kind), None, "{kind:?}");
        assert!(early_of(TaskType::Task, kind).is_some(), "{kind:?}");
        assert!(early_of(TaskType::Mile, kind).is_some(), "{kind:?}");
    }
    for kind in [
        CstrType::FinishOn,
        CstrType::FinishOnBefore,
        CstrType::MandatoryFinish,
    ] {
        assert_eq!(late_of(TaskType::Mile, kind), None, "{kind:?}");
        assert!(late_of(TaskType::FinMile, kind).is_some(), "{kind:?}");
    }
    for kind in [
        CstrType::StartOn,
        CstrType::StartOnBefore,
        CstrType::MandatoryStart,
    ] {
        assert_eq!(late_of(TaskType::FinMile, kind), None, "{kind:?}");
        assert!(late_of(TaskType::Mile, kind).is_some(), "{kind:?}");
    }
}

/// Rule CON-5 in `engine/RULES.md`.
#[test]
fn constraints_bind_only_work_that_has_not_reached_their_end() {
    // Early dates: only work that has not started.
    for status in [StatusType::InProgress, StatusType::Completed] {
        let mut m = constrained(CstrType::StartOnAfter, at(2, 7, 0), 600);
        m.activities[0].status = status;
        assert_eq!(
            Scheduler::new(&mut m).constraint_early_restart(0, true),
            None
        );
    }
    // Late dates: a start constraint binds only unstarted work, a finish
    // constraint binds anything not completed.
    let mut m = constrained(CstrType::StartOnBefore, at(2, 7, 0), 600);
    m.activities[0].status = StatusType::InProgress;
    assert_eq!(Scheduler::new(&mut m).constraint_late_reend(0, true), None);
    let mut m = constrained(CstrType::FinishOnBefore, at(2, 17, 0), 600);
    m.activities[0].status = StatusType::InProgress;
    assert_eq!(
        Scheduler::new(&mut m).constraint_late_reend(0, true),
        Some(at(2, 17, 0))
    );
    m.activities[0].status = StatusType::Completed;
    assert_eq!(Scheduler::new(&mut m).constraint_late_reend(0, true), None);
}

/// Rule CON-6 in `engine/RULES.md`.
#[test]
fn a_start_milestone_with_a_start_constraint_in_non_working_time_rolls_back_to_the_previous_finish()
{
    // Saturday noon on the Monday-to-Friday calendar.
    let mut m = constrained(CstrType::StartOnBefore, at(5, 12, 0), 0);
    m.activities[0].task_type = TaskType::Mile;
    assert_eq!(
        Scheduler::new(&mut m).constraint_late_reend(0, true),
        Some(at(4, 17, 0))
    );
    // Work with a duration instead finishes a duration after the rolled start.
    let mut m = constrained(CstrType::StartOnBefore, at(0, 7, 0), 600);
    assert_eq!(
        Scheduler::new(&mut m).constraint_late_reend(0, true),
        Some(at(0, 17, 0))
    );
}

/// Rule CON-7 in `engine/RULES.md`.
#[test]
fn mandatory_dates_are_placed_before_the_passes_and_move_on_from_non_working_time() {
    // A Mandatory Start on Saturday noon starts Monday 07:00.
    let mut m = constrained(CstrType::MandatoryStart, at(5, 12, 0), 600);
    Scheduler::new(&mut m).set_mandatory_dates_forward(&[0]);
    assert_eq!(
        (m.activities[0].restart, m.activities[0].reend),
        (Some(at(7, 7, 0)), Some(at(7, 17, 0)))
    );
    // A Mandatory Finish converts to a start through the duration.
    let mut m = constrained(CstrType::MandatoryFinish, at(2, 17, 0), 600);
    Scheduler::new(&mut m).set_mandatory_dates_forward(&[0]);
    assert_eq!(
        (m.activities[0].restart, m.activities[0].reend),
        (Some(at(2, 7, 0)), Some(at(2, 17, 0)))
    );
    // A Mandatory Finish on a start milestone has no date to give.
    let mut m = constrained(CstrType::MandatoryFinish, at(2, 17, 0), 0);
    m.activities[0].task_type = TaskType::Mile;
    m.activities[0].restart = Some(0);
    Scheduler::new(&mut m).set_mandatory_dates_forward(&[0]);
    assert_eq!(
        (m.activities[0].restart, m.activities[0].reend),
        (None, None)
    );
    // Backward: a Mandatory Finish is the late finish, the start follows.
    let mut m = constrained(CstrType::MandatoryFinish, at(2, 17, 0), 600);
    Scheduler::new(&mut m).set_mandatory_dates_backward(&[0]);
    assert_eq!(
        (m.activities[0].restart, m.activities[0].reend),
        (Some(at(2, 7, 0)), Some(at(2, 17, 0)))
    );
}

/// Rule CON-8 in `engine/RULES.md`.
#[test]
fn a_mandatory_constraint_gives_equal_early_and_late_dates_and_zero_free_float() {
    let mut m = constrained(CstrType::MandatoryStart, at(2, 7, 0), 600);
    m.activities.push(task(2, "B", 600));
    m.links.push(tie(0, 1, LinkType::FS, 0));
    m.rebuild_links().unwrap();
    run(&mut m);
    let a = &m.activities[0];
    assert_eq!(early(a), (Some(at(2, 7, 0)), Some(at(2, 17, 0))));
    assert_eq!(late(a), early(a));
    assert_eq!(a.total_float, Some(0));
    // Free float is zero under a mandatory constraint though B starts later.
    m.activities[1].cstr = CstrType::StartOnAfter;
    m.activities[1].cstr_date = Some(at(4, 7, 0));
    run(&mut m);
    assert_eq!(m.activities[0].free_float, Some(0));
    assert_eq!(m.activities[1].early_start, Some(at(4, 7, 0)));
    // The tie into a mandatory successor never drives.
    let mut m = pair(LinkType::FS, 0, plain());
    m.activities[1].cstr = CstrType::MandatoryStart;
    m.activities[1].cstr_date = Some(at(1, 7, 0));
    run(&mut m);
    assert_eq!(Scheduler::new(&mut m).relationship_driving(0), Some(false));
}

/// Rule CON-8 in `engine/RULES.md`: when every activity the passes schedule
/// is mandatory, the passes still store the dates the mandatory pre-pass
/// placed.
#[test]
fn a_schedule_of_mandatory_work_only_still_stores_its_dates() {
    // One activity with a Mandatory Finish on Wednesday 17:00.
    let mut m = constrained(CstrType::MandatoryFinish, at(2, 17, 0), 600);
    run(&mut m);
    let a = &m.activities[0];
    assert_eq!(early(a), (Some(at(2, 7, 0)), Some(at(2, 17, 0))));
    assert_eq!(late(a), early(a));
    assert_eq!(a.total_float, Some(0));
    assert_eq!(m.project.scd_end, Some(at(2, 17, 0)));

    // Everything complete except a final finish milestone with a Mandatory
    // Finish on Thursday 17:00.
    let mut done = task(1, "A", 0);
    done.status = StatusType::Completed;
    done.act_start = Some(at(0, 7, 0));
    done.act_finish = Some(at(0, 17, 0));
    let mut finish = task(2, "END", 0);
    finish.task_type = TaskType::FinMile;
    finish.cstr = CstrType::MandatoryFinish;
    finish.cstr_date = Some(at(3, 17, 0));
    let mut m = model(
        vec![done, finish],
        vec![tie(0, 1, LinkType::FS, 0)],
        plain(),
    );
    m.project.data_date = Some(at(1, 7, 0));
    run(&mut m);
    let end = &m.activities[1];
    assert_eq!(early(end), (Some(at(3, 17, 0)), Some(at(3, 17, 0))));
    assert_eq!(late(end), early(end));
    assert_eq!(end.total_float, Some(0));
    assert_eq!(m.project.scd_end, Some(at(3, 17, 0)));
}

// ---------------------------------------------------------------------------
// Expected Finish
// ---------------------------------------------------------------------------

/// Rule EXP-1 in `engine/RULES.md`.
#[test]
fn an_expected_finish_sets_the_remaining_duration_from_the_start() {
    let options = ScheduleOptions {
        use_expected_finish: true,
        ..plain()
    };
    // Later than the natural finish: the activity lengthens to it.
    let mut a = task(1, "A", 600);
    a.expect_end = Some(at(2, 17, 0));
    let mut m = model(vec![a], vec![], options.clone());
    Scheduler::new(&mut m).schedule_forward_task(0);
    let a = &m.activities[0];
    assert_eq!(
        (a.restart, a.reend, a.rem),
        (Some(at(0, 7, 0)), Some(at(2, 17, 0)), 1800)
    );
    // An expected finish at or before the start leaves no remaining work.
    let mut a = task(1, "A", 600);
    a.expect_end = Some(at(0, 7, 0));
    let mut m = model(vec![a], vec![], options.clone());
    Scheduler::new(&mut m).schedule_forward_task(0);
    let a = &m.activities[0];
    assert_eq!(
        (a.restart, a.reend, a.rem),
        (Some(at(0, 7, 0)), Some(at(0, 7, 0)), 0)
    );
    // With a date-driving assignment the remaining work is kept instead.
    let mut a = task(1, "A", 600);
    a.expect_end = Some(at(0, 7, 0));
    a.assignments.push(Assignment {
        rsrc_id: 3,
        rollup: true,
        rem: 600,
        rollup_present: true,
        ..Assignment::default()
    });
    let mut m = model(vec![a], vec![], options.clone());
    Scheduler::new(&mut m).schedule_forward_task(0);
    let a = &m.activities[0];
    assert_eq!((a.reend, a.rem), (Some(at(0, 17, 0)), 600));
    // The option off, or a driving finish-side tie, leaves the duration alone.
    let mut a = task(1, "A", 600);
    a.expect_end = Some(at(2, 17, 0));
    let mut m = model(vec![a.clone()], vec![], plain());
    Scheduler::new(&mut m).schedule_forward_task(0);
    assert_eq!(m.activities[0].rem, 600);
    let mut m = model(vec![a], vec![], options);
    m.activities[0].restart = Some(at(0, 7, 0));
    m.activities[0].reend = Some(at(0, 17, 0));
    Scheduler::new(&mut m).rem_finish(0, LinkType::FF);
    assert_eq!(m.activities[0].reend, Some(at(0, 17, 0)));
    assert_eq!(m.activities[0].rem, 600);
}

// ---------------------------------------------------------------------------
// Float and the pass results
// ---------------------------------------------------------------------------

/// A -> B -> E and A -> C -> E, all finish to start; B is one day, C three,
/// E one. A runs Monday, B Tuesday, C Tuesday to Thursday, E Friday.
fn diamond(lag_b_e: i64, options: ScheduleOptions) -> Model {
    model(
        vec![
            task(1, "A", 600),
            task(2, "B", 600),
            task(3, "C", 1800),
            task(4, "E", 600),
        ],
        vec![
            tie(0, 1, LinkType::FS, 0),
            tie(0, 2, LinkType::FS, 0),
            tie(1, 3, LinkType::FS, lag_b_e),
            tie(2, 3, LinkType::FS, 0),
        ],
        options,
    )
}

/// Rule FLT-1, BWD-4 in `engine/RULES.md`.
#[test]
fn the_forward_and_backward_pass_give_textbook_dates_and_float() {
    let mut m = diamond(0, plain());
    run(&mut m);
    let dates = |i: usize| (early(&m.activities[i]), late(&m.activities[i]));
    assert_eq!(
        dates(0),
        (
            (Some(at(0, 7, 0)), Some(at(0, 17, 0))),
            (Some(at(0, 7, 0)), Some(at(0, 17, 0)))
        )
    );
    assert_eq!(
        dates(1),
        (
            (Some(at(1, 7, 0)), Some(at(1, 17, 0))),
            (Some(at(3, 7, 0)), Some(at(3, 17, 0)))
        )
    );
    assert_eq!(
        dates(2),
        (
            (Some(at(1, 7, 0)), Some(at(3, 17, 0))),
            (Some(at(1, 7, 0)), Some(at(3, 17, 0)))
        )
    );
    assert_eq!(
        dates(3),
        (
            (Some(at(4, 7, 0)), Some(at(4, 17, 0))),
            (Some(at(4, 7, 0)), Some(at(4, 17, 0)))
        )
    );
    let float: Vec<_> = m.activities.iter().map(|a| a.total_float).collect();
    assert_eq!(float, [Some(0), Some(1200), Some(0), Some(0)]);
    // The project finish is the latest early finish.
    assert_eq!(m.project.scd_end, Some(at(4, 17, 0)));
    // The late limit is the planned finish when one is set and used.
    let mut m = diamond(0, plain());
    m.project.plan_end = Some(at(11, 17, 0));
    run(&mut m);
    assert_eq!(m.activities[3].late_finish, Some(at(11, 17, 0)));
    assert_eq!(m.activities[0].total_float, Some(3000));
    // Without it, or with the option off, the scheduled finish is the limit.
    let mut m = diamond(
        0,
        ScheduleOptions {
            use_must_finish: false,
            ..plain()
        },
    );
    m.project.plan_end = Some(at(11, 17, 0));
    run(&mut m);
    assert_eq!(m.activities[3].late_finish, Some(at(4, 17, 0)));
    // Open ends take their early finish as the late finish when asked to.
    let mut m = diamond(
        0,
        ScheduleOptions {
            open_end_uses_early: true,
            ..plain()
        },
    );
    m.project.plan_end = Some(at(11, 17, 0));
    run(&mut m);
    assert_eq!(m.activities[3].late_finish, Some(at(4, 17, 0)));
    assert_eq!(m.activities[3].total_float, Some(0));
}

/// Rule FLT-2 in `engine/RULES.md`.
#[test]
fn free_float_is_the_gap_to_the_nearest_successor_after_the_lag_and_open_ends_run_to_the_finish() {
    let mut m = diamond(0, plain());
    run(&mut m);
    let ff: Vec<_> = m.activities.iter().map(|a| a.free_float).collect();
    assert_eq!(ff, [Some(0), Some(1200), Some(0), Some(0)]);
    // A lag on the tie takes working time out of the gap.
    let mut m = diamond(300, plain());
    run(&mut m);
    assert_eq!(m.activities[1].free_float, Some(900));
    // Work whose Expected Finish is in use has none, whatever the gap.
    let mut m = diamond(
        0,
        ScheduleOptions {
            use_expected_finish: true,
            ..plain()
        },
    );
    m.activities[1].expect_end = Some(at(1, 17, 0));
    run(&mut m);
    assert_eq!(m.activities[1].free_float, Some(0));
    assert_eq!(m.activities[1].total_float, Some(1200));
    // An open end floats to the project finish.
    let mut m = model(vec![task(1, "A", 600), task(2, "Z", 3000)], vec![], plain());
    run(&mut m);
    assert_eq!(m.activities[0].free_float, Some(2400));
    assert_eq!(m.activities[1].free_float, Some(0));
}

/// Rule FLT-3 in `engine/RULES.md`.
#[test]
fn a_ties_driving_flag_is_set_when_its_free_float_is_zero() {
    let mut m = diamond(0, plain());
    run(&mut m);
    let s = Scheduler::new(&mut m);
    // B -> E has a day of float; C -> E, A -> C, A -> B have none.
    assert_eq!(s.relationship_free_float(2), Some(1200));
    assert_eq!(s.relationship_driving(2), Some(false));
    for r in [0, 1, 3] {
        assert_eq!(s.relationship_free_float(r), Some(0), "{r}");
        assert_eq!(s.relationship_driving(r), Some(true), "{r}");
    }
}

/// Rule FLT-4 in `engine/RULES.md`.
#[test]
fn ties_of_summaries_never_drive_and_ties_of_level_of_effort_work_always_do() {
    let mut m = model(
        vec![task(1, "A", 600), task(2, "L", 0), task(3, "B", 600)],
        vec![tie(0, 1, LinkType::SS, 0), tie(1, 2, LinkType::FF, 0)],
        plain(),
    );
    m.activities[1].task_type = TaskType::Loe;
    run(&mut m);
    let s = Scheduler::new(&mut m);
    for r in 0..2 {
        assert_eq!(s.relationship_free_float(r), Some(0));
        assert_eq!(s.relationship_driving(r), Some(true));
    }
    let mut m = pair(LinkType::FS, 0, plain());
    m.activities[0].task_type = TaskType::Wbs;
    run(&mut m);
    assert_eq!(Scheduler::new(&mut m).relationship_driving(0), Some(false));
}

/// Rule FLT-5 in `engine/RULES.md`.
#[test]
fn progress_replaces_a_tie_when_either_end_has_the_matching_actual_date() {
    let combos = |kind: LinkType| {
        let mut out = vec![];
        for (ps, pf, ss, sf) in [
            (true, false, false, false),
            (false, true, false, false),
            (false, false, true, false),
            (false, false, false, true),
        ] {
            let mut m = pair(kind, 0, plain());
            let set = |a: &mut Activity, s: bool, f: bool| {
                a.act_start = s.then_some(0);
                a.act_finish = f.then_some(0);
            };
            set(&mut m.activities[0], ps, pf);
            set(&mut m.activities[1], ss, sf);
            out.push(Scheduler::new(&mut m).progress_replaces_tie(0));
        }
        out
    };
    // Columns: predecessor started, predecessor finished, successor started,
    // successor finished.
    assert_eq!(combos(LinkType::FS), [false, true, true, false]);
    assert_eq!(combos(LinkType::FF), [false, true, false, true]);
    assert_eq!(combos(LinkType::SF), [true, false, false, true]);
    assert_eq!(combos(LinkType::SS), [true, false, true, false]);
    assert_eq!(combos(LinkType::None), [false, false, false, false]);
}

/// Rule FLT-6 in `engine/RULES.md`.
#[test]
fn the_float_type_chooses_start_float_finish_float_or_the_smaller() {
    let dated = |float_type, expected_finish: bool| {
        let mut a = task(1, "A", 600);
        a.early_start = Some(at(0, 7, 0));
        a.early_finish = Some(at(0, 17, 0));
        a.late_start = Some(at(1, 7, 0));
        a.late_finish = Some(at(2, 17, 0));
        if expected_finish {
            a.expect_end = Some(at(0, 17, 0));
        }
        let mut m = model(
            vec![a],
            vec![],
            ScheduleOptions {
                float_type,
                ..plain()
            },
        );
        Scheduler::new(&mut m).calc_total_float(0)
    };
    assert_eq!(dated(FloatType::Start, false), Some(600));
    assert_eq!(dated(FloatType::Finish, false), Some(1200));
    assert_eq!(dated(FloatType::Min, false), Some(600));
    // With an expected finish and Actual Dates or Progress Override, the
    // smallest-float setting reports the finish float.
    assert_eq!(dated(FloatType::Min, true), Some(1200));
    // Finish float is negative when the late finish precedes the early finish.
    let mut a = task(1, "A", 600);
    a.early_finish = Some(at(2, 17, 0));
    a.late_finish = Some(at(0, 17, 0));
    let mut m = model(
        vec![a],
        vec![],
        ScheduleOptions {
            float_type: FloatType::Finish,
            ..plain()
        },
    );
    assert_eq!(Scheduler::new(&mut m).calc_total_float(0), Some(-1200));
    assert_eq!(dated(FloatType::Start, true), Some(600));
}

/// Rule FLT-7 in `engine/RULES.md`.
#[test]
fn the_longest_path_is_the_chain_of_driving_ties_back_from_the_last_finish() {
    let mut m = diamond(0, plain());
    m.activities.push(task(5, "Side", 600));
    run(&mut m);
    let flags: Vec<_> = m.activities.iter().map(|a| a.driving_path).collect();
    // A, C, E are on the path; B has float; Side finishes earlier.
    assert_eq!(flags, [true, false, true, true, false]);
}

/// Rule FLT-8 in `engine/RULES.md`.
#[test]
fn as_late_as_possible_work_moves_to_use_its_free_float() {
    // B is one day with three days of float before E; ALAP pushes it to end
    // when E starts.
    let mut m = diamond(0, plain());
    m.activities[1].cstr = CstrType::Alap;
    run(&mut m);
    let b = &m.activities[1];
    assert_eq!(early(b), (Some(at(3, 7, 0)), Some(at(3, 17, 0))));
    assert_eq!(b.free_float, Some(0));
    // A start milestone moves to the start moment where its successor starts.
    let mut m = model(
        vec![task(1, "P", 3000), task(2, "B", 600), task(3, "M", 0)],
        vec![tie(0, 1, LinkType::FS, 0), tie(2, 1, LinkType::FS, 0)],
        plain(),
    );
    m.activities[2].task_type = TaskType::Mile;
    m.activities[2].cstr = CstrType::Alap;
    run(&mut m);
    assert_eq!(m.activities[1].early_start, Some(at(7, 7, 0)));
    assert_eq!(
        early(&m.activities[2]),
        (Some(at(7, 7, 0)), Some(at(7, 7, 0)))
    );
}

/// Rule BWD-1 in `engine/RULES.md`.
#[test]
fn start_side_limits_are_made_working_time_and_compete_with_finish_limits() {
    // A (one day) has a start-to-start successor B and a finish-to-start
    // successor C that must start Thursday 07:00.
    let build = |b_restart, lag, completed: bool| {
        let mut a = task(1, "A", 600);
        if completed {
            a.status = StatusType::Completed;
            a.act_start = Some(at(0, 7, 0));
            a.act_finish = Some(at(0, 17, 0));
        }
        let mut b = task(2, "B", 600);
        b.restart = Some(b_restart);
        let mut c = task(3, "C", 600);
        c.restart = Some(at(3, 7, 0));
        model(
            vec![a, b, c],
            vec![tie(0, 1, LinkType::SS, lag), tie(0, 2, LinkType::FS, 0)],
            plain(),
        )
    };
    // The start limit plus A's duration is earlier than C's limit: it wins.
    let mut m = build(at(1, 7, 0), 0, false);
    assert_eq!(
        Scheduler::new(&mut m).get_pred_reend_date(0),
        (Some(at(1, 17, 0)), LinkType::SS, Some(at(1, 7, 0)))
    );
    // A start limit in non-working time becomes the previous working finish.
    let mut m = build(at(5, 12, 0), 0, false);
    assert_eq!(
        Scheduler::new(&mut m).get_pred_reend_date(0),
        (Some(at(3, 7, 0)), LinkType::FS, Some(at(4, 17, 0)))
    );
    // The lag is taken off an open predecessor's start limit, not a completed one's.
    let mut m = build(at(2, 7, 0), 480, false);
    assert_eq!(
        Scheduler::new(&mut m).get_pred_reend_date(0).2,
        Some(at(1, 9, 0))
    );
    let mut m = build(at(2, 7, 0), 480, true);
    assert_eq!(
        Scheduler::new(&mut m).get_pred_reend_date(0).2,
        Some(at(2, 7, 0))
    );
}

/// Rule FLT-9 in `engine/RULES.md`.
#[test]
fn a_tie_out_of_a_mandatory_activity_gives_zero_float_and_drives_only_when_logic_is_retained() {
    for (options, drives) in [(plain(), false), (retained(), true)] {
        let mut m = pair(LinkType::FS, 0, options);
        m.activities[0].cstr = CstrType::MandatoryStart;
        m.activities[0].cstr_date = Some(at(0, 7, 0));
        run(&mut m);
        let s = Scheduler::new(&mut m);
        assert_eq!(s.relationship_free_float(0), Some(0));
        assert_eq!(s.relationship_driving(0), Some(drives));
    }
}

/// Rule BWD-5 in `engine/RULES.md`.
#[test]
fn zero_duration_work_has_equal_late_start_and_finish() {
    // A start milestone on the lunch-break calendar held to 13:00 by the
    // planned finish keeps that working moment.
    let mut z = task(1, "Z", 0);
    z.task_type = TaskType::Mile;
    z.cal = 1;
    let mut m = model(vec![z], vec![], plain());
    m.project.plan_end = Some(at(0, 13, 0));
    Scheduler::new(&mut m).schedule_backward_task(0);
    let z = &m.activities[0];
    assert_eq!(
        (z.restart, z.reend),
        (Some(at(0, 13, 0)), Some(at(0, 13, 0)))
    );
}

/// Rule RES-2 in `engine/RULES.md`.
#[test]
fn resource_dependent_work_spans_the_earliest_start_to_the_latest_finish_of_its_assignments() {
    let assign = |rsrc_id, calendar| Assignment {
        rsrc_id,
        rem: 300,
        cal: Some(calendar),
        ..Assignment::default()
    };
    let mut r = task(1, "R", 600);
    r.task_type = TaskType::Resource;
    r.assignments = vec![assign(7, 0), assign(8, 1)];
    let mut m = model(vec![r], vec![], plain());
    let mut s = Scheduler::new(&mut m);
    // Starting 12:30: the 5x10 crew starts then, the lunch-break crew at 13:00.
    s.model.activities[0].restart = Some(at(0, 12, 30));
    s.place_resource_dependent(0, false);
    assert_eq!(
        (s.model.activities[0].restart, s.model.activities[0].reend),
        (Some(at(0, 12, 30)), Some(at(1, 7, 30)))
    );
    // A backward pass places a ten-hour lunch-break crew on its own calendar.
    let mut r = task(1, "R", 600);
    r.task_type = TaskType::Resource;
    r.assignments = vec![Assignment {
        rsrc_id: 7,
        rem: 600,
        cal: Some(1),
        ..Assignment::default()
    }];
    let mut m = model(vec![r], vec![], plain());
    m.project.plan_end = Some(at(4, 17, 0));
    Scheduler::new(&mut m).schedule_backward_task(0);
    let r = &m.activities[0];
    assert_eq!(
        (r.restart, r.reend),
        (Some(at(3, 17, 0)), Some(at(4, 17, 0)))
    );
}

/// Rule FLT-5 in `engine/RULES.md`.
#[test]
fn progress_override_leaves_ties_that_progress_replaces_out_of_free_float() {
    // A -> B (B already started) and A -> C (C may not start before Wednesday).
    let build = |options| {
        let mut b = task(2, "B", 600);
        b.status = StatusType::InProgress;
        b.act_start = Some(at(0, 7, 0));
        let mut c = task(3, "C", 600);
        c.cstr = CstrType::StartOnAfter;
        c.cstr_date = Some(at(2, 7, 0));
        let mut m = model(
            vec![task(1, "A", 600), b, c],
            vec![tie(0, 1, LinkType::FS, 0), tie(0, 2, LinkType::FS, 0)],
            options,
        );
        run(&mut m);
        m
    };
    let m = build(ScheduleOptions {
        progress_override: true,
        ..plain()
    });
    assert_eq!(m.activities[0].free_float, Some(600));
    let m = build(plain());
    assert_eq!(m.activities[0].free_float, Some(0));
}

/// Rule REL-4 in `engine/RULES.md`.
#[test]
fn a_start_lag_used_up_exactly_keeps_the_date_unsnapped() {
    // A started Monday 07:00 and the data date is its closing minute: the
    // lag of one day is exactly used up, so B follows A's remaining start as is.
    let mut m = pair(LinkType::SS, 600, plain());
    m.activities[0].status = StatusType::InProgress;
    m.activities[0].act_start = Some(at(0, 7, 0));
    m.activities[0].restart = Some(at(0, 17, 0));
    m.project.data_date = Some(at(0, 17, 0));
    assert_eq!(
        Scheduler::new(&mut m).apply_lag_forward(0),
        Some(at(0, 17, 0))
    );
}

/// Rule FWD-5 in `engine/RULES.md`.
#[test]
fn a_late_start_at_a_start_ties_shift_close_is_kept_by_the_backward_pass() {
    // A (five hours) starts with B (SS); B's late start is Monday 17:00, the
    // close of the shift. A would start Tuesday 07:00 with nothing worked
    // since: it takes the close instead.
    let mut b = task(2, "B", 600);
    b.restart = Some(at(0, 17, 0));
    let mut m = model(
        vec![task(1, "A", 300), b],
        vec![tie(0, 1, LinkType::SS, 0)],
        plain(),
    );
    m.project.plan_end = Some(at(4, 17, 0));
    Scheduler::new(&mut m).schedule_backward_task(0);
    let a = &m.activities[0];
    assert_eq!(
        (a.restart, a.reend),
        (Some(at(0, 17, 0)), Some(at(1, 12, 0)))
    );
}

/// Rule EXP-1 in `engine/RULES.md`.
#[test]
fn expected_finish_is_not_used_by_alap_work_or_while_leveling() {
    let options = ScheduleOptions {
        use_expected_finish: true,
        ..plain()
    };
    let mut a = task(1, "A", 600);
    a.expect_end = Some(at(2, 17, 0));
    assert!(a.uses_expected_finish(&options, false));
    assert!(!a.uses_expected_finish(&options, true));
    a.cstr = CstrType::Alap;
    assert!(!a.uses_expected_finish(&options, false));
    a.cstr = CstrType::None;
    a.expect_end = None;
    assert!(!a.uses_expected_finish(&options, false));
}

/// Rule CON-7 in `engine/RULES.md`.
#[test]
fn mandatory_dates_never_precede_the_data_date_and_keep_a_finish_milestones_finish() {
    let dated = |m: &Model| (m.activities[0].restart, m.activities[0].reend);
    // A Mandatory Start before the data date starts at the data date.
    let mut m = constrained(CstrType::MandatoryStart, at(0, 7, 0), 600);
    m.project.data_date = Some(at(2, 7, 0));
    Scheduler::new(&mut m).set_mandatory_dates_forward(&[0]);
    assert_eq!(dated(&m), (Some(at(2, 7, 0)), Some(at(2, 17, 0))));
    // A finish milestone held to a shift's close stays there.
    let mut m = constrained(CstrType::MandatoryFinish, at(4, 17, 0), 0);
    m.activities[0].task_type = TaskType::FinMile;
    Scheduler::new(&mut m).set_mandatory_dates_forward(&[0]);
    assert_eq!(dated(&m), (Some(at(4, 17, 0)), Some(at(4, 17, 0))));
    // Backward, an earlier planned finish limits the constraint date.
    let mut m = constrained(CstrType::MandatoryFinish, at(4, 17, 0), 600);
    m.project.plan_end = Some(at(2, 17, 0));
    Scheduler::new(&mut m).set_mandatory_dates_backward(&[0]);
    assert_eq!(dated(&m), (Some(at(2, 7, 0)), Some(at(2, 17, 0))));
}

/// Rule FLT-9 in `engine/RULES.md`.
#[test]
fn tie_float_by_mode_follows_the_ends_actual_and_remaining_dates() {
    // A works Monday and B starts Thursday: four working days apart, less
    // the day A works, is 1200 minutes of float on a finish-to-start tie.
    let build = |options| {
        let mut m = pair(LinkType::FS, 0, options);
        m.activities[0].rem_restart = Some(at(0, 7, 0));
        m.activities[0].rem_reend = Some(at(0, 17, 0));
        m.activities[1].rem_restart = Some(at(3, 7, 0));
        m.activities[1].rem_reend = Some(at(3, 17, 0));
        m
    };
    let flag = |m: &mut Model| Scheduler::new(m).relationship_float_and_flag(0);
    let start = |a: &mut Activity, when: i64| {
        a.status = StatusType::InProgress;
        a.act_start = Some(when);
    };
    let over = ScheduleOptions {
        progress_override: true,
        ..plain()
    };
    // Actual Dates: to the successor's remaining start, or its actual start.
    assert_eq!(flag(&mut build(plain())), (1200, false));
    let mut m = build(plain());
    start(&mut m.activities[1], at(1, 7, 0));
    assert_eq!(flag(&mut m), (0, true));
    // Progress Override: a started successor replaces the tie.
    assert_eq!(flag(&mut build(over.clone())), (1200, false));
    let mut m = build(over);
    start(&mut m.activities[1], at(1, 7, 0));
    assert_eq!(flag(&mut m), (0, false));
    // The pair of remaining dates counts as available when either exists: with
    // only a remaining start, the missing remaining finish gives no float.
    let mut m = build(plain());
    m.activities[0].rem_reend = None;
    m.activities[0].early_finish = Some(at(0, 17, 0));
    assert_eq!(flag(&mut m), (0, false));
    // Level-of-effort work: zero float, driving.
    let mut m = build(plain());
    m.activities[1].task_type = TaskType::Loe;
    assert_eq!(flag(&mut m), (0, true));
    // Retained Logic, completed predecessor without remaining dates.
    let mut m = build(retained());
    m.activities[0].status = StatusType::Completed;
    m.activities[0].rem_restart = None;
    m.activities[0].rem_reend = None;
    assert_eq!(flag(&mut m), (0, true));
    // Completed predecessor with remaining dates: measured from its early finish.
    let mut m = build(retained());
    let a = &mut m.activities[0];
    a.status = StatusType::Completed;
    a.early_start = Some(at(0, 7, 0));
    a.early_finish = Some(at(0, 17, 0));
    a.rem_restart = Some(at(2, 7, 0));
    a.rem_reend = Some(at(2, 17, 0));
    assert_eq!(flag(&mut m), (1200, false));
    // Retained Logic, start-to-start: a started predecessor gives zero and
    // does not drive; unstarted work into a started successor drives.
    let mut m = build(retained());
    m.links[0].link_type = LinkType::SS;
    start(&mut m.activities[0], at(0, 7, 0));
    assert_eq!(flag(&mut m), (0, false));
    let mut m = build(retained());
    m.links[0].link_type = LinkType::SS;
    start(&mut m.activities[1], at(3, 7, 0));
    assert_eq!(flag(&mut m), (1800, true));
}

/// Rule PASS-8 in `engine/RULES.md`.
#[test]
fn a_fresh_schedule_stores_remaining_late_dates_equal_to_the_late_dates() {
    let mut m = diamond(0, plain());
    run(&mut m);
    for a in &m.activities {
        assert_eq!(a.rem_late_start, a.late_start, "{}", a.task_code);
        assert_eq!(a.rem_late_finish, a.late_finish, "{}", a.task_code);
        assert!(a.late_start.is_some());
    }
}

/// Rule CAL-10 in `engine/RULES.md`.
#[test]
fn cumulative_work_counts_whole_days_only_from_the_origin() {
    let c = CalendarAdapter::new(split());
    assert_eq!(c.work_at(-1), 0);
    assert_eq!(c.work_at(DAY), 480);
    assert_eq!(c.work_at(2 * DAY + 500), 960 + 20);
    // Before the origin only the day's own partial work is counted.
    assert_eq!(c.work_at(-3 * DAY + 500), 20);
    assert_eq!(c.work_at(-3 * DAY + 1100), 480);
}

/// Rule PASS-1 in `engine/RULES.md`.
#[test]
fn a_work_breakdown_summary_is_a_single_point_at_the_data_date() {
    let mut s = task(9, "WBS", 600);
    s.task_type = TaskType::Wbs;
    // A tie from the summary to B is dropped: B starts at the data date.
    let mut m = model(
        vec![task(1, "A", 3000), s, task(2, "B", 600)],
        vec![tie(1, 2, LinkType::FS, 0)],
        plain(),
    );
    m.project.data_date = Some(at(0, 5, 0));
    run(&mut m);
    assert_eq!(m.activities[2].early_start, Some(at(0, 7, 0)));
    let s = &m.activities[1];
    let point = Some(at(0, 7, 0));
    assert_eq!(early(s), (point, point));
    assert_eq!(late(s), (point, point));
    assert_eq!((s.rem, s.total_float), (0, Some(0)));
    // Its free float runs from there to the project finish.
    assert_eq!(s.free_float, Some(3000));
    assert_eq!((s.rem_late_start, s.rem_late_finish), (point, point));
}

/// Rule PASS-2 in `engine/RULES.md`.
#[test]
fn not_started_work_takes_its_planned_dates_from_the_schedule() {
    let mut a = task(1, "A", 600);
    a.target_start = Some(5);
    a.target_finish = Some(6);
    let mut b = task(2, "B", 600);
    b.status = StatusType::InProgress;
    b.act_start = Some(at(0, 7, 0));
    b.target_start = Some(7);
    b.target_finish = Some(8);
    let mut m = model(vec![a, b], vec![], plain());
    run(&mut m);
    let a = &m.activities[0];
    assert_eq!(
        (a.target_start, a.target_finish),
        (a.early_start, a.early_finish)
    );
    let b = &m.activities[1];
    assert_eq!((b.target_start, b.target_finish), (Some(7), Some(8)));
}

/// Rule PASS-3 in `engine/RULES.md`.
#[test]
fn completed_work_keeps_no_scheduled_values_unless_logic_is_retained() {
    let build = |options| {
        let mut a = task(1, "A", 0);
        a.status = StatusType::Completed;
        a.act_start = Some(at(0, 7, 0));
        a.act_finish = Some(at(0, 17, 0));
        a.expect_end = Some(at(0, 17, 0));
        model(
            vec![a, task(2, "B", 600)],
            vec![tie(0, 1, LinkType::FS, 0)],
            options,
        )
    };
    for options in [plain(), retained()] {
        let mut m = build(options);
        run(&mut m);
        let a = &m.activities[0];
        assert_eq!(
            (a.restart, a.reend, a.total_float, a.free_float),
            (None, None, None, None)
        );
        assert_eq!((a.rem_late_start, a.rem_late_finish), (None, None));
        assert_eq!(a.expect_end, None);
    }
    let mut m = build(plain());
    run(&mut m);
    assert_eq!(early(&m.activities[0]), (None, None));
    assert_eq!(late(&m.activities[0]), (None, None));
    let mut m = build(retained());
    run(&mut m);
    assert!(m.activities[0].early_start.is_some());
    assert!(m.activities[0].late_finish.is_some());
}

/// Rule PASS-4 in `engine/RULES.md`.
#[test]
fn only_open_work_is_scheduled_unless_logic_is_retained() {
    let mut done = task(1, "Done", 0);
    done.status = StatusType::Completed;
    done.act_start = Some(0);
    done.act_finish = Some(1);
    let mut going = task(2, "Going", 600);
    going.status = StatusType::InProgress;
    going.act_start = Some(0);
    let mut m = model(vec![done, going, task(3, "Open", 600)], vec![], plain());
    let mut s = Scheduler::new(&mut m);
    s.load_task_lists();
    assert_eq!(s.tasks_to_schedule, [1, 2]);
    assert_eq!(s.tasks_for_alap_loe, [1, 2]);
    assert_eq!(s.all_tasks, [0, 1, 2]);
    let mut m = model(
        vec![
            task(1, "Done", 0),
            task(2, "Going", 600),
            task(3, "Open", 600),
        ],
        vec![],
        retained(),
    );
    m.activities[0].status = StatusType::Completed;
    let mut s = Scheduler::new(&mut m);
    s.load_task_lists();
    assert_eq!(s.tasks_to_schedule, [0, 1, 2]);
    assert_eq!(s.tasks_for_alap_loe, [1, 2]);
    // Constrained activities are listed for the constraint passes.
    m.activities[2].cstr2 = CstrType::StartOnAfter;
    let mut s = Scheduler::new(&mut m);
    s.load_task_lists();
    assert_eq!(s.constrained_tasks, [2]);
}

/// Rule PASS-5 in `engine/RULES.md`.
#[test]
fn level_of_effort_and_summary_activities_take_no_part_in_the_passes() {
    assert!(Scheduler::skip_type(TaskType::Loe));
    assert!(Scheduler::skip_type(TaskType::Wbs));
    for kind in [
        TaskType::Task,
        TaskType::Mile,
        TaskType::FinMile,
        TaskType::Resource,
    ] {
        assert!(!Scheduler::skip_type(kind), "{kind:?}");
    }
}

/// Rule PASS-6 in `engine/RULES.md`.
#[test]
fn an_open_level_of_effort_has_zero_free_float_and_the_float_of_any_activity() {
    let mut m = model(
        vec![task(1, "A", 600), task(2, "L", 0), task(3, "B", 3000)],
        vec![tie(0, 1, LinkType::SS, 0), tie(2, 1, LinkType::FF, 0)],
        plain(),
    );
    m.activities[1].task_type = TaskType::Loe;
    run(&mut m);
    let l = &m.activities[1];
    assert_eq!(l.free_float, Some(0));
    assert_eq!(
        (l.rem_late_start, l.rem_late_finish),
        (l.late_start, l.late_finish)
    );
    assert!(l.total_float.is_some());
}

/// Rule PASS-7 in `engine/RULES.md`.
#[test]
fn input_order_does_not_change_any_result() {
    // Nine activities with ties of every type, listed in several orders.
    let kinds = [
        (0, 1, LinkType::FS, 0),
        (0, 2, LinkType::SS, 300),
        (1, 3, LinkType::FF, 0),
        (2, 3, LinkType::FS, 600),
        (3, 4, LinkType::SF, 0),
        (4, 5, LinkType::FS, 0),
        (2, 6, LinkType::FS, -300),
        (6, 7, LinkType::FF, 300),
        (5, 8, LinkType::FS, 0),
        (7, 8, LinkType::FS, 0),
    ];
    let lengths = [600, 1200, 1800, 900, 0, 300, 2400, 600, 600];
    let build = |order: &[usize]| {
        let mut position = [0; 9];
        for (new, &old) in order.iter().enumerate() {
            position[old] = new;
        }
        let activities = order
            .iter()
            .map(|&old| task(old as i64 + 1, &format!("T{old}"), lengths[old]))
            .collect();
        let links = kinds
            .iter()
            .map(|&(p, s, t, lag)| tie(position[p], position[s], t, lag))
            .collect();
        let mut m = model(activities, links, plain());
        run(&mut m);
        let mut out: Vec<_> = m
            .activities
            .iter()
            .map(|a| {
                (
                    a.task_code.clone(),
                    a.early_start,
                    a.early_finish,
                    a.late_start,
                    a.late_finish,
                    a.total_float,
                    a.free_float,
                    a.driving_path,
                )
            })
            .collect();
        out.sort();
        out
    };
    let forward: Vec<usize> = (0..9).collect();
    let backward: Vec<usize> = (0..9).rev().collect();
    let shuffled = vec![4, 8, 1, 6, 0, 3, 7, 2, 5];
    let want = build(&forward);
    assert_eq!(build(&backward), want);
    assert_eq!(build(&shuffled), want);
}

/// Rule RES-2 in `engine/RULES.md`.
#[test]
fn work_on_a_resource_calendar_and_activity_calendar_float_stay_apart() {
    // Ten hours on a lunch-break resource calendar run past the activity's own
    // 17:00 close; the activity calendar still measures its float.
    let mut r = task(1, "R", 600);
    r.task_type = TaskType::Resource;
    r.assignments.push(Assignment {
        rsrc_id: 7,
        rem: 600,
        cal: Some(1),
        ..Assignment::default()
    });
    let mut m = model(vec![r, task(2, "T", 3000)], vec![], plain());
    run(&mut m);
    let r = &m.activities[0];
    assert_eq!(early(r), (Some(at(0, 7, 0)), Some(at(0, 18, 0))));
    assert!(r.late_finish.is_some());
}
