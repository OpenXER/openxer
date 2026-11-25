//! Workspace storage of report definitions, saved batches, and import and
//! export of definition files and of reports in a copy of the reference
//! database. Definitions are kept as their text, so what is exported is what
//! was imported unless it was edited.
use super::stream::Stream;
use crate::reports::groups::{self, Group};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use std::path::Path;

/// Report type of the reports OpenXER ships.
pub const KIND_LIBRARY: &str = "RP_OpenXER";
/// Report type of reports made, copied or imported by the user.
pub const KIND_USER: &str = "RP_User";
/// Reports in the reference database that were made by users.
pub const REFERENCE_USER_TYPE: &str = "RP_User";
/// Preference listing the library reports already loaded into the workspace.
pub const LOADED_KEY: &str = "report_library_loaded_v1";
/// Name of the group the library reports are put in.
pub const LIBRARY_GROUP: &str = "Scheduling Reports";

pub fn ensure(db: &Connection) -> Result<(), String> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS report_defs(
            rpt_id INTEGER PRIMARY KEY AUTOINCREMENT,
            rpt_name TEXT NOT NULL,
            rpt_type TEXT NOT NULL,
            group_id INTEGER,
            lib_id TEXT,
            rpt_data BLOB NOT NULL
         );
         CREATE TABLE IF NOT EXISTS report_batches(
            batch_id INTEGER PRIMARY KEY AUTOINCREMENT,
            batch_name TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS report_batch_items(
            batch_id INTEGER NOT NULL REFERENCES report_batches(batch_id) ON DELETE CASCADE,
            rpt_id INTEGER NOT NULL,
            seq_num INTEGER NOT NULL
         );",
    )
    .map_err(|e| e.to_string())?;
    // Columns added after the first release: scope, the project a project
    // report belongs to, and the last run.
    for (table, column, definition) in [
        ("report_defs", "scope", "TEXT NOT NULL DEFAULT 'global'"),
        ("report_defs", "scope_project", "TEXT"),
        ("report_defs", "last_run", "TEXT"),
        ("report_batches", "scope", "TEXT NOT NULL DEFAULT 'global'"),
        ("report_batches", "scope_project", "TEXT"),
    ] {
        let present: i64 = db
            .query_row(
                &format!("SELECT COUNT(*) FROM pragma_table_info('{table}') WHERE name='{column}'"),
                [],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if present == 0 {
            db.execute_batch(&format!(
                "ALTER TABLE {table} ADD COLUMN {column} {definition}"
            ))
            .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Who may run a report: any project (global) or the one project it was made
/// for.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ReportScope {
    #[default]
    Global,
    /// The project ID of the project the report belongs to.
    Project(String),
}

impl ReportScope {
    pub fn caption(&self) -> &'static str {
        match self {
            ReportScope::Global => "Global",
            ReportScope::Project(_) => "Project",
        }
    }
    pub fn is_global(&self) -> bool {
        matches!(self, ReportScope::Global)
    }
    /// The stored pair: the scope word and the project ID.
    pub fn to_db(&self) -> (&'static str, Option<&str>) {
        match self {
            ReportScope::Global => ("global", None),
            ReportScope::Project(p) => ("project", Some(p)),
        }
    }
    pub fn from_db(scope: &str, project: Option<String>) -> ReportScope {
        match (scope, project) {
            ("project", Some(p)) => ReportScope::Project(p),
            _ => ReportScope::Global,
        }
    }
    /// A report of this scope can run on the project with ID `open`.
    pub fn runs_on(&self, open: &str) -> bool {
        match self {
            ReportScope::Global => true,
            ReportScope::Project(p) => p == open,
        }
    }
}

/// A stored report without its text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stored {
    pub id: i64,
    pub name: String,
    pub kind: String,
    pub group: Option<u32>,
    pub lib_id: Option<String>,
    pub scope: ReportScope,
    /// When the report last ran: `YYYY-MM-DD HH:MM:SS`, local time.
    pub last_run: Option<String>,
}

pub fn list(db: &Connection) -> Result<Vec<Stored>, String> {
    let mut q = db
        .prepare(
            "SELECT rpt_id, rpt_name, rpt_type, group_id, lib_id, scope, scope_project, last_run
             FROM report_defs ORDER BY rpt_id",
        )
        .map_err(|e| e.to_string())?;
    let rows = q
        .query_map([], |r| {
            Ok(Stored {
                id: r.get(0)?,
                name: r.get(1)?,
                kind: r.get(2)?,
                group: r
                    .get::<_, Option<i64>>(3)?
                    .and_then(|g| u32::try_from(g).ok()),
                lib_id: r.get(4)?,
                scope: ReportScope::from_db(&r.get::<_, String>(5)?, r.get(6)?),
                last_run: r.get(7)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

pub fn load(db: &Connection, id: i64) -> Result<Vec<u8>, String> {
    db.query_row(
        "SELECT rpt_data FROM report_defs WHERE rpt_id=?1",
        [id],
        |r| r.get::<_, Vec<u8>>(0),
    )
    .optional()
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "That report is no longer in the workspace.".to_string())
}

pub fn insert(
    db: &Connection,
    name: &str,
    kind: &str,
    group: Option<u32>,
    lib_id: Option<&str>,
    data: &[u8],
) -> Result<i64, String> {
    db.execute(
        "INSERT INTO report_defs(rpt_name, rpt_type, group_id, lib_id, rpt_data) VALUES(?1,?2,?3,?4,?5)",
        params![name, kind, group.map(i64::from), lib_id, data],
    )
    .map_err(|e| e.to_string())?;
    Ok(db.last_insert_rowid())
}

pub fn update_data(db: &Connection, id: i64, data: &[u8]) -> Result<(), String> {
    db.execute(
        "UPDATE report_defs SET rpt_data=?2 WHERE rpt_id=?1",
        params![id, data],
    )
    .map(|_| ())
    .map_err(|e| e.to_string())
}

pub fn rename(db: &Connection, id: i64, name: &str) -> Result<(), String> {
    db.execute(
        "UPDATE report_defs SET rpt_name=?2 WHERE rpt_id=?1",
        params![id, name],
    )
    .map(|_| ())
    .map_err(|e| e.to_string())
}

pub fn set_group(db: &Connection, id: i64, group: Option<u32>) -> Result<(), String> {
    db.execute(
        "UPDATE report_defs SET group_id=?2 WHERE rpt_id=?1",
        params![id, group.map(i64::from)],
    )
    .map(|_| ())
    .map_err(|e| e.to_string())
}

pub fn set_scope(db: &Connection, id: i64, scope: &ReportScope) -> Result<(), String> {
    let (word, project) = scope.to_db();
    db.execute(
        "UPDATE report_defs SET scope=?2, scope_project=?3 WHERE rpt_id=?1",
        params![id, word, project],
    )
    .map(|_| ())
    .map_err(|e| e.to_string())
}

pub fn set_last_run(db: &Connection, id: i64, stamp: &str) -> Result<(), String> {
    db.execute(
        "UPDATE report_defs SET last_run=?2 WHERE rpt_id=?1",
        params![id, stamp],
    )
    .map(|_| ())
    .map_err(|e| e.to_string())
}

pub fn delete(db: &Connection, id: i64) -> Result<(), String> {
    db.execute("DELETE FROM report_defs WHERE rpt_id=?1", [id])
        .map_err(|e| e.to_string())?;
    db.execute("DELETE FROM report_batch_items WHERE rpt_id=?1", [id])
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// The name of a definition: its report name, else its title.
pub fn name_of(s: &Stream) -> String {
    let n = s.root.string("ReportName").unwrap_or_default();
    let n = n.trim();
    if n.is_empty() {
        s.root
            .string("Title")
            .unwrap_or_default()
            .trim()
            .to_string()
    } else {
        n.to_string()
    }
}

/// A name no stored report has: `base`, else `base (2)` ...
pub fn free_name(existing: &[Stored], base: &str) -> String {
    let base = if base.trim().is_empty() {
        "Report"
    } else {
        base.trim()
    };
    if !existing.iter().any(|s| s.name == base) {
        return base.to_string();
    }
    (2..)
        .map(|n| format!("{base} ({n})"))
        .find(|c| !existing.iter().any(|s| s.name == *c))
        .expect("an unused number exists")
}

// --------------------------------------------------------------- batches

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Batch {
    pub id: i64,
    pub name: String,
    /// A global batch holds global reports; a project batch holds the
    /// reports of that project.
    pub scope: ReportScope,
    /// Stored report ids in run order.
    pub reports: Vec<i64>,
}

pub fn batches(db: &Connection) -> Result<Vec<Batch>, String> {
    let mut out: Vec<Batch> = vec![];
    let mut q = db
        .prepare(
            "SELECT batch_id, batch_name, scope, scope_project FROM report_batches ORDER BY batch_id",
        )
        .map_err(|e| e.to_string())?;
    let rows = q
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                ReportScope::from_db(&r.get::<_, String>(2)?, r.get(3)?),
            ))
        })
        .map_err(|e| e.to_string())?;
    for r in rows {
        let (id, name, scope) = r.map_err(|e| e.to_string())?;
        out.push(Batch {
            id,
            name,
            scope,
            reports: vec![],
        });
    }
    for b in &mut out {
        let mut q = db
            .prepare(
                "SELECT rpt_id FROM report_batch_items WHERE batch_id=?1 ORDER BY seq_num, rowid",
            )
            .map_err(|e| e.to_string())?;
        b.reports = q
            .query_map([b.id], |r| r.get(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?;
    }
    Ok(out)
}

pub fn save_batch(db: &Connection, name: &str, reports: &[i64]) -> Result<i64, String> {
    save_batch_scoped(db, name, &ReportScope::Global, reports)
}

/// Saves (or replaces) the batch called `name` with its scope and reports in
/// run order.
pub fn save_batch_scoped(
    db: &Connection,
    name: &str,
    scope: &ReportScope,
    reports: &[i64],
) -> Result<i64, String> {
    let (word, project) = scope.to_db();
    let existing: Option<i64> = db
        .query_row(
            "SELECT batch_id FROM report_batches WHERE batch_name=?1",
            [name],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let id = match existing {
        Some(id) => {
            db.execute("DELETE FROM report_batch_items WHERE batch_id=?1", [id])
                .map_err(|e| e.to_string())?;
            db.execute(
                "UPDATE report_batches SET scope=?2, scope_project=?3 WHERE batch_id=?1",
                params![id, word, project],
            )
            .map_err(|e| e.to_string())?;
            id
        }
        None => {
            db.execute(
                "INSERT INTO report_batches(batch_name, scope, scope_project) VALUES(?1,?2,?3)",
                params![name, word, project],
            )
            .map_err(|e| e.to_string())?;
            db.last_insert_rowid()
        }
    };
    for (i, r) in reports.iter().enumerate() {
        db.execute(
            "INSERT INTO report_batch_items(batch_id, rpt_id, seq_num) VALUES(?1,?2,?3)",
            params![id, r, (i * 100) as i64],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(id)
}

pub fn delete_batch(db: &Connection, id: i64) -> Result<(), String> {
    db.execute("DELETE FROM report_batch_items WHERE batch_id=?1", [id])
        .map_err(|e| e.to_string())?;
    db.execute("DELETE FROM report_batches WHERE batch_id=?1", [id])
        .map(|_| ())
        .map_err(|e| e.to_string())
}

// ------------------------------------------------- definition file import

/// Checks that bytes are a report definition and names it; the second value
/// says whether OpenXER can run and edit it (else it is kept as text).
pub fn check_definition(bytes: &[u8]) -> Result<Stream, String> {
    let s = Stream::parse_bytes(bytes)?;
    super::model::Report::from_stream(&s)?;
    Ok(s)
}

/// Imports a definition file's bytes as a new report.
pub fn import_file(
    db: &Connection,
    file_stem: &str,
    bytes: &[u8],
    group: Option<u32>,
) -> Result<i64, String> {
    let s = check_definition(bytes)?;
    let mut name = name_of(&s);
    if name.is_empty() {
        name = file_stem.to_string();
    }
    let name = free_name(&list(db)?, &name);
    insert(db, &name, KIND_USER, group, None, bytes)
}

/// The stored text of a report, for Export.
pub fn export(db: &Connection, id: i64) -> Result<Vec<u8>, String> {
    load(db, id)
}

// ----------------------------------------------- reference database import

/// One report definition row read from a reference database copy.
#[derive(Clone, Debug)]
pub struct ReferenceRow {
    pub id: i64,
    pub kind: String,
    pub name: String,
    pub group: Option<i64>,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferenceGroup {
    pub id: i64,
    pub name: String,
    pub parent: Option<i64>,
}

#[derive(Clone, Debug, Default)]
pub struct Reference {
    pub reports: Vec<ReferenceRow>,
    pub groups: Vec<ReferenceGroup>,
    /// (batch id, name, report ids in order)
    pub batches: Vec<(i64, String, Vec<i64>)>,
}

fn open_read_only(path: &Path) -> Result<Connection, String> {
    Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    )
    .map_err(|e| format!("{} could not be opened: {e}", path.display()))
}

/// Reads the report definition rows of a database copy, read-only.
#[cfg(test)]
pub fn read_reference_rows(path: &Path) -> Result<Vec<ReferenceRow>, String> {
    let db = open_read_only(path)?;
    reference_rows(&db)
}

fn reference_rows(db: &Connection) -> Result<Vec<ReferenceRow>, String> {
    let mut q = db
        .prepare(
            "SELECT RPT_ID, RPT_TYPE, RPT_NAME, RPT_GROUP_ID, RPT_DATA FROM RPT ORDER BY RPT_ID",
        )
        .map_err(|e| format!("The database has no report table: {e}"))?;
    let rows = q
        .query_map([], |r| {
            let data: rusqlite::types::Value = r.get(4)?;
            Ok(ReferenceRow {
                id: r.get(0)?,
                kind: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                name: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                group: r.get(3)?,
                data: match data {
                    rusqlite::types::Value::Text(t) => t.into_bytes(),
                    rusqlite::types::Value::Blob(b) => b,
                    _ => vec![],
                },
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

/// Reads the report, group and batch tables of a database copy (opened
/// read-only; nothing else in it is touched). Missing group or batch tables
/// are simply empty.
pub fn read_reference(path: &Path) -> Result<Reference, String> {
    let db = open_read_only(path)?;
    let reports = reference_rows(&db)?;
    let groups = db
        .prepare("SELECT RPT_GROUP_ID, RPT_GROUP_NAME, PARENT_GROUP_ID FROM RPTGROUP ORDER BY RPT_GROUP_SEQ_NUM, RPT_GROUP_ID")
        .and_then(|mut q| {
            q.query_map([], |r| {
                Ok(ReferenceGroup {
                    id: r.get(0)?,
                    name: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    parent: r.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
        })
        .unwrap_or_default();
    let mut batches: Vec<(i64, String, Vec<i64>)> = db
        .prepare("SELECT RPT_BATCH_ID, RPT_BATCH_NAME FROM RPTBATCH ORDER BY RPT_BATCH_ID")
        .and_then(|mut q| {
            q.query_map([], |r| {
                Ok((
                    r.get(0)?,
                    r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    vec![],
                ))
            })?
            .collect::<Result<Vec<_>, _>>()
        })
        .unwrap_or_default();
    for b in &mut batches {
        b.2 = db
            .prepare("SELECT RPT_ID FROM RPTLIST WHERE RPT_BATCH_ID=?1 ORDER BY SEQ_NUM")
            .and_then(|mut q| {
                q.query_map([b.0], |r| r.get(0))?
                    .collect::<Result<Vec<_>, _>>()
            })
            .unwrap_or_default();
    }
    Ok(Reference {
        reports,
        groups,
        batches,
    })
}

/// What an import from a database copy did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Imported {
    pub reports: usize,
    pub groups: usize,
    pub batches: usize,
    /// Reports whose text OpenXER cannot read (not imported).
    pub skipped: Vec<String>,
}

/// Imports the chosen reports (reference ids) with their groups and the
/// batches that hold any of them. `groups` is the workspace's group tree
/// and is changed in place; the caller saves it. All or nothing: when a
/// write fails, the reports and batches already written are rolled back
/// (and the caller drops `groups`).
pub fn import_reference(
    db: &Connection,
    reference: &Reference,
    chosen: &[i64],
    groups: &mut Vec<Group>,
) -> Result<Imported, String> {
    db.execute_batch("SAVEPOINT import_reference")
        .map_err(|e| e.to_string())?;
    match import_reference_rows(db, reference, chosen, groups) {
        Ok(done) => {
            db.execute_batch("RELEASE import_reference")
                .map_err(|e| e.to_string())?;
            Ok(done)
        }
        Err(e) => {
            let _ = db.execute_batch("ROLLBACK TO import_reference; RELEASE import_reference");
            Err(format!("Nothing was imported: {e}"))
        }
    }
}

fn import_reference_rows(
    db: &Connection,
    reference: &Reference,
    chosen: &[i64],
    groups: &mut Vec<Group>,
) -> Result<Imported, String> {
    let mut out = Imported::default();
    let mut new_ids: std::collections::BTreeMap<i64, i64> = Default::default();
    // The workspace group for a reference group, made on first use.
    let mut made: std::collections::BTreeMap<i64, u32> = Default::default();
    for id in chosen {
        let Some(row) = reference.reports.iter().find(|r| r.id == *id) else {
            continue;
        };
        let stream = match check_definition(&row.data) {
            Ok(s) => s,
            Err(e) => {
                out.skipped.push(format!("{}: {e}", row.name));
                continue;
            }
        };
        let group = row
            .group
            .map(|g| group_for(reference, g, groups, &mut made, &mut out.groups, 0));
        let mut name = row.name.trim().to_string();
        if name.is_empty() {
            name = name_of(&stream);
        }
        let name = free_name(&list(db)?, &name);
        let new = insert(db, &name, KIND_USER, group.flatten(), None, &row.data)?;
        new_ids.insert(*id, new);
        out.reports += 1;
    }
    for (_, name, items) in &reference.batches {
        let ids: Vec<i64> = items
            .iter()
            .filter_map(|i| new_ids.get(i).copied())
            .collect();
        if ids.is_empty() {
            continue;
        }
        let taken: Vec<String> = batches(db)?.into_iter().map(|b| b.name).collect();
        let mut n = name.clone();
        let mut k = 2;
        while taken.contains(&n) {
            n = format!("{name} ({k})");
            k += 1;
        }
        save_batch(db, &n, &ids)?;
        out.batches += 1;
    }
    Ok(out)
}

/// The workspace group matching a reference group (by name under the same
/// parent chain), created when missing.
fn group_for(
    reference: &Reference,
    id: i64,
    groups: &mut Vec<Group>,
    made: &mut std::collections::BTreeMap<i64, u32>,
    created: &mut usize,
    depth: usize,
) -> Option<u32> {
    if let Some(g) = made.get(&id) {
        return Some(*g);
    }
    let g = reference.groups.iter().find(|g| g.id == id)?;
    let parent = if depth < 16 {
        g.parent
            .and_then(|p| group_for(reference, p, groups, made, created, depth + 1))
    } else {
        None
    };
    let found = groups
        .iter()
        .find(|x| x.name == g.name && x.parent == parent)
        .map(|x| x.id);
    let ws = found.unwrap_or_else(|| {
        *created += 1;
        groups::add_group(groups, parent, &g.name)
    });
    made.insert(id, ws);
    Some(ws)
}

// --------------------------------------------------------------- library

/// Loads the library reports the workspace has not had yet, in a group of
/// their own. Reports the user deleted stay deleted.
pub fn load_library(db: &Connection) -> Result<usize, String> {
    let loaded: Vec<String> = db
        .query_row(
            "SELECT value FROM preferences WHERE key=?1",
            [LOADED_KEY],
            |r| r.get::<_, String>(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    let missing: Vec<&super::library::Entry> = super::library::entries()
        .iter()
        .filter(|e| !loaded.contains(&e.id.to_string()))
        .collect();
    if missing.is_empty() {
        return Ok(0);
    }
    let mut tree: Vec<Group> = db
        .query_row(
            "SELECT value FROM preferences WHERE key=?1",
            [groups::GROUPS_KEY],
            |r| r.get::<_, String>(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .map(|t| groups::groups_from_json(&t))
        .unwrap_or_default();
    let group = match tree
        .iter()
        .find(|g| g.name == LIBRARY_GROUP && g.parent.is_none())
    {
        Some(g) => g.id,
        None => groups::add_group(&mut tree, None, LIBRARY_GROUP),
    };
    let existing = list(db)?;
    let mut names = existing;
    for e in &missing {
        let name = free_name(&names, &e.name);
        let id = insert(
            db,
            &name,
            KIND_LIBRARY,
            Some(group),
            Some(e.id),
            e.text.as_bytes(),
        )?;
        names.push(Stored {
            id,
            name,
            kind: KIND_LIBRARY.into(),
            group: Some(group),
            lib_id: Some(e.id.into()),
            scope: ReportScope::Global,
            last_run: None,
        });
    }
    let mut all = loaded;
    all.extend(missing.iter().map(|e| e.id.to_string()));
    for (key, value) in [
        (groups::GROUPS_KEY, groups::groups_to_json(&tree)),
        (
            LOADED_KEY,
            serde_json::to_string(&all).map_err(|e| e.to_string())?,
        ),
    ] {
        db.execute(
            "INSERT INTO preferences(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, value],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(missing.len())
}
