use super::*;
use crate::workspace::new_project::{build, Spec};
use crate::{starter, starter_portfolio, Table};

/// Every value of `column` in every table that has it.
fn values(doc: &Document, column: &str) -> Vec<String> {
    doc.tables
        .iter()
        .filter_map(|t| t.field(column).map(|c| (t, c)))
        .flat_map(|(t, c)| {
            t.rows
                .iter()
                .map(move |r| r.get(c).map_or("", |v| v.trim()).to_string())
        })
        .filter(|v| !v.is_empty())
        .collect()
}

fn cell(t: &Table, i: usize, column: &str) -> String {
    t.value(&t.rows[i], column).to_string()
}

#[test]
fn next_is_above_every_number_in_use() {
    assert_eq!(next([]), 1);
    assert_eq!(next([3, 1]), 4);
    assert_eq!(next([-5]), 1);
}

#[test]
fn renumber_moves_every_row_of_the_project_and_the_links_into_it() {
    let mut doc = starter_portfolio();
    let before = doc.clone();
    renumber(&mut doc, 1, 9);
    for t in &doc.tables {
        let Some(c) = t.field("proj_id") else {
            continue;
        };
        let old = before.table(&t.name).unwrap();
        assert_eq!(t.rows.len(), old.rows.len());
        for (row, was) in t.rows.iter().zip(&old.rows) {
            let at = |r: &[String]| r.get(c).map_or("", |v| v.trim()).to_string();
            let want = match at(was).as_str() {
                "1" => "9".to_string(),
                other => other.to_string(),
            };
            assert_eq!(at(row), want, "{} proj_id", t.name);
        }
    }
    // NORTHYARD's relationship from RIVERBEND now names RIVERBEND's new number.
    let pred = doc.table("TASKPRED").unwrap();
    let row = pred
        .rows
        .iter()
        .position(|r| pred.value(r, "task_pred_id") == "200")
        .unwrap();
    assert_eq!(cell(pred, row, "pred_proj_id"), "9");
    assert_eq!(cell(pred, row, "proj_id"), "2");
    assert!(!values(&doc, "proj_id").contains(&"1".to_string()));
    assert!(!values(&doc, "pred_proj_id").contains(&"1".to_string()));
    // Both projects still load, and the cross-project link still resolves.
    let north = ProjectData::from_document(doc.clone(), 2, "p.xer".into()).unwrap();
    let links = north.external_links();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].pred_project, 9);
    let river = ProjectData::from_document(doc, 9, "p.xer".into()).unwrap();
    let old = ProjectData::from_document(before, 1, "p.xer".into()).unwrap();
    assert_eq!(river.model.activities.len(), old.model.activities.len());
    assert_eq!(river.model.links.len(), old.model.links.len());
}

#[test]
fn renumber_follows_project_fields_and_project_udf_values_only() {
    let mut doc = starter().document;
    let project = doc.table_mut("PROJECT");
    project.set(0, "sum_base_proj_id", "1".into());
    project.add(&[
        ("proj_id", "5".into()),
        ("proj_short_name", "RB - B1".into()),
        ("orig_proj_id", "1".into()),
    ]);
    let types = doc.table_mut("UDFTYPE");
    types.add(&[
        ("udf_type_id", "70".into()),
        ("table_name", "PROJECT".into()),
    ]);
    types.add(&[("udf_type_id", "71".into()), ("table_name", "TASK".into())]);
    let udf = doc.table_mut("UDFVALUE");
    // A project field of the project, and an activity field whose activity
    // happens to have the id 1.
    udf.add(&[
        ("udf_type_id", "70".into()),
        ("fk_id", "1".into()),
        ("proj_id", "1".into()),
        ("udf_text", "north".into()),
    ]);
    udf.add(&[
        ("udf_type_id", "71".into()),
        ("fk_id", "1".into()),
        ("proj_id", "1".into()),
        ("udf_text", "task".into()),
    ]);
    renumber(&mut doc, 1, 12);
    let project = doc.table("PROJECT").unwrap();
    assert_eq!(cell(project, 0, "proj_id"), "12");
    assert_eq!(cell(project, 0, "sum_base_proj_id"), "12");
    assert_eq!(
        cell(project, 1, "proj_id"),
        "5",
        "the baseline keeps its own"
    );
    assert_eq!(
        cell(project, 1, "orig_proj_id"),
        "12",
        "and names its owner"
    );
    let udf = doc.table("UDFVALUE").unwrap();
    assert_eq!(cell(udf, 0, "fk_id"), "12", "the project field follows");
    assert_eq!(
        cell(udf, 1, "fk_id"),
        "1",
        "an activity id is not a project"
    );
    assert_eq!(cell(udf, 0, "proj_id"), "12");
    assert_eq!(cell(udf, 1, "proj_id"), "12");
}

#[test]
fn a_renumbered_project_schedules_the_same_and_survives_the_xer_round_trip() {
    use openxer_engine::scheduler::schedule_with_resources_report as schedule;
    let mut p = starter();
    let mut q = p.clone();
    q.renumber_project(41).unwrap();
    assert_eq!(q.model.project.proj_id, 41);
    assert!(!values(&q.document, "proj_id").contains(&"1".to_string()));
    schedule(&mut p.model, &p.resources).unwrap();
    schedule(&mut q.model, &q.resources).unwrap();
    let dates = |m: &openxer_engine::model::Model| -> Vec<_> {
        m.activities
            .iter()
            .map(|a| {
                (
                    a.task_code.clone(),
                    a.early_start,
                    a.early_finish,
                    a.late_finish,
                    a.total_float,
                )
            })
            .collect()
    };
    assert_eq!(dates(&q.model), dates(&p.model), "the same results");
    // Written as XER and read back under its new number: the same project.
    let text = q.updated_document().text();
    let back =
        ProjectData::from_document(Document::parse(&text).unwrap(), 41, "rt".into()).unwrap();
    assert_eq!(dates(&back.model), dates(&q.model));
    assert_eq!(back.model.links.len(), p.model.links.len());
    assert_eq!(back.wbs, q.wbs);
    assert_eq!(
        values(&back.document, "proj_id")
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>(),
        std::collections::BTreeSet::from(["41".to_string()])
    );
    // A number the file already uses is refused.
    let mut r = ProjectData::from_document(starter_portfolio(), 1, "p".into()).unwrap();
    assert!(r.renumber_project(2).is_err());
    assert_eq!(r.model.project.proj_id, 1, "left as it was");
}

/// Two projects made apart share activity ids (both start at the starter's);
/// with numbers of their own, a link from one to the other survives the XER
/// round trip as a link to that project, never as one of this project's own.
#[test]
fn a_link_from_another_project_with_the_same_activity_ids_stays_a_link_to_it() {
    use openxer_engine::model::LinkType;
    let mut a = starter();
    a.renumber_project(2).unwrap();
    let mut b = starter();
    b.renumber_project(3).unwrap();
    let task = b.model.activities[0].task_id;
    assert!(a.model.activities.iter().any(|x| x.task_id == task));
    let own = b.model.links.len();
    b.add_external_link(2, task, task, LinkType::FS, 0).unwrap();
    let read = |p: &ProjectData| {
        let text = p.updated_document().text();
        ProjectData::from_document(Document::parse(&text).unwrap(), 3, "b".into()).unwrap()
    };
    let back = read(&b);
    assert_eq!(back.model.links.len(), own, "not read as one of B's own");
    let links = back.external_links();
    assert_eq!(links.len(), 1);
    assert_eq!((links[0].pred_project, links[0].pred_task), (2, task));
    let again = read(&back);
    assert_eq!(
        again.external_links(),
        links,
        "and again after another save"
    );
    assert_eq!(again.model.links.len(), own);
}

#[test]
fn a_new_project_carries_the_number_it_is_given_everywhere() {
    let spec = Spec {
        eps: 1,
        project_id: "N-1".into(),
        name: "New".into(),
        planned_start: 0,
        must_finish: None,
        manager: String::new(),
        rate_type: "COST_PER_QTY".into(),
    };
    let a = build(&spec, None, 7).unwrap();
    let b = build(&spec, None, 8).unwrap();
    assert_eq!(a.model.project.proj_id, 7);
    assert_eq!(b.model.project.proj_id, 8);
    for (p, n) in [(&a, "7"), (&b, "8")] {
        let all = values(&p.updated_document(), "proj_id");
        assert!(!all.is_empty());
        assert!(all.iter().all(|v| v == n), "{all:?}");
    }
}
