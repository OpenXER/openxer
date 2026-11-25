//! Rules proven on saved probe projects: REL-9, FLT-9, CON-7, EXP-2, FWD-5
//! and RES-2 in `engine/RULES.md`.
use openxer_engine::{
    calendar::{CalendarAdapter, WorkCalendar},
    model::{
        Activity, Assignment, CstrType, Link, LinkType, Model, ProjectSettings, StatusType,
        TaskType,
    },
    options::ScheduleOptions,
    scheduler::Scheduler,
};
use std::collections::BTreeMap;

const DAY: i64 = 1440;
// Serial 44998 is a Monday, so template index 0 is Monday.
const ORIGIN: i64 = 44998;

fn weekdays(name: &str, shifts: Vec<(i64, i64)>) -> CalendarAdapter {
    let mut week: [Vec<(i64, i64)>; 7] = Default::default();
    for day in week.iter_mut().take(5) {
        *day = shifts.clone();
    }
    CalendarAdapter::new(WorkCalendar::new(
        name.into(),
        false,
        ORIGIN,
        week,
        BTreeMap::new(),
    ))
}

/// Monday 07:00 data date, 07:00-17:00 weekdays, project finish two weeks on.
fn model(activities: Vec<Activity>, links: Vec<Link>, options: ScheduleOptions) -> Model {
    let mut m = Model {
        origin_serial: ORIGIN,
        project_calendar: 0,
        calendars: vec![
            weekdays("5x10", vec![(420, 1020)]),
            weekdays("lunch", vec![(420, 720), (780, 1080)]),
        ],
        activities,
        links,
        project: ProjectSettings {
            data_date: Some(420),
            scd_end: Some(11 * DAY + 1020),
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

fn retained() -> ScheduleOptions {
    ScheduleOptions {
        retained_logic: true,
        open_end_uses_early: false,
        use_expected_finish: true,
        ..ScheduleOptions::default()
    }
}

#[test]
fn expected_finish_at_a_shift_start_rolls_back_to_the_previous_finish() {
    // Probe 402: an Expected Finish on Wednesday 07:00 finishes Tuesday 17:00.
    let mut t = task(1, "X", 960);
    t.expect_end = Some(2 * DAY + 420);
    let mut m = model(vec![t], vec![], retained());
    Scheduler::new(&mut m).schedule_forward_task(0);
    assert_eq!(m.activities[0].reend, Some(DAY + 1020));
}

#[test]
fn mandatory_start_converts_to_a_late_finish_through_the_duration() {
    // Probe 201: a Mandatory Start at Monday 17:00 holds a late start of
    // Tuesday 07:00 and a late finish eight hours on.
    let mut t = task(1, "X", 480);
    t.cstr = CstrType::MandatoryStart;
    t.cstr_date = Some(1020);
    let mut m = model(vec![t], vec![], retained());
    Scheduler::new(&mut m).set_mandatory_dates_backward(&[0]);
    assert_eq!(
        (m.activities[0].restart, m.activities[0].reend),
        (Some(DAY + 420), Some(DAY + 900))
    );
}

#[test]
fn mandatory_start_on_a_finish_milestone_leaves_it_undated() {
    // Probe 201: the reader has no date, so the pass gives none.
    let mut t = task(1, "X", 0);
    t.task_type = TaskType::FinMile;
    t.cstr = CstrType::MandatoryStart;
    t.cstr_date = Some(DAY + 600);
    t.restart = Some(420);
    let mut m = model(vec![t], vec![], retained());
    let mut s = Scheduler::new(&mut m);
    s.set_mandatory_dates_forward(&[0]);
    s.set_mandatory_dates_backward(&[0]);
    assert_eq!(
        (m.activities[0].restart, m.activities[0].reend),
        (None, None)
    );
}

#[test]
fn backward_start_stays_at_a_ties_shift_close() {
    // Probe 701: a late start the next morning, with no work since the tie's
    // start limit at the previous close, is that close.
    let mut t = task(1, "P", 300);
    t.restart = Some(DAY + 420);
    let mut m = model(vec![t], vec![], retained());
    let mut s = Scheduler::new(&mut m);
    s.adjust_for_non_work_start_time_from_dependency(0, LinkType::SS, Some(1020));
    assert_eq!(s.model.activities[0].restart, Some(1020));
    // Work between the limit and the late start: nothing moves.
    s.model.activities[0].restart = Some(DAY + 420);
    s.adjust_for_non_work_start_time_from_dependency(0, LinkType::SS, Some(960));
    assert_eq!(s.model.activities[0].restart, Some(DAY + 420));
}

#[test]
fn resource_dependent_work_runs_on_the_resource_calendar() {
    // Probes 814, 816: ten hours on a crew calendar with a lunch break runs
    // Monday 07:00 to 18:00, not to the activity calendar's 17:00.
    let mut t = task(1, "R", 600);
    t.task_type = TaskType::Resource;
    t.assignments.push(Assignment {
        rsrc_id: 7,
        rem: 600,
        cal: Some(1),
        ..Assignment::default()
    });
    let mut m = model(vec![t], vec![], retained());
    let mut s = Scheduler::new(&mut m);
    s.schedule_forward_task(0);
    assert_eq!(
        (s.model.activities[0].restart, s.model.activities[0].reend),
        (Some(420), Some(1080))
    );
    s.model.activities[0].reend = Some(4 * DAY + 1020);
    s.place_resource_dependent(0, true);
    assert_eq!(
        (s.model.activities[0].restart, s.model.activities[0].reend),
        (Some(3 * DAY + 1020), Some(4 * DAY + 1020))
    );
}

fn completed_pair(options: ScheduleOptions) -> Model {
    let mut p = task(1, "P", 0);
    p.status = StatusType::Completed;
    p.act_start = Some(4 * DAY + 420);
    p.act_finish = Some(4 * DAY + 720);
    let mut s = task(2, "S", 960);
    s.rem_restart = Some(7 * DAY + 420);
    s.rem_reend = Some(8 * DAY + 960);
    let link = Link {
        pred: 0,
        succ: 1,
        link_type: LinkType::FS,
        lag: 0,
        driving_path: false,
    };
    let mut m = model(vec![p, s], vec![link], options);
    m.project.data_date = Some(7 * DAY + 420);
    m
}

#[test]
fn ties_from_completed_work_by_scheduling_mode() {
    // Probe 303 (Actual Dates): from the actual finish, Friday 12:00 to
    // Monday 07:00 is five working hours and does not drive.
    let mut m = completed_pair(ScheduleOptions::default());
    let s = Scheduler::new(&mut m);
    assert_eq!(s.relationship_free_float(0), Some(300));
    assert_eq!(s.relationship_driving(0), Some(false));
    // Probe 302 (Progress Override): the actual finish replaces the tie.
    let mut m = completed_pair(ScheduleOptions {
        progress_override: true,
        ..ScheduleOptions::default()
    });
    let s = Scheduler::new(&mut m);
    assert_eq!(s.relationship_free_float(0), Some(0));
    assert_eq!(s.relationship_driving(0), Some(false));
}
