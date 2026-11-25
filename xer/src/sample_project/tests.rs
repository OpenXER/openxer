use super::*;
use crate::edit::records::{float, num};
use openxer_engine::model::{LinkType, TaskType};

fn day(p: &ProjectData, v: Option<i64>) -> String {
    crate::date_string(p.model.origin_serial, v)
        .get(..10)
        .unwrap_or("")
        .to_string()
}

/// Prints the shape of the sample: run with `--ignored --nocapture` while
/// tuning the table.
#[test]
#[ignore = "prints the sample's shape"]
fn print_sample_shape() {
    let s = sample_project();
    for (label, p) in [("plan", &s.plan), ("current", &s.current)] {
        let m = &p.model;
        println!(
            "== {label}: {} activities, {} links",
            m.activities.len(),
            m.links.len()
        );
        let finish = m.activities.iter().filter_map(|a| a.early_finish).max();
        println!(
            "data date {} finish {}",
            day(p, m.project.data_date),
            day(p, finish)
        );
        let crit: Vec<&str> = m
            .activities
            .iter()
            .filter(|a| !a.is_completed() && a.total_float.is_some_and(|f| f <= 0))
            .map(|a| a.task_code.as_str())
            .collect();
        println!("critical ({}): {}", crit.len(), crit.join(" "));
        let near: Vec<String> = m
            .activities
            .iter()
            .filter(|a| !a.is_completed() && a.total_float.is_some_and(|f| f > 0 && f <= 600 * 10))
            .map(|a| format!("{}:{}", a.task_code, a.total_float.unwrap() / 60))
            .collect();
        println!("near critical (h): {}", near.join(" "));
        let big: Vec<String> = m
            .activities
            .iter()
            .filter(|a| !a.is_completed() && a.total_float.is_some_and(|f| f > 352 * 60))
            .map(|a| format!("{}:{}", a.task_code, a.total_float.unwrap() / 60))
            .collect();
        println!("float over 352 h: {}", big.join(" "));
        let statuses = [
            StatusType::Completed,
            StatusType::InProgress,
            StatusType::NotStarted,
        ]
        .map(|s| m.activities.iter().filter(|a| a.status == s).count());
        println!("completed / in progress / not started: {statuses:?}");
        let open: Vec<&str> = m
            .activities
            .iter()
            .filter(|a| a.preds.is_empty() || a.succs.is_empty())
            .map(|a| a.task_code.as_str())
            .collect();
        println!("open ends: {}", open.join(" "));
        let lags = m.links.iter().filter(|l| l.lag > 0).count();
        let non_fs = m
            .links
            .iter()
            .filter(|l| l.link_type != LinkType::FS)
            .count();
        println!("lagged links {lags}, non FS {non_fs}");
        for a in &m.activities {
            if a.status == StatusType::InProgress {
                println!(
                    "  in progress {} start {} rem {} h finish {}",
                    a.task_code,
                    day(p, a.act_start),
                    a.rem / 60,
                    day(p, a.early_finish)
                );
            }
        }
    }
    // Late against the plan, and the baseline execution index.
    let p = &s.current;
    let plan_finish = |id: i64| {
        s.plan
            .model
            .activities
            .iter()
            .find(|a| a.task_id == id)
            .and_then(|a| a.early_finish)
            .map(|f| f + (s.plan.model.origin_serial - p.model.origin_serial) * 1440)
    };
    let late: Vec<&str> = p
        .model
        .activities
        .iter()
        .filter(|a| !a.is_completed() && a.task_type == TaskType::Task)
        .filter(|a| a.early_finish > plan_finish(a.task_id))
        .map(|a| a.task_code.as_str())
        .collect();
    println!("late against the plan: {}", late.join(" "));
    let dd = p.model.project.data_date.unwrap();
    let due = p
        .model
        .activities
        .iter()
        .filter(|a| plan_finish(a.task_id).is_some_and(|f| f <= dd))
        .count();
    let done = p
        .model
        .activities
        .iter()
        .filter(|a| a.act_finish.is_some())
        .count();
    println!("BEI {done}/{due}");
    // Peak units a day by resource, from the current early dates.
    let t = p.document.table("TASKRSRC").unwrap();
    let mut load: BTreeMap<(i64, i64), f64> = BTreeMap::new();
    for r in &t.rows {
        let task = num(t.value(r, "task_id"));
        let a = p
            .model
            .activities
            .iter()
            .find(|a| a.task_id == task)
            .unwrap();
        if a.is_completed() {
            continue;
        }
        let (Some(s), Some(f)) = (a.early_start, a.early_finish) else {
            continue;
        };
        let units = float(t.value(r, "remain_qty_per_hr"));
        for d in s / 1440..=(f - 1) / 1440 {
            *load.entry((num(t.value(r, "rsrc_id")), d)).or_default() += units;
        }
    }
    let rt = p.document.table("RSRCRATE").unwrap();
    let names = p.document.table("RSRC").unwrap();
    for r in &names.rows {
        let id = num(names.value(r, "rsrc_id"));
        let peak = load
            .iter()
            .filter(|((k, _), _)| *k == id)
            .map(|(_, v)| *v)
            .fold(0., f64::max);
        let over = load
            .iter()
            .filter(|((k, _), _)| *k == id)
            .filter(|(_, v)| {
                let limit = rt
                    .rows
                    .iter()
                    .find(|x| num(rt.value(x, "rsrc_id")) == id)
                    .map(|x| float(rt.value(x, "max_qty_per_hr")))
                    .unwrap_or(0.);
                limit > 0. && **v > limit + 1e-9
            })
            .count();
        let limit = rt
            .rows
            .iter()
            .find(|x| num(rt.value(x, "rsrc_id")) == id)
            .map(|x| float(rt.value(x, "max_qty_per_hr")))
            .unwrap_or(0.);
        let mut weeks: BTreeMap<i64, f64> = BTreeMap::new();
        for ((k, d), v) in &load {
            if *k == id {
                *weeks
                    .entry((d + p.model.origin_serial - 2) / 7)
                    .or_default() += v;
            }
        }
        let over_weeks = weeks
            .values()
            .filter(|v| limit > 0. && **v > limit * 6. + 1e-9)
            .count();
        print!("weeks over {over_weeks} ");
        if ["ELEC", "PF", "INST"].contains(&names.value(r, "rsrc_short_name")) {
            let w: Vec<String> = weeks.values().map(|v| format!("{:.0}", v)).collect();
            println!("limit/week {} : {}", limit * 6., w.join(" "));
        }
        println!(
            "{:6} peak {:5.2} over-limit days {over}",
            names.value(r, "rsrc_short_name"),
            peak
        );
    }
    let budget: f64 = t
        .rows
        .iter()
        .map(|r| float(t.value(r, "target_cost")))
        .sum();
    let hours: f64 = t
        .rows
        .iter()
        .filter(|r| t.value(r, "rsrc_type") == "RT_Labor")
        .map(|r| float(t.value(r, "target_qty")))
        .sum();
    let e = p.document.table("PROJCOST").unwrap();
    let expenses: f64 = e
        .rows
        .iter()
        .map(|r| float(e.value(r, "target_cost")))
        .sum();
    let actual: f64 = t
        .rows
        .iter()
        .map(|r| float(t.value(r, "act_reg_cost")))
        .sum();
    println!("labor hours {hours:.0}, resource cost {budget:.0}, expenses {expenses:.0}, actual {actual:.0}");
    let _ = TaskType::Task;
}

/// The sample, built once for the tests here.
fn sample() -> &'static Sample {
    static SAMPLE: std::sync::OnceLock<Sample> = std::sync::OnceLock::new();
    SAMPLE.get_or_init(sample_project)
}

fn table_rows<'a>(p: &'a ProjectData, table: &str) -> (&'a crate::Table, &'a [Vec<String>]) {
    let t = p.document.table(table).unwrap();
    (t, &t.rows)
}

#[test]
fn the_sample_is_a_mid_size_job() {
    let s = sample();
    let m = &s.current.model;
    assert!(
        (150..=250).contains(&m.activities.len()),
        "{}",
        m.activities.len()
    );
    assert_eq!(m.project.name, SAMPLE_PROJECT_ID);
    assert_eq!(s.current.source_name, SAMPLE_SOURCE);
    // Three or four WBS levels under the project node.
    let depth = |id: i64| {
        let mut d = 0;
        let mut at = Some(id);
        while let Some(i) = at {
            at = s
                .current
                .wbs
                .iter()
                .find(|w| w.id == i)
                .and_then(|w| w.parent);
            d += 1;
        }
        d
    };
    let deepest = s.current.wbs.iter().map(|w| depth(w.id)).max().unwrap();
    assert!((3..=4).contains(&(deepest - 1)), "{deepest}");
}

#[test]
fn every_task_is_labor_and_cost_loaded() {
    let p = &sample().current;
    let (t, rows) = table_rows(p, "TASKRSRC");
    for a in &p.model.activities {
        let mine: Vec<&Vec<String>> = rows
            .iter()
            .filter(|r| num(t.value(r, "task_id")) == a.task_id)
            .collect();
        if a.task_type != TaskType::Task {
            assert!(mine.is_empty(), "{} is a milestone", a.task_code);
            continue;
        }
        assert!(
            mine.iter().any(|r| t.value(r, "rsrc_type") == "RT_Labor"),
            "{} has no labor",
            a.task_code
        );
        assert!(
            mine.iter().all(|r| float(t.value(r, "target_cost")) > 0.),
            "{} has an assignment without cost",
            a.task_code
        );
    }
}

#[test]
fn only_the_start_and_finish_are_open_ends() {
    for p in [&sample().plan, &sample().current] {
        let m = &p.model;
        let starts: Vec<&str> = m
            .activities
            .iter()
            .filter(|a| a.preds.is_empty())
            .map(|a| a.task_code.as_str())
            .collect();
        let finishes: Vec<&str> = m
            .activities
            .iter()
            .filter(|a| a.succs.is_empty())
            .map(|a| a.task_code.as_str())
            .collect();
        assert_eq!((starts, finishes), (vec!["MS0000"], vec!["PM0920"]));
    }
}

#[test]
fn a_critical_path_runs_to_the_finish_milestone() {
    let m = &sample().current.model;
    let finish = m
        .activities
        .iter()
        .find(|a| a.task_code == "PM0920")
        .unwrap();
    assert_eq!(finish.total_float, Some(0));
    let last = m.activities.iter().filter_map(|a| a.early_finish).max();
    assert_eq!(
        finish.early_finish, last,
        "the finish milestone ends the project"
    );
    // Walk back from the finish along zero-float predecessors to the
    // outage, the window that drives it.
    let mut at = m
        .activities
        .iter()
        .position(|a| a.task_code == "PM0920")
        .unwrap();
    let mut path = vec![];
    while let Some(&l) = m.activities[at].preds.iter().find(|&&l| {
        m.activities[m.links[l].pred].total_float == Some(0)
            && !m.activities[m.links[l].pred].is_completed()
    }) {
        at = m.links[l].pred;
        path.push(m.activities[at].task_code.clone());
    }
    assert!(path.len() >= 10, "{path:?}");
    assert_eq!(path.last().map(String::as_str), Some("PP3100"), "{path:?}");
    // Near-critical paths: the rack header and the heat exchanger.
    for code in ["PP2000", "PP3000", "CV1010"] {
        let a = m.activities.iter().find(|a| a.task_code == code).unwrap();
        let f = a.total_float.unwrap();
        assert!(f > 0 && f <= 120 * 60, "{code}: {f}");
    }
}

#[test]
fn budgets_add_up() {
    for p in [&sample().plan, &sample().current] {
        let (t, rows) = table_rows(p, "TASKRSRC");
        let pid = p.model.project.proj_id;
        for a in &p.model.activities {
            let mine: Vec<&Vec<String>> = rows
                .iter()
                .filter(|r| num(t.value(r, "task_id")) == a.task_id)
                .collect();
            let sum = |kind: &str, column: &str| -> f64 {
                mine.iter()
                    .filter(|r| t.value(r, "rsrc_type") == kind)
                    .map(|r| float(t.value(r, column)))
                    .sum()
            };
            let task =
                |column: &str| float(&records::task_value(&p.document, a.task_id, pid, column));
            for (kind, task_column, column) in [
                ("RT_Labor", "target_work_qty", "target_qty"),
                ("RT_Labor", "act_work_qty", "act_reg_qty"),
                ("RT_Labor", "remain_work_qty", "remain_qty"),
                ("RT_Equip", "target_equip_qty", "target_qty"),
            ] {
                assert!(
                    (task(task_column) - sum(kind, column)).abs() < 0.01,
                    "{} {task_column}",
                    a.task_code
                );
            }
            // Each assignment's costs are its units at its price.
            for r in &mine {
                let price = float(t.value(r, "cost_per_qty"));
                for (units, cost) in [
                    ("target_qty", "target_cost"),
                    ("act_reg_qty", "act_reg_cost"),
                    ("remain_qty", "remain_cost"),
                ] {
                    let expected = float(t.value(r, units)) * price;
                    assert!(
                        (float(t.value(r, cost)) - expected).abs() < 0.01,
                        "{} {cost}",
                        a.task_code
                    );
                }
            }
        }
        let budget: f64 = rows.iter().map(|r| float(t.value(r, "target_cost"))).sum();
        assert!((2_000_000. ..3_000_000.).contains(&budget), "{budget}");
    }
    // The plan carries no actuals; the budget is the same in both.
    let total = |p: &ProjectData, column: &str| -> f64 {
        let (t, rows) = table_rows(p, "TASKRSRC");
        rows.iter().map(|r| float(t.value(r, column))).sum()
    };
    let (plan, current) = (&sample().plan, &sample().current);
    assert_eq!(total(plan, "act_reg_cost"), 0.);
    assert_eq!(total(plan, "target_cost"), total(current, "target_cost"));
    assert!(total(current, "act_reg_cost") > 0.);
}

#[test]
fn progress_agrees_with_the_data_date() {
    let p = &sample().current;
    let m = &p.model;
    let dd = m.project.data_date.unwrap();
    let plan_dd = sample().plan.model.project.data_date.unwrap();
    assert!(dd > plan_dd);
    let mut counts = [0; 3];
    for a in &m.activities {
        let pct = p.details[&a.task_id].percent;
        match a.status {
            StatusType::Completed => {
                counts[0] += 1;
                assert!(a.act_finish.unwrap() <= dd, "{}", a.task_code);
                assert!(a.act_start.unwrap() <= a.act_finish.unwrap());
                assert_eq!(pct, 100., "{}", a.task_code);
            }
            StatusType::InProgress => {
                counts[1] += 1;
                assert!(a.act_start.unwrap() < dd, "{}", a.task_code);
                assert!(a.early_finish.unwrap() > dd, "{}", a.task_code);
                assert!(pct > 0. && pct < 100., "{} {pct}", a.task_code);
            }
            StatusType::NotStarted => {
                counts[2] += 1;
                assert!(a.early_start.unwrap() >= dd, "{}", a.task_code);
                assert_eq!(pct, 0., "{}", a.task_code);
            }
        }
    }
    // Roughly a third of the way in: a third of the activities are done and
    // a few are in progress.
    let total = m.activities.len() as f64;
    assert!(
        (0.25..0.4).contains(&(counts[0] as f64 / total)),
        "{counts:?}"
    );
    assert!(counts[1] >= 3, "{counts:?}");
    // No progress out of sequence.
    for l in &m.links {
        let (pred, succ) = (&m.activities[l.pred], &m.activities[l.succ]);
        if succ.act_start.is_some() && l.link_type == LinkType::FS {
            assert!(
                pred.act_finish.is_some(),
                "{} before {}",
                succ.task_code,
                pred.task_code
            );
        }
    }
}

#[test]
fn the_sample_round_trips_through_xer() {
    let p = &sample().current;
    let text = p.updated_document().text();
    let mut back =
        ProjectData::from_document(Document::parse(&text).unwrap(), 1, SAMPLE_SOURCE.into())
            .unwrap();
    assert_eq!(
        back.updated_document().text(),
        text,
        "export, import, export"
    );
    assert_eq!(back.details, p.details);
    assert_eq!(back.wbs, p.wbs);
    schedule(&mut back);
    assert_eq!(back.model, {
        // The same model once both are scheduled (a completed activity's
        // remaining dates are worked out by the schedule, not stored).
        let mut again = p.clone();
        schedule(&mut again);
        again.model
    });
    let dates = |p: &ProjectData| -> Vec<_> {
        p.model
            .activities
            .iter()
            .map(|a| {
                (
                    a.early_start,
                    a.early_finish,
                    a.late_start,
                    a.late_finish,
                    a.total_float,
                )
            })
            .collect()
    };
    assert_eq!(dates(&back), dates(p), "F9 after the round trip");
    assert_eq!(back.updated_document().text(), text);
    // Leveling runs and keeps every activity dated.
    let mut leveled = back.clone();
    leveled.model.options.do_level = true;
    schedule(&mut leveled);
    assert!(leveled
        .model
        .activities
        .iter()
        .all(|a| a.is_completed() || a.early_start.is_some()));
}

#[test]
fn the_sample_is_the_same_every_time() {
    let again = sample_project();
    assert_eq!(
        again.current.updated_document().text(),
        sample().current.updated_document().text()
    );
    assert_eq!(
        again.plan.updated_document().text(),
        sample().plan.updated_document().text()
    );
}

#[test]
fn the_sample_has_codes_fields_steps_expenses_curves_and_notes() {
    let p = &sample().current;
    let count = |table: &str| p.document.table(table).map_or(0, |t| t.rows.len());
    assert_eq!(count("ACTVTYPE"), 4);
    assert_eq!(count("TASKACTV"), 4 * p.model.activities.len());
    assert!(count("UDFVALUE") > 50);
    assert!(count("TASKPROC") >= 10);
    assert!(count("PROJCOST") >= 10);
    assert!(count("RSRCCURVDATA") >= 3);
    assert!(count("ROLES") >= 4);
    assert!(count("ACCOUNT") >= 5);
    let (t, rows) = table_rows(p, "MEMOTYPE");
    let topics: Vec<&str> = rows.iter().map(|r| t.value(r, "memo_type")).collect();
    for topic in [
        "Scope",
        "Basis and Assumptions",
        "Safety",
        "Field Notes",
        "Change Notes",
    ] {
        assert!(topics.contains(&topic), "{topics:?}");
    }
    assert!(count("TASKMEMO") >= 12);
    assert!(count("WBSMEMO") >= 6);
    // Codes are the project's own, calendars and resources are named for it.
    let (t, rows) = table_rows(p, "ACTVTYPE");
    assert!(rows
        .iter()
        .all(|r| t.value(r, "actv_code_type_scope") == "AS_Project"));
    let (t, rows) = table_rows(p, "CALENDAR");
    assert!(rows
        .iter()
        .all(|r| t.value(r, "clndr_name").starts_with("Riverbend ")));
}

/// Field rules the sample keeps: demolition before installation in each
/// area and discipline, a radiograph after every welded tie-in, only field
/// paint, and the owner furnished heat exchanger as a delivery milestone.
#[test]
fn the_sample_follows_the_field_rules() {
    let m = &sample().current.model;
    let a = |code: &str| {
        m.activities
            .iter()
            .position(|a| a.task_code == code)
            .unwrap()
    };
    let reaches = |from: usize, to: usize| {
        let mut stack = vec![from];
        let mut seen = std::collections::BTreeSet::new();
        while let Some(i) = stack.pop() {
            if i == to {
                return true;
            }
            if seen.insert(i) {
                stack.extend(m.activities[i].succs.iter().map(|&l| m.links[l].succ));
            }
        }
        false
    };
    for (demo, install) in [
        ("DM1010", "EL1000"),
        ("DM1020", "PP1000"),
        ("DM1030", "EQ1010"),
        ("DM1040", "CV1000"),
        ("DM3000", "CV3000"),
        ("PP3110", "PP3120"),
        ("PP3210", "PP3220"),
        ("PP3310", "PP3320"),
    ] {
        assert!(reaches(a(demo), a(install)), "{demo} before {install}");
    }
    for (weld, xray) in [("PP3120", "PP3130"), ("PP3220", "PP3230")] {
        assert!(reaches(a(weld), a(xray)), "{weld} is radiographed");
    }
    for act in &m.activities {
        let name = sample().current.details[&act.task_id].name.to_lowercase();
        assert!(
            !name.contains("shop paint") && !name.contains("shop prime"),
            "{name}"
        );
        if name.contains("paint") {
            assert!(name.starts_with("field paint"), "{name}");
        }
    }
    let owner = &m.activities[a("PR0320")];
    assert_eq!(owner.task_type, TaskType::FinMile);
}

/// No product, client or job name from the private lists is in the sample.
#[test]
fn the_sample_names_no_listed_word() {
    let mut words: Vec<String> = vec![];
    if let Some(lines) = openxer_engine::fixture::fixture_list("internal-refs.txt") {
        words.extend(lines.iter().map(|f| f.join(" ").to_lowercase()));
    }
    if let Some(lines) = openxer_engine::fixture::fixture_list("corpus-names.txt") {
        for f in lines.iter().filter(|f| f.len() >= 2) {
            let name = f[1..].join(" ");
            let stem = name.rsplit_once('.').map_or(name.as_str(), |(s, _)| s);
            if stem.chars().any(|c| c.is_alphabetic()) && stem.len() >= 4 {
                words.push(stem.to_lowercase());
            }
        }
    }
    if words.is_empty() {
        return;
    }
    let text = format!(
        "{}\n{}",
        DATA.to_lowercase(),
        sample().current.updated_document().text().to_lowercase()
    );
    let found: Vec<&String> = words.iter().filter(|w| text.contains(w.as_str())).collect();
    assert!(found.is_empty(), "{} listed words found", found.len());
}

/// The S-Curve and Earned Hours reports run on the sample against its
/// as-planned baseline: labor budget spread over the job, actual hours up to
/// the data date and none after it, and earned hours to date.
#[test]
fn the_s_curve_and_earned_hours_reports_run_on_the_sample() {
    use crate::scurve::{self, Detection, Group, Options, Source};
    use std::sync::Arc;
    let s = sample();
    let p = &s.current;
    let baseline = Arc::new(
        crate::ev::load(
            "As planned",
            s.plan.updated_document(),
            1,
            p.model.origin_serial,
        )
        .unwrap(),
    );
    let mut src = Source::new(p);
    src.project_baseline = Some(baseline.clone());
    src.primary_baseline = Some(baseline);
    let sources = [src];
    let detection = Detection::of(&sources);
    assert_eq!(detection.measure(), scurve::Measure::Labor);
    let curve = scurve::curve(&sources, Options::detected(&detection)).unwrap();
    let t = &curve.total;
    let budget: f64 = t.budget.iter().sum();
    let actual: f64 = t.actual.iter().sum();
    let remaining: f64 = t.remaining.iter().sum();
    assert!(budget > 20_000., "{budget}");
    assert!(actual > 1_000. && actual < budget / 2., "{actual}");
    assert!(remaining > budget / 2., "{remaining}");
    // Actual hours fall in the periods up to the data date only.
    let dd = curve.data_date.unwrap();
    for (i, (start, _)) in curve.periods.iter().enumerate() {
        if *start >= dd {
            assert_eq!(t.actual[i], 0., "actual after the data date in period {i}");
        }
    }
    // The budget rises and falls: its peak is in the middle of the job.
    let peak = (0..t.budget.len())
        .max_by(|&a, &b| t.budget[a].total_cmp(&t.budget[b]))
        .unwrap();
    assert!(
        peak > t.budget.len() / 4 && peak < t.budget.len() * 9 / 10,
        "{peak}"
    );
    let earned = curve.earned.unwrap();
    assert!(earned > 0. && earned < budget, "{earned}");
    assert!(!curve.lines.is_empty(), "broken down by resource");
    // Earned Hours: every craft that worked has earned hours.
    let hours = scurve::earned_hours(&sources[0], Group::Resource).unwrap();
    let total: f64 = hours.rows.iter().map(|r| r.earned).sum();
    assert!(
        (total - earned).abs() / earned < 0.05,
        "{total} against {earned}"
    );
    assert!(hours.rows.iter().any(|r| r.key == "ENGR" && r.earned > 0.));
    assert!(hours.rows.iter().all(|r| r.budgeted > 0.));
}
