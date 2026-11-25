//! Deleting an activity removes it, its relationships and every child row.
use openxer_engine::scheduler;
use openxer_xer::{starter, Document, ProjectData};
use std::path::PathBuf;

/// Every table whose rows belong to one activity through `task_id`. Listed
/// here on its own (not taken from the crate) so a table the crate forgets
/// still shows up as an orphan.
const ACTIVITY_TABLES: [&str; 12] = [
    "TASKRSRC", "TASKACTV", "TASKMEMO", "TASKPROC", "PROJCOST", "TASKNOTE", "TASKDOC", "TASKFDBK",
    "TASKRISK", "TASKUSER", "TASKFIN", "TRSRCFIN",
];

fn references(doc: &Document, task_id: &str) -> Vec<String> {
    let mut found = vec![];
    for t in &doc.tables {
        for r in &t.rows {
            let hit = match t.name.as_str() {
                "TASK" | "TASKPRED" => {
                    t.value(r, "task_id") == task_id || t.value(r, "pred_task_id") == task_id
                }
                "UDFVALUE" => t.value(r, "fk_id") == task_id && t.value(r, "udf_type_id") == "1",
                name if ACTIVITY_TABLES.contains(&name) => t.value(r, "task_id") == task_id,
                _ => false,
            };
            if hit {
                found.push(t.name.clone());
            }
        }
    }
    found
}

#[test]
fn delete_removes_every_child_row_and_reimports() {
    let mut p = starter();
    let doc = &mut p.document;
    let task = || ("task_id", "1050".to_string());
    let project = || ("proj_id", "1".to_string());
    doc.table_mut("TASKRSRC")
        .add(&[("taskrsrc_id", "1".into()), task(), project()]);
    doc.table_mut("TASKACTV")
        .add(&[task(), ("actv_code_id", "3".into()), project()]);
    doc.table_mut("TASKMEMO")
        .add(&[("memo_id", "1".into()), task(), project()]);
    doc.table_mut("TASKPROC")
        .add(&[("proc_id", "1".into()), task(), project()]);
    doc.table_mut("PROJCOST")
        .add(&[("cost_item_id", "1".into()), task(), project()]);
    doc.table_mut("TASKNOTE").add(&[task(), project()]);
    doc.table_mut("TASKDOC")
        .add(&[("taskdoc_id", "1".into()), task(), project()]);
    doc.table_mut("TASKFDBK").add(&[task(), project()]);
    doc.table_mut("TASKRISK").add(&[task(), project()]);
    doc.table_mut("TASKUSER").add(&[task(), project()]);
    doc.table_mut("UDFTYPE")
        .add(&[("udf_type_id", "1".into()), ("table_name", "TASK".into())]);
    doc.table_mut("UDFTYPE").add(&[
        ("udf_type_id", "2".into()),
        ("table_name", "PROJWBS".into()),
    ]);
    doc.table_mut("UDFVALUE").add(&[
        ("udf_type_id", "1".into()),
        ("fk_id", "1050".into()),
        project(),
    ]);
    // A WBS value whose key happens to equal the activity ID stays.
    doc.table_mut("UDFVALUE").add(&[
        ("udf_type_id", "2".into()),
        ("fk_id", "1050".into()),
        project(),
    ]);
    let links = p.model.links.len();
    let touching = p
        .model
        .links
        .iter()
        .filter(|l| {
            p.model.activities[l.pred].task_id == 1050 || p.model.activities[l.succ].task_id == 1050
        })
        .count();
    assert!(touching >= 2);
    let removed = p.delete_activities(&[1050]).unwrap();
    // TASK, links, one row in each of the ten tables added above, one UDF value.
    assert_eq!(removed, 1 + touching + 10 + 1);
    assert_eq!(p.model.activities.len(), 13);
    assert_eq!(p.model.links.len(), links - touching);
    assert!(!p.details.contains_key(&1050));
    p.model.validate().unwrap();
    p.refresh_derived_durations();
    scheduler::schedule_with_resources(&mut p.model, &p.resources).unwrap();
    let text = p.updated_document().text();
    let again = Document::parse(&text).unwrap();
    assert_eq!(references(&again, "1050"), Vec::<String>::new());
    let udf = again.table("UDFVALUE").unwrap();
    assert_eq!(udf.rows.len(), 1, "the WBS value survives");
    let mut reread = ProjectData::from_document(again, 1, "reimport".into()).unwrap();
    assert!(reread.warnings.is_empty(), "{:?}", reread.warnings);
    assert_eq!(reread.model.activities.len(), 13);
    reread.refresh_derived_durations();
    scheduler::schedule_with_resources(&mut reread.model, &reread.resources).unwrap();
    for (a, b) in reread.model.activities.iter().zip(&p.model.activities) {
        assert_eq!(
            (a.task_id, a.early_start, a.early_finish),
            (b.task_id, b.early_start, b.early_finish)
        );
    }
}

/// Rows in `table` whose `column` equals `value`.
fn count_where(doc: &Document, table: &str, column: &str, value: &str) -> usize {
    doc.table(table).map_or(0, |t| {
        t.rows
            .iter()
            .filter(|r| t.value(r, column) == value)
            .count()
    })
}

#[test]
fn delete_removes_period_actuals_and_assignment_field_values() {
    let mut p = starter();
    let doc = &mut p.document;
    let task = || ("task_id", "1050".to_string());
    let project = || ("proj_id", "1".to_string());
    doc.table_mut("TASKRSRC")
        .add(&[("taskrsrc_id", "77".into()), task(), project()]);
    doc.table_mut("TASKFIN")
        .add(&[("fin_dates_id", "1".into()), task(), project()]);
    doc.table_mut("TRSRCFIN").add(&[
        ("fin_dates_id", "1".into()),
        ("taskrsrc_id", "77".into()),
        task(),
        project(),
    ]);
    doc.table_mut("UDFTYPE").add(&[
        ("udf_type_id", "9".into()),
        ("table_name", "TASKRSRC".into()),
    ]);
    doc.table_mut("UDFVALUE").add(&[
        ("udf_type_id", "9".into()),
        ("fk_id", "77".into()),
        project(),
    ]);
    doc.table_mut("PROJISSU").add(&[
        ("issue_id", "1".into()),
        ("proj_id", "1".into()),
        task(),
        ("issue_name", "Late steel".into()),
    ]);
    p.delete_activities(&[1050]).unwrap();
    let again = Document::parse(&p.updated_document().text()).unwrap();
    assert_eq!(count_where(&again, "TASKFIN", "task_id", "1050"), 0);
    assert_eq!(count_where(&again, "TRSRCFIN", "task_id", "1050"), 0);
    assert_eq!(count_where(&again, "UDFVALUE", "fk_id", "77"), 0);
    let issues = again.table("PROJISSU").unwrap();
    assert_eq!(issues.rows.len(), 1, "the issue itself stays");
    assert_eq!(issues.value(&issues.rows[0], "task_id"), "");
}

/// On a corpus project, delete the activity with the most child rows.
#[test]
fn corpus_delete_reimports_cleanly() {
    let dir = std::env::var_os("OPENXER_CORPUS")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("OPENXER_CORPUS is not set"));
    let path = dir.join("5640.xer");
    let Ok(document) = Document::read(&path) else {
        println!("SKIPPED: {} not found (set OPENXER_CORPUS)", path.display());
        return;
    };
    let (pid, _) = document.projects()[0].clone();
    let mut p = ProjectData::from_document(document, pid, "5640.xer".into()).unwrap();
    let count = |doc: &Document, id: i64| references(doc, &id.to_string()).len();
    let victim = p
        .model
        .activities
        .iter()
        .filter(|a| !a.preds.is_empty() && !a.succs.is_empty())
        .max_by_key(|a| count(&p.document, a.task_id))
        .unwrap()
        .task_id;
    let before = count(&p.document, victim);
    assert!(before > 3, "the chosen activity has child rows");
    let removed = p.delete_activities(&[victim]).unwrap();
    assert!(removed >= before);
    let again = Document::parse(&p.updated_document().text()).unwrap();
    assert_eq!(count(&again, victim), 0);
    let mut reread = ProjectData::from_document(again, pid, "reimport".into()).unwrap();
    reread.refresh_derived_durations();
    scheduler::schedule_with_resources(&mut reread.model, &reread.resources).unwrap();
    println!("5640: deleted {victim} and {removed} rows; reimport schedules");
}

/// The starter with multiple float paths on, ending with activity `end`,
/// saved and read back so its SCHEDOPTIONS row names the ending activity.
fn ending_with(end: i64) -> ProjectData {
    let mut p = starter();
    p.model.options.multiple_float_paths = true;
    p.model.options.float_paths_end_activity = Some(end);
    let doc = Document::parse(&p.updated_document().text()).unwrap();
    let p = ProjectData::from_document(doc, 1, "float paths".into()).unwrap();
    assert_eq!(p.model.options.float_paths_end_activity, Some(end));
    p
}

/// The SCHEDOPTIONS cell naming the float-path ending activity.
fn stored_end(doc: &Document) -> String {
    let t = doc.table("SCHEDOPTIONS").unwrap();
    t.value(&t.rows[0], "key_activity_for_multiple_longest_paths")
        .to_string()
}

/// Deleting the activity multiple float paths end with blanks the option in
/// the same edit, so scheduling still works, and the saved file no longer
/// names it.
#[test]
fn deleting_the_float_path_ending_activity_blanks_the_option() {
    let mut p = ending_with(1050);
    assert_eq!(stored_end(&p.updated_document()), "1050");
    p.refresh_derived_durations();
    scheduler::schedule_with_resources(&mut p.model, &p.resources).unwrap();
    p.delete_activities(&[1050]).unwrap();
    assert!(
        p.model.options.multiple_float_paths,
        "only the ending activity goes"
    );
    assert_eq!(p.model.options.float_paths_end_activity, None);
    p.model.validate().unwrap();
    scheduler::schedule_with_resources(&mut p.model, &p.resources).unwrap();
    let doc = p.updated_document();
    assert_eq!(stored_end(&doc), "");
    let again = ProjectData::from_document(doc, 1, "reimport".into()).unwrap();
    assert!(again.warnings.is_empty(), "{:?}", again.warnings);
    assert_eq!(again.model.options.float_paths_end_activity, None);
}

/// Every way activities leave the model goes through
/// `remove_model_activities`, and the document-only delete the merges use
/// goes through `delete_activity_rows`: each blanks the ending activity.
#[test]
fn every_activity_removal_blanks_a_float_path_end_it_removes() {
    let mut p = ending_with(1050);
    p.remove_model_activities(&[1050]).unwrap();
    assert_eq!(p.model.options.float_paths_end_activity, None);
    p.model.validate().unwrap();
    // Another activity going leaves the option alone.
    let mut p = ending_with(1050);
    p.delete_activities(&[1040]).unwrap();
    assert_eq!(p.model.options.float_paths_end_activity, Some(1050));
    assert_eq!(stored_end(&p.updated_document()), "1050");
    // The document-level delete (XER merge, baseline update, spreadsheet
    // import) blanks the stored cell of the project it deletes from.
    let mut doc = ending_with(1050).updated_document();
    openxer_xer::delete::delete_activity_rows(&mut doc, 2, &[1050]);
    assert_eq!(stored_end(&doc), "1050", "another project's delete");
    openxer_xer::delete::delete_activity_rows(&mut doc, 1, &[1050]);
    assert_eq!(stored_end(&doc), "");
    let again = ProjectData::from_document(doc, 1, "reimport".into()).unwrap();
    assert!(again.warnings.is_empty(), "{:?}", again.warnings);
}
