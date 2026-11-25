//! Typed period values of an assignment (`ProjectData::set_period_units`)
//! and how every spread reads them. Synthetic project: the starter's
//! three-day activity 1010 on its Monday to Friday 8 hour calendar, from
//! the data date (a Monday 08:00), with one labor assignment of 1 unit an
//! hour (24 units, 8 a day).
use openxer_engine::calendar::DAY;
use openxer_xer::edit::{curves, records};
use openxer_xer::manual_units;
use openxer_xer::spread::{self, Plan};
use openxer_xer::ProjectData;

fn project() -> (ProjectData, usize, i64) {
    let mut p = openxer_xer::starter();
    p.document.table_mut("RSRC").add(&[
        ("rsrc_id", "5".into()),
        ("rsrc_short_name", "CREW".into()),
        ("rsrc_type", "RT_Labor".into()),
        ("def_qty_per_hr", "1".into()),
    ]);
    let dd = p
        .model
        .project
        .data_date
        .expect("the starter has a data date");
    let i = p
        .model
        .activities
        .iter()
        .position(|a| a.task_id == 1010)
        .unwrap();
    // Monday 08:00 to Wednesday 17:00.
    let finish = dd + 2 * DAY + 9 * 60;
    let a = &mut p.model.activities[i];
    a.early_start = Some(dd);
    a.early_finish = Some(finish);
    a.late_start = Some(dd);
    a.late_finish = Some(finish);
    a.target_start = Some(dd);
    a.target_finish = Some(finish);
    let row = p.add_assignment(1010, Some(5), None).unwrap();
    (p, row, dd.div_euclid(DAY) * DAY)
}

fn days(day0: i64, n: i64) -> Vec<(i64, i64)> {
    (0..n)
        .map(|d| (day0 + d * DAY, day0 + (d + 1) * DAY))
        .collect()
}

/// The assignment's remaining and budgeted units per day, as every usage
/// view spreads them.
fn spreads(p: &ProjectData, row: usize, day0: i64) -> (Vec<f64>, Vec<f64>) {
    let x = spread::assignment_rows(p)
        .into_iter()
        .find(|x| x.row == row)
        .unwrap();
    let it = x.item(&p.model);
    let plan = Plan::of(&p.model, &it);
    let periods = days(day0, 3);
    let mut remaining = vec![0.; 3];
    let mut budget = vec![0.; 3];
    spread::remaining(&p.model, &it, &plan, x.units[2], &periods, &mut remaining);
    spread::budget(&p.model, &it, &plan, x.units[0], &periods, &mut budget);
    (remaining, budget)
}

fn close(got: &[f64], want: &[f64]) {
    assert_eq!(got.len(), want.len());
    for (k, (g, w)) in got.iter().zip(want).enumerate() {
        assert!((g - w).abs() < 1e-9, "day {k}: {g} vs {w}\n{got:?}");
    }
}

fn get(p: &ProjectData, row: usize, column: &str) -> String {
    records::get(&p.document, "TASKRSRC", row, column)
}

fn task(p: &ProjectData, column: &str) -> String {
    records::task_value(&p.document, 1010, p.model.project.proj_id, column)
}

#[test]
fn typing_one_day_changes_that_day_and_the_totals() {
    let (mut p, row, day0) = project();
    assert_eq!(get(&p, row, "remain_crv"), "");
    close(&spreads(&p, row, day0).0, &[8., 8., 8.]);

    p.set_period_units(row, (day0 + DAY, day0 + 2 * DAY), 20.)
        .unwrap();
    // The other days keep 8; the runs count working hours from the start.
    assert_eq!(get(&p, row, "remain_crv"), "8:8;20:8;8:8");
    // Not started and linked: the budget follows.
    assert_eq!(get(&p, row, "target_crv"), "8:8;20:8;8:8");
    assert_eq!(get(&p, row, "actual_crv"), "");
    assert_eq!(get(&p, row, "remain_qty"), "36");
    assert_eq!(get(&p, row, "target_qty"), "36");
    // Units/time: the new total over the unchanged 24 hours.
    assert_eq!(get(&p, row, "remain_qty_per_hr"), "1.5");
    assert_eq!(get(&p, row, "target_qty_per_hr"), "1.5");
    // The activity's labor units follow; its duration does not move.
    assert_eq!(task(&p, "remain_work_qty"), "36");
    assert_eq!(task(&p, "target_work_qty"), "36");
    let i = p
        .model
        .activities
        .iter()
        .position(|a| a.task_id == 1010)
        .unwrap();
    assert_eq!(p.model.activities[i].rem, 24 * 60);
    assert_eq!(
        curves::curve_label(&p.document, row).as_deref(),
        Some("Manual")
    );
    let (remaining, budget) = spreads(&p, row, day0);
    close(&remaining, &[8., 20., 8.]);
    close(&budget, &[8., 20., 8.]);

    // Typing the old value back joins the runs and stays Manual.
    p.set_period_units(row, (day0 + DAY, day0 + 2 * DAY), 8.)
        .unwrap();
    assert_eq!(get(&p, row, "remain_crv"), "24:24");
    assert_eq!(get(&p, row, "remain_qty"), "24");
    assert!(manual_units::is_manual(&p.document, row));

    // A week bucket spreads evenly over its working time in the window.
    p.set_period_units(row, (day0, day0 + 7 * DAY), 30.)
        .unwrap();
    assert_eq!(get(&p, row, "remain_crv"), "30:24");
    close(&spreads(&p, row, day0).0, &[10., 10., 10.]);

    // Clear empties the runs and the curve; the units stay, spread evenly.
    assert!(curves::clear_manual(&mut p.document, row));
    assert_eq!(get(&p, row, "remain_crv"), "");
    assert_eq!(get(&p, row, "target_crv"), "");
    assert_eq!(get(&p, row, "curv_id"), "");
    assert!(!curves::clear_manual(&mut p.document, row));
    assert_eq!(curves::curve_label(&p.document, row), None);
}

#[test]
fn past_periods_and_periods_outside_the_work_are_refused() {
    let (mut p, row, day0) = project();
    let before = p.document.table("TASKRSRC").unwrap().rows.clone();
    for period in [
        (day0 - DAY, day0),
        (day0 + 3 * DAY, day0 + 4 * DAY),
        (day0 + DAY, day0 + DAY),
    ] {
        assert!(p.set_period_units(row, period, 5.).is_err(), "{period:?}");
        assert!(p.check_period_units(row, period).is_err());
    }
    assert!(p.set_period_units(row, (day0, day0 + DAY), -1.).is_err());
    assert_eq!(p.document.table("TASKRSRC").unwrap().rows, before);
    assert!(p.check_period_units(row, (day0, day0 + DAY)).is_ok());
}

#[test]
fn the_budget_stays_when_budget_and_remaining_are_not_linked() {
    let (mut p, row, day0) = project();
    let t = p.document.table_mut("PROJECT");
    t.set(0, "rem_target_link_flag", "N".into());
    p.set_period_units(row, (day0, day0 + DAY), 4.).unwrap();
    assert_eq!(get(&p, row, "remain_crv"), "4:8;16:16");
    assert_eq!(get(&p, row, "remain_qty"), "20");
    assert_eq!(get(&p, row, "target_crv"), "");
    assert_eq!(get(&p, row, "target_qty"), "24");
    let (remaining, budget) = spreads(&p, row, day0);
    close(&remaining, &[4., 8., 8.]);
    close(&budget, &[8., 8., 8.]);
}

#[test]
fn a_curved_assignment_keeps_its_other_days_when_one_is_typed() {
    let (mut p, row, day0) = project();
    curves::set_assignment_curve(&mut p.document, row, Some("Ramp Up")).unwrap();
    let before = spreads(&p, row, day0).0;
    assert!(before[0] < before[1] && before[1] < before[2]);
    p.set_period_units(row, (day0 + DAY, day0 + 2 * DAY), 5.)
        .unwrap();
    // The assigned curve is gone, the shape it gave the other days kept.
    assert_eq!(get(&p, row, "curv_id"), "");
    let after = spreads(&p, row, day0).0;
    close(&after, &[before[0], 5., before[2]]);
    let total = before[0] + 5. + before[2];
    let stored: f64 = get(&p, row, "remain_qty").parse().unwrap();
    assert!((stored - total).abs() < 1e-9);
    // Picking a curve on a Manual assignment drops its runs.
    assert!(curves::set_assignment_curve(&mut p.document, row, None).unwrap());
    assert!(!manual_units::is_manual(&p.document, row));
}

#[test]
fn runs_stretch_to_a_changed_window_and_scale_to_the_units() {
    let (mut p, row, day0) = project();
    // Runs written for 12 hours and 12 units read over the 24 hour window
    // and the 24 stored units: twice the hours, twice the units.
    records::set(
        &mut p.document,
        "TASKRSRC",
        row,
        "remain_crv",
        "2:4;8:4;2:4".into(),
    );
    close(&spreads(&p, row, day0).0, &[4., 16., 4.]);
}
