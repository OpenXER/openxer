use openxer_xer::{starter, ProjectData};
#[test]
fn imported_resource_capacity_controls_native_leveling() {
    let mut doc = starter().document;
    doc.table_mut("RSRC").add(&[
        ("rsrc_id", "10".into()),
        ("rsrc_name", "Fabrication crew".into()),
        ("rsrc_type", "RT_Labor".into()),
        ("clndr_id", "1".into()),
    ]);
    doc.table_mut("RSRCRATE").add(&[
        ("rsrc_id", "10".into()),
        ("start_date", "2020-01-01 00:00".into()),
        ("max_qty_per_hr", "1".into()),
    ]);
    for (id, qty) in [(1040, 64), (1050, 48)] {
        doc.table_mut("TASKRSRC").add(&[
            ("task_id", id.to_string()),
            ("proj_id", "1".into()),
            ("rsrc_id", "10".into()),
            ("remain_qty", qty.to_string()),
            ("remain_qty_per_hr", "1".into()),
            ("rollup_dates_flag", "N".into()),
        ]);
    }
    let mut project = ProjectData::from_document(doc, 1, "leveling-test".into()).unwrap();
    project.model.options.do_level = true;
    openxer_engine::scheduler::schedule_with_resources(&mut project.model, &project.resources)
        .unwrap();
    let a = &project.model.activities[4];
    let b = &project.model.activities[5];
    assert!(
        a.reend.unwrap() <= b.restart.unwrap() || b.reend.unwrap() <= a.restart.unwrap(),
        "Shared crew cannot work on overlapping activities"
    );
    assert!(
        a.early_start.is_some() && b.early_start.is_some(),
        "Leveling preserves the schedule dates"
    );
}

#[test]
fn resource_dependent_assignments_carry_their_resource_calendar() {
    // A Resource Dependent activity works on its resources' calendars (saved
    // reference probes 814, 816), so an assignment with units on one names it; an
    // ordinary task's assignment does not.
    let mut doc = starter().document;
    doc.table_mut("RSRC").add(&[
        ("rsrc_id", "10".into()),
        ("rsrc_name", "Crew".into()),
        ("rsrc_type", "RT_Labor".into()),
        ("clndr_id", "1".into()),
    ]);
    let tasks = doc.table_mut("TASK");
    let row = tasks
        .rows
        .iter()
        .position(|r| tasks.value(r, "task_id") == "1040")
        .unwrap();
    tasks.set(row, "task_type", "TT_Rsrc".into());
    for (id, rate) in [(1040, "1"), (1050, "1"), (1040, "0")] {
        doc.table_mut("TASKRSRC").add(&[
            ("task_id", id.to_string()),
            ("proj_id", "1".into()),
            ("rsrc_id", "10".into()),
            ("remain_qty", "16".into()),
            ("remain_qty_per_hr", rate.into()),
            ("rollup_dates_flag", "N".into()),
        ]);
    }
    let project = ProjectData::from_document(doc, 1, "rd-test".into()).unwrap();
    let cal = project
        .model
        .calendars
        .iter()
        .position(|c| c.id == "1")
        .unwrap();
    let find = |code: i64| {
        project
            .model
            .activities
            .iter()
            .find(|a| a.task_id == code)
            .unwrap()
    };
    let rd = find(1040);
    assert_eq!(
        rd.assignments.iter().map(|a| a.cal).collect::<Vec<_>>(),
        vec![Some(cal), None]
    );
    assert!(find(1050).assignments.iter().all(|a| a.cal.is_none()));
}

/// Table names are matched without regard to case everywhere else, so a
/// file written with lower-case table names gets the same cleanups.
#[test]
fn deleting_a_resource_cleans_lower_case_tables_too() {
    let text = "ERMHDR\t25.12\r\n\
        %T\trsrc\r\n%F\trsrc_id\trsrc_short_name\r\n%R\t5\tPIPE\r\n%R\t6\tWELD\r\n\
        %T\trsrcrate\r\n%F\trsrc_rate_id\trsrc_id\tcost_per_qty\r\n%R\t1\t5\t85\r\n%R\t2\t6\t90\r\n\
        %T\ttask\r\n%F\ttask_id\tproj_id\trsrc_id\r\n%R\t100\t1\t5\r\n\
        %T\tudftype\r\n%F\tudf_type_id\ttable_name\r\n%R\t3\tRSRC\r\n\
        %T\tudfvalue\r\n%F\tudf_type_id\tfk_id\tudf_text\r\n%R\t3\t5\tx\r\n%R\t3\t6\ty\r\n\
        %E\r\n";
    let mut doc = openxer_xer::Document::parse(text).unwrap();
    openxer_xer::edit::resources::delete(&mut doc, 5).unwrap();
    let rows = |name: &str| doc.table(name).unwrap().rows.clone();
    assert_eq!(rows("RSRC").len(), 1);
    assert_eq!(
        rows("RSRCRATE").len(),
        1,
        "the deleted resource's rate goes"
    );
    assert_eq!(rows("UDFVALUE").len(), 1, "its field value goes");
    let task = doc.table("TASK").unwrap();
    assert_eq!(
        task.value(&task.rows[0], "rsrc_id"),
        "",
        "the primary resource is cleared"
    );
}
