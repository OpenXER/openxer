//! Resource curves shape the usage views and planned value. The resource
//! fixture: PIPE, 24 units over the three working days of 1010 (Fixed
//! Duration and Units). With the Early Weighted preset (f(t) = (1 - t)^2,
//! cumulative 1 - (1 - t)^3) the three days hold about 0.7037, 0.2593 and
//! 0.0370 of the units: 16.89, 6.22 and 0.89. The preset's points are
//! rounded to hundredths, so the days match the closed form within 0.05.
use super::stacked::{self, StackBy};
use super::usage;
use super::*;
use crate::grid::columns;
use crate::views::assignments as asg;
use crate::views::resources as rs;
use openxer_xer::edit::{curves, records};

fn with_curve(name: &str) -> ProjectData {
    let mut p = rs::tests::with_resource();
    curves::set_assignment_curve(&mut p.document, 0, Some(name)).unwrap();
    p
}

/// Working days of 1010 and two days either side.
fn days(p: &ProjectData) -> Vec<(i64, i64)> {
    let a = p
        .model
        .activities
        .iter()
        .find(|a| a.task_id == 1010)
        .unwrap();
    let first = a.early_start.unwrap().div_euclid(DAY) * DAY - 2 * DAY;
    (0..8)
        .map(|d| (first + d * DAY, first + (d + 1) * DAY))
        .collect()
}

fn early_weighted(t: f64) -> f64 {
    1. - (1. - t).powi(3)
}

fn working(values: &[f64]) -> Vec<f64> {
    values.iter().copied().filter(|v| *v > 1e-12).collect()
}

#[test]
fn a_front_loaded_curve_moves_units_forward_in_every_usage_view() {
    let p = with_curve("Early Weighted");
    let periods = days(&p);
    let by_task = usage::labor_assignments(&p);
    let list = &by_task.values().next().unwrap();
    assert!(list[0].curved());
    let u = usage::activity_usage_of(&p, &by_task, &periods, None, false);
    let h = rs::usage_of(&p, 7, &periods, None, false);
    let sheet = asg::remaining_spread(&p, &list[0], &periods);
    let refs: Vec<&asg::Assignment> = list.iter().collect();
    let stack = stacked::series(&p, &refs, &periods, false, StackBy::Resource);
    for (k, &value) in sheet.iter().enumerate() {
        assert!((u.remaining[k] - h.remaining[k]).abs() < 1e-9, "{k}");
        assert!((u.remaining[k] - value).abs() < 1e-9, "{k}");
        let stacked: f64 = stack.iter().map(|s| s.values[k]).sum();
        assert!((u.remaining[k] - stacked).abs() < 1e-9, "{k}");
        assert!((u.budget[k] - u.remaining[k]).abs() < 1e-9, "{k}");
        assert!((u.remaining_late[k] - u.remaining[k]).abs() < 1e-9, "{k}");
    }
    let day = working(&u.remaining);
    assert_eq!(day.len(), 3, "{:?}", u.remaining);
    let want = [
        early_weighted(1. / 3.),
        early_weighted(2. / 3.) - early_weighted(1. / 3.),
        1. - early_weighted(2. / 3.),
    ];
    for (got, share) in day.iter().zip(want) {
        assert!((got - 24. * share).abs() < 0.05, "{got} vs {}", 24. * share);
    }
    assert!((day.iter().sum::<f64>() - 24.).abs() < 1e-9);
    // Costs spread with the units.
    let c = usage::activity_usage_of(&p, &by_task, &periods, None, true);
    let cost = list[0].remaining_cost;
    for k in 0..periods.len() {
        assert!((c.remaining[k] - u.remaining[k] / 24. * cost).abs() < 1e-9);
    }
}

#[test]
fn without_a_curve_or_on_fixed_units_the_spread_stays_even() {
    for p in [rs::tests::with_resource(), {
        let mut p = with_curve("Early Weighted");
        records::set_task_value(
            &mut p.document,
            1010,
            1,
            "duration_type",
            "DT_FixedQty".into(),
        );
        p
    }] {
        let periods = days(&p);
        let by_task = usage::labor_assignments(&p);
        assert!(!by_task.values().flatten().any(|a| a.curved()));
        let u = usage::activity_usage_of(&p, &by_task, &periods, None, false);
        let day = working(&u.remaining);
        assert_eq!(day.len(), 3);
        assert!(day.iter().all(|v| (v - 8.).abs() < 1e-9), "{day:?}");
    }
}

#[test]
fn planned_value_follows_the_curved_budget() {
    // The progressed starter: activity 1 has 24 labor units (1200 cost) over
    // three planned days and the data date one day in; even, PV is a third
    // (8 units, 400). With Early Weighted the first third plans about
    // 0.7037 of the budget: 16.89 units and 844.4 cost.
    let mut p = crate::grid::tests_h3::progressed_starter();
    let ix = columns::Index::new((false, 0.));
    let catalog = columns::catalog(&p);
    let get = |p: &ProjectData, ix: &columns::Index, id: &str, i: usize| {
        let c = columns::find(&catalog, id).unwrap();
        columns::value(p, ix, c, i).number().unwrap_or(f64::NAN)
    };
    let before_7 = get(&p, &ix, "planned_value_cost", 7);
    assert!((get(&p, &ix, "planned_value_labor_units", 1) - 8.).abs() < 1e-6);
    let task = p.model.activities[1].task_id;
    let row = p.assignment_rows(task)[0];
    curves::set_assignment_curve(&mut p.document, row, Some("Early Weighted")).unwrap();
    let ix = columns::Index::new((false, 0.));
    let share = early_weighted(1. / 3.);
    let units = get(&p, &ix, "planned_value_labor_units", 1);
    let cost = get(&p, &ix, "planned_value_cost", 1);
    assert!((units - 24. * share).abs() < 0.05, "{units}");
    assert!((cost - 1200. * share).abs() < 2.5, "{cost}");
    // Earned value is not a spread: it stays at 8 units, so SPI drops.
    assert!((get(&p, &ix, "earned_value_labor_units", 1) - 8.).abs() < 1e-6);
    assert!(get(&p, &ix, "schedule_performance_index_labor_units", 1) < 0.5);
    // Other activities keep theirs.
    assert_eq!(get(&p, &ix, "planned_value_cost", 7), before_7);
    // Schedule % Complete stays the time share.
    assert!((get(&p, &ix, "schedule_percent_complete", 1) - 100. / 3.).abs() < 1e-6);
    // The command line's earned value (`openxer_xer::ev::Calculator`, what
    // `earned-value` and Monitor Thresholds read) gives the same curved
    // planned value and every other part, activity by activity.
    use openxer_xer::ev::{Basis, Calculator, DateBasis};
    let calc = Calculator::new(&p, None, None, DateBasis::Planned);
    for basis in [Basis::Cost, Basis::LaborUnits] {
        for i in 0..p.model.activities.len() {
            assert_eq!(
                calc.parts(i, basis),
                crate::grid::columns_ev::parts(&p, &ix, i, basis),
                "activity {i}, {basis:?}"
            );
        }
    }
    assert!((calc.parts(1, Basis::LaborUnits).pv - units).abs() < 1e-9);
    assert!((calc.parts(1, Basis::Cost).pv - cost).abs() < 1e-9);
}

/// One 6-day labor assignment (48 units of PIPE on 1020), evenly or along
/// a curve.
fn six_days(curve: Option<&str>) -> ProjectData {
    let mut doc = openxer_xer::starter().document;
    doc.table_mut("RSRC").add(&[
        ("rsrc_id", "7".into()),
        ("clndr_id", "1".into()),
        ("rsrc_short_name", "PIPE".into()),
        ("rsrc_name", "Pipefitter".into()),
        ("rsrc_type", "RT_Labor".into()),
        ("def_qty_per_hr", "1".into()),
        ("active_flag", "Y".into()),
    ]);
    doc.table_mut("TASKRSRC").add(&[
        ("taskrsrc_id", "1".into()),
        ("task_id", "1020".into()),
        ("proj_id", "1".into()),
        ("rsrc_id", "7".into()),
        ("remain_qty", "48".into()),
        ("remain_qty_per_hr", "1".into()),
        ("target_qty", "48".into()),
        ("target_qty_per_hr", "1".into()),
        ("rollup_dates_flag", "N".into()),
    ]);
    let mut p = ProjectData::from_document(doc, 1, "six-days".into()).unwrap();
    openxer_engine::scheduler::schedule_with_resources(&mut p.model, &p.resources).unwrap();
    if let Some(name) = curve {
        curves::set_assignment_curve(&mut p.document, 0, Some(name)).unwrap();
    }
    p
}

#[test]
fn a_curve_on_a_six_day_assignment_front_loads_and_keeps_the_total() {
    let even = six_days(None);
    let early = six_days(Some("Early Weighted"));
    let first = even.model.activities[2]
        .early_start
        .unwrap()
        .div_euclid(DAY)
        * DAY;
    let periods: Vec<(i64, i64)> = (0..10)
        .map(|d| (first + d * DAY, first + (d + 1) * DAY))
        .collect();
    let e = working(&rs::usage_of(&even, 7, &periods, None, false).remaining);
    let c = working(&rs::usage_of(&early, 7, &periods, None, false).remaining);
    assert_eq!((e.len(), c.len()), (6, 6), "{e:?} {c:?}");
    assert!(e.iter().all(|v| (v - 8.).abs() < 1e-9));
    for (d, &got) in c.iter().enumerate() {
        let share = early_weighted((d + 1) as f64 / 6.) - early_weighted(d as f64 / 6.);
        assert!(
            (got - 48. * share).abs() < 0.1,
            "day {d}: {} vs {}",
            got,
            48. * share
        );
    }
    assert!((c.iter().sum::<f64>() - 48.).abs() < 1e-9);
}

/// A synthetic large project: the starter plus `tasks` activities in 100
/// chains, `resources` labor resources and `rows` assignments spread over
/// them, every assignment with its own duration (units over units/time
/// differ from the activity's), a third with a lag, a fifth Early Weighted.
fn large(tasks: usize, resources: usize, rows: usize) -> ProjectData {
    let mut doc = openxer_xer::starter().document;
    let curve = curves::ensure(&mut doc, "Early Weighted").unwrap();
    for i in 0..tasks {
        let hours = (5 + i % 40) * 8;
        doc.table_mut("TASK").add(&[
            ("task_id", (100_000 + i).to_string()),
            ("proj_id", "1".into()),
            ("wbs_id", "30".into()),
            ("clndr_id", "1".into()),
            ("task_code", format!("L-{i:05}")),
            ("task_name", format!("Synthetic work {i}")),
            ("task_type", "TT_Task".into()),
            ("status_code", "TK_NotStart".into()),
            ("duration_type", "DT_FixedDUR2".into()),
            ("remain_drtn_hr_cnt", hours.to_string()),
            ("target_drtn_hr_cnt", hours.to_string()),
        ]);
        if i >= 100 {
            doc.table_mut("TASKPRED").add(&[
                ("task_pred_id", (100_000 + i).to_string()),
                ("proj_id", "1".into()),
                ("pred_proj_id", "1".into()),
                ("pred_task_id", (100_000 + i - 100).to_string()),
                ("task_id", (100_000 + i).to_string()),
                ("pred_type", "PR_FS".into()),
                ("lag_hr_cnt", "0".into()),
            ]);
        }
    }
    for r in 0..resources {
        doc.table_mut("RSRC").add(&[
            ("rsrc_id", (1000 + r).to_string()),
            ("clndr_id", "1".into()),
            ("rsrc_short_name", format!("R{r:03}")),
            ("rsrc_name", format!("Crew {r}")),
            ("rsrc_type", "RT_Labor".into()),
            ("def_qty_per_hr", "1".into()),
            ("active_flag", "Y".into()),
        ]);
    }
    for k in 0..rows {
        let hours = 8 + (k % 23) * 4;
        doc.table_mut("TASKRSRC").add(&[
            ("taskrsrc_id", (1 + k).to_string()),
            ("task_id", (100_000 + k % tasks).to_string()),
            ("proj_id", "1".into()),
            ("rsrc_id", (1000 + k % resources).to_string()),
            ("target_qty", hours.to_string()),
            ("target_qty_per_hr", "1".into()),
            ("remain_qty", hours.to_string()),
            ("remain_qty_per_hr", "1".into()),
            ("target_cost", (hours * 50).to_string()),
            ("remain_cost", (hours * 50).to_string()),
            ("target_lag_drtn_hr_cnt", ((k % 3) * 8).to_string()),
            ("relag_drtn_hr_cnt", ((k % 3) * 8).to_string()),
            ("rollup_dates_flag", "N".into()),
            (
                "curv_id",
                if k % 5 == 0 {
                    curve.to_string()
                } else {
                    String::new()
                },
            ),
        ]);
    }
    let mut p = ProjectData::from_document(doc, 1, "large".into()).unwrap();
    openxer_engine::scheduler::schedule_with_resources(&mut p.model, &p.resources).unwrap();
    p
}

#[test]
fn curve_spread_usage_of_a_large_project_recomputes_in_bounded_time() {
    // 300 resources and 20,000 assignments on 2,000 activities over about
    // a year and a half. One usage recompute as the views do it after a change of
    // periods (a scroll), twice: the assignments are built once and indexed
    // by resource, each resource's profile and overtime come from its own
    // assignments, and the activity profile spreads them all. The second
    // pass reuses the spread plans. The views memoize the list per undo
    // snapshot (`assignment_list`, `resource_assignments`).
    let (tasks, resources, rows) = (2000, 300, 20_000);
    let p = large(tasks, resources, rows);
    let start = p.model.project.data_date.unwrap().div_euclid(DAY) * DAY;
    let weeks = |from: i64| -> Vec<(i64, i64)> {
        (0..100)
            .map(|w| {
                (
                    start + (from + w) * 7 * DAY,
                    start + (from + w + 1) * 7 * DAY,
                )
            })
            .collect()
    };
    let started = std::time::Instant::now();
    let list = asg::assignments(&p);
    assert_eq!(list.len(), rows);
    let index = asg::by_resource(&list);
    let by_task = usage::labor_assignments_in(&p, &list);
    let mut grand = 0.;
    let mut passes = vec![];
    for periods in [weeks(0), weeks(1)] {
        let pass = std::time::Instant::now();
        let mut total = 0.;
        for r in 0..resources as i64 {
            let mine = || index[&(1000 + r)].iter().map(|&k| &list[k]);
            let u = rs::usage_in(&p, 1000 + r, mine(), &periods, false);
            total += u.remaining.iter().sum::<f64>();
            let overtime = usage::resource_overtime(&p, mine(), &periods, false);
            assert!(overtime.iter().all(|v| *v == 0.));
        }
        let u = usage::activity_usage_of(&p, &by_task, &periods, None, false);
        assert!((u.remaining.iter().sum::<f64>() - total).abs() < 1e-6 * total);
        grand = total;
        passes.push(pass.elapsed());
    }
    let took = started.elapsed();
    // Everything starts within the first 100 weeks, so most of the stored
    // remaining units are in the periods.
    let stored: f64 = list.iter().map(|a| a.remaining).sum();
    assert!(grand > 0.5 * stored, "{grand} of {stored}");
    // About ten seconds in a debug build, nearly all of it the first pass
    // working out each assignment's plan (the calendar walks); generous for
    // a busy machine.
    let budget = if cfg!(debug_assertions) { 120.0 } else { 20.0 };
    assert!(
        took.as_secs_f64() < budget,
        "took {took:?} (passes {passes:?})"
    );
    eprintln!("large usage recompute: {took:?}, passes {passes:?}");
}

/// Renders for a look: the usage profiles and spreadsheets of the six-day
/// assignment, even and Early Weighted, in `artifacts/curves/`.
#[test]
#[ignore = "writes review renders to artifacts/curves; run on demand"]
fn render_curved_and_even_usage() {
    use crate::headless_render::SoftwarePainter;
    let out = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/curves");
    std::fs::create_dir_all(&out).unwrap();
    for (label, project) in [
        ("even", six_days(None)),
        ("early-weighted", six_days(Some("Early Weighted"))),
    ] {
        let path = std::env::temp_dir().join(format!(
            "openxer-curve-render-{label}-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let mut store = crate::store::Store::open(path.clone()).unwrap();
        store.save(&project).unwrap();
        let ctx = egui::Context::default();
        ctx.set_pixels_per_point(1.);
        let mut app = App::new(&ctx, store, project);
        let mut painter = SoftwarePainter::default();
        painter.screen = Some(egui::Vec2::new(1500., 900.));
        let mut time = 0.;
        let frames = |painter: &mut SoftwarePainter, app: &mut App, time: &mut f64, n: usize| {
            for _ in 0..n {
                let _ = painter.frame(app, &ctx, time, vec![], false);
            }
        };
        frames(&mut painter, &mut app, &mut time, 4);
        app.panes.usage.resource = Some(7);
        // Days, so each working day's units show.
        app.panes.usage.interval = usage::Interval::Day;
        // Zoom in on the timeline so the spreadsheet's day columns hold
        // their numbers.
        for _ in 0..3 {
            app.zoom_step(true);
            frames(&mut painter, &mut app, &mut time, 2);
        }
        for (name, view) in [
            ("resource-profile", BottomView::ResourceUsageProfile),
            ("resource-sheet", BottomView::ResourceUsageSheet),
            ("activity-profile", BottomView::ActivityUsageProfile),
            ("activity-sheet", BottomView::ActivityUsageSheet),
        ] {
            app.set_bottom(Some(view));
            frames(&mut painter, &mut app, &mut time, 4);
            let (image, _) = painter
                .frame(&mut app, &ctx, &mut time, vec![], true)
                .unwrap();
            image.save(out.join(format!("{label}-{name}.png"))).unwrap();
        }
        assert!(app.error.is_none(), "{:?}", app.error);
        drop(app);
        let _ = std::fs::remove_file(path);
    }
}
