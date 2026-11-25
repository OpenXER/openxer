//! Workspace side of XER import: list rows, back up the database, write a batch
//! of imported projects in one transaction, and undo the last import.
//!
//! In a workspace with a shared dictionary (schema 3) each imported project's
//! calendars, resources, codes and other dictionary rows are merged into the
//! dictionary as they are written (`openxer_automation::workspace_db`): a row
//! the workspace already has with the same content is reused, one whose name
//! it has with different content is added as a copy `<name>_<n>` unless the
//! import chose Keep or Update for that kind of row, and the import report
//! lists those decisions. The dictionary as it was before is kept with the
//! batch, so Undo Last Import restores it too.
use crate::store::Store;
use openxer_automation::workspace_db::{self, Choices};
use openxer_xer::workspace_scope::merge::Event;
use openxer_xer::ProjectData;
use rusqlite::{params, OptionalExtension};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq)]
pub struct WorkspaceRow {
    pub id: i64,
    pub title: String,
    pub source: String,
    pub project_key: i64,
}

pub fn rows(store: &Store) -> Result<Vec<WorkspaceRow>, String> {
    let mut stmt = store
        .connection()
        .prepare("SELECT id,title,source_name,project_key FROM projects ORDER BY title COLLATE NOCASE,id")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok(WorkspaceRow {
                id: r.get(0)?,
                title: r.get(1)?,
                source: r.get(2)?,
                project_key: r.get(3)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

/// What one imported project does to the workspace.
#[derive(Clone)]
pub enum Write {
    /// Insert as a new row (the source name is made unique).
    Insert(ProjectData),
    /// Overwrite row `id` with this project (Replace or Update Existing).
    Overwrite(i64, ProjectData),
    /// Overwrite row `id` with a project read whole from the file (Replace
    /// Existing): none of its dictionary rows are the ones the row read.
    Replace(i64, ProjectData),
}

fn ensure_undo_table(store: &Store) -> Result<(), String> {
    let c = store.connection();
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS import_undo(batch INTEGER NOT NULL, row_id INTEGER NOT NULL, \
         existed INTEGER NOT NULL, source_name TEXT, project_key INTEGER, title TEXT, \
         xer_content TEXT, source_windows_encoding INTEGER, level_resources INTEGER, \
         created_at TEXT DEFAULT CURRENT_TIMESTAMP); \
         CREATE TABLE IF NOT EXISTS import_undo_dictionary(batch INTEGER PRIMARY KEY, \
         xer_content TEXT NOT NULL, revision INTEGER NOT NULL, after_content TEXT)",
    )
    .map_err(|e| e.to_string())?;
    // The dictionary as the import left it, so undo takes back only the
    // import's own rows (a table made before this column has none).
    let has_after = c
        .prepare("SELECT after_content FROM import_undo_dictionary LIMIT 0")
        .is_ok();
    if !has_after {
        c.execute_batch("ALTER TABLE import_undo_dictionary ADD COLUMN after_content TEXT")
            .map_err(|e| e.to_string())?;
    }
    // Each row as the import wrote it, so Undo Last Import can say which
    // projects were changed after it (a table made before this column has
    // none, and then it cannot tell).
    let has_written = c
        .prepare("SELECT after_content FROM import_undo LIMIT 0")
        .is_ok();
    if !has_written {
        c.execute_batch("ALTER TABLE import_undo ADD COLUMN after_content TEXT")
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// One project the last import wrote, as Undo Last Import would treat it.
#[derive(Clone, Debug, PartialEq)]
pub struct UndoEntry {
    pub row_id: i64,
    /// The row was there before the import (it goes back to how it was);
    /// otherwise the import added it (it is deleted).
    pub existed: bool,
    /// The project's name now, or as it was before the import.
    pub title: String,
    /// Whether the stored project differs from what the import wrote:
    /// `None` when the import was recorded without that.
    pub changed: Option<bool>,
}

/// The projects the last import wrote, for the confirmation Undo Last Import
/// asks first.
pub fn last_import_entries(store: &Store) -> Result<Vec<UndoEntry>, String> {
    let Some((batch, _, _)) = last_import(store)? else {
        return Ok(vec![]);
    };
    let mut stmt = store
        .connection()
        .prepare(
            "SELECT u.row_id,u.existed,COALESCE(p.title,u.title,''),u.after_content,p.xer_content \
             FROM import_undo u LEFT JOIN projects p ON p.id=u.row_id WHERE u.batch=?1 ORDER BY u.rowid",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([batch], |r| {
            let after: Option<String> = r.get(3)?;
            let now: Option<String> = r.get(4)?;
            Ok(UndoEntry {
                row_id: r.get(0)?,
                existed: r.get(1)?,
                title: r.get(2)?,
                changed: after.map(|a| now.as_deref() != Some(a.as_str())),
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

/// The writes of an import as `preview` and `commit` merge them: each
/// project's document, what it read from the dictionary and its row.
fn merge_inputs(
    store: &Store,
    writes: &[&Write],
) -> Vec<(
    openxer_xer::Document,
    Option<workspace_db::Mark>,
    Option<i64>,
)> {
    writes
        .iter()
        .map(|w| match *w {
            Write::Insert(p) => (p.updated_document(), None, None),
            Write::Overwrite(id, p) => (
                p.updated_document(),
                store.mark_of(&p.source_name, p.model.project.proj_id),
                Some(*id),
            ),
            Write::Replace(id, p) => (p.updated_document(), None, Some(*id)),
        })
        .collect()
}

/// What an import into a workspace with a shared dictionary would do to it,
/// per write, without writing: the merge events (see
/// `workspace_db::import_report`). Empty lists in a workspace without one.
pub fn preview(
    store: &Store,
    writes: &[&Write],
    choices: &Choices,
) -> Result<Vec<Vec<Event>>, String> {
    if !store.shared()? {
        return Ok(writes.iter().map(|_| vec![]).collect());
    }
    let inputs = merge_inputs(store, writes);
    let refs: Vec<(
        &openxer_xer::Document,
        Option<&workspace_db::Mark>,
        Option<i64>,
    )> = inputs.iter().map(|(d, m, r)| (d, m.as_ref(), *r)).collect();
    workspace_db::preview(store.connection(), &refs, choices)
}

/// The name every automatic backup taken before an import starts with.
pub const IMPORT_BACKUP: &str = "openxer-before-import-";
/// The name every automatic backup taken before Undo Last Import starts with.
pub const UNDO_BACKUP: &str = "openxer-before-undo-import-";

/// A consistent copy of the whole workspace, taken before an import writes.
pub fn backup(store: &Store) -> Result<PathBuf, String> {
    backup_named(store, IMPORT_BACKUP)
}

/// A consistent copy of the whole workspace in `backups` beside it, named
/// `<prefix><time stamp>.db`. SQLite writes it from one read transaction,
/// so it is the workspace as one moment had it.
pub fn backup_named(store: &Store, prefix: &str) -> Result<PathBuf, String> {
    let dir = store
        .path
        .parent()
        .map(|p| p.join("backups"))
        .unwrap_or_else(|| PathBuf::from("backups"));
    std::fs::create_dir_all(&dir).map_err(|e| format!("Backup folder: {e}"))?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let mut path = dir.join(format!("{prefix}{stamp}.db"));
    let mut n = 1;
    while path.exists() {
        path = dir.join(format!("{prefix}{stamp}-{n}.db"));
        n += 1;
    }
    store
        .connection()
        .execute("VACUUM INTO ?1", [path.display().to_string()])
        .map_err(|e| {
            // A copy cut short (a full disk) is no backup: it goes.
            let _ = std::fs::remove_file(&path);
            format!("Backup to {} failed: {e}", path.display())
        })?;
    Ok(path)
}

fn unique_source(
    store: &Store,
    source: &str,
    key: i64,
    reserved: &[(String, i64)],
) -> Result<String, String> {
    let taken = |name: &str| -> Result<bool, String> {
        if reserved.iter().any(|(s, k)| s == name && *k == key) {
            return Ok(true);
        }
        store
            .connection()
            .query_row(
                "SELECT 1 FROM projects WHERE source_name=?1 AND project_key=?2",
                params![name, key],
                |_| Ok(()),
            )
            .optional()
            .map(|v| v.is_some())
            .map_err(|e| e.to_string())
    };
    if !taken(source)? {
        return Ok(source.to_string());
    }
    for n in 2.. {
        let candidate = format!("{source} ({n})");
        if !taken(&candidate)? {
            return Ok(candidate);
        }
    }
    unreachable!()
}

/// Write all imported projects in one transaction, keeping the previous rows so
/// the import can be undone. Returns the workspace ids written, in order.
#[cfg(test)]
pub fn commit(store: &mut Store, writes: Vec<Write>) -> Result<Vec<i64>, String> {
    commit_with(store, writes, &Choices::new(), None).map(|(ids, _)| ids)
}

/// [`commit`] with `choices` for dictionary rows whose name the workspace
/// has with different content; also returns each write's merge events.
/// New projects are filed under EPS node `folder` first, so the EPS codes
/// they bring are filed there too.
pub fn commit_with(
    store: &mut Store,
    writes: Vec<Write>,
    choices: &Choices,
    folder: Option<i64>,
) -> Result<(Vec<i64>, Vec<Vec<Event>>), String> {
    ensure_undo_table(store)?;
    let shared = store.shared()?;
    let c = store.connection();
    let batch: i64 = c
        .query_row(
            "SELECT COALESCE(MAX(batch),0)+1 FROM import_undo",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    let mut prepared = vec![];
    // What each merged project read from the dictionary.
    let mut marks: Vec<Option<workspace_db::Mark>> = vec![];
    for w in writes {
        match w {
            Write::Insert(mut p) => {
                let reserved: Vec<(String, i64)> = prepared
                    .iter()
                    .map(|(_, q): &(Option<i64>, ProjectData)| {
                        (q.source_name.clone(), q.model.project.proj_id)
                    })
                    .collect();
                p.source_name =
                    unique_source(store, &p.source_name, p.model.project.proj_id, &reserved)?;
                prepared.push((None, p));
            }
            Write::Overwrite(id, p) => {
                marks.push(store.mark_of(&p.source_name, p.model.project.proj_id));
                prepared.push((Some(id), p));
                continue;
            }
            Write::Replace(id, p) => prepared.push((Some(id), p)),
        }
        marks.push(None);
    }
    let c = store.connection();
    c.execute_batch("BEGIN IMMEDIATE")
        .map_err(|e| e.to_string())?;
    let mut events = vec![];
    let mut new_marks = vec![];
    let result = (|| -> Result<Vec<i64>, String> {
        if shared {
            // The dictionary as it was (an empty one when it has no row
            // yet), for Undo Last Import; the import's own rows are what
            // differs from it when the import is done.
            let (before, revision) = workspace_db::read_dictionary(c)?;
            c.execute(
                "INSERT INTO import_undo_dictionary(batch,xer_content,revision) VALUES(?1,?2,?3)",
                params![batch, before.text(), revision],
            )
            .map_err(|e| e.to_string())?;
        }
        let mut ids = vec![];
        for ((id, p), mark) in prepared.iter().zip(&marks) {
            let doc = p.updated_document();
            // The row of a new project, so the dictionary merge knows it.
            let id = match id {
                Some(id) => Some(*id),
                None if shared => {
                    c.execute(
                        "INSERT INTO projects(source_name,project_key,title,xer_content,source_windows_encoding,level_resources) VALUES(?1,?2,?3,'',?4,?5)",
                        params![p.source_name, p.model.project.proj_id, p.model.project.name, p.document.windows_encoding, p.model.options.do_level],
                    )
                    .map_err(|e| e.to_string())?;
                    let id = c.last_insert_rowid();
                    c.execute(
                        "INSERT INTO import_undo(batch,row_id,existed) VALUES(?1,?2,0)",
                        params![batch, id],
                    )
                    .map_err(|e| e.to_string())?;
                    if let Some(folder) = folder {
                        file_under(c, id, folder)?;
                    }
                    let stored = workspace_db::store_with(
                        c,
                        &doc,
                        None,
                        Some(id),
                        &openxer_xer::workspace_scope::merge::Policy::Save,
                        choices,
                    )?;
                    c.execute(
                        "UPDATE projects SET xer_content=?1 WHERE id=?2",
                        params![stored.text, id],
                    )
                    .map_err(|e| e.to_string())?;
                    events.push(stored.outcome.events);
                    new_marks.push(None);
                    ids.push(id);
                    continue;
                }
                None => None,
            };
            let text = if shared {
                let stored = workspace_db::store_with(
                    c,
                    &doc,
                    mark.as_ref(),
                    id,
                    &openxer_xer::workspace_scope::merge::Policy::Save,
                    choices,
                )?;
                events.push(stored.outcome.events);
                new_marks.push(id.map(|_| {
                    (
                        (p.source_name.clone(), p.model.project.proj_id),
                        stored.mark,
                    )
                }));
                stored.text
            } else {
                events.push(vec![]);
                doc.text()
            };
            match id {
                Some(id) => {
                    let saved = c
                        .execute(
                            "INSERT INTO import_undo(batch,row_id,existed,source_name,project_key,title,xer_content,source_windows_encoding,level_resources) \
                             SELECT ?1,id,1,source_name,project_key,title,xer_content,source_windows_encoding,level_resources FROM projects WHERE id=?2",
                            params![batch, id],
                        )
                        .map_err(|e| e.to_string())?;
                    if saved != 1 {
                        return Err(format!("Workspace project {id} no longer exists"));
                    }
                    c.execute(
                        "UPDATE projects SET source_name=?1,project_key=?2,title=?3,xer_content=?4,source_windows_encoding=?5,level_resources=?6,updated_at=CURRENT_TIMESTAMP WHERE id=?7",
                        params![p.source_name, p.model.project.proj_id, p.model.project.name, text, p.document.windows_encoding, p.model.options.do_level, id],
                    )
                    .map_err(|e| match e.to_string() {
                        s if s.contains("UNIQUE") => "Another workspace project already uses this source file and project ID".to_string(),
                        s => s,
                    })?;
                    ids.push(id);
                }
                None => {
                    c.execute(
                        "INSERT INTO projects(source_name,project_key,title,xer_content,source_windows_encoding,level_resources) VALUES(?1,?2,?3,?4,?5,?6)",
                        params![p.source_name, p.model.project.proj_id, p.model.project.name, text, p.document.windows_encoding, p.model.options.do_level],
                    )
                    .map_err(|e| e.to_string())?;
                    let id = c.last_insert_rowid();
                    c.execute(
                        "INSERT INTO import_undo(batch,row_id,existed) VALUES(?1,?2,0)",
                        params![batch, id],
                    )
                    .map_err(|e| e.to_string())?;
                    ids.push(id);
                }
            }
        }
        if shared {
            let (after, _) = workspace_db::read_dictionary(c)?;
            c.execute(
                "UPDATE import_undo_dictionary SET after_content=?1 WHERE batch=?2",
                params![after.text(), batch],
            )
            .map_err(|e| e.to_string())?;
        }
        c.execute(
            "UPDATE import_undo SET after_content=(SELECT xer_content FROM projects WHERE id=import_undo.row_id) WHERE batch=?1",
            [batch],
        )
        .map_err(|e| e.to_string())?;
        Ok(ids)
    })();
    match result {
        Ok(ids) => {
            commit_or_roll_back(c)?;
            for (key, mark) in new_marks.into_iter().flatten() {
                store.set_mark(key, mark);
            }
            Ok((ids, events))
        }
        Err(e) => {
            let _ = c.execute_batch("ROLLBACK");
            Err(e)
        }
    }
}

/// File workspace project `id` under EPS node `folder` (a node that no
/// longer exists is left alone).
fn file_under(c: &rusqlite::Connection, id: i64, folder: i64) -> Result<(), String> {
    use openxer_xer::workspace::{Workspace, KEY};
    let text: Option<String> = c
        .query_row("SELECT value FROM preferences WHERE key=?1", [KEY], |r| {
            r.get(0)
        })
        .optional()
        .map_err(|e| e.to_string())?;
    let mut ws = Workspace::from_text(&text.unwrap_or_default());
    if ws.place(id, folder).is_ok() {
        c.execute(
            "INSERT INTO preferences(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![KEY, ws.to_text()],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Description of the last import that can be undone.
pub fn last_import(store: &Store) -> Result<Option<(i64, usize, String)>, String> {
    ensure_undo_table(store)?;
    store
        .connection()
        .query_row(
            "SELECT batch,COUNT(*),MAX(created_at) FROM import_undo GROUP BY batch ORDER BY batch DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get::<_, i64>(1)? as usize, r.get(2)?)),
        )
        .optional()
        .map_err(|e| e.to_string())
}

/// COMMIT the transaction open on `c`, or roll it back when the commit
/// fails, so no later write runs inside a transaction left open.
fn commit_or_roll_back(c: &rusqlite::Connection) -> Result<(), String> {
    c.execute_batch("COMMIT").map_err(|e| {
        let _ = c.execute_batch("ROLLBACK");
        e.to_string()
    })
}

/// Take back the import's changes to the workspace dictionary (`before`
/// and `after` it) from the dictionary as it is now: a row changed since
/// the import stays as it is now, and a row the import added stays while a
/// project (a stored one, or one of `open`) or another dictionary row uses
/// it. Returns a line for each row that stays.
fn undo_dictionary(
    c: &rusqlite::Connection,
    before: &str,
    after: &str,
    open: &[(Option<i64>, &openxer_xer::Document)],
) -> Result<Vec<String>, String> {
    use openxer_xer::workspace_scope::delta;
    let before = openxer_xer::Document::parse(before)?;
    let after = openxer_xer::Document::parse(after)?;
    let (mut now, _) = workspace_db::read_dictionary(c)?;
    let mut d = delta::diff(&before, &after);
    let mut kept = vec![];
    for (table, id) in d.stale(&now, false) {
        kept.push(format!(
            "{} was changed after the import and stays as it is now.",
            workspace_db::row_label(&now, &table, id)
        ));
        d = d.without(&[(table, id)].into());
    }
    // Keeping one row can keep the rows it refers to: settle.
    loop {
        let used = workspace_db::removals_in_use(c, &now, &d, false, None, open)?;
        if used.is_empty() {
            break;
        }
        for ((table, id), names) in used {
            kept.push(format!(
                "{} is used by {} and stays.",
                workspace_db::row_label(&now, &table, id),
                names.join(", ")
            ));
            d = d.without(&[(table, id)].into());
        }
    }
    d.apply(&mut now, false);
    workspace_db::replace_dictionary(c, &now)?;
    Ok(kept)
}

/// Restore every row the last import wrote: overwritten rows get their previous
/// content back and inserted rows are removed. The workspace dictionary loses
/// the rows the import added or changed, except those changed since or still
/// in use (by a stored project or one of `open`, the open projects with
/// their workspace ids). Returns the restored row ids and a line for each
/// dictionary row that stays.
pub fn undo_last_import(
    store: &mut Store,
    open: &[(Option<i64>, &openxer_xer::Document)],
) -> Result<(Vec<i64>, Vec<String>), String> {
    let Some((batch, _, _)) = last_import(store)? else {
        return Err("There is no import to undo".into());
    };
    let c = store.connection();
    c.execute_batch("BEGIN IMMEDIATE")
        .map_err(|e| e.to_string())?;
    let result = (|| -> Result<(Vec<i64>, Vec<String>), String> {
        let mut stmt = c
            .prepare("SELECT row_id,existed,source_name,project_key,title,xer_content,source_windows_encoding,level_resources FROM import_undo WHERE batch=?1")
            .map_err(|e| e.to_string())?;
        type UndoRow = (
            i64,
            bool,
            Option<String>,
            Option<i64>,
            Option<String>,
            Option<String>,
            Option<bool>,
            Option<bool>,
        );
        let entries: Vec<UndoRow> = stmt
            .query_map([batch], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                ))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        let mut ids = vec![];
        for (id, existed, source, key, title, text, legacy, level) in entries {
            if existed {
                c.execute(
                    "UPDATE projects SET source_name=?1,project_key=?2,title=?3,xer_content=?4,source_windows_encoding=?5,level_resources=?6,updated_at=CURRENT_TIMESTAMP WHERE id=?7",
                    params![source, key, title, text, legacy, level, id],
                )
                .map_err(|e| e.to_string())?;
            } else {
                c.execute("DELETE FROM projects WHERE id=?1", [id])
                    .map_err(|e| e.to_string())?;
            }
            ids.push(id);
        }
        crate::baselines::store::delete_import(c, batch)?;
        // The import's own dictionary rows, now that the projects are back.
        let dictionary: Option<(String, Option<String>)> = c
            .query_row(
                "SELECT xer_content,after_content FROM import_undo_dictionary WHERE batch=?1",
                [batch],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        let restored: Vec<i64> = ids.clone();
        let others: Vec<(Option<i64>, &openxer_xer::Document)> = open
            .iter()
            .filter(|(id, _)| !id.is_some_and(|id| restored.contains(&id)))
            .copied()
            .collect();
        let kept = match dictionary {
            Some((before, Some(after))) => undo_dictionary(c, &before, &after, &others)?,
            // Recorded without the dictionary as the import left it: what
            // the import changed is unknown, so nothing is taken back.
            Some((_, None)) => vec![
                "The workspace dictionary is left as it is: this import was recorded before its changes to it could be undone."
                    .into(),
            ],
            None => vec![],
        };
        c.execute("DELETE FROM import_undo WHERE batch=?1", [batch])
            .map_err(|e| e.to_string())?;
        c.execute("DELETE FROM import_undo_dictionary WHERE batch=?1", [batch])
            .map_err(|e| e.to_string())?;
        // The last-opened project may have been removed.
        c.execute(
            "DELETE FROM preferences WHERE key='last_project' AND value NOT IN (SELECT CAST(id AS TEXT) FROM projects)",
            [],
        )
        .map_err(|e| e.to_string())?;
        Ok((ids, kept))
    })();
    match result {
        Ok(done) => {
            commit_or_roll_back(c)?;
            Ok(done)
        }
        Err(e) => {
            let _ = c.execute_batch("ROLLBACK");
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openxer_interchange::xer_merge::{merge_project, ImportConfig};

    fn temp_store(name: &str) -> (Store, PathBuf) {
        let path = std::env::temp_dir().join(format!("openxer-{name}-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        (Store::open(path.clone()).unwrap(), path)
    }

    #[test]
    fn create_replace_update_and_undo_on_the_starter() {
        let (mut store, path) = temp_store("ws-import");
        let starter = openxer_xer::starter();
        let base = store.save(&starter).unwrap();
        // Create New twice: the second gets a unique source name.
        let ids = commit(
            &mut store,
            vec![
                Write::Insert(starter.clone()),
                Write::Insert(starter.clone()),
            ],
        )
        .unwrap();
        assert_eq!(rows(&store).unwrap().len(), 3);
        assert!(rows(&store)
            .unwrap()
            .iter()
            .any(|r| r.source.ends_with("(3)")));
        undo_last_import(&mut store, &[]).unwrap();
        assert_eq!(rows(&store).unwrap().len(), 1);
        assert!(!ids.contains(&base));
        // Update Existing: rename an activity in the "file".
        let mut edited = starter.clone();
        edited.details.get_mut(&1010).unwrap().name = "Updated by import".into();
        let target = store.load(base).unwrap();
        let (doc, report) = merge_project(
            &target.updated_document(),
            1,
            &edited.updated_document(),
            1,
            &ImportConfig::default(),
        )
        .unwrap();
        assert_eq!(
            report.counts[&openxer_interchange::xer_merge::DataType::Activities].updated,
            1
        );
        let mut merged = ProjectData::from_document(doc, 1, target.source_name.clone()).unwrap();
        merged.model.options.do_level = target.model.options.do_level;
        let backup_path = backup(&store).unwrap();
        assert!(backup_path.exists());
        commit(&mut store, vec![Write::Overwrite(base, merged)]).unwrap();
        assert_eq!(
            store.load(base).unwrap().details[&1010].name,
            "Updated by import"
        );
        assert!(last_import(&store).unwrap().is_some());
        undo_last_import(&mut store, &[]).unwrap();
        assert_eq!(
            store.load(base).unwrap().details[&1010].name,
            starter.details[&1010].name
        );
        // Replace Existing with a different project keeps the row id.
        let mut other = starter.clone();
        other.model.project.name = "REPLACED".into();
        other.source_name = target.source_name.clone();
        commit(&mut store, vec![Write::Overwrite(base, other)]).unwrap();
        assert_eq!(store.load(base).unwrap().model.project.name, "REPLACED");
        undo_last_import(&mut store, &[]).unwrap();
        assert_eq!(store.load(base).unwrap().model.project.name, "RIVERBEND");
        assert!(undo_last_import(&mut store, &[]).is_err());
        drop(store);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(backup_path);
    }

    #[test]
    fn corpus_project_imports_as_new_and_updates() {
        let dir = crate::test_corpus::dir();
        let file = dir.join("6031.xer");
        if !file.exists() {
            eprintln!("skipped: corpus file {} is not available", file.display());
            return;
        }
        let (mut store, path) = temp_store("ws-corpus");
        let doc = openxer_xer::Document::read(&file).unwrap();
        let pid = doc.projects()[0].0;
        let p = ProjectData::from_document(doc.clone(), pid, file.display().to_string()).unwrap();
        let ids = commit(&mut store, vec![Write::Insert(p.clone())]).unwrap();
        let loaded = store.load(ids[0]).unwrap();
        assert_eq!(loaded.model, p.model);
        let (merged, report) = merge_project(
            &loaded.updated_document(),
            pid,
            &doc,
            pid,
            &ImportConfig::default(),
        )
        .unwrap();
        assert!(
            report
                .counts
                .values()
                .all(|c| c.added == 0 && c.updated == 0),
            "{report:?}"
        );
        let merged = ProjectData::from_document(merged, pid, loaded.source_name.clone()).unwrap();
        commit(&mut store, vec![Write::Overwrite(ids[0], merged)]).unwrap();
        assert_eq!(store.load(ids[0]).unwrap().model, p.model);
        drop(store);
        let _ = std::fs::remove_file(&path);
    }
}
