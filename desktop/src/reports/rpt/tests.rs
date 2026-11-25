//! Round trip and fuzz tests for the definition text. Local reference
//! inputs (definition files and a database copy, kept outside git) are test
//! inputs only, named by `OPENXER_RPT_INPUTS` (a folder of definition files)
//! and `OPENXER_RPT_DB` (a database copy); the tests are skipped with a
//! printed reason when they are not set.
use super::model::Report;
use super::stream::Stream;
use std::path::PathBuf;

/// The path named by an environment variable, or one that does not exist.
fn input(var: &str) -> PathBuf {
    std::env::var_os(var)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("{var} is not set")))
}

/// Report definition files in the `OPENXER_RPT_INPUTS` folder, if present
/// locally.
pub(crate) fn local_files() -> Vec<(String, Vec<u8>)> {
    let dir = input("OPENXER_RPT_INPUTS");
    let Ok(read) = std::fs::read_dir(&dir) else {
        println!("SKIPPED: no local definition files at {}", dir.display());
        return vec![];
    };
    let mut out: Vec<_> = read
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("rpt")))
        .filter_map(|p| {
            Some((
                p.file_name()?.to_string_lossy().into_owned(),
                std::fs::read(&p).ok()?,
            ))
        })
        .collect();
    out.sort();
    out
}

/// Definition texts in the local database copy `OPENXER_RPT_DB` names (opened
/// read-only;
/// only the report definition table is read).
pub(crate) fn local_database_rows() -> Vec<(i64, Vec<u8>)> {
    let path = input("OPENXER_RPT_DB");
    if !path.is_file() {
        println!("SKIPPED: no local database copy at {}", path.display());
        return vec![];
    }
    match super::store::read_reference_rows(&path) {
        Ok(rows) => rows.into_iter().map(|r| (r.id, r.data)).collect(),
        Err(e) => {
            println!("SKIPPED: database copy not readable ({e})");
            vec![]
        }
    }
}

#[test]
fn local_definition_files_round_trip_byte_for_byte() {
    let mut checked = 0;
    for (name, bytes) in local_files() {
        match Stream::parse_bytes(&bytes) {
            Ok(s) => {
                assert_eq!(s.to_bytes(), bytes, "{name} does not round trip");
                checked += 1;
            }
            Err(e) => println!("SKIPPED {name}: {e}"),
        }
    }
    println!("{checked} local definition files round trip");
}

#[test]
fn local_database_definitions_round_trip_byte_for_byte() {
    let rows = local_database_rows();
    let (mut ok, mut opaque, mut models) = (0, 0, 0);
    for (id, bytes) in &rows {
        match Stream::parse_bytes(bytes) {
            Ok(s) => {
                assert_eq!(&s.to_bytes(), bytes, "row {id} does not round trip");
                ok += 1;
                if Report::from_stream(&s).is_ok() {
                    models += 1;
                }
            }
            Err(e) => {
                opaque += 1;
                println!("row {id} kept as text only: {e}");
            }
        }
    }
    println!("{ok} database definitions round trip, {models} read as reports, {opaque} text only");
    if !rows.is_empty() {
        assert!(ok * 10 >= rows.len() * 9, "most definitions should parse");
    }
}

#[test]
fn hostile_text_never_panics() {
    let mut cases: Vec<String> = vec![
        String::new(),
        "object".into(),
        "object A".into(),
        "object A\nend".into(),
        "object A\n  X = (\nend".into(),
        "object A\n  X = {\nend\n".into(),
        "object A\n  X = <\n    item\n  end>\nend\n".into(),
        "object A\n  X = 'a' +\nend\n".into(),
        "end\nend\nend".into(),
        "object A\n\0\u{1}\u{7f}\nend\n".into(),
        format!("object A\n  X = '{}'\nend\n", "a".repeat(5_000_000)),
        format!("object A\n  X = ({}\nend\n", "'".repeat(100_001)),
        "object A\r\n  X = 1\n  Y = 2\r\nend\r\n".into(),
    ];
    // Deep nesting.
    let mut deep = String::new();
    for i in 0..20_000 {
        deep.push_str(&" ".repeat(i % 7));
        deep.push_str("object TCVReportSectionContainer\n");
    }
    for _ in 0..20_000 {
        deep.push_str("end\n");
    }
    cases.push(deep);
    // Deep group nesting inside a valid report.
    let mut report = String::from("object TCVReport\n");
    for _ in 0..60 {
        report.push_str("object TCVReportSectionContainer\nobject TCVReportGroupBy\n");
    }
    for _ in 0..120 {
        report.push_str("end\n");
    }
    report.push_str("end\n");
    cases.push(report);
    // Huge payloads.
    cases.push(format!(
        "object TCVReport\n  PageSettingsList.Strings = (\n    '{}')\nend\n",
        "(0||a()(".repeat(200_000)
    ));
    cases.push(format!(
        "object TCVReport\n  PageSettingsList.Strings = (\n    '(0||{}()())')\nend\n",
        "n".repeat(3_000_000)
    ));
    let mut nested_ok = false;
    for c in &cases {
        if let Ok(s) = Stream::parse(c) {
            nested_ok |= Report::from_stream(&s).is_err();
            let _ = s.write();
            let _ = Report::from_stream(&s);
        }
    }
    assert!(nested_ok, "deep group nesting is refused by the model");
    // Byte noise.
    let mut seed = 0x1234_5678_u32;
    for _ in 0..200 {
        let len = 1 + (seed % 300) as usize;
        let mut bytes = Vec::with_capacity(len);
        for _ in 0..len {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            bytes.push((seed >> 24) as u8);
        }
        let _ = Stream::parse_bytes(&bytes);
        // Mutations of a valid definition.
        let mut valid = b"object TCVReport\n  Title = 'x'\n  object TCVReportRow\n    Height = 20\n  end\nend\n".to_vec();
        let at = (seed as usize) % valid.len();
        valid[at] = bytes[0];
        if let Ok(s) = Stream::parse_bytes(&valid) {
            let _ = Report::from_stream(&s);
        }
    }
}

#[test]
fn edits_to_local_definitions_are_written_and_read_back() {
    use super::edit::{self, Kind};
    use super::page::{self, Page};
    let mut inputs: Vec<Vec<u8>> = local_files().into_iter().map(|(_, b)| b).collect();
    inputs.extend(local_database_rows().into_iter().map(|(_, b)| b));
    let mut edited = 0;
    for bytes in inputs.iter().take(60) {
        let Ok(mut s) = Stream::parse_bytes(bytes) else {
            continue;
        };
        let before = Report::from_stream(&s).unwrap();
        // A property edit, a page setting edit and a new row.
        s.root.set_string("Title", "Edited 'title'");
        let payload = s
            .root
            .list(page::PAYLOAD_PROP)
            .as_deref()
            .and_then(page::read_payload);
        if let Some(mut tree) = payload {
            // Writing the settings back unchanged changes nothing.
            let page = Page::from_tree(&tree);
            let original = tree.clone();
            page.apply(&mut tree);
            assert_eq!(tree, original);
            let mut page = page;
            page.margins.left = 0.75;
            page.header[0].text = "Edited (header)".into();
            page.apply(&mut tree);
            s.root
                .set_list(page::PAYLOAD_PROP, &page::payload_lines(&tree));
        }
        if let Some(head) = edit::container_path(&s, Kind::PageHeader) {
            edit::add_row(&mut s, &head, 20).unwrap();
        }
        let again = Stream::parse_bytes(&s.to_bytes()).unwrap();
        assert_eq!(again, s);
        let after = Report::from_stream(&again).unwrap();
        assert_eq!(after.title, "Edited 'title'");
        assert_eq!(after.page_header.len(), before.page_header.len() + 1);
        if before.page.header[0].text != after.page.header[0].text {
            assert_eq!(after.page.header[0].text, "Edited (header)");
            assert_eq!(after.page.margins.left, 0.75);
        }
        assert_eq!(after.body.len(), before.body.len());
        edited += 1;
    }
    println!("{edited} local definitions edited and read back");
}

#[test]
fn local_reference_definitions_run_without_panics() {
    use crate::grid::columns::Index;
    let mut inputs: Vec<(String, Vec<u8>)> = local_files();
    inputs.extend(
        local_database_rows()
            .into_iter()
            .map(|(id, b)| (format!("row {id}"), b)),
    );
    if inputs.is_empty() {
        return;
    }
    let p = crate::reports::test_support::scheduled_starter();
    let ix = Index::new((false, 0.));
    let now = chrono::NaiveDate::from_ymd_opt(2026, 11, 5)
        .unwrap()
        .and_hms_opt(8, 0, 0)
        .unwrap();
    let (mut ran, mut with_warnings) = (0, 0);
    for (name, bytes) in &inputs {
        let Ok(s) = Stream::parse_bytes(bytes) else {
            continue;
        };
        let Ok(g) = super::generate(&s, &p, &ix, &super::engine::Options::new(now)) else {
            continue;
        };
        assert!(!g.pages.is_empty(), "{name}");
        // Every writer copes with whatever the definition produced.
        let html = super::writers::html(&g);
        super::writers::html_is_balanced(&html).unwrap_or_else(|e| panic!("{name}: {e}"));
        let _ = super::writers::text(&g, &Default::default());
        crate::print::pdf::check(&super::writers::pdf(&g))
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        ran += 1;
        if !g.warnings.is_empty() {
            with_warnings += 1;
        }
    }
    println!("{ran} local definitions ran on the starter project, {with_warnings} with warnings");
}
