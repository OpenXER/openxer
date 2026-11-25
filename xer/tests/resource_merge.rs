//! Deleting and merging resources through the shared edit layer
//! (`openxer_xer::edit::resources`), on the synthetic starter project.
use openxer_xer::edit::records::{self, num};
use openxer_xer::edit::resources;
use openxer_xer::{starter, Document, ProjectData};

const PIPE: i64 = 70;
const PIPE2: i64 = 71;
const PIPE3: i64 = 72;
const HELPER: i64 = 73;

fn rows(doc: &Document, table: &str, column: &str, value: i64) -> Vec<usize> {
    doc.table(table).map_or(vec![], |t| {
        t.rows
            .iter()
            .enumerate()
            .filter(|(_, r)| num(t.value(r, column)) == value)
            .map(|(i, _)| i)
            .collect()
    })
}

fn get(doc: &Document, table: &str, row: usize, column: &str) -> String {
    records::get(doc, table, row, column)
}

/// Three copies of one pipe crew (as imports from three sources leave
/// them), each with rates, a role, a code, a field value and assignments.
fn duplicates() -> ProjectData {
    let mut p = starter();
    let d = &mut p.document;
    for (id, code, cal) in [
        (PIPE, "PIPE", ""),
        (PIPE2, "PIPE.1", "1"),
        (PIPE3, "pipe", "1"),
    ] {
        d.table_mut("RSRC").add(&[
            ("rsrc_id", id.to_string()),
            ("rsrc_short_name", code.into()),
            ("rsrc_name", "Pipe crew".into()),
            ("rsrc_type", "RT_Labor".into()),
            ("clndr_id", cal.into()),
        ]);
    }
    d.table_mut("RSRC").add(&[
        ("rsrc_id", HELPER.to_string()),
        ("rsrc_short_name", "PIPE.1.H".into()),
        ("rsrc_name", "Pipe helper".into()),
        ("rsrc_type", "RT_Labor".into()),
        ("parent_rsrc_id", PIPE2.to_string()),
    ]);
    for (n, (id, date, price)) in [
        (PIPE, "2026-01-01 00:00", "80"),
        (PIPE2, "2026-01-01 00:00", "85"),
        (PIPE2, "2026-06-01 00:00", "90"),
    ]
    .into_iter()
    .enumerate()
    {
        d.table_mut("RSRCRATE").add(&[
            ("rsrc_rate_id", (n + 1).to_string()),
            ("rsrc_id", id.to_string()),
            ("start_date", date.into()),
            ("max_qty_per_hr", "1".into()),
            ("cost_per_qty", price.into()),
        ]);
    }
    for (id, role) in [(PIPE, "5"), (PIPE2, "5"), (PIPE2, "6")] {
        d.table_mut("RSRCROLE").add(&[
            ("rsrc_id", id.to_string()),
            ("role_id", role.into()),
            ("skill_level", "3".into()),
        ]);
    }
    d.table_mut("RSRCRCAT").add(&[
        ("rsrc_id", PIPE2.to_string()),
        ("rsrc_catg_type_id", "9".into()),
        ("rsrc_catg_id", "91".into()),
    ]);
    d.table_mut("UDFTYPE").add(&[
        ("udf_type_id", "300".into()),
        ("table_name", "RSRC".into()),
        ("udf_type_label", "Crew size".into()),
    ]);
    d.table_mut("UDFTYPE").add(&[
        ("udf_type_id", "301".into()),
        ("table_name", "TASKRSRC".into()),
        ("udf_type_label", "Foreman".into()),
    ]);
    for (id, v) in [(PIPE, "4"), (PIPE2, "6"), (PIPE3, "5")] {
        d.table_mut("UDFVALUE").add(&[
            ("udf_type_id", "300".into()),
            ("fk_id", id.to_string()),
            ("proj_id", String::new()),
            ("udf_number", v.into()),
        ]);
    }
    // 1020: PIPE (actual 10 of 40, started) and PIPE.1 (budget 24, not
    // started). 1030: PIPE.1 only. 1040: pipe (finished, 16 h).
    let assignment = |d: &mut Document, id: i64, task: i64, rsrc: i64, v: [&str; 8]| {
        d.table_mut("TASKRSRC").add(&[
            ("taskrsrc_id", id.to_string()),
            ("task_id", task.to_string()),
            ("proj_id", "1".into()),
            ("rsrc_id", rsrc.to_string()),
            ("rsrc_type", "RT_Labor".into()),
            ("role_id", v[0].into()),
            ("target_qty", v[1].into()),
            ("act_reg_qty", v[2].into()),
            ("remain_qty", v[3].into()),
            ("target_cost", v[4].into()),
            ("act_start_date", v[5].into()),
            ("act_end_date", v[6].into()),
            ("target_qty_per_hr", v[7].into()),
            ("remain_qty_per_hr", v[7].into()),
            ("target_start_date", "2026-02-02 08:00".into()),
            ("target_end_date", "2026-02-06 17:00".into()),
        ]);
    };
    assignment(
        d,
        500,
        1020,
        PIPE,
        ["5", "40", "10", "30", "3200", "2026-02-02 08:00", "", "1"],
    );
    assignment(
        d,
        501,
        1020,
        PIPE2,
        ["6", "24", "0", "24", "2040", "", "", "0.6"],
    );
    assignment(
        d,
        502,
        1030,
        PIPE2,
        ["", "16", "0", "16", "1360", "", "", "1"],
    );
    assignment(
        d,
        503,
        1040,
        PIPE3,
        [
            "",
            "16",
            "16",
            "0",
            "1280",
            "2026-01-05 08:00",
            "2026-01-06 17:00",
            "1",
        ],
    );
    // Assignment details of the PIPE.1 row on 1020.
    d.table_mut("TRSRCFIN").add(&[
        ("fin_dates_id", "1".into()),
        ("taskrsrc_id", "500".into()),
        ("act_reg_qty", "10".into()),
        ("act_reg_cost", "800".into()),
    ]);
    d.table_mut("TRSRCFIN").add(&[
        ("fin_dates_id", "1".into()),
        ("taskrsrc_id", "501".into()),
        ("act_reg_qty", "2".into()),
        ("act_reg_cost", "170".into()),
    ]);
    d.table_mut("UDFVALUE").add(&[
        ("udf_type_id", "301".into()),
        ("fk_id", "501".into()),
        ("proj_id", "1".into()),
        ("udf_text", "Ruiz".into()),
    ]);
    // PIPE.1 is primary on 1030.
    let tasks = d.table_mut("TASK");
    let r = tasks
        .rows
        .iter()
        .position(|r| tasks.value(r, "task_id") == "1030")
        .unwrap();
    tasks.set(r, "rsrc_id", PIPE2.to_string());
    d.table_mut("RSRCLEVELLIST").add(&[
        ("rsrc_level_list_id", "1".into()),
        ("schedoptions_id", "1".into()),
        ("rsrc_id", PIPE.to_string()),
    ]);
    d.table_mut("RSRCLEVELLIST").add(&[
        ("rsrc_level_list_id", "2".into()),
        ("schedoptions_id", "1".into()),
        ("rsrc_id", PIPE2.to_string()),
    ]);
    let doc = p.document.clone();
    ProjectData::from_document(doc, 1, "merge-test".into()).unwrap()
}

#[test]
fn merge_moves_everything_and_combines_assignments_on_one_activity() {
    let mut p = duplicates();
    let preview = resources::preview(&p.document, &[PIPE2, PIPE3], PIPE).unwrap();
    let untouched = p.document.clone();
    let report = resources::merge_project(&mut p, &[PIPE2, PIPE3], PIPE).unwrap();
    assert_eq!(preview, report, "the preview is what the merge does");
    assert_ne!(untouched.text(), p.document.text());
    let d = &p.document;

    // Sources gone, target stays.
    assert!(resources::row(d, PIPE2).is_none() && resources::row(d, PIPE3).is_none());
    assert!(resources::row(d, PIPE).is_some());
    assert_eq!(
        report.assignments,
        vec![("PIPE.1".to_string(), 2), ("pipe".to_string(), 1)]
    );

    // 1020: one assignment, the target's own row 500, units summed.
    let on_1020: Vec<usize> = rows(d, "TASKRSRC", "task_id", 1020);
    assert_eq!(on_1020.len(), 1);
    let r = on_1020[0];
    assert_eq!(get(d, "TASKRSRC", r, "taskrsrc_id"), "500");
    assert_eq!(get(d, "TASKRSRC", r, "rsrc_id"), PIPE.to_string());
    assert_eq!(get(d, "TASKRSRC", r, "target_qty"), "64");
    assert_eq!(get(d, "TASKRSRC", r, "act_reg_qty"), "10");
    assert_eq!(get(d, "TASKRSRC", r, "remain_qty"), "54");
    assert_eq!(get(d, "TASKRSRC", r, "target_cost"), "5240");
    assert_eq!(get(d, "TASKRSRC", r, "target_qty_per_hr"), "1.6");
    assert_eq!(get(d, "TASKRSRC", r, "role_id"), "5", "kept row's role");
    // Started and open: the actual start stays, no actual finish.
    assert_eq!(get(d, "TASKRSRC", r, "act_start_date"), "2026-02-02 08:00");
    assert_eq!(get(d, "TASKRSRC", r, "act_end_date"), "");
    assert_eq!(report.combined.len(), 1);
    assert_eq!(report.combined[0].activity, "RB-1020");
    assert_eq!(report.combined[0].rows, 2);

    // Period rows summed per period on the kept row; the field value moved.
    let fin = rows(d, "TRSRCFIN", "taskrsrc_id", 500);
    assert_eq!(fin.len(), 1);
    assert_eq!(get(d, "TRSRCFIN", fin[0], "act_reg_qty"), "12");
    assert_eq!(get(d, "TRSRCFIN", fin[0], "act_reg_cost"), "970");
    assert!(rows(d, "TRSRCFIN", "taskrsrc_id", 501).is_empty());
    let foreman: Vec<usize> = rows(d, "UDFVALUE", "udf_type_id", 301);
    assert_eq!(get(d, "UDFVALUE", foreman[0], "fk_id"), "500");

    // 1030 and 1040 re-pointed, finished actuals untouched.
    for task in [1030, 1040] {
        let r = rows(d, "TASKRSRC", "task_id", task)[0];
        assert_eq!(get(d, "TASKRSRC", r, "rsrc_id"), PIPE.to_string());
    }
    let r = rows(d, "TASKRSRC", "task_id", 1040)[0];
    assert_eq!(get(d, "TASKRSRC", r, "act_end_date"), "2026-01-06 17:00");
    assert_eq!(get(d, "TASKRSRC", r, "act_reg_qty"), "16");
    assert_eq!(report.primary, 1);
    let task = rows(d, "TASK", "task_id", 1030)[0];
    assert_eq!(get(d, "TASK", task, "rsrc_id"), PIPE.to_string());

    // Rates: PIPE keeps its own 2026-01-01 row; PIPE.1's June row moves.
    let rates = rows(d, "RSRCRATE", "rsrc_id", PIPE);
    let prices: Vec<String> = rates
        .iter()
        .map(|&r| get(d, "RSRCRATE", r, "cost_per_qty"))
        .collect();
    assert_eq!(prices, ["80", "90"]);
    assert_eq!((report.rates_moved, report.rates_kept), (1, 1));
    // Roles: role 5 once, role 6 moved.
    let roles: Vec<String> = rows(d, "RSRCROLE", "rsrc_id", PIPE)
        .iter()
        .map(|&r| get(d, "RSRCROLE", r, "role_id"))
        .collect();
    assert_eq!(roles, ["5", "6"]);
    assert_eq!((report.roles_moved, report.roles_kept), (1, 1));
    // Codes: PIPE had none, takes PIPE.1's.
    assert_eq!(rows(d, "RSRCRCAT", "rsrc_id", PIPE).len(), 1);
    assert_eq!(report.codes_moved, 1);
    // Field values: the target's own 4 wins over 6 and 5.
    let crew: Vec<usize> = rows(d, "UDFVALUE", "udf_type_id", 300);
    assert_eq!(crew.len(), 1);
    assert_eq!(get(d, "UDFVALUE", crew[0], "udf_number"), "4");
    assert_eq!((report.udfs_moved, report.udfs_kept), (0, 2));
    // The child moved under PIPE; PIPE took a calendar.
    let helper = resources::row(d, HELPER).unwrap();
    assert_eq!(get(d, "RSRC", helper, "parent_rsrc_id"), PIPE.to_string());
    assert_eq!(report.children, 1);
    assert!(report.calendar.is_some());
    // The leveling list names PIPE once.
    assert_eq!(rows(d, "RSRCLEVELLIST", "rsrc_id", PIPE).len(), 1);
    assert_eq!(rows(d, "RSRCLEVELLIST", "rsrc_id", PIPE2).len(), 0);

    // The project model sees the merged assignments.
    let i = p
        .model
        .activities
        .iter()
        .position(|a| a.task_id == 1020)
        .unwrap();
    assert_eq!(p.model.activities[i].assignments.len(), 1);
    assert!(report.lines().iter().any(|l| l.contains("RB-1020")));
    assert!(report
        .lines()
        .iter()
        .all(|l| !l.contains('\u{2013}') && !l.contains('\u{2014}')));
}

#[test]
fn merged_project_survives_xer_round_trip() {
    let mut p = duplicates();
    resources::merge_project(&mut p, &[PIPE2, PIPE3], PIPE).unwrap();
    let text = p.updated_document().text();
    let back = ProjectData::from_document(Document::parse(&text).unwrap(), 1, "rt".into()).unwrap();
    let d = &back.document;
    assert!(resources::row(d, PIPE2).is_none());
    assert_eq!(rows(d, "TASKRSRC", "rsrc_id", PIPE).len(), 3);
    assert_eq!(rows(d, "TASKRSRC", "task_id", 1020).len(), 1);
    assert_eq!(back.updated_document().text(), text);
}

#[test]
fn merge_refuses_bad_requests_and_leaves_the_file_alone() {
    let p = duplicates();
    let mut d = p.document.clone();
    assert!(resources::merge(&mut d, &[], PIPE).is_err());
    assert!(resources::merge(&mut d, &[PIPE], PIPE).is_err());
    assert!(resources::merge(&mut d, &[PIPE2, PIPE2], PIPE).is_err());
    assert!(resources::merge(&mut d, &[999], PIPE).is_err());
    // Another type.
    let r = resources::row(&d, PIPE3).unwrap();
    records::set(&mut d, "RSRC", r, "rsrc_type", "RT_Equip".into());
    let before = d.text();
    let e = resources::merge(&mut d, &[PIPE3], PIPE).unwrap_err();
    assert!(e.contains("different resource types"), "{e}");
    assert_eq!(d.text(), before);
}

#[test]
fn a_target_under_a_source_takes_its_place() {
    let mut p = duplicates();
    // Merge PIPE.1 into its own child.
    let report = resources::merge_project(&mut p, &[PIPE2], HELPER).unwrap();
    let d = &p.document;
    let helper = resources::row(d, HELPER).unwrap();
    assert_eq!(get(d, "RSRC", helper, "parent_rsrc_id"), "");
    assert_eq!(report.children, 0);
    assert_eq!(rows(d, "TASKRSRC", "rsrc_id", HELPER).len(), 2);
}

#[test]
fn delete_refuses_assigned_resources_and_cleans_an_unused_one() {
    let mut p = duplicates();
    let e = resources::delete(&mut p.document, PIPE2).unwrap_err();
    assert!(e.contains("Del. / Merge"), "{e}");
    let e = resources::delete_by_code(&mut p, "PIPE.1").unwrap_err();
    assert!(e.contains("assignment"), "{e}");
    // Merge the assignments away, then the child is the only thing left.
    resources::merge_project(&mut p, &[PIPE3], PIPE).unwrap();
    resources::delete_by_code(&mut p, "PIPE.1.H").unwrap();
    assert!(resources::row(&p.document, HELPER).is_none());
    assert!(resources::find(&p.document, "nobody").is_err());
    assert_eq!(resources::find(&p.document, "pipe.1").unwrap(), PIPE2);
}
