//! A synthetic two-project workspace sample (no client data) for the
//! rollup tests, the command line tests and the Projects view renders: the
//! expected rollups are worked out by hand in `tests.rs`.
use crate::{standard_calendar_data, Document};

fn empty() -> Document {
    Document {
        header: "ERMHDR\t25.12\t2027-01-01\tProject\tOpenXER\tOpenXER\tdbxDatabaseNoName\tProject Management\tUSD".into(),
        tables: vec![],
        windows_encoding: false,
        warnings: vec![],
        lone_quotes: Default::default(),
        id_floor: Default::default(),
    }
}

/// A project file with the standard calendar (Monday to Friday, 8 hours).
pub fn project(pid: i64, code: &str, data_date: &str, must_finish: &str) -> Document {
    let mut doc = empty();
    doc.table_mut("PROJECT").add(&[
        ("proj_id", pid.to_string()),
        ("proj_short_name", code.into()),
        ("clndr_id", "1".into()),
        ("last_recalc_date", data_date.into()),
        ("plan_start_date", "2027-01-04 08:00".into()),
        ("plan_end_date", must_finish.into()),
    ]);
    doc.table_mut("CALENDAR").add(&[
        ("clndr_id", "1".into()),
        ("clndr_name", crate::STANDARD_CALENDAR_NAME.into()),
        ("day_hr_cnt", "8".into()),
        ("clndr_data", standard_calendar_data()),
    ]);
    doc
}

pub fn task(doc: &mut Document, pid: i64, id: i64, values: &[(&str, &str)]) {
    let mut v: Vec<(&str, String)> =
        vec![("task_id", id.to_string()), ("proj_id", pid.to_string())];
    v.extend(values.iter().map(|(k, x)| (*k, x.to_string())));
    doc.table_mut("TASK").add(&v);
}

/// Project A, data date Wed 13-Jan-27 08:00:
/// - A1 completed Mon 4-Jan 08:00 to Fri 8-Jan 17:00;
/// - A2 in progress since Mon 11-Jan 08:00, remaining Wed 13-Jan 08:00 to
///   Fri 15-Jan 17:00, total float 16 h;
/// - A3 not started Mon 18-Jan 08:00 to Fri 22-Jan 17:00, total float 8 h,
///   late Tue 19-Jan 08:00 to Mon 25-Jan 17:00.
///
/// Labor resource 1 on A1 (40 h budgeted, 40 actual, 0 remaining; $4,000
/// budgeted and actual), material resource 2 on A2 ($500 budgeted, $100
/// actual, $400 remaining; its 10 units are not labor) and a $1,000 expense
/// not yet spent.
pub fn project_a() -> Document {
    let mut doc = project(1, "ALPHA", "2027-01-13 08:00", "2027-01-29 17:00");
    node(&mut doc, 1, "Alpha Unit Revamp", "WS_Open");
    task(
        &mut doc,
        1,
        10,
        &[
            ("status_code", "TK_Complete"),
            ("act_start_date", "2027-01-04 08:00"),
            ("act_end_date", "2027-01-08 17:00"),
        ],
    );
    task(
        &mut doc,
        1,
        20,
        &[
            ("status_code", "TK_Active"),
            ("act_start_date", "2027-01-11 08:00"),
            ("restart_date", "2027-01-13 08:00"),
            ("reend_date", "2027-01-15 17:00"),
            ("early_start_date", "2027-01-13 08:00"),
            ("early_end_date", "2027-01-15 17:00"),
            ("total_float_hr_cnt", "16"),
        ],
    );
    task(
        &mut doc,
        1,
        30,
        &[
            ("status_code", "TK_NotStart"),
            ("early_start_date", "2027-01-18 08:00"),
            ("early_end_date", "2027-01-22 17:00"),
            ("late_start_date", "2027-01-19 08:00"),
            ("late_end_date", "2027-01-25 17:00"),
            ("total_float_hr_cnt", "8"),
        ],
    );
    doc.table_mut("RSRC")
        .add(&[("rsrc_id", "1".into()), ("rsrc_type", "RT_Labor".into())]);
    doc.table_mut("RSRC")
        .add(&[("rsrc_id", "2".into()), ("rsrc_type", "RT_Mat".into())]);
    for (task, rsrc, qty, act, rem, cost, act_cost, rem_cost) in [
        ("10", "1", "40", "40", "0", "4000", "4000", "0"),
        ("20", "2", "10", "2", "8", "500", "100", "400"),
    ] {
        doc.table_mut("TASKRSRC").add(&[
            ("task_id", task.into()),
            ("proj_id", "1".into()),
            ("rsrc_id", rsrc.into()),
            ("target_qty", qty.into()),
            ("act_reg_qty", act.into()),
            ("remain_qty", rem.into()),
            ("target_cost", cost.into()),
            ("act_reg_cost", act_cost.into()),
            ("remain_cost", rem_cost.into()),
        ]);
    }
    doc.table_mut("PROJCOST").add(&[
        ("proj_id", "1".into()),
        ("task_id", "30".into()),
        ("target_cost", "1000".into()),
        ("act_cost", "0".into()),
        ("remain_cost", "1000".into()),
    ]);
    doc
}

/// Project B, data date Mon 11-Jan-27, one activity not started Mon 1-Feb
/// 08:00 to Fri 5-Feb 17:00 with total float -8 h.
pub fn project_b() -> Document {
    let mut doc = project(7, "BRAVO", "2027-01-11 08:00", "2027-02-04 17:00");
    node(&mut doc, 7, "Bravo Tank Farm", "WS_Planned");
    task(
        &mut doc,
        7,
        70,
        &[
            ("status_code", "TK_NotStart"),
            ("early_start_date", "2027-02-01 08:00"),
            ("early_end_date", "2027-02-05 17:00"),
            ("total_float_hr_cnt", "-8"),
        ],
    );
    doc
}

/// The starter project (`crate::starter`) with crews, for the S-curve and
/// earned hours tests: PIPE and ELEC (labor) and CRANE (equipment), every
/// hour 50 of cost. The survey RB-1010 (3 days) has `survey_hours` PIPE
/// hours (24 in the plan), foundations RB-1070 (5 days) 40 PIPE hours,
/// electrical RB-1100 (5 days) 40 ELEC hours and installation RB-1080
/// 32 CRANE hours. `progressed`: the survey is complete (24 actual hours,
/// 2-Nov-26 08:00 to 4-Nov-26 17:00) and the data date is 9-Nov-26 08:00.
/// Unscheduled: run the scheduler after loading it.
pub fn crews(survey_hours: f64, progressed: bool) -> Document {
    let mut doc = crate::starter().document;
    for (id, code, kind) in [
        (7, "PIPE", "RT_Labor"),
        (8, "CRANE", "RT_Equip"),
        (9, "ELEC", "RT_Labor"),
    ] {
        doc.table_mut("RSRC").add(&[
            ("rsrc_id", id.to_string()),
            ("clndr_id", "1".into()),
            ("rsrc_short_name", code.into()),
            ("rsrc_name", format!("{code} crew")),
            ("rsrc_type", kind.into()),
            ("def_qty_per_hr", "1".into()),
        ]);
    }
    for (id, task, rsrc, units) in [
        (1, 1010, 7, survey_hours),
        (2, 1070, 7, 40.),
        (3, 1100, 9, 40.),
        (4, 1080, 8, 32.),
    ] {
        doc.table_mut("TASKRSRC").add(&[
            ("taskrsrc_id", id.to_string()),
            ("task_id", task.to_string()),
            ("proj_id", "1".into()),
            ("rsrc_id", rsrc.to_string()),
            ("target_qty", units.to_string()),
            ("remain_qty", units.to_string()),
            ("target_cost", (units * 50.).to_string()),
            ("remain_cost", (units * 50.).to_string()),
        ]);
    }
    if !progressed {
        return doc;
    }
    doc.table_mut("PROJECT")
        .set(0, "last_recalc_date", "2026-11-09 08:00".into());
    let task = doc.table_mut("TASK");
    let row = task
        .rows
        .iter()
        .position(|r| task.value(r, "task_id") == "1010")
        .expect("the starter has the survey");
    for (k, v) in [
        ("status_code", "TK_Complete"),
        ("act_start_date", "2026-11-02 08:00"),
        ("act_end_date", "2026-11-04 17:00"),
        ("remain_drtn_hr_cnt", "0"),
        ("phys_complete_pct", "100"),
    ] {
        task.set(row, k, v.into());
    }
    let a = doc.table_mut("TASKRSRC");
    for (k, v) in [
        ("remain_qty", "0"),
        ("act_reg_qty", "24"),
        ("remain_cost", "0"),
        ("act_reg_cost", "1200"),
        ("act_start_date", "2026-11-02 08:00"),
        ("act_end_date", "2026-11-04 17:00"),
    ] {
        a.set(0, k, v.into());
    }
    doc
}

/// The project node (the top WBS row) with the project's name and status.
pub fn node(doc: &mut Document, pid: i64, name: &str, status: &str) {
    doc.table_mut("PROJWBS").add(&[
        ("wbs_id", (pid * 100).to_string()),
        ("proj_id", pid.to_string()),
        ("wbs_short_name", format!("P{pid}")),
        ("wbs_name", name.into()),
        ("proj_node_flag", "Y".into()),
        ("status_code", status.into()),
    ]);
}
