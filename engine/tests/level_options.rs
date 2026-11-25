//! Leveling options: leveling only within float, the float to preserve, the
//! resources chosen for leveling, and Resource Dependent work scheduled with
//! and without leveling. Rules LVL-3, LVL-4 and RES-3 in `engine/RULES.md`.
//!
//! These rules were written from what the options mean, not from stored
//! output of the reference scheduler: the tests pin the rules as stated.
//!
//! Days are counted from a Monday origin; all work is on a Monday to Friday
//! 08:00-16:00 calendar (480 working minutes a day), the data date is Monday
//! 08:00.
use openxer_engine::{
    calendar::{CalendarAdapter, WorkCalendar},
    features::{LevelingInputs, ResourceDemand},
    fixture,
    model::{Activity, Assignment, Model, ProjectSettings, TaskType},
    options::ScheduleOptions,
    scheduler,
};
use std::collections::{BTreeMap, BTreeSet};

const DAY: i64 = 1440;
const WORK_DAY: i64 = 480;
const ORIGIN: i64 = 44998;

fn at(day: i64, hour: i64) -> i64 {
    day * DAY + hour * 60
}

fn calendar() -> CalendarAdapter {
    CalendarAdapter::new(WorkCalendar::new(
        "short".into(),
        false,
        ORIGIN,
        std::array::from_fn(|i| if i < 5 { vec![(480, 960)] } else { vec![] }),
        BTreeMap::new(),
    ))
}

fn leveled_options() -> ScheduleOptions {
    ScheduleOptions {
        retained_logic: true,
        open_end_uses_early: false,
        do_level: true,
        ..ScheduleOptions::default()
    }
}

fn task(id: i64, code: &str, days: i64) -> Activity {
    Activity {
        task_id: id,
        task_code: code.into(),
        rem: days * WORK_DAY,
        ..Activity::default()
    }
}

fn model(activities: Vec<Activity>) -> Model {
    let mut m = Model {
        origin_serial: ORIGIN,
        project_calendar: 0,
        calendars: vec![calendar()],
        activities,
        links: vec![],
        project: ProjectSettings {
            data_date: Some(at(0, 8)),
            ..ProjectSettings::default()
        },
        options: leveled_options(),
    };
    m.rebuild_links().unwrap();
    m
}

/// One unit of resource `r` each hour, needed by the activities `ids` for
/// their whole duration. The resource has a limit of one unit.
fn crew(model: &Model, inputs: &mut LevelingInputs, r: i64, ids: &[i64]) {
    inputs.limits.insert(r, vec![(0, 1.0)]);
    for a in model.activities.iter().filter(|a| ids.contains(&a.task_id)) {
        inputs
            .demands
            .entry(a.task_id)
            .or_default()
            .push(ResourceDemand {
                resource_id: r,
                rate: 1.0,
                remaining_units: (a.rem / 60) as f64,
                duration: a.rem,
                calendar: None,
            });
    }
}

fn inputs() -> LevelingInputs {
    LevelingInputs {
        horizon: 120 * DAY,
        ..LevelingInputs::default()
    }
}

/// A and B (two days each) share one crew; C is an unrelated run of
/// `longest` days that sets the project finish and so the float of A and B.
fn two_on_one_crew(longest: i64) -> (Model, LevelingInputs) {
    let m = model(vec![
        task(1, "A", 2),
        task(2, "B", 2),
        task(3, "C", longest),
    ]);
    let mut i = inputs();
    crew(&m, &mut i, 1, &[1, 2]);
    (m, i)
}

fn dates(m: &Model, id: i64) -> (Option<i64>, Option<i64>) {
    let a = m.activities.iter().find(|a| a.task_id == id).unwrap();
    (a.restart, a.reend)
}

/// Working minutes between an activity's leveled finish and its late finish.
fn float_left(m: &Model, id: i64) -> i64 {
    let a = m.activities.iter().find(|a| a.task_id == id).unwrap();
    m.calendars[a.cal].between(a.reend.unwrap(), a.late_finish.unwrap())
}

/// Rule LVL-3 in `engine/RULES.md`.
#[test]
fn leveling_without_the_float_option_delays_past_the_float() {
    // C ends Wednesday, so B has one day of float; plain leveling still
    // moves B behind A.
    let (mut m, i) = two_on_one_crew(3);
    let held = scheduler::schedule_with_resources_report(&mut m, &i).unwrap();
    assert!(held.is_empty());
    assert_eq!(dates(&m, 1), (Some(at(0, 8)), Some(at(1, 16))));
    assert_eq!(dates(&m, 2), (Some(at(2, 8)), Some(at(3, 16))));
}

/// Rule LVL-3 in `engine/RULES.md`.
#[test]
fn within_float_an_activity_is_delayed_while_its_float_allows() {
    // C ends Friday: B's late finish is Friday 16:00, three days after its
    // early finish, and the delay behind A ends Thursday 16:00.
    let (mut m, mut i) = two_on_one_crew(5);
    i.within_float = true;
    let held = scheduler::schedule_with_resources_report(&mut m, &i).unwrap();
    assert!(held.is_empty());
    assert_eq!(dates(&m, 2), (Some(at(2, 8)), Some(at(3, 16))));
    assert_eq!(float_left(&m, 2), WORK_DAY);
}

/// Rule LVL-3 in `engine/RULES.md`.
#[test]
fn within_float_an_activity_without_enough_float_stays_and_is_listed() {
    // C ends Wednesday: B may finish no later than Wednesday 16:00, but the
    // first free time ends Thursday 16:00. B stays beside A and the crew is
    // over-allocated on Monday and Tuesday.
    let (mut m, mut i) = two_on_one_crew(3);
    i.within_float = true;
    let held = scheduler::schedule_with_resources_report(&mut m, &i).unwrap();
    assert_eq!(held, vec![2]);
    assert_eq!(dates(&m, 1), (Some(at(0, 8)), Some(at(1, 16))));
    assert_eq!(dates(&m, 2), (Some(at(0, 8)), Some(at(1, 16))));
    // Early and late dates are untouched.
    let b = m.activities.iter().find(|a| a.task_id == 2).unwrap();
    assert_eq!(
        (b.early_start, b.early_finish),
        (Some(at(0, 8)), Some(at(1, 16)))
    );
    assert_eq!(b.late_finish, Some(at(2, 16)));
}

/// Rule LVL-3 in `engine/RULES.md`.
#[test]
fn preserving_two_days_of_float_keeps_two_days() {
    // C runs six days and ends Monday of the next week. Behind A, B would
    // finish Thursday and keep Friday and Monday: exactly two days.
    let run = |preserve_days: i64| {
        let (mut m, mut i) = two_on_one_crew(6);
        i.within_float = true;
        i.preserve_float = preserve_days * WORK_DAY;
        let held = scheduler::schedule_with_resources_report(&mut m, &i).unwrap();
        (m, held)
    };
    let (m, held) = run(2);
    assert!(held.is_empty());
    assert_eq!(dates(&m, 2), (Some(at(2, 8)), Some(at(3, 16))));
    assert_eq!(float_left(&m, 2), 2 * WORK_DAY);
    // Asking for three days of float rules the delay out.
    let (m, held) = run(3);
    assert_eq!(held, vec![2]);
    assert_eq!(dates(&m, 2), (Some(at(0, 8)), Some(at(1, 16))));
    // Zero, one and two days all allow it.
    for days in [0, 1] {
        let (m, held) = run(days);
        assert!(held.is_empty());
        assert_eq!(dates(&m, 2), (Some(at(2, 8)), Some(at(3, 16))));
    }
}

/// Rule LVL-3 in `engine/RULES.md`.
#[test]
fn the_float_to_preserve_is_read_only_when_leveling_within_float() {
    let (mut m, mut i) = two_on_one_crew(3);
    i.preserve_float = 5 * WORK_DAY;
    scheduler::schedule_with_resources(&mut m, &i).unwrap();
    assert_eq!(dates(&m, 2), (Some(at(2, 8)), Some(at(3, 16))));
}

/// Rule LVL-3 in `engine/RULES.md`.
#[test]
fn a_delay_follows_the_late_dates_down_a_chain() {
    // B1 -> B2 on the crew behind A; B2 may not finish after Wednesday.
    let mut m = model(vec![
        task(1, "A", 2),
        task(2, "B1", 2),
        task(4, "B2", 1),
        task(3, "C", 3),
    ]);
    m.links = vec![openxer_engine::model::Link {
        pred: 1,
        succ: 2,
        link_type: openxer_engine::model::LinkType::FS,
        lag: 0,
        driving_path: false,
    }];
    m.rebuild_links().unwrap();
    let mut i = inputs();
    crew(&m, &mut i, 1, &[1, 2]);
    i.within_float = true;
    let held = scheduler::schedule_with_resources_report(&mut m, &i).unwrap();
    // B1 has no float of its own to spend (its late finish is Tuesday 16:00
    // through B2), so it is listed and stays; B2 follows it.
    assert_eq!(held, vec![2]);
    assert_eq!(dates(&m, 2), (Some(at(0, 8)), Some(at(1, 16))));
    assert_eq!(dates(&m, 4), (Some(at(2, 8)), Some(at(2, 16))));
}

/// Rule LVL-4 in `engine/RULES.md`.
#[test]
fn leveling_all_resources_equals_selecting_every_resource() {
    let build = || {
        let m = model(vec![
            task(1, "A", 2),
            task(2, "B", 2),
            task(3, "C", 2),
            task(4, "D", 2),
        ]);
        let mut i = inputs();
        crew(&m, &mut i, 1, &[1, 2]);
        crew(&m, &mut i, 2, &[3, 4]);
        (m, i)
    };
    let (mut all, i) = build();
    scheduler::schedule_with_resources(&mut all, &i).unwrap();
    let (mut every, mut i) = build();
    i.selected = Some(BTreeSet::from([1, 2]));
    scheduler::schedule_with_resources(&mut every, &i).unwrap();
    assert_eq!(all, every);
    assert_eq!(dates(&all, 2), (Some(at(2, 8)), Some(at(3, 16))));
    assert_eq!(dates(&all, 4), (Some(at(2, 8)), Some(at(3, 16))));
}

/// Rule LVL-4 in `engine/RULES.md`.
#[test]
fn a_resource_outside_the_selection_is_not_leveled() {
    let build = || {
        let m = model(vec![
            task(1, "A", 2),
            task(2, "B", 2),
            task(3, "C", 2),
            task(4, "D", 2),
        ]);
        let mut i = inputs();
        crew(&m, &mut i, 1, &[1, 2]);
        crew(&m, &mut i, 2, &[3, 4]);
        (m, i)
    };
    let (mut chosen, mut i) = build();
    i.selected = Some(BTreeSet::from([1]));
    scheduler::schedule_with_resources(&mut chosen, &i).unwrap();
    // Resource 1 is leveled, resource 2 is booked against no limit.
    assert_eq!(dates(&chosen, 2), (Some(at(2, 8)), Some(at(3, 16))));
    assert_eq!(dates(&chosen, 4), (Some(at(0, 8)), Some(at(1, 16))));
    // The same as a project in which resource 2 has no limit at all.
    let (mut unlimited, mut i) = build();
    i.limits.remove(&2);
    scheduler::schedule_with_resources(&mut unlimited, &i).unwrap();
    assert_eq!(chosen, unlimited);
    // An empty selection levels nothing.
    let (mut none, mut i) = build();
    i.selected = Some(BTreeSet::new());
    scheduler::schedule_with_resources(&mut none, &i).unwrap();
    assert_eq!(dates(&none, 2), (Some(at(0, 8)), Some(at(1, 16))));
}

/// Two crews on their own calendars: a Resource Dependent activity of 12
/// hours each for the crews on the Monday to Friday 08:00-16:00 calendar
/// (calendar 0) and on a calendar that works 08:00-12:00 (calendar 1).
fn resource_dependent(options: ScheduleOptions) -> Model {
    let half = CalendarAdapter::new(WorkCalendar::new(
        "mornings".into(),
        false,
        ORIGIN,
        std::array::from_fn(|i| if i < 5 { vec![(480, 720)] } else { vec![] }),
        BTreeMap::new(),
    ));
    let mut r = task(1, "R", 2);
    r.task_type = TaskType::Resource;
    r.assignments = vec![
        Assignment {
            rsrc_id: 1,
            rem: 12 * 60,
            cal: Some(0),
            ..Assignment::default()
        },
        Assignment {
            rsrc_id: 2,
            rem: 12 * 60,
            cal: Some(1),
            ..Assignment::default()
        },
    ];
    let mut after = task(2, "S", 1);
    after.assignments = vec![];
    let mut m = model(vec![r, after]);
    m.calendars.push(half);
    m.links = vec![openxer_engine::model::Link {
        pred: 0,
        succ: 1,
        link_type: openxer_engine::model::LinkType::FS,
        lag: 0,
        driving_path: false,
    }];
    m.options = options;
    m.rebuild_links().unwrap();
    m
}

/// Rule RES-3 in `engine/RULES.md`.
#[test]
fn resource_dependent_work_is_scheduled_on_resource_calendars_without_leveling() {
    let mut plain = resource_dependent(ScheduleOptions {
        do_level: false,
        ..leveled_options()
    });
    scheduler::schedule(&mut plain).unwrap();
    let r = &plain.activities[0];
    // The mornings crew needs three days of 4 hours: it finishes Wednesday
    // 12:00, after the day crew's Tuesday 12:00.
    assert_eq!((r.restart, r.reend), (Some(at(0, 8)), Some(at(2, 12))));
    // The successor on the day calendar starts where the crews finish.
    assert_eq!(plain.activities[1].restart, Some(at(2, 12)));

    // The leveled run with nothing over-allocated gives the same dates.
    let mut leveled = resource_dependent(leveled_options());
    let mut i = inputs();
    i.resource_dependent.insert(1);
    i.limits.insert(1, vec![(0, 5.0)]);
    i.limits.insert(2, vec![(0, 5.0)]);
    for (r, cal) in [(1, 0), (2, 1)] {
        i.demands.entry(1).or_default().push(ResourceDemand {
            resource_id: r,
            rate: 1.0,
            remaining_units: 12.0,
            duration: 12 * 60,
            calendar: Some(cal),
        });
    }
    scheduler::schedule_with_resources(&mut leveled, &i).unwrap();
    for k in 0..2 {
        let (a, b) = (&plain.activities[k], &leveled.activities[k]);
        assert_eq!(
            (a.early_start, a.early_finish, a.late_start, a.late_finish),
            (b.early_start, b.early_finish, b.late_start, b.late_finish)
        );
        assert_eq!((a.restart, a.reend), (b.restart, b.reend));
        assert_eq!((a.total_float, a.free_float), (b.total_float, b.free_float));
    }
}

/// One `demand` line of a leveling fixture: activity and its demand.
type FixtureDemand = (i64, ResourceDemand);

fn read_demand_lines(text: &str, model: &Model) -> (BTreeSet<i64>, Vec<FixtureDemand>) {
    let mut dependent = BTreeSet::new();
    let mut demands = vec![];
    for line in text.lines() {
        let v: Vec<_> = line.split_whitespace().collect();
        match v.first().copied() {
            Some("resource_dependent") => {
                dependent.insert(v[1].parse().unwrap());
            }
            Some("demand") => {
                let calendar = (v[6] != "-")
                    .then(|| model.calendars.iter().position(|c| c.id == v[6]).unwrap());
                demands.push((
                    v[1].parse().unwrap(),
                    ResourceDemand {
                        resource_id: v[2].parse().unwrap(),
                        rate: v[3].parse().unwrap(),
                        remaining_units: v[4].parse().unwrap(),
                        duration: v[5].parse().unwrap(),
                        calendar,
                    },
                ));
            }
            _ => {}
        }
    }
    (dependent, demands)
}

/// Rule RES-3 in `engine/RULES.md`, on the saved Resource Dependent probes.
#[test]
fn saved_resource_dependent_probes_give_the_same_dates_with_and_without_leveling() {
    let Some(root) = fixture::fixture_path("features/leveling") else {
        return;
    };
    let mut compared = 0;
    for name in [
        "814-level-rsrc-dependent-377",
        "816-level-rsrc-dependent-2-377",
    ] {
        let dump = std::fs::read_to_string(root.join(format!("{name}.txt"))).unwrap();
        let levels = std::fs::read_to_string(root.join(format!("{name}.level"))).unwrap();
        let parsed = fixture::parse_model(&dump).unwrap().model;
        let (dependent, demands) = read_demand_lines(&levels, &parsed);
        let mut inputs = LevelingInputs {
            horizon: 4000 * DAY,
            resource_dependent: dependent,
            ..LevelingInputs::default()
        };
        // No limits at all: nothing can be over-allocated.
        for (task, demand) in demands {
            inputs.demands.entry(task).or_default().push(demand);
        }
        let mut plain = parsed.clone();
        plain.options.do_level = false;
        scheduler::schedule_with_resources(&mut plain, &inputs).unwrap();
        let mut leveled = parsed.clone();
        leveled.options.do_level = true;
        scheduler::schedule_with_resources(&mut leveled, &inputs).unwrap();
        for (a, b) in plain.activities.iter().zip(&leveled.activities) {
            if a.task_type != TaskType::Resource {
                continue;
            }
            assert_eq!(
                (a.restart, a.reend),
                (b.restart, b.reend),
                "{name} {}",
                a.task_code
            );
            compared += 1;
        }
    }
    assert!(
        compared >= 10,
        "only {compared} Resource Dependent activities"
    );
}
