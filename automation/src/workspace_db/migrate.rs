//! Moving a version 1 workspace to a shared workspace dictionary.
use super::{
    ensure, err, hydrate, raise_keys, read_dictionary, read_revision, schema, write_dictionary,
    DICTIONARY_SCHEMA, EARLIER_DICTIONARY_SCHEMA, R, SCHEMA_KEY,
};
use openxer_xer::workspace_scope;
use openxer_xer::workspace_scope::merge::{Event, EventKind};
use openxer_xer::workspace_scope::migrate::{
    plan_migration, schedule_result, verify_hydrated, MigrationPlan, ProjectPlan, Scheduled,
};
use openxer_xer::Document;
use rusqlite::{params, Connection, TransactionBehavior};
use serde_json::{json, Value};
use std::path::Path;

/// A stored project as migration reads it.
struct ProjectRow {
    id: i64,
    key: i64,
    title: String,
    text: String,
    windows_encoding: bool,
    level: bool,
}

fn project_rows(c: &Connection) -> R<Vec<ProjectRow>> {
    let mut stmt = c
        .prepare("SELECT id,project_key,title,xer_content,source_windows_encoding,level_resources FROM projects ORDER BY id")
        .map_err(err)?;
    let rows = stmt
        .query_map([], |r| {
            Ok(ProjectRow {
                id: r.get(0)?,
                key: r.get(1)?,
                title: r.get(2)?,
                text: r.get(3)?,
                windows_encoding: r.get(4)?,
                level: r.get(5)?,
            })
        })
        .map_err(err)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(err)
}

/// Whether two readings of the stored projects hold the same rows.
fn same_rows(a: &[ProjectRow], b: &[ProjectRow]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| {
            (x.id, x.key, &x.text, x.windows_encoding, x.level)
                == (y.id, y.key, &y.text, y.windows_encoding, y.level)
        })
}

/// The schedules a project must keep: with its own leveling setting, and
/// leveled (the dictionary holds the resource limits leveling reads).
fn schedules(doc: &Document, key: i64, level: bool) -> Vec<Result<Scheduled, String>> {
    let mut out = vec![schedule_result(doc, key, level)];
    if !level {
        out.push(schedule_result(doc, key, true));
    }
    out
}

/// Prove one migrated project: `doc` (its stored text hydrated with the new
/// dictionary) holds everything the plan expects and schedules to the same
/// dates and float as before.
fn prove(
    planned: &ProjectPlan,
    doc: &Document,
    key: i64,
    level: bool,
    before: &[Result<Scheduled, String>],
) -> R<()> {
    verify_hydrated(&planned.document, doc)
        .map_err(|e| format!("{} (workspace row {}): {e}", planned.label, planned.id))?;
    if schedules(doc, key, level) != before {
        return Err(format!(
            "{} (workspace row {}): the scheduled dates or float would change",
            planned.label, planned.id
        ));
    }
    Ok(())
}

/// Move a version 1 workspace to a shared dictionary (schema 3), or
/// upgrade a schema 2 one an earlier build made (its EPS code types move
/// into the dictionary).
///
/// The plan is made and proved first: every project, its stored text
/// hydrated with the new dictionary, must hold all of its rows (ids
/// renumbered where the plan says) and schedule, unleveled and leveled, to
/// the same dates and float as before. A dry run stops there and changes
/// nothing. Otherwise `backup`, when given, receives a copy of the database
/// first (it must not exist), then one transaction writes the dictionary
/// and every project's text, proves each project again from what was
/// written, and commits only when all pass.
pub fn migrate(c: &mut Connection, dry_run: bool, backup: Option<&Path>) -> R<Value> {
    migrate_with(c, dry_run, backup, || {})
}

/// [`migrate`], running `between` after the plan is proved and before the
/// transaction that writes it (tests use it to write from another
/// connection in between).
fn migrate_with(
    c: &mut Connection,
    dry_run: bool,
    backup: Option<&Path>,
    between: impl FnOnce(),
) -> R<Value> {
    let before = schema(c)?;
    if before > DICTIONARY_SCHEMA {
        return Err(format!(
            "This workspace uses workspace schema {before}, which needs a newer OpenXER."
        ));
    }
    if before == DICTIONARY_SCHEMA {
        return Ok(
            json!({"dry_run": dry_run, "schema_before": before, "already_migrated": true, "projects": []}),
        );
    }
    let rows = project_rows(c)?;
    // A workspace an earlier build gave a dictionary (schema 2) keeps its
    // EPS code types in each project's text: each project is read whole,
    // its text with that dictionary, and converted as a version 1 one.
    let earlier = if before == EARLIER_DICTIONARY_SCHEMA {
        Some(read_dictionary(c)?)
    } else {
        None
    };
    let eps = super::read_eps(c)?;
    let mut inputs = vec![];
    let mut originals = vec![];
    for row in &rows {
        let mut doc = Document::parse(&row.text).map_err(|e| {
            format!(
                "Workspace row {} ({}) cannot be read: {e}",
                row.id, row.title
            )
        })?;
        if let Some((dict, _)) = &earlier {
            workspace_scope::hydrate(&mut doc, dict);
        }
        doc.windows_encoding = row.windows_encoding;
        let label = doc
            .projects()
            .into_iter()
            .find(|(id, _)| *id == row.key)
            .map(|(_, name)| name)
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| row.title.clone());
        originals.push(doc.clone());
        // EPS code types name the EPS nodes of the database the project came
        // from; they move to the project's own workspace node.
        let place = super::place_in(&eps, Some(row.id));
        workspace_scope::place_eps_codes(&mut doc, &place, &Default::default());
        inputs.push((row.id, label, doc));
    }
    let plan = plan_migration(&inputs);
    let baseline: Vec<Vec<Result<Scheduled, String>>> = rows
        .iter()
        .zip(&originals)
        .map(|(row, doc)| schedules(doc, row.key, row.level))
        .collect();
    // Prove the plan in memory, through text as it will be stored.
    let dict = Document::parse(&plan.dictionary.text()).map_err(err)?;
    for ((row, planned), before) in rows.iter().zip(&plan.projects).zip(&baseline) {
        let mut doc = Document::parse(&planned.stored.text()).map_err(err)?;
        doc.windows_encoding = row.windows_encoding;
        workspace_scope::hydrate(&mut doc, &dict);
        prove(planned, &doc, row.key, row.level, before)?;
    }
    let mut result = report_json(&plan, dry_run, before);
    if dry_run {
        return Ok(result);
    }
    if let Some(path) = backup {
        if path.exists() {
            return Err(format!("The backup {} already exists.", path.display()));
        }
        c.execute("VACUUM INTO ?1", [path.to_string_lossy()])
            .map_err(|e| format!("The backup could not be written: {e}"))?;
        result["backup"] = json!(path);
    }
    between();
    // The write lock is taken first (BEGIN IMMEDIATE) and the rows are read
    // again inside it: proving a large workspace takes minutes, and a save
    // made in that time by another connection must not be overwritten by
    // text planned from the rows as they were.
    let tx = c
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(err)?;
    let revision = read_revision(&tx)?;
    if schema(&tx)? != before
        || !same_rows(&rows, &project_rows(&tx)?)
        || earlier.as_ref().is_some_and(|(_, r)| *r != revision)
    {
        return Err("The workspace changed while the conversion was being checked (a project was saved by another program). Nothing was changed; run the conversion again.".into());
    }
    ensure(&tx)?;
    write_dictionary(&tx, &plan.dictionary, revision + 1)?;
    raise_keys(&tx, &plan.keys)?;
    for planned in &plan.projects {
        tx.execute(
            "UPDATE projects SET xer_content=?1 WHERE id=?2",
            params![planned.stored.text(), planned.id],
        )
        .map_err(err)?;
    }
    tx.execute(
        "INSERT INTO preferences(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![SCHEMA_KEY, DICTIONARY_SCHEMA.to_string()],
    )
    .map_err(err)?;
    // Prove every project again from what the transaction holds.
    for ((row, planned), before) in rows.iter().zip(&plan.projects).zip(&baseline) {
        let text: String = tx
            .query_row(
                "SELECT xer_content FROM projects WHERE id=?1",
                [row.id],
                |r| r.get(0),
            )
            .map_err(err)?;
        let mut doc = Document::parse(&text).map_err(err)?;
        doc.windows_encoding = row.windows_encoding;
        hydrate(&tx, &mut doc, Some(row.id))?;
        prove(planned, &doc, row.key, row.level, before)?;
    }
    tx.commit().map_err(err)?;
    Ok(result)
}

/// The migration report as JSON, with the plain-text report beside it.
fn events(p: &ProjectPlan, kind: EventKind) -> impl Iterator<Item = &Event> {
    p.events.iter().filter(move |e| e.kind == kind)
}

fn report_json(plan: &MigrationPlan, dry_run: bool, before: i64) -> Value {
    let projects: Vec<Value> = plan
        .projects
        .iter()
        .map(|p| {
            json!({
                "workspace_id": p.id,
                "project": p.label,
                "shared": events(p, EventKind::Merged).count(),
                "added": events(p, EventKind::Added).count(),
                "copies": events(p, EventKind::Copied).map(|e| json!({
                    "table": e.table, "name": e.name, "renamed": e.renamed,
                    "differences": e.differences, "from_id": e.from, "to_id": e.to,
                })).collect::<Vec<_>>(),
                "renumbered": events(p, EventKind::Renumbered)
                    .chain(events(p, EventKind::ProjectRenumbered))
                    .map(|e| json!({"table": e.table, "name": e.name, "from_id": e.from, "to_id": e.to}))
                    .collect::<Vec<_>>(),
                "renamed_to_match": events(p, EventKind::Merged)
                    .filter(|e| e.renamed.is_some())
                    .map(|e| json!({"table": e.table, "name": e.name, "renamed": e.renamed}))
                    .collect::<Vec<_>>(),
                "verified": {"rows": true, "schedule": true},
            })
        })
        .collect();
    json!({
        "dry_run": dry_run,
        "schema_before": before,
        "schema_after": if dry_run { before } else { DICTIONARY_SCHEMA },
        "dictionary_rows": plan.dictionary_rows(),
        "projects": projects,
        "report": plan.report(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// B9/B12: a project saved by another connection while the conversion
    /// is being proved is not overwritten with text planned from the rows
    /// as they were: the conversion is refused and changes nothing.
    #[test]
    fn a_save_made_while_the_conversion_is_checked_is_never_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("w.db");
        let text = openxer_xer::starter().updated_document().text();
        {
            let c = Connection::open(&path).unwrap();
            crate::storage::schema(&c).unwrap();
            c.execute(
                "INSERT INTO projects(source_name,project_key,title,xer_content) VALUES('A.xer',1,'A',?1)",
                [&text],
            )
            .unwrap();
        }
        let saved = text.replace("RIVERBEND", "RIVERBEND SAVED MEANWHILE");
        assert_ne!(saved, text);
        let mut c = Connection::open(&path).unwrap();
        let result = migrate_with(&mut c, false, None, || {
            let other = Connection::open(&path).unwrap();
            other
                .execute("UPDATE projects SET xer_content=?1", [&saved])
                .unwrap();
        });
        let error = result.expect_err("refused");
        assert!(error.contains("changed while"), "{error}");
        let now: String = c
            .query_row("SELECT xer_content FROM projects", [], |r| r.get(0))
            .unwrap();
        assert_eq!(now, saved, "the later save stays");
        assert_eq!(schema(&c).unwrap(), 1, "nothing was converted");
    }
}
