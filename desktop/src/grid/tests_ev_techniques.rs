//! WBS earned value techniques set through the shared setters
//! (`openxer_xer::edit::earned_value`) and applied by the earned value
//! columns, checked against hand-computed values, plus the WBS view's
//! Earned Value tab: a click is one undo step and the fields survive an XER
//! round trip.
//!
//! The fixture is `progressed_starter`: activity 1 has a budget at
//! completion of $1,200 (24 labor hours), $400 actual cost, $800 remaining
//! cost, and the data date one of its three planned days in, so planned
//! value is $400 (a third) and activity percent complete is a third.
use super::tests_h3::progressed_starter;
use super::*;
use crate::qa_sweep::Driver;
use columns::Value;
use openxer_xer::edit::earned_value as ev;
use openxer_xer::{Document, ProjectData};

fn get(p: &ProjectData, id: &str, i: usize) -> Value {
    let ix = columns::Index::new((false, 0.));
    let catalog = columns::catalog(p);
    let c = columns::find(&catalog, id).unwrap_or_else(|| panic!("no column {id}"));
    columns::value(p, &ix, c, i)
}

fn n(p: &ProjectData, id: &str, i: usize) -> f64 {
    get(p, id, i)
        .number()
        .unwrap_or_else(|| panic!("{id} has no number"))
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.01
}

const I: usize = 1;

fn wbs_of(p: &ProjectData) -> i64 {
    p.details[&p.model.activities[I].task_id].wbs
}

fn set(
    p: &mut ProjectData,
    perf: Option<&str>,
    pct: Option<&str>,
    etc: Option<&str>,
    pf: Option<&str>,
) {
    let w = wbs_of(p);
    ev::set(p, w, perf, pct, etc, pf).unwrap();
}

#[test]
fn performance_techniques_give_the_hand_computed_earned_value() {
    let mut p = progressed_starter();
    // Defaults: activity percent complete, ETC = remaining cost.
    let t = ev::get(&p, wbs_of(&p)).unwrap();
    assert_eq!(
        (t.performance.as_str(), t.to_complete.as_str()),
        ("EC_Cmp_pct", "EE_Rem_hr")
    );
    assert!(near(n(&p, "earned_value_cost", I), 400.));
    assert!(near(n(&p, "estimate_to_complete", I), 800.));

    // 0/100: nothing earned until finished. EV 0, SV = 0 - 400, CV = 0 - 400.
    set(&mut p, Some("0/100"), None, None, None);
    assert!(near(n(&p, "earned_value_cost", I), 0.));
    assert!(near(n(&p, "schedule_variance", I), -400.));
    assert!(near(n(&p, "cost_variance", I), -400.));
    assert!(near(n(&p, "schedule_performance_index", I), 0.));
    assert!(near(n(&p, "cost_performance_index", I), 0.));

    // 50/50: half once started. EV 600, SPI = 600 / 400, CPI = 600 / 400.
    set(&mut p, Some("50/50"), None, None, None);
    assert!(near(n(&p, "earned_value_cost", I), 600.));
    assert!(near(n(&p, "schedule_performance_index", I), 1.5));
    assert!(near(n(&p, "cost_performance_index", I), 1.5));
    assert!(near(n(&p, "earned_value_labor_units", I), 12.));

    // Custom 25 %: EV 300 once started.
    set(&mut p, Some("custom"), Some("25"), None, None);
    assert!(near(n(&p, "earned_value_cost", I), 300.));
    assert!(near(n(&p, "performance_percent_complete", I), 25.));

    // WBS milestones: one done of weight 1 against 3 open: 25 %.
    let w = wbs_of(&p);
    for (id, done, weight) in [("1", "Y", "1"), ("2", "N", "3")] {
        crate::records::add_row(
            &mut p.document,
            "WBSSTEP",
            &[
                ("wbs_step_id", id.into()),
                ("wbs_id", w.to_string()),
                ("proj_id", "1".into()),
                ("step_wt", weight.into()),
                ("complete_flag", done.into()),
            ],
        );
    }
    set(&mut p, Some("wbs_milestones"), None, None, None);
    assert!(near(n(&p, "earned_value_cost", I), 300.));

    // Back to activity percent complete: EV 400.
    set(&mut p, Some("activity_percent"), None, None, None);
    assert!(near(n(&p, "earned_value_cost", I), 400.));
}

#[test]
fn estimate_to_complete_techniques_give_the_hand_computed_estimates() {
    let mut p = progressed_starter();
    // With 50/50: BAC 1200, EV 600, AC 400, PV 400; CPI 1.5, SPI 1.5.
    set(&mut p, Some("50/50"), None, Some("pf_1"), None);
    // PF = 1: ETC = 1200 - 600 = 600; EAC = 400 + 600; VAC = 1200 - 1000.
    assert!(near(n(&p, "estimate_to_complete", I), 600.));
    assert!(near(n(&p, "estimate_at_completion_cost", I), 1000.));
    assert!(near(n(&p, "variance_at_completion", I), 200.));
    // PF = 1 / CPI: ETC = 600 / 1.5 = 400.
    set(&mut p, None, None, Some("pf_cpi"), None);
    assert!(near(n(&p, "estimate_to_complete", I), 400.));
    // PF = 1 / (CPI * SPI): ETC = 600 / 2.25 = 266.67; EAC 666.67.
    set(&mut p, None, None, Some("pf_cpi_spi"), None);
    assert!(near(n(&p, "estimate_to_complete", I), 266.67));
    assert!(near(n(&p, "estimate_at_completion_cost", I), 666.67));
    // PF = 1.2: ETC = 1.2 x 600 = 720; VAC = 1200 - 1120 = 80.
    set(&mut p, None, None, Some("pf"), Some("1.2"));
    assert!(near(n(&p, "estimate_to_complete", I), 720.));
    assert!(near(n(&p, "variance_at_completion", I), 80.));
    // Remaining cost again: 800.
    set(&mut p, None, None, Some("remaining"), None);
    assert!(near(n(&p, "estimate_to_complete", I), 800.));
    // A factor of zero and a percent over 100 are refused and change nothing.
    let w = wbs_of(&p);
    let before = ev::get(&p, w).unwrap();
    assert!(ev::set(&mut p, w, None, Some("140"), None, None).is_err());
    assert!(ev::set(&mut p, w, None, None, None, Some("0")).is_err());
    assert!(ev::set(&mut p, w, Some("50/50"), Some("-5"), None, None).is_err());
    assert_eq!(ev::get(&p, w).unwrap(), before);
}

#[test]
fn the_earned_value_tab_edits_one_undo_step_at_a_time_and_round_trips() {
    let p = progressed_starter();
    let w = wbs_of(&p);
    let mut d = Driver::new("wbs-ev", p);
    d.click("directory_wbs");
    d.app.views.wbs.selected = Some(w);
    d.app.views.wbs.show_details = true;
    d.app.views.wbs.tab = crate::views::wbs::TABS
        .iter()
        .position(|t| *t == "Earned Value")
        .unwrap();
    d.frames(3);
    let steps = d.app.history.steps();
    d.click("wbs_ev_50_50");
    d.frames(2);
    assert_eq!(ev::get(&d.app.project, w).unwrap().performance, "EC_50_50");
    assert_eq!(d.app.history.steps(), steps + 1, "one undo step");
    assert!(near(n(&d.app.project, "earned_value_cost", I), 600.));
    d.click("wbs_etc_pf_value");
    d.frames(2);
    // The factor field is enabled now: type 1.25 and press Enter.
    d.click("wbs_etc_pf");
    d.key_with(
        egui::Key::A,
        egui::Modifiers {
            ctrl: true,
            command: true,
            ..Default::default()
        },
    );
    d.type_text("1.25");
    d.key(egui::Key::Enter);
    d.frames(2);
    let t = ev::get(&d.app.project, w).unwrap();
    assert_eq!(t.to_complete, "EE_PF_usr");
    assert!(near(t.factor, 1.25), "{}", t.factor);
    assert_eq!(d.app.history.steps(), steps + 3);
    // The custom percent box is greyed out unless Custom is picked.
    d.click("wbs_ev_custom");
    d.frames(2);
    d.click("wbs_ev_custom_pct");
    d.key_with(
        egui::Key::A,
        egui::Modifiers {
            ctrl: true,
            command: true,
            ..Default::default()
        },
    );
    d.type_text("35");
    d.key(egui::Key::Enter);
    d.frames(2);
    let t = ev::get(&d.app.project, w).unwrap();
    assert_eq!(
        (t.performance.as_str(), t.custom_percent),
        ("EC_User_pct", 35.)
    );
    // XER round trip.
    let text = d.app.project.updated_document().text();
    let back = ProjectData::from_document(Document::parse(&text).unwrap(), 1, "rt".into()).unwrap();
    assert_eq!(ev::get(&back, w).unwrap(), t);
    // Undo walks back one field at a time.
    d.app.history_move(&d.ctx.clone(), false);
    d.frames(2);
    assert_eq!(ev::get(&d.app.project, w).unwrap().custom_percent, 0.);
    d.app.history_move(&d.ctx.clone(), false);
    d.frames(2);
    assert_eq!(ev::get(&d.app.project, w).unwrap().performance, "EC_50_50");
}
