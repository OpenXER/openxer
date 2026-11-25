use openxer_engine::{fixture, scheduler};
use std::{collections::BTreeMap, fs};

#[test]
fn independent_relationship_candidates_and_negative_lag_milestones() {
    let Some(root) = fixture::fixture_path("p6-relationships") else {
        return;
    };
    let mut files: Vec<_> = fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "answers"))
        .collect();
    files.sort();
    assert_eq!(files.len(), 4);
    let mut relationships = 0;
    let mut activity_dates = 0;
    for path in files {
        let mut model =
            fixture::parse_model(&fs::read_to_string(path.with_extension("txt")).unwrap())
                .unwrap()
                .model;
        scheduler::schedule(&mut model).unwrap();
        let by_id: BTreeMap<_, _> = model
            .activities
            .iter()
            .enumerate()
            .map(|(i, a)| (a.task_id, i))
            .collect();
        let engine = scheduler::Scheduler::new(&mut model);
        for line in fs::read_to_string(&path).unwrap().lines() {
            let fields: Vec<_> = line.split_whitespace().collect();
            let number = |i: usize| fields[i].parse::<i64>().unwrap();
            match fields[0] {
                "tie" => {
                    let index = engine
                        .model
                        .links
                        .iter()
                        .position(|link| {
                            engine.model.activities[link.pred].task_id == number(1)
                                && engine.model.activities[link.succ].task_id == number(2)
                                && link.link_type.as_str() == fields[3]
                                && link.lag == number(4)
                        })
                        .unwrap();
                    assert_eq!(
                        (
                            engine.relationship_free_float(index),
                            engine.relationship_driving(index)
                        ),
                        (Some(number(5)), Some(number(6) == 1)),
                        "{}: {}",
                        path.display(),
                        line
                    );
                    relationships += 1;
                }
                "date" => {
                    let task = &engine.model.activities[by_id[&number(1)]];
                    assert_eq!(
                        (
                            task.early_start,
                            task.early_finish,
                            task.late_start,
                            task.late_finish
                        ),
                        (
                            Some(number(2)),
                            Some(number(3)),
                            Some(number(4)),
                            Some(number(5))
                        ),
                        "{}: {}",
                        path.display(),
                        task.task_code
                    );
                    activity_dates += 4;
                }
                _ => panic!("Unknown independent answer row: {line}"),
            }
        }
    }
    assert_eq!(relationships, 1500);
    assert_eq!(activity_dates, 14400);
}
