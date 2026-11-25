//! Monitor Thresholds on the cost and earned value parameters, on a
//! synthetic project with known earned value figures (`progressed_starter`):
//! activity RB-1010 has a budget at completion of $1,200, $400 actual cost,
//! $800 remaining cost and is one of three planned days in (planned value
//! $400, one day of three actual duration). Its WBS element measures
//! performance 50/50, so earned value is $600. Activity RB-1060 is not
//! started (budget $2,500, no planned or earned value).
//!
//! Hand-computed for RB-1010: CV = 600 - 400 = 200, SV = 600 - 400 = 200,
//! AV = 400 - 400 = 0, VAC = 1200 - (400 + 800) = 0, CPI = 600 / 400 = 1.5,
//! SPI = 1.5, CVI = 200 / 600 = 0.33, SVI = 200 / 400 = 0.5, Cost % of
//! Budget = 400 / 1200 = 33.33 %, Duration % of Original = 1 / 3 = 33.33 %.
use super::*;
use crate::grid::tests_h3::progressed_starter;
use crate::qa_sweep::Driver;
use openxer_xer::edit::earned_value as ev;

fn threshold(p: &mut ProjectData, parm: i64, low: &str, high: &str, wbs: bool) -> i64 {
    let id = thresholds::add_threshold(p);
    let doc = &mut p.document;
    for (column, value) in [
        ("thresh_parm_id", parm.to_string()),
        ("lo_parm_value", low.to_string()),
        ("hi_parm_value", high.to_string()),
        (
            "thresh_type",
            if wbs { "TM_WBS" } else { "TM_Task" }.to_string(),
        ),
    ] {
        record_view::set_value(doc, "PROJTHRS", "thresh_id", id, column, value);
    }
    id
}

#[test]
fn cost_and_earned_value_thresholds_raise_issues_with_the_hand_computed_values() {
    let mut p = progressed_starter();
    let i = 1;
    let task = p.model.activities[i].task_id;
    let wbs = p.details[&task].wbs;
    ev::set(&mut p, wbs, Some("50/50"), None, None, None).unwrap();
    // (parameter id, low, high, expected issue value on RB-1010 or None).
    let cases: [(i64, &str, &str, Option<&str>); 10] = [
        (2, "", "100", Some("200")),   // CV
        (3, "", "150", Some("200")),   // SV
        (1, "-1", "1", None),          // AV = 0, within
        (4, "-1", "1", None),          // VAC = 0, within
        (6, "", "1.2", Some("1.5")),   // CPI
        (9, "", "1.4", Some("1.5")),   // SPI
        (7, "", "0.3", Some("0.33")),  // CVI
        (10, "", "0.4", Some("0.5")),  // SVI
        (5, "", "30", Some("33.33")),  // Cost % of Budget
        (11, "", "30", Some("33.33")), // Duration % of Original
    ];
    let ids: Vec<i64> = cases
        .iter()
        .map(|(parm, lo, hi, _)| threshold(&mut p, *parm, lo, hi, false))
        .collect();
    // At WBS level: the element's CPI from its summed parts is 1.5 too.
    let wbs_cpi = threshold(&mut p, 6, "", "1.2", true);
    // A low CPI bound on the not started RB-1060 raises nothing (no index).
    let low_cpi = threshold(&mut p, 6, "0.5", "", false);
    let mut d = Driver::new("monitor-ev", p);
    let result = d.app.run_monitor(None);
    assert_eq!(result.skipped, 0, "every parameter is evaluated");
    assert_eq!(result.thresholds, cases.len() + 2);
    let raised = issues::issues(&d.app.project);
    for ((parm, _, _, expected), id) in cases.iter().zip(&ids) {
        let mine: Vec<&issues::Issue> = raised
            .iter()
            .filter(|x| x.threshold_id == *id && x.task_id == task)
            .collect();
        match expected {
            Some(v) => {
                assert_eq!(mine.len(), 1, "parameter {parm}: {raised:?}");
                assert_eq!(mine[0].value, *v, "parameter {parm}");
            }
            None => assert!(mine.is_empty(), "parameter {parm}: {mine:?}"),
        }
    }
    let wbs_issues: Vec<&issues::Issue> = raised
        .iter()
        .filter(|x| x.threshold_id == wbs_cpi)
        .collect();
    assert!(
        wbs_issues
            .iter()
            .any(|x| x.wbs_id == wbs && x.task_id == 0 && x.value == "1.5"),
        "{wbs_issues:?}"
    );
    assert!(
        raised.iter().all(|x| x.threshold_id != low_cpi),
        "an undefined index raises nothing"
    );
    // A second run updates nothing and adds nothing.
    let again = d.app.run_monitor(None);
    assert_eq!((again.created, again.updated), (0, 0));
    // The technique change moves the values: 0/100 earns nothing yet, so
    // CV = 0 - 400 trips a low bound of -100.
    ev::set(&mut d.app.project, wbs, Some("0/100"), None, None, None).unwrap();
    let low_cv = threshold(&mut d.app.project, 2, "-100", "", false);
    d.app.touch_records();
    d.app.run_monitor(None);
    let cv = issues::issues(&d.app.project)
        .into_iter()
        .find(|x| x.threshold_id == low_cv && x.task_id == task)
        .expect("CV issue");
    assert_eq!(cv.value, "-400");
    assert!(!d.app.status.contains("skipped"), "{}", d.app.status);
}

#[test]
fn figures_compute_each_parameter() {
    let f = thresholds::Figures {
        parts: crate::grid::columns_ev::Parts {
            bac: 1000.,
            pv: 500.,
            ev: 400.,
            ac: 450.,
            etc: 650.,
        },
        original: 4800.,
        actual: 1200.,
    };
    assert_eq!(f.value("AV"), Some(50.));
    assert_eq!(f.value("CV"), Some(-50.));
    assert_eq!(f.value("SV"), Some(-100.));
    assert_eq!(f.value("VAC"), Some(-100.));
    assert_eq!(f.value("CPI"), Some(0.89));
    assert_eq!(f.value("SPI"), Some(0.8));
    assert_eq!(f.value("CVI"), Some(-0.13));
    assert_eq!(f.value("SVI"), Some(-0.2));
    assert_eq!(f.value("act_pct_spent"), Some(45.));
    assert_eq!(f.value("drtn_pct_spent"), Some(25.));
    let empty = thresholds::Figures::default();
    for field in [
        "CPI",
        "SPI",
        "CVI",
        "SVI",
        "act_pct_spent",
        "drtn_pct_spent",
    ] {
        assert_eq!(empty.value(field), None, "{field}");
    }
    assert_eq!(empty.value("CV"), Some(0.));
}

/// The command line computes Monitor Thresholds and earned value with
/// `openxer_xer::ev::Calculator`; the app reads the activity table's
/// lookups. Both give the same inputs and the same issues, with the project
/// as its own baseline and with a stored baseline whose dates and budgets
/// differ.
#[test]
fn the_shared_calculator_gives_the_app_monitor_inputs() {
    use crate::baselines::data::{Compare, Source};
    use crate::baselines::Slot;
    use crate::grid::columns::Index;
    use openxer_xer::ev::{self as xev, Calculator, DateBasis};
    let mut p = progressed_starter();
    let task = p.model.activities[1].task_id;
    let wbs = p.details[&task].wbs;
    ev::set(&mut p, wbs, Some("50/50"), None, Some("pf_cpi"), None).unwrap();
    // A baseline: the same project two days earlier with other budgets.
    let mut copy = p.clone();
    for a in &mut copy.model.activities {
        a.target_start = a.target_start.map(|m| m - 2 * 1440);
        a.target_finish = a.target_finish.map(|m| m - 2 * 1440);
    }
    let doc = copy.updated_document();
    let stored = std::sync::Arc::new(
        xev::load("BL", doc, copy.model.project.proj_id, p.model.origin_serial).unwrap(),
    );
    for (baseline, basis) in [
        (None, DateBasis::Planned),
        (Some(stored.clone()), DateBasis::Planned),
        (Some(stored.clone()), DateBasis::Current),
    ] {
        let mut compare = Compare {
            basis,
            ..Default::default()
        };
        compare.slots[0] = match &baseline {
            Some(b) => Source::Baseline(b.clone()),
            None => Source::Current,
        };
        compare.slots[1] = Source::Current;
        let compare = std::sync::Arc::new(compare);
        let mut ix = Index::new((false, 0.));
        ix.baselines = Some(compare.clone());
        let app_figures = thresholds::figures(&p, &ix);
        let app_baseline: Vec<_> = p
            .model
            .activities
            .iter()
            .map(|a| compare.dates(&p, a, Slot::Project))
            .collect();
        let calc = Calculator::new(&p, baseline.clone(), None, basis);
        let (baseline_dates, figures) = openxer_xer::edit::thresholds::inputs(&p, &calc);
        assert_eq!(figures, app_figures, "{basis:?}");
        assert_eq!(baseline_dates, app_baseline, "{basis:?}");
        for i in 0..p.model.activities.len() {
            assert_eq!(
                calc.parts(i, xev::Basis::LaborUnits),
                crate::grid::columns_ev::parts(&p, &ix, i, xev::Basis::LaborUnits)
            );
        }
    }
}
