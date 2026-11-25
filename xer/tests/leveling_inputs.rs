//! XER rules for the resource leveling inputs and the relationship lag
//! calendar.
use openxer_engine::{features::PriorityField, options::LagCalendar};
use openxer_xer::{Document, ProjectData};

const CAL: &str = "(0||CalendarData()((0||DaysOfWeek()((0||1()())(0||2()((0||0(s|08:00|f|16:00)())))(0||3()((0||0(s|08:00|f|16:00)())))(0||4()((0||0(s|08:00|f|16:00)())))(0||5()((0||0(s|08:00|f|16:00)())))(0||6()((0||0(s|08:00|f|16:00)())))(0||7()())))(0||Exceptions()())))";

fn file(all_resources: &str, lag_calendar: &str) -> String {
    let mut s = String::new();
    s.push_str("ERMHDR\t25.12\t2026-11-04\tProject\tadmin\tAdmin\tdbxDatabaseNoName\tProject Management\tUSD\r\n");
    s.push_str("%T\tPROJECT\r\n%F\tproj_id\tproj_short_name\tclndr_id\tlast_recalc_date\tplan_start_date\r\n");
    s.push_str("%R\t1\tALPHA\t7\t2026-11-02 08:00\t2026-11-02 08:00\r\n");
    s.push_str("%T\tCALENDAR\r\n%F\tclndr_id\tdefault_flag\tclndr_name\tclndr_data\r\n");
    s.push_str(&format!("%R\t7\tY\tFive day\t{CAL}\r\n"));
    s.push_str("%T\tSCHEDOPTIONS\r\n%F\tschedoptions_id\tproj_id\tsched_calendar_on_relationship_lag\tlevel_over_alloc_pct\tlevel_within_float_flag\tlevel_keep_sched_date_flag\tlevel_all_rsrc_flag\tLevelPriorityList\r\n");
    s.push_str(&format!(
        "%R\t3\t1\t{lag_calendar}\t25\tN\tY\t{all_resources}\tACTVCODE_ORDER_1|seq_num*,ASC_BY_FIELD/ASC\u{7f}\u{7f}\r\n"
    ));
    s.push_str("%T\tPROJWBS\r\n%F\twbs_id\tproj_id\tseq_num\tproj_node_flag\twbs_short_name\twbs_name\tparent_wbs_id\r\n");
    s.push_str("%R\t10\t1\t0\tY\tALPHA\tAlpha\t\r\n");
    s.push_str("%T\tRSRC\r\n%F\trsrc_id\trsrc_short_name\trsrc_name\tclndr_id\r\n");
    s.push_str("%R\t50\tCREW\tCrew\t7\r\n%R\t51\tOTHER\tOther\t7\r\n");
    s.push_str("%T\tRSRCLEVELLIST\r\n%F\trsrc_level_list_id\tschedoptions_id\trsrc_id\r\n");
    s.push_str("%R\t1\t3\t50\r\n");
    s.push_str("%T\tRSRCRATE\r\n%F\trsrc_rate_id\trsrc_id\tmax_qty_per_hr\tstart_date\r\n");
    s.push_str("%R\t1\t50\t2\t2026-01-01 00:00\r\n%R\t2\t51\t3\t2026-01-01 00:00\r\n");
    s.push_str("%T\tACTVTYPE\r\n%F\tactv_code_type_id\tproj_id\tactv_code_type\r\n");
    s.push_str("%R\t60\t1\tOrder\r\n");
    s.push_str("%T\tACTVCODE\r\n%F\tactv_code_id\tactv_code_type_id\tseq_num\tshort_name\r\n");
    // Sequence order is 2, 10; text order of the short names is "10", "2".
    s.push_str("%R\t61\t60\t0\t2\r\n%R\t62\t60\t1\t10\r\n");
    s.push_str("%T\tTASK\r\n%F\ttask_id\tproj_id\twbs_id\tclndr_id\ttask_type\tstatus_code\ttask_code\ttask_name\tremain_drtn_hr_cnt\ttarget_drtn_hr_cnt\r\n");
    s.push_str("%R\t100\t1\t10\t7\tTT_Task\tTK_NotStart\tA100\tFirst\t8\t8\r\n");
    s.push_str("%R\t110\t1\t10\t7\tTT_Task\tTK_NotStart\tA110\tSecond\t8\t8\r\n");
    s.push_str("%T\tTASKACTV\r\n%F\ttask_id\tactv_code_type_id\tactv_code_id\tproj_id\r\n");
    s.push_str("%R\t100\t60\t61\t1\r\n%R\t110\t60\t62\t1\r\n");
    s.push_str("%E\r\n");
    s
}

fn import(text: &str) -> ProjectData {
    ProjectData::from_document(Document::parse(text).unwrap(), 1, "test".into()).unwrap()
}

#[test]
fn code_priorities_rank_by_short_name_text() {
    let p = import(&file("Y", "rcal_Predecessor"));
    assert!(matches!(
        p.resources.priorities[0].field,
        PriorityField::ActivityCode(60)
    ));
    let a = &p.resources.activity_codes[&(100, 60)];
    let b = &p.resources.activity_codes[&(110, 60)];
    // "10" (A110) ranks before "2" (A100), as the reference levels 369.xer.
    assert!(b < a, "{b} should rank before {a}");
}

#[test]
fn over_allocation_does_not_scale_limits() {
    let p = import(&file("Y", "rcal_Predecessor"));
    assert!(p.resources.unsupported.is_empty());
    assert_eq!(p.resources.limits[&50][0].1, 2.0);
    assert_eq!(p.resources.limits[&51][0].1, 3.0);
}

#[test]
fn only_selected_resources_are_leveled_without_level_all() {
    let p = import(&file("N", "rcal_Predecessor"));
    assert!(p.resources.unsupported.is_empty());
    assert_eq!(p.resources.selected, Some([50].into()));
    let p = import(&file("Y", "rcal_Predecessor"));
    assert_eq!(p.resources.selected, None);
}

#[test]
fn leveling_within_float_reads_the_float_to_preserve_in_minutes() {
    let text = file("Y", "rcal_Predecessor")
        .replace(
            "\tlevel_over_alloc_pct\tlevel_within_float_flag",
            "\tlevel_float_thrs_cnt\tlevel_over_alloc_pct\tlevel_within_float_flag",
        )
        .replace("\t25\tN\tY\tY\tACTVCODE", "\t16\t25\tY\tY\tY\tACTVCODE");
    let p = import(&text);
    assert!(p.resources.unsupported.is_empty());
    assert!(p.resources.within_float);
    assert_eq!(p.resources.preserve_float, 960);
    // The threshold is stored whatever the mode, and read only within float.
    let text = text.replace("\t16\t25\tY\tY\tY\t", "\t16\t25\tN\tY\tY\t");
    let p = import(&text);
    assert!(!p.resources.within_float);
    assert_eq!(p.resources.preserve_float, 0);
}

#[test]
fn project_default_lag_calendar_round_trips() {
    let p = import(&file("Y", "rcal_ProjDefault"));
    assert_eq!(p.model.options.lag_calendar, LagCalendar::ProjCal);
    let doc = p.updated_document();
    let t = doc.table("SCHEDOPTIONS").unwrap();
    assert_eq!(
        t.value(&t.rows[0], "sched_calendar_on_relationship_lag"),
        "rcal_ProjDefault"
    );
}
