//! The S-curve and earned hours figures on synthetic projects built from the
//! starter (no client data): labor, cost and duration-only schedules, one
//! in progress at a mid-way data date, with and without a baseline.
use super::*;
use crate::{starter, Document};
use openxer_engine::scheduler::schedule_with_resources;

fn load(doc: Document) -> ProjectData {
    let mut p = ProjectData::from_document(doc, 1, "Sample".into()).unwrap();
    schedule_with_resources(&mut p.model, &p.resources).unwrap();
    p
}

/// The crews sample (`workspace::sample::crews`) as planned.
fn labor_doc() -> Document {
    crate::workspace::sample::crews(24., false)
}

/// The crews sample after an update: the survey is complete and the data
/// date is three days later.
fn progressed_doc() -> Document {
    crate::workspace::sample::crews(24., true)
}

/// Equipment with a cost and nothing else assigned.
fn equipment_only() -> Document {
    let mut doc = labor_doc();
    let t = doc.table_mut("TASKRSRC");
    let col = t.field("rsrc_id").unwrap();
    t.rows.retain(|r| r[col] == "8");
    doc
}

fn sum(v: &[f64]) -> f64 {
    v.iter().sum()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

#[test]
fn detection_offers_what_the_project_holds_and_picks_the_first() {
    let plain = load(starter().document);
    let d = Detection::of(&[Source::new(&plain)]);
    assert_eq!(d.measures(), vec![Measure::Duration]);
    assert_eq!(d.measure(), Measure::Duration);
    assert_eq!(d.breakdown(Measure::Duration), Breakdown::Total);
    assert!(!d
        .breakdowns(Measure::Duration)
        .contains(&Breakdown::Resource));
    assert!(d
        .line(Measure::Duration)
        .starts_with("No labor hours or costs found; 11 of 14"));
    assert!(!d.financial);
    assert_eq!(d.periods(), vec![Period::Day, Period::Week, Period::Month]);

    let labor = load(labor_doc());
    let d = Detection::of(&[Source::new(&labor)]);
    assert_eq!(
        d.measures(),
        vec![Measure::Labor, Measure::Cost, Measure::Duration]
    );
    assert_eq!(
        d.line(Measure::Labor),
        "Labor hours found on 3 of 14 activities"
    );
    assert_eq!(d.line(Measure::Cost), "Costs found on 4 of 14 activities");
    assert_eq!(d.breakdown(Measure::Labor), Breakdown::Resource);
    let o = Options::detected(&d);
    assert_eq!(
        (o.measure, o.period, o.breakdown, o.late),
        (Measure::Labor, Period::Week, Breakdown::Resource, false)
    );

    // Equipment with a cost and an expense, no labor: cost comes first.
    let mut doc = equipment_only();
    doc.table_mut("PROJCOST").add(&[
        ("cost_item_id", "1".into()),
        ("proj_id", "1".into()),
        ("task_id", "1020".into()),
        ("target_cost", "600".into()),
        ("remain_cost", "600".into()),
    ]);
    let cost = load(doc);
    let d = Detection::of(&[Source::new(&cost)]);
    assert_eq!(d.measures(), vec![Measure::Cost, Measure::Duration]);
    assert_eq!(d.line(Measure::Cost), "Costs found on 2 of 14 activities");
}

#[test]
fn labor_curve_holds_the_budget_by_week_and_by_resource() {
    let p = load(labor_doc());
    let src = Source::new(&p);
    let d = Detection::of(std::slice::from_ref(&src));
    let c = curve(&[src], Options::detected(&d)).unwrap();
    // Weeks from Sunday: day serial 1 is a Sunday.
    for &(s, e) in &c.periods {
        assert_eq!((c.origin + s / DAY).rem_euclid(7), 1);
        assert_eq!(e - s, 7 * DAY);
    }
    assert!(close(sum(&c.total.budget), 104.), "crane hours stay out");
    assert!(close(sum(&c.total.remaining), 104.));
    assert!(close(sum(&c.total.actual), 0.));
    // The project's budgeted labor units as the earned value figures read
    // them.
    let figures = ev::figure_table(&p);
    let labor: f64 = p
        .model
        .activities
        .iter()
        .filter_map(|a| figures.get(&(a.task_id * 2)).map(|f| f[0].budget))
        .sum();
    assert!(close(sum(&c.total.budget), labor));
    let keys: Vec<&str> = c.lines.iter().map(|l| l.key.as_str()).collect();
    assert_eq!(keys, ["ELEC", "PIPE"]);
    assert!(close(sum(&c.lines[1].series.budget), 64.));
    for k in 0..c.periods.len() {
        let rows: f64 = c.lines.iter().map(|l| l.series.budget[k]).sum();
        assert!(close(rows, c.total.budget[k]), "period {k}");
    }
    assert!(c.budget_source.contains("no baseline"));
    assert!(c.warnings.iter().any(|w| w.contains("no labor assignment")));
    assert_eq!(c.earned, Some(0.));
    // WBS level 1 rows: the survey under RB.1, the rest under RB.3.
    let c = curve(
        &[Source::new(&p)],
        Options {
            breakdown: Breakdown::Wbs1,
            ..Options::detected(&d)
        },
    )
    .unwrap();
    let keys: Vec<&str> = c.lines.iter().map(|l| l.key.as_str()).collect();
    assert_eq!(keys, ["RB.1", "RB.3"]);
    assert!(close(sum(&c.lines[0].series.budget), 24.));
}

#[test]
fn cost_curve_adds_every_resource_and_expenses() {
    let mut doc = labor_doc();
    doc.table_mut("PROJCOST").add(&[
        ("cost_item_id", "1".into()),
        ("proj_id", "1".into()),
        ("task_id", "1020".into()),
        ("target_cost", "600".into()),
        ("remain_cost", "600".into()),
    ]);
    let p = load(doc);
    let o = Options {
        measure: Measure::Cost,
        period: Period::Month,
        breakdown: Breakdown::Resource,
        late: true,
        week_starts_sunday: true,
    };
    let c = curve(&[Source::new(&p)], o).unwrap();
    assert!(close(sum(&c.total.budget), 6800. + 600.));
    let keys: Vec<&str> = c.lines.iter().map(|l| l.key.as_str()).collect();
    assert_eq!(keys, ["(Expenses)", "CRANE", "ELEC", "PIPE"]);
    // Not started: the late curve holds the remaining cost too.
    assert!(close(sum(&c.total.late), 7400.));
}

#[test]
fn duration_curve_weighs_each_activity_by_its_planned_working_days() {
    let p = load(starter().document);
    let d = Detection::of(&[Source::new(&p)]);
    let c = curve(&[Source::new(&p)], Options::detected(&d)).unwrap();
    assert_eq!(c.options.measure, Measure::Duration);
    // 3 + 6 + 8 + 6 + 2 + 5 + 4 + 6 + 5 + 3 + 3 working days; milestones
    // weigh nothing.
    assert!(close(sum(&c.total.budget), 51.));
    assert!(close(sum(&c.total.remaining), 51.));
    assert!(c.lines.is_empty());
    assert!(
        curve(
            &[Source::new(&p)],
            Options {
                breakdown: Breakdown::Resource,
                ..Options::detected(&d)
            }
        )
        .is_err(),
        "no resources to break duration down by"
    );
}

#[test]
fn an_update_splits_actual_and_remaining_at_the_data_date_and_earns() {
    let p = load(progressed_doc());
    let dd = p.model.project.data_date.unwrap();
    for m in [Measure::Labor, Measure::Duration] {
        let c = curve(
            &[Source::new(&p)],
            Options {
                measure: m,
                period: Period::Day,
                breakdown: Breakdown::Total,
                late: false,
                week_starts_sunday: true,
            },
        )
        .unwrap();
        let k = c.earned_period.unwrap();
        assert!(c.periods[k].0 <= dd && dd < c.periods[k].1);
        for (i, &(_, e)) in c.periods.iter().enumerate() {
            if e > dd {
                assert_eq!(
                    c.total.actual[i], 0.,
                    "{m:?}: no actual after the data date"
                );
            }
            if c.periods[i].1 <= dd {
                assert_eq!(c.total.remaining[i], 0., "{m:?}: no remaining before it");
            }
        }
        let (done, all) = match m {
            Measure::Labor => (24., 104.),
            _ => (3., 51.),
        };
        assert!(close(sum(&c.total.actual), done), "{m:?}");
        assert!(close(sum(&c.total.actual) + sum(&c.total.remaining), all));
        assert!(close(c.earned.unwrap(), done), "{m:?} earned");
    }
}

/// The labor sample's plan with 30 survey hours instead of 24, as an
/// assigned project baseline of the progressed sample.
fn with_baseline(p: &ProjectData) -> Source<'_> {
    let mut plan = load(crate::workspace::sample::crews(30., false));
    plan.document = plan.updated_document();
    let loaded = ev::load("Plan", plan.document, 1, p.model.origin_serial).unwrap();
    Source {
        project_baseline: Some(Arc::new(loaded)),
        ..Source::new(p)
    }
}

#[test]
fn a_baseline_gives_the_budget_and_the_earned_basis() {
    let p = load(progressed_doc());
    let src = with_baseline(&p);
    let c = curve(
        std::slice::from_ref(&src),
        Options {
            measure: Measure::Labor,
            period: Period::Week,
            breakdown: Breakdown::Resource,
            late: false,
            week_starts_sunday: true,
        },
    )
    .unwrap();
    assert!(
        c.budget_source.starts_with("Baseline \"Plan\""),
        "{}",
        c.budget_source
    );
    assert!(close(sum(&c.total.budget), 110.));
    assert!(close(sum(&c.lines[1].series.budget), 70.));
    // Earned: the survey's 30 baseline hours at 100%.
    assert!(close(c.earned.unwrap(), 30.));
    let calc = src.calculator();
    let ev: f64 = (0..p.model.activities.len())
        .map(|i| calc.parts(i, Basis::LaborUnits).ev)
        .sum();
    assert!(close(c.earned.unwrap(), ev));
}

#[test]
fn earned_hours_follow_the_earned_value_figures() {
    let p = load(progressed_doc());
    for src in [Source::new(&p), with_baseline(&p)] {
        let e = earned_hours(&src, Group::Resource).unwrap();
        let calc = src.calculator();
        for (i, a) in p.model.activities.iter().enumerate() {
            let mine: f64 = e
                .details
                .iter()
                .filter(|d| d.activity_id == a.task_code)
                .map(|d| d.earned)
                .sum();
            assert!(
                close(mine, calc.parts(i, Basis::LaborUnits).ev),
                "{}",
                a.task_code
            );
        }
        let keys: Vec<&str> = e.rows.iter().map(|r| r.key.as_str()).collect();
        assert_eq!(keys, ["ELEC", "PIPE"]);
        let ids: Vec<(&str, &str)> = e
            .details
            .iter()
            .map(|d| (d.activity_id.as_str(), d.resource_id.as_str()))
            .collect();
        assert_eq!(
            ids,
            [
                ("RB-1010", "PIPE"),
                ("RB-1070", "PIPE"),
                ("RB-1100", "ELEC")
            ]
        );
        let survey = &e.details[0];
        assert!(close(survey.percent, 100.));
        assert!(close(survey.actual, 24.));
        assert!(close(survey.earned, survey.budgeted));
    }
    let e = earned_hours(&with_baseline(&p), Group::Wbs).unwrap();
    assert!(close(e.details[0].budgeted, 30.));
    assert!(e.budget_source.starts_with("Baseline"));
    assert_eq!(e.rows[0].key, "RB.1");
    assert!(e.percent_rule.contains("Activity % Complete"));
    let plain = load(starter().document);
    assert_eq!(
        earned_hours(&Source::new(&plain), Group::Resource).unwrap_err(),
        NO_LABOR
    );
}
