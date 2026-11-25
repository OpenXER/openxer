//! Local-only like-for-like check of a real project against the reference
//! scheduler's own recalculation (Level of Effort remaining durations, late
//! dates and float). Ignored by default and skipped, with the
//! reason printed, when the private inputs are absent.
//!
//! Inputs, all outside the repository, named by environment variables:
//! - `OPENXER_REF_XER`: the project as exported, with the edits the
//!   reference had made before it rescheduled.
//! - `OPENXER_REF_JSON`: the reference's TASK rows after its schedule.
//! - `OPENXER_REF_ORIGINAL`: the project without the edits, for the second
//!   test, which compares a plain F9 with the cells stored in that file.
//!
//! Run with
//! `cargo test -p openxer-automation --release --test local_reference -- --ignored --nocapture`.
use openxer_engine::scheduler;
use openxer_xer::{parse_absolute, Document, ProjectData};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

/// TASK columns compared: dates as minutes, durations and floats as hours.
const FIELDS: [&str; 17] = [
    "early_start_date",
    "early_end_date",
    "late_start_date",
    "late_end_date",
    "restart_date",
    "reend_date",
    "rem_late_start_date",
    "rem_late_end_date",
    "target_start_date",
    "target_end_date",
    "act_start_date",
    "act_end_date",
    "total_float_hr_cnt",
    "free_float_hr_cnt",
    "remain_drtn_hr_cnt",
    "target_drtn_hr_cnt",
    "status_code",
];

fn minutes(column: &str, text: &str) -> Option<i64> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    if column.ends_with("_date") {
        parse_absolute(&text[..text.len().min(16)])
    } else {
        text.parse::<f64>().ok().map(|h| (h * 60.0).round() as i64)
    }
}

fn same(column: &str, ours: &str, theirs: &str) -> bool {
    if column == "status_code" {
        return ours == theirs;
    }
    minutes(column, ours) == minutes(column, theirs)
}

fn text(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn input(variable: &str) -> PathBuf {
    std::env::var_os(variable).map_or_else(
        || PathBuf::from(format!("<{variable} not set>")),
        PathBuf::from,
    )
}

fn present(files: &[&PathBuf]) -> bool {
    for file in files {
        if !file.exists() {
            println!(
                "SKIPPED: {} not found (set OPENXER_REF_XER, OPENXER_REF_JSON, OPENXER_REF_ORIGINAL)",
                file.display()
            );
            return false;
        }
    }
    true
}

/// A fresh F9 of the file's first project the way the application runs it,
/// written back to a document.
fn f9(path: &Path) -> Document {
    let document = Document::read(path).expect("XER reads");
    let (pid, name) = document.projects().remove(0);
    let mut project = ProjectData::from_document(document, pid, name).expect("project loads");
    project.refresh_derived_durations();
    scheduler::schedule_with_resources(&mut project.model, &project.resources).expect("F9");
    project.write_assignment_schedule();
    project.updated_document()
}

/// Compares every field of every activity found on both sides; returns the
/// number compared and the differing cells per field.
fn compare(
    ours: &Document,
    theirs: &dyn Fn(&str, &str) -> Option<String>,
) -> (usize, BTreeMap<&'static str, Vec<String>>) {
    let table = ours.table("TASK").expect("TASK table");
    let mut compared = 0;
    let mut per_field: BTreeMap<&'static str, Vec<String>> = BTreeMap::new();
    for row in &table.rows {
        let code = table.value(row, "task_code");
        if theirs(code, "status_code").is_none() {
            continue;
        }
        compared += 1;
        for field in FIELDS {
            let a = table.value(row, field);
            let b = theirs(code, field).unwrap_or_default();
            if !same(field, a, &b) {
                per_field
                    .entry(field)
                    .or_default()
                    .push(format!("{code}: {a} vs {b}"));
            }
        }
    }
    println!("{compared} activities compared");
    for (field, rows) in &per_field {
        println!(
            "  {field}: {} differ, e.g. {:?}",
            rows.len(),
            &rows[..rows.len().min(3)]
        );
    }
    (compared, per_field)
}

#[test]
#[ignore = "needs the private project files; run with --ignored"]
fn like_for_like_f9_matches_the_reference_reschedule() {
    let xer = input("OPENXER_REF_XER");
    let json = input("OPENXER_REF_JSON");
    if !present(&[&xer, &json]) {
        return;
    }
    let written = f9(&xer);
    let reference: Value =
        serde_json::from_slice(&std::fs::read(&json).expect("json reads")).expect("json parses");
    let rows: BTreeMap<String, &Value> = reference["TASK"]
        .as_array()
        .expect("TASK rows")
        .iter()
        .map(|r| (text(&r["TASK_CODE"]), r))
        .collect();
    let (compared, per_field) = compare(&written, &|code, field| {
        rows.get(code).map(|r| text(&r[field.to_ascii_uppercase()]))
    });
    assert!(compared > 4000, "too few activities matched: {compared}");
    assert!(per_field.is_empty(), "cells differ from the reference");
}

/// Without the edit, a plain F9 is compared with the cells the file already
/// stores. Those were written before some later edits to the project, so a
/// few activities legitimately differ; every open LOE must match.
#[test]
#[ignore = "needs the private project files; run with --ignored"]
fn plain_f9_of_the_unedited_file_keeps_the_stored_loe_cells() {
    let xer = input("OPENXER_REF_ORIGINAL");
    if !present(&[&xer]) {
        return;
    }
    let stored = Document::read(&xer).expect("XER reads");
    let table = stored.table("TASK").expect("TASK table");
    let rows: BTreeMap<String, Vec<String>> = table
        .rows
        .iter()
        .map(|r| {
            let cells = FIELDS
                .iter()
                .map(|f| table.value(r, f).to_string())
                .collect();
            (table.value(r, "task_code").to_string(), cells)
        })
        .collect();
    let written = f9(&xer);
    let (compared, per_field) = compare(&written, &|code, field| {
        let at = FIELDS.iter().position(|f| *f == field)?;
        rows.get(code).map(|cells| cells[at].clone())
    });
    assert!(compared > 4000, "too few activities matched: {compared}");
    let loe_codes: Vec<&str> = table
        .rows
        .iter()
        .filter(|r| table.value(r, "task_type") == "TT_LOE")
        .map(|r| table.value(r, "task_code"))
        .collect();
    let loe: Vec<_> = per_field
        .values()
        .flatten()
        .filter(|c| {
            c.split(':')
                .next()
                .is_some_and(|code| loe_codes.contains(&code))
        })
        .collect();
    assert!(loe.is_empty(), "LOE cells differ: {loe:?}");
}
