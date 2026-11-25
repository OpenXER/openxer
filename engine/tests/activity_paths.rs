//! Activity critical paths: backward paths are the multiple float paths with
//! the chosen activity as the ending activity; forward paths follow the most
//! critical successors. The schedule itself never changes (rule FEAT-2 in
//! `engine/RULES.md`).
use openxer_engine::{
    calendar::{CalendarAdapter, WorkCalendar},
    features::activity_paths,
    model::{Activity, Link, LinkType, Model, ProjectSettings, StatusType, TaskType},
    options::ScheduleOptions,
    scheduler,
};

/// A -> B -> D -> F, A -> C -> D (C shorter), D -> E (short tail), all FS.
fn model() -> Model {
    let origin = 45000;
    let mut model = Model {
        origin_serial: origin,
        project_calendar: 0,
        calendars: vec![CalendarAdapter::new(WorkCalendar::continuous(origin))],
        activities: vec![],
        links: vec![],
        project: ProjectSettings {
            proj_id: 1,
            data_date: Some(1440),
            ..ProjectSettings::default()
        },
        // Open ends float to the project finish, so the short tail E does.
        options: ScheduleOptions {
            open_end_uses_early: false,
            ..ScheduleOptions::default()
        },
    };
    let durations = [60, 300, 120, 60, 30, 240];
    model.activities = durations
        .iter()
        .enumerate()
        .map(|(i, rem)| Activity {
            task_id: i as i64 + 1,
            task_code: format!("{}", char::from(b'A' + i as u8)),
            task_type: TaskType::Task,
            status: StatusType::NotStarted,
            rem: *rem,
            ..Activity::default()
        })
        .collect();
    let link = |pred: usize, succ: usize| Link {
        pred,
        succ,
        link_type: LinkType::FS,
        lag: 0,
        driving_path: false,
    };
    model.links = vec![
        link(0, 1),
        link(0, 2),
        link(1, 3),
        link(2, 3),
        link(3, 4),
        link(3, 5),
    ];
    model.rebuild_links().unwrap();
    scheduler::schedule(&mut model).unwrap();
    model
}

fn codes(m: &Model, paths: &[Vec<usize>]) -> Vec<String> {
    paths
        .iter()
        .map(|p| {
            p.iter()
                .map(|&i| m.activities[i].task_code.as_str())
                .collect::<Vec<_>>()
                .join("")
        })
        .collect()
}

/// Rule FEAT-2 in `engine/RULES.md`.
#[test]
fn backward_paths_match_the_float_path_columns_for_the_ending_activity() {
    let m = model();
    let before = m.clone();
    let paths = activity_paths(&m, 3, false, 3).unwrap();
    assert_eq!(m, before, "computing paths leaves the schedule alone");
    // The same numbers the scheduler writes into the Float Path columns.
    let mut scheduled = m.clone();
    scheduled.options.multiple_float_paths = true;
    scheduled.options.float_paths_end_activity = Some(4);
    scheduled.options.max_float_paths = 3;
    scheduler::schedule(&mut scheduled).unwrap();
    for (n, path) in paths.iter().enumerate() {
        for (order, &i) in path.iter().enumerate() {
            assert_eq!(scheduled.activities[i].float_path, n as i64 + 1);
            assert_eq!(scheduled.activities[i].float_path_order, order as i64 + 1);
        }
    }
    let on_paths: usize = paths.iter().map(Vec::len).sum();
    let numbered = scheduled
        .activities
        .iter()
        .filter(|a| a.float_path > 0)
        .count();
    assert_eq!(on_paths, numbered);
    assert_eq!(codes(&m, &paths[..1]), ["ABD"]);
    assert!(codes(&m, &paths).concat().contains('C'));
    // Fewer paths keep the first ones.
    assert_eq!(activity_paths(&m, 3, false, 1).unwrap(), paths[..1]);
}

/// Rule FEAT-2 in `engine/RULES.md`.
#[test]
fn forward_paths_start_at_the_activity_and_take_the_driving_successor_first() {
    let m = model();
    let paths = activity_paths(&m, 0, true, 5).unwrap();
    assert_eq!(codes(&m, &paths)[0], "ABDF");
    let all: String = codes(&m, &paths).concat();
    for code in ["C", "E"] {
        assert!(
            all.contains(code),
            "{code} is on a later forward path: {all}"
        );
    }
    assert_eq!(activity_paths(&m, 0, true, 1).unwrap().len(), 1);
    // The tail activity leads nowhere: one path of itself.
    assert_eq!(codes(&m, &activity_paths(&m, 5, true, 3).unwrap()), ["F"]);
    assert!(activity_paths(&m, 0, true, 0).is_err());
}
