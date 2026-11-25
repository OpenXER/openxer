//! Workspace storage of baselines. A baseline is a frozen copy of a project,
//! kept as its own row in the `baselines` table and linked to its parent
//! workspace project, so it never shows in project lists. Deleting the parent
//! deletes its baselines. The user baseline choices (Primary, Secondary,
//! Tertiary) live in `baseline_assign`; the project baseline is the parent's
//! own `sum_base_proj_id`, as in the file.
use rusqlite::{params, Connection, OptionalExtension};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS baselines(
    id INTEGER PRIMARY KEY,
    parent INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    project_key INTEGER NOT NULL,
    short_name TEXT NOT NULL,
    name TEXT NOT NULL,
    baseline_type TEXT NOT NULL DEFAULT '',
    data_date TEXT NOT NULL DEFAULT '',
    added_at TEXT NOT NULL DEFAULT '',
    updated_at TEXT NOT NULL DEFAULT '',
    xer_content TEXT NOT NULL,
    source_windows_encoding INTEGER NOT NULL DEFAULT 0,
    import_batch INTEGER);
CREATE INDEX IF NOT EXISTS baselines_parent ON baselines(parent);
CREATE TABLE IF NOT EXISTS baseline_assign(
    parent INTEGER PRIMARY KEY REFERENCES projects(id) ON DELETE CASCADE,
    primary_bl TEXT NOT NULL DEFAULT 'current',
    secondary_bl TEXT NOT NULL DEFAULT '',
    tertiary_bl TEXT NOT NULL DEFAULT '');
DELETE FROM baselines WHERE parent NOT IN (SELECT id FROM projects);
DELETE FROM baseline_assign WHERE parent NOT IN (SELECT id FROM projects);";

/// Create the baseline tables and drop rows whose parent is gone (a tool that
/// opened the workspace without foreign keys may have removed it).
pub fn ensure(c: &Connection) -> Result<(), String> {
    c.execute_batch(SCHEMA).map_err(|e| e.to_string())
}

/// One stored baseline. `xer` is empty in listings.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BaselineRow {
    pub id: i64,
    pub parent: i64,
    /// The baseline's project id inside its document.
    pub project_key: i64,
    /// Baseline ID (the copy's project ID).
    pub short_name: String,
    pub name: String,
    pub baseline_type: String,
    /// `YYYY-MM-DD HH:MM`, as the file stores dates.
    pub data_date: String,
    pub added: String,
    pub updated: String,
    pub xer: String,
    pub windows_encoding: bool,
    pub import_batch: Option<i64>,
}

const COLUMNS: &str = "id,parent,project_key,short_name,name,baseline_type,data_date,added_at,updated_at,xer_content,source_windows_encoding,import_batch";

fn row(r: &rusqlite::Row) -> rusqlite::Result<BaselineRow> {
    Ok(BaselineRow {
        id: r.get(0)?,
        parent: r.get(1)?,
        project_key: r.get(2)?,
        short_name: r.get(3)?,
        name: r.get(4)?,
        baseline_type: r.get(5)?,
        data_date: r.get(6)?,
        added: r.get(7)?,
        updated: r.get(8)?,
        xer: r.get(9)?,
        windows_encoding: r.get(10)?,
        import_batch: r.get(11)?,
    })
}

/// Baselines of a workspace project in the order they were added, without
/// their content.
pub fn list(c: &Connection, parent: i64) -> Result<Vec<BaselineRow>, String> {
    let sql = format!(
        "SELECT {} FROM baselines WHERE parent=?1 ORDER BY id",
        COLUMNS.replace("xer_content", "''")
    );
    let mut stmt = c.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt.query_map([parent], row).map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

pub fn count(c: &Connection, parent: i64) -> usize {
    c.query_row(
        "SELECT COUNT(*) FROM baselines WHERE parent=?1",
        [parent],
        |r| r.get::<_, i64>(0),
    )
    .map_or(0, |n| n as usize)
}

/// One baseline with its content.
pub fn get(c: &Connection, id: i64) -> Result<Option<BaselineRow>, String> {
    c.query_row(
        &format!("SELECT {COLUMNS} FROM baselines WHERE id=?1"),
        [id],
        row,
    )
    .optional()
    .map_err(|e| e.to_string())
}

/// Write a baseline: insert when `row.id` is 0 (a new id) or absent, else
/// replace it. Returns its id.
pub fn put(c: &Connection, b: &BaselineRow) -> Result<i64, String> {
    let id = (b.id > 0).then_some(b.id);
    c.execute(
        &format!("INSERT OR REPLACE INTO baselines({COLUMNS}) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)"),
        params![
            id,
            b.parent,
            b.project_key,
            b.short_name,
            b.name,
            b.baseline_type,
            b.data_date,
            b.added,
            b.updated,
            b.xer,
            b.windows_encoding,
            b.import_batch
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(id.unwrap_or_else(|| c.last_insert_rowid()))
}

pub fn delete(c: &Connection, id: i64) -> Result<(), String> {
    c.execute("DELETE FROM baselines WHERE id=?1", [id])
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Remove the baselines an import added (Undo Last Import).
pub fn delete_import(c: &Connection, batch: i64) -> Result<(), String> {
    c.execute("DELETE FROM baselines WHERE import_batch=?1", [batch])
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// What a user baseline slot shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Choice {
    #[default]
    None,
    /// The open project stands in for a baseline (its planned dates).
    Current,
    /// A stored baseline (row id).
    Baseline(i64),
}
impl Choice {
    fn key(self) -> String {
        match self {
            Choice::None => String::new(),
            Choice::Current => "current".into(),
            Choice::Baseline(id) => id.to_string(),
        }
    }
    fn from_key(s: &str) -> Self {
        match s.trim() {
            "" => Choice::None,
            "current" => Choice::Current,
            s => s.parse().map_or(Choice::None, Choice::Baseline),
        }
    }
}

/// Primary, Secondary and Tertiary user baselines of one project.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct UserBaselines(pub [Choice; 3]);
impl Default for UserBaselines {
    /// A new project compares against itself as primary, nothing else.
    fn default() -> Self {
        UserBaselines([Choice::Current, Choice::None, Choice::None])
    }
}

pub fn user_baselines(c: &Connection, parent: i64) -> UserBaselines {
    c.query_row(
        "SELECT primary_bl,secondary_bl,tertiary_bl FROM baseline_assign WHERE parent=?1",
        [parent],
        |r| {
            Ok(UserBaselines([
                Choice::from_key(&r.get::<_, String>(0)?),
                Choice::from_key(&r.get::<_, String>(1)?),
                Choice::from_key(&r.get::<_, String>(2)?),
            ]))
        },
    )
    .unwrap_or_default()
}

pub fn set_user_baselines(c: &Connection, parent: i64, u: UserBaselines) -> Result<(), String> {
    c.execute(
        "INSERT INTO baseline_assign(parent,primary_bl,secondary_bl,tertiary_bl) VALUES(?1,?2,?3,?4) \
         ON CONFLICT(parent) DO UPDATE SET primary_bl=excluded.primary_bl,secondary_bl=excluded.secondary_bl,tertiary_bl=excluded.tertiary_bl",
        params![parent, u.0[0].key(), u.0[1].key(), u.0[2].key()],
    )
    .map(|_| ())
    .map_err(|e| e.to_string())
}

/// The workspace row of a project.
pub fn parent_row(c: &Connection, source: &str, key: i64) -> Option<i64> {
    c.query_row(
        "SELECT id FROM projects WHERE source_name=?1 AND project_key=?2",
        params![source, key],
        |r| r.get(0),
    )
    .optional()
    .ok()
    .flatten()
}

/// A whole workspace project row, kept so converting a project into a
/// baseline (or copying a baseline out as a project) can be undone.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectRow {
    pub id: i64,
    pub source_name: String,
    pub project_key: i64,
    pub title: String,
    pub xer: String,
    pub windows_encoding: bool,
    pub level_resources: bool,
}

pub fn project_row(c: &Connection, id: i64) -> Result<Option<ProjectRow>, String> {
    c.query_row(
        "SELECT id,source_name,project_key,title,xer_content,source_windows_encoding,level_resources FROM projects WHERE id=?1",
        [id],
        |r| {
            Ok(ProjectRow {
                id: r.get(0)?,
                source_name: r.get(1)?,
                project_key: r.get(2)?,
                title: r.get(3)?,
                xer: r.get(4)?,
                windows_encoding: r.get(5)?,
                level_resources: r.get(6)?,
            })
        },
    )
    .optional()
    .map_err(|e| e.to_string())
}

/// Put a project row back (insert with its id) or remove it. An existing row
/// is updated in place so its baselines stay attached.
pub fn put_project_row(c: &Connection, id: i64, p: Option<&ProjectRow>) -> Result<(), String> {
    match p {
        None => c
            .execute("DELETE FROM projects WHERE id=?1", [id])
            .map(|_| ()),
        Some(p) => {
            let updated = c.execute(
                "UPDATE projects SET source_name=?1,project_key=?2,title=?3,xer_content=?4,source_windows_encoding=?5,level_resources=?6,updated_at=CURRENT_TIMESTAMP WHERE id=?7",
                params![p.source_name, p.project_key, p.title, p.xer, p.windows_encoding, p.level_resources, id],
            );
            match updated {
                Ok(0) => c
                    .execute(
                        "INSERT INTO projects(id,source_name,project_key,title,xer_content,source_windows_encoding,level_resources) VALUES(?1,?2,?3,?4,?5,?6,?7)",
                        params![id, p.source_name, p.project_key, p.title, p.xer, p.windows_encoding, p.level_resources],
                    )
                    .map(|_| ()),
                Ok(_) => Ok(()),
                Err(e) => Err(e),
            }
        }
    }
    .map_err(|e| e.to_string())
}

/// An internal project number no workspace project, no stored baseline and
/// none of `also` uses, for a project the workspace makes (File > New,
/// `eps create-project`, a pasted copy, a restored baseline):
/// `openxer_xer::workspace::numbering::next` over all of them.
pub fn next_project_number(c: &Connection, also: &[i64]) -> Result<i64, String> {
    let has = |table: &str| -> Result<bool, String> {
        c.query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1",
            [table],
            |_| Ok(()),
        )
        .optional()
        .map(|r| r.is_some())
        .map_err(|e| e.to_string())
    };
    let mut taken = also.to_vec();
    for table in ["projects", "baselines"] {
        if !has(table)? {
            continue;
        }
        let high: Option<i64> = c
            .query_row(&format!("SELECT MAX(project_key) FROM {table}"), [], |r| {
                r.get(0)
            })
            .map_err(|e| e.to_string())?;
        taken.extend(high);
    }
    Ok(openxer_xer::workspace::numbering::next(taken))
}

/// A source name no workspace project uses with `key`, for a project copied
/// out of a baseline.
pub fn unique_source(c: &Connection, base: &str, key: i64) -> String {
    let taken = |name: &str| {
        c.query_row(
            "SELECT 1 FROM projects WHERE source_name=?1 AND project_key=?2",
            params![name, key],
            |_| Ok(()),
        )
        .optional()
        .ok()
        .flatten()
        .is_some()
    };
    if !taken(base) {
        return base.to_string();
    }
    (2..)
        .map(|n| format!("{base} ({n})"))
        .find(|s| !taken(s))
        .expect("an unused name")
}
