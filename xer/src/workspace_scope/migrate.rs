//! Moving the projects of a version 1 workspace (whole XER text per
//! project) into one workspace dictionary.
//!
//! [`plan_migration`] merges every project's workspace rows into a new
//! dictionary in workspace order ([`merge`](super::merge), `Migration`
//! policy): identical rows become one, a later row whose name is taken by
//! different content is kept as `<name> (<project>)`, and ids already taken
//! are renumbered with every reference rewritten. Nothing is written here;
//! the caller stores the plan and proves it with [`verify_hydrated`].
use super::merge::{ignored, Event, EventKind, Merger, Policy};
use super::*;

/// One project's part of a migration.
#[derive(Clone, Debug)]
pub struct ProjectPlan {
    /// Its workspace row id.
    pub id: i64,
    /// Its project short name, used to name copies.
    pub label: String,
    /// The project's document after the migration: what hydrating `stored`
    /// with the new dictionary must give back.
    pub document: Document,
    /// The text to store for it: `document` without workspace rows.
    pub stored: Document,
    pub remap: Remap,
    pub events: Vec<Event>,
}

/// A whole migration, before anything is written.
#[derive(Clone, Debug)]
pub struct MigrationPlan {
    pub dictionary: Document,
    /// Highest id per workspace table, for `workspace_keys`.
    pub keys: BTreeMap<String, i64>,
    pub projects: Vec<ProjectPlan>,
}

/// Plan the migration of `projects` (workspace row id, label, document), in
/// that order.
pub fn plan_migration(projects: &[(i64, String, Document)]) -> MigrationPlan {
    let header = projects
        .first()
        .map_or("ERMHDR\t25.12".to_string(), |p| p.2.header.clone());
    let mut empty = Document::parse(&format!("{header}\r\n%E\r\n"))
        .unwrap_or_else(|_| Document::parse("ERMHDR\t25.12\r\n%E\r\n").unwrap());
    empty.warnings.clear();
    let mut merger = Merger::new(empty);
    // Ids each project keeps for its own rows in the tables of both kinds.
    for (_, _, doc) in projects {
        let scopes = Scopes::of(doc);
        for table in MIXED_TABLES {
            let Some(t) = doc.table(table) else { continue };
            let Some(k) = key_index(t) else { continue };
            for r in &t.rows {
                if !scopes.shared(t, r) {
                    if let Some(id) = int(&r[k]) {
                        merger.reserve(table, id);
                    }
                }
            }
        }
    }
    let mut out = vec![];
    let mut keys: BTreeMap<String, i64> = BTreeMap::new();
    for (id, label, doc) in projects {
        let outcome = merger.merge(
            doc,
            &Policy::Migration {
                label: label.clone(),
            },
            &BTreeSet::new(),
        );
        for (t, h) in table_highs(&outcome.document) {
            let e = keys.entry(t).or_insert(h);
            *e = (*e).max(h);
        }
        let stored = split(&outcome.document).0;
        out.push(ProjectPlan {
            id: *id,
            label: label.clone(),
            document: outcome.document,
            stored,
            remap: outcome.remap,
            events: outcome.events,
        });
    }
    for (t, h) in table_highs(&merger.dict) {
        let e = keys.entry(t).or_insert(h);
        *e = (*e).max(h);
    }
    MigrationPlan {
        dictionary: merger.dict,
        keys,
        projects: out,
    }
}

/// Check that `hydrated` (a project's stored text hydrated with the new
/// dictionary) holds everything `expected` (its planned document) does:
/// every project row identical and in order, every workspace row present
/// with the same values (GUIDs and usage counts of a merged row aside), and
/// no project row the plan did not have. Extra workspace rows, from other
/// projects, are allowed.
pub fn verify_hydrated(expected: &Document, hydrated: &Document) -> Result<(), String> {
    if expected.header != hydrated.header {
        return Err("the file header changed".into());
    }
    let es = Scopes::of(expected);
    let hs = Scopes::of(hydrated);
    for t in &expected.tables {
        let Some(h) = hydrated.table(&t.name) else {
            if t.rows.is_empty() {
                continue;
            }
            return Err(format!("table {} is missing", t.name));
        };
        let project = |table: &Table, scopes: &Scopes, laid: &Table| -> Vec<Vec<String>> {
            table
                .rows
                .iter()
                .filter(|r| !scopes.shared(table, r))
                .map(|r| {
                    laid.fields
                        .iter()
                        .map(|f| table.value(r, f).to_string())
                        .collect()
                })
                .collect()
        };
        let mine = project(t, &es, t);
        let theirs = project(h, &hs, t);
        if mine != theirs {
            let at = mine
                .iter()
                .zip(&theirs)
                .position(|(a, b)| a != b)
                .unwrap_or(mine.len().min(theirs.len()));
            return Err(format!(
                "table {}: project rows differ (expected {}, found {}, first difference at row {})",
                t.name,
                mine.len(),
                theirs.len(),
                at + 1
            ));
        }
        let shared: Vec<&Vec<String>> = t.rows.iter().filter(|r| es.shared(t, r)).collect();
        if shared.is_empty() {
            continue;
        }
        let compared: Vec<&String> = t.fields.iter().filter(|f| !ignored(&t.name, f)).collect();
        let key = key_index(t);
        let hk = key_index(h);
        let index: HashMap<i64, &Vec<String>> = match hk {
            Some(k) => h
                .rows
                .iter()
                .filter(|r| hs.shared(h, r))
                .map(|r| (num(&r[k]), r))
                .collect(),
            None => HashMap::new(),
        };
        let mut unkeyed: Vec<Vec<String>> = h
            .rows
            .iter()
            .map(|r| compared.iter().map(|f| h.value(r, f).to_string()).collect())
            .collect();
        for r in shared {
            let want: Vec<String> = compared.iter().map(|f| t.value(r, f).to_string()).collect();
            match (key, hk) {
                (Some(k), Some(_)) => {
                    let id = num(&r[k]);
                    let Some(found) = index.get(&id) else {
                        return Err(format!("{} row {id} is missing", t.name));
                    };
                    let got: Vec<String> = compared
                        .iter()
                        .map(|f| h.value(found, f).to_string())
                        .collect();
                    if got != want {
                        let column = compared
                            .iter()
                            .zip(want.iter().zip(&got))
                            .find(|(_, (a, b))| a != b)
                            .map_or(String::new(), |(f, _)| f.to_string());
                        return Err(format!("{} row {id} differs in {column}", t.name));
                    }
                }
                _ => match unkeyed.iter().position(|x| *x == want) {
                    Some(i) => {
                        unkeyed.swap_remove(i);
                    }
                    None => return Err(format!("a {} row is missing", t.name)),
                },
            }
        }
    }
    // No project rows appeared that the plan did not have.
    for h in &hydrated.tables {
        if expected.table(&h.name).is_none() && h.rows.iter().any(|r| !hs.shared(h, r)) {
            return Err(format!("table {} appeared", h.name));
        }
    }
    Ok(())
}

/// What scheduling a project gives: the dates and float of every activity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scheduled {
    pub origin_serial: i64,
    pub finish: Option<i64>,
    pub activities: Vec<ScheduledActivity>,
}

/// One activity's calculated values: its id; early start and finish, late
/// start and finish, remaining late start and finish, remaining early start
/// and finish, total and free float; the driving flag; float path and order.
pub type ScheduledActivity = (i64, [Option<i64>; 10], bool, i64, i64);

/// Schedule project `pid` of `doc` (F9), leveled or not, as the command line
/// does, and return the calculated dates. An error is a result too: the same
/// project must fail the same way before and after a migration.
pub fn schedule_result(doc: &Document, pid: i64, level: bool) -> Result<Scheduled, String> {
    let mut p = crate::ProjectData::from_document(doc.clone(), pid, String::new())?;
    p.model.options.do_level = level;
    if !level
        && p.model
            .activities
            .iter()
            .any(|a| a.task_type == openxer_engine::model::TaskType::Resource)
    {
        return Err("resource dependent activities are scheduled only with leveling".into());
    }
    p.refresh_derived_durations();
    openxer_engine::scheduler::schedule_with_resources(&mut p.model, &p.resources)?;
    let m = &p.model;
    let mut activities: Vec<_> = m
        .activities
        .iter()
        .map(|a| {
            (
                a.task_id,
                [
                    a.early_start,
                    a.early_finish,
                    a.late_start,
                    a.late_finish,
                    a.rem_late_start,
                    a.rem_late_finish,
                    a.restart,
                    a.reend,
                    a.total_float,
                    a.free_float,
                ],
                a.driving_path,
                a.float_path,
                a.float_path_order,
            )
        })
        .collect();
    activities.sort_by_key(|a| a.0);
    Ok(Scheduled {
        origin_serial: m.origin_serial,
        finish: m.project.scd_end,
        activities,
    })
}

impl MigrationPlan {
    /// Counts of each kind of decision, over every project.
    pub fn counts(&self) -> BTreeMap<EventKind, usize> {
        let mut out = BTreeMap::new();
        for p in &self.projects {
            for e in &p.events {
                *out.entry(e.kind).or_insert(0) += 1;
            }
        }
        out
    }

    /// Rows of each dictionary table.
    pub fn dictionary_rows(&self) -> BTreeMap<String, usize> {
        self.dictionary
            .tables
            .iter()
            .filter(|t| !t.rows.is_empty())
            .map(|t| (t.name.clone(), t.rows.len()))
            .collect()
    }

    /// A plain-text report of the plan, for a person to read before applying it.
    pub fn report(&self) -> String {
        let mut s = String::new();
        s.push_str(&format!(
            "Workspace dictionary: {} project(s) share {} dictionary row(s).\n",
            self.projects.len(),
            self.dictionary_rows().values().sum::<usize>()
        ));
        for (table, n) in self.dictionary_rows() {
            s.push_str(&format!("  {table}: {n}\n"));
        }
        for p in &self.projects {
            let count = |k: EventKind| p.events.iter().filter(|e| e.kind == k).count();
            s.push_str(&format!(
                "\n{} (workspace row {}): {} shared with earlier projects, {} added, {} renumbered, {} kept as renamed copies, {} own rows renumbered.\n",
                p.label,
                p.id,
                count(EventKind::Merged),
                count(EventKind::Added),
                count(EventKind::Renumbered),
                count(EventKind::Copied),
                count(EventKind::ProjectRenumbered),
            ));
            for e in &p.events {
                match e.kind {
                    EventKind::Copied => s.push_str(&format!(
                        "  {} \"{}\" is kept as \"{}\": it differs from the stored one in {}.\n",
                        e.table,
                        e.name,
                        e.renamed.as_deref().unwrap_or(&e.name),
                        if e.differences.is_empty() {
                            "its child rows".to_string()
                        } else {
                            e.differences.join(", ")
                        }
                    )),
                    EventKind::Merged if e.renamed.is_some() => s.push_str(&format!(
                        "  {} \"{}\" matches the stored \"{}\" and takes its name.\n",
                        e.table,
                        e.name,
                        e.renamed.as_deref().unwrap_or_default()
                    )),
                    EventKind::Renumbered | EventKind::ProjectRenumbered => s.push_str(&format!(
                        "  {} {}{} is renumbered {} (its id is taken).\n",
                        e.table,
                        e.from,
                        if e.name.is_empty() {
                            String::new()
                        } else {
                            format!(" \"{}\"", e.name)
                        },
                        e.to
                    )),
                    _ => {}
                }
            }
        }
        s
    }
}

#[cfg(test)]
mod tests;
