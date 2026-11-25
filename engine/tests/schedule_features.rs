use openxer_engine::{
    features::{LevelPriority, LevelingInputs, PriorityField, ResourceDemand},
    fixture, scheduler,
};
use std::{collections::BTreeMap, fs, path::PathBuf};

#[test]
fn completed_virtual_level_dates_survive_output_cleanup() {
    use openxer_engine::{
        calendar::{CalendarAdapter, WorkCalendar},
        model::{Activity, Link, LinkType, StatusType, TaskType},
    };
    let Some(input) = fixture::fixture_path("features/paths-False.txt") else {
        return;
    };
    let mut model = fixture::parse_model(&fs::read_to_string(input).unwrap())
        .unwrap()
        .model;
    model.calendars = vec![CalendarAdapter::new(WorkCalendar::continuous(
        model.origin_serial,
    ))];
    model.project_calendar = 0;
    model.project.data_date = Some(1440);
    model.project.plan_end = None;
    model.options.multiple_float_paths = false;
    model.options.do_level = true;
    model.activities = (1..=4)
        .map(|i| Activity {
            task_id: i,
            task_code: format!("{}", char::from(b'A' + i as u8 - 1)),
            task_type: TaskType::Task,
            status: if i > 2 {
                StatusType::Completed
            } else {
                StatusType::NotStarted
            },
            rem: if i > 2 { 0 } else { 60 },
            act_start: (i > 2).then_some(10),
            act_finish: (i > 2).then_some(20),
            ..Activity::default()
        })
        .collect();
    model.links = vec![
        Link {
            pred: 1,
            succ: 2,
            link_type: LinkType::FS,
            lag: 0,
            driving_path: false,
        },
        Link {
            pred: 2,
            succ: 3,
            link_type: LinkType::FS,
            lag: 0,
            driving_path: false,
        },
    ];
    let mut resources = LevelingInputs {
        horizon: 4 * 1440,
        ..LevelingInputs::default()
    };
    resources.limits.insert(1, vec![(0, 1.0)]);
    for id in [1, 2] {
        resources.demands.insert(
            id,
            vec![ResourceDemand {
                resource_id: 1,
                rate: 1.0,
                remaining_units: 1.0,
                duration: 60,
                calendar: None,
            }],
        );
    }
    scheduler::schedule_with_resources(&mut model, &resources).unwrap();
    assert_eq!(
        (
            model.activities[1].rem_restart,
            model.activities[1].rem_reend
        ),
        (Some(1500), Some(1560))
    );
    for task in &model.activities[2..] {
        assert_eq!(
            (task.early_start, task.early_finish),
            (Some(1500), Some(1500))
        );
        assert_eq!((task.restart, task.reend), (None, None));
        assert_eq!((task.rem_restart, task.rem_reend), (Some(1560), Some(1560)));
    }
    let sched = scheduler::Scheduler::new(&mut model);
    assert_eq!(sched.relationship_free_float(1), Some(60));
    assert_eq!(sched.relationship_driving(1), Some(false));
}

/// Rule FEAT-1, FEAT-2 in `engine/RULES.md`.
#[test]
fn feature_fixture_differential() {
    let Some(root) = fixture::fixture_path("features") else {
        return;
    };
    let report = fixture::run_models(&root, "engine").unwrap();
    assert_eq!(report.exact, report.total, "{:?}", report.mismatches);
    assert!(report.total > 0);
}

#[test]
fn invalid_resource_only_calendars_are_rejected() {
    let Some(root) = fixture::fixture_path("features/paths-True.txt") else {
        return;
    };
    for variant in 0..4 {
        let mut model = fixture::parse_model(&fs::read_to_string(&root).unwrap())
            .unwrap()
            .model;
        model.options.do_level = true;
        let index = model.calendars.len();
        let mut resource_calendar = model.calendars[0].clone();
        match variant {
            0 => resource_calendar.cal.template[0] = vec![(480, 480)],
            1 => {
                resource_calendar.cal.continuous = false;
                resource_calendar.cal.template = Default::default();
            }
            2 => resource_calendar.cal.origin_serial += 1,
            _ => {}
        }
        model.calendars.push(resource_calendar);
        let mut inputs = LevelingInputs::default();
        inputs.demands.insert(
            1,
            vec![ResourceDemand {
                resource_id: 1,
                rate: 1.0,
                remaining_units: 1.0,
                duration: 60,
                calendar: Some(if variant == 3 { index + 1 } else { index }),
            }],
        );
        let before = model.activities[0].early_start;
        assert!(
            scheduler::schedule_with_resources(&mut model, &inputs).is_err(),
            "variant {variant}"
        );
        assert_eq!(model.activities[0].early_start, before);
    }
}

/// Rule LVL-1, LVL-2 in `engine/RULES.md`.
#[test]
fn native_leveling_matches_saved_probes() {
    let Some(root) = fixture::fixture_path("features/leveling") else {
        return;
    };
    let mut files: Vec<_> = fs::read_dir(root)
        .unwrap()
        .map(|p| p.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "level"))
        .collect();
    files.sort();
    assert_eq!(files.len(), 17, "All 801-817 probes must be present");
    let (dates, ties) = check_level_fixtures(files);
    assert_eq!((dates, ties), (301, 72));
}

/// Rule LVL-1 in `engine/RULES.md`.
#[test]
fn native_leveling_matches_full_saved_schedule() {
    let Some(root) = fixture::fixture_path("p6-bench/369.level") else {
        return;
    };
    let (dates, ties) = check_level_fixtures(vec![root]);
    assert_eq!((dates, ties), (408, 5841));
}

fn check_level_fixtures(files: Vec<PathBuf>) -> (usize, usize) {
    let mut count = 0;
    let mut tie_count = 0;
    let mut tie_exact = 0;
    let mut mismatches = vec![];
    for path in files {
        let mut dump =
            fixture::parse_model(&fs::read_to_string(path.with_extension("txt")).unwrap()).unwrap();
        let mut inputs = LevelingInputs::default();
        let mut expected = BTreeMap::new();
        let mut expected_ties = vec![];
        for line in fs::read_to_string(&path).unwrap().lines() {
            let v: Vec<_> = line.split_whitespace().collect();
            if v.is_empty() {
                continue;
            }
            let n = |k: usize| v[k].parse::<i64>().unwrap();
            match v[0] {
                "horizon" => inputs.horizon = n(1),
                "resource" => {
                    inputs.limits.entry(n(1)).or_default();
                }
                "limit" => inputs
                    .limits
                    .entry(n(1))
                    .or_default()
                    .push((n(2), v[3].parse().unwrap())),
                "rank" => {
                    inputs.activity_priority.insert(n(1), n(2));
                }
                "resource_dependent" => {
                    inputs.resource_dependent.insert(n(1));
                }
                "wbs" => {
                    let values = if v[2] == "-" {
                        vec![]
                    } else {
                        v[2].split(',')
                            .map(|x| {
                                let (a, b) = x.split_once(':').unwrap();
                                (a.parse().unwrap(), b.parse().unwrap())
                            })
                            .collect()
                    };
                    inputs.wbs_paths.insert(n(1), values);
                }
                "code" => {
                    inputs
                        .activity_codes
                        .insert((n(1), n(2)), fixture::decode(v[3]).unwrap());
                }
                "priority" => {
                    let field = match v[1] {
                        "priority_type" => PriorityField::ActivityPriority,
                        "total_float_hr_cnt" => PriorityField::TotalFloat,
                        "early_start_date" => PriorityField::EarlyStart,
                        "late_start_date" => PriorityField::LateStart,
                        "remain_drtn_hr_cnt" => PriorityField::RemainingDuration,
                        "wbs_id" => PriorityField::Wbs,
                        s if s.starts_with("code:") => {
                            PriorityField::ActivityCode(s[5..].parse().unwrap())
                        }
                        s => panic!("Unknown priority {s}"),
                    };
                    inputs.priorities.push(LevelPriority {
                        field,
                        ascending: v[2] == "1",
                    });
                }
                "demand" => {
                    let calendar = if v[6] == "-" {
                        None
                    } else {
                        Some(
                            dump.model
                                .calendars
                                .iter()
                                .position(|c| c.id == v[6])
                                .unwrap(),
                        )
                    };
                    inputs
                        .demands
                        .entry(n(1))
                        .or_default()
                        .push(ResourceDemand {
                            resource_id: n(2),
                            rate: v[3].parse().unwrap(),
                            remaining_units: v[4].parse().unwrap(),
                            duration: n(5),
                            calendar,
                        });
                }
                "expect_tie_level" => expected_ties.push((
                    n(1),
                    n(2),
                    fixture::parse_link(v[3]).unwrap(),
                    n(4),
                    v[5] == "1",
                )),
                "expect_level" => {
                    let d = |s: &str| {
                        if s == "-" {
                            None
                        } else {
                            Some(s.parse::<i64>().unwrap())
                        }
                    };
                    expected.insert(n(1), (d(v[2]), d(v[3])));
                }
                s => panic!("Unknown level fixture row {s}"),
            }
        }
        dump.model.options.do_level = true;
        scheduler::schedule_with_resources(&mut dump.model, &inputs)
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        // The independent full-schedule fixture holds the stored early and late
        // dates of the reference. Verify these after the leveled run too.
        if path.components().any(|p| p.as_os_str() == "p6-bench") {
            assert_eq!(dump.expected.len(), 3289);
            let mut fields = 0;
            for row in &dump.expected {
                let a = dump
                    .model
                    .activities
                    .iter()
                    .find(|a| a.task_code == row.key)
                    .unwrap();
                for (key, want) in &row.fields {
                    let actual = match key.as_str() {
                        "es" => a.early_start,
                        "ef" => a.early_finish,
                        "ls" => a.late_start,
                        "lf" => a.late_finish,
                        _ => panic!("Unexpected benchmark field {key}"),
                    };
                    assert_eq!(actual, *want, "{} {key}", a.task_code);
                    fields += 1;
                }
            }
            assert_eq!(fields, 13156);
        }
        let mut exact = 0;
        for a in &dump.model.activities {
            if let Some(&wanted) = expected.get(&a.task_id) {
                count += 1;
                if (a.restart, a.reend) == wanted {
                    exact += 1;
                } else {
                    mismatches.push(format!(
                        "{} {} stored={wanted:?} engine={:?}",
                        path.file_name().unwrap().to_string_lossy(),
                        a.task_code,
                        (a.restart, a.reend)
                    ));
                }
            }
        }
        let mut used = vec![false; dump.model.links.len()];
        for (p, s, ty, ff, driving) in expected_ties {
            tie_count += 1;
            let ri = dump
                .model
                .links
                .iter()
                .enumerate()
                .position(|(index, l)| {
                    !used[index]
                        && dump.model.activities[l.pred].task_id == p
                        && dump.model.activities[l.succ].task_id == s
                        && l.link_type == ty
                })
                .unwrap();
            used[ri] = true;
            let sched = scheduler::Scheduler::new(&mut dump.model);
            let actual = sched.relationship_free_float(ri);
            let actual_drive = sched.relationship_driving(ri);
            if actual == Some(ff) && actual_drive == Some(driving) {
                tie_exact += 1;
            } else {
                mismatches.push(format!(
                    "{} tie {p}->{s} stored={ff}/{driving} engine={actual:?}/{actual_drive:?}",
                    path.display()
                ));
            }
        }
        println!(
            "{}: {exact}/{} exact",
            path.file_name().unwrap().to_string_lossy(),
            expected.len()
        );
    }
    println!(
        "Saved leveling dates: {}/{count} activity pairs exact",
        count
    );
    println!("Saved leveled relationships: {tie_exact}/{tie_count} exact float/drive pairs");
    assert!(count > 0);
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
    (count, tie_count)
}
