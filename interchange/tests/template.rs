use openxer_engine::scheduler::schedule_with_resources;
use openxer_interchange::columns::{Column, DurationUnit};
use openxer_interchange::export_template::{
    find, list_json, parse_list, Condition, ExportTemplate, Op, SortKey,
};
use openxer_interchange::xlsx_export::write_modern;
use openxer_interchange::xlsx_import::read_workbook;
use std::path::PathBuf;

fn out_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("openxer-template-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn project() -> openxer_xer::ProjectData {
    let mut p = openxer_xer::starter();
    schedule_with_resources(&mut p.model, &p.resources).unwrap();
    p
}

fn template() -> ExportTemplate {
    let mut t = ExportTemplate::new(
        "Weekly status",
        &[
            Column::ActivityId,
            Column::ActivityName,
            Column::RemainingDuration,
            Column::Finish,
            Column::TotalFloat,
        ],
    );
    t.unit = DurationUnit::Hours;
    t.match_all = true;
    t.filter = vec![Condition {
        column: Column::RemainingDuration.key(),
        op: Op::GreaterThan,
        value: "0".into(),
    }];
    t.sort = vec![
        SortKey {
            column: Column::RemainingDuration.key(),
            descending: true,
        },
        SortKey {
            column: Column::ActivityId.key(),
            descending: false,
        },
    ];
    t
}

fn ids(path: &std::path::Path) -> Vec<String> {
    let wb = read_workbook(path).unwrap();
    let s = wb.sheet("Activities").unwrap();
    wb.sheets[s]
        .rows
        .iter()
        .skip(1)
        .map(|r| r[0].text())
        .collect()
}

#[test]
fn a_template_filters_and_sorts_the_activities() {
    let p = project();
    let t = template();
    t.validate().unwrap();
    let mut options = t.options(&p).unwrap();
    options.group_by_wbs = false;
    let file = out_dir().join("filtered.xlsx");
    let _ = std::fs::remove_file(&file);
    write_modern(&p, &options, &file).unwrap();
    let rows = ids(&file);
    // Milestones have no remaining duration and drop out; the rest run from
    // the longest remaining duration down, ties by Activity ID.
    assert!(!rows.is_empty() && rows.len() < p.model.activities.len());
    let remaining = |code: &str| {
        p.model
            .activities
            .iter()
            .find(|a| a.task_code == code)
            .unwrap()
            .rem
    };
    assert!(rows.iter().all(|r| remaining(r) > 0));
    assert!(rows.windows(2).all(|w| remaining(&w[0]) > remaining(&w[1])
        || (remaining(&w[0]) == remaining(&w[1]) && w[0] < w[1])));
    // Match any: a second condition widens the result.
    let mut any = t.clone();
    any.match_all = false;
    any.filter.push(Condition {
        column: Column::ActivityName.key(),
        op: Op::Contains,
        value: "NOTICE".into(),
    });
    let wider = any.options(&p).unwrap().rows.unwrap();
    assert_eq!(wider.len(), rows.len() + 1);
}

#[test]
fn a_saved_template_reruns_to_the_same_workbook() {
    let p = project();
    let (templates, rejected) = parse_list(&list_json(&[template()]));
    assert!(rejected.is_empty(), "{rejected:?}");
    let saved = find(&templates, "weekly STATUS").expect("found ignoring case");
    let dir = out_dir();
    let (a, b) = (dir.join("run-a.xlsx"), dir.join("run-b.xlsx"));
    let script = r#"
import sys, zipfile
a, b = (zipfile.ZipFile(p) for p in sys.argv[1:3])
assert a.namelist() == b.namelist(), (a.namelist(), b.namelist())
for n in a.namelist():
    if n == 'docProps/core.xml':
        continue
    assert a.read(n) == b.read(n), n
print('identical', len(a.namelist()))
"#;
    // The Info sheet stamps the export time to the minute, so two runs that
    // straddle a minute boundary differ there. Two attempts back to back
    // cannot both straddle one.
    let mut last = String::new();
    for _ in 0..2 {
        for f in [&a, &b] {
            let _ = std::fs::remove_file(f);
            write_modern(&p, &saved.options(&p).unwrap(), f).unwrap();
        }
        let out = std::process::Command::new("python.exe")
            .arg("-c")
            .arg(script)
            .arg(&a)
            .arg(&b)
            .output();
        match out {
            Ok(o) if o.status.success() => {
                eprintln!("{}", String::from_utf8_lossy(&o.stdout));
                return;
            }
            Ok(o) => last = String::from_utf8_lossy(&o.stderr).into_owned(),
            Err(e) => {
                eprintln!("SKIPPED: python.exe not available ({e})");
                return;
            }
        }
    }
    panic!("workbooks differ: {last}");
}

#[test]
fn malformed_templates_are_rejected_on_load() {
    let good = template();
    let bad = |edit: fn(&mut ExportTemplate)| {
        let mut t = good.clone();
        edit(&mut t);
        t
    };
    let cases: Vec<(&str, ExportTemplate)> = vec![
        ("no name", bad(|t| t.name = "  ".into())),
        ("no columns", bad(|t| t.columns.clear())),
        ("unknown column", bad(|t| t.columns.push("no_such".into()))),
        ("twice", bad(|t| t.columns.push(Column::ActivityId.key()))),
        ("subject", bad(|t| t.subject = "Expenses".into())),
        ("date format", bad(|t| t.date_format = "d/m".into())),
        (
            "sort",
            bad(|t| {
                t.sort.push(SortKey {
                    column: "zzz".into(),
                    descending: false,
                })
            }),
        ),
        ("number", bad(|t| t.filter[0].value = "many".into())),
        (
            "value",
            bad(|t| {
                t.filter.push(Condition {
                    column: Column::ActivityName.key(),
                    op: Op::Contains,
                    value: " ".into(),
                })
            }),
        ),
    ];
    for (what, t) in &cases {
        assert!(t.validate().is_err(), "{what} should be refused");
    }
    // One good template and each bad one in a saved list: only the good one loads.
    let mut all = vec![good.clone()];
    all.extend(cases.into_iter().map(|(_, t)| t));
    let (loaded, rejected) = parse_list(&serde_json::to_string(&all).unwrap());
    assert_eq!(loaded, vec![good.clone()]);
    assert_eq!(rejected.len(), 9, "{rejected:?}");
    // Unknown fields, wrong types and broken JSON are rejected too.
    let (loaded, rejected) = parse_list(r#"[{"name":"x","subject":"Activities","extra":1}]"#);
    assert!((loaded.is_empty(), rejected.len()) == (true, 1));
    let (loaded, rejected) = parse_list("[1, 2");
    assert!(loaded.is_empty() && rejected.len() == 1);
    let (loaded, rejected) = parse_list("{}");
    assert!(loaded.is_empty() && rejected.len() == 1);
    // A duplicate name is not loaded twice.
    let (loaded, rejected) = parse_list(&serde_json::to_string(&[good.clone(), good]).unwrap());
    assert_eq!((loaded.len(), rejected.len()), (1, 1));
}

#[test]
fn a_template_naming_a_column_the_project_lacks_cannot_export() {
    let p = project();
    let mut t = template();
    t.columns.push("udf:99".into());
    t.validate().unwrap();
    let err = t.options(&p).unwrap_err();
    assert!(err.contains("udf:99"), "{err}");
}
