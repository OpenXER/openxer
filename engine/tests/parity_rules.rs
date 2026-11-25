//! Rules found by comparing fresh schedules with the dates the reference
//! scheduler stored in real exports: a loop that
//! closes only through a level of effort, level-of-effort remaining dates
//! after leveling, a completed predecessor's lag in the backward pass and
//! into completed work, and the leveled longest path through completed work.
//! `engine/RULES.md` lists the evidence for each rule.
//!
//! Days are counted from a Monday origin: 0 Monday ... 4 Friday. Dates are
//! minutes from origin midnight. All work is on a Monday to Friday
//! 07:00-17:00 calendar (600 working minutes a day).
use openxer_engine::{
    calendar::{CalendarAdapter, WorkCalendar},
    features::{LevelingInputs, ResourceDemand},
    model::{Activity, Link, LinkType, Model, ProjectSettings, StatusType, TaskType},
    options::ScheduleOptions,
    scheduler,
};
use std::collections::BTreeMap;

const DAY: i64 = 1440;
const WORK_DAY: i64 = 600;
// Serial 44998 is a Monday, so template index 0 is Monday.
const ORIGIN: i64 = 44998;

fn at(day: i64, hour: i64, minute: i64) -> i64 {
    day * DAY + hour * 60 + minute
}

fn calendar() -> CalendarAdapter {
    CalendarAdapter::new(WorkCalendar::new(
        "5x10".into(),
        false,
        ORIGIN,
        std::array::from_fn(|i| if i < 5 { vec![(420, 1020)] } else { vec![] }),
        BTreeMap::new(),
    ))
}

fn retained() -> ScheduleOptions {
    ScheduleOptions {
        retained_logic: true,
        open_end_uses_early: false,
        ..ScheduleOptions::default()
    }
}

fn model(activities: Vec<Activity>, links: Vec<Link>, options: ScheduleOptions) -> Model {
    let mut m = Model {
        origin_serial: ORIGIN,
        project_calendar: 0,
        calendars: vec![calendar()],
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

fn loe(id: i64, code: &str) -> Activity {
    Activity {
        task_type: TaskType::Loe,
        ..task(id, code, 0)
    }
}

fn completed(id: i64, code: &str, start: i64, finish: i64) -> Activity {
    Activity {
        status: StatusType::Completed,
        act_start: Some(start),
        act_finish: Some(finish),
        ..task(id, code, 0)
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

/// Rule PASS-10 in `engine/RULES.md`.
#[test]
fn a_loop_that_closes_only_through_a_level_of_effort_is_scheduled() {
    // A then B; the LOE is tied backwards: B finish-to-finish into it and it
    // start-to-start into A. A -> B -> LOE -> A is a loop only through the
    // LOE, which takes no part in the passes.
    let links = vec![
        tie(0, 1, LinkType::FS, 0),
        tie(1, 2, LinkType::FF, 0),
        tie(2, 0, LinkType::SS, 0),
    ];
    let mut m = model(
        vec![task(1, "A", WORK_DAY), task(2, "B", WORK_DAY), loe(3, "L")],
        links.clone(),
        retained(),
    );
    scheduler::schedule(&mut m).unwrap();
    let [a, b, l] = [&m.activities[0], &m.activities[1], &m.activities[2]];
    assert_eq!(
        (a.early_start, a.early_finish),
        (Some(at(0, 7, 0)), Some(at(0, 17, 0)))
    );
    assert_eq!(
        (b.early_start, b.early_finish),
        (Some(at(1, 7, 0)), Some(at(1, 17, 0)))
    );
    // The LOE spans A's start to B's finish whatever way its ties point.
    assert_eq!(
        (l.early_start, l.early_finish),
        (a.early_start, b.early_finish)
    );

    // A loop through ordinary work is still refused.
    let mut links = links;
    links.push(tie(1, 0, LinkType::FS, 0));
    let mut m = model(
        vec![task(1, "A", WORK_DAY), task(2, "B", WORK_DAY), loe(3, "L")],
        links,
        retained(),
    );
    let error = scheduler::schedule(&mut m).unwrap_err();
    assert!(error.contains("circular relationship"), "{error}");
    assert!(
        !error.contains('L'),
        "the loop is named without the LOE: {error}"
    );
}

/// Rule FEAT-3 in `engine/RULES.md`.
#[test]
fn after_leveling_a_level_of_effort_spans_its_partners_leveled_dates() {
    // A and B (one day each) share a crew of one; the leveler moves one of
    // them. The LOE is tied backwards (B finish-to-finish into it, it
    // start-to-start into A), outside the start-to-start / finish-to-finish
    // shape of PASS-6. Its remaining dates follow A's leveled start and B's
    // leveled finish.
    let links = vec![tie(1, 2, LinkType::FF, 0), tie(2, 0, LinkType::SS, 0)];
    let mut m = model(
        vec![task(1, "A", WORK_DAY), task(2, "B", WORK_DAY), loe(3, "L")],
        links,
        ScheduleOptions {
            do_level: true,
            ..retained()
        },
    );
    let mut inputs = LevelingInputs {
        horizon: 120 * DAY,
        ..LevelingInputs::default()
    };
    inputs.limits.insert(1, vec![(0, 1.0)]);
    for id in [1, 2] {
        inputs.demands.entry(id).or_default().push(ResourceDemand {
            resource_id: 1,
            rate: 1.0,
            remaining_units: 10.0,
            duration: WORK_DAY,
            calendar: None,
        });
    }
    scheduler::schedule_with_resources_report(&mut m, &inputs).unwrap();
    let [a, b, l] = [&m.activities[0], &m.activities[1], &m.activities[2]];
    // One of the two waits a day for the crew.
    assert_ne!(a.restart, b.restart);
    assert_eq!(a.restart.max(b.restart), Some(at(1, 7, 0)));
    assert_eq!((l.restart, l.reend), (a.restart, b.reend));
    // Its early dates still span the unleveled partners.
    assert_eq!(
        (l.early_start, l.early_finish),
        (Some(at(0, 7, 0)), Some(at(0, 17, 0)))
    );
}

/// The completed predecessor P finished Monday 12:00 and the data date is
/// Tuesday 07:00, so 300 working minutes of a 600-minute lag are used up.
fn completed_lag_model(succ: Activity, kind: LinkType, early_start_lag: bool) -> Model {
    let mut m = model(
        vec![
            completed(1, "P", at(0, 7, 0), at(0, 12, 0)),
            succ,
            task(3, "END", 2 * WORK_DAY),
        ],
        vec![tie(0, 1, kind, WORK_DAY)],
        ScheduleOptions {
            lag_from_early_start: early_start_lag,
            ..retained()
        },
    );
    m.project.data_date = Some(at(1, 7, 0));
    m
}

/// Rule BWD-6 in `engine/RULES.md`.
#[test]
fn backward_a_completed_predecessors_finish_lag_counts_only_its_unused_part() {
    for kind in [LinkType::FS, LinkType::FF] {
        // With start lags measured from the early start, P's late finish is
        // the successor's limit less the 300 minutes of lag still to run.
        let mut m = completed_lag_model(task(2, "S", WORK_DAY), kind, true);
        scheduler::schedule(&mut m).unwrap();
        let (p, s) = (&m.activities[0], &m.activities[1]);
        // END runs Tuesday and Wednesday and sets the late limit.
        assert_eq!(s.late_finish, Some(at(2, 17, 0)));
        let limit = if kind == LinkType::FS {
            s.late_start.unwrap()
        } else {
            s.late_finish.unwrap()
        };
        let expected = m.calendars[0].sub(limit, 300);
        assert_eq!(p.late_finish, Some(expected), "{kind:?}");

        // With start-to-start lag measured from the actual start the same
        // holds (saved probes 304 and 1206).
        let mut m = completed_lag_model(task(2, "S", WORK_DAY), kind, false);
        scheduler::schedule(&mut m).unwrap();
        let (p, s) = (&m.activities[0], &m.activities[1]);
        let limit = if kind == LinkType::FS {
            s.late_start.unwrap()
        } else {
            s.late_finish.unwrap()
        };
        assert_eq!(
            p.late_finish,
            Some(m.calendars[0].sub(limit, 300)),
            "{kind:?}"
        );
    }
}

/// Rule REL-14 in `engine/RULES.md`.
#[test]
fn a_completed_predecessors_unused_lag_does_not_move_completed_work() {
    // Into open work the 300 minutes left of the lag still count (REL-5):
    // S may finish no earlier than Tuesday 12:00.
    let mut m = completed_lag_model(task(2, "S", 0), LinkType::FF, true);
    scheduler::schedule(&mut m).unwrap();
    assert_eq!(m.activities[1].early_finish, Some(at(1, 12, 0)));
    // A completed successor stays at the data date.
    let mut m = completed_lag_model(
        completed(2, "S", at(0, 7, 0), at(0, 12, 0)),
        LinkType::FF,
        true,
    );
    scheduler::schedule(&mut m).unwrap();
    let s = &m.activities[1];
    assert_eq!(
        (s.early_start, s.early_finish),
        (Some(at(1, 7, 0)), Some(at(1, 7, 0)))
    );
}

/// Rule FLT-7 in `engine/RULES.md` (leveled run).
#[test]
fn the_leveled_longest_path_runs_back_into_completed_work() {
    // M (a start milestone completed last week) ties start-to-start into A,
    // the only open work. After leveling, the longest path is A and M.
    let mut m_act = completed(1, "M", at(0, 7, 0) - 7 * DAY, at(0, 7, 0) - 7 * DAY);
    m_act.task_type = TaskType::Mile;
    let mut m = model(
        vec![m_act, task(2, "A", WORK_DAY)],
        vec![tie(0, 1, LinkType::SS, 0)],
        ScheduleOptions {
            do_level: true,
            ..retained()
        },
    );
    let inputs = LevelingInputs {
        horizon: 120 * DAY,
        ..LevelingInputs::default()
    };
    scheduler::schedule_with_resources_report(&mut m, &inputs).unwrap();
    assert!(m.activities[1].driving_path);
    assert!(m.activities[0].driving_path);
}

/// One crew of limit 1 needed by every listed activity for its whole duration.
fn one_crew(m: &Model, ids: &[i64], within_float: bool) -> LevelingInputs {
    let mut inputs = LevelingInputs {
        horizon: 120 * DAY,
        within_float,
        ..LevelingInputs::default()
    };
    inputs.limits.insert(1, vec![(0, 1.0)]);
    for a in m.activities.iter().filter(|a| ids.contains(&a.task_id)) {
        inputs
            .demands
            .entry(a.task_id)
            .or_default()
            .push(ResourceDemand {
                resource_id: 1,
                rate: 1.0,
                remaining_units: (a.rem / 60) as f64,
                duration: a.rem,
                calendar: None,
            });
    }
    inputs.priorities = vec![openxer_engine::features::LevelPriority {
        field: openxer_engine::features::PriorityField::TotalFloat,
        ascending: true,
    }];
    inputs
}

/// Rule LVL-5 in `engine/RULES.md` (fitted to two exports; saved probes
/// 1201/1202 are to confirm it).
#[test]
fn leveling_within_float_breaks_ties_by_internal_id_not_activity_id() {
    // Three one-day activities with the same float share one crew. Their
    // internal ids run Z, M, A; their Activity IDs run A, M, Z. A ten-day
    // run sets the finish, so every one has room to wait.
    let build = || {
        model(
            vec![
                task(1, "Z", WORK_DAY),
                task(2, "M", WORK_DAY),
                task(3, "A", WORK_DAY),
                task(4, "LONG", 10 * WORK_DAY),
            ],
            vec![],
            ScheduleOptions {
                do_level: true,
                ..retained()
            },
        )
    };
    let order = |within: bool| {
        let mut m = build();
        let inputs = one_crew(&m, &[1, 2, 3], within);
        scheduler::schedule_with_resources_report(&mut m, &inputs).unwrap();
        let mut by_start: Vec<_> = m.activities[..3]
            .iter()
            .map(|a| (a.restart.unwrap(), a.task_code.clone()))
            .collect();
        by_start.sort();
        by_start.into_iter().map(|(_, c)| c).collect::<Vec<_>>()
    };
    assert_eq!(order(true), ["Z", "M", "A"]);
    assert_eq!(order(false), ["A", "M", "Z"]);
}

/// Rule LVL-6 in `engine/RULES.md`.
#[test]
fn leveling_within_float_moves_the_project_finish_to_the_leveled_finish() {
    // A and B (one day each) share a crew and must finish by Friday, so B
    // can wait a day within its float. The scheduled finish becomes B's
    // leveled finish, Tuesday 17:00; no early finish meets it, so no
    // activity is on the longest path. Without the float option the finish
    // stays at the latest early finish, Monday 17:00, where the path ends.
    let run = |within: bool| {
        let mut m = model(
            vec![task(1, "A", WORK_DAY), task(2, "B", WORK_DAY)],
            vec![],
            ScheduleOptions {
                do_level: true,
                use_must_finish: true,
                ..retained()
            },
        );
        m.project.plan_end = Some(at(4, 17, 0));
        let inputs = one_crew(&m, &[1, 2], within);
        scheduler::schedule_with_resources_report(&mut m, &inputs).unwrap();
        m
    };
    let m = run(true);
    assert_eq!(m.project.scd_end, Some(at(1, 17, 0)));
    assert!(m.activities.iter().all(|a| !a.driving_path));
    let m = run(false);
    assert_eq!(m.project.scd_end, Some(at(0, 17, 0)));
    assert!(m.activities.iter().any(|a| a.driving_path));
}

/// Rule RES-4 in `engine/RULES.md`.
#[test]
fn round_the_clock_hours_are_added_as_day_numbers() {
    // The reference's own answers: 0.38 units at 0.16 per hour is 142.5
    // minutes. Added to 2019-05-20 07:00 or 2026-11-17 09:00 it gives :22, to
    // 2019-07-05 17:00 it gives 19:23 (a private weekly export, saved probe
    // 1203).
    let hours = 0.38 / 0.16;
    for (serial, minute, expected) in [
        (43605, 420, 420 + 142),
        (46343, 540, 540 + 142),
        (43651, 1020, 1020 + 143),
    ] {
        let mut m = model(vec![task(1, "A", 0)], vec![], retained());
        m.origin_serial = serial;
        let s = openxer_engine::scheduler::Scheduler::new(&mut m);
        assert_eq!(s.clock_shift(minute, hours), expected, "{serial} {minute}");
    }
}
