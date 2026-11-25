//! Fresh F9 against the activity results the reference stored in each corpus XER.
//!
//! For every project in the private corpus this imports the file, works out
//! from the file itself whether the stored results are a clean reference schedule of
//! the stored inputs, schedules it the way that evidence says the reference did (leveled
//! or plain CPM, with the project's own options), writes the results back and
//! compares every activity's dates, floats and driving flag with the reference's cells.
//!
//! Run with `cargo test -p openxer-xer --release --test corpus_parity -- --nocapture`.
//! Set `OPENXER_PARITY_OUT` to a file to also get every mismatch as TSV.
//! `engine/PARITY.md` summarizes the results.
use openxer_engine::scheduler;
use openxer_xer::{parse_absolute, Document, ProjectData, Table};
use std::{collections::BTreeMap, fmt::Write as _, fs, path::PathBuf};

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
    Some(files)
}

/// TASK columns compared, with a short label. Dates compare as minutes,
/// floats as whole minutes, the flag as text.
const FIELDS: [(&str, &str); 11] = [
    ("es", "early_start_date"),
    ("ef", "early_end_date"),
    ("ls", "late_start_date"),
    ("lf", "late_end_date"),
    ("rs", "restart_date"),
    ("rf", "reend_date"),
    ("rls", "rem_late_start_date"),
    ("rlf", "rem_late_end_date"),
    ("tf", "total_float_hr_cnt"),
    ("ff", "free_float_hr_cnt"),
    ("drive", "driving_path_flag"),
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Class {
    /// Stored results are the reference's schedule of the stored inputs, no leveling.
    Clean,
    /// Stored remaining dates come from resource leveling.
    Leveled,
    /// Activities were updated after the last schedule; the stored results
    /// are still consistent, but the edit may have changed an input.
    Edited,
    /// An input changed after the last schedule: an activity was completed
    /// after it (every schedule writes dates on completed activities).
    Stale,
    /// Not written by a reference schedule (generated file).
    Generated,
}

struct Evidence {
    class: Class,
    notes: Vec<String>,
}

fn minutes(column: &str, text: &str) -> Option<i64> {
    if column.ends_with("_date") {
        parse_absolute(text)
    } else if text.trim().is_empty() {
        None
    } else {
        text.trim()
            .parse::<f64>()
            .ok()
            .map(|h| (h * 60.0).round() as i64)
    }
}

fn same(column: &str, a: &str, b: &str) -> bool {
    if column == "driving_path_flag" {
        return a.trim() == b.trim();
    }
    minutes(column, a) == minutes(column, b)
}

fn project_tasks(t: &Table, pid: i64) -> Vec<&Vec<String>> {
    t.rows
        .iter()
        .filter(|r| t.value(r, "proj_id") == pid.to_string())
        .collect()
}

/// What the file itself says about its stored results.
fn evidence(doc: &Document, pid: i64) -> Evidence {
    let mut notes = vec![];
    let header_user = doc
        .header
        .split('\t')
        .nth(4)
        .unwrap_or("")
        .trim()
        .to_string();
    let project = doc.table("PROJECT").and_then(|t| {
        t.rows
            .iter()
            .find(|r| t.value(r, "proj_id") == pid.to_string())
            .map(|r| (t, r))
    });
    let (last_schedule, data_date) = project.map_or((String::new(), String::new()), |(t, r)| {
        (
            t.value(r, "last_schedule_date").to_string(),
            t.value(r, "last_recalc_date").to_string(),
        )
    });
    notes.push(format!("data date {data_date}"));
    if !last_schedule.is_empty() {
        notes.push(format!("last schedule {last_schedule}"));
    }
    if header_user.eq_ignore_ascii_case("claude.code") {
        notes.push("written by a generator, not by the reference".into());
        return Evidence {
            class: Class::Generated,
            notes,
        };
    }
    let task = doc.table("TASK").unwrap();
    let rows = project_tasks(task, pid);
    // Unleveled CPM always stores a not-started activity's remaining dates
    // equal to its early dates; leveling with "preserve scheduled early and
    // late dates" moves only the remaining dates.
    let moved = rows
        .iter()
        .filter(|r| {
            task.value(r, "status_code") == "TK_NotStart"
                && (task.value(r, "restart_date") != task.value(r, "early_start_date")
                    || task.value(r, "reend_date") != task.value(r, "early_end_date"))
        })
        .count();
    let mut edited = if last_schedule.is_empty() {
        0
    } else {
        rows.iter()
            .filter(|r| task.value(r, "update_date") > last_schedule.as_str())
            .count()
    };
    if edited > 0 && edited == rows.len() {
        // A schedule rewrites activity rows, so a "last schedule" before
        // every update is not the last schedule (369.xer).
        notes.push("every activity updated after the stored last schedule date".into());
        edited = 0;
    } else if edited > 0 {
        notes.push(format!(
            "{edited} activities updated after the last schedule"
        ));
    }
    let completed_blank: Vec<_> = rows
        .iter()
        .filter(|r| {
            task.value(r, "status_code") == "TK_Complete"
                && task.value(r, "early_start_date").is_empty()
        })
        .map(|r| task.value(r, "task_code").to_string())
        .collect();
    if !completed_blank.is_empty() {
        notes.push(format!(
            "completed after the last schedule (no stored dates): {}",
            completed_blank.join(", ")
        ));
    }
    let class = if moved > 0 {
        notes.push(format!(
            "{moved} not-started activities have remaining dates off their early dates"
        ));
        Class::Leveled
    } else if !completed_blank.is_empty() {
        Class::Stale
    } else if edited > 0 {
        Class::Edited
    } else {
        Class::Clean
    };
    Evidence { class, notes }
}

#[derive(Default)]
struct Counts {
    per_field: BTreeMap<&'static str, (usize, usize)>,
}
impl Counts {
    fn total(&self) -> (usize, usize) {
        self.per_field
            .values()
            .fold((0, 0), |(m, t), (a, b)| (m + a, t + b))
    }
}

/// One fresh F9 of a project and its comparison with the stored cells:
/// per-field counts and the mismatching cells as TSV rows.
fn compare(
    document: &Document,
    pid: i64,
    name: &str,
    level: bool,
    finish_on_valid_time: bool,
) -> Result<(Counts, Vec<String>), String> {
    let mut project = ProjectData::from_document(document.clone(), pid, name.to_string())?;
    project.model.options.do_level = level;
    project.model.options.finish_on_valid_time = finish_on_valid_time;
    project.refresh_derived_durations();
    scheduler::schedule_with_resources(&mut project.model, &project.resources)?;
    let written = project.updated_document();
    let (a, b) = (
        document.table("TASK").unwrap(),
        written.table("TASK").unwrap(),
    );
    let mut counts = Counts::default();
    let mut rows = vec![];
    let stored = project_tasks(a, pid);
    let fresh = project_tasks(b, pid);
    assert_eq!(stored.len(), fresh.len());
    for (ra, rb) in stored.iter().zip(&fresh) {
        for (label, column) in FIELDS {
            let (x, y) = (a.value(ra, column), b.value(rb, column));
            let c = counts.per_field.entry(label).or_default();
            c.1 += 1;
            if same(column, x, y) {
                c.0 += 1;
            } else {
                rows.push(format!(
                    "{name}\t{}\t{}\t{}\t{label}\t{x}\t{y}",
                    u8::from(finish_on_valid_time),
                    a.value(ra, "task_code"),
                    a.value(ra, "status_code"),
                ));
            }
        }
    }
    Ok((counts, rows))
}

fn summary(counts: &Counts) -> String {
    let (m, t) = counts.total();
    let fields: Vec<_> = counts
        .per_field
        .iter()
        .filter(|(_, (a, b))| a != b)
        .map(|(k, (a, b))| format!("{k} {a}/{b}"))
        .collect();
    if fields.is_empty() {
        format!("{m}/{t}")
    } else {
        format!("{m}/{t} ({})", fields.join(", "))
    }
}

fn add(target: &mut Counts, counts: &Counts) {
    for (k, (a, b)) in &counts.per_field {
        let e = target.per_field.entry(k).or_default();
        e.0 += a;
        e.1 += b;
    }
}

/// Prints, per project, the evidence class and the matching cells with the
/// default administrator finish setting and with it on (XER does not carry
/// that database-wide setting).
#[test]
fn corpus_fresh_f9_matches_stored_reference_results() {
    let Some(files) = corpus() else {
        return;
    };
    let mut report = String::new();
    let mut tsv =
        String::from("file\tfinish_on_valid_time\tactivity\tstatus\tfield\tstored\topenxer\n");
    let mut totals: BTreeMap<String, Counts> = BTreeMap::new();
    let mut unexplained = vec![];
    for path in files {
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let Ok(document) = Document::read(&path) else {
            writeln!(report, "{name}: unreadable").unwrap();
            continue;
        };
        for (pid, _) in document.projects() {
            let ev = evidence(&document, pid);
            let level = ev.class == Class::Leveled;
            let mut cells = vec![];
            for fovt in [false, true] {
                match compare(&document, pid, &name, level, fovt) {
                    Ok((counts, rows)) => {
                        // With the default administrator setting, every
                        // clean, edited and leveled project matches the reference in
                        // every compared cell (engine/PARITY.md).
                        if !fovt
                            && matches!(ev.class, Class::Clean | Class::Edited | Class::Leveled)
                        {
                            unexplained.extend(rows.iter().take(5).cloned());
                        }
                        cells.push(summary(&counts));
                        tsv.extend(rows.into_iter().map(|r| r + "\n"));
                        add(
                            totals
                                .entry(format!("{:?} finish_on_valid_time={fovt}", ev.class))
                                .or_default(),
                            &counts,
                        );
                    }
                    Err(e) => cells.push(format!("rejected: {e}")),
                }
            }
            writeln!(
                report,
                "{name} {pid}: {:?}{} [{}]\n    default: {}\n    finish on valid time: {}",
                ev.class,
                if level { ", leveled F9" } else { "" },
                ev.notes.join("; "),
                cells[0],
                cells[1],
            )
            .unwrap();
        }
    }
    for (k, counts) in &totals {
        writeln!(report, "{k}: {}", summary(counts)).unwrap();
    }
    println!("{report}");
    if let Some(out) = std::env::var_os("OPENXER_PARITY_OUT") {
        fs::write(out, tsv).unwrap();
    }
    assert!(unexplained.is_empty(), "{}", unexplained.join("\n"));
}

/// An open Level of Effort stores the span of its remaining dates as its
/// remaining duration (not part of the eleven cells above, which every
/// activity shares). Counts the open LOEs whose stored duration a fresh F9
/// reproduces; a project the reference leveled is listed but not asserted.
#[test]
fn corpus_open_loe_remaining_duration_matches_stored_reference_results() {
    let Some(files) = corpus() else {
        return;
    };
    let mut report = String::new();
    let mut wrong = vec![];
    for path in files {
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let Ok(document) = Document::read(&path) else {
            continue;
        };
        for (pid, _) in document.projects() {
            let ev = evidence(&document, pid);
            let level = ev.class == Class::Leveled;
            let Ok(mut project) = ProjectData::from_document(document.clone(), pid, name.clone())
            else {
                continue;
            };
            project.model.options.do_level = level;
            project.refresh_derived_durations();
            if scheduler::schedule_with_resources(&mut project.model, &project.resources).is_err() {
                continue;
            }
            let written = project.updated_document();
            let (a, b) = (
                document.table("TASK").unwrap(),
                written.table("TASK").unwrap(),
            );
            let (mut total, mut matching) = (0, 0);
            for (ra, rb) in project_tasks(a, pid).into_iter().zip(project_tasks(b, pid)) {
                if a.value(ra, "task_type") != "TT_LOE"
                    || a.value(ra, "status_code") == "TK_Complete"
                {
                    continue;
                }
                total += 1;
                let (x, y) = (
                    a.value(ra, "remain_drtn_hr_cnt"),
                    b.value(rb, "remain_drtn_hr_cnt"),
                );
                if same("remain_drtn_hr_cnt", x, y) {
                    matching += 1;
                } else if matches!(ev.class, Class::Clean | Class::Edited) {
                    wrong.push(format!(
                        "{name} {}: stored {x}, fresh F9 {y}",
                        a.value(ra, "task_code")
                    ));
                }
            }
            if total > 0 {
                writeln!(
                    report,
                    "{name} {pid} ({:?}): open LOE remaining duration {matching}/{total}",
                    ev.class
                )
                .unwrap();
            }
        }
    }
    println!("{report}");
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
