//! Workflow 4c: Update Baseline through Project > Maintain Baselines >
//! Update..., reproducing the observed run of the scheduler users know: a
//! baseline taken, then one activity added, one deleted, one renamed and
//! one given a longer duration and budgeted units; Preview with the default
//! options plus add, delete and update lists the rename, the deletion and
//! the addition but not the duration change; with dates and budgeted units
//! ticked it lists that too; Update applies it with no question, sets Last
//! Update Date and offers the log; Close asks whether to keep the options.
use super::*;
use crate::baselines::store as bl_store;
use openxer_interchange::baseline as xb;
use openxer_interchange::baseline_update::ChangeKind;

/// The changes Preview Changes lists, as (Activity ID, change).
fn listed(d: &Driver) -> Vec<(String, ChangeKind)> {
    let v = d
        .app
        .baselines
        .preview
        .as_ref()
        .expect("Preview Changes is open");
    v.preview
        .activities
        .iter()
        .map(|c| (c.object.clone(), c.kind))
        .collect()
}

fn fields_of(d: &Driver, code: &str) -> Vec<String> {
    let v = d
        .app
        .baselines
        .preview
        .as_ref()
        .expect("Preview Changes is open");
    v.preview
        .activities
        .iter()
        .find(|c| c.object == code)
        .map(|c| c.fields.iter().map(|f| f.field.clone()).collect())
        .unwrap_or_default()
}

/// The tree line of activity `code` in Preview Changes: Project and WBS
/// groups with their changes come first.
fn tree_line(d: &Driver, code: &str) -> usize {
    let p = &d.app.baselines.preview.as_ref().unwrap().preview;
    let before = 1 + p.project.len() + 1 + p.wbs.len() + 1;
    before
        + p.activities
            .iter()
            .position(|c| c.object == code)
            .unwrap_or_else(|| panic!("{code} is listed"))
}

fn open_preview(d: &mut Driver) {
    d.click("update_preview");
    d.frames(2);
    d.assert_clean("preview");
    assert!(d.app.baselines.preview.is_some(), "Preview Changes opens");
}

fn close_preview(d: &mut Driver) {
    d.click("preview_close");
    d.frames(1);
    assert!(d.app.baselines.preview.is_none());
}

fn baseline_doc(d: &Driver, id: i64) -> (Document, i64, bl_store::BaselineRow) {
    let row = bl_store::get(d.app.store.connection(), id)
        .unwrap()
        .unwrap();
    (Document::parse(&row.xer).unwrap(), row.project_key, row)
}

fn bl_task(doc: &Document, key: i64, code: &str, col: &str) -> String {
    let t = doc.table("TASK").unwrap();
    t.rows
        .iter()
        .find(|r| t.value(r, "proj_id") == key.to_string() && t.value(r, "task_code") == code)
        .map(|r| t.value(r, col).to_string())
        .unwrap_or_else(|| panic!("{code} in the baseline"))
}

#[test]
fn workflow_4c_update_baseline_as_observed() {
    let mut d = fresh("wf4c");
    new_project(&mut d, "BU-100", "Update Trial", "04-Jan-27", "");
    show(&mut d, "activities");
    let a = add_activity(&mut d, "Demo old pump base", "5d");
    let b = add_activity(&mut d, "Set new pump", "5d");
    let c = add_activity(&mut d, "Grout pump base", "5d");
    let e = add_activity(&mut d, "Remove old piping", "2d");
    link(&mut d, a, b);
    link(&mut d, b, c);
    d.f9();
    let code = |d: &Driver, i: usize| d.app.project.model.activities[i].task_code.clone();
    let (renamed, longer, gone) = (code(&d, b), code(&d, c), code(&d, e));
    let old_name = "Set new pump".to_string();

    // The baseline, made the project baseline: an assigned one can be updated.
    open_maintain(&mut d);
    let bl = add_baseline_copy(&mut d);
    d.click("baselines_close");
    assign(&mut d, &[(0, Some(0))]);
    let (_, _, taken) = baseline_doc(&d, bl);

    // The project changes: one added, one deleted, one renamed, one longer
    // with budgeted units.
    show(&mut d, "activities");
    let added = add_activity(&mut d, "Set new pump skid", "3d");
    let added_code = code(&d, added);
    select_row(&mut d, e);
    d.click("menu_bar_edit");
    d.click("edit_delete");
    if d.has("delete_yes") {
        d.click("delete_yes");
    }
    d.frames(2);
    assert!(
        d.app
            .project
            .model
            .activities
            .iter()
            .all(|x| x.task_code != gone),
        "{gone} is deleted"
    );
    let index = |d: &Driver, code: &str| {
        d.app
            .project
            .model
            .activities
            .iter()
            .position(|x| x.task_code == code)
            .unwrap()
    };
    d.app.grid.layout.columns = [
        "activity_id",
        "activity_name",
        "original_duration",
        "remaining_duration",
        "budgeted_labor_units",
    ]
    .iter()
    .map(|c| grid::layout::ColumnSetting::new(c, 110.))
    .collect();
    d.app.drop_frame_cache();
    d.frames(3);
    let i = index(&d, &renamed);
    cell_edit(&mut d, i, "activity_name", "Set new pump renamed");
    let i = index(&d, &longer);
    cell_edit(&mut d, i, "original_duration", "8d");
    cell_edit(&mut d, i, "budgeted_labor_units", "40");
    d.f9();

    // Maintain Baselines > Update...: the defaults.
    open_maintain(&mut d);
    d.click(&baseline_line(&d, bl));
    d.click("baselines_update");
    d.frames(2);
    let opened = d
        .app
        .baselines
        .update
        .as_ref()
        .expect("Update Baseline opens");
    assert_eq!(opened.opts, crate::App::default_update_options());
    // Activities: add and delete ticked too (update is on already).
    d.click("update_category_row_1");
    d.frames(1);
    d.click("update_opt_add_new");
    d.click("update_opt_delete_missing");
    d.frames(1);
    let o = &d.app.baselines.update.as_ref().unwrap().opts;
    assert!(o.add_new && o.delete_missing && o.update_existing);
    open_preview(&mut d);
    let mut want = vec![
        (renamed.clone(), ChangeKind::Updated),
        (gone.clone(), ChangeKind::Deleted),
        (added_code.clone(), ChangeKind::Added),
    ];
    want.sort();
    assert_eq!(listed(&d), want, "the duration change is not listed");
    assert_eq!(fields_of(&d, &renamed), ["Activity Name"]);
    // Selecting a line fills the field grid.
    let line = tree_line(&d, &renamed);
    d.click(&format!("preview_tree_row_{line}"));
    d.frames(1);
    assert!(d.has("preview_fields_row_0"), "the field grid has a row");
    let p = d.app.baselines.preview.as_ref().unwrap();
    let f = &p
        .preview
        .activities
        .iter()
        .find(|c| c.object == renamed)
        .unwrap()
        .fields[0];
    assert_eq!(
        (f.project.as_str(), f.baseline.as_str()),
        ("Set new pump renamed", old_name.as_str())
    );
    close_preview(&mut d);

    // Activity: Dates, durations, data date and Budgeted units, cost.
    d.click("update_category_row_3");
    d.frames(1);
    d.click("update_opt_dates");
    d.click("update_opt_budget_no_assignments");
    open_preview(&mut d);
    let kinds: Vec<(String, ChangeKind)> = listed(&d);
    assert!(
        kinds.contains(&(longer.clone(), ChangeKind::Updated)),
        "{kinds:?}"
    );
    let fields = fields_of(&d, &longer);
    eprintln!("wf4c: fields listed for the longer activity: {fields:?}");
    eprintln!("wf4c: activities listed: {kinds:?}");
    for field in [
        "Budgeted Labor Units",
        "Early Finish",
        "Original Duration",
        "Planned Finish",
        "Remaining Duration",
        "Remaining Early Finish",
    ] {
        assert!(fields.contains(&field.to_string()), "{field} in {fields:?}");
    }
    close_preview(&mut d);

    // The log goes to the test folder.
    d.click("update_category_row_0");
    d.frames(1);
    let log = d.dir.join("update-log.txt");
    type_into(&mut d, "update_log_path", &log.display().to_string());
    // Update: no question, the done message with View Log.
    d.click("update_ok");
    d.frames(2);
    assert!(d.app.baselines.confirm.is_none(), "no question first");
    assert_eq!(d.app.baselines.update_done, Some(Some(log.clone())));
    d.click("update_done_view log");
    d.frames(2);
    let shown = d
        .app
        .baselines
        .log_view
        .as_ref()
        .expect("View Log shows the log");
    assert!(
        shown.1.contains(&added_code) && shown.1.contains(&gone),
        "{}",
        shown.1
    );
    d.click("update_log_close");
    assert!(d.app.baselines.log_view.is_none());
    assert!(d.app.baselines.update.is_some(), "the options stay open");

    // The baseline holds the update and its Last Update Date is set.
    let (doc, key, row) = baseline_doc(&d, bl);
    let codes: Vec<String> = xb::activity_codes(&doc, key).into_values().collect();
    assert!(codes.contains(&added_code) && !codes.contains(&gone));
    assert_eq!(
        bl_task(&doc, key, &renamed, "task_name"),
        "Set new pump renamed"
    );
    assert_eq!(bl_task(&doc, key, &longer, "target_drtn_hr_cnt"), "64");
    assert_eq!(
        bl_task(&doc, key, &longer, "target_work_qty").trim_end_matches(".0"),
        "40"
    );
    assert_eq!(
        xb::project_value(&doc, key, "last_baseline_update_date"),
        row.updated
    );
    assert!(row.updated >= taken.updated && !row.updated.is_empty());
    assert_eq!(row.added, taken.added, "Date Added stays");
    // With the same options there is nothing left to change.
    open_preview(&mut d);
    assert!(listed(&d).is_empty(), "{:?}", listed(&d));
    close_preview(&mut d);

    // Close after changing options: Cancel stays, Yes keeps them.
    d.click("update_close");
    d.frames(2);
    assert!(
        d.app.baselines.update.as_ref().unwrap().prompt,
        "asked to keep the options"
    );
    d.click("update_keep_cancel");
    d.frames(1);
    assert!(d.app.baselines.update.is_some());
    d.click("update_close");
    d.frames(2);
    d.click("update_keep_yes");
    d.frames(1);
    assert!(d.app.baselines.update.is_none());
    // Reopened: the options are as kept. Default, then Close and No.
    d.click(&baseline_line(&d, bl));
    d.click("baselines_update");
    d.frames(2);
    let o = d.app.baselines.update.as_ref().unwrap().opts.clone();
    assert!(o.add_new && o.dates && o.budget_no_assignments);
    d.click("update_default");
    d.frames(1);
    d.click("update_close");
    d.frames(2);
    d.click("update_keep_no");
    d.frames(1);
    assert!(d.app.baselines.update.is_none());
    d.click(&baseline_line(&d, bl));
    d.click("baselines_update");
    d.frames(2);
    assert!(
        d.app.baselines.update.as_ref().unwrap().opts.add_new,
        "No kept nothing"
    );
    // Unchanged options close without a question.
    d.click("update_close");
    d.frames(1);
    assert!(d.app.baselines.update.is_none());
    d.click("baselines_close");
    d.assert_clean("update baseline");
}
