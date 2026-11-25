use super::*;
use crate::workspace_scope::tests::{corpus, synthetic};

fn set(doc: &mut Document, table: &str, key: &str, id: i64, column: &str, text: &str) {
    let t = doc.table_mut(table);
    let i = t
        .rows
        .iter()
        .position(|r| num(t.value(r, key)) == id)
        .unwrap();
    t.set(i, column, text.into());
}

/// Hydrate each planned project from its stored text and the dictionary
/// text, as the database will, and check it against the plan and the
/// original's schedule.
fn prove(plan: &MigrationPlan, originals: &[(i64, String, Document)], pids: &[i64]) {
    let dict = Document::parse(&plan.dictionary.text()).unwrap();
    for ((p, (_, label, original)), pid) in plan.projects.iter().zip(originals).zip(pids) {
        let mut back = Document::parse(&p.stored.text()).unwrap();
        back.windows_encoding = original.windows_encoding;
        let report = hydrate(&mut back, &dict);
        assert!(report.shadowed.is_empty(), "{label}: {:?}", report.shadowed);
        verify_hydrated(&p.document, &back).unwrap_or_else(|e| panic!("{label}: {e}"));
        for level in [false, true] {
            assert_eq!(
                schedule_result(original, *pid, level),
                schedule_result(&back, *pid, level),
                "{label}: schedule (level {level}) changed"
            );
        }
    }
}

#[test]
fn two_projects_with_the_same_dictionary_share_it_and_keep_their_schedules() {
    let projects = vec![
        (1, "ALPHA".to_string(), synthetic(1, "ALPHA")),
        (2, "BETA".to_string(), synthetic(2, "BETA")),
    ];
    let plan = plan_migration(&projects);
    assert!(plan.projects.iter().all(|p| p.remap.is_empty()));
    assert_eq!(plan.dictionary.table("CALENDAR").unwrap().rows.len(), 3);
    assert_eq!(plan.keys["CALENDAR"], 4);
    assert!(schedule_result(&projects[0].2, 1, false).is_ok());
    prove(&plan, &projects, &[1, 2]);
    // Unchanged ids: each project hydrates back to its exact text.
    let mut back = plan.projects[1].stored.clone();
    hydrate(&mut back, &plan.dictionary);
    assert_eq!(
        used_subset(&back, 2).table("CALENDAR").unwrap().rows.len(),
        3,
        "the base, its calendar and the operator's"
    );
}

#[test]
fn conflicting_dictionaries_are_kept_apart_and_every_schedule_is_unchanged() {
    let a = synthetic(1, "ALPHA");
    let mut b = synthetic(2, "BETA");
    // A different "Standard" week, a resource with other limits, and a
    // conflicting code value.
    let week = crate::workspace_scope::tests::week().replace("16:00", "18:00");
    set(&mut b, "CALENDAR", "clndr_id", 2, "clndr_data", &week);
    set(
        &mut b,
        "RSRCRATE",
        "rsrc_rate_id",
        31,
        "max_qty_per_hr",
        "4",
    );
    set(
        &mut b,
        "ACTVCODE",
        "actv_code_id",
        42,
        "actv_code_name",
        "Precast",
    );
    let projects = vec![(1, "ALPHA".to_string(), a), (2, "BETA".to_string(), b)];
    let plan = plan_migration(&projects);
    let beta = &plan.projects[1];
    let copies: Vec<&str> = beta
        .events
        .iter()
        .filter(|e| e.kind == EventKind::Copied)
        .filter_map(|e| e.renamed.as_deref())
        .collect();
    assert!(copies.contains(&"Standard (BETA)"), "{copies:?}");
    assert!(copies.contains(&"OP (BETA)"), "{copies:?}");
    assert!(copies.contains(&"Phase (BETA)"), "{copies:?}");
    assert!(
        plan.projects[0].remap.is_empty(),
        "the first project keeps its ids"
    );
    prove(&plan, &projects, &[1, 2]);
    let report = plan.report();
    assert!(
        report.contains("\"Standard\" is kept as \"Standard (BETA)\""),
        "{report}"
    );
    assert!(report.contains("clndr_data"), "{report}");
}

#[test]
fn verify_finds_a_lost_row_and_a_changed_value() {
    let projects = vec![(1, "ALPHA".to_string(), synthetic(1, "ALPHA"))];
    let plan = plan_migration(&projects);
    let p = &plan.projects[0];
    let mut back = p.stored.clone();
    hydrate(&mut back, &plan.dictionary);
    verify_hydrated(&p.document, &back).unwrap();
    let mut lost = back.clone();
    lost.table_mut("RSRC").rows.pop();
    assert!(verify_hydrated(&p.document, &lost)
        .unwrap_err()
        .contains("RSRC row 8"));
    let mut changed = back.clone();
    set(
        &mut changed,
        "CALENDAR",
        "clndr_id",
        2,
        "clndr_name",
        "Other",
    );
    assert!(verify_hydrated(&p.document, &changed)
        .unwrap_err()
        .contains("clndr_name"));
    let mut task = back.clone();
    set(&mut task, "TASK", "task_id", 101, "task_name", "Moved");
    assert!(verify_hydrated(&p.document, &task)
        .unwrap_err()
        .contains("TASK"));
    let mut guid = back;
    set(&mut guid, "RSRC", "rsrc_id", 7, "guid", "other");
    verify_hydrated(&p.document, &guid).unwrap();
}

#[test]
fn corpus_projects_migrated_into_one_workspace_keep_every_date_and_float() {
    let Some(files) = corpus() else { return };
    let mut projects = vec![];
    let mut pids = vec![];
    for (i, path) in files.iter().enumerate() {
        let doc = Document::read(path).unwrap();
        for (pid, name) in doc.projects() {
            projects.push((i as i64 + 1, name, doc.clone()));
            pids.push(pid);
        }
    }
    let plan = plan_migration(&projects);
    let counts = plan.counts();
    println!(
        "{} corpus projects: {:?}; dictionary {:?}",
        projects.len(),
        counts,
        plan.dictionary_rows()
    );
    prove(&plan, &projects, &pids);
}
