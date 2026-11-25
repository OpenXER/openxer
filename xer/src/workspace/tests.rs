//! The workspace EPS rules and the rollups, on a synthetic two-project
//! workspace whose expected values are worked out by hand in the comments.
use super::rollup::{self, Rollup};
use super::sample::{project, project_a, project_b, task};
use super::*;
use crate::parse_absolute;

fn at(s: &str) -> Option<i64> {
    parse_absolute(s)
}

#[test]
fn a_project_rolls_up_by_hand_computed_values() {
    let a = rollup::of_document(&project_a(), 1).unwrap();
    assert_eq!(a.start, at("2027-01-04 08:00"));
    assert_eq!(a.finish, at("2027-01-22 17:00"));
    assert_eq!(a.late_start, at("2027-01-19 08:00"));
    assert_eq!(a.late_finish, at("2027-01-25 17:00"));
    assert_eq!(a.data_date, at("2027-01-13 08:00"));
    assert_eq!(a.must_finish, at("2027-01-29 17:00"));
    assert_eq!(a.actual_finish, None, "not every activity is done");
    // The lowest float of the unfinished activities: 8 h.
    assert_eq!(a.total_float, Some(8 * 60));
    assert_eq!((a.not_started, a.in_progress, a.completed), (1, 1, 1));
    assert_eq!(a.activities, 3);
    // Labor units: only resource 1. Costs: 4000 + 500 + 1000 budgeted,
    // 4000 + 100 actual, 0 + 400 + 1000 remaining.
    assert_eq!(
        (
            a.labor_units.budgeted,
            a.labor_units.actual,
            a.labor_units.remaining
        ),
        (40., 40., 0.)
    );
    assert_eq!(
        (a.costs.budgeted, a.costs.actual, a.costs.remaining),
        (5500., 4100., 1400.)
    );
    // Planned span: Mon 4-Jan 08:00 to Fri 22-Jan 17:00, three working weeks
    // = 120 h. Remaining span: Wed 13-Jan 08:00 to Fri 22-Jan 17:00, three
    // days and a week = 64 h. (120 - 64) / 120 = 46.67 %.
    let pct = a.schedule_percent().unwrap();
    assert!((pct - 56. / 120. * 100.).abs() < 1e-9, "{pct}");
}

#[test]
fn a_folder_rolls_up_its_projects_by_the_stated_rules() {
    let a = rollup::of_document(&project_a(), 1).unwrap();
    let b = rollup::of_document(&project_b(), 7).unwrap();
    assert_eq!(b.schedule_percent(), Some(0.), "not started");
    let mut f = Rollup::default();
    f.add(&a);
    f.add(&b);
    assert_eq!(f.projects, 2);
    assert_eq!(f.start, at("2027-01-04 08:00"), "earliest start");
    assert_eq!(f.finish, at("2027-02-05 17:00"), "latest finish");
    assert_eq!(f.data_date, at("2027-01-13 08:00"), "latest data date");
    assert_eq!(
        f.must_finish,
        at("2027-02-04 17:00"),
        "latest Must Finish By"
    );
    assert_eq!(f.total_float, Some(-8 * 60), "the most critical project");
    assert_eq!((f.not_started, f.in_progress, f.completed), (2, 1, 1));
    assert_eq!(f.costs.budgeted, 5500.);
    assert!(!f.mixed_calendars, "both use the standard calendar");
    // Planned: Mon 4-Jan 08:00 to Fri 5-Feb 17:00 = 5 weeks = 200 h.
    // Remaining: Wed 13-Jan 08:00 to Fri 5-Feb 17:00 = 24 h + 3 weeks = 144 h.
    // (200 - 144) / 200 = 28 %.
    let pct = f.schedule_percent().unwrap();
    assert!((pct - 28.).abs() < 1e-9, "{pct}");
}

#[test]
fn every_activity_done_is_one_hundred_and_mixed_calendars_use_the_standard_one() {
    let mut doc = project(3, "DONE", "2027-01-11 08:00", "");
    task(
        &mut doc,
        3,
        1,
        &[
            ("status_code", "TK_Complete"),
            ("act_start_date", "2027-01-04 08:00"),
            ("act_end_date", "2027-01-05 17:00"),
        ],
    );
    let done = rollup::of_document(&doc, 3).unwrap();
    assert_eq!(done.schedule_percent(), Some(100.));
    assert_eq!(done.actual_finish, at("2027-01-05 17:00"));
    // A project on a seven-day calendar: a folder with it and project A
    // measures in the standard calendar.
    let mut seven = project_b();
    let cal = seven.table_mut("CALENDAR");
    cal.set(0, "clndr_name", "Seven day".into());
    let week: String = (1..=7)
        .map(|d| format!("(0||{d}()((0||0(s|08:00|f|16:00)())))"))
        .collect();
    cal.set(
        0,
        "clndr_data",
        format!("(0||CalendarData()((0||DaysOfWeek()({week}))(0||Exceptions()())))"),
    );
    let b = rollup::of_document(&seven, 7).unwrap();
    let mut f = Rollup::default();
    f.add(&rollup::of_document(&project_a(), 1).unwrap());
    f.add(&b);
    assert!(f.mixed_calendars);
    assert_eq!(f.measure().name, crate::STANDARD_CALENDAR_NAME);
    assert!((f.schedule_percent().unwrap() - 28.).abs() < 1e-9);
}

#[test]
fn finish_variance_compares_with_the_baseline_in_working_time() {
    let mut a = rollup::of_document(&project_a(), 1).unwrap();
    assert_eq!(a.finish_variance(), None, "no baseline");
    // Baseline finish Thu 21-Jan 17:00: one working day before the finish.
    a.set_baseline(at("2027-01-04 08:00"), at("2027-01-21 17:00"));
    assert_eq!(a.finish_variance(), Some(-8 * 60));
    // The project baseline wins over the primary baseline; a primary that
    // is not stored is ignored.
    let stored = [(11, 500), (12, 600)];
    assert_eq!(
        rollup::pick_baseline(Some(600), &stored, Some(11)),
        Some(12)
    );
    assert_eq!(rollup::pick_baseline(None, &stored, Some(11)), Some(11));
    assert_eq!(rollup::pick_baseline(Some(9), &stored, Some(99)), None);
}

#[test]
fn folders_add_rename_move_and_delete_with_the_app_rules() {
    let mut ws = Workspace::default();
    let top = ws.find(EPS_ID).unwrap();
    let east = ws
        .add_folder(Some(top), "East Region", Some("EAST"))
        .unwrap();
    let plant = ws.add_folder(Some(east), "Plant 4", None).unwrap();
    let west = ws.add_folder(None, "West Region", Some("WEST")).unwrap();
    assert_eq!(ws.path(plant), ["Workspace", "East Region", "Plant 4"]);
    assert!(
        ws.add_folder(Some(top), "east region", None).is_err(),
        "sibling name taken"
    );
    assert!(
        ws.add_folder(None, "Other", Some("east")).is_err(),
        "EPS ID taken"
    );
    assert!(ws
        .rename_folder(plant, Some("Plant Four"), Some("P4"))
        .unwrap());
    assert_eq!(ws.find("p4"), Some(plant));
    ws.place(100, plant).unwrap();
    ws.place(101, east).unwrap();
    assert_eq!(ws.projects_under(east, &[100, 101, 102]), [100, 101]);
    // Moving a folder takes what it holds.
    assert!(ws.move_folder(plant, Some(west)).unwrap());
    assert_eq!(ws.projects_under(west, &[100, 101, 102]), [100]);
    assert!(
        ws.move_folder(west, Some(plant)).is_err(),
        "not under itself"
    );
    assert!(ws.move_folder(plant, None).unwrap());
    assert_eq!(ws.eps().iter().find(|n| n.id == plant).unwrap().depth, 0);
    // A round trip through the preference text keeps everything.
    let back = Workspace::from_text(&ws.to_text());
    assert_eq!(back.node_of(Some(100)), plant);
    assert_eq!(back.path(plant), ["Plant Four"]);
}

/// Deleting a folder takes the folders under it and their projects; the
/// placement rows of those projects go, the others stay where they are.
#[test]
fn deleting_a_folder_takes_its_folders_and_projects() {
    let mut ws = Workspace::default();
    let top = ws.find(EPS_ID).unwrap();
    let east = ws.add_folder(Some(top), "East", Some("EAST")).unwrap();
    let unit = ws.add_folder(Some(east), "Unit 1", Some("U1")).unwrap();
    let west = ws.add_folder(Some(top), "West", Some("WEST")).unwrap();
    ws.place(100, east).unwrap();
    ws.place(101, unit).unwrap();
    ws.place(102, west).unwrap();
    let all = [100, 101, 102, 103];
    assert_eq!(
        ws.folder_contents(east, &all),
        (vec![east, unit], vec![100, 101])
    );
    assert_eq!(ws.delete_eps(east, &all).unwrap(), [100, 101]);
    let left: Vec<i64> = ws.eps().iter().map(|n| n.id).collect();
    assert_eq!(left, [top, west]);
    assert_eq!(ws.node_of(Some(102)), west, "other projects stay filed");
    let filed: Vec<&str> = ws
        .doc
        .table("EPSPROJ")
        .unwrap()
        .rows
        .iter()
        .map(|r| r[0].as_str())
        .collect();
    assert_eq!(filed, ["102"], "the deleted projects are no longer filed");
    // The last top-level folder cannot go.
    assert!(ws.delete_eps(top, &all).is_err());
    assert_eq!(ws.eps().len(), 2);
}

/// The stored tables are found whatever the case of their names: an EPS
/// row written as "eps" is read, not silently replaced by the default tree.
#[test]
fn stored_tables_are_read_whatever_the_case_of_their_names() {
    let text = r#"{"tables":[{"name":"eps","fields":["eps_id","parent_eps_id","seq_num","eps_short_name","eps_name","obs_name"],"rows":[["1","","10","ROOT","Root",""],["2","1","10","SITE","Site",""]]},{"name":"EpsProj","fields":["project_id","eps_id"],"rows":[["7","2"]]}]}"#;
    let ws = Workspace::from_text(text);
    assert_eq!(ws.eps().len(), 2);
    assert_eq!(ws.doc.table("EPSPROJ").unwrap().rows.len(), 1);
}
