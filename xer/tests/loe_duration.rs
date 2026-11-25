//! A scheduled Level of Effort stores its remaining duration from its dates;
//! while it is not started its original duration follows, once started it
//! stays what it was.
use openxer_engine::scheduler;
use openxer_xer::{starter, ProjectData};

fn loe_project(status: &str) -> ProjectData {
    let mut document = starter().document;
    let tasks = document.table_mut("TASK");
    let row = tasks
        .rows
        .iter()
        .position(|r| tasks.value(r, "task_id") == "1070")
        .expect("starter has the foundations activity");
    tasks.set(row, "task_type", "TT_LOE".into());
    tasks.set(row, "status_code", status.into());
    if status == "TK_Active" {
        tasks.set(row, "act_start_date", "2026-11-02 08:00".into());
    }
    ProjectData::from_document(document, 1, "loe".into()).unwrap()
}

fn cells(project: &mut ProjectData) -> (String, String) {
    scheduler::schedule_with_resources(&mut project.model, &project.resources).unwrap();
    project.write_assignment_schedule();
    let written = project.updated_document();
    let tasks = written.table("TASK").unwrap();
    let row = tasks
        .rows
        .iter()
        .find(|r| tasks.value(r, "task_id") == "1070")
        .unwrap();
    (
        tasks.value(row, "remain_drtn_hr_cnt").to_string(),
        tasks.value(row, "target_drtn_hr_cnt").to_string(),
    )
}

#[test]
fn not_started_loe_original_duration_follows_its_scheduled_span() {
    let mut project = loe_project("TK_NotStart");
    let (remaining, original) = cells(&mut project);
    let span = project.model.activities.iter().find(|a| a.task_id == 1070);
    let span = span.unwrap().rem;
    assert!(span > 0, "the LOE spans the gap between its neighbours");
    assert_eq!(remaining.parse::<f64>().unwrap() * 60., span as f64);
    assert_eq!(original, remaining);
    assert_ne!(original, "40", "no longer the imported 5 days");
}

#[test]
fn started_loe_keeps_its_original_duration() {
    let mut project = loe_project("TK_Active");
    let (remaining, original) = cells(&mut project);
    assert!(remaining.parse::<f64>().unwrap() > 0.);
    assert_eq!(original.parse::<f64>().unwrap(), 40.);
}
