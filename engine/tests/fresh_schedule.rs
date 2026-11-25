use openxer_engine::{
    calendar::{CalendarAdapter, WorkCalendar},
    model::{Activity, CstrType, Model, ProjectSettings},
    options::ScheduleOptions,
    scheduler::Scheduler,
};

fn model() -> Model {
    Model {
        origin_serial: 45000,
        project_calendar: 0,
        calendars: vec![CalendarAdapter::new(WorkCalendar::continuous(45000))],
        activities: vec![Activity {
            task_id: 1,
            task_code: "A".into(),
            rem: 60,
            ..Activity::default()
        }],
        links: vec![],
        project: ProjectSettings {
            data_date: Some(0),
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
fn remaining_dates_are_early_not_late() {
    let mut model = model();
    model.project.plan_end = Some(200);
    Scheduler::new(&mut model).schedule_now().unwrap();
    let task = &model.activities[0];
    assert_eq!((task.late_start, task.late_finish), (Some(140), Some(200)));
    assert_eq!((task.restart, task.reend), (Some(0), Some(60)));
}

#[test]
fn fresh_cpm_replaces_imported_remaining_dates() {
    let mut model = model();
    model.activities[0].rem_restart = Some(9990);
    model.activities[0].rem_reend = Some(9999);
    Scheduler::new(&mut model).schedule_now().unwrap();
    let task = &model.activities[0];
    assert_eq!((task.rem_restart, task.rem_reend), (Some(0), Some(60)));
}

#[test]
fn alap_consumes_free_float_once() {
    let mut model = model();
    model.activities[0].cstr = CstrType::Alap;
    model.activities.push(Activity {
        task_id: 2,
        task_code: "B".into(),
        rem: 180,
        ..Activity::default()
    });
    Scheduler::new(&mut model).schedule_now().unwrap();
    let task = &model.activities[0];
    assert_eq!(
        (task.early_start, task.early_finish),
        (Some(120), Some(180))
    );
    assert_eq!((task.restart, task.reend), (Some(120), Some(180)));
}
