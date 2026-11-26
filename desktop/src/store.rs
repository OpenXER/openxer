use openxer_automation::workspace_db::{self, Mark};
use openxer_xer::workspace_scope::{self, delta::Delta};
use openxer_xer::{Document, ProjectData};
use rusqlite::{params, Connection, OptionalExtension};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
/// What [`Store::sync_dictionary`] did.
pub enum Sync {
    /// The dictionary holds the project's rows. `document` is the project's
    /// document as stored when the merge changed it (a renamed copy, a row
    /// matched to a stored one, an EPS code moved to the project's node).
    Done {
        delta: Delta,
        document: Option<Document>,
        revision: i64,
    },
    /// Nothing was written: the edit deleted rows other projects use.
    Refused(String),
}

pub struct Store {
    connection: Connection,
    pub path: PathBuf,
    /// The last project listings and the database state they were read at.
    listings: RefCell<Listings>,
    /// In a workspace with a dictionary (schema 3): what each opened
    /// project read from it, by source name and project key, so a save
    /// knows which workspace rows are the project's own copies.
    marks: RefCell<HashMap<(String, i64), Mark>>,
}

/// `Store::list` and `Store::stamps` as last read. Views ask for them every
/// frame, and each query walks the stored XER text of every row (a few ms
/// with one 3,000-activity project), so they are kept until the database
/// changes: a write through this connection moves `total_changes`, a commit
/// by another connection moves `data_version`, and nothing is kept while a
/// transaction is open.
#[derive(Default)]
struct Listings {
    at: Option<(u64, i64)>,
    list: Option<Listing>,
    stamps: Option<Listing>,
}

/// Workspace rows: id and title, or id and last-save stamp.
type Listing = Vec<(i64, String)>;

impl Store {
    /// Open (creating) a workspace as an earlier OpenXER made it: projects
    /// keep their own dictionaries (schema 1).
    #[cfg(test)]
    pub fn open_version_1(path: PathBuf) -> Self {
        let c = Connection::open(&path).unwrap();
        c.execute_batch("CREATE TABLE IF NOT EXISTS projects(id INTEGER PRIMARY KEY, source_name TEXT NOT NULL, project_key INTEGER NOT NULL, title TEXT NOT NULL, xer_content TEXT NOT NULL, updated_at TEXT DEFAULT CURRENT_TIMESTAMP, UNIQUE(source_name,project_key));").unwrap();
        drop(c);
        Self::open(path).unwrap()
    }
    /// Forget the kept listings.
    #[cfg(test)]
    pub fn forget_listings(&self) {
        *self.listings.borrow_mut() = Listings::default();
    }

    /// The database state listings are kept for, `None` inside a transaction.
    fn listing_state(&self) -> Option<(u64, i64)> {
        if !self.connection.is_autocommit() {
            return None;
        }
        let version = self
            .connection
            .query_row("PRAGMA data_version", [], |r| r.get(0))
            .ok()?;
        Some((self.connection.total_changes(), version))
    }

    /// A listing kept by `slot` while the database is unchanged, else `read`.
    fn listing(
        &self,
        slot: fn(&mut Listings) -> &mut Option<Listing>,
        read: impl FnOnce() -> Result<Listing, String>,
    ) -> Result<Listing, String> {
        let state = self.listing_state();
        let mut l = self.listings.borrow_mut();
        if state.is_none() || l.at != state {
            *l = Listings {
                at: state,
                ..Default::default()
            };
        }
        if let Some(v) = slot(&mut l).as_ref().filter(|_| state.is_some()) {
            return Ok(v.clone());
        }
        let v = read()?;
        if state.is_some() {
            *slot(&mut l) = Some(v.clone());
        }
        Ok(v)
    }
}
impl Store {
    /// Direct access for workspace-level operations (import backups and undo).
    pub fn connection(&self) -> &Connection {
        &self.connection
    }
    pub fn preference(&self, key: &str) -> Result<Option<String>, String> {
        self.connection
            .query_row("SELECT value FROM preferences WHERE key=?1", [key], |row| {
                row.get(0)
            })
            .optional()
            .map_err(|e| e.to_string())
    }
    pub fn set_preference(&self, key: &str, value: &str) -> Result<(), String> {
        self.connection.execute("INSERT INTO preferences(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![key,value])
            .map(|_| ()).map_err(|e| e.to_string())
    }
    pub fn open(path: PathBuf) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut connection = Connection::open(&path).map_err(|e| e.to_string())?;
        // A workspace from a newer OpenXER is refused before anything is
        // written to it: the table setup below, baselines and reports would
        // otherwise change it in ways that build does not expect.
        let found = check_schema(workspace_db::schema(&connection)?)?;
        let new = connection
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name='projects'",
                [],
                |_| Ok(()),
            )
            .optional()
            .map_err(|e| e.to_string())?
            .is_none();
        connection.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; CREATE TABLE IF NOT EXISTS projects(id INTEGER PRIMARY KEY, source_name TEXT NOT NULL, project_key INTEGER NOT NULL, title TEXT NOT NULL, xer_content TEXT NOT NULL, updated_at TEXT DEFAULT CURRENT_TIMESTAMP, UNIQUE(source_name,project_key)); CREATE TABLE IF NOT EXISTS preferences(key TEXT PRIMARY KEY, value TEXT NOT NULL);").map_err(|e|e.to_string())?;
        let columns = connection
            .prepare("PRAGMA table_info(projects)")
            .map_err(|e| e.to_string())?
            .query_map([], |r| r.get::<_, String>(1))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        for name in ["source_windows_encoding", "level_resources"] {
            if !columns.iter().any(|column| column == name) {
                connection
                    .execute(
                        &format!(
                            "ALTER TABLE projects ADD COLUMN {name} INTEGER NOT NULL DEFAULT 0"
                        ),
                        [],
                    )
                    .map_err(|e| e.to_string())?;
            }
        }
        // A workspace an earlier OpenXER gave a shared dictionary (schema 2:
        // EPS codes in each project's text) is upgraded before it is used,
        // after a backup, in one transaction that proves every project's
        // dates first; on any failure nothing is changed and it is not opened.
        if found == workspace_db::EARLIER_DICTIONARY_SCHEMA {
            let backup = backup_path(&path, "openxer-before-upgrade")?;
            workspace_db::migrate(&mut connection, false, Some(&backup)).map_err(|e| {
                format!("This workspace was made by an earlier OpenXER and could not be upgraded, so nothing was changed: {e}")
            })?;
        }
        // A new workspace keeps one shared dictionary from the start; an
        // existing one is converted only when the user asks
        // (`migrate_dictionary`).
        if new {
            workspace_db::start_new(&connection)?;
        }
        crate::baselines::store::ensure(&connection)?;
        crate::reports::rpt::store::ensure(&connection)?;
        crate::reports::rpt::store::load_library(&connection)?;
        Ok(Self {
            connection,
            path,
            listings: Default::default(),
            marks: Default::default(),
        })
    }
    /// The workspace schema: 1 stores each project's whole text, 3 keeps the
    /// workspace rows in one shared dictionary (`workspace_db`).
    pub fn schema(&self) -> Result<i64, String> {
        let schema = check_schema(workspace_db::schema(&self.connection)?)?;
        if schema == workspace_db::EARLIER_DICTIONARY_SCHEMA {
            // Upgraded on open; only a change made since gets here.
            return Err("This workspace uses the dictionary layout of an earlier OpenXER (workspace schema 2); open it again to upgrade it.".into());
        }
        Ok(schema)
    }
    /// Whether projects share one workspace dictionary (schema 3). An
    /// unreadable or newer schema is an error, never "not shared": a caller
    /// that took it for a version 1 workspace would write whole project
    /// text into it.
    pub fn shared(&self) -> Result<bool, String> {
        Ok(self.schema()? >= workspace_db::DICTIONARY_SCHEMA)
    }
    /// What open project `source` `key` read from the dictionary.
    pub fn mark_of(&self, source: &str, key: i64) -> Option<Mark> {
        self.marks.borrow().get(&(source.to_string(), key)).cloned()
    }
    /// Note what a project written outside `save` now holds of the dictionary.
    pub fn set_mark(&self, key: (String, i64), mark: Mark) {
        self.marks.borrow_mut().insert(key, mark);
    }
    /// Forget what project `key` (source name, project key) read from the
    /// dictionary: it is being replaced by a project read from a file, whose
    /// rows are not the dictionary's even where their ids match. Its next
    /// save merges them by name and content and deletes nothing.
    pub fn forget_mark(&self, key: &(String, i64)) {
        self.marks.borrow_mut().remove(key);
    }
    /// Whether open project `key` (source name, project key) was read from
    /// or written to the dictionary in this session.
    pub fn has_mark(&self, key: &(String, i64)) -> bool {
        self.marks.borrow().contains_key(key)
    }
    /// The text to store for a project written straight into the `projects`
    /// table: with a shared dictionary its dictionary rows are merged in
    /// first and left out of the text.
    #[cfg(test)]
    pub fn stored_text(&self, doc: &Document, row: Option<i64>) -> Result<String, String> {
        if !self.shared()? {
            return Ok(doc.text());
        }
        Ok(workspace_db::store(&self.connection, doc, None, row)?.text)
    }
    /// The workspace dictionary's revision (0 before it has rows).
    pub fn dictionary_revision(&self) -> Result<i64, String> {
        workspace_db::read_revision(&self.connection)
    }
    /// The workspace dictionary and its revision.
    pub fn dictionary(&self) -> Result<(Document, i64), String> {
        workspace_db::read_dictionary(&self.connection)
    }
    /// Merge the workspace rows of open project `p` (workspace row `row`)
    /// into the dictionary right away, as an edit makes them, so the other
    /// projects see them. A row the project deleted is refused while another
    /// project uses it: a stored one, or one of `open` (the other open
    /// projects, with their workspace ids, which may hold unsaved uses).
    pub fn sync_dictionary(
        &mut self,
        p: &ProjectData,
        row: Option<i64>,
        open: &[(Option<i64>, &Document)],
    ) -> Result<Sync, String> {
        let key = (p.source_name.clone(), p.model.project.proj_id);
        let document = p.updated_document();
        let mark = self.marks.borrow().get(&key).cloned();
        let tx = immediate(&mut self.connection)?;
        let (before, _) = workspace_db::read_dictionary(&tx)?;
        let mut gone: Vec<(String, i64)> = vec![];
        if let Some(m) = &mark {
            let present = workspace_db::workspace_rows(&document);
            gone = m
                .rows
                .iter()
                .filter(|k| !present.contains(*k))
                .cloned()
                .collect();
            if !gone.is_empty() {
                let mut users = workspace_db::users(&tx, row)?;
                for (id, doc) in open {
                    if id.is_some() && *id == row {
                        continue;
                    }
                    for k in workspace_scope::referenced_ids(doc) {
                        users.entry(k).or_default().insert(id.unwrap_or(-1));
                    }
                }
                let titles = workspace_db::titles(&tx)?;
                let mut refused = vec![];
                for k in &gone {
                    let Some(ids) = users.get(k) else { continue };
                    let names: Vec<String> = ids
                        .iter()
                        .map(|i| {
                            titles
                                .get(i)
                                .cloned()
                                .unwrap_or_else(|| "an open project".into())
                        })
                        .collect();
                    refused.push(format!(
                        "{} is used by {} and stays: remove it from {} first.",
                        workspace_db::row_label(&before, &k.0, k.1),
                        names.join(", "),
                        if names.len() == 1 {
                            "that project"
                        } else {
                            "those projects"
                        }
                    ));
                }
                if !refused.is_empty() {
                    return Ok(Sync::Refused(refused.join(" ")));
                }
            }
        }
        let stored = workspace_db::store(&tx, &document, mark.as_ref(), row)?;
        // The project's saved text may still use a row this edit removed
        // from the dictionary (an assignment removed in an earlier step, not
        // saved yet). The dictionary change is written now and the text only
        // at the next save, so a crash in between would leave the saved
        // project pointing at nothing: its current text is written with it.
        if let Some(id) = row {
            let removed: Vec<&(String, i64)> = gone
                .iter()
                .filter(|k| !stored.kept_in_use.contains(*k))
                .collect();
            if !removed.is_empty() {
                let text: String = tx
                    .query_row("SELECT xer_content FROM projects WHERE id=?1", [id], |r| {
                        r.get(0)
                    })
                    .map_err(|e| e.to_string())?;
                let used = Document::parse(&text)
                    .map(|d| workspace_scope::referenced_ids(&d))
                    .unwrap_or_default();
                if removed.iter().any(|k| used.contains(*k)) {
                    tx.execute(
                        "UPDATE projects SET title=?1,xer_content=?2,updated_at=CURRENT_TIMESTAMP WHERE id=?3",
                        params![p.model.project.name, stored.text, id],
                    )
                    .map_err(|e| e.to_string())?;
                }
            }
        }
        let (after, revision) = workspace_db::read_dictionary(&tx)?;
        tx.commit().map_err(|e| e.to_string())?;
        let changed = stored.changed_document();
        self.marks.borrow_mut().insert(key, stored.mark);
        *self.listings.borrow_mut() = Listings::default();
        Ok(Sync::Done {
            delta: workspace_scope::delta::diff(&before, &after),
            document: changed.then_some(stored.outcome.document),
            revision,
        })
    }
    /// Put the current dictionary rows into open project `p` (workspace row
    /// `row`) in place of the ones it holds, keeping its own rows, and
    /// rebuild its calendars and resource inputs when those changed. On
    /// error `p` is unchanged.
    pub fn rehydrate(&self, p: &mut ProjectData, row: Option<i64>) -> Result<(), String> {
        let (dict, revision) = workspace_db::read_dictionary(&self.connection)?;
        let (mut doc, _) = workspace_scope::split(&p.document);
        let warnings = p.document.warnings.clone();
        let mark = workspace_db::hydrate_with(&self.connection, &mut doc, row, &dict, revision)?;
        doc.warnings = warnings;
        let old = std::mem::replace(&mut p.document, doc);
        let changed = |t: &str| match (p.document.table(t), old.table(t)) {
            (Some(a), Some(b)) => a.fields != b.fields || a.rows != b.rows,
            (None, None) => false,
            _ => true,
        };
        let calendars = changed("CALENDAR");
        let resources = calendars
            || [
                "RSRC", "RSRCRATE", "RSRCROLE", "ROLES", "ROLERATE", "SHIFT", "SHIFTPER",
            ]
            .iter()
            .any(|t| changed(t));
        // Only what the new rows change is rebuilt: the scheduling model
        // keeps its calculated values, as after an edit made here.
        if calendars {
            if let Err(e) = p.rebuild_calendars() {
                p.document = old;
                return Err(e);
            }
        }
        if resources {
            p.refresh_resource_inputs();
        }
        self.marks
            .borrow_mut()
            .insert((p.source_name.clone(), p.model.project.proj_id), mark);
        Ok(())
    }
    /// Apply a dictionary change forward (redo) or backward (undo). It is
    /// refused whole, with a message, when a row it changes was changed again
    /// since (by this or another project): putting the row back would
    /// silently revert that later change. Rows it would remove are refused
    /// while something uses them: a stored project other than `except`, one
    /// of `open` (the open projects with their workspace ids, which may hold
    /// unsaved uses), a dictionary row, or the saved text of `except` itself
    /// (its next save would otherwise point at nothing after a crash).
    pub fn apply_dictionary_delta(
        &mut self,
        delta: &Delta,
        forward: bool,
        except: Option<i64>,
        open: &[(Option<i64>, &Document)],
    ) -> Result<(), String> {
        let step = if forward { "redone" } else { "undone" };
        let tx = immediate(&mut self.connection)?;
        let (mut dict, _) = workspace_db::read_dictionary(&tx)?;
        let stale = delta.stale(&dict, forward);
        if !stale.is_empty() {
            let names: Vec<String> = stale
                .iter()
                .map(|(t, id)| workspace_db::row_label(&dict, t, *id))
                .collect();
            return Err(format!(
                "{} changed in the workspace dictionary after this step, so the step is not {step} there and the later change stays.",
                names.join(", ")
            ));
        }
        let mut used = workspace_db::removals_in_use(&tx, &dict, delta, forward, except, open)?;
        // The project the step belongs to: its open document (the step
        // already undone or redone in it) and its saved text. A row the
        // open document still uses stays. A row only the saved text still
        // uses (saved before the step was undone) goes, and the project's
        // current text is written with it, so a crash before the next save
        // never leaves the saved project pointing at nothing. When the
        // project is not open its saved text keeps the row.
        let mut rewrite: Option<(i64, (String, i64), &Document)> = None;
        if let Some(id) = except {
            let removes = delta.removes(forward);
            if !removes.is_empty() {
                let (text, title, source, key): (String, String, String, i64) = tx
                    .query_row(
                        "SELECT xer_content,title,source_name,project_key FROM projects WHERE id=?1",
                        [id],
                        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                    )
                    .optional()
                    .map_err(|e| e.to_string())?
                    .unwrap_or_default();
                let saved = Document::parse(&text)
                    .map(|d| workspace_scope::referenced_ids(&d))
                    .unwrap_or_default();
                let own = open
                    .iter()
                    .find(|(i, _)| i.is_some() && *i == except)
                    .map(|(_, d)| *d);
                let now = own.map(workspace_scope::referenced_ids);
                for k in removes {
                    match &now {
                        Some(now) if now.contains(&k) => {
                            used.entry(k).or_default().push(title.clone())
                        }
                        Some(_) if saved.contains(&k) => {
                            rewrite = own.map(|d| (id, (source.clone(), key), d))
                        }
                        None if saved.contains(&k) => used
                            .entry(k)
                            .or_default()
                            .push(format!("the saved copy of {title}")),
                        _ => {}
                    }
                }
            }
        }
        if let Some(((table, id), names)) = used.into_iter().next() {
            return Err(format!(
                "{} is used by {} now, so this step is not {step} in the workspace dictionary.",
                workspace_db::row_label(&dict, &table, id),
                names.join(", ")
            ));
        }
        delta.apply(&mut dict, forward);
        workspace_db::replace_dictionary(&tx, &dict)?;
        let mut mark = None;
        if let Some((id, key, doc)) = rewrite {
            let stored = workspace_db::store(&tx, doc, self.marks.borrow().get(&key), Some(id))?;
            tx.execute(
                "UPDATE projects SET xer_content=?1,updated_at=CURRENT_TIMESTAMP WHERE id=?2",
                params![stored.text, id],
            )
            .map_err(|e| e.to_string())?;
            mark = Some((key, stored.mark));
        }
        tx.commit().map_err(|e| e.to_string())?;
        if let Some((key, m)) = mark {
            self.marks.borrow_mut().insert(key, m);
        }
        *self.listings.borrow_mut() = Listings::default();
        Ok(())
    }
    /// The stored projects (workspace id and title) other than `except`
    /// that use dictionary row `table` `id`.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn dictionary_usage(
        &self,
        table: &str,
        id: i64,
        except: Option<i64>,
    ) -> Result<Vec<(i64, String)>, String> {
        workspace_db::dictionary_usage(&self.connection, table, id, except)
    }
    /// Plan, or apply, moving this workspace's projects to a shared
    /// workspace dictionary (`workspace_db::migrate`). A dry run changes
    /// nothing; applying first copies the database into `backups` beside it.
    /// Returns the JSON report; [`Store::migration_report`] gives its text.
    /// Admin > Share Dictionaries Across Projects runs it.
    pub fn migrate_dictionary(&mut self, dry_run: bool) -> Result<serde_json::Value, String> {
        let backup = if dry_run {
            None
        } else {
            Some(backup_path(&self.path, "openxer-before-dictionary")?)
        };
        let result = workspace_db::migrate(&mut self.connection, dry_run, backup.as_deref());
        self.marks.borrow_mut().clear();
        *self.listings.borrow_mut() = Listings::default();
        result
    }
    /// The plain-text part of a migration report, for a person to read.
    #[cfg(test)]
    pub fn migration_report(report: &serde_json::Value) -> String {
        report["report"].as_str().unwrap_or_default().to_string()
    }
    /// The workspace OpenXER opens when `--database` does not name one
    /// ([`Store::default_path_in`] for this process).
    pub fn default_path() -> PathBuf {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf));
        Self::default_path_in(
            exe_dir.as_deref(),
            std::env::var_os(crate::util::DATA_DIR_VAR),
            openxer_automation::locate::user_data_base(),
        )
    }
    /// The default workspace for a program in `exe_dir`, with `data_dir`
    /// the value of `OPENXER_DATA_DIR` and `local_appdata` the per-user
    /// base folder (`openxer_automation::locate::user_data_base`:
    /// `%LOCALAPPDATA%` on Windows, the XDG data folder elsewhere):
    /// - `OPENXER_DATA_DIR` set: `openxer.db` in that folder;
    /// - an installed copy (the installer's marker file beside the EXE):
    ///   `<local_appdata>/OpenXER/openxer.db`, never the program folder,
    ///   which an upgrade or uninstall replaces. A workspace a portable copy
    ///   left beside the EXE is still used while the per-user one does not
    ///   exist, so it never drops out of sight;
    /// - a portable copy: `openxer.db` beside the EXE when that folder can
    ///   be written, else `<local_appdata>/OpenXER/openxer.db`.
    pub fn default_path_in(
        exe_dir: Option<&Path>,
        data_dir: Option<std::ffi::OsString>,
        local_appdata: Option<std::ffi::OsString>,
    ) -> PathBuf {
        // One rule for the app and for agents (`find_schedules`).
        openxer_automation::locate::default_workspace_in(exe_dir, data_dir, local_appdata)
    }
    pub fn save(&mut self, p: &ProjectData) -> Result<i64, String> {
        self.save_stored(p).map(|(id, _)| id)
    }
    /// [`Store::save`], also returning the project's document as the
    /// workspace stored it when the dictionary merge changed it (a row
    /// renumbered because its id was taken, a renamed copy, a row matched to
    /// a stored one, an EPS code moved). The open project must take it
    /// ([`adopt_document`]): its activities and assignments refer to the new
    /// ids, and the next rehydrate would otherwise put the dictionary's row
    /// of the old id under them.
    pub fn save_stored(&mut self, p: &ProjectData) -> Result<(i64, Option<Document>), String> {
        let shared = self.schema()? >= workspace_db::DICTIONARY_SCHEMA;
        let key = (p.source_name.clone(), p.model.project.proj_id);
        let document = p.updated_document();
        let tx = immediate(&mut self.connection)?;
        let (text, mark) = if shared {
            let row: Option<i64> = tx
                .query_row(
                    "SELECT id FROM projects WHERE source_name=?1 AND project_key=?2",
                    params![key.0, key.1],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            let mut mark = self.marks.borrow().get(&key).cloned();
            let mut document = document;
            let mut refreshed = false;
            // The project read the dictionary at an older revision: another
            // writer changed it since. Its marked rows are written as the
            // project's own, so a stale copy would put the older rows back
            // for every project. It takes the dictionary's rows as they are
            // now first (as a rehydrate does; its own rows stay), and the
            // open project takes the result.
            if let Some(m) = mark.as_ref() {
                let (dict, revision) = workspace_db::read_dictionary(&tx)?;
                if m.revision != revision {
                    let (mut fresh, _) = workspace_scope::split(&document);
                    fresh.windows_encoding = document.windows_encoding;
                    fresh.warnings = document.warnings.clone();
                    mark = Some(workspace_db::hydrate_with(
                        &tx, &mut fresh, row, &dict, revision,
                    )?);
                    refreshed = fresh.text() != document.text();
                    document = fresh;
                }
            }
            let stored = workspace_db::store(&tx, &document, mark.as_ref(), row)?;
            let changed =
                (refreshed || stored.changed_document()).then_some(stored.outcome.document);
            (stored.text, Some((stored.mark, changed)))
        } else {
            (document.text(), None)
        };
        tx.execute("INSERT INTO projects(source_name,project_key,title,xer_content,source_windows_encoding,level_resources) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(source_name,project_key) DO UPDATE SET title=excluded.title,xer_content=excluded.xer_content,source_windows_encoding=excluded.source_windows_encoding,level_resources=excluded.level_resources,updated_at=CURRENT_TIMESTAMP",params![p.source_name,p.model.project.proj_id,p.model.project.name,text,p.document.windows_encoding,p.model.options.do_level]).map_err(|e|e.to_string())?;
        let id: i64 = tx
            .query_row(
                "SELECT id FROM projects WHERE source_name=?1 AND project_key=?2",
                params![p.source_name, p.model.project.proj_id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        tx.execute("INSERT INTO preferences(key,value) VALUES('last_project',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![id.to_string()]).map_err(|e|e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        let mut changed = None;
        if let Some((mark, document)) = mark {
            self.marks.borrow_mut().insert(key, mark);
            changed = document;
        }
        Ok((id, changed))
    }
    /// Give closed workspace project `id` an internal project number no
    /// workspace project, baseline or number in `also` uses, in one
    /// transaction: its row's key and every row of its stored text that
    /// carries the old number ([`numbering::renumber`]), and the
    /// relationships into it that projects read from the same file hold
    /// (they find it by that number). Returns (old, new). Refused, with
    /// nothing changed, while a recovery copy of a project from that file
    /// is waiting: the copy still holds the old number.
    ///
    /// [`numbering::renumber`]: openxer_xer::workspace::numbering::renumber
    pub fn give_fresh_number(&mut self, id: i64, also: &[i64]) -> Result<(i64, i64), String> {
        use openxer_xer::workspace::numbering;
        let tx = immediate(&mut self.connection)?;
        let (source, from, text): (String, i64, String) = tx
            .query_row(
                "SELECT source_name,project_key,xer_content FROM projects WHERE id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .map_err(|e| e.to_string())?;
        let recovery = tx
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name='recovery_copies'",
                [],
                |_| Ok(()),
            )
            .optional()
            .map_err(|e| e.to_string())?
            .is_some();
        if recovery
            && tx
                .query_row(
                    "SELECT 1 FROM recovery_copies WHERE source_name=?1",
                    [&source],
                    |_| Ok(()),
                )
                .optional()
                .map_err(|e| e.to_string())?
                .is_some()
        {
            return Err("a recovery copy of it is waiting".into());
        }
        let mut doc = Document::parse(&text)?;
        let mut siblings = vec![];
        {
            let mut stmt = tx
                .prepare("SELECT id,xer_content FROM projects WHERE source_name=?1 AND id<>?2")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(params![source, id], |r| {
                    Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
                })
                .map_err(|e| e.to_string())?;
            for row in rows {
                let (sibling, text) = row.map_err(|e| e.to_string())?;
                siblings.push((sibling, Document::parse(&text)?));
            }
        }
        let mut taken = also.to_vec();
        taken.extend(numbering::in_document(&doc));
        for (_, d) in &siblings {
            taken.extend(numbering::in_document(d));
        }
        let to = openxer_automation::baseline_store::next_project_number(&tx, &taken)?;
        numbering::renumber(&mut doc, from, to);
        tx.execute(
            "UPDATE projects SET project_key=?1,xer_content=?2,updated_at=CURRENT_TIMESTAMP WHERE id=?3",
            params![to, doc.text(), id],
        )
        .map_err(|e| e.to_string())?;
        for (sibling, mut d) in siblings {
            let before = d.text();
            numbering::renumber(&mut d, from, to);
            let after = d.text();
            if after != before {
                tx.execute(
                    "UPDATE projects SET xer_content=?1,updated_at=CURRENT_TIMESTAMP WHERE id=?2",
                    params![after, sibling],
                )
                .map_err(|e| e.to_string())?;
            }
        }
        tx.commit().map_err(|e| e.to_string())?;
        // What the project read from the dictionary is now under its new key.
        let mut marks = self.marks.borrow_mut();
        if let Some(mark) = marks.remove(&(source.clone(), from)) {
            marks.insert((source, to), mark);
        }
        Ok((from, to))
    }
    /// The workspace id of the project saved from `source` with project key `key`.
    pub fn id_of(&self, source: &str, key: i64) -> Option<i64> {
        self.connection
            .query_row(
                "SELECT id FROM projects WHERE source_name=?1 AND project_key=?2",
                params![source, key],
                |r| r.get(0),
            )
            .optional()
            .ok()
            .flatten()
    }
    pub fn list(&self) -> Result<Vec<(i64, String)>, String> {
        self.listing(|l| &mut l.list, || self.read_list())
    }
    fn read_list(&self) -> Result<Vec<(i64, String)>, String> {
        let mut stmt = self
            .connection
            .prepare("SELECT id,title FROM projects ORDER BY updated_at DESC,id DESC")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }
    /// Workspace rows with their last-save stamp, so views can cache summaries.
    pub fn stamps(&self) -> Result<Vec<(i64, String)>, String> {
        self.listing(|l| &mut l.stamps, || self.read_stamps())
    }
    fn read_stamps(&self) -> Result<Vec<(i64, String)>, String> {
        let mut stmt = self
            .connection
            .prepare("SELECT id,COALESCE(updated_at,'') FROM projects ORDER BY id")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }
    /// The stored source name, project key and XER text, without building a schedule model.
    pub fn source(&self, id: i64) -> Result<(String, i64, String), String> {
        self.connection
            .query_row(
                "SELECT source_name,project_key,xer_content FROM projects WHERE id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .map_err(|e| e.to_string())
    }
    /// The stored source name, project key and document, with the workspace
    /// dictionary's rows added on a schema 3 workspace (calendars, resources
    /// and codes the project uses), without building a schedule model. The
    /// Projects view rolls closed projects up from it.
    pub fn source_document(&self, id: i64) -> Result<(String, i64, Document), String> {
        let (source, key, text) = self.source(id)?;
        let mut doc = Document::parse(&text)?;
        if self.schema()? >= workspace_db::DICTIONARY_SCHEMA {
            workspace_db::hydrate(&self.connection, &mut doc, Some(id))?;
        }
        Ok((source, key, doc))
    }
    pub fn load(&self, id: i64) -> Result<ProjectData, String> {
        let (source, pid, text, legacy, level): (String, i64, String, bool, bool) = self
            .connection
            .query_row(
                "SELECT source_name,project_key,xer_content,source_windows_encoding,level_resources FROM projects WHERE id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .map_err(|e| e.to_string())?;
        let mut document = Document::parse(&text)?;
        document.windows_encoding = legacy;
        if self.schema()? >= workspace_db::DICTIONARY_SCHEMA {
            let mark = workspace_db::hydrate(&self.connection, &mut document, Some(id))?;
            self.marks.borrow_mut().insert((source.clone(), pid), mark);
        }
        let mut project = ProjectData::from_document(document, pid, source)?;
        project.model.options.do_level = level;
        Ok(project)
    }
    pub fn clear_last(&self) {
        let _ = self
            .connection
            .execute("DELETE FROM preferences WHERE key='last_project'", []);
    }
    pub fn last(&self) -> Result<Option<ProjectData>, String> {
        let id: Option<String> = self
            .connection
            .query_row(
                "SELECT value FROM preferences WHERE key='last_project'",
                [],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        let Some(s) = id else {
            return Ok(None);
        };
        let row_id: i64 = s.parse().map_err(|_| "Invalid saved project ID")?;
        let exists: Option<i64> = self
            .connection
            .query_row("SELECT 1 FROM projects WHERE id=?1", [row_id], |r| r.get(0))
            .optional()
            .map_err(|e| e.to_string())?;
        if exists.is_none() {
            self.clear_last();
            return Ok(None);
        }
        self.load(row_id).map(Some)
    }
}
/// One running app's hold on a workspace, so a second app on the same
/// database is refused instead of writing over the first one's dictionary
/// rows from its own, older copies. It is an operating system lock on
/// `<database>.lock` beside the database, released when the app exits, and
/// by the system when the app crashes, so a stale lock file never blocks.
pub struct WorkspaceLock {
    _file: std::fs::File,
}

impl WorkspaceLock {
    pub fn take(db: &Path) -> Result<Self, String> {
        if let Some(parent) = db.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut name = db.as_os_str().to_owned();
        name.push(".lock");
        let path = PathBuf::from(name);
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(|e| format!("The workspace lock {} cannot be made: {e}", path.display()))?;
        match file.try_lock() {
            Ok(()) => Ok(Self { _file: file }),
            Err(std::fs::TryLockError::WouldBlock) => Err(
                "This workspace is already open in another OpenXER window. Use that window, or close it and start OpenXER again.".into(),
            ),
            Err(std::fs::TryLockError::Error(e)) => Err(format!(
                "The workspace lock {} cannot be taken: {e}",
                path.display()
            )),
        }
    }
}

/// A write transaction that takes the database's write lock at once
/// (BEGIN IMMEDIATE), so it reads what no other writer can change before it
/// commits, and waits for the busy timeout instead of failing on a stale
/// snapshot.
fn immediate(c: &mut Connection) -> Result<rusqlite::Transaction<'_>, String> {
    c.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())
}
/// A new file in the `backups` folder beside workspace `db`, named
/// `<prefix>-<time stamp>.db`.
fn backup_path(db: &Path, prefix: &str) -> Result<PathBuf, String> {
    let dir = db
        .parent()
        .map(|p| p.join("backups"))
        .unwrap_or_else(|| PathBuf::from("backups"));
    std::fs::create_dir_all(&dir).map_err(|e| format!("Backup folder: {e}"))?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let mut path = dir.join(format!("{prefix}-{stamp}.db"));
    let mut n = 1;
    while path.exists() {
        path = dir.join(format!("{prefix}-{stamp}-{n}.db"));
        n += 1;
    }
    Ok(path)
}
/// `schema`, or an error when this build cannot work with it.
fn check_schema(schema: i64) -> Result<i64, String> {
    if schema > workspace_db::DICTIONARY_SCHEMA {
        return Err(format!(
            "This workspace uses workspace schema {schema}, which needs a newer OpenXER."
        ));
    }
    Ok(schema)
}
/// Make open project `p` hold `doc`, its document as the workspace stored
/// it ([`Store::save_stored`], [`Sync::Done`]). The project is built again
/// from it, as loading it would: activities, assignments and the project
/// itself refer to calendars and resources by id, and the merge may have
/// given those rows new ids. On error `p` is unchanged.
pub fn adopt_document(p: &mut ProjectData, mut doc: Document) -> Result<(), String> {
    doc.windows_encoding = p.document.windows_encoding;
    doc.warnings = p.document.warnings.clone();
    let mut q = ProjectData::from_document(doc, p.model.project.proj_id, p.source_name.clone())?;
    q.model.options.do_level = p.model.options.do_level;
    q.warnings = std::mem::take(&mut p.warnings);
    *p = q;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use openxer_xer::edit::{calendars, records, tables};

    fn temp_db(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("openxer-{name}-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        path
    }

    /// A workspace with a shared dictionary.
    fn shared_store(path: &Path) -> Store {
        let s = Store::open(path.to_path_buf()).unwrap();
        workspace_db::ensure(s.connection()).unwrap();
        s.set_preference(workspace_db::SCHEMA_KEY, "3").unwrap();
        s
    }

    fn project(source: &str) -> ProjectData {
        let mut p = openxer_xer::starter();
        p.source_name = source.into();
        p
    }

    fn rows_named(doc: &Document, table: &str, column: &str, name: &str) -> usize {
        doc.table(table).map_or(0, |t| {
            t.rows.iter().filter(|r| t.value(r, column) == name).count()
        })
    }

    fn rename_resource(p: &mut ProjectData, short: &str, name: &str) {
        let t = p.document.table_mut("RSRC");
        let i = t
            .rows
            .iter()
            .position(|r| t.value(r, "rsrc_short_name") == short)
            .unwrap();
        t.set(i, "rsrc_name", name.into());
    }

    #[test]
    fn workspace_rows_saved_in_one_project_are_found_when_another_loads() {
        let path = temp_db("shared-dictionary");
        let mut s = shared_store(&path);
        let a_id = s.save(&project("A.xer")).unwrap();
        let mut b = project("B.xer");
        let cal =
            calendars::add(&mut b, calendars::Kind::Global, None, "2026-11-08 09:00").unwrap();
        calendars::rename(&mut b, &cal, "Night shift").unwrap();
        tables::add_resource(&mut b, "WLD").unwrap();
        tables::add_code_type(&mut b, "Phase", "global", None).unwrap();
        let udf = records::next_id(&b.document, "UDFTYPE", "udf_type_id");
        records::add_row(
            &mut b.document,
            "UDFTYPE",
            &[
                ("udf_type_id", udf.to_string()),
                ("table_name", "TASK".into()),
                ("udf_type_name", "user_field_1".into()),
                ("udf_type_label", "Area lead".into()),
                ("logical_data_type", "FT_TEXT".into()),
            ],
        );
        let b_id = s.save(&b).unwrap();
        for id in [a_id, b_id] {
            let stored = s.source(id).unwrap().2;
            assert!(
                !stored.contains("Night shift"),
                "row {id} stores no workspace rows"
            );
            assert!(
                !stored.contains("Area lead"),
                "row {id} stores no workspace rows"
            );
        }
        let a = s.load(a_id).unwrap();
        assert_eq!(
            rows_named(&a.document, "CALENDAR", "clndr_name", "Night shift"),
            1
        );
        assert!(a.calendar_names.iter().any(|n| n == "Night shift"));
        assert_eq!(rows_named(&a.document, "RSRC", "rsrc_short_name", "WLD"), 1);
        assert_eq!(
            rows_named(&a.document, "ACTVTYPE", "actv_code_type", "Phase"),
            1
        );
        assert_eq!(
            rows_named(&a.document, "UDFTYPE", "udf_type_label", "Area lead"),
            1
        );
        assert_eq!(
            rows_named(
                &a.document,
                "CALENDAR",
                "clndr_name",
                "Standard 5-day / 8-hour"
            ),
            1,
            "the calendar both projects had is stored once"
        );
        // A change made in A reaches B.
        let mut a = a;
        rename_resource(&mut a, "WLD", "Welder");
        s.save(&a).unwrap();
        let mut b = s.load(b_id).unwrap();
        assert_eq!(rows_named(&b.document, "RSRC", "rsrc_name", "Welder"), 1);
        // A new row gets an id above every id the workspace has used.
        let high = b.document.id_floor_of("CALENDAR");
        assert!(high >= cal.parse::<i64>().unwrap());
        let next =
            calendars::add(&mut b, calendars::Kind::Global, None, "2026-11-08 10:00").unwrap();
        assert!(next.parse::<i64>().unwrap() > high);
        drop(s);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a_deleted_workspace_row_goes_unless_another_project_uses_it() {
        let path = temp_db("shared-delete");
        let mut s = shared_store(&path);
        let mut a = project("A.xer");
        tables::add_resource(&mut a, "WLD").unwrap();
        tables::add_resource(&mut a, "CRN").unwrap();
        tables::add_code_type(&mut a, "Phase", "global", None).unwrap();
        let a_id = s.save(&a).unwrap();
        let b_id = s.save(&project("B.xer")).unwrap();
        // B uses the crane.
        let mut b = s.load(b_id).unwrap();
        let crane = b
            .document
            .table("RSRC")
            .and_then(|t| {
                t.rows
                    .iter()
                    .find(|r| t.value(r, "rsrc_short_name") == "CRN")
                    .map(|r| t.value(r, "rsrc_id").to_string())
            })
            .unwrap();
        let task = *b.details.keys().next().unwrap();
        records::add_row(
            &mut b.document,
            "TASKRSRC",
            &[
                ("taskrsrc_id", "1".into()),
                ("task_id", task.to_string()),
                ("proj_id", "1".into()),
                ("rsrc_id", crane),
            ],
        );
        s.save(&b).unwrap();
        // A deletes all three.
        let mut a = s.load(a_id).unwrap();
        for (table, column, name) in [
            ("RSRC", "rsrc_short_name", "WLD"),
            ("RSRC", "rsrc_short_name", "CRN"),
            ("ACTVTYPE", "actv_code_type", "Phase"),
        ] {
            let t = a.document.table_mut(table);
            let c = t.field(column).unwrap();
            t.rows.retain(|r| r[c] != name);
        }
        s.save(&a).unwrap();
        let b = s.load(b_id).unwrap();
        assert_eq!(rows_named(&b.document, "RSRC", "rsrc_short_name", "WLD"), 0);
        assert_eq!(
            rows_named(&b.document, "ACTVTYPE", "actv_code_type", "Phase"),
            0
        );
        assert_eq!(
            rows_named(&b.document, "RSRC", "rsrc_short_name", "CRN"),
            1,
            "B still uses the crane"
        );
        drop(s);
        let _ = std::fs::remove_file(path);
    }

    /// A recovery copy an earlier run left (the open_set::recovery table).
    fn add_recovery_copy(c: &Connection) {
        c.execute_batch(
            "CREATE TABLE IF NOT EXISTS recovery_copies(session TEXT NOT NULL, source_name TEXT NOT NULL, project_key INTEGER NOT NULL, title TEXT NOT NULL, xer_content TEXT NOT NULL, windows_encoding INTEGER NOT NULL, level_resources INTEGER NOT NULL, saved_at TEXT NOT NULL, PRIMARY KEY(session, source_name, project_key));
             INSERT INTO recovery_copies VALUES('earlier','A.xer',1,'A','text',0,0,'2026-11-09 10:00');",
        )
        .unwrap();
    }

    fn recovery_copies(c: &Connection) -> i64 {
        c.query_row("SELECT COUNT(*) FROM recovery_copies", [], |r| r.get(0))
            .unwrap()
    }

    /// A workspace an earlier build gave a shared dictionary (schema 2): the
    /// dictionary holds the shared rows except the EPS code types, which
    /// stay in each project's text. Built here from whole projects: the
    /// dictionary takes their resources, the project text keeps the rest.
    fn earlier_dictionary_workspace(path: &Path, projects: &[ProjectData]) -> Vec<i64> {
        let c = Connection::open(path).unwrap();
        c.execute_batch("CREATE TABLE projects(id INTEGER PRIMARY KEY, source_name TEXT NOT NULL, project_key INTEGER NOT NULL, title TEXT NOT NULL, xer_content TEXT NOT NULL, updated_at TEXT DEFAULT CURRENT_TIMESTAMP, source_windows_encoding INTEGER NOT NULL DEFAULT 0, level_resources INTEGER NOT NULL DEFAULT 0, UNIQUE(source_name,project_key)); CREATE TABLE preferences(key TEXT PRIMARY KEY, value TEXT NOT NULL); INSERT INTO preferences VALUES('workspace_schema','2');").unwrap();
        workspace_db::ensure(&c).unwrap();
        let mut dict: Option<Document> = None;
        let mut ids = vec![];
        for p in projects {
            let whole = p.updated_document();
            let (_, shared) = workspace_scope::split(&whole);
            let mut rsrc = shared.clone();
            rsrc.tables.retain(|t| t.name == "RSRC");
            let mut own = whole.clone();
            own.table_mut("RSRC").rows.clear();
            match &mut dict {
                None => dict = Some(rsrc),
                Some(d) => {
                    let t = d.table_mut("RSRC");
                    for r in &rsrc.table("RSRC").unwrap().rows {
                        if !t.rows.contains(r) {
                            t.rows.push(r.clone());
                        }
                    }
                }
            }
            c.execute(
                "INSERT INTO projects(source_name,project_key,title,xer_content) VALUES(?1,1,?2,?3)",
                params![p.source_name, p.model.project.name, own.text()],
            )
            .unwrap();
            ids.push(c.last_insert_rowid());
        }
        c.execute(
            "INSERT INTO workspace_dictionary(id,xer_content,revision) VALUES(1,?1,4)",
            [dict.unwrap().text()],
        )
        .unwrap();
        ids
    }

    /// Schema marker: a schema 2 workspace an earlier build made is upgraded
    /// when it opens, after a backup: its EPS code types move into the
    /// dictionary, every project keeps its rows and schedule, and the
    /// backup is the workspace as it was. A schema 1 workspace still
    /// converts only through the conversion.
    #[test]
    fn an_earlier_shared_dictionary_workspace_is_upgraded_with_a_backup() {
        let dir = std::env::temp_dir().join(format!(
            "openxer-upgrade-{}-{}",
            std::process::id(),
            chrono::Local::now().format("%H%M%S%f")
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("openxer.db");
        let mut a = project("A.xer");
        tables::add_resource(&mut a, "WLD").unwrap();
        tables::add_code_type(&mut a, "Unit", "eps", None).unwrap();
        let mut b = project("B.xer");
        b.model.project.name = "BRAVO".into();
        tables::add_resource(&mut b, "WLD").unwrap();
        let ids = earlier_dictionary_workspace(&path, &[a.clone(), b.clone()]);
        add_recovery_copy(&Connection::open(&path).unwrap());
        let unit_in_text = |s: &Store, id: i64| s.source(id).unwrap().2.contains("\tUnit\t");
        let s = Store::open(path.clone()).unwrap();
        assert_eq!(s.schema().unwrap(), workspace_db::DICTIONARY_SCHEMA);
        let (dict, _) = s.dictionary().unwrap();
        assert_eq!(rows_named(&dict, "ACTVTYPE", "actv_code_type", "Unit"), 1);
        assert!(!unit_in_text(&s, ids[0]), "the EPS code left A's text");
        let a2 = s.load(ids[0]).unwrap();
        let b2 = s.load(ids[1]).unwrap();
        assert_eq!(a2.model, a.model);
        assert_eq!(b2.model, b.model);
        assert_eq!(
            rows_named(&a2.document, "ACTVTYPE", "actv_code_type", "Unit"),
            1
        );
        assert_eq!(
            rows_named(&a2.document, "RSRC", "rsrc_short_name", "WLD"),
            1
        );
        drop(s);
        let backups: Vec<PathBuf> = std::fs::read_dir(dir.join("backups"))
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        assert_eq!(backups.len(), 1, "one backup");
        let c = Connection::open(&backups[0]).unwrap();
        assert_eq!(
            workspace_db::schema(&c).unwrap(),
            2,
            "the backup is as it was"
        );
        assert_eq!(recovery_copies(&c), 1, "the backup keeps the recovery copy");
        let text: String = c
            .query_row(
                "SELECT xer_content FROM projects WHERE id=?1",
                [ids[0]],
                |r| r.get(0),
            )
            .unwrap();
        assert!(text.contains("\tUnit\t"));
        drop(c);
        // Opened again: nothing more to do.
        let s = Store::open(path.clone()).unwrap();
        assert_eq!(s.schema().unwrap(), workspace_db::DICTIONARY_SCHEMA);
        assert_eq!(
            recovery_copies(s.connection()),
            1,
            "the upgrade keeps the recovery copy"
        );
        drop(s);
        assert_eq!(std::fs::read_dir(dir.join("backups")).unwrap().count(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// B4/B7 for undo: a resource added and assigned, saved, then both
    /// undone in the open project. Undoing the dictionary step is not
    /// refused because the saved text still has the assignment; the
    /// project's current text is written with it, so the saved project never
    /// points at a resource the workspace no longer has. With the project
    /// closed, its saved text keeps the resource.
    #[test]
    fn undoing_an_added_resource_after_a_save_rewrites_the_saved_project() {
        let path = temp_db("undo-after-save");
        let mut s = shared_store(&path);
        let bare = project("A.xer");
        let id = s.save(&bare).unwrap();
        let mut a = s.load(id).unwrap();
        let (before, _) = s.dictionary().unwrap();
        tables::add_resource(&mut a, "WLD").unwrap();
        let rsrc = a
            .document
            .table("RSRC")
            .and_then(|t| {
                t.rows
                    .iter()
                    .find(|r| t.value(r, "rsrc_short_name") == "WLD")
                    .map(|r| t.value(r, "rsrc_id").to_string())
            })
            .unwrap();
        let task = *a.details.keys().next().unwrap();
        records::add_row(
            &mut a.document,
            "TASKRSRC",
            &[
                ("taskrsrc_id", "1".into()),
                ("task_id", task.to_string()),
                ("proj_id", "1".into()),
                ("rsrc_id", rsrc.clone()),
            ],
        );
        s.save(&a).unwrap();
        let (after, _) = s.dictionary().unwrap();
        let delta = workspace_scope::delta::diff(&before, &after);
        // Closed: the saved copy keeps the welder.
        let refused = s.apply_dictionary_delta(&delta, false, Some(id), &[]);
        assert!(refused.unwrap_err().contains("saved copy"));
        // Open, with the steps undone: the welder goes and the saved text follows.
        let undone = s.load(id).unwrap();
        let mut undone_doc = undone.updated_document();
        undone_doc.table_mut("TASKRSRC").rows.clear();
        let t = undone_doc.table_mut("RSRC");
        let c = t.field("rsrc_short_name").unwrap();
        t.rows.retain(|r| r[c] != "WLD");
        s.apply_dictionary_delta(&delta, false, Some(id), &[(Some(id), &undone_doc)])
            .unwrap();
        let (dict, _) = s.dictionary().unwrap();
        assert_eq!(rows_named(&dict, "RSRC", "rsrc_short_name", "WLD"), 0);
        let saved = Document::parse(&s.source(id).unwrap().2).unwrap();
        assert!(
            !workspace_scope::referenced_ids(&saved)
                .contains(&("RSRC".to_string(), rsrc.parse().unwrap())),
            "the saved project no longer uses the welder"
        );
        drop(s);
        let _ = std::fs::remove_file(path);
    }

    /// B9: a project read from the dictionary before another writer changed
    /// a row it holds is saved without putting the older row back, and the
    /// saved project takes the newer row.
    #[test]
    fn a_stale_copy_never_reverts_a_newer_dictionary_row() {
        let path = temp_db("stale-mark");
        let mut first = shared_store(&path);
        let a_id = first.save(&project("A.xer")).unwrap();
        let b_id = first.save(&project("B.xer")).unwrap();
        let a = first.load(a_id).unwrap();
        // Another writer renames the shared calendar.
        let mut second = Store::open(path.clone()).unwrap();
        let mut b = second.load(b_id).unwrap();
        let standard = b
            .document
            .table("CALENDAR")
            .map(|t| t.value(&t.rows[0], "clndr_id").to_string())
            .unwrap();
        calendars::rename(&mut b, &standard, "Day shift").unwrap();
        second.save(&b).unwrap();
        drop(second);
        // The first writer saves its stale copy of A.
        let (_, stored) = first.save_stored(&a).unwrap();
        let (dict, _) = first.dictionary().unwrap();
        assert_eq!(
            rows_named(&dict, "CALENDAR", "clndr_name", "Day shift"),
            1,
            "the newer name stays"
        );
        let stored = stored.expect("A takes the newer row");
        assert_eq!(
            rows_named(&stored, "CALENDAR", "clndr_name", "Day shift"),
            1
        );
        assert_eq!(
            rows_named(
                &first.load(b_id).unwrap().document,
                "CALENDAR",
                "clndr_name",
                "Day shift"
            ),
            1
        );
        drop(first);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a_shared_dictionary_workspace_survives_reopen() {
        let path = temp_db("shared-reopen");
        let mut whole = project("A.xer");
        whole.details.get_mut(&1000).unwrap().name = "Persisted change".into();
        tables::add_resource(&mut whole, "WLD").unwrap();
        {
            let mut s = shared_store(&path);
            s.save(&whole).unwrap();
        }
        {
            let s = Store::open(path.clone()).unwrap();
            assert_eq!(s.schema().unwrap(), 3);
            let p = s.last().unwrap().unwrap();
            assert_eq!(p.details[&1000].name, "Persisted change");
            assert_eq!(rows_named(&p.document, "RSRC", "rsrc_short_name", "WLD"), 1);
            // One project alone hydrates back to the text it was saved from.
            assert_eq!(p.updated_document().text(), whole.updated_document().text());
        }
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a_version_1_workspace_keeps_whole_project_text() {
        let path = temp_db("whole-text");
        let mut s = Store::open_version_1(path.clone());
        assert_eq!(s.schema().unwrap(), 1);
        let mut p = project("A.xer");
        tables::add_resource(&mut p, "WLD").unwrap();
        let id = s.save(&p).unwrap();
        assert_eq!(s.source(id).unwrap().2, p.updated_document().text());
        let loaded = s.load(id).unwrap();
        assert_eq!(loaded.document.text(), p.updated_document().text());
        let tables: i64 = s
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name='workspace_dictionary'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(tables, 0);
        drop(s);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn migration_previews_then_moves_projects_to_a_shared_dictionary() {
        let path = temp_db("migrate");
        let mut s = Store::open_version_1(path.clone());
        let mut a = project("A.xer");
        tables::add_resource(&mut a, "WLD").unwrap();
        let a_id = s.save(&a).unwrap();
        add_recovery_copy(s.connection());
        let mut b = project("B.xer");
        b.model.project.name = "BRAVO".into();
        // B's welder is a different resource of the same name.
        tables::add_resource(&mut b, "WLD").unwrap();
        rename_resource(&mut b, "WLD", "Welder, night");
        let b_id = s.save(&b).unwrap();
        let before = [s.source(a_id).unwrap().2, s.source(b_id).unwrap().2];
        let preview = s.migrate_dictionary(true).unwrap();
        assert_eq!(preview["dry_run"], true);
        assert_eq!(s.schema().unwrap(), 1, "a preview changes nothing");
        assert_eq!(
            [s.source(a_id).unwrap().2, s.source(b_id).unwrap().2],
            before
        );
        let text = Store::migration_report(&preview);
        assert!(
            text.contains("\"WLD\" is kept as \"WLD (BRAVO)\""),
            "{text}"
        );
        let applied = s.migrate_dictionary(false).unwrap();
        assert_eq!(s.schema().unwrap(), 3);
        let backup = PathBuf::from(applied["backup"].as_str().unwrap());
        assert!(backup.exists());
        assert_eq!(
            recovery_copies(s.connection()),
            1,
            "the conversion keeps the recovery copy"
        );
        let a2 = s.load(a_id).unwrap();
        let b2 = s.load(b_id).unwrap();
        for p in [&a2, &b2] {
            assert_eq!(rows_named(&p.document, "RSRC", "rsrc_short_name", "WLD"), 1);
            assert_eq!(
                rows_named(&p.document, "RSRC", "rsrc_short_name", "WLD (BRAVO)"),
                1
            );
        }
        assert_eq!(a2.model, a.model);
        assert_eq!(b2.model, b.model);
        // The backup is the workspace as it was.
        let old = Store::open(backup.clone()).unwrap();
        assert_eq!(old.schema().unwrap(), 1);
        assert_eq!(
            recovery_copies(old.connection()),
            1,
            "and so does the backup"
        );
        assert_eq!(old.source(a_id).unwrap().2, before[0]);
        drop(old);
        // Migrating again does nothing.
        assert_eq!(
            s.migrate_dictionary(true).unwrap()["already_migrated"],
            true
        );
        drop(s);
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(backup);
    }

    #[test]
    fn a_new_workspace_shares_its_dictionaries_and_an_old_one_stays_as_it_is() {
        let path = temp_db("new-schema");
        let s = Store::open(path.clone()).unwrap();
        assert_eq!(s.schema().unwrap(), 3);
        drop(s);
        assert_eq!(Store::open(path.clone()).unwrap().schema().unwrap(), 3);
        let _ = std::fs::remove_file(&path);
        let old = Store::open_version_1(path.clone());
        assert_eq!(old.schema().unwrap(), 1);
        drop(old);
        assert_eq!(
            Store::open(path.clone()).unwrap().schema().unwrap(),
            1,
            "never converted on open"
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a_newer_workspace_schema_is_refused() {
        let path = temp_db("newer-schema");
        let mut s = Store::open(path.clone()).unwrap();
        s.set_preference(workspace_db::SCHEMA_KEY, "99").unwrap();
        assert!(s
            .save(&project("A.xer"))
            .unwrap_err()
            .contains("newer OpenXER"));
        // Every path that asks whether the dictionary is shared gets the
        // error, so none writes whole project text into it (B10).
        assert!(s.shared().unwrap_err().contains("newer OpenXER"));
        assert!(s
            .stored_text(&project("B.xer").updated_document(), None)
            .is_err());
        drop(s);
        let _ = std::fs::remove_file(path);
    }

    /// B10: a workspace a newer OpenXER made is refused on open, before
    /// this build writes anything to it (tables, baselines, reports).
    #[test]
    fn a_workspace_from_a_newer_build_is_refused_before_anything_is_written() {
        let path = temp_db("newer-untouched");
        {
            let c = Connection::open(&path).unwrap();
            c.execute_batch(
                "CREATE TABLE projects(id INTEGER PRIMARY KEY, source_name TEXT NOT NULL, project_key INTEGER NOT NULL, title TEXT NOT NULL, xer_content TEXT NOT NULL, updated_at TEXT, UNIQUE(source_name,project_key));
                 CREATE TABLE preferences(key TEXT PRIMARY KEY, value TEXT NOT NULL);
                 INSERT INTO preferences VALUES('workspace_schema','99');",
            )
            .unwrap();
        }
        let before = std::fs::read(&path).unwrap();
        let error = Store::open(path.clone()).err().expect("refused");
        assert!(error.contains("newer OpenXER"), "{error}");
        assert_eq!(std::fs::read(&path).unwrap(), before, "nothing written");
        let _ = std::fs::remove_file(path);
    }
    #[test]
    fn legacy_encoding_and_level_setting_survive_reopen() {
        let base = std::env::temp_dir().join(format!("openxer-legacy-{}", std::process::id()));
        let database = base.with_extension("db");
        let export = base.with_extension("xer");
        {
            let mut store = Store::open(database.clone()).unwrap();
            let mut p = openxer_xer::starter();
            p.document.windows_encoding = true;
            p.model.options.do_level = true;
            p.details.get_mut(&1000).unwrap().name = "Café equipment".into();
            store.save(&p).unwrap();
        }
        {
            let store = Store::open(database.clone()).unwrap();
            let p = store.last().unwrap().unwrap();
            assert!(p.document.windows_encoding);
            assert!(p.model.options.do_level);
            p.updated_document().write(&export).unwrap();
            let bytes = std::fs::read(&export).unwrap();
            assert!(bytes.windows(4).any(|b| b == b"Caf\xe9"));
            assert!(!bytes.windows(5).any(|b| b == b"Caf\xc3\xa9"));
            assert_eq!(Document::read(&export).unwrap().projects()[0].0, 1);
        }
        let _ = std::fs::remove_file(database);
        let _ = std::fs::remove_file(export);
    }
    #[test]
    fn storage_survives_reopen() {
        let path = std::env::temp_dir().join(format!("openxer-storage-{}.db", std::process::id()));
        {
            let mut s = Store::open(path.clone()).unwrap();
            let mut p = openxer_xer::starter();
            p.details.get_mut(&1000).unwrap().name = "Persisted change".into();
            s.save(&p).unwrap();
        }
        {
            let s = Store::open(path.clone()).unwrap();
            assert_eq!(
                s.last().unwrap().unwrap().details[&1000].name,
                "Persisted change"
            );
        }
        let _ = std::fs::remove_file(path);
    }
}
