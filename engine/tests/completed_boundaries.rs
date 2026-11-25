use openxer_engine::{
    calendar::{CalendarAdapter, WorkCalendar},
    model::{Activity, CstrType, Link, LinkType, Model, ProjectSettings, StatusType, TaskType},
    options::ScheduleOptions,
    scheduler::Scheduler,
};
use std::collections::BTreeMap;

fn completed_pair(kind: TaskType) -> Model {
    Model {
        origin_serial: 45000,
        project_calendar: 0,
        calendars: vec![CalendarAdapter::new(WorkCalendar::new(
            "7x12".into(),
            false,
            45000,
            std::array::from_fn(|_| vec![(420, 1140)]),
            BTreeMap::new(),
        ))],
        activities: vec![
            Activity {
                task_id: 1,
                task_code: "P".into(),
                status: StatusType::Completed,
                act_start: Some(-1000),
                act_finish: Some(-1),
                restart: Some(360),
                reend: Some(360),
                succs: vec![0],
                ..Activity::default()
            },
            Activity {
                task_id: 2,
                task_code: "F".into(),
                task_type: kind,
                status: StatusType::Completed,
                act_start: Some(-1000),
                act_finish: Some(-1),
                preds: vec![0],
                ..Activity::default()
            },
        ],
        links: vec![Link {
            pred: 0,
            succ: 1,
            link_type: LinkType::FF,
            lag: 0,
            driving_path: false,
        }],
        project: ProjectSettings {
            data_date: Some(360),
            plan_end: Some(6 * 1440),
            ..ProjectSettings::default()
        },
        options: ScheduleOptions {
            retained_logic: true,
            open_end_uses_early: false,
            ..ScheduleOptions::default()
        },
    }
}

#[test]
fn completed_finish_milestone_keeps_data_date_outside_working_hours() {
    let mut model = completed_pair(TaskType::FinMile);
    Scheduler::new(&mut model).schedule_forward_task(1);
    assert_eq!(
        (model.activities[1].restart, model.activities[1].reend),
        (Some(360), Some(360))
    );
}

#[test]
fn completed_late_dates_keep_nonworking_limit() {
    let mut model = completed_pair(TaskType::Task);
    Scheduler::new(&mut model).schedule_backward_task(1);
    assert_eq!(
        (model.activities[1].restart, model.activities[1].reend),
        (Some(6 * 1440), Some(6 * 1440))
    );
}

#[test]
fn completed_relationship_lag_keeps_original_activity_calendar() {
    let mut model = completed_pair(TaskType::FinMile);
    let scheduler = Scheduler::new(&mut model);
    assert_eq!(scheduler.lag_calendar(0).id, "7x12");
    assert!(!scheduler.lag_calendar(0).is_work(360));
}

#[test]
fn exhausted_actual_lag_does_not_snap_completed_predecessor_to_next_shift() {
    let mut model = completed_pair(TaskType::Task);
    model.links[0].lag = 60;
    model.project.data_date = Some(1800);
    model.activities[0].restart = Some(1800);
    model.activities[0].reend = Some(1800);
    model.activities[0].act_finish = Some(0);
    assert_eq!(Scheduler::new(&mut model).apply_lag_forward(0), Some(1800));
}

/// Rule REL-1 in `engine/RULES.md`.
#[test]
fn milestone_relationships_use_the_endpoint_that_exists() {
    use LinkType::{FF, FS, SF, SS};
    use TaskType::{FinMile, Mile, Task};
    // REL-1. Columns: raw FS, SS, FF, SF.
    for (pred, succ, expected) in [
        (Task, Task, [FS, SS, FF, SF]),
        (Task, Mile, [FS, SS, FS, SS]),
        (Task, FinMile, [FF, SF, FF, SF]),
        (Mile, Task, [SS, SS, SF, SF]),
        (Mile, Mile, [SS, SS, SS, SS]),
        (Mile, FinMile, [SF, SF, SF, SF]),
        (FinMile, Task, [FS, FS, FF, FF]),
        (FinMile, Mile, [FS, FS, FS, FS]),
        (FinMile, FinMile, [FF, FF, FF, FF]),
    ] {
        for (raw, want) in [FS, SS, FF, SF].into_iter().zip(expected) {
            let mut model = completed_pair(succ);
            model.activities[0].task_type = pred;
            model.links[0].link_type = raw;
            assert_eq!(
                Scheduler::new(&mut model).get_logical_dep_type(0),
                want,
                "{pred:?} -> {succ:?}, raw {raw:?}"
            );
        }
    }
}

#[test]
fn primary_mandatory_successor_constraint_suppresses_relationship_driving() {
    for constraint in [CstrType::MandatoryStart, CstrType::MandatoryFinish] {
        let mut model = completed_pair(TaskType::Task);
        for activity in &mut model.activities {
            activity.early_start = Some(360);
            activity.early_finish = Some(360);
        }
        // Only the primary constraint suppresses driving (CON-8).
        model.activities[1].cstr2 = constraint;
        assert_eq!(
            Scheduler::new(&mut model).relationship_driving(0),
            Some(true)
        );
        model.activities[1].cstr = constraint;
        assert_eq!(
            Scheduler::new(&mut model).relationship_driving(0),
            Some(false)
        );
    }
}

fn admin_boundary_model(enabled: bool) -> Model {
    let mut model = completed_pair(TaskType::FinMile);
    model.calendars[0] = CalendarAdapter::new(WorkCalendar::new(
        "daily".into(),
        false,
        45000,
        std::array::from_fn(|_| vec![(480, 1020)]),
        BTreeMap::new(),
    ));
    model.options = ScheduleOptions {
        finish_on_valid_time: enabled,
        ..ScheduleOptions::default()
    };
    model.project.data_date = Some(0);
    for task in &mut model.activities {
        task.status = StatusType::NotStarted;
        task.act_start = None;
        task.act_finish = None;
    }
    model.activities[0].restart = Some(1980);
    model.activities[0].reend = Some(1980);
    model.links[0].lag = -60;
    model
}

/// Rule ADM-1 in `engine/RULES.md`.
#[test]
fn negative_finish_boundary_uses_admin_setting_independently_of_retained_logic() {
    for enabled in [false, true] {
        for kind in [LinkType::FF, LinkType::SF] {
            let mut model = admin_boundary_model(enabled);
            model.links[0].link_type = kind;
            let (start, _, finish) = Scheduler::new(&mut model).get_succ_restart_date(1);
            let expected = Some(if enabled { 1020 } else { 1920 });
            assert_eq!((start, finish), (expected, expected));
        }
    }
}

/// Rule ADM-1 in `engine/RULES.md`.
#[test]
fn zero_work_finish_gap_respects_admin_setting() {
    for enabled in [false, true] {
        let mut model = admin_boundary_model(enabled);
        model.activities[1].reend = Some(1020);
        Scheduler::new(&mut model).adjust_for_non_work_finish_time_from_dependency(
            1,
            LinkType::FF,
            Some(1920),
        );
        assert_eq!(
            model.activities[1].reend,
            Some(if enabled { 1020 } else { 1920 })
        );
    }
}

/// Rule ADM-1 in `engine/RULES.md`.
#[test]
fn admin_setting_does_not_roll_back_lunch_boundary() {
    let mut model = admin_boundary_model(true);
    model.calendars[0] = CalendarAdapter::new(WorkCalendar::new(
        "lunch".into(),
        false,
        45000,
        std::array::from_fn(|_| vec![(480, 660), (690, 1020)]),
        BTreeMap::new(),
    ));
    model.activities[1].reend = Some(660);
    Scheduler::new(&mut model).adjust_for_non_work_finish_time_from_dependency(
        1,
        LinkType::FF,
        Some(690),
    );
    assert_eq!(model.activities[1].reend, Some(690));
}
