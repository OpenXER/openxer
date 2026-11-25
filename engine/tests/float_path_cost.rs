//! Multiple float paths rank each relationship once per run (rule FEAT-2 in
//! `engine/RULES.md`). A run asking for hundreds of paths with no ending
//! activity used to rank every relationship again for every path, so a
//! project of a few thousand activities on a calendar with many holidays took
//! minutes. Synthetic data only.
use openxer_engine::{
    calendar::{CalendarAdapter, WorkCalendar, DAY},
    model::{Activity, Link, LinkType, Model, ProjectSettings, StatusType, TaskType},
    options::ScheduleOptions,
    scheduler,
};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

const ORIGIN: i64 = 45000;
const CHAINS: usize = 1500;
const ACTIVITIES: usize = 3 * CHAINS + 1;
const PATHS: usize = 500;

/// Weekdays 08:00 to 17:00 with a holiday every ninth day for about ten years.
fn calendar() -> CalendarAdapter {
    let day = vec![(8 * 60, 17 * 60)];
    let template: [Vec<(i64, i64)>; 7] = std::array::from_fn(|i| {
        if (1..=5).contains(&i) {
            day.clone()
        } else {
            vec![]
        }
    });
    let holidays: BTreeMap<i64, Vec<(i64, i64)>> =
        (0..400).map(|k| (ORIGIN + 9 * k, vec![])).collect();
    CalendarAdapter::new(WorkCalendar::new(
        "Weekdays with holidays".into(),
        false,
        ORIGIN,
        template,
        holidays,
    ))
}

/// Many short parallel chains of three activities, each with its own
/// durations and lag, all finishing into one last activity: every chain has
/// its own float, so each becomes a float path of its own.
fn model(use_total: bool) -> Model {
    let mut model = Model {
        origin_serial: ORIGIN,
        project_calendar: 0,
        calendars: vec![calendar()],
        activities: vec![],
        links: vec![],
        project: ProjectSettings {
            proj_id: 1,
            // Well after the origin, so no run reaches before it.
            data_date: Some(30 * DAY + 8 * 60),
            ..ProjectSettings::default()
        },
        options: ScheduleOptions {
            multiple_float_paths: true,
            float_paths_use_total: use_total,
            max_float_paths: PATHS,
            float_paths_end_activity: None,
            ..ScheduleOptions::default()
        },
    };
    model.activities = (0..ACTIVITIES)
        .map(|i| Activity {
            task_id: i as i64 + 1,
            task_code: format!("A{:05}", (i * 7919) % 100_000),
            task_type: TaskType::Task,
            status: StatusType::NotStarted,
            rem: 60 * (8 + (i as i64 * 37) % 40),
            ..Activity::default()
        })
        .collect();
    let link = |pred: usize, succ: usize, lag: i64| Link {
        pred,
        succ,
        link_type: LinkType::FS,
        lag,
        driving_path: false,
    };
    let last = ACTIVITIES - 1;
    for c in 0..CHAINS {
        let first = 3 * c;
        model.links.push(link(first, first + 1, 0));
        model
            .links
            .push(link(first + 1, first + 2, 60 * (c as i64 % 5)));
        model.links.push(link(first + 2, last, 0));
    }
    model.rebuild_links().unwrap();
    model
}

fn check(use_total: bool) {
    let mut m = model(use_total);
    let started = Instant::now();
    scheduler::schedule(&mut m).unwrap();
    let took = started.elapsed();
    // Before relationships were ranked once per run this took minutes (each of
    // the 500 paths ranked all 4,500 relationships with calendar arithmetic).
    assert!(
        took < Duration::from_secs(30),
        "scheduling with {PATHS} float paths took {took:?}"
    );
    // Every path is numbered 1, 2, ... with its activities in order 1, 2, ...
    let mut paths: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
    for a in &m.activities {
        if a.float_path > 0 {
            paths
                .entry(a.float_path)
                .or_default()
                .push(a.float_path_order);
        }
    }
    assert!(paths.len() > 1, "several float paths");
    assert!(paths.len() <= PATHS);
    for (n, (path, mut orders)) in paths.into_iter().enumerate() {
        assert_eq!(path, n as i64 + 1, "paths are numbered without gaps");
        orders.sort_unstable();
        assert_eq!(orders, (1..=orders.len() as i64).collect::<Vec<_>>());
    }
    assert!(m.activities.iter().all(|a| a.early_start.is_some()));
}

/// Rule FEAT-2 in `engine/RULES.md`.
#[test]
fn many_total_float_paths_without_an_ending_activity_finish_promptly() {
    check(true);
}

/// Rule FEAT-2 in `engine/RULES.md`.
#[test]
fn many_free_float_paths_without_an_ending_activity_finish_promptly() {
    check(false);
}
