//! Late dates before the model's origin (BWD-5, CAL-10 in `engine/RULES.md`).
//! Work positions are counted from the origin day, so the position
//! functions mark any request that reaches before it, and the scheduling
//! entry points then run again from an earlier origin. The results must be
//! exactly those of the same project counted from an earlier origin to
//! begin with, through every entry point.
use openxer_engine::{
    calendar::{clear_before_origin, take_before_origin, CalendarAdapter, WorkCalendar, DAY},
    features::{LevelingInputs, ResourceDemand},
    model::{Activity, CstrType, Link, LinkType, Model, ProjectSettings},
    options::ScheduleOptions,
    scheduler::{self, Partition},
};
use std::collections::BTreeMap;

const ORIGIN: i64 = 46300;
/// Days the comparison model's origin lies earlier.
const BACK: i64 = 1000;

/// Mon-Fri 08:00-12:00 and 13:00-17:00 with a holiday, counted from
/// `origin` (a day serial; serial 0 is a Saturday).
fn weekdays(origin: i64) -> CalendarAdapter {
    let week: [Vec<(i64, i64)>; 7] = std::array::from_fn(|i| {
        let weekday = (origin + i as i64 + 6).rem_euclid(7); // 0 = Sunday
        if (1..=5).contains(&weekday) {
            vec![(480, 720), (780, 1020)]
        } else {
            vec![]
        }
    });
    let mut exc = BTreeMap::new();
    exc.insert(46300 - 10, vec![]);
    CalendarAdapter::new(WorkCalendar::new(
        "Standard".into(),
        false,
        origin,
        week,
        exc,
    ))
}

/// Monday 2026-11-02 08:00, 28 days after the origin.
fn data_date() -> i64 {
    (46328 - ORIGIN) * DAY + 480
}

fn task(id: i64, days: i64) -> Activity {
    Activity {
        task_id: id,
        task_code: format!("A{id}"),
        rem: days * 480,
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

/// A1 -> A2 -> A3, A3 to finish 60 days before the data date; A4 on its own.
fn project() -> Model {
    let mut a3 = task(3, 2);
    a3.cstr = CstrType::FinishOnBefore;
    a3.cstr_date = Some(data_date() - 60 * DAY + 540);
    let mut m = Model {
        origin_serial: ORIGIN,
        project_calendar: 0,
        calendars: vec![weekdays(ORIGIN)],
        activities: vec![task(1, 5), task(2, 4), a3, task(4, 3)],
        links: vec![tie(0, 1), tie(1, 2)],
        project: ProjectSettings {
            proj_id: 1,
            name: "P".into(),
            data_date: Some(data_date()),
            ..ProjectSettings::default()
        },
        options: ScheduleOptions {
            open_end_uses_early: false,
            ..ScheduleOptions::default()
        },
    };
    m.rebuild_links().unwrap();
    m
}

fn moved_back(m: &Model) -> Model {
    let mut out = m.clone();
    out.rebase(ORIGIN - BACK);
    out
}

/// Rule BWD-5, CAL-10 in `engine/RULES.md`.
#[test]
fn a_late_date_before_the_origin_is_computed_as_from_an_earlier_origin() {
    let mut m = project();
    let mut early = moved_back(&m);
    scheduler::schedule(&mut m).unwrap();
    scheduler::schedule(&mut early).unwrap();
    early.rebase(ORIGIN);
    assert_eq!(m, early);
    // The chain's late dates lie before the origin, as negative minutes.
    assert!(m.activities[0].late_start.unwrap() < 0);
    assert_eq!(m.origin_serial, ORIGIN);
}

/// Rule BWD-5, CAL-10 in `engine/RULES.md`: the leveled run too, with the
/// resource limits moved with the dates.
#[test]
fn a_leveled_run_before_the_origin_is_computed_as_from_an_earlier_origin() {
    let inputs = |shift: i64| {
        let mut i = LevelingInputs {
            horizon: data_date() + 200 * DAY + shift,
            ..LevelingInputs::default()
        };
        i.limits
            .insert(7, vec![(data_date() - 30 * DAY + shift, 1.0)]);
        for (id, days) in [(1, 5), (4, 3)] {
            i.demands.insert(
                id,
                vec![ResourceDemand {
                    resource_id: 7,
                    rate: 1.0,
                    remaining_units: (days * 8) as f64,
                    duration: days * 480,
                    calendar: None,
                }],
            );
        }
        i
    };
    let mut m = project();
    m.options.do_level = true;
    m.options.retained_logic = true;
    let mut early = moved_back(&m);
    let held = scheduler::schedule_with_resources_report(&mut m, &inputs(0)).unwrap();
    let held_early =
        scheduler::schedule_with_resources_report(&mut early, &inputs(BACK * DAY)).unwrap();
    early.rebase(ORIGIN);
    assert_eq!(m, early);
    assert_eq!(held, held_early);
    // The crew was shared: one of A1 and A4 waits for the other.
    let (a1, a4) = (&m.activities[0], &m.activities[3]);
    assert!(a1.rem_restart != a4.rem_restart);
    assert!(m.activities[0].late_start.unwrap() < 0);
}

/// Rule BWD-5, CAL-10, PASS-9 in `engine/RULES.md`: several projects
/// scheduled together, with each project's dates and the stored external
/// limits moved back too.
#[test]
fn projects_scheduled_together_before_the_origin_match_an_earlier_origin() {
    let build = |m: &Model, shift: i64| {
        let settings = |id: i64, name: &str, dd: i64| ProjectSettings {
            proj_id: id,
            name: name.into(),
            data_date: Some(dd + shift),
            ..ProjectSettings::default()
        };
        let mut m = m.clone();
        // A4 is the second project, two weeks later, fed by A3.
        m.links.push(tie(2, 3));
        m.rebuild_links().unwrap();
        let partition = Partition {
            of: vec![0, 0, 0, 1],
            projects: vec![
                settings(1, "A", data_date()),
                settings(2, "B", data_date() + 14 * DAY),
            ],
            ..Partition::default()
        };
        (m, partition)
    };
    let (mut m, partition) = build(&project(), 0);
    let (mut early, partition_early) = build(&moved_back(&project()), BACK * DAY);
    let out = scheduler::schedule_together(&mut m, partition).unwrap();
    let mut out_early = scheduler::schedule_together(&mut early, partition_early).unwrap();
    early.rebase(ORIGIN);
    assert_eq!(m, early);
    for p in &mut out_early.projects {
        p.shift_dates(-BACK * DAY);
    }
    for d in out_early
        .external_start
        .iter_mut()
        .chain(out_early.external_finish.iter_mut())
        .flatten()
    {
        *d -= BACK * DAY;
    }
    assert_eq!(out, out_early);
    assert!(out.external_finish[2].is_some());
    assert!(m.activities[0].late_start.unwrap() < 0);
}

/// Rule CAL-10 in `engine/RULES.md`: the position functions mark a request
/// before the origin, and only such a request.
#[test]
fn positions_before_the_origin_are_marked() {
    let c = weekdays(ORIGIN);
    let dd = data_date();
    clear_before_origin();
    c.work_at(dd);
    c.add(dd, 4800);
    c.sub(dd, 480);
    c.snap_start(0);
    c.start_at(0);
    c.finish_at(0);
    assert!(!take_before_origin());
    c.work_at(-1);
    assert!(take_before_origin());
    // Taking the mark clears it.
    assert!(!take_before_origin());
    c.start_at(-5);
    assert!(take_before_origin());
    c.finish_at(-5);
    assert!(take_before_origin());
    // A step back that would pass the origin is clamped and marked.
    c.sub(dd, 100 * 480);
    assert!(take_before_origin());
    c.cal.get_start_date(dd, 100 * 480);
    assert!(take_before_origin());
}
