//! Tests for the editing commands. Data rules are tested on `ProjectData`;
//! production-UI tests drive `App::update_ui` through egui `RawInput` only:
//! no native window, no system pointer.
use super::gc::{Assignment, Condition, GlobalChange};
use super::ops::{self, CopyOptions, Renumber};
use super::*;
use crate::grid::layout::ColumnSetting;
use crate::records;
use openxer_engine::model::LinkType;
use std::path::PathBuf;

fn starter() -> ProjectData {
    let mut p = openxer_xer::starter();
    schedule_model(&mut p.model, &p.resources).unwrap();
    p
}

fn index_of(p: &ProjectData, code: &str) -> usize {
    p.model
        .activities
        .iter()
        .position(|a| a.task_code == code)
        .unwrap_or_else(|| panic!("no activity {code}"))
}

fn rows_of(p: &ProjectData, table: &str, key: &str, task_id: i64) -> usize {
    p.document.table(table).map_or(0, |t| {
        t.rows
            .iter()
            .filter(|r| records::num(t.value(r, key)) == task_id)
            .count()
    })
}

/// The starter project with one row in every activity detail table for
/// RB-1010 (task 1010): code, UDF, notebook, step, expense, document,
/// risk, note and a resource assignment.
fn rich() -> ProjectData {
    let mut p = starter();
    let doc = &mut p.document;
    doc.table_mut("ACTVTYPE").add(&[
        ("actv_code_type_id", "7".into()),
        ("actv_code_type", "Area".into()),
        ("actv_code_type_scope", "AS_Project".into()),
        ("proj_id", "1".into()),
    ]);
    doc.table_mut("ACTVCODE").add(&[
        ("actv_code_id", "71".into()),
        ("actv_code_type_id", "7".into()),
        ("short_name", "N".into()),
    ]);
    doc.table_mut("UDFTYPE").add(&[
        ("udf_type_id", "9".into()),
        ("table_name", "TASK".into()),
        ("udf_type_label", "Crew".into()),
        ("logical_data_type", "FT_TEXT".into()),
    ]);
    doc.table_mut("MEMOTYPE")
        .add(&[("memo_type_id", "3".into()), ("memo_type", "Scope".into())]);
    doc.table_mut("RSRC").add(&[
        ("rsrc_id", "5".into()),
        ("rsrc_name", "Pipefitter".into()),
        ("rsrc_type", "RT_Labor".into()),
    ]);
    doc.table_mut("DOCUMENT").add(&[
        ("doc_id", "4".into()),
        ("proj_id", "1".into()),
        ("doc_name", "Spec".into()),
    ]);
    doc.table_mut("RISK")
        .add(&[("risk_id", "6".into()), ("proj_id", "1".into())]);
    let id = |s: &str| s.to_string();
    for (table, values) in [
        (
            "TASKACTV",
            vec![
                ("task_id", id("1010")),
                ("actv_code_type_id", id("7")),
                ("actv_code_id", id("71")),
                ("proj_id", id("1")),
            ],
        ),
        (
            "UDFVALUE",
            vec![
                ("udf_type_id", id("9")),
                ("fk_id", id("1010")),
                ("proj_id", id("1")),
                ("udf_text", id("Civil crew")),
            ],
        ),
        (
            "TASKMEMO",
            vec![
                ("memo_id", id("1")),
                ("task_id", id("1010")),
                ("memo_type_id", id("3")),
                ("proj_id", id("1")),
                ("task_memo", id("<p>Survey</p>")),
            ],
        ),
        (
            "TASKPROC",
            vec![
                ("proc_id", id("1")),
                ("task_id", id("1010")),
                ("proj_id", id("1")),
                ("seq_num", id("1")),
                ("proc_name", id("Stake out")),
            ],
        ),
        (
            "PROJCOST",
            vec![
                ("cost_item_id", id("1")),
                ("task_id", id("1010")),
                ("proj_id", id("1")),
                ("cost_name", id("Survey crew")),
                ("target_cost", id("900")),
            ],
        ),
        (
            "TASKDOC",
            vec![
                ("taskdoc_id", id("1")),
                ("doc_id", id("4")),
                ("proj_id", id("1")),
                ("task_id", id("1010")),
            ],
        ),
        (
            "TASKRISK",
            vec![
                ("task_id", id("1010")),
                ("risk_id", id("6")),
                ("proj_id", id("1")),
            ],
        ),
        (
            "TASKNOTE",
            vec![
                ("task_id", id("1010")),
                ("proj_id", id("1")),
                ("task_notes", id("Bring the level")),
            ],
        ),
    ] {
        records::add_row(doc, table, &values);
    }
    let t = doc.table_mut("TASKRSRC");
    t.fields = [
        "taskrsrc_id",
        "task_id",
        "proj_id",
        "rsrc_id",
        "target_qty",
        "remain_qty",
        "guid",
    ]
    .map(String::from)
    .to_vec();
    t.add(&[
        ("taskrsrc_id", "1".into()),
        ("task_id", "1010".into()),
        ("proj_id", "1".into()),
        ("rsrc_id", "5".into()),
        ("target_qty", "24".into()),
        ("remain_qty", "24".into()),
        ("guid", "AAAAAAAAAAAAAAAAAAAAAA".into()),
    ]);
    p.rebuild_assignments();
    p
}

const DETAIL: [(&str, &str); 9] = [
    ("TASKRSRC", "task_id"),
    ("TASKACTV", "task_id"),
    ("UDFVALUE", "fk_id"),
    ("TASKMEMO", "task_id"),
    ("TASKPROC", "task_id"),
    ("PROJCOST", "task_id"),
    ("TASKDOC", "task_id"),
    ("TASKRISK", "task_id"),
    ("TASKNOTE", "task_id"),
];

/// Every detail row and relationship of the project refers to an activity
/// that exists in the exported TASK table.
fn assert_no_orphans(doc: &openxer_xer::Document) {
    let tasks: BTreeSet<i64> = doc
        .table("TASK")
        .map(|t| {
            t.rows
                .iter()
                .map(|r| records::num(t.value(r, "task_id")))
                .collect()
        })
        .unwrap_or_default();
    for (table, key) in DETAIL {
        if let Some(t) = doc.table(table) {
            for r in &t.rows {
                let id = records::num(t.value(r, key));
                assert!(tasks.contains(&id), "{table} row for missing activity {id}");
            }
        }
    }
    if let Some(t) = doc.table("TASKPRED") {
        for r in &t.rows {
            for k in ["task_id", "pred_task_id"] {
                let id = records::num(t.value(r, k));
                assert!(tasks.contains(&id), "TASKPRED {k} {id} has no activity");
            }
        }
    }
}

// ------------------------------------------------------------- numbering

#[test]
fn numbering_reads_project_defaults_and_hands_out_free_ids() {
    let mut p = starter();
    assert_eq!(
        ops::numbering(&p),
        openxer_xer::edit::new_activity::Numbering::default()
    );
    let t = p.document.table_mut("PROJECT");
    t.set(0, "task_code_prefix", "MP-".into());
    t.set(0, "task_code_base", "0100".into());
    t.set(0, "task_code_step", "5".into());
    t.set(0, "task_code_prefix_flag", "Y".into());
    let n = ops::numbering(&p);
    assert_eq!(
        n,
        openxer_xer::edit::new_activity::Numbering {
            prefix: "MP-".into(),
            suffix: "0100".into(),
            step: 5,
            from_selected: true
        }
    );
    assert_eq!(ops::split_id("RB-1010"), ("RB-", Some(1010), 4));
    assert_eq!(ops::split_id("MOB"), ("MOB", None, 0));
    let mut ids = ops::Ids::new(&p);
    assert_eq!(ids.auto(&n), "MP-0100");
    assert_eq!(ids.auto(&n), "MP-0105");
    // From an ID: same text and digit count, past IDs in use.
    assert_eq!(ids.after("RB-1000", 10), "RB-1140");
    assert_eq!(ids.after("RB-1120", 5), "RB-1125");
    assert!(!ids.take("RB-1125"));
    // Auto-numbering continues after the highest number with the prefix.
    p.model.activities[0].task_code = "MP-0500".into();
    let mut ids = ops::Ids::new(&p);
    assert_eq!(ids.auto(&n), "MP-0505");
}

// ------------------------------------------------------------ copy/paste

#[test]
fn paste_copies_child_rows_with_new_keys_ids_and_wbs() {
    let mut p = rich();
    let a = index_of(&p, "RB-1010");
    let b = index_of(&p, "RB-1020");
    let clip = ops::copy(&p, &[a, b]);
    assert_eq!(
        clip.activities[0].children.len(),
        9,
        "one row per detail table"
    );
    let before_links = p.model.links.len();
    let rows = ops::paste(&mut p, &clip, &CopyOptions::default(), 30, None).unwrap();
    assert_eq!(rows.len(), 2);
    let (x, y) = (&p.model.activities[rows[0]], &p.model.activities[rows[1]]);
    // Taken IDs are replaced by the project's auto-numbering (A, 1000, 10).
    assert_eq!(
        (x.task_code.as_str(), y.task_code.as_str()),
        ("A1000", "A1010")
    );
    assert!(x.task_id > 1130 && y.task_id == x.task_id + 1);
    assert_eq!(p.details[&x.task_id].wbs, 30);
    assert_eq!(p.details[&x.task_id].name, "Site investigation & survey");
    assert_eq!(x.rem, p.model.activities[a].rem);
    // Every detail row came along with new keys.
    for (table, key) in DETAIL {
        assert_eq!(rows_of(&p, table, key, x.task_id), 1, "{table}");
        assert_eq!(rows_of(&p, table, key, 1010), 1, "{table} source kept");
    }
    let rsrc = p.document.table("TASKRSRC").unwrap();
    let copy = rsrc
        .rows
        .iter()
        .find(|r| records::num(rsrc.value(r, "task_id")) == x.task_id)
        .unwrap();
    assert_eq!(rsrc.value(copy, "taskrsrc_id"), "2");
    assert_eq!(rsrc.value(copy, "guid").len(), 22);
    assert_ne!(rsrc.value(copy, "guid"), "AAAAAAAAAAAAAAAAAAAAAA");
    assert_eq!(
        records::lookup(&p.document, "TASKMEMO", "task_id", x.task_id, "memo_id"),
        "2"
    );
    // Only the relationship between the two copies is added by default.
    assert_eq!(p.model.links.len(), before_links + 1);
    let new = p.model.links.last().unwrap();
    assert_eq!(
        (new.pred, new.succ, new.link_type),
        (rows[0], rows[1], LinkType::FS)
    );
    // With "Only between copied activities" off, outside links are copied too.
    let mut q = rich();
    let o = CopyOptions {
        only_between: false,
        ..Default::default()
    };
    let before = q.model.links.len();
    let clip = ops::copy(&q, &[a, b]);
    let rows = ops::paste(&mut q, &clip, &o, 30, None).unwrap();
    let touching = |p: &ProjectData, i: usize| {
        p.model
            .links
            .iter()
            .filter(|l| l.pred == i || l.succ == i)
            .count()
    };
    assert_eq!(touching(&q, rows[0]), touching(&q, a));
    assert_eq!(touching(&q, rows[1]), touching(&q, b));
    assert_eq!(
        q.model.links.len(),
        before + 1 + (touching(&q, a) - 1) + (touching(&q, b) - 1)
    );
    // Unchecked parts stay behind.
    let mut r = rich();
    let o = CopyOptions {
        relationships: false,
        assignments: false,
        codes: false,
        notebooks: false,
        steps: false,
        expenses: false,
        documents: false,
        risks: false,
        ..Default::default()
    };
    let links = r.model.links.len();
    let clip = ops::copy(&r, &[a]);
    let rows = ops::paste(&mut r, &clip, &o, 10, None).unwrap();
    let t = r.model.activities[rows[0]].task_id;
    assert_eq!(r.model.links.len(), links);
    for (table, key) in DETAIL {
        let expect = usize::from(matches!(table, "UDFVALUE" | "TASKNOTE"));
        assert_eq!(rows_of(&r, table, key, t), expect, "{table}");
    }
}

#[test]
fn pasted_activities_export_and_reimport_cleanly() {
    let mut p = rich();
    // A TASK table as the reference scheduler writes it carries the GUID and stamp columns.
    let t = p.document.table_mut("TASK");
    for k in ["guid", "create_date", "update_date"] {
        t.set(0, k, String::new());
    }
    let a = index_of(&p, "RB-1010");
    let clip = ops::copy(&p, &[a, a + 1, a + 2]);
    let rows = ops::paste(&mut p, &clip, &CopyOptions::default(), 20, None).unwrap();
    let ids: Vec<i64> = rows
        .iter()
        .map(|&i| p.model.activities[i].task_id)
        .collect();
    let exported = p.updated_document();
    assert_no_orphans(&exported);
    let tasks = exported.table("TASK").unwrap();
    for id in &ids {
        let row = tasks
            .rows
            .iter()
            .find(|r| records::num(tasks.value(r, "task_id")) == *id)
            .unwrap();
        assert_eq!(tasks.value(row, "guid").len(), 22, "new GUID");
        let i = p
            .model
            .activities
            .iter()
            .position(|a| a.task_id == *id)
            .unwrap();
        assert_eq!(
            tasks.value(row, "task_type"),
            p.model.activities[i].task_type.as_str()
        );
        assert_eq!(tasks.value(row, "wbs_id"), "20");
        assert!(!tasks.value(row, "create_date").is_empty());
        assert_eq!(tasks.value(row, "duration_type"), "DT_FixedDUR2");
    }
    let guids: BTreeSet<&str> = tasks
        .rows
        .iter()
        .map(|r| tasks.value(r, "guid"))
        .filter(|g| !g.is_empty())
        .collect();
    assert_eq!(guids.len(), 3);
    let text = exported.text();
    let reread =
        ProjectData::from_document(openxer_xer::Document::parse(&text).unwrap(), 1, "x".into())
            .unwrap();
    assert_eq!(reread.model.activities.len(), 17);
    assert_eq!(reread.model.links, p.model.links);
    for id in &ids {
        let i = reread
            .model
            .activities
            .iter()
            .position(|a| a.task_id == *id)
            .unwrap();
        assert_eq!(reread.details[id].wbs, 20);
        assert_eq!(
            reread.model.activities[i].task_code,
            p.model.activities[rows[0] + (i - 14)].task_code
        );
    }
    // Scheduling the pasted network works and the export stays stable.
    let mut q = reread.clone();
    schedule_model(&mut q.model, &q.resources).unwrap();
    assert!(q.model.activities.iter().all(|a| a.early_start.is_some()));
    assert_eq!(reread.updated_document().text(), text);
}

#[test]
fn paste_into_another_project_keeps_free_ids_and_drops_missing_references() {
    let source = rich();
    let a = index_of(&source, "RB-1010");
    let clip = ops::copy(&source, &[a]);
    let mut target = starter();
    target.model.project.name = "OTHER".into();
    target.delete_activities(&[1010]).unwrap();
    let rows = ops::paste(&mut target, &clip, &CopyOptions::default(), 40, None).unwrap();
    let x = &target.model.activities[rows[0]];
    assert_eq!(x.task_code, "RB-1010", "a free ID is kept");
    let id = x.task_id;
    // The target has no resource, code, memo type, document or risk
    // dictionaries: those rows are dropped; the step, expense and note stay.
    for (table, key) in DETAIL {
        let expect = usize::from(matches!(table, "TASKPROC" | "PROJCOST" | "TASKNOTE"));
        assert_eq!(rows_of(&target, table, key, id), expect, "{table}");
    }
    assert_no_orphans(&target.updated_document());
}

#[test]
fn corpus_activities_copy_with_every_child_row() {
    let path = crate::test_corpus::file("5640.xer");
    let Ok(doc) = openxer_xer::Document::read(&path) else {
        return;
    };
    let key = doc.projects()[0].0;
    let mut p = ProjectData::from_document(doc, key, "5640".into()).unwrap();
    let rows: Vec<usize> = (0..p.model.activities.len().min(40)).collect();
    let clip = ops::copy(&p, &rows);
    let children: usize = clip.activities.iter().map(|c| c.children.len()).sum();
    assert!(children > 0);
    let wbs = p.details[&p.model.activities[0].task_id].wbs;
    let tables_before: Vec<(String, usize)> = p
        .document
        .tables
        .iter()
        .map(|t| (t.name.clone(), t.rows.len()))
        .collect();
    let pasted = ops::paste(&mut p, &clip, &CopyOptions::default(), wbs, None).unwrap();
    assert_eq!(pasted.len(), rows.len());
    let grown: usize = p
        .document
        .tables
        .iter()
        .filter(|t| DETAIL.iter().any(|(n, _)| *n == t.name))
        .map(|t| t.rows.len())
        .sum::<usize>()
        - tables_before
            .iter()
            .filter(|(n, _)| DETAIL.iter().any(|(d, _)| d == n))
            .map(|(_, c)| c)
            .sum::<usize>();
    assert_eq!(grown, children);
    let exported = p.updated_document();
    let reread = ProjectData::from_document(
        openxer_xer::Document::parse(&exported.text()).unwrap(),
        key,
        "5640".into(),
    )
    .unwrap();
    assert_eq!(reread.model.activities.len(), p.model.activities.len());
    assert_eq!(reread.model.links.len(), p.model.links.len());
    let codes: BTreeSet<&str> = reread
        .model
        .activities
        .iter()
        .map(|a| a.task_code.as_str())
        .collect();
    assert_eq!(codes.len(), reread.model.activities.len(), "IDs unique");
}

// ------------------------------------------------------------- dissolve

#[test]
fn dissolve_joins_the_predecessor_start_to_the_successor_end_and_adds_both_lags() {
    // RB-1050 (5) has predecessor RB-1030 (3) and successor RB-1080 (8).
    let mut p = starter();
    let x = index_of(&p, "RB-1050");
    let pred = index_of(&p, "RB-1030");
    let succ = index_of(&p, "RB-1080");
    let l = p
        .model
        .links
        .iter_mut()
        .find(|l| l.pred == pred && l.succ == x)
        .unwrap();
    l.link_type = LinkType::SS;
    l.lag = 960;
    let l = p
        .model
        .links
        .iter_mut()
        .find(|l| l.pred == x && l.succ == succ)
        .unwrap();
    l.link_type = LinkType::FF;
    l.lag = 480;
    p.model.rebuild_links().unwrap();
    let mut q = p.clone();
    let links = p.model.links.len();
    assert_eq!(ops::dissolve(&mut p, &[1050], true).unwrap().added, 1);
    assert_eq!(p.model.links.len(), links - 2 + 1);
    assert!(p.model.activities.iter().all(|a| a.task_id != 1050));
    let (pred, succ) = (index_of(&p, "RB-1030"), index_of(&p, "RB-1080"));
    let new = p
        .model
        .links
        .iter()
        .find(|l| l.pred == pred && l.succ == succ)
        .unwrap();
    // SS gives the predecessor's start, FF the successor's finish: SF, with
    // the two lags added.
    assert_eq!((new.link_type, new.lag), (LinkType::SF, 960 + 480));
    // Without Retain Lag the new relationship has no lag.
    ops::dissolve(&mut q, &[1050], false).unwrap();
    let (pred, succ) = (index_of(&q, "RB-1030"), index_of(&q, "RB-1080"));
    let new = q
        .model
        .links
        .iter()
        .find(|l| l.pred == pred && l.succ == succ)
        .unwrap();
    assert_eq!((new.link_type, new.lag), (LinkType::SF, 0));
    // The export has no rows left for the dissolved activity.
    let doc = p.updated_document();
    assert_no_orphans(&doc);
    assert_eq!(rows_of(&p, "TASK", "task_id", 1050), 0);
    let reread = ProjectData::from_document(
        openxer_xer::Document::parse(&doc.text()).unwrap(),
        1,
        "x".into(),
    )
    .unwrap();
    assert_eq!(reread.model.links, p.model.links);
}

#[test]
fn dissolve_does_not_duplicate_links_and_chains_several_activities() {
    // RB-1020 (2) feeds RB-1030 (3) and RB-1040 (4); RB-1010 feeds RB-1020.
    let mut p = starter();
    let (a, c) = (index_of(&p, "RB-1010"), index_of(&p, "RB-1030"));
    crate::relationships::add(&mut p.model, a, c, LinkType::FS, 0).unwrap();
    let links = p.model.links.len();
    // RB-1010 -> RB-1030 exists already; only RB-1010 -> RB-1040 is new.
    assert_eq!(ops::dissolve(&mut p, &[1020], true).unwrap().added, 1);
    assert_eq!(p.model.links.len(), links - 3 + 1);
    // Dissolving a chain one after another keeps the path connected.
    let mut q = starter();
    ops::dissolve(&mut q, &[1010, 1020], true).unwrap();
    let (start, design) = (index_of(&q, "RB-1000"), index_of(&q, "RB-1030"));
    assert!(q
        .model
        .links
        .iter()
        .any(|l| l.pred == start && l.succ == design));
}

// ------------------------------------------------------------- renumber

#[test]
fn renumber_rules_and_collisions() {
    let codes: Vec<String> = ["RB-1000", "RB-1010", "X7"].map(String::from).to_vec();
    assert_eq!(
        ops::renumber_ids(&codes, &Renumber::Increment { step: 5 }).unwrap(),
        ["RB-1000", "RB-1005", "RB-1010"]
    );
    assert_eq!(
        ops::renumber_ids(
            &codes,
            &Renumber::Auto {
                prefix: "EL-".into(),
                suffix: "0010".into(),
                step: 10
            }
        )
        .unwrap(),
        ["EL-0010", "EL-0020", "EL-0030"]
    );
    assert_eq!(
        ops::renumber_ids(
            &codes,
            &Renumber::Replace {
                count: 3,
                with: "FL2-".into()
            }
        )
        .unwrap(),
        ["FL2-1000", "FL2-1010", "FL2-"]
    );
    assert!(ops::renumber_ids(
        &codes,
        &Renumber::Replace {
            count: 9,
            with: String::new()
        }
    )
    .is_err());
    assert!(ops::renumber_ids(&codes, &Renumber::Increment { step: 0 }).is_err());
    let p = starter();
    // Renumbering RB-1000 and RB-1010 by 5 collides only with RB-1010, which
    // is being renumbered itself, so there is no conflict.
    let plan = vec![(0, "RB-1000".to_string()), (1, "RB-1005".to_string())];
    assert!(ops::renumber_conflicts(&p, &plan).is_empty());
    // Swapping two IDs is fine; taking an ID that stays is not.
    let swap = vec![(0, "RB-1010".to_string()), (1, "RB-1000".to_string())];
    assert!(ops::renumber_conflicts(&p, &swap).is_empty());
    let clash = vec![(0, "RB-1020".to_string()), (1, "RB-1030".to_string())];
    assert_eq!(ops::renumber_conflicts(&p, &clash), [0, 1]);
    let twice = vec![(0, "N".to_string()), (1, "N".to_string())];
    assert_eq!(ops::renumber_conflicts(&p, &twice), [0, 1]);
}

// ------------------------------------------------------------------ text

#[test]
fn tab_separated_text_round_trips_quotes_tabs_and_line_breaks() {
    let rows = vec![
        vec!["Activity ID".to_string(), "Name".into()],
        vec!["A1".into(), "Pour \"slab\"\tnorth".into()],
        vec!["A2".into(), "two\nlines".into()],
        vec!["A3".into(), String::new()],
    ];
    let text = ops::to_tsv(&rows);
    assert!(text.ends_with("\r\n"));
    assert_eq!(ops::parse_tsv(&text), rows);
    assert_eq!(ops::parse_tsv("a\tb\nc"), vec![vec!["a", "b"], vec!["c"]]);
    assert!(ops::same_text("a\tb\r\n", "a\tb\n"));
}

// ---------------------------------------------------------- production UI

fn frame(
    app: &mut App,
    ctx: &egui::Context,
    events: Vec<egui::Event>,
    time: &mut f64,
    modifiers: egui::Modifiers,
) {
    *time += 0.1;
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1500., 900.))),
            time: Some(*time),
            events,
            modifiers,
            focused: true,
            ..Default::default()
        },
        |ctx| app.update_ui(ctx),
    );
}

fn idle(app: &mut App, ctx: &egui::Context, time: &mut f64) {
    frame(app, ctx, vec![], time, Default::default());
}

fn setup(name: &str, project: ProjectData) -> (PathBuf, egui::Context, App, f64) {
    setup_with(name, project, true)
}

fn setup_with(name: &str, project: ProjectData, warm: bool) -> (PathBuf, egui::Context, App, f64) {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "openxer-editing-{name}-{}-{unique}.db",
        std::process::id()
    ));
    let store = Store::open(path.clone()).unwrap();
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.);
    let mut app = App::new(&ctx, store, project);
    app.grid.layout.columns = [
        "activity_id",
        "activity_name",
        "original_duration",
        "activity_status",
    ]
    .iter()
    .map(|id| ColumnSetting::new(id, 120.))
    .collect();
    let mut time = 0.;
    if warm {
        for _ in 0..3 {
            idle(&mut app, &ctx, &mut time);
        }
    }
    (path, ctx, app, time)
}

fn press(
    app: &mut App,
    ctx: &egui::Context,
    pos: Pos2,
    modifiers: egui::Modifiers,
    time: &mut f64,
) {
    idle(app, ctx, time);
    for pressed in [true, false] {
        frame(
            app,
            ctx,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers,
                },
            ],
            time,
            modifiers,
        );
    }
    frame(
        app,
        ctx,
        vec![egui::Event::PointerGone],
        time,
        Default::default(),
    );
    idle(app, ctx, time);
}

fn click_action(app: &mut App, ctx: &egui::Context, action: &str, time: &mut f64) {
    idle(app, ctx, time);
    idle(app, ctx, time);
    let [l, t, r, b] = *app
        .action_rects
        .get(action)
        .unwrap_or_else(|| panic!("No rendered control {action:?}"));
    press(
        app,
        ctx,
        Pos2::new((l + r) / 2., (t + b) / 2.),
        Default::default(),
        time,
    );
}

/// The center of activity `code`'s cell in column `id`.
fn cell(app: &App, code: &str, id: &str) -> Pos2 {
    let task_id = app.project.model.activities[index_of(&app.project, code)].task_id;
    let row = app
        .bar_geometry
        .iter()
        .find(|b| b.task_id == task_id)
        .unwrap()
        .row;
    let [l, _, r, _] = app.action_rects[&format!("grid_header_{id}")];
    Pos2::new((l + r) / 2., row.center().y)
}

fn codes(app: &App, rows: &[usize]) -> Vec<String> {
    rows.iter()
        .map(|&i| app.project.model.activities[i].task_code.clone())
        .collect()
}

const SHIFT: egui::Modifiers = egui::Modifiers {
    alt: false,
    ctrl: false,
    shift: true,
    mac_cmd: false,
    command: false,
};
const CTRL: egui::Modifiers = egui::Modifiers {
    alt: false,
    ctrl: true,
    shift: false,
    mac_cmd: false,
    command: true,
};

#[test]
fn click_shift_click_ctrl_click_and_ctrl_a_select_rows() {
    let (path, ctx, mut app, mut time) = setup("select", starter());
    let pos = cell(&app, "RB-1010", "activity_name");
    press(&mut app, &ctx, pos, Default::default(), &mut time);
    assert_eq!(codes(&app, &app.selected_in_order()), ["RB-1010"]);
    let pos = cell(&app, "RB-1030", "activity_name");
    press(&mut app, &ctx, pos, SHIFT, &mut time);
    assert_eq!(
        codes(&app, &app.selected_in_order()),
        ["RB-1010", "RB-1020", "RB-1030"]
    );
    let pos = cell(&app, "RB-1020", "activity_name");
    press(&mut app, &ctx, pos, CTRL, &mut time);
    assert_eq!(
        codes(&app, &app.selected_in_order()),
        ["RB-1010", "RB-1030"]
    );
    let pos = cell(&app, "RB-1100", "activity_name");
    press(&mut app, &ctx, pos, CTRL, &mut time);
    assert_eq!(
        codes(&app, &app.selected_in_order()),
        ["RB-1010", "RB-1030", "RB-1100"]
    );
    assert!(app.is_selected(index_of(&app.project, "RB-1100")));
    // Keyboard navigation drops back to one row.
    frame(
        &mut app,
        &ctx,
        vec![egui::Event::Key {
            key: Key::ArrowDown,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        }],
        &mut time,
        Default::default(),
    );
    assert_eq!(app.selected_in_order().len(), 1);
    frame(
        &mut app,
        &ctx,
        vec![egui::Event::Key {
            key: Key::A,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: CTRL,
        }],
        &mut time,
        CTRL,
    );
    assert_eq!(app.selected_in_order().len(), 14);
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn copy_paste_cut_and_undo_through_keyboard_events() {
    let (path, ctx, mut app, mut time) = setup("copy", rich());
    let pos = cell(&app, "RB-1010", "activity_name");
    press(&mut app, &ctx, pos, Default::default(), &mut time);
    let pos = cell(&app, "RB-1020", "activity_name");
    press(&mut app, &ctx, pos, SHIFT, &mut time);
    frame(
        &mut app,
        &ctx,
        vec![egui::Event::Copy],
        &mut time,
        Default::default(),
    );
    let clip = app.editing.clipboard.clone().unwrap();
    assert_eq!(clip.activities.len(), 2);
    // The text is the selected cells of the visible columns under a header.
    let lines = ops::parse_tsv(&clip.text);
    assert_eq!(
        lines[0],
        [
            "Activity ID",
            "Activity Name",
            "Original Duration",
            "Activity Status"
        ]
    );
    assert_eq!(
        lines[1][..2],
        [
            "RB-1010".to_string(),
            "Site investigation & survey".to_string()
        ]
    );
    assert_eq!(lines.len(), 3);
    // Pasting that text pastes activities through Copy Activity Options.
    let undo_before = app.history.counts().0;
    frame(
        &mut app,
        &ctx,
        vec![egui::Event::Paste(clip.text.replace("\r\n", "\n"))],
        &mut time,
        Default::default(),
    );
    assert!(app.editing.dialogs.copy_options.is_some());
    click_action(&mut app, &ctx, "copy_options_ok", &mut time);
    assert_eq!(app.project.model.activities.len(), 16);
    assert_eq!(codes(&app, &app.selected_in_order()), ["A1000", "A1010"]);
    assert_eq!(app.history.counts().0, undo_before + 1, "one undo step");
    let pasted = app.project.model.activities[14].task_id;
    assert_eq!(rows_of(&app.project, "TASKRSRC", "task_id", pasted), 1);
    assert_eq!(app.project.assignment_rows(pasted).len(), 1);
    // Undo and redo take the whole paste.
    app.history_move(&ctx, false);
    assert_eq!(app.project.model.activities.len(), 14);
    assert_eq!(rows_of(&app.project, "TASKRSRC", "task_id", pasted), 0);
    app.history_move(&ctx, true);
    assert_eq!(app.project.model.activities.len(), 16);
    app.select_activities(&[14, 15]);
    // Cut asks nothing and changes nothing; Paste into the same WBS moves
    // nothing and records no step.
    let undo_before = app.history.counts().0;
    frame(
        &mut app,
        &ctx,
        vec![egui::Event::Cut],
        &mut time,
        Default::default(),
    );
    assert_eq!(app.project.model.activities.len(), 16);
    assert!(app.delete_confirm.is_none());
    assert!(app.editing.clipboard.as_ref().is_some_and(|c| c.cut));
    assert_eq!(app.status, "2 activities cut. Paste moves them.");
    app.editing.pending = Some(Command::Paste);
    idle(&mut app, &ctx, &mut time);
    assert!(app.editing.dialogs.copy_options.is_none());
    assert_eq!(app.project.model.activities.len(), 16);
    assert_eq!(app.history.counts().0, undo_before);
    assert!(app
        .project
        .model
        .activities
        .iter()
        .any(|a| a.task_code == "A1000"));
    assert_no_orphans(&app.project.updated_document());
    assert!(app.error.is_none(), "{:?}", app.error);
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn spreadsheet_text_pastes_into_editable_cells_and_reports_rejects() {
    let (path, ctx, mut app, mut time) = setup("text", starter());
    let pos = cell(&app, "RB-1010", "activity_name");
    press(&mut app, &ctx, pos, Default::default(), &mut time);
    let (a, b) = (
        index_of(&app.project, "RB-1010"),
        index_of(&app.project, "RB-1020"),
    );
    let od = crate::grid::columns::find(&columns::catalog(&app.project), "original_duration")
        .unwrap()
        .clone();
    let three = {
        let mut q = app.project.clone();
        q.details.get_mut(&1000).unwrap().original_duration = 3 * 480;
        let ix = app.grid_index();
        gc::input_text(&q, 0, &columns::value(&q, &ix, &od, 0))
    };
    let undo = app.history.counts().0;
    let text = format!("Pour slab\t{three}\tCompleted\r\nSet forms\tabc\r\n");
    frame(
        &mut app,
        &ctx,
        vec![egui::Event::Paste(text)],
        &mut time,
        Default::default(),
    );
    assert_eq!(app.project.name(a), "Pour slab");
    assert_eq!(app.project.name(b), "Set forms");
    assert_eq!(app.project.details[&1010].original_duration, 3 * 480);
    // Activity Status takes the pasted status with the Status tab's rules.
    let done = &app.project.model.activities[a];
    assert_eq!(done.status, StatusType::Completed);
    assert!(done.act_start.is_some() && done.act_finish.is_some());
    let s = app.editing.dialogs.summary.as_ref().unwrap();
    // RB-1010's Original Duration already was three days: an unchanged cell
    // does not count as a change.
    assert_eq!(s.applied, 3);
    let reasons: Vec<(&str, &str)> = s
        .rejected
        .iter()
        .map(|r| (r.activity.as_str(), r.column.as_str()))
        .collect();
    assert_eq!(reasons, [("RB-1020", "Original Duration")]);
    assert_eq!(app.history.counts().0, undo + 1);
    // Its predecessor is not finished: no question with the warning off
    // (the default).
    assert!(app.sched.sequence.is_none());
    click_action(&mut app, &ctx, "summary_ok", &mut time);
    assert!(app.editing.dialogs.summary.is_none());
    app.history_move(&ctx, false);
    assert_eq!(app.project.name(a), "Site investigation & survey");
    // A header line is skipped; a single value fills every selected row.
    app.paste_text("Activity Name\tOriginal Duration\nHeader skipped\n", time);
    assert_eq!(app.project.name(a), "Header skipped");
    let pos = cell(&app, "RB-1030", "activity_name");
    press(&mut app, &ctx, pos, SHIFT, &mut time);
    app.paste_text("Same name", time);
    assert!((a..=a + 2).all(|i| app.project.name(i) == "Same name"));
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn fill_down_copies_the_first_row_over_a_column_range() {
    let (path, ctx, mut app, mut time) = setup("fill", starter());
    let pos = cell(&app, "RB-1010", "activity_name");
    press(&mut app, &ctx, pos, Default::default(), &mut time);
    let pos = cell(&app, "RB-1030", "original_duration");
    press(&mut app, &ctx, pos, SHIFT, &mut time);
    assert_eq!(
        app.command_columns()
            .iter()
            .map(|c| c.id.as_str())
            .collect::<Vec<_>>(),
        ["activity_name", "original_duration"]
    );
    frame(
        &mut app,
        &ctx,
        vec![egui::Event::Key {
            key: Key::E,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: CTRL,
        }],
        &mut time,
        CTRL,
    );
    for id in [1020, 1030] {
        assert_eq!(app.project.details[&id].name, "Site investigation & survey");
        assert_eq!(app.project.details[&id].original_duration, 3 * 480);
    }
    assert!(app.editing.dialogs.summary.is_none());
    // Copy with a column range copies those cells alone, without a header.
    let rows = app.selected_in_order();
    assert_eq!(ops::parse_tsv(&app.selection_text(&rows))[0].len(), 2);
    assert_eq!(ops::parse_tsv(&app.selection_text(&rows)).len(), 3);
    app.history_move(&ctx, false);
    assert_eq!(app.project.details[&1020].name, "Complete detailed design");
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn dissolve_and_renumber_through_the_dialogs_with_undo() {
    let (path, ctx, mut app, mut time) = setup("dissolve", starter());
    let pos = cell(&app, "RB-1050", "activity_name");
    press(&mut app, &ctx, pos, Default::default(), &mut time);
    app.editing.pending = Some(Command::Dissolve);
    click_action(&mut app, &ctx, "dissolve_yes", &mut time);
    assert_eq!(app.project.model.activities.len(), 13);
    let (p, s) = (
        index_of(&app.project, "RB-1030"),
        index_of(&app.project, "RB-1080"),
    );
    assert!(app
        .project
        .model
        .links
        .iter()
        .any(|l| l.pred == p && l.succ == s));
    app.history_move(&ctx, false);
    assert_eq!(app.project.model.activities.len(), 14);
    // Renumber the first three rows by auto-number; RB-1030 is taken.
    let pos = cell(&app, "RB-1000", "activity_name");
    press(&mut app, &ctx, pos, Default::default(), &mut time);
    let pos = cell(&app, "RB-1020", "activity_name");
    press(&mut app, &ctx, pos, SHIFT, &mut time);
    app.editing.pending = Some(Command::Renumber);
    idle(&mut app, &ctx, &mut time);
    {
        let d = app.editing.dialogs.renumber.as_mut().unwrap();
        d.settings.mode = 1;
        d.settings.prefix = "RB-".into();
        d.settings.suffix = "1010".into();
        d.settings.auto_step = "10".into();
    }
    click_action(&mut app, &ctx, "renumber_ok", &mut time);
    // RB-1030 stays, so the third new ID needs the Duplicates step.
    let dup = app.editing.dialogs.duplicates.as_ref().unwrap();
    assert_eq!(dup.shown, [2]);
    app.editing.dialogs.duplicates.as_mut().unwrap().targets[2].1 = "RB-1005".into();
    click_action(&mut app, &ctx, "duplicates_ok", &mut time);
    assert_eq!(codes(&app, &[0, 1, 2]), ["RB-1010", "RB-1020", "RB-1005"]);
    app.history_move(&ctx, false);
    assert_eq!(codes(&app, &[0, 1, 2]), ["RB-1000", "RB-1010", "RB-1020"]);
    // The settings are kept for the next time.
    assert_eq!(app.editing.options.renumber.mode, 1);
    assert!(app.error.is_none(), "{:?}", app.error);
    drop(app);
    let _ = std::fs::remove_file(path);
}

fn sample_change() -> GlobalChange {
    GlobalChange {
        name: "Field work".into(),
        conditions: vec![Condition {
            column: "activity_name".into(),
            op: "contains".into(),
            value: "install".into(),
            ..Default::default()
        }],
        then: vec![
            Assignment {
                column: "activity_name".into(),
                value: "Activity Name".into(),
                operator: "&".into(),
                value2: "\" (field)\"".into(),
            },
            Assignment {
                column: "original_duration".into(),
                value: "Original Duration".into(),
                operator: "x".into(),
                value2: "2".into(),
            },
        ],
        otherwise: vec![Assignment {
            column: "activity_id".into(),
            value: "X-".into(),
            operator: "&".into(),
            value2: "Activity ID".into(),
        }],
    }
}

#[test]
fn global_change_if_then_else_previews_commits_checked_lines_and_undoes() {
    let (path, ctx, mut app, mut time) = setup("gc", starter());
    let g = sample_change();
    let before = app.project.clone();
    let changes = app.global_change_preview(&g);
    assert_eq!(app.project.model, before.model, "preview changes nothing");
    assert_eq!(app.project.details, before.details);
    // Then: 2 names and 2 durations; Else: 12 IDs.
    assert_eq!(changes.len(), 16);
    let install = index_of(&app.project, "RB-1080");
    let name = changes
        .iter()
        .find(|c| c.activity == "RB-1080" && c.column == "activity_name")
        .unwrap();
    assert_eq!(name.new, "Install structural assemblies (field)");
    let dur = changes
        .iter()
        .find(|c| c.activity == "RB-1080" && c.column == "original_duration")
        .unwrap();
    assert!(dur.error.is_none());
    let first = changes.iter().find(|c| c.activity == "RB-1000").unwrap();
    assert_eq!(
        (first.new.as_str(), &first.error),
        ("X-RB-1000", &None),
        "{changes:#?}"
    );
    // Through the dialog: open the report, clear one line, commit.
    app.editing.global_changes.push(g);
    app.save_global_changes();
    app.editing.pending = Some(Command::GlobalChange);
    click_action(&mut app, &ctx, "gc_row_0", &mut time);
    click_action(&mut app, &ctx, "gc_apply", &mut time);
    let report = app.editing.dialogs.gc_report.as_mut().unwrap();
    let k = report
        .changes
        .iter()
        .position(|c| c.activity == "RB-1000")
        .unwrap();
    click_action(&mut app, &ctx, &format!("gc_report_check_{k}"), &mut time);
    let undo = app.history.counts().0;
    click_action(&mut app, &ctx, "gc_commit", &mut time);
    assert_eq!(app.history.counts().0, undo + 1);
    assert_eq!(
        app.project.name(install),
        "Install structural assemblies (field)"
    );
    assert_eq!(
        app.project.details[&1080].original_duration,
        2 * before.details[&1080].original_duration
    );
    assert_eq!(
        app.project.model.activities[0].task_code, "RB-1000",
        "cleared line not applied"
    );
    assert_eq!(app.project.model.activities[1].task_code, "X-RB-1010");
    app.history_move(&ctx, false);
    assert_eq!(app.project.model, before.model);
    assert_eq!(app.project.details, before.details);
    app.history_move(&ctx, true);
    assert_eq!(app.project.model.activities[1].task_code, "X-RB-1010");
    // Definitions are kept in preferences and validated when read.
    let stored = app.store.preference(GLOBAL_CHANGES_KEY).unwrap().unwrap();
    assert_eq!(gc::from_json(&stored).unwrap(), vec![sample_change()]);
    assert!(gc::from_json(r#"[{"name":"","then":[]}]"#).is_err());
    assert!(
        gc::from_json(r#"[{"name":"x","then":[{"column":"activity_name","operator":"%"}]}]"#)
            .is_err()
    );
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn global_change_operators_and_errors() {
    let (path, _ctx, app, _time) = setup("gcops", starter());
    let p = &app.project;
    let catalog = columns::catalog(p);
    let ix = app.grid_index();
    let i = index_of(p, "RB-1080");
    let run = |column: &str, value: &str, operator: &str, value2: &str| {
        gc::compute(
            p,
            &ix,
            &catalog,
            &Assignment {
                column: column.into(),
                value: value.into(),
                operator: operator.into(),
                value2: value2.into(),
            },
            i,
        )
    };
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
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn menu_commands_run_after_the_panels() {
    let (path, ctx, mut app, mut time) = setup("menu", starter());
    let pos = cell(&app, "RB-1010", "activity_name");
    press(&mut app, &ctx, pos, Default::default(), &mut time);
    for command in [Command::SelectAll, Command::Copy] {
        app.editing.pending = Some(command);
        idle(&mut app, &ctx, &mut time);
    }
    assert!(app.editing.pending.is_none());
    assert_eq!(app.editing.clipboard.as_ref().unwrap().activities.len(), 14);
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn delete_uses_the_multi_row_selection() {
    let (path, ctx, mut app, mut time) = setup("delete", rich());
    let pos = cell(&app, "RB-1010", "activity_name");
    press(&mut app, &ctx, pos, Default::default(), &mut time);
    let pos = cell(&app, "RB-1030", "activity_name");
    press(&mut app, &ctx, pos, SHIFT, &mut time);
    app.request_delete();
    assert_eq!(app.delete_confirm, Some(vec![1010, 1020, 1030]));
    app.delete_activities(&[1010, 1020, 1030], time);
    assert_eq!(app.project.model.activities.len(), 11);
    assert_no_orphans(&app.project.updated_document());
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn render_editing_dialogs_to_png() {
    use crate::headless_render::SoftwarePainter;
    let artifacts = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts");
    std::fs::create_dir_all(&artifacts).unwrap();
    let (path, ctx, mut app, mut time) = setup_with("render", starter(), false);
    app.editing.global_changes = vec![
        sample_change(),
        GlobalChange {
            name: "Increase Durations".into(),
            ..Default::default()
        },
    ];
    app.selected = Some(0);
    let rows: Vec<usize> = (0..4).collect();
    app.select_activities(&rows);
    let mut painter = SoftwarePainter::default();
    type Shot = (&'static str, fn(&mut App));
    let shots: [Shot; 7] = [
        ("headless-copy-activity-options.png", |a| {
            a.editing.dialogs.copy_options = Some(CopyOptions::default())
        }),
        ("headless-renumber.png", |a| a.open_renumber()),
        ("headless-renumber-duplicates.png", |a| {
            a.editing.dialogs.duplicates = Some(dialogs::DuplicatesDraft {
                targets: vec![(0, "RB-1010".into()), (1, "RB-1020".into())],
                shown: vec![0, 1],
                paste: None,
            })
        }),
        ("headless-global-change.png", |a| a.open_global_change()),
        ("headless-global-change-modify.png", |a| {
            a.editing.dialogs.gc_edit = Some(dialogs::GcEdit {
                original: Some("Field work".into()),
                gc: sample_change(),
                selected: (1, Some(0)),
                confirm_delete: false,
            })
        }),
        ("headless-global-change-report.png", |a| {
            let changes = a.global_change_preview(&sample_change());
            a.editing.dialogs.gc_report = Some(a.gc_report_for("Field work", changes));
        }),
        ("headless-paste-summary.png", |a| {
            a.editing.dialogs.summary = Some(dialogs::Summary {
                title: "Paste".into(),
                applied: 3,
                rejected: vec![Rejected {
                    activity: "RB-1020".into(),
                    column: "Original Duration".into(),
                    value: "abc".into(),
                    reason: "\"abc\" is not a duration.".into(),
                }],
            })
        }),
    ];
    for (name, open) in shots {
        app.editing.dialogs = Default::default();
        open(&mut app);
        for _ in 0..2 {
            let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
        }
        let (image, _) = painter
            .frame(&mut app, &ctx, &mut time, vec![], true)
            .unwrap();
        image.save(artifacts.join(name)).unwrap();
        println!(
            "Software-rendered editing dialog: {}",
            artifacts.join(name).display()
        );
    }
    assert!(app.error.is_none(), "{:?}", app.error);
    drop(app);
    let _ = std::fs::remove_file(path);
}

/// Milestones carry no assignments: a copied milestone that holds one (an
/// imported file) pastes without it, the copy of a task keeps its own, and
/// the Project Checker lists the original as "Milestone with resource
/// assignments".
#[test]
fn milestone_assignments_are_not_pasted_and_the_checker_flags_them() {
    let mut p = rich();
    let mile = (0..p.model.activities.len())
        .find(|&i| openxer_xer::assignments::is_milestone(p.model.activities[i].task_type))
        .expect("a milestone");
    let task = index_of(&p, "RB-1010");
    let (mile_id, pid) = (p.model.activities[mile].task_id, p.model.project.proj_id);
    let rsrc = records::get(&p.document, "TASKRSRC", 0, "rsrc_id");
    p.document.table_mut("TASKRSRC").add(&[
        ("taskrsrc_id", "990".into()),
        ("task_id", mile_id.to_string()),
        ("proj_id", pid.to_string()),
        ("rsrc_id", rsrc),
    ]);
    let flagged = crate::reports::checker::run(&p, &crate::reports::checker::Config::default())
        .into_iter()
        .find(|r| r.rule == crate::reports::checker::Rule::MilestoneWithAssignments)
        .unwrap();
    let code = p.model.activities[mile].task_code.clone();
    assert_eq!(flagged.findings.len(), 1, "{flagged:?}");
    assert_eq!(flagged.findings[0].code, code);
    // A material assignment counts too (the rule is any resource or role
    // assignment); an expense is not an assignment and comes along.
    p.document.table_mut("RSRC").add(&[
        ("rsrc_id", "8".into()),
        ("rsrc_name", "Rebar".into()),
        ("rsrc_type", "RT_Mat".into()),
    ]);
    p.document.table_mut("TASKRSRC").add(&[
        ("taskrsrc_id", "991".into()),
        ("task_id", mile_id.to_string()),
        ("proj_id", pid.to_string()),
        ("rsrc_id", "8".into()),
    ]);
    records::add_row(
        &mut p.document,
        "PROJCOST",
        &[
            ("cost_item_id", "77".into()),
            ("task_id", mile_id.to_string()),
            ("proj_id", pid.to_string()),
            ("cost_name", "Permit fee".into()),
        ],
    );
    let clip = ops::copy(&p, &[mile, task]);
    let pasted = ops::paste_activities(&mut p, &clip, &CopyOptions::default(), 30, None).unwrap();
    let rows = &pasted.rows;
    let count = |p: &ProjectData, i: usize| {
        records::rows_for(&p.document, "TASKRSRC", p.model.activities[i].task_id, pid).len()
    };
    assert_eq!(
        count(&p, rows[0]),
        0,
        "the pasted milestone has no assignment"
    );
    assert!(
        count(&p, rows[1]) > 0,
        "the pasted task keeps its assignment"
    );
    assert_eq!(count(&p, mile), 2, "the original is untouched");
    let new_mile = p.model.activities[rows[0]].task_id;
    assert_eq!(rows_of(&p, "PROJCOST", "task_id", new_mile), 1, "expense");
    // What was left out is reported, never silent.
    let pasted_code = p.model.activities[rows[0]].task_code.clone();
    assert_eq!(pasted.milestone_assignments, [(pasted_code.clone(), 2)]);
    let note = pasted.note().unwrap();
    assert!(
        note.starts_with("2 resource assignments were not pasted because")
            && note.contains(openxer_xer::assignments::MILESTONE_RULE)
            && note.contains(&pasted_code),
        "{note}"
    );
    // A milestone that had no assignments pastes with nothing to report.
    let other_mile = (0..p.model.activities.len())
        .find(|&i| {
            i != mile
                && openxer_xer::assignments::is_milestone(p.model.activities[i].task_type)
                && count(&p, i) == 0
        })
        .expect("a second milestone");
    let clip = ops::copy(&p, &[other_mile]);
    let quiet = ops::paste_activities(&mut p, &clip, &CopyOptions::default(), 30, None).unwrap();
    assert_eq!(quiet.rows.len(), 1);
    assert!(quiet.milestone_assignments.is_empty());
    assert_eq!(quiet.note(), None);
    // Assigning to the milestone through the shared setter is refused.
    let e = p
        .add_assignment(
            mile_id,
            Some(records::num(&records::get(
                &p.document,
                "TASKRSRC",
                0,
                "rsrc_id",
            ))),
            None,
        )
        .unwrap_err();
    assert!(e.contains(openxer_xer::assignments::MILESTONE_RULE), "{e}");
}

/// The Paste command's status message says which assignment rows of a
/// copied milestone were not pasted.
#[test]
fn pasting_a_milestone_with_assignments_says_what_was_left_out() {
    let mut p = rich();
    let mile = (0..p.model.activities.len())
        .find(|&i| openxer_xer::assignments::is_milestone(p.model.activities[i].task_type))
        .expect("a milestone");
    let (mile_id, pid) = (p.model.activities[mile].task_id, p.model.project.proj_id);
    p.document.table_mut("TASKRSRC").add(&[
        ("taskrsrc_id", "990".into()),
        ("task_id", mile_id.to_string()),
        ("proj_id", pid.to_string()),
        ("rsrc_id", "5".into()),
    ]);
    let clip = ops::copy(&p, &[mile]);
    let (path, _ctx, mut app, time) = setup_with("mile-status", p, false);
    app.editing.clipboard = Some(clip);
    app.editing.options.copy.show_dialog = false;
    app.paste_activities(time);
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(
        app.status
            .starts_with("1 activity pasted. 1 resource assignment was not pasted because")
            && app
                .status
                .contains(openxer_xer::assignments::MILESTONE_RULE),
        "{}",
        app.status
    );
    drop(app);
    let _ = std::fs::remove_file(path);
}

/// Pasting into a project that opened without calendars gives it the
/// standard calendar first, as Add does: every pasted activity points at a
/// calendar that exists, and the file saves and reopens.
#[test]
fn paste_into_a_project_without_calendars_saves_and_reopens() {
    let source = rich();
    let a = index_of(&source, "RB-1010");
    let clip = ops::copy(&source, &[a, a + 1]);
    let text = "ERMHDR\t25.12\r\n%T\tPROJECT\r\n%F\tproj_id\tproj_short_name\tclndr_id\tplan_start_date\r\n%R\t5\tEMPTY\t\t2026-01-05\r\n%T\tPROJWBS\r\n%F\twbs_id\tproj_id\tseq_num\tproj_node_flag\twbs_short_name\twbs_name\tparent_wbs_id\r\n%R\t50\t5\t0\tY\tEMPTY\tEmpty\t\r\n%E\r\n";
    let mut p = ProjectData::from_document(
        openxer_xer::Document::parse(text).unwrap(),
        5,
        "empty".into(),
    )
    .unwrap();
    assert!(p.model.calendars.is_empty());
    let rows = ops::paste(&mut p, &clip, &CopyOptions::default(), 50, None).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(
        p.calendar_names,
        [openxer_xer::STANDARD_CALENDAR_NAME.to_string()]
    );
    assert!(rows.iter().all(|&i| p.model.activities[i].cal == 0));
    let saved = p.updated_document();
    let cal_ids: BTreeSet<String> = saved
        .table("CALENDAR")
        .unwrap()
        .rows
        .iter()
        .map(|r| {
            saved
                .table("CALENDAR")
                .unwrap()
                .value(r, "clndr_id")
                .to_string()
        })
        .collect();
    let tasks = saved.table("TASK").unwrap();
    for r in &tasks.rows {
        assert!(cal_ids.contains(tasks.value(r, "clndr_id")), "{r:?}");
    }
    let project = saved.table("PROJECT").unwrap();
    assert!(cal_ids.contains(project.value(&project.rows[0], "clndr_id")));
    let again = ProjectData::from_document(
        openxer_xer::Document::parse(&saved.text()).unwrap(),
        5,
        "again".into(),
    )
    .unwrap();
    assert_eq!(again.model.activities.len(), 2);
    assert_eq!(again.model.calendars.len(), 1);
    let mut model = again.model.clone();
    schedule_model(&mut model, &again.resources).unwrap();
    assert!(model.activities.iter().all(|a| a.early_start.is_some()));
}
