//! New activities take the project's Defaults tab values: types, calendar,
//! duration and the Activity ID auto-numbering (`edit::new_activity`).
use openxer_engine::model::TaskType;
use openxer_xer::edit::new_activity::{self as new, add_activity, new_activity_id};
use openxer_xer::edit::records;
use openxer_xer::sample_project::sample_project;
use openxer_xer::workspace::new_project::{build, Spec};
use openxer_xer::{starter, ProjectData};

fn set_project(p: &mut ProjectData, values: &[(&str, &str)]) {
    let t = p.document.table_mut("PROJECT");
    for (k, v) in values {
        t.set(0, k, (*v).into());
    }
}

fn index_of(p: &ProjectData, code: &str) -> usize {
    p.model
        .activities
        .iter()
        .position(|a| a.task_code == code)
        .unwrap_or_else(|| panic!("activity {code}"))
}

fn wbs_of(p: &ProjectData, i: usize) -> i64 {
    p.details[&p.model.activities[i].task_id].wbs
}

/// The starter with its first activities renamed to `codes` and the
/// numbering defaults A, 1000, 10 with the switch as given.
fn numbered(codes: &[&str], from_selected: bool) -> ProjectData {
    let mut p = starter();
    for (i, code) in codes.iter().enumerate() {
        p.model.activities[i].task_code = (*code).into();
    }
    set_project(
        &mut p,
        &[
            ("task_code_prefix", "A"),
            ("task_code_base", "1000"),
            ("task_code_step", "10"),
            (
                "task_code_prefix_flag",
                if from_selected { "Y" } else { "N" },
            ),
        ],
    );
    p
}

#[test]
fn increment_from_selected_fills_the_first_free_gap() {
    // A1000 selected, A1010 taken: A1020.
    let mut p = numbered(&["A1000", "A1010"], true);
    let sel = index_of(&p, "A1000");
    let w = wbs_of(&p, sel);
    let i = add_activity(&mut p, w, Some(sel));
    assert_eq!(p.model.activities[i].task_code, "A1020");
    // A1020 renamed to A1100: the gap at A1020 is filled, not max + 10.
    let mut p = numbered(&["A1000", "A1010", "A1100"], true);
    let sel = index_of(&p, "A1000");
    assert_eq!(new_activity_id(&p, Some(sel)), "A1020");
    // The step is the project's increment.
    set_project(&mut p, &[("task_code_step", "5")]);
    assert_eq!(new_activity_id(&p, Some(sel)), "A1005");
}

#[test]
fn switch_off_or_nothing_selected_continues_after_the_highest_id() {
    // A1000 selected, highest A1100: A1110 whatever is selected.
    let mut p = numbered(&["A1000", "A1010", "A1100"], false);
    let sel = index_of(&p, "A1000");
    let w = wbs_of(&p, sel);
    let i = add_activity(&mut p, w, Some(sel));
    assert_eq!(p.model.activities[i].task_code, "A1110");
    // Then with nothing selected (the project row): A1120.
    let w = p.wbs[0].id;
    let i = add_activity(&mut p, w, None);
    assert_eq!(p.model.activities[i].task_code, "A1120");
    // Switch on but nothing selected: the same rule.
    set_project(&mut p, &[("task_code_prefix_flag", "Y")]);
    assert_eq!(new_activity_id(&p, None), "A1130");
}

#[test]
fn an_empty_project_starts_at_prefix_and_suffix() {
    let mut p = starter();
    let all: Vec<i64> = p.model.activities.iter().map(|a| a.task_id).collect();
    p.delete_activities(&all).unwrap();
    set_project(
        &mut p,
        &[
            ("task_code_prefix", "MP-"),
            ("task_code_base", "0100"),
            ("task_code_step", "5"),
            ("task_code_prefix_flag", "Y"),
        ],
    );
    assert_eq!(new_activity_id(&p, None), "MP-0100");
    let w = p.wbs[0].id;
    let i = add_activity(&mut p, w, None);
    assert_eq!(p.model.activities[i].task_code, "MP-0100");
    assert_eq!(new_activity_id(&p, Some(i)), "MP-0105");
}

#[test]
fn ids_without_a_trailing_number_fall_back_to_auto_numbering() {
    // A selected ID without digits cannot be incremented: the highest ID
    // with the prefix plus the increment.
    let mut p = numbered(&["A1000", "MOB", "A1050"], true);
    assert_eq!(new_activity_id(&p, Some(index_of(&p, "MOB"))), "A1060");
    // A selected ID with another prefix keeps its own text.
    p.model.activities[3].task_code = "RB-1010".into();
    p.model.activities[4].task_code = "RB-1030".into();
    assert_eq!(
        new_activity_id(&p, Some(index_of(&p, "RB-1010"))),
        "RB-1020"
    );
    // IDs without the prefix never count as the highest.
    set_project(&mut p, &[("task_code_prefix_flag", "N")]);
    assert_eq!(new_activity_id(&p, None), "A1060");
}

#[test]
fn a_new_activity_takes_the_project_defaults() {
    let mut p = sample_project().plan;
    let calendar = p.model.calendars[p.model.project_calendar].id.clone();
    let other = p
        .model
        .calendars
        .iter()
        .position(|c| c.id != calendar)
        .expect("the sample has several calendars");
    let other_id = p.model.calendars[other].id.clone();
    set_project(
        &mut p,
        &[
            ("def_duration_type", "DT_FixedQty"),
            ("def_complete_pct_type", "CP_Units"),
            ("def_task_type", "TT_Rsrc"),
            ("clndr_id", &other_id),
            ("task_code_prefix", "N-"),
            ("task_code_base", "500"),
            ("task_code_step", "1"),
            ("task_code_prefix_flag", "N"),
        ],
    );
    let w = p.wbs[1].id;
    let i = add_activity(&mut p, w, None);
    let a = &p.model.activities[i];
    let (task_id, pid) = (a.task_id, p.model.project.proj_id);
    assert_eq!(a.task_code, "N-500");
    assert_eq!(a.task_type, TaskType::Resource);
    assert_eq!(a.cal, other);
    assert_eq!(a.rem, new::NEW_ACTIVITY_MINUTES);
    assert_eq!(new::NEW_ACTIVITY_MINUTES, 40 * 60, "five 8 hour days");
    let d = &p.details[&task_id];
    assert_eq!(d.name, "New Activity");
    assert_eq!(d.wbs, w);
    assert_eq!(d.original_duration, 40 * 60);
    let value = |c: &str| records::task_value(&p.document, task_id, pid, c);
    assert_eq!(value("duration_type"), "DT_FixedQty");
    assert_eq!(value("complete_pct_type"), "CP_Units");
    // The saved file carries them.
    let doc = p.updated_document();
    let t = doc.table("TASK").unwrap();
    let row = t
        .rows
        .iter()
        .find(|r| t.value(r, "task_id") == task_id.to_string())
        .unwrap();
    assert_eq!(t.value(row, "task_code"), "N-500");
    assert_eq!(t.value(row, "task_type"), "TT_Rsrc");
    assert_eq!(t.value(row, "duration_type"), "DT_FixedQty");
    assert_eq!(t.value(row, "complete_pct_type"), "CP_Units");
    assert_eq!(t.value(row, "clndr_id"), other_id);
    assert_eq!(t.value(row, "target_drtn_hr_cnt"), "40");

    // A milestone default gives no duration.
    set_project(&mut p, &[("def_task_type", "TT_FinMile")]);
    let i = add_activity(&mut p, w, None);
    assert_eq!(p.model.activities[i].task_code, "N-501");
    assert_eq!(p.model.activities[i].task_type, TaskType::FinMile);
    assert_eq!(p.model.activities[i].rem, 0);
}

#[test]
fn missing_defaults_fall_back_to_the_new_project_types() {
    // The starter carries no Defaults tab values.
    let p = starter();
    let d = new::defaults(&p);
    assert_eq!(d.duration_type, "DT_FixedDUR2");
    assert_eq!(d.percent_type, "CP_Drtn");
    assert_eq!(d.task_type, TaskType::Task);
    assert_eq!(d.calendar, p.model.project_calendar);
    assert_eq!(new::numbering(&p), new::Numbering::default());
}

#[test]
fn new_project_starts_with_the_observed_defaults() {
    let spec = Spec {
        eps: 1,
        project_id: "NEWJOB".into(),
        name: "New job".into(),
        planned_start: 0,
        must_finish: None,
        manager: String::new(),
        rate_type: "COST_PER_QTY".into(),
    };
    let mut p = build(&spec, None, 4).unwrap();
    assert_eq!(p.model.project.proj_id, 4, "the number given");
    let t = p.document.table("PROJECT").unwrap();
    for (k, v) in new::NEW_PROJECT_DEFAULTS {
        assert_eq!(t.value(&t.rows[0], k), v, "{k}");
    }
    let n = new::numbering(&p);
    assert_eq!(
        n,
        new::Numbering {
            prefix: "A".into(),
            suffix: "1000".into(),
            step: 10,
            from_selected: true,
        }
    );
    // The first activity of the empty project is A1000, the next after it
    // A1010.
    let w = p.wbs[0].id;
    let i = add_activity(&mut p, w, None);
    assert_eq!(p.model.activities[i].task_code, "A1000");
    let j = add_activity(&mut p, w, Some(i));
    assert_eq!(p.model.activities[j].task_code, "A1010");
    // Values a project already holds are kept.
    set_project(&mut p, &[("task_code_prefix", "EC"), ("def_task_type", "")]);
    new::new_project_defaults(&mut p);
    let t = p.document.table("PROJECT").unwrap();
    assert_eq!(t.value(&t.rows[0], "task_code_prefix"), "EC");
    assert_eq!(t.value(&t.rows[0], "def_task_type"), "TT_Task");
}
