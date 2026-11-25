//! XER export: one or several workspace projects into one file, with the version
//! label written to the header. A single project goes through the project's own
//! `updated_document`, so every table and column OpenXER does not own is kept.
use crate::util::{num, KeySpec};
use openxer_xer::{Document, ProjectData, Table};
use std::collections::{HashMap, HashSet};

/// Version labels offered in the export wizard (newest first).
pub const XER_VERSIONS: [&str; 9] = [
    "25.12", "24.12", "23.12", "22.12", "21.12", "20.12", "19.12", "18.8", "8.4",
];

/// Project tables whose row ids nothing else refers to. Two projects of one
/// file that each added such a row (a relationship, a notebook entry, a step,
/// a document link) took the same next id; the second is renumbered.
const RENUMBERED: [&str; 5] = ["TASKPRED", "TASKMEMO", "WBSMEMO", "TASKPROC", "TASKDOC"];

/// Replace the version and export-date fields of an `ERMHDR` line.
pub fn with_version(header: &str, version: &str, date: &str) -> String {
    let mut parts: Vec<String> = header.split('\t').map(String::from).collect();
    while parts.len() < 3 {
        parts.push(String::new());
    }
    // A label typed by the user must not split the header line.
    let clean = |v: &str| -> String {
        v.chars()
            .filter(|c| !c.is_control())
            .collect::<String>()
            .trim()
            .to_string()
    };
    let (version, date) = (clean(version), clean(date));
    parts[0] = "ERMHDR".into();
    if !version.trim().is_empty() {
        parts[1] = version.trim().into();
    }
    if !date.trim().is_empty() {
        parts[2] = date.trim().into();
    }
    parts.join("\t")
}

pub fn header_version(header: &str) -> String {
    header.split('\t').nth(1).unwrap_or_default().to_string()
}

fn project_of(table: &Table, row: &[String]) -> Option<i64> {
    let p = table.value(row, "proj_id").trim();
    (!p.is_empty()).then(|| num(p))
}

/// Combine projects into one document. The first project's document is the base;
/// later projects add their own rows and any dictionary rows the base lacks.
/// Rows the base holds for a later project are stale copies (a multi-project
/// file is stored whole with each of its projects) and are replaced by that
/// project's own rows. Cross-project relationships belong to the successor's
/// project, so each is written once, from its owner.
pub fn combine(projects: &[&ProjectData]) -> Result<Document, String> {
    let first = projects.first().ok_or("Choose at least one project")?;
    let mut out = first.updated_document();
    let mut included: HashSet<i64> = HashSet::from([first.model.project.proj_id]);
    // Projects in the base document that were not selected are removed.
    let selected: HashSet<i64> = projects.iter().map(|p| p.model.project.proj_id).collect();
    if selected.len() != projects.len() {
        return Err("Two selected projects share a project ID; export them separately".into());
    }
    for p in &projects[1..] {
        let doc = p.updated_document();
        out.keep_lone_quotes_of(&doc);
        let pid = p.model.project.proj_id;
        for t in &mut out.tables {
            if let Some(col) = t.field("proj_id") {
                t.rows.retain(|r| {
                    r.get(col)
                        .is_none_or(|v| v.trim().is_empty() || num(v) != pid)
                });
            }
        }
        for t in &doc.tables {
            let has_project = t.field("proj_id").is_some();
            let upper = t.name.to_ascii_uppercase();
            // Rows of a project are told apart by the table's real key (a
            // composite one where the first column is shared, such as a UDF
            // type) or, for a table OpenXER does not know, by their whole
            // content; rows of no project (dictionaries) by their id.
            let owned_key = KeySpec::of(&t.name, None);
            let dictionary_key = KeySpec::of(&t.name, t.fields.first().map(String::as_str));
            let target = out.table_mut(&t.name);
            if target.fields.is_empty() {
                target.fields = t.fields.clone();
            }
            let (target_owned, target_dictionary) =
                (owned_key.reader(target), dictionary_key.reader(target));
            let existing: HashSet<Vec<String>> =
                target.rows.iter().map(|r| target_owned.value(r)).collect();
            let mut dictionary: HashMap<Vec<String>, usize> = HashMap::new();
            for (i, r) in target.rows.iter().enumerate() {
                dictionary.entry(target_dictionary.value(r)).or_insert(i);
            }
            let (source_owned, source_dictionary) = (owned_key.reader(t), dictionary_key.reader(t));
            let renumber = match &owned_key {
                KeySpec::Id(c) if RENUMBERED.contains(&upper.as_str()) => {
                    t.field(c).map(|i| (i, c.as_str()))
                }
                _ => None,
            };
            let largest = |tab: &Table, name: &str| {
                tab.field(name).map_or(0, |k| {
                    tab.rows
                        .iter()
                        .map(|r| r.get(k).map_or(0, |v| num(v)))
                        .max()
                        .unwrap_or(0)
                })
            };
            let mut next_id = renumber.map_or(0, |(_, name)| {
                largest(target, name).max(largest(t, name)) + 1
            });
            let rows: Vec<&Vec<String>> = t
                .rows
                .iter()
                .filter(|r| match project_of(t, r) {
                    Some(owner) if has_project => owner == pid,
                    _ => true,
                })
                .collect();
            for r in rows {
                let owned = has_project && project_of(t, r) == Some(pid);
                let mut renumbered = None;
                if owned {
                    if existing.contains(&source_owned.value(r)) {
                        if let Some((c, _)) = renumber {
                            renumbered = Some((c, next_id.to_string()));
                            next_id += 1;
                        } else if source_owned.is_whole() {
                            // The same row is already there.
                            continue;
                        } else {
                            return Err(format!(
                                "Projects {} and {} come from different files and share {} ids; export them separately",
                                first.model.project.name, p.model.project.name, t.name
                            ));
                        }
                    }
                } else if let Some(&i) = dictionary.get(&source_dictionary.value(r)) {
                    // Dictionary row already present. A calendar of the same
                    // id with other work times would move this project's
                    // dates, so that is refused.
                    if upper == "CALENDAR" {
                        let data = |tab: &Table, row: &[String]| {
                            tab.value(row, "clndr_data").trim().to_string()
                        };
                        let (mine, theirs) = (data(t, r), data(target, &target.rows[i]));
                        if !mine.is_empty() && !theirs.is_empty() && mine != theirs {
                            return Err(format!(
                                "Projects {} and {} hold different versions of calendar {} ({}); one would change the other's dates. Export them separately",
                                first.model.project.name,
                                p.model.project.name,
                                t.value(r, "clndr_id").trim(),
                                t.value(r, "clndr_name").trim()
                            ));
                        }
                    }
                    continue;
                }
                let mut values = vec![String::new(); target.fields.len()];
                for (k, field) in t.fields.iter().enumerate() {
                    let mut v = r.get(k).cloned().unwrap_or_default();
                    if let Some((c, n)) = &renumbered {
                        if k == *c {
                            v = n.clone();
                        }
                    }
                    match target.field(field) {
                        Some(c) => values[c] = v,
                        None if !v.is_empty() => {
                            target.fields.push(field.clone());
                            for row in &mut target.rows {
                                row.push(String::new());
                            }
                            values.push(v);
                        }
                        None => {}
                    }
                }
                target.rows.push(values);
            }
        }
        included.insert(pid);
    }
    // Drop rows of projects that live in the base document but were not chosen.
    let others: HashSet<i64> = out
        .projects()
        .into_iter()
        .map(|(id, _)| id)
        .filter(|id| !included.contains(id))
        .collect();
    if !others.is_empty() {
        for t in &mut out.tables {
            if t.field("proj_id").is_none() {
                continue;
            }
            let col = t.field("proj_id").expect("checked");
            let pred_col = t.field("pred_proj_id");
            t.rows.retain(|r| {
                let owner = r.get(col).map(|v| v.trim()).unwrap_or("");
                let pred = pred_col
                    .and_then(|c| r.get(c))
                    .map(|v| v.trim())
                    .unwrap_or("");
                let foreign = |v: &str| !v.is_empty() && others.contains(&num(v));
                // A relationship stays when either end is exported (an external tie).
                !(foreign(owner) && (pred.is_empty() || foreign(pred)))
            });
        }
    }
    Ok(out)
}

/// Build the export document: combined projects, header version and date.
pub fn export_document(projects: &[&ProjectData], version: &str) -> Result<Document, String> {
    let mut doc = combine(projects)?;
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    doc.header = with_version(&doc.header, version, &today);
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn header_version_and_date() {
        let h = "ERMHDR\t23.12\t2026-11-04\tProject\tOpenXER\tOpenXER\t\tUSD";
        let out = with_version(h, "19.12", "2026-12-01");
        assert_eq!(
            out,
            "ERMHDR\t19.12\t2026-12-01\tProject\tOpenXER\tOpenXER\t\tUSD"
        );
        assert_eq!(header_version(&out), "19.12");
        let injected = with_version(h, "19.12\r\n%T\tTASK", "");
        assert!(!injected.contains(['\r', '\n']));
        assert_eq!(injected.split('\t').count(), h.split('\t').count());
    }
}
