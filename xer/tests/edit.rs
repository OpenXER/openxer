//! The shared editing layer (`openxer_xer::edit`) on the synthetic starter
//! project: the rules the desktop, the command line and MCP all use.
use openxer_engine::model::{CstrType, StatusType, TaskType};
use openxer_xer::assignments::{cost_in, cost_places, AssignmentEdit};
use openxer_xer::edit::activity::{self as act, Edit};
use openxer_xer::edit::{records, wbs};
use openxer_xer::{parse_relative, starter, ProjectData};

const CREW: i64 = 77;

fn with_crew() -> ProjectData {
    let mut p = starter();
    p.document.table_mut("RSRC").add(&[
        ("rsrc_id", CREW.to_string()),
        ("rsrc_name", "Pipe crew".into()),
        ("rsrc_short_name", "PIPE".into()),
        ("rsrc_type", "RT_Labor".into()),
        ("def_qty_per_hr", "1".into()),
        ("clndr_id", "1".into()),
    ]);
    p.document.table_mut("RSRCRATE").add(&[
        ("rsrc_id", CREW.to_string()),
        ("start_date", "2020-01-01 00:00".into()),
        ("cost_per_qty", "1".into()),
    ]);
    p
}

fn index(p: &ProjectData, task_id: i64) -> usize {
    p.model
        .activities
        .iter()
        .position(|a| a.task_id == task_id)
        .unwrap()
}

fn at(p: &ProjectData, text: &str) -> i64 {
    parse_relative(p.model.origin_serial, text)
        .unwrap()
        .unwrap()
}

fn cell(p: &ProjectData, row: usize, column: &str) -> String {
    records::get(&p.document, "TASKRSRC", row, column)
}

/// Run `f` on its own thread and fail if it does not finish in time (a
/// calendar walk that never ends would otherwise hang the test run).
fn within_seconds<T: Send + 'static>(secs: u64, f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(std::time::Duration::from_secs(secs))
        .expect("the edit did not finish: it is looping on the calendar")
}

/// The starter project with activity 1020 on a calendar with no working
/// time at all, as an empty new calendar or a file can give it.
fn on_a_calendar_without_work() -> (ProjectData, usize) {
    let mut p = with_crew();
    let i = index(&p, 1020);
    let cal = p.model.activities[i].cal;
    let c = &mut p.model.calendars[cal].cal;
    c.template = Default::default();
    c.exc.clear();
    (p, i)
}

#[test]
fn edits_on_a_calendar_without_work_finish() {
    within_seconds(20, || {
        let (mut p, i) = on_a_calendar_without_work();
        act::edit_activity(&mut p, i, Edit::OriginalDuration(16 * 60)).unwrap();
        let start = act::default_actual_start(&p, i);
        act::edit_activity(&mut p, i, Edit::ActualStart(Some(start))).unwrap();
        act::default_actual_finish(&p, i);
        act::edit_activity(&mut p, i, Edit::ActualFinish(Some(start + 2 * 1440))).unwrap();
    });
    within_seconds(20, || {
        let (mut p, i) = on_a_calendar_without_work();
        let task_id = p.model.activities[i].task_id;
        let row = p.add_assignment(task_id, Some(CREW), None).unwrap();
        p.edit_assignment(row, AssignmentEdit::Lag(8 * 60)).unwrap();
        p.assignment_view(row).unwrap();
    });
}

#[test]
fn ticking_started_or_finished_never_records_an_actual_after_the_data_date() {
    let mut p = starter();
    let dd = p.model.project.data_date.unwrap();
    // An activity planned to start after the data date.
    let late = index(&p, 1030);
    let future = |p: &mut ProjectData| {
        let a = &mut p.model.activities[late];
        assert_eq!(a.status, StatusType::NotStarted);
        a.target_start = Some(dd + 3 * 1440);
        a.target_finish = Some(dd + 6 * 1440);
    };
    future(&mut p);
    assert_eq!(act::default_actual_start(&p, late), dd);
    assert_eq!(act::default_actual_finish(&p, late), dd);
    act::edit_activity(&mut p, late, Edit::Status(StatusType::InProgress)).unwrap();
    assert_eq!(p.model.activities[late].act_start, Some(dd));
    act::edit_activity(&mut p, late, Edit::Status(StatusType::Completed)).unwrap();
    let a = &p.model.activities[late];
    assert_eq!((a.act_start, a.act_finish), (Some(dd), Some(dd)));

    // An activity planned before the data date keeps its planned dates.
    let mut p = starter();
    let i = index(&p, 1020);
    let planned = at(&p, "2026-10-26 08:00");
    let planned_finish = at(&p, "2026-10-28 17:00");
    let a = &mut p.model.activities[i];
    a.target_start = Some(planned);
    a.target_finish = Some(planned_finish);
    assert_eq!(act::default_actual_start(&p, i), planned);
    assert_eq!(act::default_actual_finish(&p, i), planned_finish);
    act::edit_activity(&mut p, i, Edit::Status(StatusType::Completed)).unwrap();
    let a = &p.model.activities[i];
    assert_eq!(
        (a.act_start, a.act_finish),
        (Some(planned), Some(planned_finish))
    );

    // Started before the data date, planned to finish after it: the finish
    // stops at the data date.
    let mut p = starter();
    let i = index(&p, 1020);
    let a = &mut p.model.activities[i];
    a.target_start = Some(planned);
    a.target_finish = Some(dd + 5 * 1440);
    act::edit_activity(&mut p, i, Edit::Status(StatusType::Completed)).unwrap();
    let a = &p.model.activities[i];
    assert_eq!((a.act_start, a.act_finish), (Some(planned), Some(dd)));

    // A typed actual start after the data date is still taken as typed, and
    // the finish then is never before it.
    let mut p = starter();
    future(&mut p);
    act::edit_activity(&mut p, late, Edit::ActualStart(Some(dd + 1440))).unwrap();
    assert_eq!(p.model.activities[late].act_start, Some(dd + 1440));
    assert_eq!(act::default_actual_finish(&p, late), dd + 1440);
}

#[test]
fn durations_stay_equal_until_the_activity_starts() {
    let mut p = starter();
    let i = index(&p, 1020);
    let out = act::edit_activity(&mut p, i, Edit::RemainingDuration(16 * 60)).unwrap();
    assert!(out.changed);
    assert_eq!(p.model.activities[i].rem, 16 * 60);
    assert_eq!(p.details[&1020].original_duration, 16 * 60);
    // Locked values refuse and change nothing.
    let finish = at(&p, "2026-11-06 17:00");
    act::edit_activity(&mut p, i, Edit::ActualStart(Some(finish - 4 * 1440))).unwrap();
    act::edit_activity(&mut p, i, Edit::ActualFinish(Some(finish))).unwrap();
    let before = p.clone();
    assert!(act::edit_activity(&mut p, i, Edit::RemainingDuration(60)).is_err());
    assert_eq!(p.model.activities[i], before.model.activities[i]);
}

#[test]
fn suspend_and_resume_dates_keep_their_order() {
    let mut p = starter();
    let i = index(&p, 1020);
    let start = at(&p, "2026-10-26 08:00");
    act::edit_activity(&mut p, i, Edit::ActualStart(Some(start))).unwrap();
    let suspend = |p: &ProjectData| records::task_value(&p.document, 1020, 1, "suspend_date");
    // Suspend not before the actual start.
    assert!(act::edit_activity(&mut p, i, Edit::SuspendDate(Some(start - 1440))).is_err());
    assert_eq!(suspend(&p), "");
    let s = at(&p, "2026-10-28 08:00");
    act::edit_activity(&mut p, i, Edit::SuspendDate(Some(s))).unwrap();
    // Resume after suspend.
    for bad in [s, s - 60] {
        let err = act::edit_activity(&mut p, i, Edit::ResumeDate(Some(bad))).unwrap_err();
        assert!(err.contains("after the suspend date"), "{err}");
        assert_eq!(p.model.activities[i].resume, None);
    }
    let r = at(&p, "2026-10-29 08:00");
    act::edit_activity(&mut p, i, Edit::ResumeDate(Some(r))).unwrap();
    assert_eq!(p.model.activities[i].resume, Some(r));
    // A suspend moved past the resume is refused; clearing either is fine.
    assert!(act::edit_activity(&mut p, i, Edit::SuspendDate(Some(r + 60))).is_err());
    assert_eq!(suspend(&p), "2026-10-28 08:00");
    act::edit_activity(&mut p, i, Edit::ResumeDate(None)).unwrap();
    act::edit_activity(&mut p, i, Edit::SuspendDate(None)).unwrap();
    assert_eq!(suspend(&p), "");
}

#[test]
fn status_follows_actual_dates_and_assignments_take_them() {
    let mut p = with_crew();
    let i = index(&p, 1020);
    let row = p.add_assignment(1020, Some(CREW), None).unwrap();
    let start = at(&p, "2026-10-26 08:00");
    let out = act::edit_activity(&mut p, i, Edit::ActualStart(Some(start))).unwrap();
    assert!(out.progress_changed && out.records_changed);
    assert_eq!(p.model.activities[i].status, StatusType::InProgress);
    assert_eq!(cell(&p, row, "act_start_date"), "2026-10-26 08:00");
    let finish = at(&p, "2026-10-29 17:00");
    act::edit_activity(&mut p, i, Edit::ActualFinish(Some(finish))).unwrap();
    assert_eq!(p.model.activities[i].status, StatusType::Completed);
    assert_eq!(cell(&p, row, "act_end_date"), "2026-10-29 17:00");
    // Reopening clears the assignment's actual finish.
    act::edit_activity(&mut p, i, Edit::ActualFinish(None)).unwrap();
    assert_eq!(p.model.activities[i].status, StatusType::InProgress);
    assert_eq!(cell(&p, row, "act_end_date"), "");
}

#[test]
fn starting_work_replans_its_finish_on_the_current_calendar() {
    let mut p = starter();
    let i = index(&p, 1020);
    let planned = at(&p, "2026-11-02 08:00");
    p.model.activities[i].target_start = Some(planned);
    p.model.activities[i].target_finish = Some(planned);
    act::edit_activity(&mut p, i, Edit::ActualStart(Some(planned))).unwrap();
    // 48 working hours from Monday 08:00 end the next Monday 17:00.
    assert_eq!(
        p.model.activities[i].target_finish,
        Some(at(&p, "2026-11-09 17:00"))
    );
    // A new original duration moves the planned finish too.
    act::edit_activity(&mut p, i, Edit::OriginalDuration(8 * 60)).unwrap();
    assert_eq!(
        p.model.activities[i].target_finish,
        Some(at(&p, "2026-11-02 17:00"))
    );
}

#[test]
fn a_finished_activity_shows_no_remaining_units() {
    let mut p = with_crew();
    let i = index(&p, 1020);
    let row = p.add_assignment(1020, Some(CREW), None).unwrap();
    let start = at(&p, "2026-10-26 08:00");
    act::edit_activity(&mut p, i, Edit::ActualStart(Some(start))).unwrap();
    act::edit_activity(&mut p, i, Edit::ActualFinish(Some(start + 1440))).unwrap();
    // The assignment may still carry a remainder, as the reference keeps it.
    p.edit_assignment(row, AssignmentEdit::RemainingUnits(0.38))
        .unwrap();
    assert_eq!(cell(&p, row, "remain_qty"), "0.38");
    let pid = p.model.project.proj_id;
    assert_eq!(
        records::task_value(&p.document, 1020, pid, "remain_work_qty"),
        "0"
    );
}

#[test]
fn zero_remaining_duration_keeps_units_per_time() {
    let mut p = with_crew();
    let i = index(&p, 1020);
    let row = p.add_assignment(1020, Some(CREW), None).unwrap();
    let start = at(&p, "2026-10-26 08:00");
    act::edit_activity(&mut p, i, Edit::ActualStart(Some(start))).unwrap();
    p.edit_assignment(row, AssignmentEdit::RemainingUnits(24.))
        .unwrap();
    let rate = cell(&p, row, "remain_qty_per_hr");
    act::edit_activity(&mut p, i, Edit::RemainingDuration(0)).unwrap();
    p.edit_assignment(row, AssignmentEdit::RemainingUnits(0.))
        .unwrap();
    assert_eq!(cell(&p, row, "remain_qty"), "0");
    assert_eq!(cell(&p, row, "remain_qty_per_hr"), rate);
}

#[test]
fn an_entered_remaining_duration_survives_edits_elsewhere_before_its_units() {
    // Seen in two consecutive private weekly exports (Fixed Duration and
    // Units, started):
    // remaining duration 0, then other activities' assignments edited, then
    // its own remaining units 0. The duration stays 0 and the units per time
    // stay as they were, as the reference stored them.
    let mut p = with_crew();
    let i = index(&p, 1020);
    let row = p.add_assignment(1020, Some(CREW), None).unwrap();
    let other = p.add_assignment(1010, Some(CREW), None).unwrap();
    let start = at(&p, "2026-10-26 08:00");
    act::edit_activity(&mut p, i, Edit::ActualStart(Some(start))).unwrap();
    p.edit_assignment(row, AssignmentEdit::RemainingUnits(70.))
        .unwrap();
    let rate = cell(&p, row, "remain_qty_per_hr");
    act::edit_activity(&mut p, i, Edit::RemainingDuration(0)).unwrap();
    p.edit_assignment(other, AssignmentEdit::RemainingUnits(5.))
        .unwrap();
    assert_eq!(p.model.activities[i].rem, 0);
    p.edit_assignment(row, AssignmentEdit::RemainingUnits(0.))
        .unwrap();
    assert_eq!(p.model.activities[i].rem, 0);
    assert_eq!(cell(&p, row, "remain_qty"), "0");
    assert_eq!(cell(&p, row, "remain_qty_per_hr"), rate);
}

#[test]
fn scheduling_takes_a_started_fixed_duration_and_units_remaining_duration_from_its_units() {
    // Saved probe 1301b, U2: a started Fixed Duration and Units activity with
    // 35.0833 h left whose assignment row says 11.82 units at 3.0164 per hour.
    // At F9 the remaining duration becomes units / units per hour to the
    // whole minute (235 minutes); units and units per time stay. A refresh in
    // the middle of edits leaves the duration alone.
    let mut p = with_crew();
    let i = index(&p, 1020);
    let row = p.add_assignment(1020, Some(CREW), None).unwrap();
    let start = at(&p, "2026-10-26 08:00");
    act::edit_activity(&mut p, i, Edit::ActualStart(Some(start))).unwrap();
    act::edit_activity(&mut p, i, Edit::RemainingDuration(2105)).unwrap();
    p.edit_assignment(row, AssignmentEdit::RemainingUnitsPerTime(3.0164))
        .unwrap();
    assert_eq!(p.model.activities[i].rem, 2105);
    records::set(
        &mut p.document,
        "TASKRSRC",
        row,
        "remain_qty",
        "11.82".into(),
    );
    p.rebuild_assignments();
    assert_eq!(p.model.activities[i].rem, 2105);
    p.refresh_derived_durations();
    assert_eq!(p.model.activities[i].rem, 235);
    assert_eq!(cell(&p, row, "remain_qty"), "11.82");
    assert_eq!(cell(&p, row, "remain_qty_per_hr"), "3.0164");
}

#[test]
fn choices_take_codes_and_display_names() {
    assert_eq!(act::parse_task_type("TT_Task"), Some(TaskType::Task));
    assert_eq!(
        act::parse_task_type("finish milestone"),
        Some(TaskType::FinMile)
    );
    assert_eq!(act::parse_task_type("TT_Rsrc"), Some(TaskType::Resource));
    assert_eq!(act::parse_constraint("CS_ALAP"), Some(CstrType::Alap));
    assert_eq!(
        act::parse_constraint("Start On or After"),
        Some(CstrType::StartOnAfter)
    );
    assert_eq!(act::parse_constraint(""), Some(CstrType::None));
    assert_eq!(act::parse_constraint("CS_NOPE"), None);
    assert_eq!(
        act::duration_type_code("DT_FixedDUR2"),
        Some("DT_FixedDUR2")
    );
    assert_eq!(act::duration_type_code("Fixed Units"), Some("DT_FixedQty"));
    assert_eq!(act::percent_type_code("physical"), Some("CP_Phys"));
    assert_eq!(act::priority_code("PT_High"), Some("PT_High"));
    assert_eq!(act::priority_code("Lowest"), Some("PT_Lowest"));
    assert_eq!(act::parse_status("TK_Active"), Some(StatusType::InProgress));
    assert_eq!(act::parse_status("Completed"), Some(StatusType::Completed));
}

#[test]
fn ids_names_and_priority() {
    let mut p = starter();
    let i = index(&p, 1020);
    assert!(act::set_activity_id(&mut p, i, "RB-1010").is_err());
    assert!(act::set_activity_id(&mut p, i, "  ").is_err());
    assert!(act::set_activity_id(&mut p, i, "NEW-1").unwrap());
    assert!(act::set_activity_name(&mut p, i, "Survey").unwrap());
    assert_eq!(p.name(i), "Survey");
    let out = act::edit_activity(&mut p, i, Edit::Priority("PT_Top")).unwrap();
    assert!(out.records_changed);
    let pid = p.model.project.proj_id;
    assert_eq!(
        records::task_value(&p.document, 1020, pid, "priority_type"),
        "PT_Top"
    );
}

#[test]
fn wbs_paths_find_and_add_elements() {
    let mut p = starter();
    let first = wbs::find_path(&p, "RB.1").unwrap();
    assert_eq!(wbs::find_path(&p, "rb.1"), Some(first));
    assert_eq!(wbs::find_path(&p, "RB.9"), None);
    let (id, added) = wbs::ensure_path(&mut p, "RB.1.A.B", Some("Deep")).unwrap();
    assert_eq!(added, 2);
    assert_eq!(wbs::find_path(&p, "RB.1.A.B"), Some(id));
    assert_eq!(p.wbs.iter().find(|w| w.id == id).unwrap().name, "Deep");
    let a = wbs::find_path(&p, "RB.1.A").unwrap();
    assert_eq!(p.wbs.iter().find(|w| w.id == id).unwrap().parent, Some(a));
    assert_eq!(
        p.wbs.iter().find(|w| w.id == a).unwrap().parent,
        Some(first)
    );
    assert_eq!(wbs::ensure_path(&mut p, "RB.1.A.B", None).unwrap(), (id, 0));
    assert!(wbs::ensure_path(&mut p, "RB..X", None).is_err());
}

#[test]
fn costs_round_like_the_file_stores_them() {
    assert_eq!(cost_in(2, 934.555), "934.56");
    assert_eq!(cost_in(2, 2361.345), "2361.34");
    assert_eq!(cost_in(2, 2766.9176), "2766.92");
    assert_eq!(cost_in(2, 0.), "0.00");
    assert_eq!(cost_in(4, 12.5), "12.5000");
    let mut p = with_crew();
    assert_eq!(cost_places(&p.document), 4, "no costs yet");
    let row = p.add_assignment(1020, Some(CREW), None).unwrap();
    for column in ["target_cost", "act_reg_cost", "act_ot_cost", "remain_cost"] {
        let v = cell(&p, row, column);
        let v = format!("{:.2}", v.parse::<f64>().unwrap_or(0.));
        records::set(&mut p.document, "TASKRSRC", row, column, v);
    }
    assert_eq!(cost_places(&p.document), 2);
    p.edit_assignment(row, AssignmentEdit::BudgetedUnits(2.333333))
        .unwrap();
    assert_eq!(cell(&p, row, "target_cost"), "2.33");
}

/// RB.3 (30) moved under RB.2 (20), with a row of every kind that hangs off
/// RB.3: a WBS milestone, a spending plan row, a budget log entry, a
/// document of the element and one of an activity, a notebook topic, an
/// issue and a threshold.
fn wbs_with_rows() -> (ProjectData, i64) {
    let mut p = starter();
    wbs::shift_right(&mut p, 30).unwrap();
    let task = p
        .details
        .iter()
        .find(|(_, d)| d.wbs == 30)
        .map(|(t, _)| *t)
        .expect("RB.3 holds an activity");
    let d = &mut p.document;
    d.table_mut("WBSSTEP")
        .add(&[("wbs_step_id", "1".into()), ("wbs_id", "30".into())]);
    d.table_mut("WBSBUDG")
        .add(&[("wbs_budg_id", "1".into()), ("wbs_id", "30".into())]);
    d.table_mut("BUDGCHNG")
        .add(&[("budg_chng_id", "1".into()), ("wbs_id", "30".into())]);
    d.table_mut("TASKDOC").add(&[
        ("taskdoc_id", "1".into()),
        ("wbs_id", "30".into()),
        ("task_id", String::new()),
    ]);
    d.table_mut("TASKDOC").add(&[
        ("taskdoc_id", "2".into()),
        ("wbs_id", "30".into()),
        ("task_id", task.to_string()),
    ]);
    d.table_mut("WBSMEMO")
        .add(&[("wbs_memo_id", "1".into()), ("wbs_id", "30".into())]);
    d.table_mut("PROJISSU")
        .add(&[("issue_id", "1".into()), ("wbs_id", "30".into())]);
    d.table_mut("PROJTHRS")
        .add(&[("thresh_id", "1".into()), ("wbs_id", "30".into())]);
    (p, task)
}

fn wbs_ids(p: &ProjectData, table: &str) -> Vec<String> {
    p.document.table(table).map_or(vec![], |t| {
        t.rows
            .iter()
            .map(|r| t.value(r, "wbs_id").to_string())
            .collect()
    })
}

#[test]
fn merging_a_wbs_element_moves_its_rows_to_the_parent() {
    let (mut p, _) = wbs_with_rows();
    let related = wbs::related(&p, &[30].into());
    assert_eq!(
        (related.milestones, related.spending, related.budget_log),
        (1, 1, 1)
    );
    assert_eq!((related.documents, related.notebooks), (1, 1));
    assert_eq!((related.issues, related.thresholds), (1, 1));
    wbs::delete_merge(&mut p, 30).unwrap();
    for table in ["WBSSTEP", "WBSBUDG", "BUDGCHNG", "PROJISSU", "PROJTHRS"] {
        assert_eq!(wbs_ids(&p, table), ["20"], "{table}");
    }
    assert_eq!(wbs_ids(&p, "TASKDOC"), ["20", "20"]);
    assert!(wbs_ids(&p, "WBSMEMO").is_empty(), "the notebook topic goes");
}

#[test]
fn deleting_a_wbs_element_removes_its_rows_and_repoints_issues_and_thresholds() {
    let (mut p, task) = wbs_with_rows();
    let n = wbs::delete_with_activities(&mut p, 30).unwrap();
    assert!(n >= 1);
    assert!(!p.details.contains_key(&task));
    for table in ["WBSSTEP", "WBSBUDG", "BUDGCHNG", "TASKDOC", "WBSMEMO"] {
        assert!(
            wbs_ids(&p, table).is_empty(),
            "{table}: {:?}",
            wbs_ids(&p, table)
        );
    }
    for table in ["PROJISSU", "PROJTHRS"] {
        assert_eq!(wbs_ids(&p, table), ["20"], "{table} names the parent");
    }
}

#[test]
fn deleting_a_top_level_wbs_element_clears_issue_and_threshold_references() {
    let mut p = starter();
    let empty = wbs::add(&mut p, Some(40)).unwrap();
    let d = &mut p.document;
    d.table_mut("WBSBUDG")
        .add(&[("wbs_budg_id", "1".into()), ("wbs_id", empty.to_string())]);
    d.table_mut("PROJISSU")
        .add(&[("issue_id", "1".into()), ("wbs_id", empty.to_string())]);
    wbs::delete_merge(&mut p, empty).unwrap();
    assert!(wbs_ids(&p, "WBSBUDG").is_empty(), "nothing to move it to");
    assert_eq!(wbs_ids(&p, "PROJISSU"), [""]);
}

/// The milestone rule on type changes: a task with assignments is not made
/// a milestone (the message says what to do), and a milestone whose primary
/// resource is not assigned loses it when it becomes a task, so the rule
/// "a task's primary resource is one of its assignments" holds.
#[test]
fn type_changes_keep_the_milestone_and_primary_resource_rules() {
    use openxer_xer::assignments::{is_milestone, unassigned_primary};
    let mut p = with_crew();
    let pid = p.model.project.proj_id;
    let task = (0..p.model.activities.len())
        .find(|&i| p.model.activities[i].task_type == TaskType::Task)
        .unwrap();
    let task_id = p.model.activities[task].task_id;
    p.add_assignment(task_id, Some(CREW), None).unwrap();
    let before = p.clone();
    let e = act::edit_activity(&mut p, task, Edit::ActivityType(TaskType::FinMile)).unwrap_err();
    assert!(e.contains("Remove its assignments first (PIPE)"), "{e}");
    assert!(e.contains("then change the activity type"), "{e}");
    assert_eq!(p.model.activities[task].task_type, TaskType::Task);
    assert_eq!(
        p.updated_document().text(),
        before.updated_document().text()
    );
    // A milestone may name any resource as its primary resource.
    let mile = (0..p.model.activities.len())
        .find(|&i| is_milestone(p.model.activities[i].task_type))
        .unwrap();
    let mile_id = p.model.activities[mile].task_id;
    act::edit_activity(&mut p, mile, Edit::PrimaryResource(CREW)).unwrap();
    assert_eq!(
        records::task_value(&p.document, mile_id, pid, "rsrc_id"),
        "77"
    );
    assert!(unassigned_primary(&p, mile_id));
    // As a task it cannot keep a primary resource it does not have.
    let out = act::edit_activity(&mut p, mile, Edit::ActivityType(TaskType::Task)).unwrap();
    assert!(out.changed);
    assert_eq!(p.model.activities[mile].task_type, TaskType::Task);
    assert_eq!(
        records::task_value(&p.document, mile_id, pid, "rsrc_id"),
        ""
    );
    assert!(!unassigned_primary(&p, mile_id));
    // A task whose primary resource is assigned keeps it through a change
    // between task types.
    act::edit_activity(&mut p, task, Edit::PrimaryResource(CREW)).unwrap();
    act::edit_activity(&mut p, task, Edit::ActivityType(TaskType::Resource)).unwrap();
    assert_eq!(
        records::task_value(&p.document, task_id, pid, "rsrc_id"),
        "77"
    );
}

/// Notes typed as plain text that start with "<" are text, not HTML.
#[test]
fn plain_text_starting_with_a_less_than_sign_is_not_taken_for_html() {
    for html in [
        "<p>Pour</p>",
        "<html><body>x</body></html>",
        "<br>",
        "<!DOCTYPE html><p>",
        "</b>",
    ] {
        assert!(records::looks_like_html(html), "{html}");
    }
    for text in [
        "<5 days float",
        "< 3 crews",
        "<b float",
        "",
        "a <p> b",
        "<>",
    ] {
        assert!(!records::looks_like_html(text), "{text}");
    }
    assert_eq!(
        records::text_to_memo("<5 days float"),
        "<html><body>&lt;5 days float</body></html>"
    );
}

/// Deleting a calendar in use moves every user to the replacement, then
/// removes it; a bad replacement changes nothing; the default global
/// calendar is refused.
#[test]
fn a_calendar_in_use_is_deleted_onto_a_replacement() {
    use openxer_xer::edit::calendars as cal;
    let mut p = starter();
    let first = cal::list(&p.document, cal::Kind::Global, 0)[0].id.clone();
    let stamp = "2026-01-05 08:00";
    let gone = cal::add(&mut p, cal::Kind::Global, Some(&first), stamp).unwrap();
    let to = cal::add(&mut p, cal::Kind::Global, Some(&first), stamp).unwrap();
    let globals = [first, gone.clone(), to.clone()];
    let k = p.model.calendars.iter().position(|c| c.id == gone).unwrap();
    for i in [1, 2] {
        p.model.activities[i].cal = k;
    }
    // A resource on it moves too.
    if let Some(t) = p.document.tables.iter_mut().find(|t| t.name == "RSRC") {
        if let (Some(c), Some(row)) = (t.field("clndr_id"), t.rows.first_mut()) {
            row[c] = gone.clone();
        }
    }
    let users: Vec<i64> = p
        .model
        .activities
        .iter()
        .filter(|a| p.model.calendars[a.cal].id == gone)
        .map(|a| a.task_id)
        .collect();
    assert!(!users.is_empty());
    assert_eq!(cal::delete_check(&p, &gone), Ok(true));
    assert!(cal::delete(&mut p, &gone).is_err(), "refused");
    let before = p.clone();
    assert!(cal::delete_replacing(&mut p, &gone, &gone).is_err());
    assert!(cal::delete_replacing(&mut p, &gone, "no such").is_err());
    assert_eq!(p.model, before.model, "nothing changed");
    assert!(cal::row_of(&p.document, &gone).is_some());

    assert_eq!(cal::delete_replacing(&mut p, &gone, &to), Ok(users.len()));
    assert!(cal::row_of(&p.document, &gone).is_none(), "deleted");
    for a in &p.model.activities {
        let id = &p.model.calendars[a.cal].id;
        assert_ne!(*id, gone);
        if users.contains(&a.task_id) {
            assert_eq!(*id, to, "activity {} moved", a.task_code);
        }
    }
    for (table, column) in [
        ("TASK", "clndr_id"),
        ("PROJECT", "clndr_id"),
        ("RSRC", "clndr_id"),
        ("CALENDAR", "base_clndr_id"),
    ] {
        if let Some(t) = p.document.table(table) {
            assert!(t.rows.iter().all(|r| t.value(r, column) != gone), "{table}");
        }
    }

    cal::set_default(&mut p, &to).unwrap();
    let refused = cal::delete_check(&p, &to).unwrap_err();
    assert!(refused.contains("default global calendar"), "{refused}");
    assert!(cal::delete_replacing(&mut p, &to, &globals[0]).is_err());
    assert_eq!(cal::default_global(&p.document), Some(to));
}

/// A WBS copy takes the rows Copy WBS Options names, with new keys, under
/// the copied elements; the originals keep theirs.
#[test]
fn a_wbs_copy_brings_the_chosen_rows_with_new_keys() {
    let mut p = starter();
    let node = wbs::project_node(&p);
    let source = p
        .wbs
        .iter()
        .find(|w| Some(w.id) != node && w.parent == node)
        .map(|w| w.id)
        .expect("a top element");
    let pid = p.model.project.proj_id.to_string();
    records::add_row(
        &mut p.document,
        "WBSSTEP",
        &[
            ("wbs_step_id", "1".into()),
            ("proj_id", pid.clone()),
            ("wbs_id", source.to_string()),
            ("step_name", "Milestone".into()),
        ],
    );
    let doc_key = records::next_id(&p.document, "TASKDOC", "taskdoc_id");
    records::add_row(
        &mut p.document,
        "TASKDOC",
        &[
            ("taskdoc_id", doc_key.to_string()),
            ("doc_id", "1".into()),
            ("proj_id", pid.clone()),
            ("wbs_id", source.to_string()),
        ],
    );
    let memo_key = records::next_id(&p.document, "WBSMEMO", "wbs_memo_id");
    let memo = p.document.table_mut("WBSMEMO");
    if memo.fields.is_empty() {
        memo.fields = "wbs_memo_id proj_id memo_type_id wbs_id wbs_memo"
            .split(' ')
            .map(String::from)
            .collect();
    }
    memo.add(&[
        ("wbs_memo_id", memo_key.to_string()),
        ("proj_id", pid),
        ("wbs_id", source.to_string()),
        ("wbs_memo", "Note".into()),
    ]);
    let on = |p: &ProjectData, table: &str, id: i64| {
        p.document.table(table).map_or(0, |t| {
            t.rows
                .iter()
                .filter(|r| records::num(t.value(r, "wbs_id")) == id)
                .count()
        })
    };
    let parts = wbs::CopyParts {
        notebook: true,
        milestones: false,
        documents: true,
    };
    let map = wbs::copy_subtree(&mut p, source, node, parts).unwrap();
    let copy = map[&source];
    assert_ne!(copy, source);
    assert_eq!(on(&p, "WBSMEMO", copy), 1, "notebook copied");
    assert_eq!(on(&p, "TASKDOC", copy), 1, "document copied");
    assert_eq!(on(&p, "WBSSTEP", copy), 0, "milestones left out");
    assert_eq!(on(&p, "WBSSTEP", source), 1, "the original keeps its own");
    let keys = |table: &str, key: &str| -> Vec<i64> {
        let t = p.document.table(table).unwrap();
        t.rows
            .iter()
            .map(|r| records::num(t.value(r, key)))
            .collect()
    };
    for (table, key) in [("WBSMEMO", "wbs_memo_id"), ("TASKDOC", "taskdoc_id")] {
        let all = keys(table, key);
        let unique: std::collections::BTreeSet<_> = all.iter().collect();
        assert_eq!(unique.len(), all.len(), "{table}: new keys");
    }
    // Activities stay where they were: the elements alone are copied here.
    let copies: Vec<i64> = map.values().copied().collect();
    assert!(p.details.values().all(|d| !copies.contains(&d.wbs)));
}
