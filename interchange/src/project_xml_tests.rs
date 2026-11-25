use super::*;
use openxer_xer::sample_project::sample_project;
use openxer_xer::ProjectData;

/// The sample project with what it lacks added, so that every table the
/// XML form carries has rows: a project calendar with exceptions, a global
/// activity code, UDFs of several kinds on activities, WBS, the project and
/// a resource, units of measure, a role rate, every relationship type with
/// lag, both constraints, a currency, the OBS, resource codes, a financial
/// period template, schedule options, a project note, a baseline project
/// and a cross-project link.
fn built_document() -> Document {
    let mut doc = sample_project().current.updated_document();
    let wbs = |doc: &Document| {
        let t = doc.table("PROJWBS").unwrap();
        t.rows
            .iter()
            .find(|r| t.value(r, "proj_node_flag") != "Y")
            .map(|r| t.value(r, "wbs_id").to_string())
            .unwrap()
    };
    let wbs_id = wbs(&doc);
    let tasks: Vec<String> = {
        let t = doc.table("TASK").unwrap();
        t.rows
            .iter()
            .map(|r| t.value(r, "task_id").to_string())
            .collect()
    };
    let mut exceptions = CalendarData::default();
    // Monday 07:00 to 11:00 and 12:00 to 15:30, Tuesday 22:00 to 06:00.
    exceptions.week[1] = vec![(420, 660), (720, 930)];
    exceptions.week[2] = vec![(1320, 1800)];
    // 2026-12-31 off, 2027-01-03 08:00 to 12:00.
    exceptions.exceptions.insert(46387, vec![]);
    exceptions.exceptions.insert(46390, vec![(480, 720)]);
    doc.table_mut("CALENDAR").add(&[
        ("clndr_id", "900".into()),
        ("clndr_name", "Project nights & weekends".into()),
        ("clndr_type", "CA_Project".into()),
        ("proj_id", "1".into()),
        ("base_clndr_id", "1".into()),
        ("default_flag", "N".into()),
        ("day_hr_cnt", "7.5".into()),
        ("week_hr_cnt", "37.5".into()),
        ("clndr_data", exceptions.serialize()),
    ]);
    doc.table_mut("ACTVTYPE").add(&[
        ("actv_code_type_id", "900".into()),
        ("actv_code_type", "Shift <global>".into()),
        ("actv_short_len", "4".into()),
        ("seq_num", "5".into()),
        ("actv_code_type_scope", "AS_Global".into()),
    ]);
    doc.table_mut("ACTVCODE").add(&[
        ("actv_code_id", "9001".into()),
        ("actv_code_type_id", "900".into()),
        ("short_name", "DAY".into()),
        ("actv_code_name", "Day \"A\" shift".into()),
        ("seq_num", "1".into()),
        ("color", "1A2B3C".into()),
    ]);
    doc.table_mut("ACTVCODE").add(&[
        ("actv_code_id", "9002".into()),
        ("actv_code_type_id", "900".into()),
        ("parent_actv_code_id", "9001".into()),
        ("short_name", "NITE".into()),
        ("actv_code_name", "Night".into()),
        ("seq_num", "2".into()),
    ]);
    doc.table_mut("TASKACTV").add(&[
        ("task_id", tasks[0].clone()),
        ("actv_code_type_id", "900".into()),
        ("actv_code_id", "9002".into()),
        ("proj_id", "1".into()),
    ]);
    for (id, table, label, ty) in [
        ("901", "TASK", "Inspection date", "FT_START_DATE"),
        ("902", "TASK", "Bid value", "FT_MONEY"),
        ("903", "PROJWBS", "Area lead", "FT_TEXT"),
        ("904", "PROJECT", "Contract number", "FT_TEXT"),
        ("905", "RSRC", "Crew size", "FT_INT"),
        ("906", "TASK", "Rating", "FT_FLOAT_2_DECIMALS"),
    ] {
        doc.table_mut("UDFTYPE").add(&[
            ("udf_type_id", id.into()),
            ("table_name", table.into()),
            ("udf_type_name", format!("user_field_{id}")),
            ("udf_type_label", label.into()),
            ("logical_data_type", ty.into()),
        ]);
    }
    let rsrc = {
        let t = doc.table("RSRC").unwrap();
        t.value(&t.rows[0], "rsrc_id").to_string()
    };
    for (ty, fk, pid, col, v) in [
        ("901", tasks[1].clone(), "1", "udf_date", "2026-03-04 09:30"),
        ("902", tasks[1].clone(), "1", "udf_number", "1250.75"),
        ("903", wbs_id.clone(), "1", "udf_text", "North & <east>"),
        ("904", "1".to_string(), "1", "udf_text", "C-77"),
        ("905", rsrc.clone(), "", "udf_number", "6"),
        ("906", tasks[2].clone(), "1", "udf_number", "-0.25"),
    ] {
        doc.table_mut("UDFVALUE").add(&[
            ("udf_type_id", ty.into()),
            ("fk_id", fk),
            ("proj_id", pid.into()),
            (col, v.into()),
        ]);
    }
    doc.table_mut("ROLERATE").add(&[
        ("role_rate_id", "1".into()),
        ("role_id", "1".into()),
        ("cost_per_qty", "85.5".into()),
        ("cost_per_qty2", "90".into()),
        ("start_date", "2026-01-01 00:00".into()),
        ("max_qty_per_hr", "2".into()),
    ]);
    doc.table_mut("CURRTYPE").add(&[
        ("curr_id", "1".into()),
        ("decimal_digit_cnt", "2".into()),
        ("curr_symbol", "$".into()),
        ("decimal_symbol", ".".into()),
        ("digit_group_symbol", ",".into()),
        ("pos_curr_fmt_type", "#1.1".into()),
        ("neg_curr_fmt_type", "(#1.1)".into()),
        ("curr_type", "Test Dollar".into()),
        ("curr_short_name", "TSD".into()),
        ("base_exch_rate", "1".into()),
    ]);
    doc.table_mut("OBS").add(&[
        ("obs_id", "70".into()),
        ("seq_num", "1".into()),
        ("obs_name", "Site team".into()),
        ("obs_descr", "<p>Field office</p>".into()),
    ]);
    doc.table_mut("RCATTYPE").add(&[
        ("rsrc_catg_type_id", "80".into()),
        ("seq_num", "1".into()),
        ("rsrc_catg_short_len", "10".into()),
        ("rsrc_catg_type", "Crew".into()),
    ]);
    doc.table_mut("RCATVAL").add(&[
        ("rsrc_catg_id", "81".into()),
        ("rsrc_catg_type_id", "80".into()),
        ("seq_num", "1".into()),
        ("rsrc_catg_short_name", "C1".into()),
        ("rsrc_catg_name", "Crew one".into()),
    ]);
    doc.table_mut("RSRCRCAT").add(&[
        ("rsrc_id", rsrc.clone()),
        ("rsrc_catg_type_id", "80".into()),
        ("rsrc_catg_id", "81".into()),
    ]);
    doc.table_mut("FINTMPL").add(&[
        ("fintmpl_id", "3".into()),
        ("fintmpl_name", "Weeks".into()),
        ("default_flag", "N".into()),
    ]);
    doc.table_mut("WBSMEMO").add(&[
        ("wbs_memo_id", "950".into()),
        ("proj_id", "1".into()),
        ("memo_type_id", "1".into()),
        ("wbs_id", "100".into()),
        ("wbs_memo", "<p>Project scope note</p>".into()),
    ]);
    doc.table_mut("SCHEDOPTIONS").add(&[
        ("schedoptions_id", "1".into()),
        ("proj_id", "1".into()),
        ("sched_outer_depend_type", "SD_Both".into()),
        ("sched_open_critical_flag", "N".into()),
        ("sched_lag_early_start_flag", "Y".into()),
        ("sched_retained_logic", "Y".into()),
        ("sched_progress_override", "N".into()),
        ("sched_float_type", "FT_Min".into()),
        (
            "sched_calendar_on_relationship_lag",
            "rcal_Predecessor".into(),
        ),
        ("sched_use_expect_end_flag", "Y".into()),
        ("sched_use_project_end_date_for_float", "Y".into()),
        ("level_float_thrs_cnt", "8".into()),
        ("level_outer_assign_flag", "N".into()),
        ("level_outer_assign_priority", "5".into()),
        ("level_over_alloc_pct", "25".into()),
        ("level_within_float_flag", "N".into()),
        ("level_keep_sched_date_flag", "Y".into()),
        ("level_all_rsrc_flag", "Y".into()),
        ("enable_multiple_longest_path_calc", "N".into()),
        ("max_multiple_longest_path", "10".into()),
        ("use_total_float_multiple_longest_paths", "Y".into()),
    ]);
    {
        // The WBS element and the root under the OBS element.
        let t = doc.table_mut("PROJWBS");
        for row in 0..t.rows.len() {
            t.set(row, "obs_id", "70".into());
        }
    }
    // Every relationship type, with lag in hours.
    let next_link = {
        let t = doc.table("TASKPRED").unwrap();
        t.rows
            .iter()
            .map(|r| num(t.value(r, "task_pred_id")))
            .max()
            .unwrap_or(0)
            + 1
    };
    for (i, ty) in ["PR_SS", "PR_FF", "PR_SF"].iter().enumerate() {
        doc.table_mut("TASKPRED").add(&[
            ("task_pred_id", (next_link + i as i64).to_string()),
            ("task_id", tasks[5 + i].clone()),
            ("pred_task_id", tasks[3].clone()),
            ("proj_id", "1".into()),
            ("pred_proj_id", "1".into()),
            ("pred_type", ty.to_string()),
            ("lag_hr_cnt", ["-4", "12.5", "0"][i].into()),
            ("comments", "Tie & check".into()),
        ]);
    }
    // Both constraints on one activity, and a physical percent.
    {
        let t = doc.table_mut("TASK");
        t.set(4, "cstr_type", "CS_MSOA".into());
        t.set(4, "cstr_date", "2026-02-02 08:00".into());
        t.set(4, "cstr_type2", "CS_MEOB".into());
        t.set(4, "cstr_date2", "2026-06-30 17:00".into());
        t.set(4, "priority_type", "PT_High".into());
        t.set(4, "phys_complete_pct", "50".into());
    }
    // A baseline of the project, held in the same file, and a project
    // linked to it.
    doc.table_mut("PROJECT").add(&[
        ("proj_id", "2".into()),
        ("proj_short_name", "RIVERBEND-B1".into()),
        ("orig_proj_id", "1".into()),
        ("last_recalc_date", "2026-01-05 08:00".into()),
        ("plan_start_date", "2026-01-05 08:00".into()),
        ("clndr_id", "1".into()),
    ]);
    doc.table_mut("PROJWBS").add(&[
        ("wbs_id", "9100".into()),
        ("proj_id", "2".into()),
        ("seq_num", "0".into()),
        ("proj_node_flag", "Y".into()),
        ("status_code", "WS_Inactive".into()),
        ("wbs_short_name", "RIVERBEND-B1".into()),
        ("wbs_name", "Baseline one".into()),
    ]);
    doc.table_mut("TASK").add(&[
        ("task_id", "91000".into()),
        ("proj_id", "2".into()),
        ("wbs_id", "9100".into()),
        ("clndr_id", "1".into()),
        ("task_code", "B-1000".into()),
        ("task_name", "Baseline milestone".into()),
        ("task_type", "TT_Mile".into()),
        ("status_code", "TK_NotStart".into()),
        ("duration_type", "DT_FixedDUR2".into()),
        ("target_drtn_hr_cnt", "0".into()),
        ("remain_drtn_hr_cnt", "0".into()),
        ("target_start_date", "2026-01-05 08:00".into()),
        ("target_end_date", "2026-01-05 08:00".into()),
    ]);
    doc.table_mut("TASKPRED").add(&[
        ("task_pred_id", (next_link + 10).to_string()),
        ("task_id", tasks[0].clone()),
        ("pred_task_id", "91000".into()),
        ("proj_id", "1".into()),
        ("pred_proj_id", "2".into()),
        ("pred_type", "PR_FS".into()),
        ("lag_hr_cnt", "8".into()),
    ]);
    doc
}

/// A value as the comparison sees it: numbers by value, text as stored.
fn normalized(v: &str) -> String {
    let t = v.trim();
    match t.parse::<f64>() {
        Ok(n) if !t.is_empty() => trim_number(&format!("{n:.6}")),
        _ => t.to_string(),
    }
}

/// The rows of `table` reduced to `cols`, sorted, without rows that `skip`
/// selects (rows the XML form holds elsewhere).
fn projected(doc: &Document, table: &str, cols: &[&str]) -> Vec<Vec<String>> {
    let Some(t) = doc.table(table) else {
        return vec![];
    };
    let mut rows: Vec<Vec<String>> = t
        .rows
        .iter()
        .filter(|r| !(table == "PROJWBS" && t.value(r, "proj_node_flag") == "Y"))
        .map(|r| cols.iter().map(|c| normalized(t.value(r, c))).collect())
        .collect();
    rows.sort();
    rows
}

fn root_rows(doc: &Document) -> Vec<Vec<String>> {
    let t = doc.table("PROJWBS").unwrap();
    let mut cols = carried_root_columns();
    cols.push("proj_node_flag");
    let mut rows: Vec<Vec<String>> = t
        .rows
        .iter()
        .filter(|r| t.value(r, "proj_node_flag") == "Y")
        .map(|r| cols.iter().map(|c| normalized(t.value(r, c))).collect())
        .collect();
    rows.sort();
    rows
}

type Week = [Vec<Shift>; 7];

fn calendars(doc: &Document) -> Vec<(String, Week, BTreeMap<i64, Vec<Shift>>)> {
    let t = doc.table("CALENDAR").unwrap();
    let mut out: Vec<_> = t
        .rows
        .iter()
        .map(|r| {
            let c = CalendarData::parse(t.value(r, "clndr_data")).unwrap();
            (t.value(r, "clndr_id").to_string(), c.week, c.exceptions)
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// Every carried table, column and calendar of `a` is in `b`, and nothing more.
fn assert_same_carried(a: &Document, b: &Document) {
    for (table, cols) in carried_columns() {
        let (x, y) = (projected(a, table, &cols), projected(b, table, &cols));
        assert!(
            !x.is_empty(),
            "the built project has no {table} rows to test"
        );
        if x != y {
            let missing: Vec<_> = x.iter().filter(|r| !y.contains(r)).take(3).collect();
            let extra: Vec<_> = y.iter().filter(|r| !x.contains(r)).take(3).collect();
            panic!("{table} differs ({cols:?}):\n  lost {missing:?}\n  gained {extra:?}");
        }
    }
    assert_eq!(root_rows(a), root_rows(b), "project root WBS rows");
    assert_eq!(
        calendars(a),
        calendars(b),
        "calendar work weeks and exceptions"
    );
}

#[test]
fn model_to_xml_to_model_keeps_every_carried_table() {
    let doc = built_document();
    let xml = write(&doc).unwrap();
    assert!(xml.starts_with(DECLARATION));
    assert!(xml.contains(&root_tag()), "the root carries the namespace");
    assert!(
        xml.contains("<BaselineProject>"),
        "the baseline goes as a baseline"
    );
    assert!(xml.contains("<Type>Start to Finish</Type>"));
    let constraint = "<PrimaryConstraintType>Start On or After</PrimaryConstraintType>";
    assert!(xml.contains(constraint));
    assert!(xml.contains("North &amp; &lt;east&gt;"));
    let back = read(&xml).unwrap();
    assert_same_carried(&doc, &back);
    assert_eq!(crate::baseline::baselines_in(&back), vec![(2, 1)]);
    // The project loads as an import would load it.
    let p = ProjectData::from_document(back, 1, "xml".into()).unwrap();
    assert_eq!(
        p.model.activities.len(),
        sample_project().current.model.activities.len()
    );
}

#[test]
fn xer_to_model_to_xml_to_model_to_xer_keeps_the_covered_tables() {
    let text = built_document().text();
    let first = ProjectData::from_document(Document::parse(&text).unwrap(), 1, "xer".into())
        .unwrap()
        .updated_document();
    let xml = write(&first).unwrap();
    let second = ProjectData::from_document(read(&xml).unwrap(), 1, "xml".into())
        .unwrap()
        .updated_document();
    assert_same_carried(&first, &second);
    // Written again, the XML is the same.
    assert_eq!(write(&second).unwrap(), xml);
}

#[test]
fn work_periods_end_a_minute_early_in_the_file() {
    let doc = built_document();
    let xml = write(&doc).unwrap();
    // The project calendar's Monday: 07:00 to 11:00 and 12:00 to 15:30.
    assert!(xml.contains("<Finish>10:59:00</Finish>"));
    assert!(xml.contains("<Finish>15:29:00</Finish>"));
    // A period across midnight: 22:00 to 06:00.
    assert!(xml.contains("<Finish>05:59:00</Finish>"));
    assert!(xml.contains("<Date>2026-12-31T00:00:00</Date>"));
}

#[test]
fn reads_a_file_written_elsewhere() {
    // Namespaces, attributes, a comment, entities, CDATA, a top WBS whose
    // parent is the project, no root WBS id, and percents as fractions.
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<!-- written by another tool -->
<p:APIBusinessObjects xmlns:p="urn:example" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
  <p:Calendar>
    <p:ObjectId>7</p:ObjectId>
    <p:Name>Five &amp; eight</p:Name>
    <p:Type>Global</p:Type>
    <p:HoursPerDay>8</p:HoursPerDay>
    <p:StandardWorkWeek>
      <p:StandardWorkHours><p:DayOfWeek>Monday</p:DayOfWeek>
        <p:WorkTime><p:Start>08:00:00</p:Start><p:Finish>11:59:00</p:Finish></p:WorkTime>
        <p:WorkTime><p:Start>13:00:00</p:Start><p:Finish>17:00:00</p:Finish></p:WorkTime>
      </p:StandardWorkHours>
      <p:StandardWorkHours><p:DayOfWeek>Sunday</p:DayOfWeek></p:StandardWorkHours>
    </p:StandardWorkWeek>
    <p:HolidayOrExceptions>
      <p:HolidayOrException><p:Date>2026-12-25T00:00:00</p:Date></p:HolidayOrException>
    </p:HolidayOrExceptions>
  </p:Calendar>
  <p:Project>
    <p:ObjectId>55</p:ObjectId>
    <p:Id>PLANT-7</p:Id>
    <p:Name><![CDATA[Plant <7> shutdown]]></p:Name>
    <p:DataDate>2026-03-02T08:00:00</p:DataDate>
    <p:PlannedStartDate>2026-03-02T08:00:00</p:PlannedStartDate>
    <p:ActivityDefaultCalendarObjectId>7</p:ActivityDefaultCalendarObjectId>
    <p:WBS><p:ObjectId>200</p:ObjectId><p:Code>A</p:Code><p:Name>Area A</p:Name><p:ParentObjectId>55</p:ParentObjectId></p:WBS>
    <p:WBS><p:ObjectId>201</p:ObjectId><p:Code>A1</p:Code><p:Name>Unit 1</p:Name><p:ParentObjectId>200</p:ParentObjectId></p:WBS>
    <p:Activity>
      <p:ObjectId>3000</p:ObjectId><p:Id>A1000</p:Id><p:Name>Isolate &#x26; drain</p:Name>
      <p:WBSObjectId>201</p:WBSObjectId><p:CalendarObjectId>7</p:CalendarObjectId>
      <p:Type>Task Dependent</p:Type><p:Status>In Progress</p:Status>
      <p:PhysicalPercentComplete>0.375</p:PhysicalPercentComplete>
      <p:PlannedDuration>16</p:PlannedDuration><p:RemainingDuration>8</p:RemainingDuration>
      <p:ActualStartDate>2026-03-02T08:00:00.000-05:00</p:ActualStartDate>
      <p:AutoComputeActuals>false</p:AutoComputeActuals>
      <p:UnknownField>kept out</p:UnknownField>
    </p:Activity>
    <p:Activity>
      <p:ObjectId>3001</p:ObjectId><p:Id>A1010</p:Id><p:Name>Open vessel</p:Name>
      <p:Type>Finish Milestone</p:Type><p:Status>Not Started</p:Status>
    </p:Activity>
    <p:Relationship>
      <p:ObjectId>1</p:ObjectId><p:PredecessorActivityObjectId>3000</p:PredecessorActivityObjectId>
      <p:SuccessorActivityObjectId>3001</p:SuccessorActivityObjectId><p:Type>Finish to Start</p:Type><p:Lag>4</p:Lag>
    </p:Relationship>
  </p:Project>
</p:APIBusinessObjects>"#;
    let doc = read_bytes(xml.as_bytes()).unwrap();
    let t = doc.table("PROJWBS").unwrap();
    let root = t
        .rows
        .iter()
        .find(|r| t.value(r, "proj_node_flag") == "Y")
        .unwrap();
    assert_eq!(t.value(root, "wbs_name"), "Plant <7> shutdown");
    let root_id = t.value(root, "wbs_id").to_string();
    let area = t
        .rows
        .iter()
        .find(|r| t.value(r, "wbs_id") == "200")
        .unwrap();
    assert_eq!(
        t.value(area, "parent_wbs_id"),
        root_id,
        "top WBS under the project"
    );
    let tasks = doc.table("TASK").unwrap();
    let a = &tasks.rows[0];
    assert_eq!(tasks.value(a, "task_name"), "Isolate & drain");
    assert_eq!(tasks.value(a, "phys_complete_pct"), "37.5");
    assert_eq!(tasks.value(a, "act_start_date"), "2026-03-02 08:00");
    assert_eq!(tasks.value(a, "auto_compute_act_flag"), "N");
    assert_eq!(
        tasks.value(&tasks.rows[1], "wbs_id"),
        root_id,
        "no WBS: the project"
    );
    let (_, week, exceptions) = calendars(&doc).pop().unwrap();
    assert_eq!(week[1], vec![(480, 720), (780, 1020)]);
    assert!(week[0].is_empty());
    assert_eq!(
        exceptions.get(&day_to_serial("2026-12-25").unwrap()),
        Some(&vec![])
    );
    let p = ProjectData::from_document(doc, 55, "xml".into()).unwrap();
    assert_eq!(p.model.activities.len(), 2);
}

#[test]
fn tells_xml_from_xer_and_refuses_other_xml() {
    let xer = openxer_xer::starter().updated_document();
    let bytes = xer.to_bytes().unwrap();
    assert!(!is_xml(&bytes));
    assert_eq!(read_bytes(&bytes).unwrap().projects(), xer.projects());
    assert!(is_xml(b"\xef\xbb\xbf  <?xml version=\"1.0\"?><a/>"));
    let e = read("<?xml version=\"1.0\"?><Project/>").unwrap_err();
    assert!(e.contains("not a project file"), "{e}");
    assert!(read("<APIBusinessObjects><Project>").is_err());
    assert!(read("<APIBusinessObjects></APIBusinessObjects>").is_err());
}

#[test]
fn decimals_move_exactly() {
    for (v, places, out) in [
        ("50", -2, "0.5"),
        ("0.5", 2, "50"),
        ("33.333333333333", -2, "0.33333333333333"),
        ("0.33333333333333", 2, "33.333333333333"),
        ("100", -2, "1"),
        ("0", -2, "0"),
        ("-12.5", -2, "-0.125"),
        ("1e2", -2, "1"),
    ] {
        assert_eq!(
            shift_decimal(v, places).as_deref(),
            Some(out),
            "{v} by {places}"
        );
    }
    assert_eq!(shift_decimal("abc", 2), None);
}

#[test]
fn every_project_and_starter_round_trip() {
    let doc = openxer_xer::starter_portfolio();
    let back = read(&write(&doc).unwrap()).unwrap();
    assert_eq!(back.projects(), doc.projects());
    let cols = ["task_id", "pred_task_id", "proj_id", "pred_proj_id"];
    let links = |d: &Document| projected(d, "TASKPRED", &cols);
    assert_eq!(links(&back), links(&doc), "cross-project links kept");
}

/// With `OPENXER_SAMPLE_XER` set to a path outside the repo, writes the
/// synthetic sample project there as XER, so that the command line can
/// export it for a structural comparison; otherwise does nothing.
#[test]
fn sample_project_as_xer_on_request() {
    let Ok(path) = std::env::var("OPENXER_SAMPLE_XER") else {
        return;
    };
    let text = sample_project().current.updated_document().text();
    std::fs::write(path, text).unwrap();
}

#[test]
fn reads_any_namespace_version_or_none() {
    let doc = openxer_xer::starter().updated_document();
    let xml = write(&doc).unwrap();
    let written = namespace();
    assert!(SCHEMA_BASE.contains("{version}"));
    assert!(written.contains(SCHEMA_VERSION));
    let other = SCHEMA_BASE.replace("{version}", "99.1");
    let elsewhere = xml.replace(&written, "urn:example:projects");
    let bare = xml.replace(&format!(" xmlns=\"{written}\""), "");
    assert_ne!(bare, xml);
    let tasks = |d: &Document| d.table("TASK").map_or(0, |t| t.rows.len());
    let base = SCHEMA_BASE.replace("{version}", SCHEMA_VERSION);
    for text in [xml.replace(&base, &other), elsewhere, bare] {
        let back = read(&text).unwrap();
        assert_eq!(back.projects(), doc.projects());
        assert_eq!(tasks(&back), tasks(&doc));
    }
}

// ------------------------------------------------------------ structure
// The structure the reference wrote in an exported project file, from the
// notes of that observation (names and order only; no values). One type per
// line: its key, a colon, then its children in the order written.
// Continuation lines start with spaces.

const OBSERVED_ROOT: &[&str] = &[
    "ProjectList",
    "DisplayCurrency",
    "Currency",
    "UDFType",
    "NotebookTopic",
    "OBS",
    "Calendar",
    "ResourceCodeType",
    "ResourceCode",
    "Resource",
    "FinancialPeriodTemplate",
    "Project",
];

const OBSERVED: &str = "
Project: ActivityDefaultActivityType ActivityDefaultCalendarObjectId
  ActivityDefaultCostAccountObjectId ActivityDefaultDurationType
  ActivityDefaultPercentCompleteType ActivityDefaultPricePerUnit
  ActivityIdBasedOnSelectedActivity ActivityIdIncrement ActivityIdPrefix
  ActivityIdSuffix ActivityPercentCompleteBasedOnActivitySteps
  AddActualToRemaining AddedBy AllowNegativeActualUnitsFlag AllowStatusReview
  AnnualDiscountRate AnticipatedFinishDate AnticipatedStartDate
  AssignmentDefaultDrivingFlag AssignmentDefaultRateType CheckOutStatus
  CostQuantityRecalculateFlag CriticalActivityFloatLimit CriticalActivityPathType
  CurrentBaselineProjectObjectId DataDate DateAdded DefaultPriceTimeUnits
  DiscountApplicationPeriod EarnedValueComputeType EarnedValueETCComputeType
  EarnedValueETCUserValue EarnedValueUserPercent EnableSummarization
  FinancialPeriodTemplateId FiscalYearStartMonth GUID Id IndependentETCLaborUnits
  IndependentETCTotalCost LastFinancialPeriodObjectId LevelingPriority
  LinkActualToActualThisPeriod LinkPercentCompleteWithActual
  LinkPlannedAndAtCompletionFlag MustFinishByDate Name OBSObjectId ObjectId
  OriginalBudget ParentEPSObjectId PlannedStartDate PrimaryBaselineObjectId
  PrimaryResourcesCanMarkActivitiesAsCompleted ProjectForecastStartDate
  ResetPlannedToRemainingFlag ResourceCanBeAssignedToSameActivityMoreThanOnce
  ResourcesCanAssignThemselvesToActivities ScheduledFinishDate
  SecondaryBaselineObjectId Status StrategicPriority SummarizeToWBSLevel
  SummaryLevel TertiaryBaselineObjectId UseProjectBaselineForEarnedValue
  WBSCodeSeparator WBSObjectId WebSiteRootDirectory WebSiteURL
  Calendar WBS ProjectNote ActivityCodeType ActivityCode Activity
  ResourceAssignment ActivityExpense ActivityNote Relationship ScheduleOptions
ProjectList: Project
ProjectList/Project: Id Name TemplateId TemplateName
DisplayCurrency: Currency
Currency: DecimalPlaces DecimalSymbol DigitGroupingSymbol ExchangeRate Id Name
  NegativeSymbol ObjectId PositiveSymbol Symbol
UDFType: DataType IsSecureCode ObjectId SubjectArea Title
NotebookTopic: AvailableForActivity AvailableForEPS AvailableForProject
  AvailableForWBS Name ObjectId SequenceNumber
OBS: Description GUID Name ObjectId ParentObjectId SequenceNumber
Calendar: BaseCalendarObjectId HoursPerDay HoursPerMonth HoursPerWeek HoursPerYear
  IsDefault IsPersonal Name ObjectId ProjectObjectId Type StandardWorkWeek
  HolidayOrExceptions
StandardWorkWeek: StandardWorkHours
StandardWorkHours: DayOfWeek WorkTime
WorkTime: Start Finish
HolidayOrExceptions: HolidayOrException
HolidayOrException: Date WorkTime
ResourceCodeType: IsSecureCode Length Name ObjectId SequenceNumber
ResourceCode: CodeTypeObjectId CodeValue Description ObjectId ParentObjectId
  SequenceNumber
Resource: AutoComputeActuals CalculateCostFromUnits CalendarObjectId
  CurrencyObjectId DefaultUnitsPerTime EmailAddress EmployeeId GUID Id IsActive
  IsOverTimeAllowed Name ObjectId OfficePhone OtherPhone OvertimeFactor
  ParentObjectId PrimaryRoleObjectId ResourceNotes ResourceType SequenceNumber
  ShiftObjectId TimesheetApprovalManagerObjectId Title UnitOfMeasureObjectId
  UseTimesheets UserObjectId Code
Resource/Code: TypeObjectId ValueObjectId
FinancialPeriodTemplate: FinancialPeriodTemplateName ObjectId
WBS: AnticipatedFinishDate AnticipatedStartDate Code EarnedValueComputeType
  EarnedValueETCComputeType EarnedValueETCUserValue EarnedValueUserPercent GUID
  IndependentETCLaborUnits IndependentETCTotalCost Name OBSObjectId ObjectId
  OriginalBudget ParentObjectId ProjectObjectId SequenceNumber Status
  WBSCategoryObjectId
ProjectNote: Note NotebookTopicObjectId ObjectId ProjectObjectId WBSObjectId
ActivityCodeType: EPSObjectId IsSecureCode Length Name ObjectId ProjectObjectId
  RefProjectObjectIds Scope SequenceNumber
ActivityCode: CodeTypeObjectId CodeValue Color Description ObjectId ParentObjectId
  ProjectObjectId SequenceNumber
Activity: ActualDuration ActualFinishDate ActualLaborCost ActualLaborUnits
  ActualNonLaborCost ActualNonLaborUnits ActualStartDate ActualThisPeriodLaborCost
  ActualThisPeriodLaborUnits ActualThisPeriodNonLaborCost
  ActualThisPeriodNonLaborUnits AtCompletionDuration AtCompletionExpenseCost
  AtCompletionLaborCost AtCompletionLaborUnits AtCompletionNonLaborCost
  AtCompletionNonLaborUnits AutoComputeActuals CalendarObjectId
  DurationPercentComplete DurationType EstimatedWeight ExpectedFinishDate
  ExternalEarlyStartDate ExternalLateFinishDate Feedback FinishDate GUID Id
  IsNewFeedback LevelingPriority Name NonLaborUnitsPercentComplete
  NotesToResources ObjectId PercentComplete PercentCompleteType
  PhysicalPercentComplete PlannedDuration PlannedFinishDate PlannedLaborCost
  PlannedLaborUnits PlannedNonLaborCost PlannedNonLaborUnits PlannedStartDate
  PrimaryConstraintDate PrimaryConstraintType PrimaryResourceObjectId
  ProjectObjectId RemainingDuration RemainingEarlyFinishDate
  RemainingEarlyStartDate RemainingLaborCost RemainingLaborUnits
  RemainingLateFinishDate RemainingLateStartDate RemainingNonLaborCost
  RemainingNonLaborUnits ResumeDate ReviewRequired ScopePercentComplete
  SecondaryConstraintDate SecondaryConstraintType StartDate Status SuspendDate
  Type UnitsPercentComplete WBSObjectId Code UDF
Activity/Code: TypeObjectId ValueObjectId
Activity/UDF: TypeObjectId CostValue TextValue
ResourceAssignment: ActivityObjectId ActualCost ActualCurve ActualFinishDate
  ActualOvertimeCost ActualOvertimeUnits ActualRegularCost ActualRegularUnits
  ActualStartDate ActualThisPeriodCost ActualThisPeriodUnits ActualUnits
  AtCompletionCost AtCompletionUnits CostAccountObjectId DrivingActivityDatesFlag
  FinishDate GUID IsCostUnitsLinked IsPrimaryResource ObjectId OvertimeFactor
  PlannedCost PlannedCurve PlannedFinishDate PlannedLag PlannedStartDate
  PlannedUnits PlannedUnitsPerTime PricePerUnit Proficiency ProjectObjectId
  RateSource RateType RemainingCost RemainingCurve RemainingDuration
  RemainingFinishDate RemainingLag RemainingStartDate RemainingUnits
  RemainingUnitsPerTime ResourceCurveObjectId ResourceObjectId ResourceType
  RoleObjectId StartDate UnitsPercentComplete WBSObjectId
ActivityExpense: AccrualType ActivityObjectId ActualCost ActualUnits
  AutoComputeActuals CostAccountObjectId DocumentNumber ExpenseCategoryObjectId
  ExpenseDescription ExpenseItem ExpensePercentComplete ObjectId PlannedCost
  PlannedUnits PricePerUnit ProjectObjectId RemainingCost RemainingUnits
  UnitOfMeasure Vendor WBSObjectId
ActivityNote: ActivityObjectId Note NotebookTopicObjectId ObjectId ProjectObjectId
  WBSObjectId
Relationship: Comments Lag ObjectId PredecessorActivityObjectId
  PredecessorProjectObjectId SuccessorActivityObjectId SuccessorProjectObjectId
  Type
ScheduleOptions: CalculateFloatBasedOnFinishDate ComputeTotalFloatType
  CriticalActivityFloatThreshold CriticalActivityPathType
  ExternalProjectPriorityLimit IgnoreOtherProjectRelationships
  IncludeExternalResAss LevelAllResources LevelWithinFloat
  MakeOpenEndedActivitiesCritical MaximumMultipleFloatPaths MinFloatToPreserve
  MultipleFloatPathsEnabled MultipleFloatPathsEndingActivityObjectId
  MultipleFloatPathsUseTotalFloat OutOfSequenceScheduleType
  OverAllocationPercentage PreserveScheduledEarlyAndLateDates PriorityList
  ProjectObjectId RelationshipLagCalendar ResourceList
  StartToStartLagCalculationType UseExpectedFinishDates
";

/// Children OpenXER writes that the observed file did not hold, allowed at
/// the end of a type's list: UDF values on types other than activities, the
/// typed UDF values other than text and cost, and a baseline's original
/// project.
const NOT_OBSERVED_TAIL: &str = "
Project: UDF
WBS: UDF
Resource: UDF
ResourceAssignment: UDF
ActivityExpense: UDF
Activity/UDF: StartDateValue FinishDateValue DoubleValue IntegerValue
  IndicatorValue
";

/// Element types the observed file did not hold. Their plain fields are
/// checked against the ordering rule only.
const NOT_OBSERVED_TYPES: &[&str] = &[
    "BaselineProject",
    "CostAccount",
    "ExpenseCategory",
    "UnitOfMeasure",
    "Role",
    "RoleRate",
    "ResourceRate",
    "ResourceRole",
    "Project/UDF",
    "WBS/UDF",
    "Resource/UDF",
    "ResourceAssignment/UDF",
    "ActivityExpense/UDF",
];

fn table(text: &str) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut key = String::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let rest = if line.starts_with(' ') {
            line
        } else {
            let (k, rest) = line.split_once(':').expect("a type line");
            key = k.to_string();
            rest
        };
        let names = rest.split_whitespace().map(str::to_string);
        out.entry(key.clone()).or_default().extend(names);
    }
    out
}

/// The key of an element type in the tables: its name, or its parent's
/// name and its own where the same name means different types.
fn type_key(parent: &str, name: &str) -> String {
    match name {
        "Code" | "UDF" => format!("{parent}/{name}"),
        "Project" if parent == "ProjectList" => "ProjectList/Project".into(),
        _ => name.to_string(),
    }
}

/// Child names with runs of the same name as one.
fn collapsed(e: &Element) -> Vec<&str> {
    let mut out: Vec<&str> = vec![];
    for c in &e.children {
        if out.last() != Some(&c.name.as_str()) {
            out.push(c.name.as_str());
        }
    }
    out
}

fn in_order(names: &[&str], order: &[String]) -> bool {
    let mut rest = order.iter();
    names.iter().all(|n| rest.any(|o| o == n))
}

/// Every element type with children, by key, and the elements of each.
fn by_type<'a>(e: &'a Element, out: &mut BTreeMap<String, Vec<&'a Element>>) {
    for c in &e.children {
        if !c.children.is_empty() {
            out.entry(type_key(&e.name, &c.name)).or_default().push(c);
            by_type(c, out);
        }
    }
}

#[test]
fn writes_the_observed_root_line_and_child_order_of_every_type() {
    let xml = write(&built_document()).unwrap();
    // The first two lines: the declaration and the root element with its
    // namespace, the instance namespace and the schema location.
    let base = SCHEMA_BASE.replace("{version}", SCHEMA_VERSION);
    let namespace = format!("{base}/BusinessObjects");
    let root = format!(
        "<APIBusinessObjects xmlns=\"{namespace}\" \
         xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" \
         xsi:schemaLocation=\"{namespace} {base}/{SCHEMA_FILE}\">"
    );
    assert_eq!(
        xml.split("\r\n").take(2).collect::<Vec<_>>(),
        vec!["<?xml version=\"1.0\" encoding=\"utf-8\"?>", root.as_str()]
    );
    assert!(SCHEMA_BASE.ends_with("/V{version}/API"));
    assert_eq!(SCHEMA_VERSION, "25.12");
    assert!(SCHEMA_FILE.ends_with(".xsd"));
    // CR LF line ends, two spaces per level.
    assert!(!xml.replace("\r\n", "").contains('\n'));
    assert!(xml.contains("\r\n  <ProjectList>\r\n    <Project ObjectId=\"1\">\r\n"));

    let tree = parse_tree(&xml).unwrap();
    let observed = table(OBSERVED);
    let tail = table(NOT_OBSERVED_TAIL);
    // The root: the observed types in the observed order, each present.
    let names = collapsed(&tree);
    let seen: Vec<&str> = names
        .iter()
        .copied()
        .filter(|n| OBSERVED_ROOT.contains(n))
        .collect();
    assert_eq!(seen, OBSERVED_ROOT, "root children");
    let mut types = BTreeMap::new();
    by_type(&tree, &mut types);
    for key in observed.keys() {
        assert!(types.contains_key(key), "the built file has no {key}");
    }
    for (key, elements) in &types {
        let unobserved = NOT_OBSERVED_TYPES.contains(&key.as_str());
        assert!(
            unobserved || observed.contains_key(key),
            "{key} is written but has no observed order"
        );
        let mut order = observed.get(key).cloned().unwrap_or_default();
        order.extend(tail.get(key).cloned().unwrap_or_default());
        for e in elements {
            let names = collapsed(e);
            if !unobserved {
                assert!(
                    in_order(&names, &order),
                    "{key} children {names:?} are not in the observed order"
                );
            }
            // Plain fields in ordinal order, nested elements after them
            // (a work period is Start then Finish, a UDF or code its type
            // first, as observed).
            if key == "WorkTime" || key.ends_with("/UDF") {
                continue;
            }
            let plain: Vec<&str> = e
                .children
                .iter()
                .take_while(|c| c.children.is_empty() && !(c.name == "WorkTime" && c.nil))
                .map(|c| c.name.as_str())
                .collect();
            assert!(
                plain.windows(2).all(|w| w[0] < w[1]),
                "{key} plain fields {plain:?} are not in ordinal order"
            );
        }
    }
}

#[test]
fn writes_the_observed_value_forms() {
    let doc = built_document();
    let xml = write(&doc).unwrap();
    let tree = parse_tree(&xml).unwrap();
    let project = tree
        .all("Project")
        .find(|p| p.value("ObjectId") == Some("1"))
        .unwrap();
    // The project's WBS node is not a WBS element: the project names it,
    // and top WBS elements have a nil parent.
    let node_id = project.value("WBSObjectId").unwrap();
    let named = project
        .all("WBS")
        .any(|w| w.value("ObjectId") == Some(node_id));
    assert!(!named, "the project's node is not a WBS element");
    let top_nil = project
        .all("WBS")
        .any(|w| w.child("ParentObjectId").is_some_and(|p| p.nil));
    assert!(top_nil);
    assert!(xml.contains("<ParentObjectId xsi:nil=\"true\" />"));
    // The project's own note names no WBS element.
    let own_note = project
        .all("ProjectNote")
        .any(|n| n.child("WBSObjectId").is_some_and(|w| w.nil));
    assert!(own_note);
    // The project's ID and name as CDATA.
    assert!(xml.contains("<Id><![CDATA["));
    // A day without work: one nil work period. Booleans 0 and 1.
    assert!(xml.contains("<WorkTime xsi:nil=\"true\" />"));
    assert!(xml.contains("<IsDefault>0</IsDefault>"));
    assert!(!xml.contains(">true<") && !xml.contains(">false<"));
    // Percent as a fraction, dates without a zone, durations in hours.
    let activity = project
        .all("Activity")
        .find(|a| a.value("PhysicalPercentComplete") == Some("0.5"))
        .expect("50 % written as 0.5");
    assert_eq!(activity.value("PercentComplete"), Some("0.5"));
    let start = activity.value("PlannedStartDate").unwrap();
    assert_eq!(start.len(), 19, "{start}");
    assert_eq!(&start[10..11], "T");
    // The color of a code value, and the currency shown.
    assert!(xml.contains("<Color>#1A2B3C</Color>"));
    let display = tree.child("DisplayCurrency").unwrap();
    assert_eq!(display.child_names(), vec!["Currency"]);
    assert!(xml.contains("<DecimalSymbol>Period</DecimalSymbol>"));
    // Schedule options: an ending activity written empty when there is none.
    let options = project.child("ScheduleOptions").unwrap();
    let ending = options
        .child("MultipleFloatPathsEndingActivityObjectId")
        .unwrap();
    assert!(!ending.nil && ending.text.is_empty());
    assert!(xml.contains("<MultipleFloatPathsEndingActivityObjectId />"));
    assert_eq!(
        options.value("OutOfSequenceScheduleType"),
        Some("Retained Logic")
    );
    assert_eq!(
        options.value("ComputeTotalFloatType"),
        Some("Smallest of Start Float and Finish Float")
    );
}

#[test]
fn the_reader_reports_what_it_read_and_what_it_skipped() {
    let doc = built_document();
    let xml = write(&doc).unwrap();
    let (back, report) = read_with_report(&xml).unwrap();
    assert!(report.unhandled.is_empty(), "{:?}", report.unhandled);
    // WBS elements are counted without the projects' own nodes.
    let wbs = |d: &Document| {
        let t = d.table("PROJWBS").unwrap();
        t.rows
            .iter()
            .filter(|r| t.value(r, "proj_node_flag") != "Y")
            .count()
    };
    assert_eq!(report.elements.get("WBS"), Some(&wbs(&doc)));
    assert_eq!(wbs(&back), wbs(&doc));
    for key in [
        "Activity/Code",
        "Activity/UDF",
        "Resource/Code",
        "ProjectNote",
    ] {
        assert!(report.elements.contains_key(key), "{key}");
    }
    // Types and fields OpenXER has no place for are skipped and listed.
    let other = xml
        .replace(
            "<ProjectList>",
            "<UnknownDictionary><ObjectId>1</ObjectId></UnknownDictionary>\r\n  <ProjectList>",
        )
        .replacen(
            "<Activity>",
            "<Activity>\r\n<ActivityStep><Name>One</Name></ActivityStep>\
             <ActualLaborCost>12</ActualLaborCost>",
            1,
        );
    let (_, report) = read_with_report(&other).unwrap();
    assert_eq!(report.unhandled.get("UnknownDictionary"), Some(&1));
    assert_eq!(report.unhandled.get("Activity/ActivityStep"), Some(&1));
    assert_eq!(
        report.fields_not_read.get("Activity.ActualLaborCost"),
        Some(&1)
    );
}

/// User defined field values of every subject area the XML form carries
/// (activities, WBS, the project, resources, assignments and expenses), of
/// every data type it writes, come back from an export as they went out.
#[test]
fn udf_values_of_every_carried_subject_area_round_trip() {
    let mut doc = built_document();
    let first = |doc: &Document, table: &str, key: &str| {
        let t = doc.table(table).unwrap();
        t.value(&t.rows[0], key).to_string()
    };
    let assignment = first(&doc, "TASKRSRC", "taskrsrc_id");
    let expense = first(&doc, "PROJCOST", "cost_item_id");
    let rsrc = first(&doc, "RSRC", "rsrc_id");
    let wbs = {
        let t = doc.table("PROJWBS").unwrap();
        t.rows
            .iter()
            .find(|r| t.value(r, "proj_node_flag") != "Y")
            .map(|r| t.value(r, "wbs_id").to_string())
            .unwrap()
    };
    let project = "1".to_string();
    // Field id, subject area, data type and value, and the record.
    let rows = [
        "950|TASKRSRC|FT_MONEY|85.5",
        "951|TASKRSRC|FT_STATICTYPE|Green",
        "952|PROJCOST|FT_INT|4410",
        "953|PROJCOST|FT_END_DATE|2026-04-30 17:00",
        "954|PROJWBS|FT_FLOAT_2_DECIMALS|12.25",
        "955|PROJWBS|FT_STATICTYPE|Red",
        "956|PROJECT|FT_START_DATE|2026-01-12 08:00",
        "957|RSRC|FT_MONEY|1200",
    ];
    let records = [
        &assignment,
        &assignment,
        &expense,
        &expense,
        &wbs,
        &wbs,
        &project,
        &rsrc,
    ];
    let fields: Vec<[&str; 4]> = rows
        .iter()
        .map(|f| {
            let parts: Vec<&str> = f.split('|').collect();
            [parts[0], parts[1], parts[2], parts[3]]
        })
        .collect();
    for ([id, table, ty, value], fk) in fields.iter().zip(records) {
        doc.table_mut("UDFTYPE").add(&[
            ("udf_type_id", id.to_string()),
            ("table_name", table.to_string()),
            ("udf_type_name", format!("user_field_{id}")),
            ("udf_type_label", format!("Field {id}")),
            ("logical_data_type", ty.to_string()),
        ]);
        let pid = if *table == "RSRC" { "" } else { "1" };
        let column = match *ty {
            "FT_STATICTYPE" => "udf_text",
            "FT_END_DATE" | "FT_START_DATE" => "udf_date",
            _ => "udf_number",
        };
        doc.table_mut("UDFVALUE").add(&[
            ("udf_type_id", id.to_string()),
            ("fk_id", fk.to_string()),
            ("proj_id", pid.into()),
            (column, value.to_string()),
        ]);
    }
    let back = read(&write(&doc).unwrap()).unwrap();
    let cols = ["udf_type_id", "fk_id", "udf_text", "udf_number", "udf_date"];
    let ours = |d: &Document| -> Vec<Vec<String>> {
        projected(d, "UDFVALUE", &cols)
            .into_iter()
            .filter(|r| fields.iter().any(|f| f[0] == r[0]))
            .collect()
    };
    assert_eq!(ours(&back).len(), fields.len(), "every value came back");
    assert_eq!(ours(&doc), ours(&back));
    let areas = projected(&back, "UDFTYPE", &["udf_type_id", "table_name"]);
    for [id, table, ..] in &fields {
        assert!(
            areas.contains(&vec![id.to_string(), table.to_string()]),
            "field {id} keeps its subject area {table}"
        );
    }
}
