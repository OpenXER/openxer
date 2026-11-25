//! Tests for the column registry, filters, grouping, sorting, layouts and
//! the table dialogs. Production-UI tests drive `App::update_ui` through egui
//! `RawInput` only: no native window, no system pointer.
use super::columns::{self, Kind, Value};
use super::filters::{Criterion, FilterSelection, Op, Standard, UserFilter};
use super::layout::{self, GroupLevel, SortKey};
use super::*;
use crate::records;
use std::path::PathBuf;

mod align_tests;
mod edit_tests;
mod quotes_tests;

fn scheduled_starter() -> ProjectData {
    let mut p = openxer_xer::starter();
    schedule_model(&mut p.model, &p.resources).unwrap();
    p
}

/// The starter project plus one code type, a UDF, a notebook topic and an
/// assignment, so every kind of column has data.
fn rich_starter() -> ProjectData {
    let mut p = scheduled_starter();
    let doc = &mut p.document;
    doc.table_mut("ACTVTYPE").add(&[
        ("actv_code_type_id", "7".into()),
        ("actv_code_type", "Area".into()),
        ("actv_code_type_scope", "AS_Project".into()),
        ("proj_id", "1".into()),
        ("seq_num", "1".into()),
    ]);
    for (id, short, name, seq) in [(71, "N", "North", 1), (72, "S", "South", 2)] {
        doc.table_mut("ACTVCODE").add(&[
            ("actv_code_id", id.to_string()),
            ("actv_code_type_id", "7".into()),
            ("short_name", short.into()),
            ("actv_code_name", name.into()),
            ("seq_num", seq.to_string()),
        ]);
    }
    for (i, a) in p.model.activities.iter().enumerate() {
        records::add_row(
            doc,
            "TASKACTV",
            &[
                ("task_id", a.task_id.to_string()),
                ("actv_code_type_id", "7".into()),
                ("actv_code_id", if i % 2 == 0 { "71" } else { "72" }.into()),
                ("proj_id", "1".into()),
            ],
        );
    }
    doc.table_mut("UDFTYPE").add(&[
        ("udf_type_id", "9".into()),
        ("table_name", "TASK".into()),
        ("udf_type_name", "user_text1".into()),
        ("udf_type_label", "Crew".into()),
        ("logical_data_type", "FT_TEXT".into()),
    ]);
    let first = p.model.activities[1].task_id;
    records::add_row(
        doc,
        "UDFVALUE",
        &[
            ("udf_type_id", "9".into()),
            ("fk_id", first.to_string()),
            ("proj_id", "1".into()),
            ("udf_text", "Civil crew A".into()),
        ],
    );
    doc.table_mut("MEMOTYPE").add(&[
        ("memo_type_id", "3".into()),
        ("memo_type", "Scope".into()),
        ("seq_num", "1".into()),
    ]);
    records::add_row(
        doc,
        "TASKMEMO",
        &[
            ("memo_id", "1".into()),
            ("task_id", first.to_string()),
            ("memo_type_id", "3".into()),
            ("proj_id", "1".into()),
            ("task_memo", "<p>Survey the north pad</p>".into()),
        ],
    );
    doc.table_mut("RSRC").add(&[
        ("rsrc_id", "5".into()),
        ("rsrc_name", "Pipefitter".into()),
        ("rsrc_type", "RT_Labor".into()),
    ]);
    let t = doc.table_mut("TASKRSRC");
    if t.fields.is_empty() {
        t.fields = [
            "taskrsrc_id",
            "task_id",
            "proj_id",
            "rsrc_id",
            "target_qty",
            "act_reg_qty",
            "remain_qty",
            "target_cost",
            "act_reg_cost",
            "remain_cost",
        ]
        .map(String::from)
        .to_vec();
    }
    t.rows.clear();
    t.add(&[
        ("taskrsrc_id", "1".into()),
        ("task_id", first.to_string()),
        ("proj_id", "1".into()),
        ("rsrc_id", "5".into()),
        ("target_qty", "24".into()),
        ("act_reg_qty", "0".into()),
        ("remain_qty", "24".into()),
        ("target_cost", "1200".into()),
        ("act_reg_cost", "0".into()),
        ("remain_cost", "1200".into()),
    ]);
    records::set_task_value(doc, first, 1, "rsrc_id", "5".into());
    p.resource_names.insert(5, "Pipefitter".into());
    p
}

fn index_of(p: &ProjectData, task_id: i64) -> usize {
    p.model
        .activities
        .iter()
        .position(|a| a.task_id == task_id)
        .unwrap()
}

/// Every registry column yields a value for every activity, and the
/// columns that must have data do.
fn check_every_column(p: &ProjectData, expect_dynamic: bool) {
    let catalog = columns::catalog(p);
    let ix = columns::Index::new((false, 0.));
    assert!(catalog.len() >= columns::builtin().len());
    let mut ids = BTreeSet::new();
    for c in &catalog {
        assert!(ids.insert(c.id.clone()), "duplicate column id {}", c.id);
        assert!(!c.title.is_empty() && c.width > 0.);
        let values: Vec<Value> = (0..p.model.activities.len())
            .map(|i| columns::value(p, &ix, c, i))
            .collect();
        for v in &values {
            let text = columns::display(p, None, v);
            if matches!(v, Value::Empty) {
                assert!(text.is_empty());
            }
        }
        let must_have_data = [
            "activity_id",
            "activity_name",
            "activity_type",
            "activity_status",
            "wbs",
            "wbs_name",
            "calendar",
            "start",
            "finish",
            "early_start",
            "early_finish",
            "late_start",
            "late_finish",
            "original_duration",
            "remaining_duration",
            "at_completion_duration",
            "total_float",
            "free_float",
            "critical",
            "duration_percent_complete",
            "activity_percent_complete",
            "predecessors",
            "successors",
        ];
        if must_have_data.contains(&c.id.as_str()) {
            assert!(
                values.iter().any(|v| !matches!(v, Value::Empty)),
                "column {} has no values",
                c.id
            );
        }
        // Sorting by any column is total and stable.
        let mut order: Vec<usize> = (0..values.len()).collect();
        group::sort(
            p,
            &ix,
            &catalog,
            &[SortKey {
                column: c.id.clone(),
                descending: false,
            }],
            &mut order,
        );
        for pair in order.windows(2) {
            assert_ne!(
                columns::compare(&values[pair[0]], &values[pair[1]]),
                std::cmp::Ordering::Greater,
                "{} not sorted",
                c.id
            );
        }
    }
    if expect_dynamic {
        for group in [columns::CODES, columns::USER_DEFINED] {
            assert!(
                catalog.iter().any(|c| c.group == group),
                "no {group} columns"
            );
        }
    }
}

#[test]
fn every_registry_column_has_values_for_the_starter_project() {
    let p = rich_starter();
    check_every_column(&p, true);
    let catalog = columns::catalog(&p);
    let ix = columns::Index::new((false, 0.));
    let first = 1;
    let get = |id: &str| {
        columns::display(
            &p,
            Some(first),
            &columns::value(&p, &ix, columns::find(&catalog, id).unwrap(), first),
        )
    };
    assert_eq!(get("code:Area"), "S");
    assert_eq!(get("udf:Crew"), "Civil crew A");
    assert_eq!(get("notebook:Scope"), "Survey the north pad");
    assert_eq!(get("primary_resource"), "Pipefitter");
    assert_eq!(get("budgeted_labor_units"), "24h");
    assert_eq!(get("budgeted_labor_cost"), "$1,200.00");
    assert_eq!(get("budgeted_total_cost"), "$1,200.00");
    assert_eq!(get("predecessors"), "RB-1000");
    assert_eq!(get("successors"), "RB-1020");
    assert_eq!(get("wbs"), "RIVERBEND.RB.1");
}

#[test]
fn every_registry_column_has_values_for_a_corpus_project() {
    let path = crate::test_corpus::file("5640.xer");
    let Ok(doc) = Document::read(&path) else {
        return;
    };
    let key = doc.projects()[0].0;
    let p = ProjectData::from_document(doc, key, "5640".into()).unwrap();
    check_every_column(&p, true);
    let catalog = columns::catalog(&p);
    let ix = columns::Index::new((false, 0.));
    for id in ["budgeted_labor_units", "remaining_labor_units"] {
        let c = columns::find(&catalog, id).unwrap();
        assert!(
            (0..p.model.activities.len())
                .any(|i| columns::value(&p, &ix, c, i).number().unwrap_or(0.) > 0.),
            "{id} is zero everywhere"
        );
    }
    let code = catalog.iter().find(|c| c.group == columns::CODES).unwrap();
    assert!((0..p.model.activities.len())
        .any(|i| !matches!(columns::value(&p, &ix, code, i), Value::Empty)));
}

fn build(p: &ProjectData, layout: &layout::Layout, users: &[UserFilter]) -> group::Built {
    let catalog = columns::catalog(p);
    let ix = columns::Index::new((false, 0.));
    group::build(
        p,
        &ix,
        &catalog,
        layout,
        &group::Scope {
            query: "",
            wbs_filter: None,
            collapsed: &BTreeSet::new(),
            users,
        },
    )
}

fn activities(built: &group::Built) -> Vec<usize> {
    built
        .rows
        .iter()
        .filter_map(|r| match r {
            Row::Activity(i) => Some(*i),
            Row::Group(_) => None,
        })
        .collect()
}

fn filtered(p: &ProjectData, keys: &[&str], match_all: bool, users: &[UserFilter]) -> Vec<usize> {
    let l = layout::Layout {
        filter: FilterSelection {
            all_activities: false,
            selected: keys.iter().map(|k| k.to_string()).collect(),
            match_all,
            highlight: false,
        },
        ..Default::default()
    };
    let mut out = activities(&build(p, &l, users));
    out.sort();
    out
}

#[test]
fn standard_filters_select_the_right_activities() {
    let mut p = scheduled_starter();
    let n = p.model.activities.len();
    // Progress two activities and constrain one.
    p.model.activities[0].status = StatusType::Completed;
    p.model.activities[1].status = StatusType::InProgress;
    p.model.activities[5].cstr = CstrType::StartOnAfter;
    p.model.activities[6].cstr = CstrType::FinishOnBefore;
    p.model.activities[7].task_type = TaskType::Loe;
    p.model.activities[8].total_float = Some(-480);
    let expect = |f: &dyn Fn(&Activity) -> bool| -> Vec<usize> {
        (0..n).filter(|&i| f(&p.model.activities[i])).collect()
    };
    let crit = |a: &Activity| !a.is_completed() && a.total_float.is_some_and(|f| f <= 0);
    let cases: Vec<(Standard, Vec<usize>)> = vec![
        (Standard::Completed, vec![0]),
        (Standard::InProgress, vec![1]),
        (Standard::NotStarted, (2..n).collect()),
        (Standard::InProgressNotStarted, (1..n).collect()),
        (Standard::Critical, expect(&crit)),
        (Standard::NonCritical, expect(&|a| !crit(a))),
        (Standard::HasStartConstraint, vec![5]),
        (Standard::HasFinishConstraint, vec![6]),
        (Standard::LevelOfEffort, vec![7]),
        (Standard::NegativeFloat, vec![8]),
        (
            Standard::Milestone,
            expect(&|a| matches!(a.task_type, TaskType::Mile | TaskType::FinMile)),
        ),
        (
            Standard::Normal,
            expect(&|a| matches!(a.task_type, TaskType::Task | TaskType::Resource)),
        ),
        (
            Standard::LongestPath,
            expect(&|a| !a.is_completed() && a.driving_path),
        ),
    ];
    for (filter, expected) in cases {
        assert_eq!(
            filtered(&p, &[&filter.key()], true, &[]),
            expected,
            "{}",
            filter.caption()
        );
        assert_eq!(Standard::from_key(&filter.key()), Some(filter));
    }
    assert!(!filtered(&p, &[&Standard::Critical.key()], true, &[]).is_empty());
    // All activities: the check overrides any selection.
    let mut l = layout::Layout::default();
    l.filter.selected = vec![Standard::Completed.key()];
    assert_eq!(activities(&build(&p, &l, &[])).len(), n);
    // All / Any of two standard filters.
    let both = filtered(
        &p,
        &[&Standard::Completed.key(), &Standard::InProgress.key()],
        true,
        &[],
    );
    assert!(both.is_empty());
    let either = filtered(
        &p,
        &[&Standard::Completed.key(), &Standard::InProgress.key()],
        false,
        &[],
    );
    assert_eq!(either, vec![0, 1]);
    // Highlight keeps every row and marks the matches.
    l.filter = FilterSelection {
        all_activities: false,
        selected: vec![Standard::Completed.key()],
        match_all: true,
        highlight: true,
    };
    let built = build(&p, &l, &[]);
    assert_eq!(activities(&built).len(), n);
    assert_eq!(built.highlighted, BTreeSet::from([0]));
}

#[test]
fn user_filters_match_all_or_any_of_their_rows() {
    let p = scheduled_starter();
    let row = |column: &str, op, value: &str, high: &str| Criterion {
        column: column.into(),
        op,
        value: value.into(),
        high: high.into(),
        level: 0,
    };
    let users = vec![
        UserFilter {
            name: "Long construction".into(),
            match_all: true,
            rows: vec![
                row("wbs_name", Op::Equals, "Construction", ""),
                row("original_duration", Op::GreaterOrEqual, "40h", ""),
            ],
        },
        UserFilter {
            name: "Install or check".into(),
            match_all: false,
            rows: vec![
                row("activity_name", Op::StartsWith, "install", ""),
                row("activity_name", Op::Contains, "checks", ""),
            ],
        },
        UserFilter {
            name: "Mid range".into(),
            match_all: true,
            rows: vec![row("remaining_duration", Op::Within, "24h", "40h")],
        },
    ];
    let names =
        |v: Vec<usize>| -> Vec<String> { v.into_iter().map(|i| p.name(i).to_string()).collect() };
    assert_eq!(
        names(filtered(&p, &["user:Long construction"], true, &users)),
        [
            "Prepare foundations",
            "Mechanical installation",
            "Electrical & instrumentation"
        ]
    );
    assert_eq!(
        names(filtered(&p, &["user:Install or check"], true, &users)),
        ["Install structural assemblies", "Pre-commissioning checks"]
    );
    let mid = filtered(&p, &["user:Mid range"], true, &users);
    assert!(mid
        .iter()
        .all(|&i| (24 * 60..=40 * 60).contains(&p.model.activities[i].rem)));
    assert!(!mid.is_empty());
    // Two user filters combined with Any, then All.
    let any = filtered(
        &p,
        &["user:Long construction", "user:Install or check"],
        false,
        &users,
    );
    assert_eq!(any.len(), 5);
    let all = filtered(
        &p,
        &["user:Long construction", "user:Install or check"],
        true,
        &users,
    );
    assert!(all.is_empty());
    // Dates compare by day; not-equal is the complement of equals.
    let start = columns::display(
        &p,
        Some(3),
        &Value::Date {
            minute: p.model.activities[3].early_start.unwrap(),
            mark: crate::format::DateMark::Plain,
        },
    );
    let on_day = vec![UserFilter {
        name: "Day".into(),
        match_all: true,
        rows: vec![row("early_start", Op::Equals, &start, "")],
    }];
    let hits = filtered(&p, &["user:Day"], true, &on_day);
    assert!(hits.contains(&3));
    let off_day = vec![UserFilter {
        rows: vec![row("early_start", Op::NotEqual, &start, "")],
        ..on_day[0].clone()
    }];
    let rest = filtered(&p, &["user:Day"], true, &off_day);
    assert_eq!(hits.len() + rest.len(), p.model.activities.len());
}

#[test]
fn group_by_wbs_nests_bands_and_honors_to_level_and_collapse() {
    let p = wbs_child_project();
    let l = layout::Layout::default();
    let built = build(&p, &l, &[]);
    let labels: Vec<(usize, String)> = built
        .bands
        .iter()
        .map(|b| (b.level, b.label.clone()))
        .collect();
    assert_eq!(
        labels[0],
        (0, "RIVERBEND.RB.1  Engineering & approvals".into())
    );
    assert!(labels.contains(&(1, "RIVERBEND.RB.1.D  Design".into())));
    // Every activity appears exactly once, under its own band.
    let mut seen = activities(&built);
    seen.sort();
    assert_eq!(seen, (0..p.model.activities.len()).collect::<Vec<_>>());
    let design = built
        .bands
        .iter()
        .position(|b| b.label.ends_with("Design"))
        .unwrap();
    assert_eq!(built.bands[design].members.len(), 2);
    // To Level 1 flattens children into the top band.
    let mut flat = l.clone();
    flat.group_sort.levels[0].to_level = Some(1);
    let built = build(&p, &flat, &[]);
    assert!(built.bands.iter().all(|b| b.level == 0));
    assert_eq!(built.bands.len(), 4);
    // Collapsing a band hides its rows but keeps the band.
    let catalog = columns::catalog(&p);
    let ix = columns::Index::new((false, 0.));
    let key = build(&p, &l, &[]).bands[0].key.clone();
    let collapsed = BTreeSet::from([key]);
    let built = group::build(
        &p,
        &ix,
        &catalog,
        &l,
        &group::Scope {
            query: "",
            wbs_filter: None,
            collapsed: &collapsed,
            users: &[],
        },
    );
    assert!(matches!(built.rows[0], Row::Group(0)));
    assert!(
        matches!(built.rows[1], Row::Group(_)),
        "first band collapsed"
    );
    assert_eq!(activities(&built).len(), p.model.activities.len() - 4);
    // Summaries only: bands without activity rows.
    let mut summaries = l.clone();
    summaries.group_sort.show_summaries_only = true;
    assert!(activities(&build(&p, &summaries, &[])).is_empty());
}

/// The starter with a child WBS element under RB.1 holding two activities.
fn wbs_child_project() -> ProjectData {
    let mut p = scheduled_starter();
    p.wbs.push(openxer_xer::Wbs {
        id: 11,
        parent: Some(10),
        code: "D".into(),
        name: "Design".into(),
        sequence: 1,
    });
    for task in [1010, 1020] {
        p.details.get_mut(&task).unwrap().wbs = 11;
    }
    p
}

#[test]
fn group_by_code_then_status_and_sort_by_several_columns() {
    let p = rich_starter();
    let mut l = layout::Layout::default();
    l.group_sort.levels = vec![
        GroupLevel {
            column: "code:Area".into(),
            to_level: None,
            indent: true,
            interval: String::new(),
        },
        GroupLevel {
            column: "activity_type".into(),
            to_level: None,
            indent: true,
            interval: String::new(),
        },
    ];
    l.group_sort.sort = vec![
        SortKey {
            column: "original_duration".into(),
            descending: true,
        },
        SortKey {
            column: "activity_id".into(),
            descending: false,
        },
    ];
    let built = build(&p, &l, &[]);
    let top: Vec<&str> = built
        .bands
        .iter()
        .filter(|b| b.level == 0)
        .map(|b| b.label.as_str())
        .collect();
    assert_eq!(top, ["N  North", "S  South"]);
    assert!(built
        .bands
        .iter()
        .any(|b| b.level == 1 && b.label == "Task Dependent"));
    // Within each innermost band: longest original duration first, then ID.
    let catalog = columns::catalog(&p);
    let ix = columns::Index::new((false, 0.));
    let od = columns::find(&catalog, "original_duration").unwrap();
    let mut run: Vec<usize> = vec![];
    let check = |run: &Vec<usize>| {
        for pair in run.windows(2) {
            let (a, b) = (
                columns::value(&p, &ix, od, pair[0]),
                columns::value(&p, &ix, od, pair[1]),
            );
            let o = columns::compare(&a, &b);
            assert_ne!(o, std::cmp::Ordering::Less, "descending duration");
            if o == std::cmp::Ordering::Equal {
                assert!(
                    p.model.activities[pair[0]].task_code < p.model.activities[pair[1]].task_code
                );
            }
        }
    };
    for row in &built.rows {
        match row {
            Row::Activity(i) => run.push(*i),
            Row::Group(_) => {
                check(&run);
                run.clear();
            }
        }
    }
    check(&run);
    // Every activity of a North band has code N.
    let north = &built.bands[0];
    assert!(north.members.iter().all(|i| i % 2 == 0));
    // Alphabetical band order and an empty code band labelled "No Area".
    let mut q = p.clone();
    let t = q.document.table_mut("TASKACTV");
    t.rows
        .retain(|r| t_value(r, &t.fields, "task_id") != "1000");
    l.group_sort.levels.truncate(1);
    l.group_sort.sort_bands_alphabetically = true;
    let built = build(&q, &l, &[]);
    let labels: Vec<&str> = built.bands.iter().map(|b| b.label.as_str()).collect();
    assert_eq!(labels, ["N  North", "S  South", "No Area"]);
}

fn t_value<'a>(row: &'a [String], fields: &[String], name: &str) -> &'a str {
    fields
        .iter()
        .position(|f| f == name)
        .map_or("", |i| row[i].as_str())
}

#[test]
fn band_totals_roll_up_dates_floats_and_costs() {
    let p = rich_starter();
    let catalog = columns::catalog(&p);
    let ix = columns::Index::new((false, 0.));
    let all: Vec<usize> = (0..p.model.activities.len()).collect();
    let total = |id| group::band_total(&p, &ix, columns::find(&catalog, id).unwrap(), &all);
    let start = p
        .model
        .activities
        .iter()
        .filter_map(|a| a.early_start)
        .min()
        .unwrap();
    assert_eq!(
        total("start"),
        Some(Value::Date {
            minute: start,
            mark: crate::format::DateMark::Plain
        })
    );
    assert_eq!(total("budgeted_total_cost"), Some(Value::Cost(1200.)));
    assert!(matches!(total("total_float"), Some(Value::Duration(_))));
    assert_eq!(total("activity_name"), None);
}

#[test]
fn layouts_round_trip_and_reject_invalid_data() {
    let mut l = layout::Layout {
        name: "Look ahead".into(),
        ..Default::default()
    };
    l.columns[1].title = Some("Description".into());
    l.columns[2].align = Some(columns::Align::Center);
    l.filter.selected = vec![Standard::Critical.key()];
    l.filter.all_activities = false;
    l.group_sort.bands[2].background = [0, 0, 255];
    l.group_sort.bands[2].font_size = 12.;
    l.gantt = Some(serde_json::json!({"row_height": 18.0}));
    l.bottom = Some(layout::Bottom {
        show_details: false,
        tab: "status".into(),
        view: "trace_logic".into(),
    });
    l.panes = Some(serde_json::json!({"top": "network"}));
    let back = layout::Layout::from_json(&l.to_json()).unwrap();
    assert_eq!(back, l);
    let text = layout::library_to_json(&[l.clone(), layout::Layout::default()]);
    assert_eq!(layout::library_from_json(&text).unwrap().len(), 2);
    let bad = |edit: &dyn Fn(&mut serde_json::Value)| {
        let mut v = l.to_json();
        edit(&mut v);
        layout::Layout::from_json(&v)
    };
    for (what, result) in [
        ("empty name", bad(&|v| v["name"] = "  ".into())),
        ("no columns", bad(&|v| v["columns"] = serde_json::json!([]))),
        ("width", bad(&|v| v["columns"][0]["width"] = (-5.).into())),
        (
            "nan width",
            bad(&|v| v["columns"][0]["width"] = "wide".into()),
        ),
        (
            "duplicate column",
            bad(&|v| v["columns"][1]["id"] = "activity_id".into()),
        ),
        (
            "align",
            bad(&|v| v["columns"][0]["align"] = "justify".into()),
        ),
        ("filter", bad(&|v| v["filter"] = 3.into())),
        (
            "color",
            bad(&|v| v["group_sort"]["bands"][0]["background"] = "blue".into()),
        ),
        (
            "font",
            bad(&|v| v["group_sort"]["bands"][0]["font_size"] = 99.into()),
        ),
        (
            "to_level",
            bad(&|v| v["group_sort"]["levels"][0]["to_level"] = 0.into()),
        ),
        ("gantt", bad(&|v| v["gantt"] = "x".into())),
        (
            "not an object",
            layout::Layout::from_json(&serde_json::json!([1])),
        ),
    ] {
        assert!(result.is_err(), "{what} must be rejected");
    }
    assert!(layout::library_from_json("[1,2]").is_err());
    assert!(layout::library_from_json(&layout::library_to_json(&[l.clone(), l.clone()])).is_err());
    assert!(layout::library_from_json("not json").is_err());
    let f = UserFilter {
        name: "Mine".into(),
        match_all: false,
        rows: vec![Criterion {
            column: "start".into(),
            op: Op::Within,
            value: "2026-11-01".into(),
            high: "2026-12-01".into(),
            level: 0,
        }],
    };
    let text = layout::filters_to_json(std::slice::from_ref(&f));
    assert_eq!(layout::filters_from_json(&text).unwrap(), vec![f]);
    assert!(layout::filters_from_json(
        r#"[{"name":"x","rows":[{"column":"a","op":"approx","value":"","high":""}]}]"#
    )
    .is_err());
}

#[test]
fn parse_input_reads_back_display_text() {
    let p = openxer_xer::starter();
    for (kind, v) in [
        (Kind::Duration, Value::Duration(1440)),
        (Kind::Percent, Value::Percent(37.5)),
        (Kind::Units, Value::Units(12.)),
        (Kind::Cost, Value::Cost(1234.5)),
        (Kind::Number, Value::Number(7.)),
        (Kind::Flag, Value::Flag(true)),
        (Kind::Text, Value::Text("A-1".into())),
    ] {
        let text = columns::display(&p, None, &v);
        assert_eq!(
            columns::parse_input(&p, None, kind, &text).unwrap(),
            v,
            "{text}"
        );
    }
    let d = columns::parse_input(&p, None, Kind::Date, "2026-11-04 08:00").unwrap();
    let text = columns::display(&p, None, &d);
    let back = columns::parse_input(&p, None, Kind::Date, &text).unwrap();
    assert_eq!(
        back.number().unwrap().div_euclid(DAY as f64),
        d.number().unwrap().div_euclid(DAY as f64)
    );
    assert!(columns::parse_input(&p, None, Kind::Date, "soon").is_err());
    assert!(columns::parse_input(&p, None, Kind::Duration, "abc").is_err());
}

// ---------------------------------------------------------------------------
// Production UI

fn frame(app: &mut App, ctx: &egui::Context, events: Vec<egui::Event>, time: &mut f64) {
    frame_mod(app, ctx, events, time, Default::default());
}

thread_local! {
    /// Width of the headless window; the Status tab's Units section sits
    /// beyond 1500 px, so tests that type into it widen the window.
    static SCREEN_WIDTH: std::cell::Cell<f32> = const { std::cell::Cell::new(1500.) };
}

fn frame_mod(
    app: &mut App,
    ctx: &egui::Context,
    events: Vec<egui::Event>,
    time: &mut f64,
    modifiers: egui::Modifiers,
) {
    *time += 0.1;
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                Pos2::ZERO,
                Vec2::new(SCREEN_WIDTH.with(|w| w.get()), 900.),
            )),
            time: Some(*time),
            events,
            modifiers,
            focused: true,
            ..Default::default()
        },
        |ctx| app.update_ui(ctx),
    );
}

fn press_at(
    app: &mut App,
    ctx: &egui::Context,
    pos: Pos2,
    button: egui::PointerButton,
    modifiers: egui::Modifiers,
    time: &mut f64,
) {
    // A dialog opened last frame is laid out invisibly first; let it settle.
    frame(app, ctx, vec![], time);
    for pressed in [true, false] {
        frame_mod(
            app,
            ctx,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button,
                    pressed,
                    modifiers,
                },
            ],
            time,
            modifiers,
        );
    }
    frame(app, ctx, vec![egui::Event::PointerGone], time);
    frame(app, ctx, vec![], time);
}

fn double_click(app: &mut App, ctx: &egui::Context, pos: Pos2, time: &mut f64) {
    frame(app, ctx, vec![egui::Event::PointerMoved(pos)], time);
    for pressed in [true, false, true, false] {
        frame(
            app,
            ctx,
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            }],
            time,
        );
    }
    frame(app, ctx, vec![], time);
}

fn rect(app: &App, action: &str) -> Rect {
    let [l, t, r, b] = *app
        .action_rects
        .get(action)
        .unwrap_or_else(|| panic!("No rendered control {action:?}"));
    Rect::from_min_max(Pos2::new(l, t), Pos2::new(r, b))
}

fn click(app: &mut App, ctx: &egui::Context, action: &str, time: &mut f64) {
    // A newly opened window settles its position over two frames.
    frame(app, ctx, vec![], time);
    frame(app, ctx, vec![], time);
    let pos = rect(app, action).center();
    press_at(
        app,
        ctx,
        pos,
        egui::PointerButton::Primary,
        Default::default(),
        time,
    );
}

fn shift_click(app: &mut App, ctx: &egui::Context, action: &str, time: &mut f64) {
    // A newly opened window settles its position over two frames.
    frame(app, ctx, vec![], time);
    frame(app, ctx, vec![], time);
    let pos = rect(app, action).center();
    let shift = egui::Modifiers {
        shift: true,
        ..Default::default()
    };
    press_at(app, ctx, pos, egui::PointerButton::Primary, shift, time);
}

fn right_click(app: &mut App, ctx: &egui::Context, action: &str, time: &mut f64) {
    // A newly opened window settles its position over two frames.
    frame(app, ctx, vec![], time);
    frame(app, ctx, vec![], time);
    let pos = rect(app, action).center();
    press_at(
        app,
        ctx,
        pos,
        egui::PointerButton::Secondary,
        Default::default(),
        time,
    );
}

fn key(app: &mut App, ctx: &egui::Context, key: Key, modifiers: egui::Modifiers, time: &mut f64) {
    for pressed in [true, false] {
        frame_mod(
            app,
            ctx,
            vec![egui::Event::Key {
                key,
                physical_key: Some(key),
                pressed,
                repeat: false,
                modifiers,
            }],
            time,
            modifiers,
        );
    }
}

fn type_text(app: &mut App, ctx: &egui::Context, text: &str, time: &mut f64) {
    frame(app, ctx, vec![egui::Event::Text(text.into())], time);
}

fn select_all(app: &mut App, ctx: &egui::Context, time: &mut f64) {
    let ctrl = egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    };
    key(app, ctx, Key::A, ctrl, time);
}

fn setup(name: &str, project: ProjectData) -> (PathBuf, egui::Context, App, f64) {
    setup_with(name, project, true)
}

fn setup_with(name: &str, project: ProjectData, warm: bool) -> (PathBuf, egui::Context, App, f64) {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "openxer-grid-{name}-{}-{unique}.db",
        std::process::id()
    ));
    let store = Store::open(path.clone()).unwrap();
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.);
    let mut app = App::new(&ctx, store, project);
    let mut time = 0.;
    if warm {
        for _ in 0..3 {
            frame(&mut app, &ctx, vec![], &mut time);
        }
    }
    (path, ctx, app, time)
}

fn visible_ids(app: &App) -> Vec<String> {
    app.rows()
        .into_iter()
        .filter_map(|r| match r {
            Row::Activity(i) => Some(app.project.model.activities[i].task_code.clone()),
            Row::Group(_) => None,
        })
        .collect()
}

/// The center of activity `i`'s cell in column `id`, from the rendered header
/// and the activity's row.
fn cell(app: &App, i: usize, id: &str) -> Pos2 {
    let task_id = app.project.model.activities[i].task_id;
    let row = app
        .bar_geometry
        .iter()
        .find(|b| b.task_id == task_id)
        .unwrap()
        .row;
    Pos2::new(
        rect(app, &format!("grid_header_{id}")).center().x,
        row.center().y,
    )
}

#[test]
fn header_click_shift_click_hide_and_context_menu_use_the_rendered_headers() {
    let (path, ctx, mut app, mut time) = setup("header", scheduled_starter());
    click(&mut app, &ctx, "grid_header_remaining_duration", &mut time);
    assert_eq!(
        app.grid.layout.group_sort.sort,
        vec![SortKey {
            column: "remaining_duration".into(),
            descending: false
        }]
    );
    click(&mut app, &ctx, "grid_header_remaining_duration", &mut time);
    assert!(app.grid.layout.group_sort.sort[0].descending);
    shift_click(&mut app, &ctx, "grid_header_activity_id", &mut time);
    assert_eq!(app.grid.layout.group_sort.sort.len(), 2);
    assert_eq!(app.grid.layout.group_sort.sort[1].column, "activity_id");
    // Rows follow the two keys inside each WBS band.
    let rows = app.rows();
    let mut last: Option<(i64, String)> = None;
    for r in rows {
        match r {
            Row::Group(_) => last = None,
            Row::Activity(i) => {
                let a = &app.project.model.activities[i];
                let key = (-a.rem, a.task_code.clone());
                if let Some(prev) = &last {
                    assert!(prev <= &key);
                }
                last = Some(key);
            }
        }
    }
    // Right-click menu (the table's context menu): Columns... opens the dialog.
    right_click(&mut app, &ctx, "grid_header_finish", &mut time);
    click(&mut app, &ctx, "header_menu_columns", &mut time);
    assert!(app.grid.dialogs.columns.is_some());
    key(&mut app, &ctx, Key::Escape, Default::default(), &mut time);
    assert!(app.grid.dialogs.columns.is_none());
    right_click(&mut app, &ctx, "grid_header_finish", &mut time);
    click(&mut app, &ctx, "header_menu_group_sort", &mut time);
    assert!(app.grid.dialogs.group_sort.is_some());
    click(&mut app, &ctx, "group_cancel", &mut time);
    right_click(&mut app, &ctx, "grid_header_finish", &mut time);
    click(&mut app, &ctx, "header_menu_filters", &mut time);
    assert!(app.grid.dialogs.filters.is_some());
    click(&mut app, &ctx, "filters_cancel", &mut time);
    // Double-clicking a header edge fits the column to its contents.
    let edge = rect(&app, "grid_resize_activity_name").center();
    double_click(&mut app, &ctx, edge, &mut time);
    let name = app
        .grid
        .layout
        .columns
        .iter()
        .find(|c| c.id == "activity_name")
        .unwrap();
    assert!(
        name.width < 250.,
        "fit narrows the 250 px default: {}",
        name.width
    );
    assert!(app.error.is_none(), "{:?}", app.error);
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn columns_dialog_adds_removes_orders_edits_and_defaults() {
    let (path, ctx, mut app, mut time) = setup("columns", rich_starter());
    click(&mut app, &ctx, "layout_button", &mut time);
    click(&mut app, &ctx, "menu_columns", &mut time);
    assert!(app.grid.dialogs.columns.is_some());
    // Open the Durations group, pick Original Duration, add it.
    click(&mut app, &ctx, "columns_group_Durations", &mut time);
    click(
        &mut app,
        &ctx,
        "columns_available_original_duration",
        &mut time,
    );
    click(&mut app, &ctx, "columns_selected_1", &mut time);
    click(&mut app, &ctx, "columns_add", &mut time);
    let draft = |app: &App| -> Vec<String> {
        app.grid
            .dialogs
            .columns
            .as_ref()
            .unwrap()
            .columns
            .iter()
            .map(|c| c.id.clone())
            .collect()
    };
    assert_eq!(draft(&app)[2], "original_duration");
    // Add a code column too.
    click(&mut app, &ctx, "columns_group_Activity Codes", &mut time);
    click(&mut app, &ctx, "columns_available_code:Area", &mut time);
    click(&mut app, &ctx, "columns_add", &mut time);
    assert!(draft(&app).contains(&"code:Area".to_string()));
    // Move the new column up, then remove Free Float.
    let at = draft(&app).iter().position(|c| c == "code:Area").unwrap();
    click(&mut app, &ctx, &format!("columns_selected_{at}"), &mut time);
    click(&mut app, &ctx, "columns_up", &mut time);
    assert_eq!(draft(&app)[at - 1], "code:Area");
    let ff = draft(&app).iter().position(|c| c == "free_float").unwrap();
    click(&mut app, &ctx, &format!("columns_selected_{ff}"), &mut time);
    click(&mut app, &ctx, "columns_remove", &mut time);
    assert!(!draft(&app).contains(&"free_float".to_string()));
    // Nothing applies before OK/Apply.
    assert!(!app.grid.layout.columns.iter().any(|c| c.id == "code:Area"));
    // Edit Column: new title, width, alignment.
    click(&mut app, &ctx, "columns_selected_0", &mut time);
    click(&mut app, &ctx, "columns_edit", &mut time);
    assert!(app.grid.dialogs.edit_column.is_some());
    click(&mut app, &ctx, "edit_column_title", &mut time);
    select_all(&mut app, &ctx, &mut time);
    type_text(&mut app, &ctx, "ID", &mut time);
    click(&mut app, &ctx, "edit_column_align_center", &mut time);
    click(&mut app, &ctx, "edit_column_ok", &mut time);
    let first = app.grid.dialogs.columns.as_ref().unwrap().columns[0].clone();
    assert_eq!(first.title.as_deref(), Some("ID"));
    assert_eq!(first.align, Some(columns::Align::Center));
    click(&mut app, &ctx, "columns_apply", &mut time);
    assert!(app.grid.layout.columns.iter().any(|c| c.id == "code:Area"));
    assert!(
        app.grid.dialogs.columns.is_some(),
        "Apply keeps the dialog open"
    );
    click(&mut app, &ctx, "columns_ok", &mut time);
    assert!(app.grid.dialogs.columns.is_none());
    frame(&mut app, &ctx, vec![], &mut time);
    assert!(app.action_rects.contains_key("grid_header_code:Area"));
    assert!(app
        .action_rects
        .contains_key("grid_header_original_duration"));
    let snapshot = app.current_layout_table();
    assert_eq!(snapshot.columns[0].title, "ID");
    // Cancel discards; Default restores the standard set.
    app.open_columns_dialog();
    frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "columns_remove_all", &mut time);
    click(&mut app, &ctx, "columns_cancel", &mut time);
    assert!(app.grid.layout.columns.len() > 3);
    app.open_columns_dialog();
    frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "columns_default", &mut time);
    click(&mut app, &ctx, "columns_ok", &mut time);
    assert_eq!(app.grid.layout.columns, layout::default_columns());
    // OK with no columns is refused.
    app.open_columns_dialog();
    frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "columns_remove_all", &mut time);
    click(&mut app, &ctx, "columns_ok", &mut time);
    assert!(app.error.is_some());
    assert_eq!(app.grid.layout.columns, layout::default_columns());
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn filters_dialog_applies_standard_and_new_user_filters() {
    let (path, ctx, mut app, mut time) = setup("filters", scheduled_starter());
    let n = app.project.model.activities.len();
    click(&mut app, &ctx, "filter_button", &mut time);
    assert!(app.grid.dialogs.filters.is_some());
    click(
        &mut app,
        &ctx,
        &format!("filter_select_{}", Standard::Milestone.key()),
        &mut time,
    );
    let draft = &app.grid.dialogs.filters.as_ref().unwrap().selection;
    assert!(
        !draft.all_activities,
        "selecting a filter clears All Activities"
    );
    click(&mut app, &ctx, "filters_apply", &mut time);
    assert_eq!(visible_ids(&app).len(), 3, "three milestones");
    // New... user filter: Activity Name contains "install".
    click(&mut app, &ctx, "filters_new", &mut time);
    assert!(app.grid.dialogs.filter_editor.is_some());
    click(&mut app, &ctx, "filter_name", &mut time);
    select_all(&mut app, &ctx, &mut time);
    type_text(&mut app, &ctx, "Installs", &mut time);
    {
        let e = app.grid.dialogs.filter_editor.as_mut().unwrap();
        e.filter.rows[0].column = "activity_name".into();
        e.filter.rows[0].op = Op::Contains;
    }
    click(&mut app, &ctx, "filter_value_0", &mut time);
    type_text(&mut app, &ctx, "install", &mut time);
    click(&mut app, &ctx, "filter_ok", &mut time);
    assert!(app.grid.dialogs.filter_editor.is_none(), "{:?}", app.error);
    let key = "user:Installs";
    click(&mut app, &ctx, &format!("filter_select_{key}"), &mut time);
    // Any selected filter: milestones or installs.
    click(&mut app, &ctx, "filters_match_any", &mut time);
    click(&mut app, &ctx, "filters_ok", &mut time);
    assert!(app.grid.dialogs.filters.is_none());
    let ids = visible_ids(&app);
    assert_eq!(ids.len(), 5, "{ids:?}");
    assert!(rect(&app, "filter_button").width() > 50.);
    // All selected filters: nothing is both.
    app.open_filters_dialog();
    frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "filters_match_all", &mut time);
    click(&mut app, &ctx, "filters_ok", &mut time);
    assert!(visible_ids(&app).is_empty());
    // A bad value is refused in the editor.
    app.open_filters_dialog();
    frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, &format!("filter_row_{key}"), &mut time);
    click(&mut app, &ctx, "filters_modify", &mut time);
    {
        let e = app.grid.dialogs.filter_editor.as_mut().unwrap();
        e.filter.rows[0].column = "start".into();
        e.filter.rows[0].op = Op::Greater;
        e.filter.rows[0].value = "next tuesday".into();
    }
    click(&mut app, &ctx, "filter_ok", &mut time);
    assert!(app.grid.dialogs.filter_editor.is_some());
    assert!(app.error.as_deref().unwrap_or("").contains("Start"));
    click(&mut app, &ctx, "filter_cancel", &mut time);
    // All Activities shows everything; the user filter persists.
    click(&mut app, &ctx, "filters_all_activities", &mut time);
    click(&mut app, &ctx, "filters_ok", &mut time);
    assert_eq!(visible_ids(&app).len(), n);
    let saved = app.store.preference(FILTERS_KEY).unwrap().unwrap();
    assert!(saved.contains("Installs"));
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn group_and_sort_dialog_groups_by_code_and_sorts_by_several_columns() {
    let (path, ctx, mut app, mut time) = setup("group", rich_starter());
    click(&mut app, &ctx, "layout_button", &mut time);
    click(&mut app, &ctx, "menu_group_sort", &mut time);
    assert!(app.grid.dialogs.group_sort.is_some());
    // Replace WBS with the Area code (combo choices are set on the draft).
    app.grid.dialogs.group_sort.as_mut().unwrap().gs.levels[0].column = "code:Area".into();
    click(&mut app, &ctx, "group_show_title", &mut time);
    click(&mut app, &ctx, "group_sort", &mut time);
    assert!(app.grid.dialogs.sort.is_some());
    click(&mut app, &ctx, "sort_add", &mut time);
    {
        let s = app.grid.dialogs.sort.as_mut().unwrap();
        s.keys[0] = SortKey {
            column: "total_float".into(),
            descending: true,
        };
        s.keys[1] = SortKey {
            column: "activity_name".into(),
            descending: false,
        };
    }
    click(&mut app, &ctx, "sort_row_1", &mut time);
    click(&mut app, &ctx, "sort_up", &mut time);
    assert_eq!(
        app.grid.dialogs.sort.as_ref().unwrap().keys[0].column,
        "activity_name"
    );
    click(&mut app, &ctx, "sort_down", &mut time);
    click(&mut app, &ctx, "sort_ok", &mut time);
    assert!(app.grid.dialogs.sort.is_none());
    click(&mut app, &ctx, "group_ok", &mut time);
    assert!(app.grid.dialogs.group_sort.is_none());
    let gs = &app.grid.layout.group_sort;
    assert_eq!(gs.levels[0].column, "code:Area");
    assert_eq!(gs.sort[0].column, "total_float");
    assert!(gs.sort[0].descending);
    let _ = app.rows();
    let labels: Vec<String> = app
        .grid
        .bands
        .borrow()
        .iter()
        .map(|b| b.label.clone())
        .collect();
    assert_eq!(labels, ["Area:  N  North", "Area:  S  South"]);
    frame(&mut app, &ctx, vec![], &mut time);
    // Clicking a band's expand box collapses it in the real table.
    let table = app.current_layout_table();
    let before = table.rows.len();
    let first_box = rect(&app, "grid_band_box_0").center();
    press_at(
        &mut app,
        &ctx,
        first_box,
        egui::PointerButton::Primary,
        Default::default(),
        &mut time,
    );
    assert!(app.current_layout_table().rows.len() < before);
    // Default restores WBS grouping.
    app.open_group_sort_dialog();
    frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "group_default", &mut time);
    click(&mut app, &ctx, "group_ok", &mut time);
    assert_eq!(app.grid.layout.group_sort.levels[0].column, "wbs");
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn layouts_save_open_delete_and_prompt_through_the_dialogs() {
    let (path, ctx, mut app, mut time) = setup("layouts", scheduled_starter());
    assert_eq!(app.grid.layout.name, layout::STANDARD_NAME);
    // Change the layout and Save As "Look ahead".
    app.hide_column("free_float");
    app.grid.layout.group_sort.sort = vec![SortKey {
        column: "start".into(),
        descending: false,
    }];
    app.show_details = false;
    app.gantt.options.show_relationships = false;
    // Layout > Save As... from the layout bar menu.
    click(&mut app, &ctx, "layout_button", &mut time);
    key(&mut app, &ctx, Key::Escape, Default::default(), &mut time);
    app.open_save_as_dialog();
    click(&mut app, &ctx, "save_as_name", &mut time);
    select_all(&mut app, &ctx, &mut time);
    type_text(&mut app, &ctx, "Look ahead", &mut time);
    click(&mut app, &ctx, "save_as_ok", &mut time);
    assert!(app.grid.dialogs.save_as.is_none(), "{:?}", app.error);
    assert_eq!(app.grid.layout.name, "Look ahead");
    let saved = app
        .grid
        .library
        .iter()
        .find(|l| l.name == "Look ahead")
        .unwrap();
    assert!(saved.gantt.is_some());
    assert!(!saved.bottom.as_ref().unwrap().show_details);
    // An empty name is refused.
    app.open_save_as_dialog();
    frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "save_as_name", &mut time);
    select_all(&mut app, &ctx, &mut time);
    type_text(&mut app, &ctx, " ", &mut time);
    click(&mut app, &ctx, "save_as_ok", &mut time);
    assert!(app.grid.dialogs.save_as.is_some());
    click(&mut app, &ctx, "save_as_cancel", &mut time);
    app.error = None;
    // Open the standard layout: unchanged, so no prompt.
    app.open_layout_dialog();
    frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "open_layout_item_0", &mut time);
    click(&mut app, &ctx, "open_layout_open", &mut time);
    assert!(app.grid.dialogs.save_prompt.is_none());
    assert_eq!(app.grid.layout.name, layout::STANDARD_NAME);
    assert_eq!(app.grid.layout.columns, layout::default_columns());
    app.gantt.options.show_relationships = true;
    // Change it, open "Look ahead": the prompt appears; No discards.
    app.hide_column("finish");
    app.open_layout_dialog();
    frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "open_layout_item_1", &mut time);
    click(&mut app, &ctx, "open_layout_open", &mut time);
    assert!(app.grid.dialogs.save_prompt.is_some());
    click(&mut app, &ctx, "prompt_no", &mut time);
    assert_eq!(app.grid.layout.name, "Look ahead");
    assert!(!app.grid.layout.columns.iter().any(|c| c.id == "free_float"));
    assert!(!app.show_details, "the bottom pane comes from the layout");
    assert!(
        !app.gantt.options.show_relationships,
        "Gantt settings come from the layout"
    );
    // Delete it from the Open Layout list.
    app.open_layout_dialog();
    frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "open_layout_item_1", &mut time);
    click(&mut app, &ctx, "open_layout_delete", &mut time);
    // It asks first; Yes deletes.
    assert!(app.grid.dialogs.remove_prompt.is_some(), "asks first");
    click(&mut app, &ctx, "remove_layout_yes", &mut time);
    // Only the starter samples are left.
    assert!(app
        .grid
        .library
        .iter()
        .all(|l| l.name.ends_with("(OpenXER sample)")));
    click(&mut app, &ctx, "open_layout_cancel", &mut time);
    assert!(app.error.is_none(), "{:?}", app.error);
    // The working layout and library survive a restart.
    app.save_layout_as("Second").unwrap();
    for _ in 0..8 {
        frame(&mut app, &ctx, vec![], &mut time);
    }
    let expected = app.grid.layout.clone();
    drop(app);
    let ctx2 = egui::Context::default();
    let reopened = App::new(
        &ctx2,
        Store::open(path.clone()).unwrap(),
        scheduled_starter(),
    );
    assert_eq!(reopened.grid.layout.columns, expected.columns);
    assert_eq!(reopened.grid.layout.name, "Second");
    assert_eq!(
        reopened.layout_names(),
        [
            layout::STANDARD_NAME,
            "Lookahead (OpenXER sample)",
            "Second",
            "WBS Summary (OpenXER sample)"
        ]
    );
    drop(reopened);
    // Invalid stored data is rejected and the defaults load.
    let store = Store::open(path.clone()).unwrap();
    store
        .set_preference(LIBRARY_KEY, r#"[{"name":"","columns":[]}]"#)
        .unwrap();
    store
        .set_preference(
            CURRENT_KEY,
            r#"{"name":"Broken","columns":[{"id":"activity_id","width":-1}]}"#,
        )
        .unwrap();
    let ctx3 = egui::Context::default();
    let fresh = App::new(&ctx3, store, scheduled_starter());
    assert!(fresh.grid.library.is_empty());
    assert_eq!(fresh.grid.layout.name, layout::STANDARD_NAME);
    assert!(fresh.status.contains("ignored"));
    drop(fresh);
    let _ = std::fs::remove_file(path);
}

#[test]
fn old_column_order_and_widths_migrate_once() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("openxer-grid-migrate-{unique}.db"));
    let store = Store::open(path.clone()).unwrap();
    store
        .set_preference(
            "activities_view_v1",
            r#"{"column_order":[1,0,2,3,4,5,6,7],"column_widths":[0.1,0.3,0.075,0.12,0.12,0.075,0.11,0.1],"grid_fraction":0.6}"#,
        )
        .unwrap();
    let ctx = egui::Context::default();
    let app = App::new(&ctx, store, scheduled_starter());
    let ids: Vec<&str> = app
        .grid
        .layout
        .columns
        .iter()
        .map(|c| c.id.as_str())
        .collect();
    assert_eq!(
        ids,
        [
            "activity_name",
            "activity_id",
            "remaining_duration",
            "start",
            "finish",
            "total_float",
            "activity_status",
            "free_float"
        ]
    );
    assert_eq!(app.grid.layout.columns[0].width, 270.);
    assert!(app.store.preference(CURRENT_KEY).unwrap().is_some());
    drop(app);
    let _ = std::fs::remove_file(path);
}

/// Turning an activity with assignments into a milestone is refused in the
/// Activity Type field with the plain reason and what to do, in the error
/// box every refused edit raises; nothing changes.
#[test]
fn a_resourced_activity_type_change_to_milestone_says_to_remove_assignments() {
    let (path, ctx, mut app, mut time) = setup("milestone", rich_starter());
    frame(&mut app, &ctx, vec![], &mut time);
    let task_id = records::num(&records::get(
        &app.project.document,
        "TASKRSRC",
        0,
        "task_id",
    ));
    let i = index_of(&app.project, task_id);
    let before = app.project.model.activities[i].task_type;
    assert!(!app.apply_cell(i, "activity_type", "Finish Milestone"));
    let shown = app.error.clone().expect("the refusal is shown");
    assert!(shown.contains("cannot become a milestone"), "{shown}");
    assert!(shown.contains("Remove its assignments first"), "{shown}");
    assert_eq!(app.project.model.activities[i].task_type, before);
    // The error box draws without a panic.
    frame(&mut app, &ctx, vec![], &mut time);
    assert!(app.error.is_some());
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn in_place_edits_validate_like_activity_details() {
    let (path, ctx, mut app, mut time) = setup("edit", rich_starter());
    for id in [
        "original_duration",
        "activity_type",
        "calendar",
        "wbs",
        "primary_constraint",
        "primary_constraint_date",
        "expected_finish",
        "physical_percent_complete",
        "code:Area",
    ] {
        app.grid
            .layout
            .columns
            .insert(2, layout::ColumnSetting::new(id, 60.));
    }
    frame(&mut app, &ctx, vec![], &mut time);
    let i = index_of(&app.project, 1030);
    // Double-click Activity ID, type a duplicate: refused.
    let pos = cell(&app, i, "activity_id");
    double_click(&mut app, &ctx, pos, &mut time);
    assert_eq!(app.edit_cell, Some((i, "activity_id".to_string())));
    select_all(&mut app, &ctx, &mut time);
    type_text(&mut app, &ctx, "RB-1000", &mut time);
    key(&mut app, &ctx, Key::Enter, Default::default(), &mut time);
    assert_eq!(app.project.model.activities[i].task_code, "RB-1030");
    assert!(app.error.as_deref().unwrap().contains("unique"));
    app.error = None;
    // A unique ID is accepted and undo restores the old one.
    assert!(app.apply_cell(i, "activity_id", "RB-1031"));
    assert_eq!(app.project.model.activities[i].task_code, "RB-1031");
    for (id, good, bad) in [
        ("original_duration", "16h", "-3"),
        ("remaining_duration", "12h", "lots"),
        ("activity_type", "Level of Effort", "Hammer"),
        ("calendar", "Standard 5-day / 8-hour", "Moon"),
        ("wbs", "RB.2", "RB.9"),
        ("physical_percent_complete", "40", "140"),
        ("primary_constraint", "Start On or After", "Sometime"),
        ("primary_constraint_date", "2026-12-01", "never"),
        ("expected_finish", "2026-12-20", "later"),
        ("code:Area", "N", "Z"),
    ] {
        assert!(!app.apply_cell(i, id, bad), "{id} accepted {bad:?}");
        assert!(
            app.apply_cell(i, id, good),
            "{id} refused {good:?}: {:?}",
            app.error
        );
    }
    let a = &app.project.model.activities[i];
    assert_eq!(a.rem, 12 * 60);
    assert_eq!(a.task_type, TaskType::Loe);
    assert_eq!(a.cstr, CstrType::StartOnAfter);
    assert!(a.cstr_date.is_some() && a.expect_end.is_some());
    let d = &app.project.details[&1030];
    // Until the activity starts, Remaining and Original Duration move together.
    assert_eq!(d.original_duration, 12 * 60);
    assert_eq!(d.percent, 40.);
    assert_eq!(d.wbs, 20);
    let catalog = columns::catalog(&app.project);
    let ix = app.grid_index();
    let area = columns::find(&catalog, "code:Area").unwrap();
    assert_eq!(
        columns::value(&app.project, &ix, area, i),
        Value::Text("N".into())
    );
    // A constraint date needs a constraint; read-only columns refuse edits.
    let j = index_of(&app.project, 1040);
    assert!(!app.apply_cell(j, "primary_constraint_date", "2026-12-01"));
    assert!(!app.apply_cell(j, "total_float", "3"));
    // The choice editor lists values and commits a click.
    frame(&mut app, &ctx, vec![], &mut time);
    let pos = cell(&app, j, "activity_type");
    double_click(&mut app, &ctx, pos, &mut time);
    assert!(app.edit_cell.is_some());
    click(&mut app, &ctx, "grid_choice_3", &mut time);
    assert_eq!(app.project.model.activities[j].task_type, TaskType::Mile);
    // F2 edits the focused cell (the error box the refused edits raised
    // is dismissed first: it holds the keyboard).
    app.error = None;
    frame(&mut app, &ctx, vec![], &mut time);
    app.selected = Some(j);
    app.grid.focus_column = Some("activity_name".into());
    key(&mut app, &ctx, Key::F2, Default::default(), &mut time);
    assert_eq!(app.edit_cell, Some((j, "activity_name".to_string())));
    key(&mut app, &ctx, Key::Escape, Default::default(), &mut time);
    assert!(app.edit_cell.is_none());
    // Undo returns the last edit.
    let ctrl = egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    };
    for _ in 0..3 {
        frame(&mut app, &ctx, vec![], &mut time);
    }
    let before = app.project.model.activities[j].task_type;
    key(&mut app, &ctx, Key::Z, ctrl, &mut time);
    assert_ne!(app.project.model.activities[j].task_type, before);
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn find_and_replace_select_and_edit_matching_cells() {
    let (path, ctx, mut app, mut time) = setup("find", scheduled_starter());
    let ctrl = egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    };
    key(&mut app, &ctx, Key::F, ctrl, &mut time);
    assert!(app.grid.dialogs.find.as_ref().is_some_and(|f| !f.replace));
    click(&mut app, &ctx, "find_what", &mut time);
    type_text(&mut app, &ctx, "commission", &mut time);
    click(&mut app, &ctx, "find_next", &mut time);
    let first = app.selected.unwrap();
    assert!(app
        .project
        .name(first)
        .to_lowercase()
        .contains("commission"));
    click(&mut app, &ctx, "find_next", &mut time);
    let second = app.selected.unwrap();
    assert_ne!(first, second);
    assert!(app
        .project
        .name(second)
        .to_lowercase()
        .contains("commission"));
    // Match case finds nothing for lower-case "Commissioning" mismatches.
    click(&mut app, &ctx, "find_match_case", &mut time);
    click(&mut app, &ctx, "find_what", &mut time);
    select_all(&mut app, &ctx, &mut time);
    type_text(&mut app, &ctx, "COMMISSION", &mut time);
    click(&mut app, &ctx, "find_next", &mut time);
    assert!(app.status.contains("not found"));
    click(&mut app, &ctx, "find_close", &mut time);
    assert!(app.grid.dialogs.find.is_none());
    // Replace All in names, case-insensitive; one undo step reverts it.
    key(&mut app, &ctx, Key::H, ctrl, &mut time);
    let f = app.grid.dialogs.find.as_mut().unwrap();
    assert!(f.replace);
    f.match_case = false;
    f.what = "install".into();
    f.with = "Erect".into();
    f.column = Some("activity_name".into());
    frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "replace_all", &mut time);
    let names: Vec<String> = (0..app.project.model.activities.len())
        .map(|i| app.project.name(i).to_string())
        .collect();
    assert!(names.contains(&"Erect structural assemblies".to_string()));
    assert!(names.contains(&"Mechanical Erectation".to_string()));
    assert!(app.status.starts_with("Replaced 2"));
    // Replace on an Activity ID that would duplicate another is refused.
    let f = app.grid.dialogs.find.as_mut().unwrap();
    f.what = "1010".into();
    f.with = "1000".into();
    f.column = Some("activity_id".into());
    frame(&mut app, &ctx, vec![], &mut time);
    click(&mut app, &ctx, "replace_all", &mut time);
    assert!(app.status.contains("1 rejected"));
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn render_table_dialogs_to_png() {
    use crate::headless_render::SoftwarePainter;
    let artifacts = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts");
    std::fs::create_dir_all(&artifacts).unwrap();
    let (path, ctx, mut app, mut time) = setup_with("render", rich_starter(), false);
    let mut painter = SoftwarePainter::default();
    type Shot = (&'static str, fn(&mut App));
    let shots: [Shot; 7] = [
        ("headless-grid-columns.png", |a| {
            a.open_columns_dialog();
            if let Some(d) = a.grid.dialogs.columns.as_mut() {
                d.open_groups.insert(columns::DATES.into());
            }
        }),
        ("headless-grid-filters.png", |a| a.open_filters_dialog()),
        ("headless-grid-filter-editor.png", |a| {
            a.open_filters_dialog();
            a.grid.dialogs.filter_editor = Some(super::dialogs::FilterEditor {
                original: None,
                filter: UserFilter {
                    name: "(Untitled Filter)".into(),
                    match_all: true,
                    rows: vec![Criterion {
                        column: "total_float".into(),
                        op: Op::LessOrEqual,
                        value: "0".into(),
                        high: String::new(),
                        level: 0,
                    }],
                },
                selected: Some(0),
            });
        }),
        ("headless-grid-group-sort.png", |a| {
            a.open_group_sort_dialog()
        }),
        ("headless-grid-open-layout.png", |a| a.open_layout_dialog()),
        ("headless-grid-find.png", |a| a.open_find(true)),
        ("headless-grid-grouped.png", |a| {
            a.grid.layout.group_sort.levels = vec![GroupLevel {
                column: "code:Area".into(),
                to_level: None,
                indent: true,
                interval: String::new(),
            }];
            for id in ["code:Area", "budgeted_total_cost", "predecessors"] {
                a.grid
                    .layout
                    .columns
                    .insert(2, layout::ColumnSetting::new(id, 90.));
            }
            a.grid.layout.group_sort.bands[0].background = [0, 0, 255];
            a.grid.layout.group_sort.bands[0].text = [255, 255, 0];
            a.grid.layout.group_sort.bands[0].font_size = 10.;
        }),
    ];
    for (name, open) in shots {
        app.grid.dialogs = Default::default();
        open(&mut app);
        for _ in 0..2 {
            let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
        }
        let (image, _) = painter
            .frame(&mut app, &ctx, &mut time, vec![], true)
            .unwrap();
        image.save(artifacts.join(name)).unwrap();
        println!(
            "Software-rendered table view: {}",
            artifacts.join(name).display()
        );
    }
    assert!(app.error.is_none(), "{:?}", app.error);
    drop(app);
    let _ = std::fs::remove_file(path);
}

/// A WBS chain 100,000 levels deep (depth comes from the XER file) must group
/// without recursion, and the table must draw it.
#[test]
fn a_100000_level_wbs_chain_groups_without_recursion() {
    let mut p = scheduled_starter();
    const DEPTH: i64 = 100_000;
    let base = 1_000_000;
    for n in 0..DEPTH {
        p.wbs.push(openxer_xer::Wbs {
            id: base + n,
            parent: (n > 0).then(|| base + n - 1).or(Some(10)),
            code: format!("L{n}"),
            name: format!("Level {n}"),
            sequence: 1,
        });
    }
    let deepest = base + DEPTH - 1;
    p.details.get_mut(&1000).unwrap().wbs = deepest;
    let l = layout::Layout::default();
    let built = build(&p, &l, &[]);
    let mut seen = activities(&built);
    seen.sort();
    assert_eq!(seen, (0..p.model.activities.len()).collect::<Vec<_>>());
    let last = built
        .bands
        .iter()
        .find(|b| b.key.ends_with(&format!("w{deepest}|")))
        .unwrap();
    assert_eq!(last.level, DEPTH as usize);
    assert_eq!(&*last.members, &[index_of(&p, 1000)]);
    // Codes stay bounded on the deep path.
    assert!(last.label.len() < 400, "{}", last.label.len());
    // To Level 3 lumps the chain into the third band.
    let mut lumped = l.clone();
    lumped.group_sort.levels[0].to_level = Some(3);
    let built = build(&p, &lumped, &[]);
    assert!(built.bands.iter().all(|b| b.level < 3));
    assert_eq!(activities(&built).len(), p.model.activities.len());
    // The production table builds and paints the rows.
    let (path, ctx, mut app, mut time) = setup("deep", p);
    frame(&mut app, &ctx, vec![], &mut time);
    assert!(app.rows().len() > DEPTH as usize);
    assert!(app.error.is_none(), "{:?}", app.error);
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn find_and_replace_walk_characters_of_any_width() {
    use super::dialogs::{find_span, replace_matches};
    // Lowercasing changes byte lengths here; matches must use the text's own offsets.
    assert_eq!(replace_matches("İẞx", "ß", "ss", false), "İssx");
    assert_eq!(replace_matches("ẞİa", "a", "b", false), "ẞİb");
    assert_eq!(
        replace_matches("İstanbul İzmir", "istanbul", "X", false),
        "İstanbul İzmir"
    );
    assert_eq!(replace_matches("İstanbul", "İ", "I", true), "Istanbul");
    assert_eq!(replace_matches("ΣΑΣ σας", "σ", "x", false), "xΑx xας");
    assert_eq!(replace_matches("ΣΑΣ", "σ", "x", true), "ΣΑΣ");
    assert_eq!(
        replace_matches("🚧 Install pump 🚧 install", "INSTALL", "Erect", false),
        "🚧 Erect pump 🚧 Erect"
    );
    assert_eq!(replace_matches("👷👷", "👷", "W", false), "WW");
    assert_eq!(replace_matches("abc", "", "x", false), "abc");
    assert_eq!(find_span("aẞẞb", "ßB", false), Some((4, 8)));
    assert_eq!(find_span("İstanbul", "stan", false), Some((2, 6)));
    assert_eq!(find_span("ΣΑΣ", "ας", false), None);
    assert_eq!(find_span("x🚧y", "🚧Y", false), Some((1, 6)));
    // Every boundary returned is a char boundary of the original text.
    for (text, needle) in [("İẞΣ🚧ß", "ß"), ("ǅǆǄ", "ǆ"), ("ﬃ ffi", "ffi")] {
        let mut at = 0;
        while let Some((s, e)) = find_span(&text[at..], needle, false) {
            assert!(text.is_char_boundary(at + s) && text.is_char_boundary(at + e));
            at += e;
        }
    }
    // Through the dialog: Find and Replace All on mixed-width names.
    let mut p = scheduled_starter();
    p.details.get_mut(&1000).unwrap().name = "İẞ 🚧 Notice".into();
    let (path, ctx, mut app, mut time) = setup("unicode", p);
    let f = super::dialogs::FindState {
        what: "ß 🚧".into(),
        with: "SS".into(),
        column: Some("activity_name".into()),
        match_case: false,
        replace: true,
    };
    assert!(app.find_next(&f));
    assert_eq!(app.project.name(app.selected.unwrap()), "İẞ 🚧 Notice");
    assert_eq!(app.replace_all(&f), (1, 0));
    assert_eq!(app.project.name(index_of(&app.project, 1000)), "İSS Notice");
    frame(&mut app, &ctx, vec![], &mut time);
    drop(app);
    let _ = std::fs::remove_file(path);
}

#[test]
fn band_styles_default_to_the_theme_levels_and_follow_their_fonts() {
    // Band row pitch from the font: 8 and 9 pt 18 px, 11 pt 22 px, 12 pt 24 px.
    for (size, pitch) in [(8., 18.), (9., 18.), (11., 22.), (12., 24.)] {
        assert_eq!(layout::band_row_height(size), pitch, "{size} pt");
    }
    let gs = layout::Layout::default().group_sort;
    assert_eq!(gs.bands.len(), layout::MAX_BAND_LEVELS);
    let first = &gs.bands[0];
    // Level 1 is navy with white text at 12 pt, level 2 light blue with navy
    // text at 11 pt, level 3 9 pt, deeper levels 8 pt: the top bands are
    // taller than activity rows.
    assert_eq!(
        (first.background, first.text),
        ([0x1F, 0x3A, 0x5F], [255; 3])
    );
    assert_eq!((first.font_size, first.row_height), (12., 24.));
    assert_eq!(gs.bands[1].background, [0xD6, 0xE4, 0xF2]);
    assert_eq!(gs.bands[1].text, [0x1F, 0x3A, 0x5F]);
    assert_eq!((gs.bands[1].font_size, gs.band_height(1)), (11., 22.));
    assert_eq!((gs.bands[2].font_size, gs.band_height(2)), (9., 18.));
    assert!(gs.bands.iter().skip(3).all(|b| b.font_size == 8.));
    // A layout saved with its own band fonts keeps them.
    let mut own = layout::Layout::default();
    own.group_sort.bands[0].font_size = 9.;
    own.group_sort.bands[0].row_height = 18.;
    own.group_sort.bands[1].font_size = 8.;
    own.group_sort.bands[1].row_height = 18.;
    let back = layout::Layout::from_json(&own.to_json()).unwrap();
    assert_eq!(back.group_sort.bands[0].font_size, 9.);
    assert_eq!(back.group_sort.bands[1].font_size, 8.);
    assert_eq!(back.group_sort.band_height(0), 18.);
    assert!(gs.bands.iter().all(|b| b.bold));
    // A stored height never makes a band shorter than its font needs.
    let mut short = gs.clone();
    short.bands[0].font_size = 12.;
    short.bands[0].row_height = 18.;
    assert_eq!(short.band_height(0), 24.);
    short.bands[3].row_height = 30.;
    assert_eq!(short.band_height(3), 30.);
    // Layouts saved with the old single gray style get the measured styles;
    // edited styles are kept.
    let mut old = layout::Layout::default();
    old.group_sort.bands = vec![layout::BandStyle::default(); layout::MAX_BAND_LEVELS];
    let back = layout::Layout::from_json(&old.to_json()).unwrap();
    assert_eq!(back.group_sort.bands, layout::default_bands());
    old.group_sort.bands[4].background = [1, 2, 3];
    let back = layout::Layout::from_json(&old.to_json()).unwrap();
    assert_eq!(back.group_sort.bands[4].background, [1, 2, 3]);
    assert_eq!(back.group_sort.bands[0], layout::BandStyle::default());
}

#[test]
fn rows_take_band_heights_and_strip_depths_and_the_gantt_follows() {
    let (path, ctx, mut app, mut time) = setup("row-metrics", wbs_child_project());
    // Bands of 12 and 11 pt (24 and 22 px) stand for the larger band fonts.
    for (level, size) in [(0, 12.), (1, 11.)] {
        let band = &mut app.grid.layout.group_sort.bands[level];
        band.font_size = size;
        band.row_height = layout::band_row_height(size);
    }
    frame(&mut app, &ctx, vec![], &mut time);
    let rows = app.rows();
    let bands = app.grid.bands.borrow().clone();
    let m = app.row_metrics(&rows);
    let activity = app.gantt_row_height();
    let mut under = 0;
    for (k, row) in rows.iter().enumerate() {
        match *row {
            Row::Group(b) => {
                let level = bands[b].level;
                assert_eq!(m.depths[k], level);
                assert_eq!(m.heights[k], [24., 22.].get(level).copied().unwrap_or(18.));
                under = level + 1;
            }
            Row::Activity(_) => {
                assert_eq!((m.depths[k], m.heights[k]), (under, activity));
            }
        }
        assert_eq!(m.offsets[k + 1], m.offsets[k] + m.heights[k]);
    }
    assert!(
        m.depths.contains(&2),
        "the child WBS rows sit two strips in"
    );
    assert_eq!(m.row_at(0.), 0);
    assert_eq!(m.row_at(m.offsets[3] + 0.5), 3);
    assert_eq!(m.row_at(1e9), rows.len() - 1);
    // The painted table and Gantt rows use these offsets.
    assert_eq!(app.grid.row_offsets, m.offsets);
    for (k, row) in rows.iter().enumerate() {
        let Row::Activity(i) = *row else { continue };
        let Some(bar) = app.bar_geometry.iter().find(|b| b.activity == i) else {
            continue;
        };
        assert_eq!(bar.row.top(), app.grid.body_top + m.offsets[k]);
        assert_eq!(bar.row.height(), m.heights[k]);
    }
    drop(app);
    let _ = std::fs::remove_file(path);
}

/// Bar and timescale changes count as layout changes: opening another
/// layout asks to save them, and Yes keeps them in the layout.
#[test]
fn unsaved_bar_and_timescale_changes_prompt_before_another_layout_opens() {
    use crate::headless_render::SoftwarePainter;
    let (path, ctx, mut app, mut time) =
        setup_with("layout-gantt-prompt", scheduled_starter(), false);
    let mut painter = SoftwarePainter::default();
    app.save_layout_as("Bars").unwrap();
    assert!(!app.layout_changed(), "just saved");
    // A Bar Chart Options change.
    app.gantt.options.show_relationships = !app.gantt.options.show_relationships;
    assert!(
        app.layout_changed(),
        "a bar chart option is an unsaved change"
    );
    app.save_layout_as("Bars").unwrap();
    // A Bars change.
    app.gantt.bars[0].display = !app.gantt.bars[0].display;
    assert!(app.layout_changed(), "a bar change is an unsaved change");
    app.save_layout_as("Bars").unwrap();
    // A Timescale change.
    app.gantt.timescale.three_lines = !app.gantt.timescale.three_lines;
    assert!(
        app.layout_changed(),
        "a timescale change is an unsaved change"
    );
    // Opening the standard layout asks first.
    app.open_layout_dialog();
    for _ in 0..2 {
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    }
    painter.click_action(&mut app, &ctx, &mut time, "open_layout_item_0");
    painter.click_action(&mut app, &ctx, &mut time, "open_layout_open");
    assert!(app.grid.dialogs.save_prompt.is_some(), "the question");
    let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
    let (image, _) = painter
        .frame(&mut app, &ctx, &mut time, vec![], true)
        .unwrap();
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/bugfix-help-audit");
    std::fs::create_dir_all(&out).unwrap();
    image
        .save(out.join("layout-save-prompt-gantt.png"))
        .unwrap();
    let changed = app.gantt.timescale.clone();
    painter.click_action(&mut app, &ctx, &mut time, "prompt_yes");
    assert_eq!(app.grid.layout.name, layout::STANDARD_NAME);
    let saved = app.grid.library.iter().find(|l| l.name == "Bars").unwrap();
    let settings = crate::gantt::settings::GanttSettings::from_json(
        &saved.gantt.as_ref().unwrap()["settings"].to_string(),
    )
    .unwrap();
    assert_eq!(settings.timescale, changed, "Yes kept the timescale change");
    drop(app);
    let _ = std::fs::remove_file(path);
}

/// Group and Sort: the band Font & Color editor opens for every level the
/// grid lists, not just the first eight.
#[test]
fn band_font_and_color_editor_opens_for_every_level() {
    use crate::headless_render::SoftwarePainter;
    let (path, ctx, mut app, mut time) =
        setup_with("band-editor-levels", scheduled_starter(), false);
    let mut painter = SoftwarePainter::default();
    app.open_group_sort_dialog();
    let columns = [
        "status",
        "activity_id",
        "activity_name",
        "start",
        "finish",
        "total_float",
        "free_float",
        "remaining_duration",
        "original_duration",
        "calendar",
        "activity_type",
        "duration_type",
    ];
    {
        let d = app.grid.dialogs.group_sort.as_mut().unwrap();
        d.gs.levels = columns
            .iter()
            .map(|c| GroupLevel {
                column: (*c).into(),
                to_level: None,
                indent: true,
                interval: String::new(),
            })
            .collect();
        // A layout saved with fewer band styles than levels.
        d.gs.bands.truncate(9);
    }
    let editor = egui::Id::new("group_band_editor");
    for level in [0, 8, 11] {
        app.grid.dialogs.group_sort.as_mut().unwrap().band_edit = Some(level);
        for _ in 0..2 {
            let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
        }
        let d = app.grid.dialogs.group_sort.as_ref().unwrap();
        assert_eq!(d.band_edit, Some(level));
        assert!(
            ctx.memory(|m| m
                .areas()
                .is_visible(&egui::LayerId::new(egui::Order::Middle, editor))),
            "the editor shows for level {}",
            level + 1
        );
        assert!(d.gs.bands.len() > level, "level {} has a style", level + 1);
    }
    drop(app);
    let _ = std::fs::remove_file(path);
}
