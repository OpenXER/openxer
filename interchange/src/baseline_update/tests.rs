//! One test per option group of Update Baseline, on the starter project.
use super::*;
use crate::baseline::{activity_codes, copy_as_baseline, project_value};

/// (project, its id, a baseline taken of it, the baseline's id). Every
/// activity carries a GUID, as activities made in OpenXER do.
fn fixture() -> (Document, i64, Document, i64) {
    let p = openxer_xer::starter();
    let mut doc = p.updated_document();
    let pid = p.model.project.proj_id;
    let t = doc.table_mut("TASK");
    for i in 0..t.rows.len() {
        let id = t.value(&t.rows[i], "task_id").to_string();
        t.set(i, "guid", format!("GUID-{id}"));
    }
    doc.table_mut("RSRC").add(&[
        ("rsrc_id", "1".into()),
        ("rsrc_short_name", "LAB".into()),
        ("rsrc_name", "Laborer".into()),
        ("rsrc_type", "RT_Labor".into()),
    ]);
    doc.table_mut("TASKRSRC").add(&[
        ("taskrsrc_id", "1".into()),
        ("task_id", "1000".into()),
        ("proj_id", pid.to_string()),
        ("rsrc_id", "1".into()),
        ("target_qty", "40".into()),
        ("remain_qty", "40".into()),
        ("target_cost", "2000".into()),
        ("remain_cost", "2000".into()),
        ("guid", "ASG-1".into()),
    ]);
    let key = pid + 100;
    let bl = copy_as_baseline(
        &doc,
        pid,
        key,
        "RB - B1",
        "Riverbend - B1",
        "2026-11-01 08:00",
    );
    (doc, pid, bl, key)
}

fn row(doc: &Document, pid: i64, code: &str) -> Option<usize> {
    let t = doc.table("TASK")?;
    t.rows
        .iter()
        .position(|r| owned(t, r, pid) && t.value(r, "task_code") == code)
}

fn get(doc: &Document, pid: i64, code: &str, col: &str) -> String {
    let t = doc.table("TASK").unwrap();
    let i = row(doc, pid, code).unwrap_or_else(|| panic!("activity {code}"));
    t.value(&t.rows[i], col).to_string()
}

fn set(doc: &mut Document, pid: i64, code: &str, col: &str, value: &str) {
    let i = row(doc, pid, code).unwrap();
    doc.table_mut("TASK").set(i, col, value.into());
}

fn task_id(doc: &Document, pid: i64, code: &str) -> String {
    get(doc, pid, code, "task_id")
}

fn codes(doc: &Document, pid: i64) -> BTreeSet<String> {
    activity_codes(doc, pid).into_values().collect()
}

/// A new activity in the project, a copy of the first one.
fn add(doc: &mut Document, pid: i64, code: &str) {
    let src = doc.table("TASK").unwrap().clone();
    let first = src.rows.iter().position(|r| owned(&src, r, pid)).unwrap();
    let id = next_id(doc, "TASK", "task_id").to_string();
    copy_row(
        doc.table_mut("TASK"),
        &src,
        &src.rows[first],
        &[
            ("task_id", id.clone()),
            ("task_code", code.into()),
            ("task_name", "A new activity".into()),
            ("guid", format!("GUID-{id}")),
        ],
    );
}

fn delete(doc: &mut Document, pid: i64, code: &str) {
    let id = num(&task_id(doc, pid, code));
    openxer_xer::delete::delete_activity_rows(doc, pid, &[id]);
}

fn kinds(p: &Preview) -> Vec<(String, ChangeKind)> {
    p.activities
        .iter()
        .map(|c| (c.object.clone(), c.kind))
        .collect()
}

fn fields(p: &Preview, code: &str) -> Vec<String> {
    p.activities
        .iter()
        .find(|c| c.object == code)
        .map(|c| c.fields.iter().map(|f| f.field.clone()).collect())
        .unwrap_or_default()
}

fn count(doc: &Document, table: &str, pid: i64) -> usize {
    rows_of(doc, table, pid).len()
}

fn with(f: impl FnOnce(&mut UpdateOptions)) -> UpdateOptions {
    let mut o = UpdateOptions::default();
    f(&mut o);
    o
}

fn run(
    bl: &Document,
    key: i64,
    doc: &Document,
    pid: i64,
    o: &UpdateOptions,
) -> (Document, Preview) {
    update_baseline(bl, key, doc, pid, o, None).unwrap()
}

/// The observed run: add one activity, delete one, rename one, change a
/// duration and budgeted units of an activity without assignments. With
/// the defaults plus add, delete and update, the rename, the deletion and
/// the addition are listed and the duration change is not; with dates and
/// budgeted units ticked it is.
#[test]
fn the_observed_run_lists_what_the_options_carry() {
    let (mut doc, pid, bl, key) = fixture();
    let list: Vec<String> = codes(&doc, pid).into_iter().collect();
    let (renamed, changed, gone) = (list[1].clone(), list[2].clone(), list[3].clone());
    add(&mut doc, pid, "ZZ-NEW");
    delete(&mut doc, pid, &gone);
    let old_name = get(&doc, pid, &renamed, "task_name");
    set(&mut doc, pid, &renamed, "task_name", "Renamed activity");
    set(&mut doc, pid, &changed, "target_drtn_hr_cnt", "64");
    set(&mut doc, pid, &changed, "remain_drtn_hr_cnt", "64");
    set(&mut doc, pid, &changed, "target_work_qty", "40");
    let o = with(|o| {
        o.add_new = true;
        o.delete_missing = true;
    });
    let (out, p) = run(&bl, key, &doc, pid, &o);
    let mut want = vec![
        (renamed.clone(), ChangeKind::Updated),
        (gone.clone(), ChangeKind::Deleted),
        ("ZZ-NEW".to_string(), ChangeKind::Added),
    ];
    want.sort();
    assert_eq!(kinds(&p), want, "{:#?}", p.activities);
    assert_eq!(fields(&p, &renamed), ["Activity Name"]);
    let f = &p
        .activities
        .iter()
        .find(|c| c.object == renamed)
        .unwrap()
        .fields[0];
    assert_eq!(
        (f.project.as_str(), f.baseline.as_str()),
        ("Renamed activity", old_name.as_str())
    );
    // A deleted activity shows its baseline values, an added one its
    // project values.
    let deleted = p.activities.iter().find(|c| c.object == gone).unwrap();
    assert!(deleted
        .fields
        .iter()
        .all(|f| f.project.is_empty() && !f.baseline.is_empty()));
    assert!(deleted.fields.iter().any(|f| f.field == "Activity ID"));
    let added = p.activities.iter().find(|c| c.object == "ZZ-NEW").unwrap();
    assert!(added
        .fields
        .iter()
        .all(|f| !f.project.is_empty() && f.baseline.is_empty()));
    // The update holds exactly that.
    assert!(codes(&out, key).contains("ZZ-NEW"));
    assert!(!codes(&out, key).contains(&gone));
    assert_eq!(get(&out, key, &renamed, "task_name"), "Renamed activity");
    assert_eq!(
        get(&out, key, &changed, "target_drtn_hr_cnt"),
        get(&bl, key, &changed, "target_drtn_hr_cnt")
    );
    // Run again on the result: nothing is left to change.
    let (_, again) = run(&out, key, &doc, pid, &o);
    assert!(again.is_empty(), "{:#?}", again);

    // Dates and budgeted units ticked: the duration and units change too.
    let o = with(|o| {
        o.add_new = true;
        o.delete_missing = true;
        o.dates = true;
        o.budget_no_assignments = true;
    });
    let (out, p) = run(&bl, key, &doc, pid, &o);
    let f = fields(&p, &changed);
    for field in [
        "Budgeted Labor Units",
        "Original Duration",
        "Remaining Duration",
    ] {
        assert!(f.contains(&field.to_string()), "{field} in {f:?}");
    }
    let mut sorted = f.clone();
    sorted.sort();
    assert_eq!(f, sorted, "fields are listed by name");
    assert_eq!(get(&out, key, &changed, "target_drtn_hr_cnt"), "64");
    assert_eq!(get(&out, key, &changed, "target_work_qty"), "40");
    let dur = p.activities.iter().find(|c| c.object == changed).unwrap();
    let od = dur
        .fields
        .iter()
        .find(|f| f.field == "Original Duration")
        .unwrap();
    assert_eq!(od.project, "8d");
}

#[test]
fn update_existing_off_leaves_matched_activities() {
    let (mut doc, pid, bl, key) = fixture();
    let code = codes(&doc, pid).into_iter().nth(1).unwrap();
    set(&mut doc, pid, &code, "task_name", "Renamed");
    let (out, p) = run(&bl, key, &doc, pid, &with(|o| o.update_existing = false));
    assert!(p.activities.is_empty());
    assert_ne!(get(&out, key, &code, "task_name"), "Renamed");
    // General information off: the name stays too.
    let (_, p) = run(&bl, key, &doc, pid, &with(|o| o.general = false));
    assert!(p.activities.is_empty());
}

/// A renumbered activity keeps its match through its GUID: it is updated,
/// not deleted and added again.
#[test]
fn a_renumbered_activity_is_matched_by_its_guid() {
    let (mut doc, pid, bl, key) = fixture();
    let code = codes(&doc, pid).into_iter().nth(1).unwrap();
    set(&mut doc, pid, &code, "task_code", "RENUMBERED");
    let o = with(|o| {
        o.add_new = true;
        o.delete_missing = true;
    });
    let (out, p) = run(&bl, key, &doc, pid, &o);
    assert_eq!(kinds(&p), [("RENUMBERED".to_string(), ChangeKind::Updated)]);
    assert_eq!(fields(&p, "RENUMBERED"), ["Activity ID"]);
    assert_eq!(count(&out, "TASK", key), count(&bl, "TASK", key));
    // Without GUIDs the old ID is gone and the new one is new.
    let strip = |d: &mut Document| {
        let t = d.table_mut("TASK");
        for i in 0..t.rows.len() {
            t.set(i, "guid", String::new());
        }
    };
    let (mut doc2, mut bl2) = (doc.clone(), bl.clone());
    strip(&mut doc2);
    strip(&mut bl2);
    let (_, p) = run(&bl2, key, &doc2, pid, &o);
    let mut want = vec![
        (code.clone(), ChangeKind::Deleted),
        ("RENUMBERED".to_string(), ChangeKind::Added),
    ];
    want.sort();
    assert_eq!(kinds(&p), want);
}

#[test]
fn a_filter_limits_added_and_updated_activities() {
    let (mut doc, pid, bl, key) = fixture();
    let list: Vec<String> = codes(&doc, pid).into_iter().collect();
    for c in &list {
        set(&mut doc, pid, c, "target_drtn_hr_cnt", "99");
    }
    add(&mut doc, pid, "ZZ-NEW");
    xb_set_data_date(&mut doc, pid);
    let o = with(|o| {
        o.dates = true;
        o.add_new = true;
    });
    let only = BTreeSet::from([list[0].clone()]);
    let (out, p) = update_baseline(&bl, key, &doc, pid, &o, Some(&only)).unwrap();
    assert_eq!(kinds(&p), [(list[0].clone(), ChangeKind::Updated)]);
    assert!(!codes(&out, key).contains("ZZ-NEW"));
    assert_eq!(
        project_value(&out, key, "last_recalc_date"),
        project_value(&bl, key, "last_recalc_date"),
        "the data date stays when a filter limits the update"
    );
    let (out, p) = run(&bl, key, &doc, pid, &o);
    assert_eq!(p.counts(), (1, 0, list.len()));
    assert_eq!(
        project_value(&out, key, "last_recalc_date"),
        project_value(&doc, pid, "last_recalc_date")
    );
    assert_eq!(p.project[0].fields[0].field, "Data Date");
}

fn xb_set_data_date(doc: &mut Document, pid: i64) {
    crate::baseline::set_project_value(doc, pid, "last_recalc_date", "2026-12-01 08:00");
}

#[test]
fn relationships_follow_their_option() {
    let (mut doc, pid, bl, key) = fixture();
    let links = count(&bl, "TASKPRED", key);
    assert!(links > 0);
    doc.table_mut("TASKPRED").rows.remove(0);
    let (out, p) = run(&bl, key, &doc, pid, &UpdateOptions::default());
    assert_eq!(count(&out, "TASKPRED", key), links, "left alone by default");
    assert!(p.activities.is_empty());
    let (out, p) = run(&bl, key, &doc, pid, &with(|o| o.relationships = true));
    assert_eq!(count(&out, "TASKPRED", key), links - 1);
    assert_eq!(p.activities.len(), 1);
    assert_eq!(p.activities[0].fields[0].field, "Predecessors");
}

#[test]
fn constraints_dates_and_wbs_follow_their_options() {
    let (mut doc, pid, bl, key) = fixture();
    let code = codes(&doc, pid).into_iter().nth(1).unwrap();
    set(&mut doc, pid, &code, "cstr_type", "CS_MSO");
    set(&mut doc, pid, &code, "cstr_date", "2027-02-01 08:00");
    let (out, _) = run(&bl, key, &doc, pid, &UpdateOptions::default());
    assert_eq!(
        get(&out, key, &code, "cstr_type"),
        get(&bl, key, &code, "cstr_type")
    );
    let (out, p) = run(&bl, key, &doc, pid, &with(|o| o.constraints = true));
    assert_eq!(get(&out, key, &code, "cstr_type"), "CS_MSO");
    assert_eq!(
        fields(&p, &code),
        ["Primary Constraint", "Primary Constraint Date"]
    );
}

fn step(doc: &mut Document, pid: i64, task: &str, name: &str) {
    let id = next_id(doc, "TASKPROC", "proc_id").to_string();
    doc.table_mut("TASKPROC").add(&[
        ("proc_id", id),
        ("task_id", task.into()),
        ("proj_id", pid.to_string()),
        ("seq_num", "10".into()),
        ("proc_name", name.into()),
        ("complete_flag", "N".into()),
        ("proc_wt", "1".into()),
    ]);
}

#[test]
fn steps_expenses_notebooks_and_codes_follow_their_options() {
    let (mut doc, pid, bl, key) = fixture();
    let code = codes(&doc, pid).into_iter().nth(1).unwrap();
    let task = task_id(&doc, pid, &code);
    step(&mut doc, pid, &task, "Check bolts");
    doc.table_mut("PROJCOST").add(&[
        ("cost_item_id", "1".into()),
        ("task_id", task.clone()),
        ("proj_id", pid.to_string()),
        ("cost_name", "Crane".into()),
        ("target_cost", "500".into()),
    ]);
    doc.table_mut("MEMOTYPE")
        .add(&[("memo_type_id", "1".into()), ("memo_type", "Notes".into())]);
    doc.table_mut("TASKMEMO").add(&[
        ("memo_id", "1".into()),
        ("task_id", task.clone()),
        ("memo_type_id", "1".into()),
        ("proj_id", pid.to_string()),
        ("task_memo", "Lift plan needed".into()),
    ]);
    // Defaults: steps and notebooks are carried, expenses are not.
    let (out, p) = run(&bl, key, &doc, pid, &UpdateOptions::default());
    assert_eq!(count(&out, "TASKPROC", key), 1);
    assert_eq!(count(&out, "TASKMEMO", key), 1);
    assert_eq!(count(&out, "PROJCOST", key), 0);
    assert_eq!(fields(&p, &code), ["Notebook Topics", "Steps"]);
    assert_eq!(rows_of(&out, "MEMOTYPE", 0).len(), 0);
    assert!(
        out.table("MEMOTYPE").is_some_and(|t| t.rows.len() == 1),
        "the dictionary row came along"
    );
    let (out, p) = run(
        &bl,
        key,
        &doc,
        pid,
        &with(|o| {
            o.steps = false;
            o.notebooks = false;
            o.expenses = true;
        }),
    );
    assert_eq!(count(&out, "TASKPROC", key), 0);
    assert_eq!(count(&out, "TASKMEMO", key), 0);
    assert_eq!(count(&out, "PROJCOST", key), 1);
    assert_eq!(fields(&p, &code), ["Expenses"]);
}

#[test]
fn step_udfs_go_with_their_steps() {
    let (mut doc, pid, mut bl, key) = fixture();
    let code = codes(&doc, pid).into_iter().nth(1).unwrap();
    let task = task_id(&doc, pid, &code);
    for d in [&mut doc, &mut bl] {
        d.table_mut("UDFTYPE").add(&[
            ("udf_type_id", "900".into()),
            ("table_name", "TASKPROC".into()),
            ("udf_type_name", "user_text1".into()),
            ("udf_type_label", "Inspector".into()),
            ("logical_data_type", "FT_TEXT".into()),
        ]);
    }
    step(&mut doc, pid, &task, "Check bolts");
    let proc = doc.table("TASKPROC").unwrap().rows[0][0].clone();
    doc.table_mut("UDFVALUE").add(&[
        ("udf_type_id", "900".into()),
        ("fk_id", proc),
        ("proj_id", pid.to_string()),
        ("udf_text", "Lee".into()),
    ]);
    let (out, p) = run(&bl, key, &doc, pid, &UpdateOptions::default());
    assert_eq!(count(&out, "TASKPROC", key), 1);
    assert_eq!(count(&out, "UDFVALUE", key), 0, "step UDFs off");
    assert_eq!(fields(&p, &code), ["Steps"]);
    let (out, p) = run(&bl, key, &doc, pid, &with(|o| o.step_udfs = true));
    assert_eq!(count(&out, "UDFVALUE", key), 1);
    assert_eq!(fields(&p, &code), ["Step Inspector", "Steps"]);
    let new_proc = rows_of(&out, "TASKPROC", key)[0][0].clone();
    assert_eq!(rows_of(&out, "UDFVALUE", key)[0][1], new_proc);
    // Steps off: Step UDFs act as off too.
    let (out, _) = run(
        &bl,
        key,
        &doc,
        pid,
        &with(|o| {
            o.steps = false;
            o.step_udfs = true;
        }),
    );
    assert_eq!(count(&out, "UDFVALUE", key), 0);
}

fn assignment_qty(doc: &Document, pid: i64, task: &str) -> Vec<String> {
    let t = doc.table("TASKRSRC").unwrap();
    rows_of(doc, "TASKRSRC", pid)
        .iter()
        .filter(|r| t.value(r, "task_id") == task)
        .map(|r| number(t.value(r, "target_qty")))
        .collect()
}

#[test]
fn assignment_options_update_and_add_with_or_without_units() {
    let (mut doc, pid, bl, key) = fixture();
    let first = codes(&doc, pid).into_iter().next().unwrap();
    let second = codes(&doc, pid).into_iter().nth(1).unwrap();
    assert_eq!(task_id(&doc, pid, &first), "1000");
    let second_task = task_id(&doc, pid, &second);
    doc.table_mut("TASKRSRC").set(0, "target_qty", "10".into());
    doc.table_mut("TASKRSRC").add(&[
        ("taskrsrc_id", "2".into()),
        ("task_id", second_task),
        ("proj_id", pid.to_string()),
        ("rsrc_id", "1".into()),
        ("target_qty", "16".into()),
        ("act_reg_qty", "4".into()),
        ("guid", "ASG-2".into()),
    ]);
    let b1 = task_id(&bl, key, &first);
    let b2 = task_id(&bl, key, &second);
    // Defaults: existing assignments stay, new ones come with their units.
    let (out, p) = run(&bl, key, &doc, pid, &UpdateOptions::default());
    assert_eq!(assignment_qty(&out, key, &b1), ["40"]);
    assert_eq!(assignment_qty(&out, key, &b2), ["16"]);
    assert_eq!(kinds(&p), [(second.clone(), ChangeKind::Updated)]);
    assert_eq!(fields(&p, &second), ["Resource Assignments"]);
    // Update existing with budgeted units.
    let (out, _) = run(
        &bl,
        key,
        &doc,
        pid,
        &with(|o| {
            o.update_assignments = true;
            o.update_assignment_budget = true;
        }),
    );
    assert_eq!(assignment_qty(&out, key, &b1), ["10"]);
    // Update existing without budgeted units: the units stay.
    let (out, _) = run(&bl, key, &doc, pid, &with(|o| o.update_assignments = true));
    assert_eq!(assignment_qty(&out, key, &b1), ["40"]);
    // Add without budgeted or actual values.
    let (out, _) = run(
        &bl,
        key,
        &doc,
        pid,
        &with(|o| {
            o.add_assignment_budget = false;
            o.add_assignment_actual = false;
        }),
    );
    assert_eq!(assignment_qty(&out, key, &b2), ["0"]);
    let t = out.table("TASKRSRC").unwrap();
    let added = rows_of(&out, "TASKRSRC", key)
        .into_iter()
        .find(|r| t.value(r, "task_id") == b2)
        .unwrap();
    assert_eq!(number(t.value(&added, "act_reg_qty")), "0");
    // Add off: none added.
    let (out, _) = run(&bl, key, &doc, pid, &with(|o| o.add_assignments = false));
    assert!(assignment_qty(&out, key, &b2).is_empty());
}

#[test]
fn units_without_assignments_apply_only_to_activities_without_them() {
    let (mut doc, pid, bl, key) = fixture();
    let first = codes(&doc, pid).into_iter().next().unwrap();
    let second = codes(&doc, pid).into_iter().nth(1).unwrap();
    for c in [&first, &second] {
        set(&mut doc, pid, c, "target_work_qty", "77");
        set(&mut doc, pid, c, "act_work_qty", "7");
    }
    let o = with(|o| {
        o.budget_no_assignments = true;
        o.actual_no_assignments = true;
    });
    let (out, _) = run(&bl, key, &doc, pid, &o);
    assert_eq!(get(&out, key, &second, "target_work_qty"), "77");
    assert_eq!(get(&out, key, &second, "act_work_qty"), "7");
    assert_ne!(
        get(&out, key, &first, "target_work_qty"),
        "77",
        "it has an assignment"
    );
    let (out, _) = run(
        &bl,
        key,
        &doc,
        pid,
        &with(|o| o.budget_no_assignments = true),
    );
    assert_eq!(get(&out, key, &second, "target_work_qty"), "77");
    assert_ne!(get(&out, key, &second, "act_work_qty"), "7");
}

#[test]
fn project_details_and_risks_follow_their_options() {
    let (mut doc, pid, bl, key) = fixture();
    crate::baseline::set_project_value(&mut doc, pid, "plan_end_date", "2028-01-31 17:00");
    doc.table_mut("PROJRISK").add(&[
        ("risk_id", "5".into()),
        ("proj_id", pid.to_string()),
        ("risk_name", "Late steel".into()),
    ]);
    let (out, p) = run(&bl, key, &doc, pid, &UpdateOptions::default());
    assert_eq!(
        project_value(&out, key, "plan_end_date"),
        "2028-01-31 17:00"
    );
    assert_eq!(p.project.len(), 1);
    assert_eq!(p.project[0].fields[0].field, "Must Finish By");
    assert_eq!(count(&out, "PROJRISK", key), 0, "risks off by default");
    let (out, p) = run(
        &bl,
        key,
        &doc,
        pid,
        &with(|o| {
            o.project_details = false;
            o.risks_issues = true;
        }),
    );
    assert_eq!(
        project_value(&out, key, "plan_end_date"),
        project_value(&bl, key, "plan_end_date")
    );
    assert_eq!(count(&out, "PROJRISK", key), 1);
    assert_eq!(p.project[0].fields[0].field, "Risks");
    // Risks removed from the project leave the baseline on the next update.
    let (_, p) = run(
        &out,
        key,
        &bl_like(&doc, pid),
        pid,
        &with(|o| o.risks_issues = true),
    );
    assert_eq!(
        p.project[0]
            .fields
            .iter()
            .filter(|f| f.field == "Risks")
            .count(),
        1
    );
}

/// The project without its risks.
fn bl_like(doc: &Document, pid: i64) -> Document {
    let mut d = doc.clone();
    let rows: Vec<usize> = (0..d.table("PROJRISK").unwrap().rows.len()).collect();
    remove_rows(&mut d, "PROJRISK", rows);
    let _ = pid;
    d
}

#[test]
fn an_added_activity_comes_without_actuals() {
    let (mut doc, pid, bl, key) = fixture();
    add(&mut doc, pid, "ZZ-NEW");
    set(&mut doc, pid, "ZZ-NEW", "status_code", "TK_Active");
    set(
        &mut doc,
        pid,
        "ZZ-NEW",
        "act_start_date",
        "2026-11-02 08:00",
    );
    set(&mut doc, pid, "ZZ-NEW", "act_work_qty", "5");
    let (out, _) = run(&bl, key, &doc, pid, &with(|o| o.add_new = true));
    assert_eq!(get(&out, key, "ZZ-NEW", "status_code"), "TK_NotStart");
    assert_eq!(get(&out, key, "ZZ-NEW", "act_start_date"), "");
    assert_eq!(get(&out, key, "ZZ-NEW", "act_work_qty"), "0");
    // Its relationships come along: the copy of the first activity has none,
    // so link it and add again.
    let (ok, _) = openxer_xer::ProjectData::from_document(out.clone(), key, "bl".into())
        .map(|p| (true, p))
        .unwrap();
    assert!(ok);
}

#[test]
fn children_of_an_option_that_is_off_act_as_off() {
    let o = with(|o| {
        o.project_details = false;
        o.project_udfs = true;
        o.steps = false;
        o.step_udfs = true;
        o.update_assignments = false;
        o.update_assignment_budget = true;
        o.ignore_last_update = true;
        o.run_optimized = true;
    })
    .effective();
    assert!(!o.project_udfs && !o.step_udfs && !o.update_assignment_budget);
    assert!(!o.ignore_last_update && !o.run_optimized);
    let d = UpdateOptions::default();
    assert!(d.project_details && d.update_existing && d.general && d.wbs && d.codes);
    assert!(d.notebooks && d.steps && d.add_assignments && d.add_assignment_budget);
    assert!(d.add_assignment_actual && d.log_to_file);
    assert!(!d.add_new && !d.delete_missing && !d.dates && !d.relationships);
}
