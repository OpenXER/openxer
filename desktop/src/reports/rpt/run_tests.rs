//! Generation and output tests on the starter project.
use super::edit::{self, CellSpec, Kind, NewCell};
use super::engine::{Generated, Options};
use super::stream::Stream;
use super::writers;
use crate::grid::columns::Index;
use crate::reports::test_support::scheduled_starter;
use chrono::NaiveDate;

pub(crate) fn now() -> chrono::NaiveDateTime {
    NaiveDate::from_ymd_opt(2026, 11, 5)
        .unwrap()
        .and_hms_opt(8, 30, 0)
        .unwrap()
}

pub(crate) fn run(s: &Stream) -> Generated {
    let p = scheduled_starter();
    let ix = Index::new((false, 0.));
    super::generate(s, &p, &ix, &Options::new(now())).unwrap()
}

/// A WBS tree with its activities and a total per element.
fn tree_report() -> Stream {
    let mut s = edit::new_report("Activities by WBS", true);
    let head = edit::container_path(&s, Kind::PageHeader).unwrap();
    let row = edit::add_row(&mut s, &head, 22).unwrap();
    edit::add_cell(
        &mut s,
        &row,
        &CellSpec::new(NewCell::Variable("[title]".into()), 0, 600).heading(),
    )
    .unwrap();
    let row = edit::add_row(&mut s, &head, 20).unwrap();
    for (i, (f, w)) in [
        ("task_code", 100),
        ("task_name", 300),
        ("start_date", 100),
        ("end_date", 100),
    ]
    .iter()
    .enumerate()
    {
        let left = [0, 100, 400, 500][i];
        edit::add_cell(
            &mut s,
            &row,
            &CellSpec::new(
                NewCell::Title {
                    field: (*f).into(),
                    title: String::new(),
                },
                left,
                *w,
            )
            .heading(),
        )
        .unwrap();
    }
    let detail = edit::container_path(&s, Kind::Detail).unwrap();
    let wbs = edit::add_group(&mut s, &detail, "PROJWBS", None, true).unwrap();
    let wh = edit::group_part(&s, &wbs, Kind::GroupHeader).unwrap();
    let r = edit::add_row(&mut s, &wh, 20).unwrap();
    let mut c = CellSpec::new(NewCell::Field("wbs_name".into()), 0, 400).heading();
    c.bold = true;
    let cell = edit::add_cell(&mut s, &r, &c).unwrap();
    edit::get_mut(&mut s.root, &cell)
        .unwrap()
        .set_flag("ShowIndent", true);
    let wb = edit::group_part(&s, &wbs, Kind::Detail).unwrap();
    let tasks = edit::add_group(&mut s, &wb, "PROJWBS", Some("TASK"), false).unwrap();
    let tb = edit::group_part(&s, &tasks, Kind::Detail).unwrap();
    let r = edit::add_row(&mut s, &tb, 18).unwrap();
    for (f, left, w) in [
        ("task_code", 0, 100),
        ("task_name", 100, 300),
        ("start_date", 400, 100),
        ("end_date", 500, 100),
    ] {
        edit::add_cell(
            &mut s,
            &r,
            &CellSpec::new(NewCell::Field(f.into()), left, w),
        )
        .unwrap();
    }
    let wf = edit::group_part(&s, &wbs, Kind::GroupFooter).unwrap();
    let r = edit::add_row(&mut s, &wf, 20).unwrap();
    edit::add_cell(
        &mut s,
        &r,
        &CellSpec::new(
            NewCell::Summary {
                field: "remain_cost".into(),
                path: "TASK-PROJWBS:".into(),
                mode: "amSum".into(),
            },
            400,
            200,
        )
        .right(),
    )
    .unwrap();
    s
}

#[test]
fn a_wbs_tree_with_activities_runs() {
    let g = run(&tree_report());
    assert!(g.warnings.is_empty(), "{:?}", g.warnings);
    let lines = g.all_lines();
    let texts: Vec<String> = lines
        .iter()
        .map(|l| {
            l.cells
                .iter()
                .map(|c| c.text.as_str())
                .collect::<Vec<_>>()
                .join("|")
        })
        .collect();
    assert!(texts.iter().any(|t| t.contains("RB-1000")), "{texts:#?}");
    assert!(texts[0].contains("Activities by WBS"));
    let p = scheduled_starter();
    let shown = texts.iter().filter(|t| t.contains("RB-")).count();
    assert_eq!(shown, p.model.activities.len());
    // Same inputs, same output.
    assert_eq!(g, run(&tree_report()));
    let html = writers::html(&g);
    assert!(writers::html_is_balanced(&html).unwrap() > 20);
    assert!(html.contains("RB-1000"));
    let text = writers::text(&g, &writers::TextOptions::default());
    assert!(
        text.lines().any(|l| l.starts_with("\"RB-1000\"\t")),
        "{text}"
    );
    let pdf = writers::pdf(&g);
    assert!(crate::print::pdf::check(&pdf).unwrap() >= 1);
}

fn corpus_projects() -> Vec<(String, openxer_xer::ProjectData)> {
    let dir = crate::test_corpus::dir();
    let Ok(read) = std::fs::read_dir(&dir) else {
        println!("SKIPPED: XER corpus not found at {}", dir.display());
        return vec![];
    };
    let mut files: Vec<_> = read
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("xer")))
        .collect();
    files.sort();
    let mut out = vec![];
    for f in files {
        let name = f.file_stem().unwrap().to_string_lossy().to_string();
        let Ok(doc) = openxer_xer::Document::read(&f) else {
            continue;
        };
        let projects = openxer_interchange::baseline::schedule_projects(&doc);
        let Some((pid, _)) = projects.first().cloned() else {
            continue;
        };
        if let Ok(mut p) = openxer_xer::ProjectData::from_document(doc, pid, name.clone()) {
            let _ = crate::scheduling::schedule_model(&mut p.model, &p.resources);
            out.push((name, p));
        }
    }
    out
}

#[test]
fn every_library_report_round_trips_and_runs_on_the_starter_project() {
    let p = scheduled_starter();
    let ix = Index::new((false, 0.));
    for e in super::library::entries() {
        let s = Stream::parse(e.text).unwrap_or_else(|err| panic!("{}: {err}", e.id));
        assert_eq!(s.write(), e.text, "{} does not round trip", e.id);
        let g = super::generate(&s, &p, &ix, &Options::new(now()))
            .unwrap_or_else(|err| panic!("{}: {err}", e.id));
        assert!(!g.pages.is_empty());
        assert!(g.warnings.is_empty(), "{}: {:?}", e.id, g.warnings);
        let lines = g.all_lines();
        assert!(lines.len() >= 4, "{} is empty", e.id);
        assert_eq!(
            g,
            super::generate(&s, &p, &ix, &Options::new(now())).unwrap()
        );
        let html = writers::html(&g);
        writers::html_is_balanced(&html).unwrap();
        crate::print::pdf::check(&writers::pdf(&g)).unwrap();
    }
}

#[test]
fn library_reports_run_on_the_corpus_without_errors() {
    let projects = corpus_projects();
    let mut runs = 0;
    for (name, p) in &projects {
        let ix = Index::new((false, 0.));
        for e in super::library::entries() {
            let s = Stream::parse(e.text).unwrap();
            let g = super::generate(&s, p, &ix, &Options::new(now()))
                .unwrap_or_else(|err| panic!("{} on {name}: {err}", e.id));
            assert!(
                g.warnings.iter().all(|w| !w.starts_with("Unknown")),
                "{} on {name}: {:?}",
                e.id,
                g.warnings
            );
            runs += 1;
            crate::print::pdf::check(&writers::pdf(&g)).unwrap();
        }
    }
    println!(
        "{runs} library runs over {} corpus projects",
        projects.len()
    );
}

/// Writes every library report in every format to the ignored artifacts
/// folder, for looking at.
#[test]
fn write_library_samples() {
    let out = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts/r1/library");
    std::fs::create_dir_all(&out).unwrap();
    let p = scheduled_starter();
    let ix = Index::new((false, 0.));
    for e in super::library::entries() {
        let s = Stream::parse(e.text).unwrap();
        let g = super::generate(&s, &p, &ix, &Options::new(now())).unwrap();
        std::fs::write(out.join(format!("{}.pdf", e.id)), writers::pdf(&g)).unwrap();
        std::fs::write(out.join(format!("{}.html", e.id)), writers::html(&g)).unwrap();
        std::fs::write(
            out.join(format!("{}.txt", e.id)),
            writers::text(&g, &writers::TextOptions::default()),
        )
        .unwrap();
    }
    // The same reports on a corpus project that has resource assignments.
    for (name, p) in corpus_projects().iter().take(8) {
        let ix = Index::new((false, 0.));
        if crate::views::assignments::assignments(p).len() < 5 {
            continue;
        }
        for e in super::library::entries() {
            let s = Stream::parse(e.text).unwrap();
            let g = super::generate(&s, p, &ix, &Options::new(now())).unwrap();
            std::fs::write(out.join(format!("{}-{name}.pdf", e.id)), writers::pdf(&g)).unwrap();
        }
        break;
    }
}

#[test]
fn every_vocabulary_field_binds_on_a_corpus_project() {
    use super::data::{vocabulary, Data, SOURCES};
    let projects = corpus_projects();
    for (name, p) in projects.iter().take(6) {
        let ix = Index::new((false, 0.));
        let d = Data::new(p, &ix, now());
        for (table, _) in SOURCES {
            let recs = d.records(table).unwrap();
            for r in recs.iter().take(50) {
                for (field, _) in vocabulary(table) {
                    let _ = d.text(*r, field);
                }
            }
        }
        let w = d.take_warnings();
        assert!(w.is_empty(), "{name}: {w:?}");
    }
}

#[test]
fn hostile_reports_stop_at_the_line_limit_and_nesting_depth() {
    // Every activity against every activity, nested sections, and a
    // hierarchy joined to itself: bounded by the line limit and depths.
    let mut s = edit::new_report("Hostile", true);
    let mut holder = edit::container_path(&s, Kind::Detail).unwrap();
    for level in 0..14 {
        let g = edit::add_group(&mut s, &holder, "TASK", None, false).unwrap();
        let body = edit::group_part(&s, &g, Kind::Detail).unwrap();
        let r = edit::add_row(&mut s, &body, 18).unwrap();
        edit::add_cell(
            &mut s,
            &r,
            &CellSpec::new(NewCell::Field("task_code".into()), 0, 100),
        )
        .unwrap();
        if level % 5 == 4 {
            // A hierarchy that continues forever if it is not bounded.
            let t = edit::add_group(&mut s, &body, "PROJWBS", None, true).unwrap();
            edit::get_mut(&mut s.root, &t)
                .unwrap()
                .set_int("MaxLevels", 0);
        }
        holder = body;
    }
    let p = scheduled_starter();
    let ix = Index::new((false, 0.));
    let mut opts = Options::new(now());
    opts.max_lines = 500;
    let started = std::time::Instant::now();
    let g = super::generate(&s, &p, &ix, &opts).unwrap();
    assert!(started.elapsed().as_secs() < 20);
    assert!(g.truncated || g.warnings.iter().any(|w| w.contains("nested too deeply")));
    let lines: usize = g.pages.iter().map(|p| p.body.len()).sum();
    assert!(lines <= 500, "{lines}");
    writers::html(&g);
    crate::print::pdf::check(&writers::pdf(&g)).unwrap();
}

/// Loops that visit many records and never print a line are cut off by the
/// work budget, not by the line limit.
#[test]
fn loops_that_print_nothing_are_stopped_by_the_work_budget() {
    use super::filter::{Filter, Op};
    let mut s = edit::new_report("Silent", true);
    let mut holder = edit::container_path(&s, Kind::Detail).unwrap();
    for _ in 0..6 {
        let g = edit::add_group(&mut s, &holder, "TASK", None, false).unwrap();
        holder = edit::group_part(&s, &g, Kind::Detail).unwrap();
    }
    let g = edit::add_group(&mut s, &holder, "TASK", None, false).unwrap();
    let never = Filter::Cmp {
        field: "task_code".into(),
        op: Op::Eq,
        values: vec!["NO-SUCH-CODE".into()],
    };
    edit::set_lines(
        edit::get_mut(&mut s.root, &g).unwrap(),
        "Filter.Strings",
        &super::filter::write(&never),
    );
    let body = edit::group_part(&s, &g, Kind::Detail).unwrap();
    let r = edit::add_row(&mut s, &body, 18).unwrap();
    edit::add_cell(
        &mut s,
        &r,
        &CellSpec::new(NewCell::Field("task_code".into()), 0, 100),
    )
    .unwrap();
    let p = scheduled_starter();
    let ix = Index::new((false, 0.));
    let mut opts = Options::new(now());
    opts.max_work = 50_000;
    let started = std::time::Instant::now();
    let g = super::generate(&s, &p, &ix, &opts).unwrap();
    assert!(started.elapsed().as_secs() < 20);
    assert!(g.truncated);
    assert!(
        g.warnings.iter().any(|w| w.contains("steps of work")),
        "{:?}",
        g.warnings
    );
    assert!(!g.warnings.iter().any(|w| w.contains("lines")));
    // The same report with the default budget finishes on a small project
    // only if it fits; a tiny definition always does.
    let small = tree_report();
    let g = super::generate(&small, &p, &ix, &Options::new(now())).unwrap();
    assert!(!g.truncated);
}
