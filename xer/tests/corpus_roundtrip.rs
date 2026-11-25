//! Round trips over the private XER corpus: preservation without scheduling,
//! and import, schedule, export, reimport, schedule with exact equality.
//!
//! The corpus holds client data and is never copied into the repository. Set
//! `OPENXER_CORPUS` to its folder. When it is not set or the folder is
//! missing the tests print the reason and skip.
use openxer_engine::scheduler;
use openxer_xer::{Document, ProjectData};
use std::{collections::BTreeMap, fs, path::PathBuf};

fn corpus() -> Option<Vec<PathBuf>> {
    let dir = std::env::var_os("OPENXER_CORPUS")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("OPENXER_CORPUS is not set"));
    let Ok(entries) = fs::read_dir(&dir) else {
        println!(
            "SKIPPED: XER corpus not found at {} (set OPENXER_CORPUS)",
            dir.display()
        );
        return None;
    };
    let mut files: Vec<_> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("xer")))
        .collect();
    files.sort();
    if files.is_empty() {
        println!("SKIPPED: no .xer files in {}", dir.display());
        return None;
    }
    Some(files)
}

/// Every difference between two documents, as (table, column, kind) counts.
fn differences(a: &Document, b: &Document) -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    let mut note = |key: String| *out.entry(key).or_insert(0) += 1;
    if a.header != b.header {
        note("header".into());
    }
    let names_a: Vec<_> = a.tables.iter().map(|t| t.name.as_str()).collect();
    let names_b: Vec<_> = b.tables.iter().map(|t| t.name.as_str()).collect();
    if names_a != names_b {
        note(format!("table order {names_a:?} -> {names_b:?}"));
    }
    for ta in &a.tables {
        let Some(tb) = b.table(&ta.name) else {
            note(format!("{} lost", ta.name));
            continue;
        };
        if ta.fields != tb.fields {
            note(format!(
                "{} fields {:?} -> {:?}",
                ta.name, ta.fields, tb.fields
            ));
            continue;
        }
        if ta.rows.len() != tb.rows.len() {
            note(format!(
                "{} rows {} -> {}",
                ta.name,
                ta.rows.len(),
                tb.rows.len()
            ));
            continue;
        }
        for (ra, rb) in ta.rows.iter().zip(&tb.rows) {
            for (i, field) in ta.fields.iter().enumerate() {
                if ra.get(i) != rb.get(i) {
                    note(format!("{}.{field}", ta.name));
                }
            }
        }
    }
    out
}

#[test]
fn unscheduled_export_preserves_every_byte() {
    let Some(files) = corpus() else { return };
    let Some(unreadable) = known_unreadable() else {
        return;
    };
    let mut failures = vec![];
    let mut checked = 0;
    for path in &files {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let original = fs::read(path).unwrap();
        let document = Document::from_bytes(&original).unwrap();
        for (pid, _) in document.projects() {
            let project = match import(&document, pid, &name, &unreadable) {
                Ok(Some(p)) => p,
                Ok(None) => continue,
                Err(e) => {
                    failures.push(e);
                    continue;
                }
            };
            checked += 1;
            let exported = project.updated_document();
            let diff = differences(&document, &exported);
            if !diff.is_empty() {
                failures.push(format!("{name} {pid}: {diff:?}"));
            }
            let bytes = exported.to_bytes().unwrap();
            // Files written without the %E terminator gain it on export.
            let mut expected = original.clone();
            if !expected.ends_with(b"%E\r\n") {
                expected.extend_from_slice(b"%E\r\n");
            }
            if bytes != expected {
                let at = bytes
                    .iter()
                    .zip(&expected)
                    .position(|(x, y)| x != y)
                    .unwrap_or(bytes.len().min(expected.len()));
                failures.push(format!(
                    "{name} {pid}: bytes differ at offset {at} (lengths {} vs {})",
                    bytes.len(),
                    expected.len()
                ));
            }
        }
    }
    println!("Checked unscheduled preservation of {checked} corpus projects");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The file writes a quote mark as two; the document holds one. Every corpus
/// file reads back to the bytes it was, and no quoted value is left doubled.
#[test]
fn corpus_quote_marks_are_single_in_the_document_and_doubled_in_the_file() {
    let Some(files) = corpus() else { return };
    let mut quoted = 0;
    for path in &files {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let original = fs::read(path).unwrap();
        let document = Document::from_bytes(&original).unwrap();
        assert_eq!(document.to_bytes().unwrap(), {
            let mut b = original.clone();
            if !b.ends_with(b"%E\r\n") {
                b.extend_from_slice(b"%E\r\n");
            }
            b
        });
        let text = if document.windows_encoding {
            encoding_rs::WINDOWS_1252.decode(&original).0.into_owned()
        } else {
            String::from_utf8_lossy(&original).into_owned()
        };
        let cells: std::collections::HashSet<&str> = text
            .lines()
            .filter(|l| l.starts_with("%R\t"))
            .flat_map(|l| l.split('\t'))
            .filter(|c| c.contains('"'))
            .collect();
        let four = cells.iter().any(|c| c.contains("\"\"\"\""));
        for table in &document.tables {
            for row in &table.rows {
                for value in row.iter().filter(|v| v.contains('"')) {
                    quoted += 1;
                    let file_form = openxer_xer::escape_quotes(value);
                    let stored = document.lone_quotes.get(value).map(String::as_str);
                    assert!(
                        cells.contains(stored.unwrap_or(&file_form)),
                        "{name} {}: {value:?} is not in the file in its file form",
                        table.name
                    );
                    assert!(
                        !value.contains("\"\"") || four,
                        "{name} {}: {value:?} still holds a doubled quote mark",
                        table.name
                    );
                }
            }
        }
    }
    println!("Checked {quoted} quoted corpus values");
}

fn schedule(project: &mut ProjectData) -> Result<(), String> {
    project.refresh_derived_durations();
    scheduler::schedule_with_resources(&mut project.model, &project.resources)
}

/// Corpus files that cannot be imported, with the reason, from the private
/// fixture `corpus-names.txt` (lines `unreadable <file> <reason>`): the file
/// names identify jobs, so they are not kept in the repository. Anything else
/// that fails to import is a test failure. Without the list the tests that
/// import every corpus file skip with a printed reason.
fn known_unreadable() -> Option<Vec<(String, String)>> {
    openxer_engine::fixture::corpus_names("unreadable")
}

fn import(
    document: &Document,
    pid: i64,
    name: &str,
    unreadable: &[(String, String)],
) -> Result<Option<ProjectData>, String> {
    match ProjectData::from_document(document.clone(), pid, name.into()) {
        Ok(p) => Ok(Some(p)),
        Err(e) => match unreadable.iter().find(|(file, _)| file == name) {
            Some((_, why)) => {
                println!("SKIPPED {name} {pid}: {e} ({why})");
                Ok(None)
            }
            None => Err(format!("{name} {pid}: import failed: {e}")),
        },
    }
}

/// Completed activities keep internal remaining dates after scheduling that the
/// XER deliberately leaves blank, and assignment
/// late dates are scheduler output that the importer does not read (TASKRSRC
/// is not written back). Every other field must survive exactly.
fn without_internal_dates(model: &openxer_engine::model::Model) -> openxer_engine::model::Model {
    let mut m = model.clone();
    for a in &mut m.activities {
        if a.is_completed() {
            a.rem_restart = a.restart;
            a.rem_reend = a.reend;
        }
        for r in &mut a.assignments {
            r.rem_late_start = None;
            r.rem_late_finish = None;
        }
    }
    m
}

#[test]
fn schedule_export_reimport_schedule_is_stable() {
    let Some(files) = corpus() else { return };
    let Some(unreadable) = known_unreadable() else {
        return;
    };
    let mut failures = vec![];
    let mut checked = 0;
    let mut rewritten: BTreeMap<String, usize> = BTreeMap::new();
    for path in &files {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let document = Document::read(path).unwrap();
        for (pid, _) in document.projects() {
            let mut first = match import(&document, pid, &name, &unreadable) {
                Ok(Some(p)) => p,
                Ok(None) => continue,
                Err(e) => {
                    failures.push(e);
                    continue;
                }
            };
            if let Err(e) = schedule(&mut first) {
                failures.push(format!("{name} {pid}: schedule failed: {e}"));
                continue;
            }
            checked += 1;
            let exported = first.updated_document();
            for (key, count) in differences(&document, &exported) {
                *rewritten.entry(key).or_default() += count;
            }
            let bytes = exported.to_bytes().unwrap();
            let reread = Document::from_bytes(&bytes).unwrap();
            let mut second = ProjectData::from_document(reread, pid, name.clone()).unwrap();
            let mut problems = vec![];
            if second.model != without_internal_dates(&first.model) {
                problems.push(format!(
                    "reimport differs: {}",
                    model_difference(&without_internal_dates(&first.model), &second.model)
                ));
            }
            if second.details != first.details {
                problems.push("details differ after reimport".into());
            }
            let wbs = |p: &ProjectData| {
                p.wbs
                    .iter()
                    .map(|w| (w.id, w.parent, w.code.clone(), w.name.clone(), w.sequence))
                    .collect::<Vec<_>>()
            };
            if wbs(&first) != wbs(&second) {
                problems.push("WBS differs after reimport".into());
            }
            // Scheduling the reimport must equal scheduling the in-memory project
            // again: the XER carries every input the scheduler reads.
            let mut again = first.clone();
            schedule(&mut again).unwrap();
            schedule(&mut second).unwrap();
            if without_internal_dates(&second.model) != without_internal_dates(&again.model) {
                problems.push(format!(
                    "reimport schedule differs from rescheduling in memory: {}",
                    model_difference(&again.model, &second.model)
                ));
            }
            // And F9 on the reimport equals the first F9 exactly.
            if second.model != first.model {
                problems.push(format!(
                    "second schedule differs: {}",
                    model_difference(&first.model, &second.model)
                ));
            }
            if !problems.is_empty() {
                failures.push(format!("{name} {pid}: {}", problems.join("; ")));
            }
        }
    }
    println!("Checked schedule round trips of {checked} corpus projects");
    println!("Cells rewritten by a scheduled export, over the whole corpus:");
    for (key, count) in &rewritten {
        println!("  {key}: {count}");
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn model_difference(x: &openxer_engine::model::Model, y: &openxer_engine::model::Model) -> String {
    let mut out = vec![];
    if x.origin_serial != y.origin_serial {
        out.push(format!("origin {} vs {}", x.origin_serial, y.origin_serial));
    }
    if x.project != y.project {
        out.push(format!("project {:?} vs {:?}", x.project, y.project));
    }
    if x.options != y.options {
        out.push("options".into());
    }
    if x.links != y.links {
        out.push(format!("links {} vs {}", x.links.len(), y.links.len()));
    }
    if x.calendars != y.calendars {
        out.push("calendars".into());
    }
    let mut fields: BTreeMap<&str, (usize, String)> = BTreeMap::new();
    for (p, q) in x.activities.iter().zip(&y.activities) {
        for (name, differs) in activity_fields(p, q) {
            if differs {
                let e = fields
                    .entry(name)
                    .or_insert((0, format!("{} {:?}", p.task_code, p.status)));
                e.0 += 1;
            }
        }
    }
    for (name, (count, example)) in fields {
        out.push(format!("{name} x{count} (e.g. {example})"));
    }
    out.join(", ")
}

fn activity_fields(
    p: &openxer_engine::model::Activity,
    q: &openxer_engine::model::Activity,
) -> Vec<(&'static str, bool)> {
    macro_rules! fields {
        ($($f:ident),*) => { vec![$((stringify!($f), p.$f != q.$f)),*] };
    }
    fields!(
        task_id,
        task_code,
        task_type,
        status,
        act_start,
        act_finish,
        resume,
        expect_end,
        cstr,
        cstr_date,
        cstr2,
        cstr_date2,
        rem,
        cal,
        ext_early_start,
        ext_late_end,
        restart,
        reend,
        rem_restart,
        rem_reend,
        early_start,
        early_finish,
        late_start,
        late_finish,
        rem_late_start,
        rem_late_finish,
        total_float,
        free_float,
        driving_path,
        preds,
        succs,
        assignments,
        target_start,
        target_finish,
        float_path,
        float_path_order
    )
}

/// The table order and the column lists OpenXER uses for tables it creates
/// agree with what the reference wrote in every corpus file.
#[test]
fn table_order_and_columns_match_reference_exports() {
    let Some(files) = corpus() else { return };
    let rank = |n: &str| openxer_xer::TABLE_ORDER.iter().position(|t| *t == n);
    let mut failures = vec![];
    let mut checked = 0;
    for path in &files {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let document = Document::read(path).unwrap();
        let ranks: Vec<_> = document
            .tables
            .iter()
            .map(|t| (t.name.as_str(), rank(&t.name)))
            .collect();
        for (table, r) in &ranks {
            if r.is_none() {
                failures.push(format!("{name}: table {table} is missing from TABLE_ORDER"));
            }
        }
        // Two corpus files were written by a generator, not by the reference; it puts
        // CALENDAR before PROJECT. Their order is not evidence of the reference's.
        let generated = document.header.contains("Claude.Code");
        if !generated && ranks.windows(2).any(|w| w[0].1 > w[1].1) {
            failures.push(format!(
                "{name}: table order {ranks:?} disagrees with TABLE_ORDER"
            ));
        }
        // version 25.12 column lists; older versions differ only by added columns.
        if document.header.split('\t').nth(1) == Some("25.12") {
            for t in &document.tables {
                if let Some(fields) = openxer_xer::standard_fields(&t.name) {
                    checked += 1;
                    if t.fields != fields {
                        failures.push(format!(
                            "{name}: {} columns differ from standard_fields",
                            t.name
                        ));
                    }
                }
            }
        }
        if document.header.split('\t').count() != 9 {
            failures.push(format!("{name}: header does not have 9 fields"));
        }
    }
    println!("Compared {checked} version 25.12 column lists");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// A table no reference version writes, appended to every corpus file so the round
/// trip below can prove that unknown tables pass through byte-identical.
const PROBE: &[u8] = b"%T\tZZPROBE\r\n%F\tprobe_id\tprobe_text\tprobe_blank\r\n%R\t1\tkeep \"quotes\", <b>html</b> & \xe9\t\r\n%R\t2\t\t\r\n";

/// Each table's bytes (from its `%T` line up to the next table or `%E`), by
/// name in file order.
fn sections(bytes: &[u8]) -> Vec<(String, &[u8])> {
    // Every file starts with its header line, so each table follows a line feed.
    let mut starts: Vec<usize> = bytes
        .windows(4)
        .enumerate()
        .filter(|(_, w)| *w == b"\n%T\t")
        .map(|(i, _)| i + 1)
        .collect();
    let end = bytes
        .windows(4)
        .rposition(|w| w == b"\n%E\r" || w == b"\n%E\n")
        .map_or(bytes.len(), |i| i + 1);
    starts.push(end);
    starts
        .windows(2)
        .map(|w| {
            let section = &bytes[w[0]..w[1]];
            let name_end = section
                .iter()
                .position(|&b| b == b'\r' || b == b'\n')
                .unwrap_or(section.len());
            (
                String::from_utf8_lossy(&section[3..name_end]).to_string(),
                section,
            )
        })
        .collect()
}

/// Corpus round trip at scale, as the desktop does it: import each project,
/// save it to the workspace (the workspace stores `updated_document().text()`
/// and the source encoding, see `desktop/src/store.rs`), reload it from that
/// text, export the file and reimport the export. Every table must come back
/// with the same columns and rows, the project with the same model, and every
/// table's bytes, including an unknown table, must be identical. The only
/// normalization is the documented one: a file without `%E` gains it.
#[test]
fn workspace_save_reload_export_reimport_keeps_every_table() {
    let Some(files) = corpus() else { return };
    let Some(unreadable) = known_unreadable() else {
        return;
    };
    let mut failures = vec![];
    let mut checked = 0;
    let mut tables_checked = 0;
    for path in &files {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let mut original = fs::read(path).unwrap();
        if !original.ends_with(b"%E\r\n") {
            original.extend_from_slice(b"%E\r\n");
        }
        let at = original.len() - 4;
        original.splice(at..at, PROBE.iter().copied());
        let document = Document::from_bytes(&original).unwrap();
        let expected_sections = sections(&original);
        for (pid, _) in document.projects() {
            let first = match import(&document, pid, &name, &unreadable) {
                Ok(Some(p)) => p,
                Ok(None) => continue,
                Err(e) => {
                    failures.push(e);
                    continue;
                }
            };
            checked += 1;
            // Save to the workspace and reload.
            let saved = first.updated_document().text();
            let mut reloaded = match Document::parse(&saved) {
                Ok(d) => d,
                Err(e) => {
                    failures.push(format!("{name} {pid}: workspace text unreadable: {e}"));
                    continue;
                }
            };
            reloaded.windows_encoding = first.document.windows_encoding;
            let second = match ProjectData::from_document(reloaded, pid, name.clone()) {
                Ok(p) => p,
                Err(e) => {
                    failures.push(format!("{name} {pid}: reload failed: {e}"));
                    continue;
                }
            };
            // Export and reimport.
            let bytes = match second.updated_document().to_bytes() {
                Ok(b) => b,
                Err(e) => {
                    failures.push(format!("{name} {pid}: export failed: {e}"));
                    continue;
                }
            };
            let reread = Document::from_bytes(&bytes).unwrap();
            let third = match ProjectData::from_document(reread.clone(), pid, name.clone()) {
                Ok(p) => p,
                Err(e) => {
                    failures.push(format!("{name} {pid}: reimport failed: {e}"));
                    continue;
                }
            };
            let mut problems = vec![];
            let diff = differences(&document, &reread);
            if !diff.is_empty() {
                problems.push(format!("tables differ: {diff:?}"));
            }
            let diff = differences(&document, &third.updated_document());
            if !diff.is_empty() {
                problems.push(format!("re-export differs: {diff:?}"));
            }
            if third.model != first.model {
                problems.push(format!(
                    "model differs: {}",
                    model_difference(&first.model, &third.model)
                ));
            }
            if third.details != first.details {
                problems.push("details differ".into());
            }
            let got = sections(&bytes);
            let names = |s: &[(String, &[u8])]| s.iter().map(|t| t.0.clone()).collect::<Vec<_>>();
            if names(&got) != names(&expected_sections) {
                problems.push(format!(
                    "table list {:?} -> {:?}",
                    names(&expected_sections),
                    names(&got)
                ));
            }
            for ((table, want), (_, have)) in expected_sections.iter().zip(&got) {
                tables_checked += 1;
                if want != have {
                    problems.push(format!("{table} bytes differ"));
                }
            }
            if !got.iter().any(|(t, s)| t == "ZZPROBE" && *s == PROBE) {
                problems.push("unknown table ZZPROBE did not pass through".into());
            }
            if !problems.is_empty() {
                failures.push(format!("{name} {pid}: {}", problems.join("; ")));
            }
        }
    }
    println!(
        "Workspace round trip of {checked} corpus projects, {tables_checked} tables compared byte for byte"
    );
    assert!(checked > 0 || files.is_empty());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
