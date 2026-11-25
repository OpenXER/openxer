//! Top-down estimation: WBS and activity weights split a labor-unit total,
//! the units land on activities and their labor assignments without moving
//! the schedule, and everything survives an XER round trip.
use openxer_engine::{model::StatusType, scheduler};
use openxer_xer::{starter, Document, ProjectData};
use std::collections::HashMap;

fn units(lines: &[(i64, f64)], task: i64) -> f64 {
    lines.iter().find(|(t, _)| *t == task).map_or(0., |l| l.1)
}

fn task_cell(p: &ProjectData, task: i64, column: &str) -> String {
    let t = p.document.table("TASK").unwrap();
    let r = t
        .rows
        .iter()
        .find(|r| t.value(r, "task_id") == task.to_string())
        .unwrap();
    t.value(r, column).to_string()
}

fn reimport(p: &ProjectData) -> ProjectData {
    let text = p.updated_document().text();
    ProjectData::from_document(Document::parse(&text).unwrap(), 1, "reimport".into()).unwrap()
}

fn schedule(p: &mut ProjectData) {
    p.refresh_derived_durations();
    scheduler::schedule_with_resources(&mut p.model, &p.resources).unwrap();
}

/// Early and late dates and remaining duration of each activity.
type Dates = (Option<i64>, Option<i64>, Option<i64>, Option<i64>, i64);

fn dates(p: &ProjectData) -> Vec<Dates> {
    p.model
        .activities
        .iter()
        .map(|a| {
            (
                a.early_start,
                a.early_finish,
                a.late_start,
                a.late_finish,
                a.rem,
            )
        })
        .collect()
}

#[test]
fn weights_split_the_total_down_the_wbs() {
    let mut p = starter();
    let none = HashMap::new();
    // Equal weights: four elements, then their tasks (milestones get none).
    let lines = p.top_down_distribution(1000., None, true, &none);
    assert_eq!(lines.len(), 11, "the eleven tasks; milestones are skipped");
    let sum: f64 = lines.iter().map(|l| l.1).sum();
    assert!((sum - 1000.).abs() < 1e-9);
    assert_eq!(units(&lines, 1010), 125.);
    assert_eq!(units(&lines, 1060), 50.);
    assert_eq!(units(&lines, 1000), 0.);
    // WBS weight 2 on RB.1 and activity weight 3 on 1010.
    assert!(p.set_wbs_estimation_weight(10, 2.));
    assert!(!p.set_wbs_estimation_weight(10, 2.), "unchanged");
    {
        let t = p.document.table_mut("TASK");
        let row = t
            .rows
            .iter()
            .position(|r| t.value(r, "task_id") == "1010")
            .unwrap();
        t.set(row, "est_wt", "3".into());
    }
    assert_eq!(p.wbs_estimation_weight(10), 2.);
    assert_eq!(p.wbs_estimation_weight(20), 1., "blank reads as 1");
    assert_eq!(p.activity_estimation_weight(1010), 3.);
    let lines = p.top_down_distribution(1000., None, true, &none);
    assert_eq!(units(&lines, 1010), 300.);
    assert_eq!(units(&lines, 1020), 100.);
    assert_eq!(units(&lines, 1040), 100.);
    assert_eq!(units(&lines, 1060), 40.);
    // Draft weights override the stored ones.
    let draft = HashMap::from([(10, 0.), (20, 0.), (30, 0.), (40, 1.)]);
    let lines = p.top_down_distribution(1000., None, true, &draft);
    assert_eq!(units(&lines, 1110), 500.);
    assert_eq!(units(&lines, 1010), 0.);
    // All zero: equal shares, nothing lost.
    let zero = HashMap::from([(10, 0.), (20, 0.), (30, 0.), (40, 0.)]);
    let lines = p.top_down_distribution(1000., None, true, &zero);
    assert!((lines.iter().map(|l| l.1).sum::<f64>() - 1000.).abs() < 1e-9);
    // One element only.
    let lines = p.top_down_distribution(90., Some(30), true, &none);
    assert_eq!(lines.len(), 5);
    assert_eq!(units(&lines, 1080), 18.);
    // Started activities are left out unless asked for.
    let i = p
        .model
        .activities
        .iter()
        .position(|a| a.task_id == 1010)
        .unwrap();
    p.model.activities[i].status = StatusType::InProgress;
    let lines = p.top_down_distribution(1000., Some(10), true, &none);
    assert_eq!(lines, vec![(1020, 1000.)]);
    let lines = p.top_down_distribution(1000., Some(10), false, &none);
    assert_eq!(units(&lines, 1010), 750.);
    assert!(p.top_down_distribution(0., None, true, &none).is_empty());
    assert!(p
        .top_down_distribution(10., Some(999), true, &none)
        .is_empty());
}

#[test]
fn estimates_write_units_keep_the_schedule_and_round_trip() {
    let mut p = starter();
    p.document.table_mut("RSRC").add(&[
        ("rsrc_id", "10".into()),
        ("rsrc_name", "Pipe crew".into()),
        ("rsrc_short_name", "PIPE".into()),
        ("rsrc_type", "RT_Labor".into()),
        ("def_qty_per_hr", "1".into()),
        ("clndr_id", "1".into()),
    ]);
    p.document.table_mut("RSRCRATE").add(&[
        ("rsrc_id", "10".into()),
        ("start_date", "2020-01-01 00:00".into()),
        ("max_qty_per_hr", "4".into()),
        ("cost_per_qty", "50".into()),
    ]);
    // Two crews on 1060 (two days): 16 units each at 1 per hour.
    let a = p.add_assignment(1060, Some(10), None).unwrap();
    let b = p.add_assignment(1060, Some(10), None).unwrap();
    schedule(&mut p);
    let before = dates(&p);
    let rem_before: Vec<i64> = p
        .model
        .activities
        .iter()
        .flat_map(|a| a.assignments.iter().map(|x| x.rem))
        .collect();
    p.set_wbs_estimation_weight(30, 3.);
    let lines = p.top_down_distribution(600., None, true, &HashMap::new());
    let changed = p.apply_labor_estimate(&lines);
    assert_eq!(changed, 11);
    // Element weights 1, 1, 3, 1: RB.3 gets 300, 60 a task.
    assert_eq!(task_cell(&p, 1060, "target_work_qty"), "60");
    assert_eq!(task_cell(&p, 1060, "remain_work_qty"), "60");
    assert_eq!(task_cell(&p, 1010, "target_work_qty"), "50");
    let cell = |p: &ProjectData, row: usize, k: &str| {
        let t = p.document.table("TASKRSRC").unwrap();
        t.value(&t.rows[row], k).to_string()
    };
    for row in [a, b] {
        assert_eq!(cell(&p, row, "target_qty"), "30");
        assert_eq!(cell(&p, row, "remain_qty"), "30");
        assert_eq!(cell(&p, row, "target_qty_per_hr"), "1.875");
        assert_eq!(cell(&p, row, "remain_qty_per_hr"), "1.875");
        assert_eq!(cell(&p, row, "target_cost"), "1500.0000");
    }
    // Durations and the schedule do not move.
    let rem_after: Vec<i64> = p
        .model
        .activities
        .iter()
        .flat_map(|a| a.assignments.iter().map(|x| x.rem))
        .collect();
    assert_eq!(rem_after, rem_before);
    let mut again = p.clone();
    schedule(&mut again);
    assert_eq!(dates(&again), before);
    // A second run with the same lines changes nothing.
    assert_eq!(p.apply_labor_estimate(&lines), 0);
    // Weights and units survive export and reimport.
    let back = reimport(&p);
    assert_eq!(back.wbs_estimation_weight(30), 3.);
    assert_eq!(task_cell(&back, 1060, "target_work_qty"), "60");
    assert_eq!(task_cell(&back, 1110, "remain_work_qty"), "50");
    let rows = back.assignment_rows(1060);
    assert_eq!(cell(&back, rows[0], "target_qty"), "30");
    let mut back = back;
    schedule(&mut back);
    assert_eq!(dates(&back), before);
}

#[test]
fn started_activities_keep_their_actual_units() {
    let mut p = starter();
    {
        let t = p.document.table_mut("TASK");
        let row = t
            .rows
            .iter()
            .position(|r| t.value(r, "task_id") == "1020")
            .unwrap();
        t.set(row, "act_work_qty", "30".into());
    }
    let applied = p.apply_labor_estimate(&[(1020, 100.), (1010, 10.)]);
    assert_eq!(applied, 2);
    assert_eq!(task_cell(&p, 1020, "target_work_qty"), "100");
    assert_eq!(task_cell(&p, 1020, "remain_work_qty"), "70");
    p.apply_labor_estimate(&[(1020, 20.)]);
    assert_eq!(
        task_cell(&p, 1020, "remain_work_qty"),
        "0",
        "not below zero"
    );
}
