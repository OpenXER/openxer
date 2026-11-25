//! Editable resource assignments: edits rebuild the scheduling model, F9
//! schedules them, and scheduled dates are written back to TASKRSRC.
use openxer_engine::{model::StatusType, scheduler};
use openxer_xer::assignments::AssignmentEdit;
use openxer_xer::{parse_absolute, starter, Document, ProjectData};
use std::{collections::BTreeMap, fs, path::PathBuf};

const CREW: i64 = 10;

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
        ("max_qty_per_hr", "4".into()),
        ("cost_per_qty", "85".into()),
        ("cost_per_qty2", "95".into()),
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
fn set_duration_type(p: &mut ProjectData, task_id: i64, code: &str) {
    let t = p.document.table_mut("TASK");
    let row = t
        .rows
        .iter()
        .position(|r| t.value(r, "task_id") == task_id.to_string())
        .unwrap();
    t.set(row, "duration_type", code.into());
}
fn value(p: &ProjectData, row: usize, column: &str) -> String {
    let t = p.document.table("TASKRSRC").unwrap();
    t.value(&t.rows[row], column).to_string()
}
fn schedule(p: &mut ProjectData) {
    p.refresh_derived_durations();
    scheduler::schedule_with_resources(&mut p.model, &p.resources).unwrap();
}
fn reimport(p: &ProjectData) -> ProjectData {
    let text = p.updated_document().text();
    ProjectData::from_document(Document::parse(&text).unwrap(), 1, "reimport".into()).unwrap()
}

#[test]
fn assignment_dates_on_a_project_without_calendars_use_elapsed_time() {
    let mut p = with_crew();
    let i = index(&p, 1020);
    let task_id = p.model.activities[i].task_id;
    let row = p.add_assignment(task_id, Some(CREW), None).unwrap();
    // Dates as a schedule run leaves them.
    let start = openxer_xer::parse_relative(p.model.origin_serial, "2026-11-03 08:00")
        .unwrap()
        .unwrap();
    let a = &mut p.model.activities[i];
    a.restart = Some(start);
    a.reend = Some(start + 2 * 1440);
    a.early_start = Some(start);
    a.early_finish = Some(start + 2 * 1440);
    // The project loses every calendar (as one opened without any).
    p.model.calendars.clear();
    p.calendar_names.clear();
    p.assignment_view(row).unwrap();
    p.edit_assignment(row, AssignmentEdit::Lag(8 * 60)).unwrap();
    p.edit_assignment(row, AssignmentEdit::RemainingStart(start + 90))
        .unwrap();
    let view = p.assignment_view(row).unwrap();
    // Elapsed time: the lag is the 90 minutes typed, not working time.
    assert_eq!(view.lag, 90);
    assert_eq!(view.dates.rem_start, Some(start + 90));
    p.edit_assignment(row, AssignmentEdit::RemainingFinish(start + 600))
        .unwrap();
    assert_eq!(
        p.assignment_view(row).unwrap().dates.rem_finish,
        Some(start + 600)
    );
}

#[test]
fn removed_assignment_takes_its_child_rows_and_a_new_one_inherits_none() {
    let mut p = with_crew();
    let row = p.add_assignment(1010, Some(CREW), None).unwrap();
    let id = value(&p, row, "taskrsrc_id");
    let doc = &mut p.document;
    doc.table_mut("ASGNMNTACAT").add(&[
        ("taskrsrc_id", id.clone()),
        ("asgnmnt_catg_type_id", "1".into()),
        ("asgnmnt_catg_id", "1".into()),
        ("proj_id", "1".into()),
    ]);
    doc.table_mut("TRSRCFIN").add(&[
        ("fin_dates_id", "1".into()),
        ("taskrsrc_id", id.clone()),
        ("task_id", "1010".into()),
        ("proj_id", "1".into()),
        ("act_qty", "40".into()),
    ]);
    doc.table_mut("UDFTYPE").add(&[
        ("udf_type_id", "9".into()),
        ("table_name", "TASKRSRC".into()),
    ]);
    doc.table_mut("UDFVALUE").add(&[
        ("udf_type_id", "9".into()),
        ("fk_id", id.clone()),
        ("proj_id", "1".into()),
        ("udf_text", "old".into()),
    ]);
    p.remove_assignment(row).unwrap();
    let new = p.add_assignment(1020, Some(CREW), None).unwrap();
    let new_id = value(&p, new, "taskrsrc_id");
    let text = p.updated_document().text();
    let again = Document::parse(&text).unwrap();
    for (table, column) in [
        ("ASGNMNTACAT", "taskrsrc_id"),
        ("TRSRCFIN", "taskrsrc_id"),
        ("UDFVALUE", "fk_id"),
    ] {
        let Some(t) = again.table(table) else {
            continue;
        };
        for target in [&id, &new_id] {
            assert!(
                !t.rows.iter().any(|r| t.value(r, column) == target.as_str()),
                "{table} still holds a row for assignment {target}"
            );
        }
    }
}

#[test]
fn added_assignment_takes_the_activity_duration_and_becomes_primary() {
    let mut p = with_crew();
    let i = index(&p, 1010);
    let row = p.add_assignment(1010, Some(CREW), None).unwrap();
    assert_eq!(value(&p, row, "remain_qty"), "24");
    assert_eq!(value(&p, row, "target_qty"), "24");
    assert_eq!(value(&p, row, "remain_qty_per_hr"), "1");
    assert_eq!(value(&p, row, "cost_per_qty"), "85.0000");
    assert_eq!(value(&p, row, "target_cost"), "2040.0000");
    assert_eq!(value(&p, row, "rollup_dates_flag"), "Y");
    assert_eq!(value(&p, row, "rsrc_type"), "RT_Labor");
    assert_eq!(value(&p, row, "guid").len(), 22);
    assert_eq!(p.model.activities[i].rem, 1440);
    assert_eq!(p.model.activities[i].assignments.len(), 1);
    assert_eq!(p.model.activities[i].assignments[0].rem, 1440);
    let view = p.assignment_view(row).unwrap();
    assert!(view.primary, "the first resource becomes primary");
    assert_eq!(view.remaining_duration, 1440);
    // Milestones take no assignments.
    assert!(p.add_assignment(1000, Some(CREW), None).is_err());
    // The exported row reimports as the same assignment.
    let again = reimport(&p);
    assert_eq!(
        again.model.activities[i].assignments,
        p.model.activities[i].assignments
    );
}

#[test]
fn unit_edits_follow_the_duration_type_and_drive_f9() {
    // Fixed Units/Time: more units make the assignment, and so the activity, longer.
    let mut p = with_crew();
    set_duration_type(&mut p, 1010, "DT_FixedRate");
    let i = index(&p, 1010);
    let row = p.add_assignment(1010, Some(CREW), None).unwrap();
    assert!(p.details[&1010].resource_driven_duration);
    p.edit_assignment(row, AssignmentEdit::RemainingUnits(48.))
        .unwrap();
    assert_eq!(p.model.activities[i].rem, 2880);
    assert_eq!(
        p.details[&1010].original_duration, 2880,
        "not started: linked"
    );
    assert_eq!(value(&p, row, "target_qty"), "48");
    assert_eq!(value(&p, row, "remain_cost"), "4080.0000");
    schedule(&mut p);
    let a = &p.model.activities[i];
    let cal = &p.model.calendars[a.cal];
    assert_eq!(
        cal.between(a.early_start.unwrap(), a.early_finish.unwrap()),
        2880
    );
    let again = reimport(&p);
    assert_eq!(again.model.activities[i].rem, 2880);

    // Fixed Duration and Units/Time (DT_FixedDrtn): units change the
    // units/time, not the duration.
    let mut p = with_crew();
    set_duration_type(&mut p, 1010, "DT_FixedDrtn");
    let row = p.add_assignment(1010, Some(CREW), None).unwrap();
    p.edit_assignment(row, AssignmentEdit::RemainingUnits(48.))
        .unwrap();
    assert_eq!(value(&p, row, "remain_qty_per_hr"), "2");
    assert_eq!(p.model.activities[i].rem, 1440);
    // ...and a longer assignment duration makes the activity longer.
    p.edit_assignment(row, AssignmentEdit::RemainingDuration(2400))
        .unwrap();
    assert_eq!(value(&p, row, "remain_qty"), "80");
    assert_eq!(p.model.activities[i].rem, 2400);
    schedule(&mut p);
    let a = &p.model.activities[i];
    assert_eq!(
        p.model.calendars[a.cal].between(a.early_start.unwrap(), a.early_finish.unwrap()),
        2400
    );

    // Fixed Units: a shorter duration raises the units/time and keeps the units.
    let mut p = with_crew();
    set_duration_type(&mut p, 1010, "DT_FixedQty");
    let row = p.add_assignment(1010, Some(CREW), None).unwrap();
    p.edit_assignment(row, AssignmentEdit::RemainingDuration(720))
        .unwrap();
    assert_eq!(value(&p, row, "remain_qty"), "24");
    assert_eq!(value(&p, row, "remain_qty_per_hr"), "2");
    assert_eq!(p.model.activities[i].rem, 720);
    schedule(&mut p);
    assert_eq!(reimport(&p).model.activities[i].rem, 720);
}

#[test]
fn remove_and_drive_flag_update_the_model_and_leveling_inputs() {
    let mut p = with_crew();
    set_duration_type(&mut p, 1020, "DT_FixedQty");
    let i = index(&p, 1020);
    let row = p.add_assignment(1020, Some(CREW), None).unwrap();
    let demand = &p.resources.demands[&1020];
    assert_eq!(demand.len(), 1);
    assert_eq!(demand[0].resource_id, CREW);
    assert_eq!(demand[0].remaining_units, 48.);
    p.edit_assignment(row, AssignmentEdit::RemainingUnitsPerTime(2.))
        .unwrap();
    assert_eq!(p.resources.demands[&1020][0].rate, 2.);
    assert_eq!(p.resources.demands[&1020][0].duration, 1440);
    assert_eq!(
        p.model.activities[i].rem, 1440,
        "Fixed Units: duration follows"
    );
    assert!(p.details[&1020].resource_driven_duration);
    p.edit_assignment(row, AssignmentEdit::DriveActivityDates(false))
        .unwrap();
    assert!(!p.details[&1020].resource_driven_duration);
    assert!(!p.model.activities[i].assignments[0].rollup);
    p.remove_assignment(row).unwrap();
    assert!(p.model.activities[i].assignments.is_empty());
    assert!(!p.resources.demands.contains_key(&1020));
    assert_eq!(
        p.model.activities[i].rem, 1440,
        "removing keeps the duration"
    );
    assert!(
        !p.assignment_view(0).is_some_and(|v| v.primary),
        "primary resource cleared"
    );
    let tasks = p.document.table("TASK").unwrap();
    let r = tasks
        .rows
        .iter()
        .find(|r| tasks.value(r, "task_id") == "1020")
        .unwrap();
    assert_eq!(tasks.value(r, "rsrc_id"), "");
}

#[test]
fn rate_type_price_and_account_edits() {
    let mut p = with_crew();
    p.document.table_mut("ACCOUNT").add(&[
        ("acct_id", "77".into()),
        ("acct_short_name", "6100".into()),
        ("acct_name", "Field labor".into()),
    ]);
    let row = p.add_assignment(1010, Some(CREW), None).unwrap();
    p.edit_assignment(row, AssignmentEdit::RateType("COST_PER_QTY2".into()))
        .unwrap();
    assert_eq!(value(&p, row, "cost_per_qty"), "95.0000");
    assert_eq!(value(&p, row, "target_cost"), "2280.0000");
    p.edit_assignment(row, AssignmentEdit::PricePerUnit(100.))
        .unwrap();
    assert_eq!(value(&p, row, "cost_per_qty_source_type"), "ST_Custom");
    assert_eq!(value(&p, row, "remain_cost"), "2400.0000");
    p.edit_assignment(row, AssignmentEdit::CostAccount(Some(77)))
        .unwrap();
    assert_eq!(value(&p, row, "acct_id"), "77");
    assert!(p
        .edit_assignment(row, AssignmentEdit::CostAccount(Some(5)))
        .is_err());
    assert!(p
        .edit_assignment(row, AssignmentEdit::RemainingUnits(-1.))
        .is_err());
}

#[test]
fn roles_can_be_assigned_and_staffed() {
    let mut p = with_crew();
    p.document.table_mut("ROLES").add(&[
        ("role_id", "5".into()),
        ("role_short_name", "PF".into()),
        ("role_name", "Pipefitter".into()),
    ]);
    p.document
        .table_mut("ROLERATE")
        .add(&[("role_id", "5".into()), ("cost_per_qty", "70".into())]);
    let row = p.add_assignment(1050, None, Some(5)).unwrap();
    assert_eq!(value(&p, row, "rsrc_id"), "");
    assert_eq!(value(&p, row, "role_id"), "5");
    assert_eq!(value(&p, row, "cost_per_qty_source_type"), "ST_Role");
    assert_eq!(value(&p, row, "cost_per_qty"), "70.0000");
    assert!(!p.assignment_view(row).unwrap().primary);
    p.edit_assignment(row, AssignmentEdit::Resource(CREW))
        .unwrap();
    assert_eq!(value(&p, row, "rsrc_id"), "10");
    assert_eq!(value(&p, row, "role_id"), "5");
    assert_eq!(value(&p, row, "cost_per_qty"), "85.0000");
    p.set_primary_resource(row, true).unwrap();
    assert!(p.assignment_view(row).unwrap().primary);
}

#[test]
fn activity_duration_edits_recalculate_assignments() {
    let mut p = with_crew();
    let i = index(&p, 1010);
    set_duration_type(&mut p, 1010, "DT_FixedDrtn");
    let row = p.add_assignment(1010, Some(CREW), None).unwrap();
    // Fixed Duration and Units/Time (DT_FixedDrtn): a longer activity has more units.
    p.model.activities[i].rem = 2400;
    p.details.get_mut(&1010).unwrap().original_duration = 2400;
    p.activity_duration_changed(1010, Some(2400), Some(2400))
        .unwrap();
    assert_eq!(value(&p, row, "remain_qty"), "40");
    assert_eq!(value(&p, row, "target_qty"), "40");
    assert_eq!(p.model.activities[i].assignments[0].rem, 2400);
}

#[test]
fn schedule_write_back_offsets_shorter_and_lagged_assignments() {
    let mut p = with_crew();
    let i = index(&p, 1020);
    let long = p.add_assignment(1020, Some(CREW), None).unwrap();
    let short = p.add_assignment(1020, Some(CREW), None).unwrap();
    // Not driving, so the activity keeps its six days.
    p.edit_assignment(short, AssignmentEdit::DriveActivityDates(false))
        .unwrap();
    p.edit_assignment(short, AssignmentEdit::RemainingDuration(960))
        .unwrap();
    p.document
        .table_mut("TASKRSRC")
        .set(short, "relag_drtn_hr_cnt", "8".into());
    schedule(&mut p);
    let changed = p.write_assignment_schedule();
    assert_eq!(changed, 2);
    let a = p.model.activities[i].clone();
    let origin = p.model.origin_serial;
    let date = |p: &ProjectData, row: usize, k: &str| {
        let t = p.document.table("TASKRSRC").unwrap();
        parse_absolute(t.value(&t.rows[row], k)).map(|v| v - origin * 1440)
    };
    assert_eq!(date(&p, long, "restart_date"), a.restart);
    assert_eq!(date(&p, long, "reend_date"), a.reend);
    assert_eq!(date(&p, long, "rem_late_start_date"), a.rem_late_start);
    assert_eq!(date(&p, long, "rem_late_end_date"), a.rem_late_finish);
    assert_eq!(date(&p, long, "target_start_date"), a.target_start);
    let cal = &p.model.calendars[a.cal];
    let start = cal.add_start(a.restart.unwrap(), 480);
    assert_eq!(date(&p, short, "restart_date"), Some(start));
    assert_eq!(date(&p, short, "reend_date"), Some(cal.add(start, 960)));
    assert_eq!(
        p.write_assignment_schedule(),
        0,
        "a second write changes nothing"
    );
    // Completed activities carry no remaining dates.
    p.model.activities[i].status = StatusType::Completed;
    p.model.activities[i].act_start = a.restart;
    p.model.activities[i].act_finish = a.reend;
    p.write_assignment_schedule();
    assert_eq!(date(&p, long, "restart_date"), None);
    assert_eq!(date(&p, long, "rem_late_end_date"), None);
    assert_eq!(date(&p, long, "act_end_date"), a.reend);
}

#[test]
fn expected_finish_rewrites_remaining_units() {
    let mut p = with_crew();
    set_duration_type(&mut p, 1010, "DT_FixedDrtn");
    let i = index(&p, 1010);
    let row = p.add_assignment(1010, Some(CREW), None).unwrap();
    let dd = p.model.project.data_date.unwrap();
    {
        let a = &mut p.model.activities[i];
        a.status = StatusType::InProgress;
        a.act_start = Some(dd - 1440);
    }
    let cal = p.model.calendars[p.model.activities[i].cal].clone();
    p.model.activities[i].expect_end = Some(cal.add(dd, 4800));
    p.model.options.use_expected_finish = true;
    schedule(&mut p);
    assert_eq!(p.model.activities[i].rem, 4800);
    p.write_assignment_schedule();
    // Fixed Duration and Units/Time (DT_FixedDrtn): units follow the new remaining duration.
    assert_eq!(value(&p, row, "remain_qty"), "80");
    assert_eq!(value(&p, row, "remain_qty_per_hr"), "1");
    assert_eq!(value(&p, row, "remain_cost"), "6800.0000");
}

/// An in-progress activity 1010 with one crew assignment of 24 units at 1
/// per hour (three days), the given duration type and an expected finish
/// ten working days after the data date; scheduled with Use Expected Finish.
fn expected_finish_case(code: &str, driving: bool) -> (ProjectData, usize, usize) {
    let mut p = with_crew();
    set_duration_type(&mut p, 1010, code);
    let i = index(&p, 1010);
    let row = p.add_assignment(1010, Some(CREW), None).unwrap();
    if !driving {
        p.edit_assignment(row, AssignmentEdit::DriveActivityDates(false))
            .unwrap();
    }
    let dd = p.model.project.data_date.unwrap();
    {
        let a = &mut p.model.activities[i];
        a.status = StatusType::InProgress;
        a.act_start = Some(dd - 1440);
    }
    let cal = p.model.calendars[p.model.activities[i].cal].clone();
    p.model.activities[i].expect_end = Some(cal.add(dd, 4800));
    p.model.options.use_expected_finish = true;
    schedule(&mut p);
    assert_eq!(p.model.activities[i].rem, 4800, "{code}");
    (p, i, row)
}

/// Remaining units after an expected finish moves the remaining duration:
/// the types whose units follow the duration (Fixed Duration and
/// Units/Time, Fixed Units/Time) scale the units at the same units/time;
/// the types that fix the units (Fixed Duration and Units, Fixed Units)
/// keep them and spread them at a new units/time. The model assignment
/// takes the new duration, and the engine dates do not change.
#[test]
fn expected_finish_remaining_units_follow_the_duration_type() {
    for (code, qty, per_hr, cost) in [
        ("DT_FixedDrtn", "80", "1", "6800.0000"),
        ("DT_FixedRate", "80", "1", "6800.0000"),
        ("DT_FixedDUR2", "24", "0.3", "2040.0000"),
        ("DT_FixedQty", "24", "0.3", "2040.0000"),
    ] {
        let (mut p, i, row) = expected_finish_case(code, true);
        let dates = |p: &ProjectData| {
            p.model
                .activities
                .iter()
                .map(|a| (a.restart, a.reend, a.rem_late_start, a.rem_late_finish))
                .collect::<Vec<_>>()
        };
        let scheduled = dates(&p);
        p.write_assignment_schedule();
        assert_eq!(value(&p, row, "remain_qty"), qty, "{code}");
        assert_eq!(value(&p, row, "remain_qty_per_hr"), per_hr, "{code}");
        assert_eq!(value(&p, row, "remain_cost"), cost, "{code}");
        assert_eq!(value(&p, row, "target_qty"), "24", "{code} budget kept");
        assert_eq!(
            p.model.activities[i].assignments[0].rem, 4800,
            "{code}: the model assignment takes the new duration"
        );
        assert_eq!(dates(&p), scheduled, "{code}: write-back moves no dates");
        let view = p.assignment_view(row).unwrap();
        assert_eq!(view.remaining_duration, 4800, "{code}");
        // The rewritten row schedules the same, now and after a reimport.
        let mut again = p.clone();
        schedule(&mut again);
        assert_eq!(dates(&again), scheduled, "{code}: a second F9");
        assert_eq!(again.write_assignment_schedule(), 0, "{code}: idempotent");
        let mut back = reimport(&p);
        back.model.options.use_expected_finish = true;
        let row_back = back.assignment_rows(1010)[0];
        assert_eq!(value(&back, row_back, "remain_qty"), qty, "{code}");
        assert_eq!(
            back.model.activities[index(&back, 1010)].assignments[0].rem,
            4800,
            "{code}: the reimport reads the same duration"
        );
    }
}

/// A non-driving assignment keeps its own units and duration; units with
/// no units/time keep their units and gain the units/time that spreads
/// them over the new remaining duration.
#[test]
fn expected_finish_leaves_non_driving_rows_and_spreads_rateless_units() {
    let (mut p, _, row) = expected_finish_case("DT_FixedDrtn", false);
    p.write_assignment_schedule();
    assert_eq!(value(&p, row, "remain_qty"), "24");
    assert_eq!(value(&p, row, "remain_qty_per_hr"), "1");

    let (mut p, _, row) = expected_finish_case("DT_FixedDrtn", true);
    p.document
        .table_mut("TASKRSRC")
        .set(row, "remain_qty_per_hr", "0".into());
    p.write_assignment_schedule();
    assert_eq!(value(&p, row, "remain_qty"), "24", "units are kept");
    assert_eq!(value(&p, row, "remain_qty_per_hr"), "0.3");
}

fn corpus() -> Option<Vec<PathBuf>> {
    let dir = std::env::var_os("OPENXER_CORPUS")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("OPENXER_CORPUS is not set"));
    let Ok(entries) = fs::read_dir(&dir) else {
        println!(
            "SKIPPED: XER corpus not found at {} (set OPENXER_CORPUS)",
            dir.display()
        );
        return None;
    };
    let mut files: Vec<_> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("xer")))
        .collect();
    files.sort();
    Some(files)
}

const DATE_COLUMNS: [&str; 8] = [
    "act_start_date",
    "act_end_date",
    "restart_date",
    "reend_date",
    "rem_late_start_date",
    "rem_late_end_date",
    "target_start_date",
    "target_end_date",
];

#[derive(Default)]
struct Tally {
    /// Per column: (matches, total).
    columns: BTreeMap<&'static str, (usize, usize)>,
    /// Per file: (matches, total).
    files: BTreeMap<String, (usize, usize)>,
    examples: Vec<String>,
}
impl Tally {
    fn add(
        &mut self,
        name: &str,
        column: &'static str,
        same: bool,
        example: impl FnOnce() -> String,
    ) {
        let c = self.columns.entry(column).or_default();
        let f = self.files.entry(name.to_string()).or_default();
        c.1 += 1;
        f.1 += 1;
        if same {
            c.0 += 1;
            f.0 += 1;
        } else if self.examples.len() < 40 {
            self.examples.push(example());
        }
    }
    fn report(&self, title: &str) -> (usize, usize) {
        let (m, t) = self
            .columns
            .values()
            .fold((0, 0), |(m, t), (a, b)| (m + a, t + b));
        println!(
            "{title}: {m}/{t} cells match ({:.3}%)",
            100. * m as f64 / t.max(1) as f64
        );
        for (c, (a, b)) in &self.columns {
            println!("  {c}: {a}/{b}");
        }
        for (f, (a, b)) in &self.files {
            if a != b {
                println!("  {f}: {a}/{b}");
            }
        }
        for e in self.examples.iter().take(16) {
            println!("    {e}");
        }
        (m, t)
    }
}

/// Compare written TASKRSRC date cells with the ones the reference stored. `all` counts
/// every cell; `settled` only cells whose activity has the same value in the
/// written TASK row as in the reference's, which isolates the assignment rule from
/// scheduling differences.
fn compare(
    stored: &Document,
    written: &ProjectData,
    name: &str,
    all: &mut Tally,
    settled: &mut Tally,
) {
    let pid = written.model.project.proj_id;
    let doc = written.updated_document();
    let (Some(a), Some(b)) = (stored.table("TASKRSRC"), doc.table("TASKRSRC")) else {
        return;
    };
    let (Some(ta), Some(tb)) = (stored.table("TASK"), doc.table("TASK")) else {
        return;
    };
    let task = |t: &openxer_xer::Table, id: &str| {
        t.rows
            .iter()
            .find(|r| t.value(r, "task_id") == id)
            .cloned()
            .unwrap_or_default()
    };
    assert_eq!(a.rows.len(), b.rows.len());
    for (ra, rb) in a.rows.iter().zip(&b.rows) {
        if a.value(ra, "proj_id") != pid.to_string() {
            continue;
        }
        let id = a.value(ra, "task_id");
        let (sa, sb) = (task(ta, id), task(tb, id));
        for c in DATE_COLUMNS {
            let same = a.value(ra, c) == b.value(rb, c);
            let example = || {
                format!(
                    "{name} task {id} {c}: reference {:?} written {:?}",
                    a.value(ra, c),
                    b.value(rb, c)
                )
            };
            all.add(name, c, same, example);
            if ta.value(&sa, c) == tb.value(&sb, c) {
                settled.add(name, c, same, example);
            }
        }
    }
}

/// The write-back rule against the reference's stored TASKRSRC dates:
/// 1. rule only: the imported (reference-scheduled) activity dates, written back
///    without scheduling, must reproduce the stored assignment dates;
/// 2. fresh F9 at the same data date, then write back: every cell, and the
///    cells whose activity date the fresh schedule reproduced.
///
/// Prints the match rates; asserts floors the current corpus meets.
#[test]
fn corpus_assignment_dates_match_the_stored_dates() {
    let Some(files) = corpus() else {
        return;
    };
    let mut rule = Tally::default();
    let mut ignore = Tally::default();
    let mut fresh = Tally::default();
    let mut fresh_settled = Tally::default();
    let mut projects = 0;
    for path in files {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let Ok(document) = Document::read(&path) else {
            continue;
        };
        for (pid, _) in document.projects() {
            let Ok(project) = ProjectData::from_document(document.clone(), pid, name.clone())
            else {
                println!("SKIPPED {name} {pid}: not importable");
                continue;
            };
            projects += 1;
            let mut unscheduled = project.clone();
            unscheduled.write_assignment_schedule();
            compare(&document, &unscheduled, &name, &mut rule, &mut ignore);
            // XER does not say whether the stored dates were leveled (369
            // was), so use whichever run reproduces more of the reference's
            // stored activity remaining dates.
            let mut best: Option<(usize, ProjectData)> = None;
            for level in [false, true] {
                let mut run = project.clone();
                run.model.options.do_level = level;
                run.refresh_derived_durations();
                if scheduler::schedule_with_resources(&mut run.model, &run.resources).is_err() {
                    continue;
                }
                let score = activity_date_matches(&document, &run);
                if best.as_ref().is_none_or(|(s, _)| score > *s) {
                    best = Some((score, run));
                }
            }
            let Some((_, mut scheduled)) = best else {
                println!("SKIPPED {name} {pid}: schedule rejected");
                continue;
            };
            if scheduled.model.options.do_level {
                println!("{name} {pid}: the leveled run matches more stored dates");
            }
            scheduled.write_assignment_schedule();
            compare(&document, &scheduled, &name, &mut fresh, &mut fresh_settled);
            // The written file reimports with the same assignments.
            let again = ProjectData::from_document(
                Document::parse(&scheduled.updated_document().text()).unwrap(),
                pid,
                name.clone(),
            )
            .unwrap();
            for (x, y) in again
                .model
                .activities
                .iter()
                .zip(&scheduled.model.activities)
            {
                assert_eq!(x.assignments.len(), y.assignments.len());
                assert_eq!(x.rem, y.rem);
            }
        }
    }
    println!("{projects} corpus projects");
    let (m, t) = rule.report("Rule only (imported reference activity dates)");
    assert!(m * 1000 >= t * 999, "rule-only match rate fell below 99.9%");
    let (m, t) = fresh_settled.report("Fresh F9, cells whose activity date matches the reference");
    assert!(m * 1000 >= t * 999, "settled match rate fell below 99.9%");
    fresh.report("Fresh F9, every cell");
}

/// Activities whose written remaining dates equal the reference's stored ones.
fn activity_date_matches(stored: &Document, p: &ProjectData) -> usize {
    let written = p.updated_document();
    let (Some(a), Some(b)) = (stored.table("TASK"), written.table("TASK")) else {
        return 0;
    };
    a.rows
        .iter()
        .zip(&b.rows)
        .filter(|(x, y)| {
            a.value(x, "restart_date") == b.value(y, "restart_date")
                && a.value(x, "reend_date") == b.value(y, "reend_date")
        })
        .count()
}

#[test]
fn fixed_duration_and_units_is_dt_fixed_dur2() {
    use openxer_xer::assignments::DurationType;
    assert_eq!(
        DurationType::from_code("DT_FixedDUR2"),
        DurationType::FixedDurationAndUnits
    );
    assert_eq!(
        DurationType::from_code("DT_FixedDrtn"),
        DurationType::FixedDurationAndUnitsPerTime
    );
    assert_eq!(DurationType::FixedDurationAndUnits.code(), "DT_FixedDUR2");
    // Under DT_FixedDUR2 a unit edit changes units/time, never the duration.
    let mut p = with_crew();
    let i = index(&p, 1010);
    let before = p.model.activities[i].rem;
    let row = p.add_assignment(1010, Some(CREW), None).unwrap();
    p.edit_assignment(row, AssignmentEdit::RemainingUnits(48.))
        .unwrap();
    assert_eq!(p.model.activities[i].rem, before);
}

/// Scheduling inputs of an activity's model assignments (the late dates are
/// scheduling output).
fn inputs(p: &ProjectData, i: usize) -> Vec<(i64, bool, i64, i64, i64)> {
    p.model.activities[i]
        .assignments
        .iter()
        .map(|a| (a.rsrc_id, a.rollup, a.rem, a.remain_lag, a.relag))
        .collect()
}

/// A second crew with its own default units/time and overtime factor.
fn with_two_crews() -> ProjectData {
    let mut p = with_crew();
    p.document.table_mut("RSRC").add(&[
        ("rsrc_id", "11".into()),
        ("rsrc_name", "Weld crew".into()),
        ("rsrc_short_name", "WELD".into()),
        ("rsrc_type", "RT_Labor".into()),
        ("def_qty_per_hr", "2".into()),
        ("ot_factor", "1.5".into()),
        ("clndr_id", "1".into()),
    ]);
    p.document.table_mut("RSRCRATE").add(&[
        ("rsrc_id", "11".into()),
        ("start_date", "2020-01-01 00:00".into()),
        ("max_qty_per_hr", "2".into()),
        ("cost_per_qty", "90".into()),
    ]);
    p
}

#[test]
fn lag_edits_move_the_assignment_and_survive_f9_and_reimport() {
    let mut p = with_crew();
    let i = index(&p, 1020);
    let row = p.add_assignment(1020, Some(CREW), None).unwrap();
    p.edit_assignment(row, AssignmentEdit::Lag(480)).unwrap();
    assert_eq!(value(&p, row, "relag_drtn_hr_cnt"), "8");
    assert_eq!(
        value(&p, row, "target_lag_drtn_hr_cnt"),
        "8",
        "budget and remaining are linked before the start"
    );
    let view = p.assignment_view(row).unwrap();
    assert_eq!((view.lag, view.planned_lag), (480, 480));
    assert_eq!(p.model.activities[i].assignments[0].remain_lag, 480);
    assert!(p.edit_assignment(row, AssignmentEdit::Lag(-60)).is_err());
    p.edit_assignment(row, AssignmentEdit::PlannedLag(240))
        .unwrap();
    assert_eq!(value(&p, row, "target_lag_drtn_hr_cnt"), "4");
    assert_eq!(value(&p, row, "relag_drtn_hr_cnt"), "8");
    schedule(&mut p);
    p.write_assignment_schedule();
    let a = p.model.activities[i].clone();
    let cal = &p.model.calendars[a.cal];
    let origin = p.model.origin_serial;
    let t = p.document.table("TASKRSRC").unwrap();
    let start = parse_absolute(t.value(&t.rows[row], "restart_date")).map(|v| v - origin * 1440);
    assert_eq!(start, Some(cal.add_start(a.restart.unwrap(), 480)));
    // The file and the model agree after a reimport.
    let again = reimport(&p);
    assert_eq!(inputs(&again, i), inputs(&p, i));
    assert_eq!(again.model.activities[i].rem, p.model.activities[i].rem);
    assert_eq!(value(&again, row, "relag_drtn_hr_cnt"), "8");
}

#[test]
fn actual_units_and_costs_follow_the_project_settings() {
    let mut p = with_two_crews();
    let i = index(&p, 1010);
    let row = p.add_assignment(1010, Some(CREW), None).unwrap();
    assert!(
        p.edit_assignment(row, AssignmentEdit::ActualRegularUnits(8.))
            .is_err(),
        "a not started activity has no actuals"
    );
    let dd = p.model.project.data_date.unwrap();
    p.model.activities[i].status = StatusType::InProgress;
    p.model.activities[i].act_start = Some(dd - 1440);
    // Add Actual to Remaining (the default): remaining units stay.
    p.edit_assignment(row, AssignmentEdit::ActualRegularUnits(8.))
        .unwrap();
    assert_eq!(value(&p, row, "act_reg_qty"), "8");
    assert_eq!(value(&p, row, "act_reg_cost"), "680.0000");
    assert_eq!(value(&p, row, "remain_qty"), "24");
    // Subtract Actual from At Completion: remaining units take the change.
    let t = p.document.table_mut("PROJECT");
    t.set(0, "add_act_remain_flag", "N".into());
    p.edit_assignment(row, AssignmentEdit::ActualRegularUnits(12.))
        .unwrap();
    assert_eq!(value(&p, row, "remain_qty"), "20");
    p.edit_assignment(row, AssignmentEdit::OvertimeFactor(1.5))
        .unwrap();
    p.edit_assignment(row, AssignmentEdit::ActualOvertimeUnits(2.))
        .unwrap();
    assert_eq!(value(&p, row, "act_ot_qty"), "2");
    assert_eq!(value(&p, row, "act_ot_cost"), "255.0000");
    assert_eq!(value(&p, row, "remain_qty"), "18");
    p.edit_assignment(row, AssignmentEdit::ActualRegularCost(1000.))
        .unwrap();
    assert_eq!(value(&p, row, "act_reg_cost"), "1000.0000");
    let view = p.assignment_view(row).unwrap();
    assert_eq!(view.actual_units, 14.);
    // F9 and a reimport keep the actuals and the remaining units.
    schedule(&mut p);
    p.write_assignment_schedule();
    let again = reimport(&p);
    assert_eq!(value(&again, row, "act_reg_qty"), "12");
    assert_eq!(value(&again, row, "remain_qty"), "18");
    assert_eq!(inputs(&again, i), inputs(&p, i));
    assert_eq!(
        format!("{:?}", again.resources.demands),
        format!("{:?}", p.resources.demands)
    );
}

#[test]
fn recalculate_option_shares_units_when_assignments_are_added_or_removed() {
    // Fixed Units: the units stay, the duration shortens.
    let mut p = with_two_crews();
    set_duration_type(&mut p, 1010, "DT_FixedQty");
    let i = index(&p, 1010);
    let first = p.add_assignment(1010, Some(CREW), None).unwrap();
    let second = p.add_assignment_with(1010, Some(11), None, true).unwrap();
    assert_eq!(value(&p, first, "remain_qty"), "8");
    assert_eq!(value(&p, second, "remain_qty"), "16");
    assert_eq!(p.model.activities[i].rem, 480);
    // Removing one gives its units back to the other.
    p.remove_assignment_with(second, true).unwrap();
    assert_eq!(value(&p, first, "remain_qty"), "24");
    assert_eq!(p.model.activities[i].rem, 1440);

    // Fixed Duration and Units: the duration stays, units/time drops.
    let mut p = with_two_crews();
    set_duration_type(&mut p, 1010, "DT_FixedDUR2");
    let first = p.add_assignment(1010, Some(CREW), None).unwrap();
    let second = p.add_assignment_with(1010, Some(CREW), None, true).unwrap();
    assert_eq!(value(&p, first, "remain_qty"), "12");
    assert_eq!(value(&p, second, "remain_qty_per_hr"), "0.5");
    assert_eq!(p.model.activities[i].rem, 1440);

    // Preserve (the default) leaves the first assignment alone.
    let mut p = with_two_crews();
    set_duration_type(&mut p, 1010, "DT_FixedQty");
    let first = p.add_assignment(1010, Some(CREW), None).unwrap();
    p.add_assignment_with(1010, Some(11), None, false).unwrap();
    assert_eq!(value(&p, first, "remain_qty"), "24");
}

#[test]
fn replacing_a_resource_follows_the_staffing_choice() {
    let mut p = with_two_crews();
    let row = p.add_assignment(1010, Some(CREW), None).unwrap();
    p.replace_resource(row, 11, false, false).unwrap();
    assert_eq!(value(&p, row, "rsrc_id"), "11");
    assert_eq!(value(&p, row, "remain_qty_per_hr"), "1", "kept");
    assert_eq!(value(&p, row, "cost_per_qty"), "90.0000");
    p.replace_resource(row, CREW, false, false).unwrap();
    p.replace_resource(row, 11, true, false).unwrap();
    assert_eq!(
        value(&p, row, "remain_qty_per_hr"),
        "2",
        "the new resource's"
    );
    assert_eq!(value(&p, row, "remain_qty"), "48");
    assert_eq!(value(&p, row, "ot_factor"), "1.5");
    // A shared role prices the assignment from the role when asked to.
    p.document
        .table_mut("ROLES")
        .add(&[("role_id", "5".into()), ("role_short_name", "PF".into())]);
    p.document
        .table_mut("ROLERATE")
        .add(&[("role_id", "5".into()), ("cost_per_qty", "70".into())]);
    p.edit_assignment(row, AssignmentEdit::Role(Some(5)))
        .unwrap();
    p.replace_resource(row, CREW, false, true).unwrap();
    assert_eq!(value(&p, row, "cost_per_qty"), "70.0000");
    assert_eq!(value(&p, row, "cost_per_qty_source_type"), "ST_Role");
}

/// Model minutes of the assignment's stored date cell.
fn cell_date(p: &ProjectData, row: usize, column: &str) -> Option<i64> {
    parse_absolute(&value(p, row, column)).map(|v| v - p.model.origin_serial * 1440)
}

/// 1010 in progress since a day before the data date, with one crew assignment.
fn in_progress_case() -> (ProjectData, usize, usize) {
    let mut p = with_crew();
    let i = index(&p, 1010);
    let row = p.add_assignment(1010, Some(CREW), None).unwrap();
    let dd = p.model.project.data_date.unwrap();
    {
        let a = &mut p.model.activities[i];
        a.status = StatusType::InProgress;
        a.act_start = Some(dd - 1440);
    }
    schedule(&mut p);
    p.write_assignment_schedule();
    (p, i, row)
}

#[test]
fn actual_dates_are_checked_stored_kept_by_f9_and_round_trip() {
    let (mut p, i, row) = in_progress_case();
    let dd = p.model.project.data_date.unwrap();
    let start = dd - 1440;
    assert_eq!(cell_date(&p, row, "act_start_date"), Some(start));
    assert_eq!(p.assignment_view(row).unwrap().dates.act_start, Some(start));
    // Refused: after the data date, before the activity started, unstarted
    // activities, a finish before the start.
    let refused = |p: &mut ProjectData, edit| p.edit_assignment(row, edit).unwrap_err();
    assert!(refused(&mut p, AssignmentEdit::ActualStart(dd + 60)).contains("data date"));
    assert!(
        refused(&mut p, AssignmentEdit::ActualStart(start - 60)).contains("before its activity")
    );
    assert!(refused(&mut p, AssignmentEdit::ActualFinish(dd + 60)).contains("data date"));
    assert!(refused(&mut p, AssignmentEdit::ActualFinish(start - 60))
        .contains("before the actual start"));
    let other = p.add_assignment(1020, Some(CREW), None).unwrap();
    assert!(p
        .edit_assignment(other, AssignmentEdit::ActualStart(start))
        .unwrap_err()
        .contains("started activity"));
    // A valid start and finish are stored, and one undoable change each:
    // the rows alone hold them.
    let entered_start = start + 480;
    let entered_finish = dd - 60;
    p.edit_assignment(row, AssignmentEdit::ActualStart(entered_start))
        .unwrap();
    p.edit_assignment(row, AssignmentEdit::ActualFinish(entered_finish))
        .unwrap();
    assert_eq!(cell_date(&p, row, "act_start_date"), Some(entered_start));
    assert_eq!(cell_date(&p, row, "act_end_date"), Some(entered_finish));
    assert!(
        refused(&mut p, AssignmentEdit::ActualStart(entered_finish + 30)).contains("actual finish")
    );
    // F9 and the write-back keep them.
    schedule(&mut p);
    p.write_assignment_schedule();
    assert_eq!(cell_date(&p, row, "act_start_date"), Some(entered_start));
    assert_eq!(cell_date(&p, row, "act_end_date"), Some(entered_finish));
    let view = p.assignment_view(row).unwrap();
    assert_eq!(view.dates.act_start, Some(entered_start));
    assert_eq!(view.dates.act_finish, Some(entered_finish));
    assert_eq!(p.write_assignment_schedule(), 0, "idempotent");
    // XER round trip: export, reimport, schedule, write: same dates.
    let mut back = reimport(&p);
    let back_row = back.assignment_rows(1010)[0];
    assert_eq!(
        cell_date(&back, back_row, "act_start_date"),
        Some(entered_start)
    );
    assert_eq!(
        back.model.activities[index(&back, 1010)].act_start,
        Some(start),
        "the activity's own actual start is unchanged"
    );
    schedule(&mut back);
    back.write_assignment_schedule();
    assert_eq!(
        cell_date(&back, back_row, "act_start_date"),
        Some(entered_start)
    );
    assert_eq!(
        cell_date(&back, back_row, "act_end_date"),
        Some(entered_finish)
    );
    // An activity whose actual start moves past the assignment's takes the
    // assignment with it (the entered date is no longer consistent).
    p.model.activities[i].act_start = Some(entered_start + 120);
    p.write_assignment_schedule();
    assert_eq!(
        cell_date(&p, row, "act_start_date"),
        Some(entered_start + 120)
    );
}

#[test]
fn completed_activities_allow_actual_dates_up_to_their_own_finish() {
    let (mut p, i, row) = in_progress_case();
    let dd = p.model.project.data_date.unwrap();
    {
        let a = &mut p.model.activities[i];
        a.status = StatusType::Completed;
        a.act_finish = Some(dd - 120);
    }
    p.write_assignment_schedule();
    assert_eq!(cell_date(&p, row, "act_end_date"), Some(dd - 120));
    assert!(p
        .edit_assignment(row, AssignmentEdit::ActualFinish(dd - 60))
        .unwrap_err()
        .contains("after its activity"));
    p.edit_assignment(row, AssignmentEdit::ActualFinish(dd - 180))
        .unwrap();
    p.write_assignment_schedule();
    assert_eq!(cell_date(&p, row, "act_end_date"), Some(dd - 180));
    // Completed activities have no remaining dates to enter.
    for edit in [
        AssignmentEdit::RemainingStart(dd + 480),
        AssignmentEdit::RemainingFinish(dd + 960),
    ] {
        assert!(p
            .edit_assignment(row, edit)
            .unwrap_err()
            .contains("no remaining dates"));
    }
}

#[test]
fn remaining_start_and_finish_are_stored_as_lag_and_duration_and_survive_f9() {
    let mut p = with_crew();
    let i = index(&p, 1020);
    let row = p.add_assignment(1020, Some(CREW), None).unwrap();
    // Not driving, so the activity keeps its own dates.
    p.edit_assignment(row, AssignmentEdit::DriveActivityDates(false))
        .unwrap();
    schedule(&mut p);
    p.write_assignment_schedule();
    let a = p.model.activities[i].clone();
    let dd = p.model.project.data_date.unwrap();
    let cal = p.model.calendars[a.cal].clone();
    assert_eq!(cell_date(&p, row, "restart_date"), a.restart);
    // Refused: before the data date, before the activity's own start.
    let start_of_activity = a.restart.unwrap();
    assert!(p
        .edit_assignment(row, AssignmentEdit::RemainingStart(dd - 60))
        .unwrap_err()
        .contains("data date"));
    if start_of_activity > dd {
        assert!(p
            .edit_assignment(row, AssignmentEdit::RemainingStart(start_of_activity - 60))
            .unwrap_err()
            .contains("before its activity"));
    }
    // A start one work day later is a lag of one day.
    let later = cal.add_start(start_of_activity, 480);
    p.edit_assignment(row, AssignmentEdit::RemainingStart(later))
        .unwrap();
    assert_eq!(p.assignment_view(row).unwrap().lag, 480);
    assert_eq!(value(&p, row, "relag_drtn_hr_cnt"), "8");
    // Then a finish two work days after that start is a two day duration.
    let finish = cal.add(later, 960);
    p.edit_assignment(row, AssignmentEdit::RemainingFinish(finish))
        .unwrap();
    assert_eq!(p.assignment_view(row).unwrap().remaining_duration, 960);
    assert!(p
        .edit_assignment(row, AssignmentEdit::RemainingFinish(later - 60))
        .is_err());
    // F9 reproduces both dates, now and after export and reimport.
    for _ in 0..2 {
        schedule(&mut p);
        p.write_assignment_schedule();
        assert_eq!(cell_date(&p, row, "restart_date"), Some(later));
        assert_eq!(cell_date(&p, row, "reend_date"), Some(finish));
        let view = p.assignment_view(row).unwrap();
        assert_eq!(view.dates.rem_start, Some(later));
        assert_eq!(view.dates.rem_finish, Some(finish));
    }
    let mut back = reimport(&p);
    let back_row = back.assignment_rows(1020)[0];
    schedule(&mut back);
    back.write_assignment_schedule();
    assert_eq!(cell_date(&back, back_row, "restart_date"), Some(later));
    assert_eq!(cell_date(&back, back_row, "reend_date"), Some(finish));
}

#[test]
fn remaining_dates_of_a_started_activity_follow_the_activity() {
    let (mut p, _, row) = in_progress_case();
    let dd = p.model.project.data_date.unwrap();
    // F9 ignores lag once an activity has started, so a remaining start
    // would be overwritten: it is refused with the reason.
    let err = p
        .edit_assignment(row, AssignmentEdit::RemainingStart(dd + 960))
        .unwrap_err();
    assert!(err.contains("remaining duration"), "{err}");
    // The remaining finish is the remaining duration from the activity's
    // remaining start, so it can be entered and F9 keeps it (not driving).
    p.edit_assignment(row, AssignmentEdit::DriveActivityDates(false))
        .unwrap();
    let a = p.model.activities[index(&p, 1010)].clone();
    let cal = p.model.calendars[a.cal].clone();
    let start = a.restart.unwrap();
    let finish = cal.add(start, 240);
    p.edit_assignment(row, AssignmentEdit::RemainingFinish(finish))
        .unwrap();
    assert_eq!(p.assignment_view(row).unwrap().remaining_duration, 240);
    schedule(&mut p);
    p.write_assignment_schedule();
    assert_eq!(cell_date(&p, row, "restart_date"), Some(start));
    assert_eq!(cell_date(&p, row, "reend_date"), Some(finish));
}

fn set_option(p: &mut ProjectData, column: &str, text: &str) {
    p.document.table_mut("PROJECT").set(0, column, text.into());
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

#[test]
fn costs_and_actual_this_period_follow_the_calculation_options() {
    let (mut p, i, row) = in_progress_case();
    set_option(&mut p, "act_this_per_link_flag", "Y");
    // Actual units move the period figure and the activity's totals.
    p.edit_assignment(row, AssignmentEdit::ActualRegularUnits(8.))
        .unwrap();
    assert_eq!(value(&p, row, "act_this_per_qty"), "8");
    assert_eq!(value(&p, row, "act_this_per_cost"), "680.0000");
    assert_eq!(task_cell(&p, 1010, "act_work_qty"), "8");
    assert_eq!(task_cell(&p, 1010, "act_this_per_work_qty"), "8");
    assert_eq!(task_cell(&p, 1010, "target_work_qty"), "24");
    // Editing the period moves the actual to date by the same amount.
    p.edit_assignment(row, AssignmentEdit::ThisPeriodUnits(5.))
        .unwrap();
    assert_eq!(value(&p, row, "act_reg_qty"), "5");
    assert_eq!(value(&p, row, "act_this_per_qty"), "5");
    assert_eq!(value(&p, row, "act_reg_cost"), "425.0000");
    assert_eq!(value(&p, row, "act_this_per_cost"), "425.0000");
    assert!(p
        .edit_assignment(row, AssignmentEdit::ThisPeriodUnits(-1.))
        .is_err());
    // Actuals beyond the period figure stay out of it.
    p.edit_assignment(row, AssignmentEdit::ThisPeriodUnits(0.))
        .unwrap();
    assert_eq!(value(&p, row, "act_reg_qty"), "0");
    p.edit_assignment(row, AssignmentEdit::ThisPeriodUnits(5.))
        .unwrap();
    // Without the link only the period figure is set.
    set_option(&mut p, "act_this_per_link_flag", "N");
    p.edit_assignment(row, AssignmentEdit::ThisPeriodUnits(2.))
        .unwrap();
    assert_eq!(value(&p, row, "act_reg_qty"), "5");
    assert_eq!(value(&p, row, "act_this_per_qty"), "2");
    assert_eq!(task_cell(&p, 1010, "act_this_per_work_qty"), "2");
    p.edit_assignment(row, AssignmentEdit::ThisPeriodCost(30.))
        .unwrap();
    assert_eq!(value(&p, row, "act_this_per_cost"), "30.0000");
    assert_eq!(value(&p, row, "act_reg_cost"), "425.0000");
    set_option(&mut p, "act_this_per_link_flag", "Y");
    p.edit_assignment(row, AssignmentEdit::ActualRegularUnits(7.))
        .unwrap();
    assert_eq!(
        value(&p, row, "act_this_per_qty"),
        "4",
        "2 plus the 2 added"
    );
    // Costs go through the price: budget and remaining cost set the units.
    p.edit_assignment(row, AssignmentEdit::BudgetedCost(1700.))
        .unwrap();
    assert_eq!(value(&p, row, "target_qty"), "20");
    assert_eq!(value(&p, row, "target_cost"), "1700.0000");
    p.edit_assignment(row, AssignmentEdit::RemainingCost(850.))
        .unwrap();
    assert_eq!(value(&p, row, "remain_qty"), "10");
    assert_eq!(value(&p, row, "remain_cost"), "850.0000");
    assert_eq!(task_cell(&p, 1010, "remain_work_qty"), "10");
    assert_eq!(task_cell(&p, 1010, "target_work_qty"), "20");
    // Unlinked remaining cost is kept on its own.
    p.document
        .table_mut("TASKRSRC")
        .set(row, "cost_qty_link_flag", "N".into());
    p.edit_assignment(row, AssignmentEdit::RemainingCost(900.))
        .unwrap();
    assert_eq!(value(&p, row, "remain_cost"), "900.0000");
    assert_eq!(value(&p, row, "remain_qty"), "10");
    // A refused edit changes nothing.
    let before = p.document.table("TASKRSRC").unwrap().rows.clone();
    p.document
        .table_mut("TASKRSRC")
        .set(row, "cost_per_qty", "0".into());
    let before_price = p.document.table("TASKRSRC").unwrap().rows.clone();
    assert!(before != before_price);
    assert!(p
        .edit_assignment(row, AssignmentEdit::BudgetedCost(10.))
        .is_err());
    assert_eq!(p.document.table("TASKRSRC").unwrap().rows, before_price);
    // F9 and a reimport keep all of it.
    schedule(&mut p);
    p.write_assignment_schedule();
    let again = reimport(&p);
    for column in [
        "act_reg_qty",
        "act_this_per_qty",
        "remain_cost",
        "target_qty",
    ] {
        assert_eq!(
            value(&again, row, column),
            value(&p, row, column),
            "{column}"
        );
    }
    assert_eq!(inputs(&again, i), inputs(&p, i));
}

#[test]
fn an_activity_without_assignments_carries_its_own_units() {
    use openxer_xer::assignments::{ResourceSlot, UnitsPhase};
    let mut p = starter();
    let i = index(&p, 1010);
    let own = |p: &ProjectData| p.activity_own_units(1010, ResourceSlot::Labor);
    // Budgeted and remaining move together until the activity starts.
    assert!(p
        .edit_activity_units(i, ResourceSlot::Labor, UnitsPhase::Budgeted, 40.)
        .unwrap());
    assert_eq!(own(&p), [40., 0., 40., 0.]);
    p.edit_activity_units(i, ResourceSlot::Labor, UnitsPhase::Remaining, 36.)
        .unwrap();
    assert_eq!(own(&p), [36., 0., 36., 0.]);
    // Actuals need a started activity; a refusal changes nothing.
    assert!(p
        .edit_activity_units(i, ResourceSlot::Labor, UnitsPhase::Actual, 8.)
        .is_err());
    assert!(p
        .edit_activity_units(i, ResourceSlot::Labor, UnitsPhase::Budgeted, -1.)
        .is_err());
    assert!(p
        .edit_activity_units(i, ResourceSlot::Labor, UnitsPhase::Budgeted, f64::NAN)
        .is_err());
    assert_eq!(own(&p), [36., 0., 36., 0.]);
    assert!(p
        .edit_activity_units(i, ResourceSlot::Material, UnitsPhase::Budgeted, 1.)
        .is_err());
    let dd = p.model.project.data_date.unwrap();
    p.model.activities[i].status = StatusType::InProgress;
    p.model.activities[i].act_start = Some(dd - 1440);
    // Add Actual to Remaining: remaining stays; the period follows when linked.
    p.edit_activity_units(i, ResourceSlot::Labor, UnitsPhase::Actual, 8.)
        .unwrap();
    assert_eq!(own(&p), [36., 8., 36., 0.]);
    set_option(&mut p, "act_this_per_link_flag", "Y");
    set_option(&mut p, "add_act_remain_flag", "N");
    p.edit_activity_units(i, ResourceSlot::Labor, UnitsPhase::Actual, 12.)
        .unwrap();
    assert_eq!(own(&p), [36., 12., 32., 4.], "remaining takes the change");
    p.edit_activity_units(i, ResourceSlot::Labor, UnitsPhase::ThisPeriod, 6.)
        .unwrap();
    assert_eq!(own(&p), [36., 14., 30., 6.]);
    // Budget and remaining are separate once the activity has started.
    p.edit_activity_units(i, ResourceSlot::Labor, UnitsPhase::Budgeted, 50.)
        .unwrap();
    assert_eq!(own(&p), [50., 14., 30., 6.]);
    // Equipment units use the equipment quantities.
    p.edit_activity_units(i, ResourceSlot::Nonlabor, UnitsPhase::Actual, 3.)
        .unwrap();
    assert_eq!(task_cell(&p, 1010, "act_equip_qty"), "3");
    assert_eq!(task_cell(&p, 1010, "act_work_qty"), "14");
    // F9, export and reimport keep the values.
    schedule(&mut p);
    let again = reimport(&p);
    assert_eq!(again.activity_own_units(1010, ResourceSlot::Labor), own(&p));
    assert_eq!(
        again.activity_own_units(1010, ResourceSlot::Nonlabor),
        [0., 3., 0., 3.]
    );
    // Completed activities have no remaining units.
    let a = &mut p.model.activities[i];
    a.status = StatusType::Completed;
    a.act_finish = Some(dd);
    assert!(p
        .edit_activity_units(i, ResourceSlot::Labor, UnitsPhase::Remaining, 5.)
        .is_err());
    p.edit_activity_units(i, ResourceSlot::Labor, UnitsPhase::Remaining, 0.)
        .unwrap();
    assert_eq!(own(&p)[2], 0.);
    // Reset clears the period figures of the project.
    assert!(p.reset_actual_this_period() > 0);
    assert_eq!(own(&p)[3], 0.);
    assert_eq!(own(&p)[1], 14.);
    assert_eq!(p.reset_actual_this_period(), 0);
}

#[test]
fn at_completion_units_set_the_remaining_and_the_budget_follows_only_before_the_start() {
    use openxer_xer::edit::activity::{edit_activity, Edit, Measure, Phase};
    // Not started, budget and remaining linked, Fixed Duration and Units:
    // both take the new total, the units/time follows and the duration stays.
    let mut p = with_crew();
    set_duration_type(&mut p, 1010, "DT_FixedDUR2");
    let i = index(&p, 1010);
    let row = p.add_assignment(1010, Some(CREW), None).unwrap();
    let rem = p.model.activities[i].rem;
    assert_eq!(value(&p, row, "remain_qty"), "24");
    p.edit_assignment(row, AssignmentEdit::AtCompletionUnits(36.))
        .unwrap();
    assert_eq!(value(&p, row, "target_qty"), "36");
    assert_eq!(value(&p, row, "remain_qty"), "36");
    assert_eq!(value(&p, row, "remain_qty_per_hr"), "1.5");
    assert_eq!(value(&p, row, "target_cost"), "3060.0000");
    assert_eq!(p.model.activities[i].rem, rem, "the duration stays");
    // The Status tab figure goes to the same assignment.
    let figure = |value: f64| Edit::Figure {
        measure: Measure::LaborUnits,
        phase: Phase::AtCompletion,
        value,
    };
    edit_activity(&mut p, i, figure(48.)).unwrap();
    assert_eq!(value(&p, row, "remain_qty"), "48");
    assert_eq!(task_cell(&p, 1010, "target_work_qty"), "48");
    // At Completion cost is typed too: the remaining cost takes it (no
    // actual yet), and with cost and units linked the units follow at the
    // price, so the budget follows before the start.
    let price: f64 = value(&p, row, "cost_per_qty").parse().unwrap();
    let cost = Edit::Figure {
        measure: Measure::LaborCost,
        phase: Phase::AtCompletion,
        value: price * 60.,
    };
    edit_activity(&mut p, i, cost).unwrap();
    assert_eq!(value(&p, row, "remain_qty"), "60");
    assert_eq!(value(&p, row, "target_qty"), "60");

    // In progress: the remaining takes At Completion less Actual and the
    // budget stays.
    let (mut p, i, row) = in_progress_case();
    p.edit_assignment(row, AssignmentEdit::ActualRegularUnits(8.))
        .unwrap();
    let budget = value(&p, row, "target_qty");
    p.edit_assignment(row, AssignmentEdit::AtCompletionUnits(40.))
        .unwrap();
    assert_eq!(value(&p, row, "remain_qty"), "32");
    assert_eq!(value(&p, row, "act_reg_qty"), "8");
    assert_eq!(value(&p, row, "target_qty"), budget);
    assert_eq!(value(&p, row, "remain_cost"), "2720.0000");
    let before = p.document.table("TASKRSRC").unwrap().rows.clone();
    assert!(
        p.edit_assignment(row, AssignmentEdit::AtCompletionUnits(4.))
            .is_err(),
        "not below the actual units"
    );
    assert!(edit_activity(&mut p, i, figure(4.)).is_err());
    assert_eq!(p.document.table("TASKRSRC").unwrap().rows, before);
    let dd = p.model.project.data_date.unwrap();
    {
        let a = &mut p.model.activities[i];
        a.status = StatusType::Completed;
        a.act_finish = Some(dd);
    }
    assert!(p
        .edit_assignment(row, AssignmentEdit::AtCompletionUnits(50.))
        .is_err());
    assert!(edit_activity(&mut p, i, figure(50.)).is_err());
    assert!(p
        .edit_assignment(row, AssignmentEdit::AtCompletionCost(5000.))
        .is_err());

    // Without an assignment the activity's own units take it.
    use openxer_xer::assignments::ResourceSlot;
    let mut p = starter();
    let i = index(&p, 1010);
    edit_activity(&mut p, i, figure(30.)).unwrap();
    assert_eq!(
        p.activity_own_units(1010, ResourceSlot::Labor),
        [30., 0., 30., 0.]
    );
}

#[test]
fn actual_cost_and_cost_units_linked_on_the_assignment() {
    let (mut p, _, row) = in_progress_case();
    p.edit_assignment(row, AssignmentEdit::OvertimeFactor(1.5))
        .unwrap();
    p.edit_assignment(row, AssignmentEdit::ActualOvertimeUnits(2.))
        .unwrap();
    assert_eq!(value(&p, row, "act_ot_cost"), "255.0000");
    // Actual cost is regular plus overtime: the regular cost takes the rest.
    p.edit_assignment(row, AssignmentEdit::ActualCost(1000.))
        .unwrap();
    assert_eq!(value(&p, row, "act_reg_cost"), "745.0000");
    assert_eq!(p.assignment_view(row).unwrap().actual_cost, 1000.);
    assert!(p
        .edit_assignment(row, AssignmentEdit::ActualCost(100.))
        .is_err());
    // Cost Units Linked off: remaining cost is kept on its own; back on,
    // it follows the units at the price again.
    assert!(p.assignment_view(row).unwrap().cost_linked);
    p.edit_assignment(row, AssignmentEdit::CostUnitsLinked(false))
        .unwrap();
    assert_eq!(value(&p, row, "cost_qty_link_flag"), "N");
    assert!(!p.assignment_view(row).unwrap().cost_linked);
    p.edit_assignment(row, AssignmentEdit::RemainingCost(900.))
        .unwrap();
    assert_eq!(value(&p, row, "remain_cost"), "900.0000");
    p.edit_assignment(row, AssignmentEdit::CostUnitsLinked(true))
        .unwrap();
    assert_eq!(value(&p, row, "cost_qty_link_flag"), "Y");
    let units: f64 = value(&p, row, "remain_qty").parse().unwrap();
    let expected = format!("{:.4}", units * 85.);
    assert_eq!(value(&p, row, "remain_cost"), expected);
}
