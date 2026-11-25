//! Tests for the earned value, percent complete, count, step, feedback and
//! other added columns, relative-date and nested filters, Group Interval
//! and Page Break, and the reference layout's columns.
use super::columns::{self, Kind, Value};
use super::filters::{self, Criterion, FilterSelection, Op, UserFilter, GROUP_ANY};
use super::layout::{self, ColumnSetting, GroupLevel};
use super::relative;
use super::*;
use crate::qa_sweep::Driver;
use crate::records;
use std::path::PathBuf;

fn scheduled_starter() -> ProjectData {
    let mut p = openxer_xer::starter();
    schedule_model(&mut p.model, &p.resources).unwrap();
    p
}

fn add_rows(p: &mut ProjectData, table: &str, rows: &[&[(&str, String)]]) {
    for r in rows {
        records::add_row(&mut p.document, table, r);
    }
}

/// The starter with progress on RB-1010 (a third done, one day into its
/// three), labor and equipment assignments, steps, feedback, a role, a
/// leveling priority and stored added dates. The data date moves one day.
pub(crate) fn progressed_starter() -> ProjectData {
    let mut p = scheduled_starter();
    let first = p.model.activities[1].task_id;
    let second = p.model.activities[7].task_id;
    add_rows(
        &mut p,
        "RSRC",
        &[
            &[
                ("rsrc_id", "5".into()),
                ("rsrc_short_name", "PF".into()),
                ("rsrc_name", "Pipefitter".into()),
                ("rsrc_type", "RT_Labor".into()),
            ],
            &[
                ("rsrc_id", "6".into()),
                ("rsrc_short_name", "CR".into()),
                ("rsrc_name", "Crane".into()),
                ("rsrc_type", "RT_Equip".into()),
            ],
        ],
    );
    add_rows(
        &mut p,
        "ROLES",
        &[&[
            ("role_id", "3".into()),
            ("role_short_name", "FIT".into()),
            ("role_name", "Fitter".into()),
        ]],
    );
    let assignment = |id: &str, task: i64, rsrc: &str, role: &str, q: [f64; 6]| {
        vec![
            ("taskrsrc_id", id.to_string()),
            ("task_id", task.to_string()),
            ("proj_id", "1".into()),
            ("rsrc_id", rsrc.to_string()),
            ("role_id", role.to_string()),
            ("target_qty", q[0].to_string()),
            ("act_reg_qty", q[1].to_string()),
            ("remain_qty", q[2].to_string()),
            ("target_cost", q[3].to_string()),
            ("act_reg_cost", q[4].to_string()),
            ("remain_cost", q[5].to_string()),
        ]
    };
    let rows = [
        assignment("1", first, "5", "3", [24., 8., 16., 1200., 400., 800.]),
        assignment("2", second, "5", "", [40., 0., 40., 2000., 0., 2000.]),
        assignment("3", second, "6", "", [10., 0., 10., 500., 0., 500.]),
    ];
    for r in &rows {
        records::add_row(&mut p.document, "TASKRSRC", r);
    }
    add_rows(
        &mut p,
        "TASKPROC",
        &[
            &[
                ("proc_id", "1".into()),
                ("task_id", first.to_string()),
                ("proj_id", "1".into()),
                ("seq_num", "1".into()),
                ("proc_name", "Set out".into()),
                ("complete_flag", "Y".into()),
                ("proc_wt", "1".into()),
            ],
            &[
                ("proc_id", "2".into()),
                ("task_id", first.to_string()),
                ("proj_id", "1".into()),
                ("seq_num", "2".into()),
                ("proc_name", "Survey".into()),
                ("complete_flag", "N".into()),
                ("proc_wt", "3".into()),
            ],
        ],
    );
    add_rows(
        &mut p,
        "TASKFDBK",
        &[&[
            ("task_id", first.to_string()),
            ("proj_id", "1".into()),
            ("task_fdbk", "<p>Grid lines staked</p>".into()),
        ]],
    );
    let doc = &mut p.document;
    records::set_task_value(doc, first, 1, "priority_type", "PT_High".into());
    records::set_task_value(doc, first, 1, "create_date", "2026-10-01 09:30".into());
    records::set_task_value(doc, first, 1, "update_date", "2026-10-20 14:00".into());
    records::set_task_value(doc, first, 1, "create_user", "planner".into());
    records::set_task_value(doc, first, 1, "rev_fdbk_flag", "Y".into());
    // A third done, one working day after the start.
    let start = p.model.activities[1].early_start.unwrap();
    let a = &mut p.model.activities[1];
    a.status = StatusType::InProgress;
    a.act_start = Some(start);
    a.rem = 16 * 60;
    p.model.project.data_date = Some(start + DAY);
    p
}

/// An app whose context has drawn nothing yet, so a software painter sees
/// every texture from the first frame.
fn fresh_app(name: &str, project: ProjectData) -> (PathBuf, egui::Context, App) {
    let path = std::env::temp_dir().join(format!(
        "openxer-h3-{name}-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.);
    let app = App::new(&ctx, Store::open(path.clone()).unwrap(), project);
    (path, ctx, app)
}

fn get(p: &ProjectData, ix: &columns::Index, id: &str, i: usize) -> Value {
    let catalog = columns::catalog(p);
    let c = columns::find(&catalog, id).unwrap_or_else(|| panic!("no column {id}"));
    columns::value(p, ix, c, i)
}

fn number(v: &Value) -> f64 {
    v.number().unwrap_or_else(|| panic!("{v:?} has no number"))
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

/// Every percent column stays within 0 to 100.
fn percents_in_range(p: &ProjectData) {
    let catalog = columns::catalog(p);
    let ix = columns::Index::new((false, 0.));
    for c in catalog.iter().filter(|c| c.kind == Kind::Percent) {
        for i in 0..p.model.activities.len() {
            if let Some(v) = columns::value(p, &ix, c, i).number() {
                assert!((0. ..=100.).contains(&v), "{} = {v} for activity {i}", c.id);
            }
        }
    }
}

#[test]
fn added_columns_have_values_on_the_starter_project() {
    let p = progressed_starter();
    let ix = columns::Index::new((false, 0.));
    let i = 1;
    let text = |id: &str| columns::display(&p, Some(i), &get(&p, &ix, id, i));
    // General.
    assert_eq!(text("project_id"), "RIVERBEND");
    assert_eq!(text("leveling_priority"), "High");
    assert_eq!(text("resource_ids"), "PF");
    assert_eq!(text("resources"), "Pipefitter");
    assert_eq!(text("roles"), "Fitter");
    assert_eq!(text("added_by"), "planner");
    assert_eq!(
        get(&p, &ix, "added_date", i),
        Value::Date {
            minute: openxer_xer::parse_relative(p.model.origin_serial, "2026-10-01 09:30")
                .unwrap()
                .unwrap(),
            mark: crate::format::DateMark::Plain
        }
    );
    assert!(!matches!(
        get(&p, &ix, "last_modified_date", i),
        Value::Empty
    ));
    assert_eq!(text("resources").len(), "Pipefitter".len());
    assert_eq!(
        columns::display(&p, Some(7), &get(&p, &ix, "resource_ids", 7)),
        "PF, CR"
    );
    // Feedback and steps.
    assert_eq!(text("feedback"), "Grid lines staked");
    assert_eq!(get(&p, &ix, "new_feedback", i), Value::Flag(true));
    assert_eq!(get(&p, &ix, "number_of_steps", i), Value::Number(2.));
    assert_eq!(get(&p, &ix, "completed_steps", i), Value::Number(1.));
    assert_eq!(
        get(&p, &ix, "steps_percent_complete", i),
        Value::Percent(25.)
    );
    assert_eq!(get(&p, &ix, "steps_percent_complete", 0), Value::Empty);
    // Counts.
    assert_eq!(get(&p, &ix, "number_of_activities", i), Value::Number(1.));
    assert_eq!(
        get(&p, &ix, "number_of_in_progress_activities", i),
        Value::Number(1.)
    );
    assert_eq!(
        get(&p, &ix, "number_of_not_started_activities", i),
        Value::Number(0.)
    );
    // Percent completes from the stored units and costs.
    let third = 100. / 3.;
    assert!(close(
        number(&get(&p, &ix, "labor_units_percent_complete", i)),
        third
    ));
    assert!(close(
        number(&get(&p, &ix, "cost_percent_complete", i)),
        third
    ));
    assert_eq!(
        get(&p, &ix, "labor_units_percent_complete", 7),
        Value::Percent(0.)
    );
    assert!(close(
        number(&get(&p, &ix, "units_percent_complete", i)),
        third
    ));
    // Schedule % Complete: one of three days has passed since the planned
    // start; Performance % Complete is the activity's percent complete.
    assert!(close(
        number(&get(&p, &ix, "schedule_percent_complete", i)),
        third
    ));
    assert!(close(
        number(&get(&p, &ix, "performance_percent_complete", i)),
        number(&get(&p, &ix, "activity_percent_complete", i))
    ));
    // Earned value with the open project as its own baseline.
    assert_eq!(
        get(&p, &ix, "budget_at_completion_labor_units", i),
        Value::Units(24.)
    );
    assert!(close(
        number(&get(&p, &ix, "planned_value_labor_units", i)),
        8.
    ));
    assert!(close(
        number(&get(&p, &ix, "earned_value_labor_units", i)),
        8.
    ));
    assert!(close(number(&get(&p, &ix, "planned_value_cost", i)), 400.));
    assert!(close(number(&get(&p, &ix, "earned_value_cost", i)), 400.));
    assert_eq!(get(&p, &ix, "actual_cost", i), Value::Cost(400.));
    assert_eq!(
        get(&p, &ix, "schedule_performance_index", i),
        Value::Number(1.)
    );
    assert_eq!(get(&p, &ix, "cost_performance_index", i), Value::Number(1.));
    assert!(close(number(&get(&p, &ix, "schedule_variance", i)), 0.));
    assert!(close(number(&get(&p, &ix, "cost_variance", i)), 0.));
    // ETC = remaining cost by default; EAC = AC + ETC; VAC = BAC - EAC.
    assert_eq!(get(&p, &ix, "estimate_to_complete", i), Value::Cost(800.));
    assert_eq!(
        get(&p, &ix, "estimate_at_completion_cost", i),
        Value::Cost(1200.)
    );
    assert!(close(
        number(&get(&p, &ix, "variance_at_completion", i)),
        0.
    ));
    assert_eq!(
        get(&p, &ix, "estimate_to_complete_labor_units", i),
        Value::Units(16.)
    );
    // A not started activity has no planned value yet and no indexes.
    assert_eq!(get(&p, &ix, "planned_value_cost", 7), Value::Cost(0.));
    assert_eq!(get(&p, &ix, "cost_performance_index", 7), Value::Empty);
    // Dates and durations.
    assert!(!matches!(get(&p, &ix, "remaining_float", 7), Value::Empty));
    assert_eq!(
        get(&p, &ix, "remaining_float", 7),
        p.model.activities[7]
            .total_float
            .map_or(Value::Empty, Value::Duration),
        "unprogressed: remaining float is total float"
    );
    percents_in_range(&p);
}

#[test]
fn earned_value_follows_the_wbs_techniques_and_band_totals_roll_up() {
    let mut p = progressed_starter();
    let wbs = p.details[&p.model.activities[1].task_id].wbs;
    // 50/50 and ETC = PF * (BAC - EV) with PF = 1 / CPI on the activity's WBS.
    let t = p.document.table_mut("PROJWBS");
    for field in ["ev_compute_type", "ev_etc_compute_type"] {
        if t.field(field).is_none() {
            t.fields.push(field.into());
            for r in &mut t.rows {
                r.push(String::new());
            }
        }
    }
    let (id_col, ev_col, etc_col) = (
        t.field("wbs_id").unwrap(),
        t.field("ev_compute_type").unwrap(),
        t.field("ev_etc_compute_type").unwrap(),
    );
    for r in &mut t.rows {
        if r[id_col] == wbs.to_string() {
            r[ev_col] = "EC_50_50".into();
            r[etc_col] = "EE_PF_cpi".into();
        }
    }
    let ix = columns::Index::new((false, 0.));
    let i = 1;
    assert_eq!(
        get(&p, &ix, "performance_percent_complete", i),
        Value::Percent(50.)
    );
    assert!(close(number(&get(&p, &ix, "earned_value_cost", i)), 600.));
    // CPI = 600 / 400 = 1.5; ETC = (1200 - 600) / 1.5 = 400.
    assert_eq!(
        get(&p, &ix, "cost_performance_index", i),
        Value::Number(1.5)
    );
    assert!(close(
        number(&get(&p, &ix, "estimate_to_complete", i)),
        400.
    ));
    // Band totals: sums, and indexes from the sums.
    let catalog = columns::catalog(&p);
    let members: Vec<usize> = (0..p.model.activities.len()).collect();
    let total = |id: &str| {
        group::band_total(&p, &ix, columns::find(&catalog, id).unwrap(), &members).unwrap()
    };
    assert_eq!(
        total("number_of_activities"),
        Value::Number(p.model.activities.len() as f64)
    );
    assert_eq!(total("budget_at_completion"), Value::Cost(3700.));
    assert!(close(number(&total("earned_value_cost")), 600.));
    assert_eq!(total("cost_performance_index"), Value::Number(1.5));
    let lu = number(&total("labor_units_percent_complete"));
    assert!(close(lu, 8. / 64. * 100.), "{lu}");
}

#[test]
fn earned_value_reads_the_assigned_baseline() {
    let p = progressed_starter();
    let mut ix = columns::Index::new((false, 0.));
    // A baseline whose RB-1010 budget was 30 hours and $1,500.
    let mut by_code = std::collections::HashMap::new();
    let a = &p.model.activities[1];
    let mut d = crate::baselines::data::current(&p, a);
    d.budget_cost = 1500.;
    d.budget_labor_units = 30.;
    by_code.insert(a.task_code.clone(), d);
    let loaded = std::sync::Arc::new(crate::baselines::data::Loaded {
        name: "BL".into(),
        by_code,
        project: None,
    });
    let mut compare = crate::baselines::Compare::default();
    compare.slots[0] = crate::baselines::data::Source::Baseline(loaded);
    ix.baselines = Some(std::sync::Arc::new(compare));
    assert_eq!(
        get(&p, &ix, "budget_at_completion_labor_units", 1),
        Value::Units(30.)
    );
    assert!(close(number(&get(&p, &ix, "planned_value_cost", 1)), 500.));
    // An activity the baseline lacks has no budget there.
    assert_eq!(get(&p, &ix, "budget_at_completion", 7), Value::Cost(0.));
}

fn corpus(name: &str) -> Option<ProjectData> {
    let path = crate::test_corpus::file(name);
    let doc = Document::read(&path).ok()?;
    let key = doc.projects()[0].0;
    let mut p = ProjectData::from_document(doc, key, name.into()).ok()?;
    schedule_model(&mut p.model, &p.resources).ok()?;
    Some(p)
}

#[test]
fn added_columns_match_the_stored_data_of_a_corpus_project() {
    let Some(p) = corpus("5640.xer") else {
        return;
    };
    let ix = columns::Index::new((false, 0.));
    let doc = &p.document;
    let t = doc.table("TASKRSRC").unwrap();
    let rsrc = doc.table("RSRC").unwrap();
    let labor: std::collections::HashSet<i64> = rsrc
        .rows
        .iter()
        .filter(|r| rsrc.value(r, "rsrc_type") == "RT_Labor")
        .map(|r| records::num(rsrc.value(r, "rsrc_id")))
        .collect();
    // (budget, actual, remaining) labor units per activity as stored.
    let mut stored: std::collections::HashMap<i64, (f64, f64, f64)> = Default::default();
    for r in &t.rows {
        if !labor.contains(&records::num(t.value(r, "rsrc_id"))) {
            continue;
        }
        let v = |k: &str| records::float(t.value(r, k));
        let e = stored
            .entry(records::num(t.value(r, "task_id")))
            .or_default();
        e.0 += v("target_qty");
        e.1 += v("act_reg_qty") + v("act_ot_qty");
        e.2 += v("remain_qty");
    }
    assert!(!stored.is_empty());
    let task = doc.table("TASK").unwrap();
    let mut checked = 0;
    for (i, a) in p.model.activities.iter().enumerate() {
        if let Some(&(budget, actual, remaining)) = stored.get(&a.task_id) {
            let bac = number(&get(&p, &ix, "budget_at_completion_labor_units", i));
            assert!(
                (bac - budget).abs() < 1e-6,
                "{}: {bac} vs {budget}",
                a.task_code
            );
            let pct = number(&get(&p, &ix, "labor_units_percent_complete", i));
            let want = if actual + remaining > 0. {
                actual / (actual + remaining) * 100.
            } else {
                0.
            };
            assert!(
                (pct - want).abs() < 1e-6,
                "{}: {pct} vs {want}",
                a.task_code
            );
            let ev = number(&get(&p, &ix, "earned_value_labor_units", i));
            assert!(ev <= bac + 1e-6 && ev >= 0.);
            checked += 1;
        }
        let row = task
            .rows
            .iter()
            .find(|r| records::num(task.value(r, "task_id")) == a.task_id)
            .unwrap();
        let priority = match task.value(row, "priority_type") {
            "PT_Top" => "Top",
            "PT_High" => "High",
            "PT_Normal" => "Normal",
            "PT_Low" => "Low",
            "PT_Lowest" => "Lowest",
            _ => "",
        };
        assert_eq!(
            columns::display(&p, Some(i), &get(&p, &ix, "leveling_priority", i)),
            priority
        );
        let added =
            openxer_xer::parse_relative(p.model.origin_serial, task.value(row, "create_date"))
                .ok()
                .flatten();
        assert_eq!(
            get(&p, &ix, "added_date", i),
            added.map_or(Value::Empty, |minute| Value::Date {
                minute,
                mark: crate::format::DateMark::Plain
            })
        );
        assert_eq!(get(&p, &ix, "number_of_activities", i), Value::Number(1.));
    }
    assert!(checked > 10, "{checked} activities with labor checked");
    let project = doc.table("PROJECT").unwrap();
    let short = project
        .value(&project.rows[0], "proj_short_name")
        .to_string();
    assert_eq!(
        columns::display(&p, Some(0), &get(&p, &ix, "project_id", 0)),
        short
    );
    percents_in_range(&p);
}

#[test]
fn columns_dialog_lists_the_reference_groups() {
    let p = progressed_starter();
    let catalog = columns::catalog(&p);
    for group in [
        "Activity Steps",
        "Costs",
        "Dates",
        "Durations",
        "Earned Value",
        "Feedback",
        "General",
        "Lists",
        "Multiple Float Paths",
        "Number of Activities",
        "Percent Completes",
        "Units",
    ] {
        assert!(
            catalog.iter().any(|c| c.group == group),
            "no {group} columns"
        );
    }
}

/// The reference layout's nine columns and measured widths.
const REFERENCE: [(&str, f32); 9] = [
    ("activity_id", 139.),
    ("activity_name", 409.),
    ("original_duration", 51.),
    ("budgeted_labor_units", 65.),
    ("earned_value_labor_units", 73.),
    ("labor_units_percent_complete", 69.),
    ("start", 99.),
    ("finish", 103.),
    ("bl_project_finish", 89.),
];

#[test]
fn the_reference_layout_columns_build_and_render_at_the_measured_widths() {
    use crate::headless_render::SoftwarePainter;
    let mut d = Driver::new("reference-layout", progressed_starter());
    d.app.grid.layout.columns = REFERENCE
        .iter()
        .map(|(id, w)| ColumnSetting::new(id, *w))
        .collect();
    d.app.grid_fraction = 0.9;
    d.frames(3);
    let catalog = columns::catalog(&d.app.project);
    let shown = d.app.visible_columns(&catalog);
    assert_eq!(shown.len(), 9, "every reference column exists");
    let titles: Vec<&str> = shown.iter().map(|c| c.title.as_str()).collect();
    assert_eq!(
        titles,
        [
            "Activity ID",
            "Activity Name",
            "Original Duration",
            "Budgeted Labor Units",
            "Earned Value Labor Units",
            "Labor Units % Complete",
            "Start",
            "Finish",
            "BL Project Finish"
        ]
    );
    // Header cells (inset 4 px each side) and resize handles follow the widths.
    let mut edges = vec![];
    for (id, w) in REFERENCE {
        let header = d.rect(&format!("grid_header_{id}"));
        assert!(
            (header.width() - (w - 8.)).abs() <= 1.,
            "{id}: {} for {w}",
            header.width()
        );
        edges.push(d.rect(&format!("grid_resize_{id}")).center().x);
    }
    for (k, pair) in edges.windows(2).enumerate() {
        assert!(
            (pair[1] - pair[0] - REFERENCE[k + 1].1).abs() <= 1.,
            "{} width",
            REFERENCE[k + 1].0
        );
    }
    let snapshot = d.app.current_layout_table();
    let row = snapshot
        .rows
        .iter()
        .find_map(|r| match r {
            TableRow::Activity {
                index: 1, texts, ..
            } => Some(texts.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        row[4],
        columns::display(&d.app.project, Some(1), &Value::Units(8.))
    );
    assert_eq!(row[5], "33.33%");
    let artifacts = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts");
    std::fs::create_dir_all(&artifacts).unwrap();
    d.assert_clean("reference layout");
    let (path, ctx, mut app) = fresh_app("reference", progressed_starter());
    app.grid.layout.columns = d.app.grid.layout.columns.clone();
    app.grid_fraction = 0.9;
    let mut painter = SoftwarePainter::default();
    painter.screen = Some(Vec2::new(1500., 900.));
    let mut time = 0.;
    for _ in 0..3 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    image
        .save(artifacts.join("headless-grid-reference-layout.png"))
        .unwrap();
    drop(app);
    let _ = std::fs::remove_file(path);
}

// ------------------------------------------------------------ filters

fn row(column: &str, op: Op, value: &str, high: &str, level: usize) -> Criterion {
    Criterion {
        column: column.into(),
        op,
        value: value.into(),
        high: high.into(),
        level,
    }
}

fn matching(p: &ProjectData, f: &UserFilter) -> Vec<usize> {
    let l = layout::Layout {
        filter: FilterSelection {
            all_activities: false,
            selected: vec![f.key()],
            match_all: true,
            highlight: false,
        },
        ..Default::default()
    };
    let catalog = columns::catalog(p);
    let ix = columns::Index::new((false, 0.));
    let built = group::build(
        p,
        &ix,
        &catalog,
        &l,
        &group::Scope {
            query: "",
            wbs_filter: None,
            collapsed: &BTreeSet::new(),
            users: std::slice::from_ref(f),
        },
    );
    let mut out: Vec<usize> = built
        .rows
        .iter()
        .filter_map(|r| match r {
            Row::Activity(i) => Some(*i),
            Row::Group(_) => None,
        })
        .collect();
    out.sort();
    out
}

#[test]
fn relative_dates_parse_and_resolve() {
    let p = scheduled_starter();
    let dd = p.model.project.data_date.unwrap();
    let r = relative::parse("DD + 3W").unwrap();
    assert_eq!(r.text(), "DD + 3w");
    assert_eq!(relative::resolve_at(&p, &r, 0), Some(dd + 21 * DAY));
    let r = relative::parse("data date - 10").unwrap();
    assert_eq!(relative::resolve_at(&p, &r, 0), Some(dd - 10 * DAY));
    // Months follow the calendar: 2 Nov + 1 month is 2 Dec.
    let r = relative::parse("dd+1m").unwrap();
    assert_eq!(relative::resolve_at(&p, &r, 0), Some(dd + 30 * DAY));
    // Start of this week and month from a known "now" (Wednesday 4 Nov).
    let now = dd + 2 * DAY;
    let sw = relative::parse("SW").unwrap();
    let sunday = (dd.div_euclid(DAY) - 1) * DAY;
    assert_eq!(relative::resolve_at(&p, &sw, now), Some(sunday));
    let sm = relative::parse("Start of This Month + 1w").unwrap();
    let first = (dd.div_euclid(DAY) - 1) * DAY;
    assert_eq!(relative::resolve_at(&p, &sm, now), Some(first + 7 * DAY));
    assert_eq!(
        relative::resolve_at(&p, &relative::parse("CD").unwrap(), now),
        Some(now)
    );
    for bad in [
        "",
        "2026-11-04",
        "DD +",
        "DD * 2",
        "DD + 3y",
        "DDD",
        "DD + 999999d",
    ] {
        assert!(relative::parse(bad).is_none(), "{bad:?}");
    }
    assert!(filters::check_value(&p, Kind::Date, "DD + 2w").is_ok());
    assert!(filters::check_value(&p, Kind::Number, "DD + 2w").is_err());
}

#[test]
fn a_three_week_lookahead_follows_the_data_date() {
    let mut p = scheduled_starter();
    let f = UserFilter {
        name: "Lookahead".into(),
        match_all: true,
        rows: vec![row("start", Op::Within, "DD", "DD + 3W", 0)],
    };
    let expect = |p: &ProjectData| -> Vec<usize> {
        let dd = p.model.project.data_date.unwrap().div_euclid(DAY);
        (0..p.model.activities.len())
            .filter(|&i| {
                let s = crate::util::displayed_start(&p.model.activities[i], false).unwrap();
                (dd..=dd + 21).contains(&s.div_euclid(DAY))
            })
            .collect()
    };
    let before = matching(&p, &f);
    assert_eq!(before, expect(&p));
    assert!(!before.is_empty() && before.len() < p.model.activities.len());
    // Move the data date four weeks on: the window moves with it.
    p.model.project.data_date = p.model.project.data_date.map(|d| d + 28 * DAY);
    let after = matching(&p, &f);
    assert_eq!(after, expect(&p));
    assert_ne!(after, before);
}

#[test]
fn nested_any_inside_all_filters() {
    let p = scheduled_starter();
    // Construction activities that are long or name structural work.
    let f = UserFilter {
        name: "Nested".into(),
        match_all: true,
        rows: vec![
            row("wbs_name", Op::Equals, "Construction", "", 0),
            row(GROUP_ANY, Op::Equals, "", "", 0),
            row("original_duration", Op::GreaterOrEqual, "48h", "", 1),
            row("activity_name", Op::Contains, "structural", "", 1),
        ],
    };
    let names: Vec<String> = matching(&p, &f)
        .into_iter()
        .map(|i| p.name(i).to_string())
        .collect();
    assert_eq!(
        names,
        ["Install structural assemblies", "Mechanical installation"]
    );
    // The same rows flat (all of them) match nothing.
    let flat = UserFilter {
        rows: f
            .rows
            .iter()
            .filter(|r| r.group().is_none())
            .map(|r| Criterion {
                level: 0,
                ..r.clone()
            })
            .collect(),
        ..f.clone()
    };
    assert!(matching(&p, &flat).is_empty());
    // The tree: a row deeper than allowed joins the group above.
    match filters::tree(&f) {
        filters::Node::Group(true, members) => {
            assert_eq!(members.len(), 2);
            assert!(matches!(&members[1], filters::Node::Group(false, inner) if inner.len() == 2));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn filters_save_in_a_layout_reload_and_reject_malformed_data() {
    let p = scheduled_starter();
    let f = UserFilter {
        name: "Lookahead nested".into(),
        match_all: true,
        rows: vec![
            row("start", Op::Within, "DD", "DD + 3w", 0),
            row(GROUP_ANY, Op::Equals, "", "", 0),
            row("activity_name", Op::Contains, "install", "", 1),
            row("activity_name", Op::Contains, "design", "", 1),
        ],
    };
    let l = layout::Layout {
        filter: FilterSelection {
            all_activities: false,
            selected: vec![f.key()],
            match_all: true,
            highlight: false,
        },
        ..Default::default()
    };
    let saved_filters = layout::filters_to_json(std::slice::from_ref(&f));
    let saved_layout = layout::library_to_json(std::slice::from_ref(&l));
    let filters_back = layout::filters_from_json(&saved_filters).unwrap();
    let layout_back = layout::library_from_json(&saved_layout).unwrap();
    assert_eq!(filters_back, vec![f.clone()]);
    assert_eq!(layout_back[0].filter, l.filter);
    assert_eq!(matching(&p, &filters_back[0]), matching(&p, &f));
    // Malformed rows are refused as a whole, never a panic.
    let v: serde_json::Value = serde_json::from_str(&saved_filters).unwrap();
    let bad = |edit: &dyn Fn(&mut serde_json::Value)| {
        let mut x = v.clone();
        edit(&mut x);
        layout::filters_from_json(&x.to_string())
    };
    for case in [
        bad(&|x| x[0]["rows"][1]["level"] = 99.into()),
        bad(&|x| x[0]["rows"][1]["level"] = "deep".into()),
        bad(&|x| x[0]["rows"][1]["level"] = (-1).into()),
        bad(&|x| x[0]["rows"][0]["op"] = "near".into()),
        bad(&|x| x[0]["rows"] = "none".into()),
        bad(&|x| x[0]["rows"][0]["value"] = 5.into()),
        bad(&|x| x[0]["name"] = "".into()),
    ] {
        assert!(case.is_err());
    }
    let old = r#"[{"name":"Old","match_all":true,"rows":[{"column":"start","op":"eq","value":"","high":""}]}]"#;
    assert_eq!(layout::filters_from_json(old).unwrap()[0].rows[0].level, 0);
    // Group Interval and Page Break are kept and checked too.
    let mut l2 = l.clone();
    l2.group_sort.levels = vec![GroupLevel {
        column: "start".into(),
        to_level: None,
        indent: true,
        interval: "week".into(),
    }];
    l2.group_sort.page_breaks = vec![0, 2];
    let back = layout::Layout::from_json(&l2.to_json()).unwrap();
    assert_eq!(back, l2);
    let mut j = l2.to_json();
    j["group_sort"]["page_breaks"] = serde_json::json!([0, 0]);
    assert!(layout::Layout::from_json(&j).is_err());
    j["group_sort"]["page_breaks"] = serde_json::json!([40]);
    assert!(layout::Layout::from_json(&j).is_err());
    j["group_sort"]["page_breaks"] = serde_json::Value::Null;
    j["group_sort"]["levels"][0]["interval"] = 7.into();
    assert!(layout::Layout::from_json(&j).is_err());
}

// ------------------------------------------------------------ grouping

fn bands(p: &ProjectData, gs: layout::GroupSort) -> Vec<group::Band> {
    let l = layout::Layout {
        group_sort: gs,
        ..Default::default()
    };
    let catalog = columns::catalog(p);
    let ix = columns::Index::new((false, 0.));
    group::build(
        p,
        &ix,
        &catalog,
        &l,
        &group::Scope {
            query: "",
            wbs_filter: None,
            collapsed: &BTreeSet::new(),
            users: &[],
        },
    )
    .bands
}

fn by(column: &str, interval: &str) -> layout::GroupSort {
    layout::GroupSort {
        levels: vec![GroupLevel {
            column: column.into(),
            to_level: None,
            indent: true,
            interval: interval.into(),
        }],
        ..Default::default()
    }
}

#[test]
fn group_interval_bands_dates_by_week_and_month_and_numbers_by_step() {
    let p = scheduled_starter();
    let origin = p.model.origin_serial;
    let week_of = |i: usize| {
        let s = crate::util::displayed_start(&p.model.activities[i], false).unwrap();
        // Weeks from Sunday; the data date is Monday 2 Nov 2026.
        let monday = p.model.project.data_date.unwrap().div_euclid(DAY);
        (s.div_euclid(DAY) - monday + 1).div_euclid(7)
    };
    let weeks = bands(&p, by("start", "week"));
    let days = bands(&p, by("start", ""));
    assert!(weeks.len() < days.len(), "weeks merge days");
    for b in &weeks {
        assert!(b.label.starts_with("Week of "), "{}", b.label);
        let w = week_of(b.members[0]);
        assert!(b.members.iter().all(|&i| week_of(i) == w), "{}", b.label);
    }
    let distinct: BTreeSet<i64> = (0..p.model.activities.len()).map(week_of).collect();
    assert_eq!(weeks.len(), distinct.len());
    let months = bands(&p, by("start", "month"));
    assert!(months.len() <= 3 && !months.is_empty());
    assert!(months[0].label.contains("2026"), "{}", months[0].label);
    let first_label = openxer_xer::short_date(origin, Some(0));
    assert!(!first_label.is_empty());
    // Original duration in steps of 3 days (24 h): 0-3d, 3-6d, 6-9d.
    let steps = bands(&p, by("original_duration", "24h"));
    let ix = columns::Index::new((false, 0.));
    for b in &steps {
        let low = b
            .members
            .iter()
            .map(|&i| number(&get(&p, &ix, "original_duration", i)).div_euclid(24. * 60.) as i64);
        let lows: BTreeSet<i64> = low.collect();
        assert_eq!(lows.len(), 1, "{}", b.label);
        assert!(b.label.contains(" - "), "{}", b.label);
    }
    assert_eq!(
        steps.len(),
        3,
        "{:?}",
        steps.iter().map(|b| &b.label).collect::<Vec<_>>()
    );
    // A bad interval groups by value.
    assert_eq!(bands(&p, by("start", "fortnight")).len(), days.len());
    assert!(super::interval::check(&p, Kind::Date, "fortnight").is_err());
    assert!(super::interval::check(&p, Kind::Duration, "-5").is_err());
    assert!(super::interval::check(&p, Kind::Duration, "5d").is_ok());
}

#[test]
fn page_break_starts_a_printed_page_per_band() {
    let mut d = Driver::new("page-break", scheduled_starter());
    let setup = crate::print::setup::PageSetup {
        scaling: crate::print::setup::Scaling::Adjust { percent: 100 },
        ..Default::default()
    };
    let plain = d.app.print_with(&setup).unwrap();
    let across = plain.across;
    let plain_blocks = plain.pages.len() / across;
    // Break before every top-level WBS band.
    d.app.grid.layout.group_sort.page_breaks = vec![0];
    d.app.grid.drop_rows_cache();
    let printed = d.app.print_with(&setup).unwrap();
    let _ = d.app.rows();
    let top_bands: Vec<String> = d
        .app
        .grid
        .bands
        .borrow()
        .iter()
        .filter(|b| b.level == 0)
        .map(|b| b.label.clone())
        .collect();
    assert_eq!(top_bands.len(), 4);
    assert_eq!(printed.pages.len() / printed.across, top_bands.len());
    assert!(printed.pages.len() / printed.across > plain_blocks);
    // Each page block opens with its band.
    for (k, label) in top_bands.iter().enumerate() {
        let texts = printed.pages[k * printed.across].texts().join("\n");
        assert!(texts.contains(label.as_str()), "page {k}: {label}");
        for other in top_bands.iter().filter(|o| *o != label) {
            assert!(!texts.contains(other.as_str()), "page {k} also has {other}");
        }
    }
}

// ------------------------------------------------------------ dialogs

#[test]
fn group_sort_dialog_sets_interval_and_page_break_and_cancel_keeps_the_layout() {
    let mut d = Driver::new("group-dialog", scheduled_starter());
    let before = d.app.grid.layout.clone();
    d.app.open_group_sort_dialog();
    d.frames(2);
    // WBS levels below the WBS row, as in the reference grid.
    assert!(d.has("group_page_break_0"));
    d.click("group_page_break_0");
    d.click("group_cancel");
    assert!(d.app.grid.dialogs.group_sort.is_none());
    assert_eq!(d.app.grid.layout, before, "Cancel leaves the layout");
    // Replace WBS by Start through the in-grid combo: select, click again, pick.
    d.app.open_group_sort_dialog();
    d.frames(2);
    d.click("group_by_0");
    let catalog = columns::catalog(&d.app.project);
    let titles: Vec<String> = {
        let mut rest: Vec<String> = catalog
            .iter()
            .filter(|c| c.id != "wbs" && !matches!(c.source, columns::Source::Notebook(_)))
            .map(|c| c.title.clone())
            .collect();
        rest.sort_by_key(|t| t.to_lowercase());
        rest
    };
    // The list scrolls; set the pick on the draft, as the combo would.
    assert!(d.has("group_by_0_item_0"), "the list opened");
    assert!(titles.iter().any(|t| t == "Start"));
    d.app.grid.dialogs.group_sort.as_mut().unwrap().gs.levels[0].column = "start".into();
    d.key(egui::Key::Escape);
    d.app.open_group_sort_dialog();
    d.app.grid.dialogs.group_sort.as_mut().unwrap().gs.levels[0].column = "start".into();
    d.frames(2);
    // Group Interval: a combo for dates.
    d.click("group_interval_0");
    d.click("group_interval_0_item_2");
    assert_eq!(
        d.app.grid.dialogs.group_sort.as_ref().unwrap().gs.levels[0].interval,
        "week"
    );
    d.click("group_page_break_0");
    d.click("group_ok");
    assert!(d.app.grid.dialogs.group_sort.is_none(), "{:?}", d.app.error);
    let gs = &d.app.grid.layout.group_sort;
    assert_eq!(gs.levels[0].column, "start");
    assert_eq!(gs.levels[0].interval, "week");
    assert_eq!(gs.page_breaks, vec![0]);
    let _ = d.app.rows();
    assert!(d
        .app
        .grid
        .bands
        .borrow()
        .iter()
        .all(|b| b.label.starts_with("Week of ")));
    // A bad numeric interval is refused on OK.
    d.app.open_group_sort_dialog();
    {
        let g = &mut d.app.grid.dialogs.group_sort.as_mut().unwrap().gs;
        g.levels[0].column = "total_float".into();
        g.levels[0].interval = "soon".into();
    }
    d.frames(2);
    d.click("group_ok");
    assert!(d.app.grid.dialogs.group_sort.is_some());
    assert!(d.app.error.take().is_some());
    d.key(egui::Key::Escape);
    assert!(d.app.grid.dialogs.group_sort.is_none());
    assert_eq!(d.app.grid.layout.group_sort.levels[0].column, "start");
}

#[test]
fn filter_editor_builds_a_nested_relative_date_filter() {
    let mut d = Driver::new("filter-editor", scheduled_starter());
    d.app.open_filters_dialog();
    d.frames(2);
    d.click("filters_new");
    assert!(d.app.grid.dialogs.filter_editor.is_some());
    d.click("filter_name");
    d.key_with(
        egui::Key::A,
        egui::Modifiers {
            ctrl: true,
            command: true,
            ..Default::default()
        },
    );
    d.type_text("Lookahead");
    // Row 0: Start is within DD .. DD + 3w, chosen through the editors.
    d.click("filter_where_0");
    d.click("filter_param_0");
    assert!(d.has("filter_param_0_item_0"));
    d.app
        .grid
        .dialogs
        .filter_editor
        .as_mut()
        .unwrap()
        .filter
        .rows[0]
        .column = "start".into();
    d.frames(2);
    d.click("filter_op_0");
    let within = Op::ALL.iter().position(|o| *o == Op::Within).unwrap();
    d.click(&format!("filter_op_0_item_{within}"));
    d.click("filter_value_anchor_0");
    d.click("filter_value_anchor_0_item_0");
    assert_eq!(
        d.app
            .grid
            .dialogs
            .filter_editor
            .as_ref()
            .unwrap()
            .filter
            .rows[0]
            .value,
        "DD"
    );
    d.click("filter_high_anchor_0");
    d.click("filter_high_anchor_0_item_0");
    d.click("filter_high_0");
    d.key(egui::Key::End);
    d.type_text(" + 3w");
    // Row 1: an Any group; row 2 nested under it.
    d.click("filter_add");
    d.app
        .grid
        .dialogs
        .filter_editor
        .as_mut()
        .unwrap()
        .filter
        .rows[1]
        .column = GROUP_ANY.into();
    d.frames(1);
    d.click("filter_add");
    {
        let e = d.app.grid.dialogs.filter_editor.as_ref().unwrap();
        assert_eq!(e.filter.rows[2].level, 1, "added inside the group");
    }
    d.app
        .grid
        .dialogs
        .filter_editor
        .as_mut()
        .unwrap()
        .filter
        .rows[2] = row("activity_name", Op::Contains, "install", "", 1);
    d.click("filter_add");
    d.app
        .grid
        .dialogs
        .filter_editor
        .as_mut()
        .unwrap()
        .filter
        .rows[3] = row("activity_name", Op::Contains, "design", "", 1);
    // Out of the group and back in with the move pad.
    d.click("filter_where_3");
    d.click("filter_left");
    assert_eq!(
        d.app
            .grid
            .dialogs
            .filter_editor
            .as_ref()
            .unwrap()
            .filter
            .rows[3]
            .level,
        0
    );
    d.click("filter_right");
    assert_eq!(
        d.app
            .grid
            .dialogs
            .filter_editor
            .as_ref()
            .unwrap()
            .filter
            .rows[3]
            .level,
        1
    );
    d.click("filter_ok");
    assert!(
        d.app.grid.dialogs.filter_editor.is_none(),
        "{:?}",
        d.app.error
    );
    let draft = d.app.grid.dialogs.filters.as_ref().unwrap();
    let f = draft
        .users
        .iter()
        .find(|u| u.name == "Lookahead")
        .unwrap()
        .clone();
    assert_eq!(f.rows[0].high, "DD + 3w");
    d.click("filter_select_user:Lookahead");
    d.click("filters_ok");
    let p = &d.app.project;
    assert_eq!(
        d.app
            .rows()
            .iter()
            .filter(|r| matches!(r, Row::Activity(_)))
            .count(),
        matching(p, &f).len()
    );
    assert!(!matching(p, &f).is_empty());
    // Cancel and Escape leave the saved filters alone.
    let saved = d.app.grid.user_filters.clone();
    d.app.open_filters_dialog();
    d.frames(2);
    d.click("filter_row_user:Lookahead");
    d.click("filters_modify");
    d.click("filter_where_0");
    d.click("filter_delete");
    d.key(egui::Key::Escape);
    assert!(d.app.grid.dialogs.filter_editor.is_none());
    d.click("filters_cancel");
    assert_eq!(d.app.grid.user_filters, saved);
    d.assert_clean("filter editor");
}

#[test]
fn render_h3_dialogs_to_png() {
    use crate::headless_render::SoftwarePainter;
    let artifacts = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts");
    std::fs::create_dir_all(&artifacts).unwrap();
    let (path, ctx, mut app) = fresh_app("h3-render", progressed_starter());
    let mut time = 0.;
    let mut painter = SoftwarePainter::default();
    painter.screen = Some(Vec2::new(1500., 900.));
    type Shot = (&'static str, fn(&mut App));
    let shots: [Shot; 3] = [
        ("headless-h3-filter-editor.png", |a| {
            a.open_filters_dialog();
            a.grid.dialogs.filter_editor = Some(super::dialogs::FilterEditor {
                original: None,
                filter: UserFilter {
                    name: "3 week lookahead".into(),
                    match_all: true,
                    rows: vec![
                        row("start", Op::Within, "DD", "DD + 3w", 0),
                        row(GROUP_ANY, Op::Equals, "", "", 0),
                        row("activity_name", Op::Contains, "install", "", 1),
                        row("total_float", Op::LessOrEqual, "0", "", 1),
                    ],
                },
                selected: Some(2),
            });
        }),
        ("headless-h3-group-sort.png", |a| {
            a.open_group_sort_dialog();
            if let Some(g) = a.grid.dialogs.group_sort.as_mut() {
                g.gs.page_breaks = vec![1];
            }
        }),
        ("headless-h3-columns-ev.png", |a| {
            a.open_columns_dialog();
            if let Some(c) = a.grid.dialogs.columns.as_mut() {
                c.open_groups.insert("Earned Value".into());
            }
        }),
    ];
    for (name, open) in shots {
        app.grid.dialogs = Default::default();
        open(&mut app);
        for _ in 0..3 {
            let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
        }
        let (image, _) = painter
            .frame(&mut app, &ctx, &mut time, vec![], true)
            .unwrap();
        image.save(artifacts.join(name)).unwrap();
    }
    assert!(app.error.is_none(), "{:?}", app.error);
    drop(app);
    let _ = std::fs::remove_file(path);
    // A two-level WBS lists WBS level 2 under the WBS row, with its own
    // Page Break and band style.
    let mut doc = openxer_xer::starter().document;
    let t = doc.table_mut("PROJWBS");
    if t.field("parent_wbs_id").is_none() {
        t.fields.push("parent_wbs_id".into());
        for r in &mut t.rows {
            r.push(String::new());
        }
    }
    let (id, parent) = (
        t.field("wbs_id").unwrap(),
        t.field("parent_wbs_id").unwrap(),
    );
    for r in &mut t.rows {
        if r[id] != "10" {
            r[parent] = "10".into();
        }
    }
    let mut nested = ProjectData::from_document(doc, 1, "nested".into()).unwrap();
    schedule_model(&mut nested.model, &nested.resources).unwrap();
    let (path, ctx, mut app) = fresh_app("h3-render-nested", nested);
    app.open_group_sort_dialog();
    if let Some(g) = app.grid.dialogs.group_sort.as_mut() {
        g.gs.page_breaks = vec![1];
    }
    for _ in 0..3 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    assert!(app.action_rects.contains_key("group_page_break_1"));
    assert!(app.action_rects.contains_key("group_band_1"));
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    image
        .save(artifacts.join("headless-h3-group-sort-nested.png"))
        .unwrap();
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn excel_export_writes_the_added_columns_from_the_registry() {
    use openxer_interchange::xlsx_import::{self as xi, Cell};
    let mut d = Driver::new("excel-h3", progressed_starter());
    d.app.grid.layout.columns = REFERENCE
        .iter()
        .map(|(id, w)| ColumnSetting::new(id, *w))
        .collect();
    d.frames(1);
    let file = d.dir.join("reference.xlsx");
    d.app.export_excel_to(&file);
    assert!(file.exists(), "{}", d.app.status);
    let wb = xi::read_workbook(&file).unwrap();
    let s = wb.sheet("Activities").unwrap();
    let headers: Vec<String> = wb.sheets[s].rows[0].iter().map(Cell::text).collect();
    let col = |h: &str| {
        headers
            .iter()
            .position(|x| x == h)
            .unwrap_or_else(|| panic!("no {h} in {headers:?}"))
    };
    let (ev, pct) = (
        col("Earned Value Labor Units"),
        col("Labor Units % Complete"),
    );
    assert!(col("BL Project Finish") > pct);
    let row = wb.sheets[s]
        .rows
        .iter()
        .find(|r| r[0].text() == "RB-1010")
        .unwrap();
    assert!(
        matches!(row[ev], Cell::Number(n) if (n - 8.).abs() < 1e-9),
        "{:?}",
        row[ev]
    );
    assert!(
        matches!(row[pct], Cell::Number(n) if (n - 100. / 3.).abs() < 1e-6),
        "{:?}",
        row[pct]
    );
}

#[test]
fn non_what_if_reads_the_project_status() {
    let mut p = scheduled_starter();
    let n = p.model.activities.len();
    let key = filters::Standard::NonWhatIf.key();
    let l = |key: &str| layout::Layout {
        filter: FilterSelection {
            all_activities: false,
            selected: vec![key.to_string()],
            match_all: true,
            highlight: false,
        },
        ..Default::default()
    };
    let count = |p: &ProjectData| {
        let catalog = columns::catalog(p);
        let ix = columns::Index::new((false, 0.));
        group::build(
            p,
            &ix,
            &catalog,
            &l(&key),
            &group::Scope {
                query: "",
                wbs_filter: None,
                collapsed: &BTreeSet::new(),
                users: &[],
            },
        )
        .rows
        .iter()
        .filter(|r| matches!(r, Row::Activity(_)))
        .count()
    };
    assert_eq!(count(&p), n);
    records::add_row(
        &mut p.document,
        "PROJWBS",
        &[
            ("wbs_id", "1".into()),
            ("proj_id", "1".into()),
            ("proj_node_flag", "Y".into()),
            ("status_code", "WS_Whatif".into()),
        ],
    );
    assert_eq!(count(&p), 0);
    // The Default list matches the reference: thirteen filters.
    let listed: Vec<&str> = filters::Standard::LISTED
        .iter()
        .map(|s| s.caption())
        .collect();
    assert_eq!(listed.len(), 13);
    assert_eq!(listed[9], "Non What-If");
    assert_eq!(listed[12], "Not Started");
}
