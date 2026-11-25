//! Several projects scheduled as one network (`scheduler::schedule_together`,
//! rule PASS-9 in `engine/RULES.md`) and moving a model to another origin
//! (`Model::rebase`).
use openxer_engine::{
    calendar::{CalendarAdapter, WorkCalendar, DAY},
    fixture,
    model::{Activity, Link, LinkType, Model, ProjectSettings},
    options::ScheduleOptions,
    scheduler::{self, Partition},
};
use std::collections::BTreeMap;

/// Mon-Fri 08:00-12:00 and 13:00-17:00, counted from `origin` (a day serial).
fn weekdays(origin: i64) -> CalendarAdapter {
    // Serial 0 (1899-12-30) is a Saturday; the template starts on `origin`'s weekday.
    let week: [Vec<(i64, i64)>; 7] = std::array::from_fn(|i| {
        let weekday = (origin + i as i64 + 6).rem_euclid(7); // 0 = Sunday
        if (1..=5).contains(&weekday) {
            vec![(480, 720), (780, 1020)]
        } else {
            vec![]
        }
    });
    let mut exc = BTreeMap::new();
    exc.insert(origin + 23, vec![]); // a holiday
    CalendarAdapter::new(WorkCalendar::new(
        "Standard".into(),
        false,
        origin,
        week,
        exc,
    ))
}

fn task(id: i64, rem_days: i64) -> Activity {
    Activity {
        task_id: id,
        task_code: format!("A{id}"),
        rem: rem_days * 480,
        ..Activity::default()
    }
}

fn link(pred: usize, succ: usize, kind: LinkType, lag: i64) -> Link {
    Link {
        pred,
        succ,
        link_type: kind,
        lag,
        driving_path: false,
    }
}

fn model(origin: i64, activities: Vec<Activity>, links: Vec<Link>, data_date: i64) -> Model {
    let mut m = Model {
        origin_serial: origin,
        project_calendar: 0,
        calendars: vec![weekdays(origin)],
        activities,
        links,
        project: ProjectSettings {
            proj_id: 1,
            name: "P".into(),
            data_date: Some(data_date),
            ..ProjectSettings::default()
        },
        options: ScheduleOptions {
            retained_logic: true,
            lag_from_early_start: true,
            use_expected_finish: true,
            open_end_uses_early: false,
            out_of_sequence: openxer_engine::options::OutOfSequence::Both,
            ..ScheduleOptions::default()
        },
    };
    m.rebuild_links().unwrap();
    m
}

/// Monday 2026-11-02 08:00 as minutes from origin serial `origin`.
fn monday(origin: i64) -> i64 {
    (46328 - origin) * DAY + 480
}

#[test]
fn rebasing_keeps_every_absolute_date() {
    let origin = 46300;
    let dd = monday(origin);
    let mut a = model(
        origin,
        vec![task(1, 3), task(2, 7), task(3, 2)],
        vec![link(0, 1, LinkType::FS, 480), link(1, 2, LinkType::SS, 960)],
        dd,
    );
    a.activities[2].ext_late_end = Some(dd + 40 * DAY);
    let mut b = a.clone();
    b.rebase(origin - 10);
    assert_eq!(b.project.data_date, Some(dd + 10 * DAY));
    scheduler::schedule(&mut a).unwrap();
    scheduler::schedule(&mut b).unwrap();
    b.rebase(origin);
    assert_eq!(a.activities, b.activities);
    assert_eq!(a.project, b.project);
    assert_eq!(a.calendars, b.calendars);
}

/// Rule PASS-9 in `engine/RULES.md`: a one-project partition is the plain
/// schedule, on every fixture model.
#[test]
fn one_project_partition_is_the_plain_schedule() {
    let Some(root) = fixture::fixture_path("") else {
        return;
    };
    let mut checked = 0;
    for dir in ["models", "regressions", "features"] {
        let Ok(entries) = std::fs::read_dir(root.join(dir)) else {
            continue;
        };
        let mut paths: Vec<_> = entries.flatten().map(|e| e.path()).collect();
        paths.retain(|p| p.extension().is_some_and(|e| e == "txt"));
        paths.sort();
        for path in paths {
            let text = std::fs::read_to_string(&path).unwrap();
            let Ok(dump) = fixture::parse_model(&text) else {
                continue;
            };
            let mut plain = dump.model.clone();
            if plain.options.do_level || scheduler::schedule(&mut plain).is_err() {
                continue;
            }
            let mut together = dump.model.clone();
            let partition = Partition {
                of: vec![0; together.activities.len()],
                projects: vec![together.project.clone()],
                ..Partition::default()
            };
            let out = scheduler::schedule_together(&mut together, partition).unwrap();
            assert_eq!(plain, together, "{}", path.display());
            assert_eq!(
                out.projects[0].scd_end,
                plain.project.scd_end,
                "{}",
                path.display()
            );
            assert!(out.external_start.iter().all(Option::is_none));
            checked += 1;
        }
    }
    eprintln!("{checked} fixture models checked");
}

/// Project A: A1 -> A2. Project B: B1 -> B2. Cross link A2 -> B1 (FS).
fn pair(dd_a: i64, dd_b: i64) -> (Model, Partition) {
    let origin = 46300;
    let mut m = model(
        origin,
        vec![task(1, 2), task(2, 3), task(11, 1), task(12, 2)],
        vec![
            link(0, 1, LinkType::FS, 0),
            link(2, 3, LinkType::FS, 0),
            link(1, 2, LinkType::FS, 0),
        ],
        dd_a.min(dd_b),
    );
    m.rebuild_links().unwrap();
    let a = ProjectSettings {
        proj_id: 1,
        name: "A".into(),
        data_date: Some(dd_a),
        ..ProjectSettings::default()
    };
    let b = ProjectSettings {
        proj_id: 2,
        name: "B".into(),
        data_date: Some(dd_b),
        ..ProjectSettings::default()
    };
    let partition = Partition {
        of: vec![0, 0, 1, 1],
        projects: vec![a, b],
        ..Partition::default()
    };
    (m, partition)
}

/// Rule PASS-9 in `engine/RULES.md`.
#[test]
fn cross_project_finish_to_start_drives_the_successor() {
    let dd = monday(46300);
    let (mut m, partition) = pair(dd, dd);
    let out = scheduler::schedule_together(&mut m, partition).unwrap();
    let a2 = &m.activities[1];
    let b1 = &m.activities[2];
    // A1 Mon-Tue, A2 Wed-Fri, B1 the next Monday.
    assert_eq!(a2.early_finish, Some(dd + 4 * DAY + 540));
    assert_eq!(b1.early_start, Some(dd + 7 * DAY));
    // The stored limit is the relationship's own date (A2's finish); the
    // forward pass places the start on B1's calendar from it.
    assert_eq!(out.external_start[2], a2.early_finish);
    assert_eq!(out.external_start[0], None);
    // Each project's finish is its own (float on each project).
    assert_eq!(out.projects[0].scd_end, a2.early_finish);
    assert_eq!(out.projects[1].scd_end, m.activities[3].early_finish);
    // A2's late finish is held by B1's late start, and stored as external.
    assert_eq!(out.external_finish[1], b1.late_start);
    assert_eq!(a2.late_finish, Some(dd + 4 * DAY + 540));
    assert_eq!(a2.total_float, Some(0));
    // Each project has a longest path.
    assert!(a2.driving_path && m.activities[3].driving_path);
}

/// Rule PASS-9 in `engine/RULES.md`.
#[test]
fn each_project_keeps_its_own_data_date() {
    let dd = monday(46300);
    // B's data date is two weeks later than A's finish.
    let (mut m, partition) = pair(dd, dd + 14 * DAY);
    let out = scheduler::schedule_together(&mut m, partition).unwrap();
    assert_eq!(m.activities[0].early_start, Some(dd));
    assert_eq!(m.activities[2].early_start, Some(dd + 14 * DAY));
    // The relationship's limit is still stored: A2's finish.
    assert_eq!(out.external_start[2], Some(dd + 4 * DAY + 540));
}

/// Rule PASS-9 in `engine/RULES.md`.
#[test]
fn stored_external_dates_reproduce_the_combined_run() {
    let dd = monday(46300);
    let (mut together, partition) = pair(dd, dd);
    let out = scheduler::schedule_together(&mut together, partition).unwrap();
    // Project B alone, with A closed: its stored external early start.
    let mut b = model(
        46300,
        vec![task(11, 1), task(12, 2)],
        vec![link(0, 1, LinkType::FS, 0)],
        dd,
    );
    b.activities[0].ext_early_start = out.external_start[2];
    scheduler::schedule(&mut b).unwrap();
    assert_eq!(
        b.activities[0].early_start,
        together.activities[2].early_start
    );
    assert_eq!(
        b.activities[1].early_finish,
        together.activities[3].early_finish
    );
    // Ignoring relationships to other projects drops the limit.
    b.options.out_of_sequence = openxer_engine::options::OutOfSequence::None;
    scheduler::schedule(&mut b).unwrap();
    assert_eq!(b.activities[0].early_start, Some(dd));
}

/// Rule PASS-9 in `engine/RULES.md`.
#[test]
fn partition_input_is_checked() {
    let dd = monday(46300);
    let (mut m, mut partition) = pair(dd, dd);
    partition.of.pop();
    assert!(scheduler::schedule_together(&mut m, partition).is_err());
    let (mut m, partition) = pair(dd, dd);
    m.options.do_level = true;
    assert!(scheduler::schedule_together(&mut m, partition).is_err());
}
