//! Schedule Comparison: every change type on the starter project against an
//! edited copy and against a stored baseline, the report in every format,
//! the results window filters and jump-to, and software renders of the
//! dialog and the results window (gitignored `artifacts/g3/`).
use super::diff::{compare, Area, Kind2, Options};
use super::*;
use crate::headless_render::SoftwarePainter;
use crate::reports::test_support::{app, artifacts, crop_dialog, scheduled_starter, temp};
use crate::reports::{html, pdf};
use openxer_engine::model::LinkType;
use openxer_xer::Table;

fn row(doc: &Document, code: &str) -> usize {
    let t = doc.table("TASK").unwrap();
    t.rows
        .iter()
        .position(|r| t.value(r, "task_code") == code)
        .unwrap_or_else(|| panic!("{code}"))
}

fn set(doc: &mut Document, code: &str, field: &str, value: &str) {
    let r = row(doc, code);
    doc.table_mut("TASK").set(r, field, value.into());
}

fn get(doc: &Document, code: &str, field: &str) -> String {
    let r = row(doc, code);
    let t = doc.table("TASK").unwrap();
    t.value(&t.rows[r], field).to_string()
}

fn link_row(t: &Table, pred: i64, succ: i64) -> usize {
    t.rows
        .iter()
        .position(|r| {
            t.value(r, "pred_task_id") == pred.to_string()
                && t.value(r, "task_id") == succ.to_string()
        })
        .unwrap()
}

/// The scheduled starter with an "Area" code type, RB-1100 coded A1.
fn original_document() -> Document {
    let mut doc = scheduled_starter().updated_document();
    doc.table_mut("ACTVTYPE").add(&[
        ("actv_code_type_id", "7".into()),
        ("actv_code_type", "Area".into()),
        ("actv_code_type_scope", "AS_Project".into()),
        ("proj_id", "1".into()),
        ("seq_num", "1".into()),
    ]);
    for (id, short) in [(71, "A1"), (72, "A2")] {
        doc.table_mut("ACTVCODE").add(&[
            ("actv_code_id", id.to_string()),
            ("actv_code_type_id", "7".into()),
            ("short_name", short.into()),
            ("actv_code_name", format!("Area {short}")),
            ("seq_num", id.to_string()),
        ]);
    }
    doc.table_mut("TASKACTV").add(&[
        ("task_id", "1100".into()),
        ("actv_code_type_id", "7".into()),
        ("actv_code_id", "71".into()),
        ("proj_id", "1".into()),
    ]);
    // A second calendar for the calendar change.
    let cal = doc.table("CALENDAR").unwrap().rows[0].clone();
    let t = doc.table_mut("CALENDAR");
    t.rows.push(cal);
    let last = t.rows.len() - 1;
    t.set(last, "clndr_id", "2".into());
    t.set(last, "clndr_name", "Night shift".into());
    doc
}

fn shift_date(s: &str, days: i64) -> String {
    let m = openxer_xer::parse_absolute(s).unwrap() + days * 1440;
    openxer_xer::date_string(0, Some(m))
}

/// One edit of every kind the comparison reports.
fn revised_document(original: &Document) -> Document {
    let mut doc = original.clone();
    set(
        &mut doc,
        "RB-1010",
        "task_name",
        "Site investigation and survey",
    );
    set(&mut doc, "RB-1020", "target_drtn_hr_cnt", "64");
    set(&mut doc, "RB-1020", "remain_drtn_hr_cnt", "64");
    let start = get(&doc, "RB-1030", "early_start_date");
    set(&mut doc, "RB-1030", "cstr_type", "CS_MSOA");
    set(&mut doc, "RB-1030", "cstr_date", &start);
    set(&mut doc, "RB-1040", "clndr_id", "2");
    set(&mut doc, "RB-1050", "wbs_id", "30");
    set(&mut doc, "RB-1060", "complete_pct_type", "CP_Phys");
    set(&mut doc, "RB-1060", "phys_complete_pct", "50");
    for field in ["early_start_date", "early_end_date"] {
        let v = shift_date(&get(&doc, "RB-1070", field), 2);
        set(&mut doc, "RB-1070", field, &v);
    }
    let start = get(&doc, "RB-1080", "early_start_date");
    set(&mut doc, "RB-1080", "act_start_date", &start);
    set(&mut doc, "RB-1080", "status_code", "TK_Active");
    set(&mut doc, "RB-1090", "total_float_hr_cnt", "16");
    let codes = doc.table_mut("TASKACTV");
    codes.set(0, "actv_code_id", "72".into());
    // Delete the last activity (and its relationship), add a new one.
    let r = row(&doc, "RB-1130");
    let t = doc.table_mut("TASK");
    let added = t.rows[r].clone();
    t.rows.remove(r);
    let links = doc.table_mut("TASKPRED");
    let gone = link_row(links, 1120, 1130);
    links.rows.remove(gone);
    let t = doc.table_mut("TASK");
    t.rows.push(added);
    let last = t.rows.len() - 1;
    t.set(last, "task_id", "2000".into());
    t.set(last, "task_code", "RB-2000".into());
    t.set(last, "task_name", "Punch list walkdown".into());
    let links = doc.table_mut("TASKPRED");
    let template = links.rows[0].clone();
    for (id, pred, succ, kind) in [(90, 1120, 2000, "PR_FS"), (91, 1000, 1020, "PR_FS")] {
        links.rows.push(template.clone());
        let last = links.rows.len() - 1;
        links.set(last, "task_pred_id", id.to_string());
        links.set(last, "pred_task_id", pred.to_string());
        links.set(last, "task_id", succ.to_string());
        links.set(last, "pred_type", kind.into());
    }
    // Relationship deleted and relationship changed (type and lag).
    let gone = link_row(links, 1060, 1070);
    links.rows.remove(gone);
    let changed = link_row(links, 1030, 1050);
    links.set(changed, "pred_type", "PR_SS".into());
    links.set(changed, "lag_hr_cnt", "8".into());
    // Data date one day later.
    let t = doc.table_mut("PROJECT");
    let dd = t.value(&t.rows[0], "last_recalc_date").to_string();
    t.set(0, "last_recalc_date", shift_date(&dd, 1));
    doc
}

fn projects() -> (ProjectData, ProjectData) {
    let original = original_document();
    let revised = revised_document(&original);
    (
        ProjectData::from_document(original, 1, "original".into()).unwrap(),
        ProjectData::from_document(revised, 1, "revised".into()).unwrap(),
    )
}

fn changes<'c>(
    c: &'c super::diff::Comparison,
    field: &str,
) -> Vec<(&'c str, &'c super::diff::FieldChange)> {
    c.activities
        .iter()
        .flat_map(|a| a.changes.iter().map(move |f| (a.code.as_str(), f)))
        .filter(|(_, f)| f.field == field)
        .collect()
}

fn only(c: &super::diff::Comparison, field: &str, code: &str) -> super::diff::FieldChange {
    let found = changes(c, field);
    assert_eq!(found.len(), 1, "{field}: {found:#?}");
    assert_eq!(found[0].0, code, "{field}");
    found[0].1.clone()
}

#[test]
fn edited_copy_reports_each_change_exactly_once() {
    let (old, new) = projects();
    let c = compare(&old, "Original", &new, "Revised", &Options::default());
    let name = only(&c, "Activity Name", "RB-1010");
    assert_eq!(name.old.text(), "Site investigation & survey");
    assert_eq!(name.new.text(), "Site investigation and survey");
    let od = only(&c, "Original Duration", "RB-1020");
    assert_eq!(od.variance, Some(2.));
    only(&c, "Remaining Duration", "RB-1020");
    let cstr = only(&c, "Primary Constraint", "RB-1030");
    assert_eq!(cstr.old.text(), "");
    assert!(!cstr.new.text().is_empty());
    only(&c, "Primary Constraint Date", "RB-1030");
    let cal = only(&c, "Calendar", "RB-1040");
    assert_eq!(cal.new.text(), "Night shift");
    let wbs = only(&c, "WBS", "RB-1050");
    assert_eq!((wbs.old.text(), wbs.new.text()), ("RB.2", "RB.3"));
    let pct = only(&c, "Activity % Complete", "RB-1060");
    assert_eq!(pct.variance, Some(50.));
    let finish = only(&c, "Finish", "RB-1070");
    assert_eq!(finish.variance, Some(2.));
    assert_eq!(finish.variance_text, "+2d");
    let starts = changes(&c, "Start");
    assert_eq!(
        starts.iter().map(|s| s.0).collect::<Vec<_>>(),
        vec!["RB-1070", "RB-1080"],
        "the date move and the new actual start"
    );
    only(&c, "Actual Start", "RB-1080");
    only(&c, "Activity Status", "RB-1080");
    let tf = only(&c, "Total Float", "RB-1090");
    assert_eq!(tf.new.text().trim_end_matches('d'), "2");
    let code = only(&c, "Area", "RB-1100");
    assert_eq!((code.old.text(), code.new.text()), ("A1", "A2"));
    // Nothing else changed on any activity.
    let total: usize = c.activities.iter().map(|a| a.changes.len()).sum();
    assert_eq!(total, 15, "{:#?}", c.activities);
    // Added and deleted activities.
    let added: Vec<_> = c
        .activities
        .iter()
        .filter(|a| a.kind == Kind2::Added)
        .collect();
    let deleted: Vec<_> = c
        .activities
        .iter()
        .filter(|a| a.kind == Kind2::Deleted)
        .collect();
    assert_eq!(added.len(), 1);
    assert_eq!(added[0].code, "RB-2000");
    assert_eq!(added[0].facts.len(), 3);
    assert_eq!(deleted.len(), 1);
    assert_eq!(deleted[0].code, "RB-1130");
    assert_eq!(c.count(Kind2::Changed), 10);
    // Relationships: two deleted (one with the activity), two added, one changed.
    let links = |k| {
        c.links
            .iter()
            .filter(|l| l.kind == k)
            .map(|l| format!("{}>{}", l.pred, l.succ))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        links(Kind2::Added),
        vec!["RB-1000>RB-1020", "RB-1120>RB-2000"]
    );
    assert_eq!(
        links(Kind2::Deleted),
        vec!["RB-1060>RB-1070", "RB-1120>RB-1130"]
    );
    assert_eq!(links(Kind2::Changed), vec!["RB-1030>RB-1050"]);
    let changed = c.links.iter().find(|l| l.kind == Kind2::Changed).unwrap();
    assert_eq!(changed.old.as_ref().unwrap().link_type, LinkType::FS);
    assert_eq!(changed.new.as_ref().unwrap().link_type, LinkType::SS);
    assert_eq!(changed.new.as_ref().unwrap().lag, 8 * 60);
    // Project: the data date moved one day, nothing else at project level.
    assert_eq!(
        c.project
            .iter()
            .map(|f| f.field.as_str())
            .collect::<Vec<_>>(),
        vec!["Data Date"]
    );
    assert_eq!(c.project[0].variance, Some(1.));
    assert!(c.wbs.is_empty());
    // Same project against itself: nothing.
    let none = compare(&old, "a", &old, "b", &Options::default());
    assert!(none.is_empty(), "{none:#?}");
}

/// Durations on an activity with a 10-hour calendar: the variance and the
/// exported numbers are in that calendar's days, the same days as the text.
#[test]
fn duration_variance_uses_the_activity_calendar_days() {
    let mut original = original_document();
    let night = {
        let t = original.table("CALENDAR").unwrap();
        t.rows
            .iter()
            .position(|r| t.value(r, "clndr_id") == "2")
            .unwrap()
    };
    original
        .table_mut("CALENDAR")
        .set(night, "day_hr_cnt", "10".into());
    set(&mut original, "RB-1020", "clndr_id", "2");
    set(&mut original, "RB-1020", "target_drtn_hr_cnt", "50");
    let mut revised = original.clone();
    set(&mut revised, "RB-1020", "target_drtn_hr_cnt", "60");
    let old = ProjectData::from_document(original, 1, "original".into()).unwrap();
    let new = ProjectData::from_document(revised, 1, "revised".into()).unwrap();
    let c = compare(&old, "a", &new, "b", &Options::default());
    let od = only(&c, "Original Duration", "RB-1020");
    assert_eq!((od.old.text(), od.new.text()), ("5d", "6d"));
    assert_eq!(od.variance, Some(1.));
    assert_eq!(od.variance_text, "+1d");
    let number = |cell: &crate::reports::doc::Cell| match cell {
        crate::reports::doc::Cell::Number { value, .. } => *value,
        other => panic!("{other:?}"),
    };
    assert_eq!((number(&od.old), number(&od.new)), (5., 6.));
}

/// Two values that differ by less than the display shows are not listed as
/// a change with identical old and new text.
#[test]
fn values_that_read_the_same_are_not_changes() {
    let original = original_document();
    let mut revised = original.clone();
    set(&mut revised, "RB-1020", "target_drtn_hr_cnt", "48.01");
    let old = ProjectData::from_document(original, 1, "original".into()).unwrap();
    let new = ProjectData::from_document(revised, 1, "revised".into()).unwrap();
    let c = compare(&old, "a", &new, "b", &Options::default());
    for (code, f) in c
        .activities
        .iter()
        .flat_map(|a| a.changes.iter().map(move |f| (a.code.as_str(), f)))
    {
        assert_ne!(f.old.text(), f.new.text(), "{code} {}: {f:?}", f.field);
    }
}

#[test]
fn options_limit_the_areas_compared_and_group_the_report() {
    let (old, new) = projects();
    let mut options = Options::default();
    options.areas.retain(|a| *a == Area::Dates);
    let c = compare(&old, "a", &new, "b", &options);
    assert!(c.links.is_empty() && c.project.is_empty());
    assert!(c.activities.iter().all(|a| a.kind == Kind2::Changed));
    assert!(c
        .activities
        .iter()
        .flat_map(|a| &a.changes)
        .all(|f| f.area == Area::Dates));
    let grouped = report::report(&c);
    let changes = grouped
        .sections
        .iter()
        .find(|s| s.heading == "Activity Changes")
        .unwrap();
    assert_eq!(changes.tables.len(), 1);
    assert!(matches!(
        changes.tables[0].rows[0].tone,
        crate::reports::doc::Tone::Band(_)
    ));
    options.group_by_activity = false;
    let c = compare(&old, "a", &new, "b", &options);
    let flat = report::report(&c);
    let changes = flat
        .sections
        .iter()
        .find(|s| s.heading == "Activity Changes")
        .unwrap();
    assert_eq!(changes.tables[0].name, "Dates");
    let quiet = flat.sections.last().unwrap();
    assert!(quiet.paragraphs[1].starts_with("Not compared:"));
}

#[test]
fn report_leads_with_finish_movement_and_writes_every_format() {
    let (old, new) = projects();
    let c = compare(&old, "Original", &new, "Revised", &Options::default());
    let r = report::report(&c);
    assert_eq!(r.sections[0].heading, "Summary");
    assert!(r
        .sections
        .iter()
        .any(|s| s.heading == "Largest Finish Date Movements"));
    let slips = r
        .sections
        .iter()
        .find(|s| s.heading == "Largest Finish Date Movements")
        .unwrap();
    assert_eq!(slips.tables[0].rows[0].cells[0].text(), "RB-1070");
    let dir = temp("formats");
    for format in Output::ALL {
        let path = dir.join(format!("cmp.{}", format.extension()));
        doc::write(&r, format, &path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        match format {
            Output::Html => {
                let text = String::from_utf8(bytes).unwrap();
                html::well_formed(&text).unwrap();
                assert!(text.contains("Site investigation and survey"));
                assert!(text.contains("The following Activities have been added to the project"));
            }
            Output::Pdf => {
                let pages = pdf::check(&bytes).unwrap();
                assert!(pages >= 2, "{pages} pages");
            }
            Output::Excel => {
                let wb = openxer_interchange::xlsx_import::read_workbook(&path).unwrap();
                assert_eq!(wb.sheets[0].name, "Summary");
                let added = &wb.sheets[wb.sheet("Added Activities").unwrap()];
                assert!(added
                    .rows
                    .iter()
                    .any(|r| r.first().is_some_and(|c| c.text() == "RB-2000")));
                let changes = &wb.sheets[wb.sheet("Activity Changes").unwrap()];
                let dates = changes
                    .rows
                    .iter()
                    .flatten()
                    .filter(|c| matches!(c, openxer_interchange::xlsx_import::Cell::Date(_)))
                    .count();
                assert!(dates >= 4, "dates are real dates: {dates}");
            }
            Output::Csv => {
                let text = String::from_utf8(bytes).unwrap();
                assert!(text
                    .lines()
                    .any(|l| l.starts_with("RB-2000,Punch list walkdown")));
            }
        }
    }
}

#[test]
fn baseline_comparison_runs_from_the_dialog_and_jumps_to_activities() {
    let dir = temp("baseline");
    let (mut app, _ctx) = app(&dir);
    app.add_baseline_copy(1.).unwrap();
    // Edit the open project after the baseline was taken.
    let i = 2;
    let task_id = app.project.model.activities[i].task_id;
    app.project.details.get_mut(&task_id).unwrap().name = "Complete detailed design rev B".into();
    app.project
        .details
        .get_mut(&task_id)
        .unwrap()
        .original_duration = 64 * 60;
    app.project.model.activities[i].rem = 64 * 60;
    app.open_schedule_comparison();
    let d = app.compare.dialog.as_ref().unwrap();
    assert!(
        matches!(d.sources[d.old].0, Source::Baseline(_)),
        "the baseline is the default original"
    );
    assert_eq!(d.sources[d.new].0, Source::Open);
    let path = dir.join("out.html");
    {
        let d = app.compare.dialog.as_mut().unwrap();
        d.save_file = true;
        d.path = path.display().to_string();
    }
    app.run_schedule_comparison().unwrap();
    assert!(app.compare.dialog.is_none());
    html::well_formed(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let r = app.compare.results.as_mut().unwrap();
    let c = &r.comparison;
    assert_eq!(c.count(Kind2::Changed), 1);
    assert_eq!(c.count(Kind2::Added) + c.count(Kind2::Deleted), 0);
    let fields: Vec<&str> = c.activities[0]
        .changes
        .iter()
        .map(|f| f.field.as_str())
        .collect();
    assert_eq!(
        fields,
        vec!["Activity Name", "Original Duration", "Remaining Duration"]
    );
    assert!(c.links.is_empty());
    // Filters: Added shows nothing, Changed both rows, Find narrows.
    r.show = 1;
    assert!(r.visible().is_empty());
    r.show = 3;
    // The baseline is a renamed copy, so its Project ID also differs.
    assert_eq!(r.visible().len(), 3 + c.project.len());
    assert!(c.project.iter().all(|f| f.field.starts_with("Project")));
    r.find = "original dur".into();
    assert_eq!(r.visible().len(), 1);
    r.area = 1 + Area::ACTIVITY_FIELDS
        .iter()
        .position(|a| *a == Area::General)
        .unwrap();
    assert!(r.visible().is_empty());
    let code = c.activities[0].code.clone();
    app.selected = Some(0);
    assert!(app.jump_to_code(&code));
    assert_eq!(app.selected, Some(i));
    assert!(!app.jump_to_code("NOPE"));
    // Export from the results window.
    let xlsx = dir.join("out.xlsx");
    app.export_comparison(Output::Excel, &xlsx).unwrap();
    openxer_interchange::xlsx_import::read_workbook(&xlsx).unwrap();
    // Two identical sources are refused.
    app.open_schedule_comparison();
    app.compare.dialog.as_mut().unwrap().old = 0;
    app.compare.dialog.as_mut().unwrap().new = 0;
    assert!(app.run_schedule_comparison().is_err());
}

#[test]
fn xer_file_projects_are_sources() {
    let dir = temp("file");
    let (original, revised) = (original_document(), revised_document(&original_document()));
    let path = dir.join("rev.xer");
    std::fs::write(&path, revised.text()).unwrap();
    let file = load_file(&path, 1).unwrap();
    let old = ProjectData::from_document(original, 1, "o".into()).unwrap();
    let c = compare(&old, "o", &file, "f", &Options::default());
    assert_eq!(c.count(Kind2::Added), 1);
    assert_eq!(c.count(Kind2::Deleted), 1);
}

#[test]
fn render_comparison_dialog_and_results() {
    let dir = temp("render");
    let (mut app, ctx) = app(&dir);
    app.add_baseline_copy(1.).unwrap();
    let task_id = app.project.model.activities[2].task_id;
    app.project.details.get_mut(&task_id).unwrap().name = "Complete detailed design (IFC)".into();
    let mut painter = SoftwarePainter::default();
    let mut time = 0.;
    for _ in 0..2 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    // Open it from the Tools menu.
    painter.click_action(&mut app, &ctx, &mut time, "menu_bar_tools");
    painter.click_action(&mut app, &ctx, &mut time, "menu_schedule_comparison");
    assert!(app.compare.dialog.is_some());
    let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    let out = artifacts();
    crop_dialog(&image, &ctx, "compare")
        .save(out.join("schedule-comparison.png"))
        .unwrap();
    // Toggle an option by clicking it, then compare.
    painter.click_action(&mut app, &ctx, &mut time, "compare_group_by_activity");
    assert!(
        !app.compare
            .dialog
            .as_ref()
            .unwrap()
            .options
            .group_by_activity
    );
    painter.click_action(&mut app, &ctx, &mut time, "compare_group_by_activity");
    painter.click_action(&mut app, &ctx, &mut time, "compare_run");
    assert!(app.compare.dialog.is_none(), "{:?}", app.status);
    assert!(app.compare.results.is_some());
    for _ in 0..2 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    crop_dialog(&image, &ctx, "compare_results")
        .save(out.join("schedule-comparison-results.png"))
        .unwrap();
    // Select the change row, then Go To.
    let first = app
        .compare
        .results
        .as_ref()
        .unwrap()
        .rows
        .iter()
        .position(|r| r.jump.is_some())
        .unwrap();
    painter.click_action(
        &mut app,
        &ctx,
        &mut time,
        &format!("compare_rows_row_{first}"),
    );
    assert_eq!(app.compare.results.as_ref().unwrap().selected, Some(first));
    painter.click_action(&mut app, &ctx, &mut time, "compare_results_goto");
    assert_eq!(app.selected, Some(2));
    painter.click_action(&mut app, &ctx, &mut time, "compare_results_close");
    assert!(app.compare.results.is_none());
}
