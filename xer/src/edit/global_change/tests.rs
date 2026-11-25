use super::*;
use crate::edit::fields::tests::coded;

fn index_of(p: &ProjectData, code: &str) -> usize {
    p.model
        .activities
        .iter()
        .position(|a| a.task_code == code)
        .unwrap()
}

fn assignment(column: &str, value: &str, operator: &str, value2: &str) -> Assignment {
    Assignment {
        column: column.into(),
        value: value.into(),
        operator: operator.into(),
        value2: value2.into(),
    }
}

fn condition(column: &str, op: &str, value: &str) -> Condition {
    Condition {
        column: column.into(),
        op: op.into(),
        value: value.into(),
        ..Default::default()
    }
}

#[test]
fn operators_compute_in_the_field_kind() {
    let mut p = coded();
    let i = index_of(&p, "RB-1080");
    let host = ProjectHost::new(&mut p);
    let run = |c: &str, v: &str, o: &str, v2: &str| compute(&host, &assignment(c, v, o, v2), i);
    assert_eq!(
        run("physical_percent_complete", "10", "+", "5").unwrap(),
        "15"
    );
    assert_eq!(
        run("physical_percent_complete", "10", "x", "2.5").unwrap(),
        "25"
    );
    assert_eq!(run("activity_name", "A", "+", "B").unwrap(), "AB");
    assert!(run("physical_percent_complete", "10", "/", "0")
        .unwrap_err()
        .contains("zero"));
    assert!(run("activity_name", "A", "x", "2").is_err());
    let start = run("primary_constraint_date", "2026-11-02 08:00", "+", "3").unwrap();
    assert_eq!(start, "2026-11-05 08:00");
    // Fields named by title, durations in hours.
    assert_eq!(
        run("Original Duration", "Original Duration", "x", "2").unwrap(),
        "64h"
    );
}

#[test]
fn preview_lists_the_changes_and_commit_writes_the_kept_ones() {
    let mut p = coded();
    let before = p.clone();
    let g = GlobalChange {
        name: "Area north".into(),
        conditions: vec![condition("code:Area", "under", "N")],
        then: vec![
            assignment("original_duration", "Original Duration", "x", "2"),
            assignment("udf:Crew", "\"Night\"", "", ""),
        ],
        otherwise: vec![assignment("udf:Weight", "3", "", "")],
    };
    let n = p.model.activities.len();
    let changes = {
        let mut scratch = p.clone();
        let mut host = ProjectHost::new(&mut scratch);
        preview(&mut host, &g, &vec![true; n])
    };
    assert_eq!(p.model, before.model, "a preview works on a copy");
    // Then: RB-1080's duration and crew; Else: every other activity's weight.
    let then: Vec<(&str, &str, &str, &str)> = changes
        .iter()
        .filter(|c| c.activity == "RB-1080")
        .map(|c| {
            (
                c.title.as_str(),
                c.old.as_str(),
                c.new.as_str(),
                c.new_text.as_str(),
            )
        })
        .collect();
    assert_eq!(
        then,
        [
            ("Original Duration", "32h", "64h", "64h"),
            ("Crew", "", "Night", "Night")
        ]
    );
    assert_eq!(changes.len(), 2 + (n - 1), "{changes:#?}");
    assert!(changes.iter().all(|c| c.error.is_none()));
    let mut host = ProjectHost::new(&mut p);
    assert_eq!(commit(&mut host, &changes), (changes.len(), 0));
    let i = index_of(&p, "RB-1080");
    let task = p.model.activities[i].task_id;
    assert_eq!(p.details[&task].original_duration, 64 * 60);
}

#[test]
fn the_if_heading_joins_every_row_all_or_any() {
    let mut p = coded();
    let mut g = GlobalChange {
        name: "x".into(),
        conditions: vec![
            condition("activity_name", "contains", "install"),
            condition("wbs", "under", "RB.1"),
        ],
        ..Default::default()
    };
    assert_eq!(g.heading(), Some(true));
    let host = ProjectHost::new(&mut p);
    let install = index_of(host.project(), "RB-1080");
    let design = index_of(host.project(), "RB-1020");
    assert!(!matches(&host, &g, install), "All: RB-1080 is not in RB.1");
    g.set_heading(false);
    assert_eq!(g.heading(), Some(false));
    assert!(matches(&host, &g, install), "Any: its name holds");
    assert!(matches(&host, &g, design), "Any: RB-1020 is under RB.1");
    g.conditions.push(condition("activity_id", "ne", "X"));
    assert_eq!(g.heading(), None, "rows joined both ways");
}

#[test]
fn a_rule_given_as_json_takes_captions_and_a_match_heading() {
    let g = from_value(serde_json::json!({
        "match": "any",
        "conditions": [
            {"column": "WBS", "op": "is under", "value": "RB.3"},
            {"column": "Activity Name", "op": "contains", "value": "design"}
        ],
        "then": [{"column": "Original Duration", "value": "8"}]
    }))
    .unwrap();
    assert_eq!(g.name, "(Global Change)");
    assert_eq!(g.conditions[0].op, "under");
    assert_eq!(g.heading(), Some(false));
    assert!(from_value(serde_json::json!({"match": "some"})).is_err());
    assert!(from_value(serde_json::json!({"then": [{"column": "x", "operator": "%"}]})).is_err());
}
