//! Project Checker: each rule on a project built to break it, the clean
//! starter, configuration round trip, the report in every format, the dialog
//! flow with jump-to, and software renders of the dialog and results.
use super::checker::{self, Config, GroupBy, ProjectResult, Rule};
use super::checker_ui::Target;
use super::doc::{self, Output};
use super::test_support::{app, artifacts, crop_dialog, scheduled_starter, temp};
use super::{html, pdf};
use crate::headless_render::SoftwarePainter;
use openxer_xer::{Document, ProjectData};
use std::collections::BTreeMap;

fn task_row(doc: &Document, code: &str) -> usize {
    let t = doc.table("TASK").unwrap();
    t.rows
        .iter()
        .position(|r| t.value(r, "task_code") == code)
        .unwrap()
}

fn set(doc: &mut Document, code: &str, field: &str, value: &str) {
    let r = task_row(doc, code);
    doc.table_mut("TASK").set(r, field, value.into());
}

fn get(doc: &Document, code: &str, field: &str) -> String {
    let r = task_row(doc, code);
    let t = doc.table("TASK").unwrap();
    t.value(&t.rows[r], field).to_string()
}

fn add_link(doc: &mut Document, id: i64, pred: i64, succ: i64, kind: &str, lag_hours: i64) {
    let t = doc.table_mut("TASKPRED");
    let template = t.rows[0].clone();
    t.rows.push(template);
    let last = t.rows.len() - 1;
    for (f, v) in [
        ("task_pred_id", id.to_string()),
        ("pred_task_id", pred.to_string()),
        ("task_id", succ.to_string()),
        ("pred_type", kind.to_string()),
        ("lag_hr_cnt", lag_hours.to_string()),
    ] {
        t.set(last, f, v);
    }
}

/// The scheduled starter with one or more breaks of every rule.
fn broken() -> ProjectData {
    let mut doc = scheduled_starter().updated_document();
    let dd = {
        let t = doc.table("PROJECT").unwrap();
        t.value(&t.rows[0], "last_recalc_date").to_string()
    };
    let shift = |s: &str, days: i64| {
        openxer_xer::date_string(
            0,
            Some(openxer_xer::parse_absolute(s).unwrap() + days * 1440),
        )
    };
    let start = get(&doc, "RB-1030", "early_start_date");
    set(&mut doc, "RB-1030", "cstr_type", "CS_MANDSTART");
    set(&mut doc, "RB-1030", "cstr_date", &start);
    set(&mut doc, "RB-1040", "cstr_type", "CS_MSOA");
    set(&mut doc, "RB-1040", "cstr_date", &start);
    add_link(&mut doc, 101, 1010, 1040, "PR_FS", -8);
    add_link(&mut doc, 102, 1020, 1060, "PR_SF", 0);
    add_link(&mut doc, 103, 1020, 1070, "PR_SS", 0);
    add_link(&mut doc, 104, 1050, 1110, "PR_FS", 80);
    set(&mut doc, "RB-1090", "remain_drtn_hr_cnt", "480");
    set(&mut doc, "RB-1090", "target_drtn_hr_cnt", "480");
    set(&mut doc, "RB-1110", "remain_drtn_hr_cnt", "0");
    set(&mut doc, "RB-1110", "target_drtn_hr_cnt", "0");
    set(&mut doc, "RB-1100", "total_float_hr_cnt", "-16");
    set(&mut doc, "RB-1080", "total_float_hr_cnt", "400");
    // Actual start after the data date on a started activity.
    set(&mut doc, "RB-1000", "act_start_date", &shift(&dd, 3));
    set(&mut doc, "RB-1000", "status_code", "TK_Active");
    // A not-started activity forecast before the data date.
    set(&mut doc, "RB-1020", "early_start_date", &shift(&dd, -2));
    // Out of sequence: RB-1120 started, its predecessors not finished.
    set(&mut doc, "RB-1120", "act_start_date", &dd);
    set(&mut doc, "RB-1120", "status_code", "TK_Active");
    // One activity code type; only RB-1050 has a value.
    doc.table_mut("ACTVTYPE").add(&[
        ("actv_code_type_id", "7".into()),
        ("actv_code_type", "Crew".into()),
        ("actv_code_type_scope", "AS_Project".into()),
        ("proj_id", "1".into()),
    ]);
    doc.table_mut("ACTVCODE").add(&[
        ("actv_code_id", "71".into()),
        ("actv_code_type_id", "7".into()),
        ("short_name", "C1".into()),
    ]);
    for (task, value) in [(1050, 71), (1060, 999)] {
        doc.table_mut("TASKACTV").add(&[
            ("task_id", task.to_string()),
            ("actv_code_type_id", "7".into()),
            ("actv_code_id", value.to_string()),
            ("proj_id", "1".into()),
        ]);
    }
    // Records that point at missing records.
    add_link(&mut doc, 105, 7777, 1050, "PR_FS", 0);
    doc.table_mut("TASKRSRC").add(&[
        ("taskrsrc_id", "1".into()),
        ("task_id", "8888".into()),
        ("proj_id", "1".into()),
        ("rsrc_id", "".into()),
    ]);
    doc.table_mut("TASKRSRC").add(&[
        ("taskrsrc_id", "2".into()),
        ("task_id", "1060".into()),
        ("proj_id", "1".into()),
        ("rsrc_id", "4242".into()),
    ]);
    doc.table_mut("TASKMEMO").add(&[
        ("memo_id", "1".into()),
        ("task_id", "1070".into()),
        ("memo_type_id", "55".into()),
        ("proj_id", "1".into()),
        ("task_memo", "<p>note</p>".into()),
    ]);
    // Duplicates: an Activity ID, a relationship and a WBS code.
    set(&mut doc, "RB-1130", "task_code", "RB-1120");
    add_link(&mut doc, 106, 1000, 1010, "PR_FS", 0);
    doc.table_mut("PROJWBS").add(&[
        ("wbs_id", "50".into()),
        ("proj_id", "1".into()),
        ("wbs_short_name", "RB.4".into()),
        ("wbs_name", "Commissioning copy".into()),
        ("seq_num", "50".into()),
    ]);
    let mut p = ProjectData::from_document(doc, 1, "broken".into()).unwrap();
    // A WBS and a calendar the activity rows point at but the file lacks.
    let (wbs_row, cal_row) = (
        task_row(&p.document, "RB-1070"),
        task_row(&p.document, "RB-1080"),
    );
    p.document
        .table_mut("TASK")
        .set(wbs_row, "wbs_id", "999".into());
    p.document
        .table_mut("TASK")
        .set(cal_row, "clndr_id", "999".into());
    p
}

fn found(results: &[checker::RuleResult]) -> BTreeMap<Rule, Vec<String>> {
    results
        .iter()
        .map(|r| (r.rule, r.findings.iter().map(|f| f.code.clone()).collect()))
        .collect()
}

#[test]
fn clean_starter_only_has_its_open_ends() {
    let p = scheduled_starter();
    let results = checker::run(&p, &Config::default());
    assert_eq!(results.len(), Rule::ALL.len());
    let f = found(&results);
    assert_eq!(f[&Rule::MissingPredecessor], vec!["RB-1000"]);
    assert_eq!(f[&Rule::MissingSuccessor], vec!["RB-1130"]);
    for (rule, codes) in &f {
        if !matches!(rule, Rule::MissingPredecessor | Rule::MissingSuccessor) {
            assert!(codes.is_empty(), "{rule:?}: {codes:?}");
        }
    }
    let codes = results
        .iter()
        .find(|r| r.rule == Rule::MissingCode)
        .unwrap();
    assert_eq!(codes.checked, 0, "no code types, nothing to check");
}

#[test]
fn every_rule_finds_its_break() {
    let p = broken();
    let f = found(&checker::run(&p, &Config::default()));
    let expect: &[(Rule, &[&str])] = &[
        (Rule::MissingPredecessor, &["RB-1000"]),
        (Rule::MissingSuccessor, &["RB-1120"]),
        (Rule::HardConstraint, &["RB-1030"]),
        (Rule::SoftConstraint, &["RB-1040"]),
        (Rule::NegativeLag, &["RB-1040"]),
        (Rule::LongLag, &["RB-1110"]),
        (Rule::StartToFinish, &["RB-1060"]),
        (Rule::NonFinishToStart, &["RB-1070"]),
        (Rule::LongDuration, &["RB-1090"]),
        (Rule::ZeroDurationTask, &["RB-1110"]),
        (Rule::NegativeFloat, &["RB-1100"]),
        (Rule::LargeFloat, &["RB-1080"]),
        (Rule::ActualAfterDataDate, &["RB-1000"]),
        (Rule::ForecastBeforeDataDate, &["RB-1020"]),
        (Rule::OutOfSequence, &["RB-1120"]),
        (Rule::RelationshipMissingActivity, &["RB-1050"]),
        (Rule::ActivityMissingWbs, &["RB-1070"]),
        (Rule::ActivityMissingCalendar, &["RB-1080"]),
        (Rule::AssignmentMissingActivity, &[""]),
        (Rule::AssignmentMissingResource, &["RB-1060"]),
        (Rule::CodeAssignmentMissingValue, &["RB-1060"]),
        (Rule::NotebookMissingTopic, &["RB-1070"]),
        (Rule::DuplicateActivityId, &["RB-1120", "RB-1120"]),
        (Rule::DuplicateRelationship, &["RB-1010"]),
        (Rule::DuplicateWbsCode, &["", ""]),
    ];
    for (rule, codes) in expect {
        let mut got = f[rule].clone();
        got.sort();
        let mut want: Vec<String> = codes.iter().map(|s| s.to_string()).collect();
        want.sort();
        assert_eq!(got, want, "{rule:?}");
    }
    // Missing codes: every activity but RB-1050 (RB-1060's value is gone).
    let missing = &f[&Rule::MissingCode];
    assert_eq!(missing.len(), p.model.activities.len() - 1);
    assert!(!missing.contains(&"RB-1050".to_string()));
}

#[test]
fn configuration_limits_and_round_trip() {
    let p = broken();
    let mut config = Config {
        priority_only: true,
        ..Config::default()
    };
    let results = checker::run(&p, &config);
    assert!(results.iter().all(|r| r.rule.priority()));
    config.priority_only = false;
    config.long_lag_days = 20.;
    config.large_float_days = 60.;
    config.rules.remove(&Rule::MissingCode);
    let f = found(&checker::run(&p, &config));
    assert!(f[&Rule::LongLag].is_empty());
    assert!(f[&Rule::LargeFloat].is_empty());
    assert!(!f.contains_key(&Rule::MissingCode));
    config.group_by = GroupBy::Activity;
    config.errors_only = true;
    let back = Config::from_json(&config.to_json());
    assert_eq!(back, config);
}

#[test]
fn report_lists_checks_and_findings_in_every_format() {
    let p = broken();
    let mut config = Config::default();
    let projects = vec![ProjectResult {
        label: "RIVERBEND".into(),
        results: checker::run(&p, &config),
    }];
    let r = checker::report(&projects, &config);
    assert_eq!(r.sections[0].heading, "Summary");
    assert!(r
        .sections
        .iter()
        .any(|s| s.heading.starts_with("Hard constraints (1)")));
    config.group_by = GroupBy::Activity;
    config.errors_only = true;
    let grouped = checker::report(&projects, &config);
    assert!(grouped
        .sections
        .iter()
        .any(|s| s.heading == "Findings by Activity"));
    let summary = &grouped.sections[0].tables[0];
    assert!(summary
        .rows
        .iter()
        .all(|row| row.cells[4].text() != "Passed"));
    let dir = temp("checker-formats");
    for format in Output::ALL {
        let path = dir.join(format!("check.{}", format.extension()));
        doc::write(&r, format, &path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        match format {
            Output::Html => html::well_formed(&String::from_utf8(bytes).unwrap()).unwrap(),
            Output::Pdf => assert!(pdf::check(&bytes).unwrap() >= 1),
            Output::Excel => {
                let wb = openxer_interchange::xlsx_import::read_workbook(&path).unwrap();
                assert!(wb.sheet("Hard constraints").is_some());
            }
            Output::Csv => {
                let text = String::from_utf8(bytes).unwrap();
                assert!(text.contains("RB-1030"));
            }
        }
    }
}

#[test]
fn checker_dialog_runs_projects_and_jumps() {
    let dir = temp("checker-dialog");
    let (mut app, ctx) = app(&dir);
    // A second workspace project to add to the list.
    let mut other = scheduled_starter();
    other.source_name = "Other".into();
    app.store.save(&other).unwrap();
    app.open_project_checker();
    {
        let d = app.reports.checker.as_mut().unwrap();
        assert_eq!(d.selected[0].0, Target::Open);
        assert_eq!(d.available.len(), 1);
        d.path = dir.join("check.xlsx").display().to_string();
    }
    let mut painter = SoftwarePainter::default();
    let mut time = 0.;
    for _ in 0..2 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    painter.click_action(&mut app, &ctx, &mut time, "checker_available_row_0");
    painter.click_action(&mut app, &ctx, &mut time, "checker_add");
    assert_eq!(app.reports.checker.as_ref().unwrap().selected.len(), 2);
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    crop_dialog(&image, &ctx, "checker")
        .save(artifacts().join("project-checker.png"))
        .unwrap();
    painter.click_action(&mut app, &ctx, &mut time, "checker_run");
    assert!(
        app.reports.checker.is_none(),
        "{:?}",
        app.reports.checker.as_ref().map(|d| d.error.clone())
    );
    let r = app.reports.checker_results.as_ref().unwrap();
    assert_eq!(r.projects.len(), 2);
    assert_eq!(r.open, Some(0));
    openxer_interchange::xlsx_import::read_workbook(&dir.join("check.xlsx")).unwrap();
    for _ in 0..2 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    crop_dialog(&image, &ctx, "checker_results")
        .save(artifacts().join("project-checker-results.png"))
        .unwrap();
    // The second check (no successors) of the open project: jump to RB-1130.
    painter.click_action(&mut app, &ctx, &mut time, "checker_result_rows_row_1");
    painter.click_action(&mut app, &ctx, &mut time, "checker_findings_row_0");
    painter.click_action(&mut app, &ctx, &mut time, "checker_results_goto");
    assert_eq!(app.selected, Some(13));
    // Exports.
    let pdf_path = dir.join("check.pdf");
    app.export_checker(Output::Pdf, &pdf_path).unwrap();
    pdf::check(&std::fs::read(&pdf_path).unwrap()).unwrap();
    painter.click_action(&mut app, &ctx, &mut time, "checker_results_close");
    assert!(app.reports.checker_results.is_none());
}

/// With Report Errors Only on, the summary still counts every check that
/// ran (and every one that found problems), not just the listed rows.
#[test]
fn checker_summary_counts_every_check_with_errors_only_on() {
    let dir = temp("checker-errors-only");
    let (mut app, ctx) = app(&dir);
    app.open_project_checker();
    {
        let d = app.reports.checker.as_mut().unwrap();
        d.config.errors_only = true;
        d.path = String::new();
    }
    let mut painter = SoftwarePainter::default();
    let mut time = 0.;
    for _ in 0..2 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    painter.click_action(&mut app, &ctx, &mut time, "checker_run");
    let r = app.reports.checker_results.as_ref().expect("results");
    assert!(r.config.errors_only);
    let all: Vec<_> = r.projects.iter().flat_map(|p| p.results.iter()).collect();
    let found = all.iter().filter(|x| !x.passed()).count();
    assert!(
        r.rows().len() < all.len(),
        "some checks pass on the starter"
    );
    assert_eq!(r.rows().len(), found, "only the problems are listed");
    assert!(
        r.summary().starts_with(&format!(
            "{} checks run; {found} found problems.",
            all.len()
        )),
        "{}",
        r.summary()
    );
}
