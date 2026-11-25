use crate::{project_json, AutomationError, Result};
use openxer_xer::{Document, ProjectData};
use rusqlite::{params, Connection, OpenFlags};
use serde_json::{json, Value};
use std::{fs, io::Read, path::Path};

pub(crate) struct Loaded {
    pub project: ProjectData,
    pub workspace_id: Option<i64>,
    /// From a workspace with a shared dictionary (schema 3): what the
    /// project read from it, so a save knows its own dictionary rows.
    pub mark: Option<crate::workspace_db::Mark>,
}
pub(crate) fn io(error: impl std::fmt::Display) -> AutomationError {
    AutomationError::new("io_error", error.to_string())
}
pub(crate) fn invalid(error: impl std::fmt::Display) -> AutomationError {
    AutomationError::new("invalid_source", error.to_string())
}
pub(crate) fn is_database_extension(path: &Path) -> bool {
    path.extension().and_then(|s| s.to_str()).is_some_and(|s| {
        ["db", "sqlite", "sqlite3"]
            .iter()
            .any(|v| s.eq_ignore_ascii_case(v))
    })
}
pub(crate) fn check_destination(path: &Path) -> Result<()> {
    check_destination_as(path, false)
}
/// Like [`check_destination`]; with `xml` the output is a project XML file
/// and must end in `.xml`.
pub(crate) fn check_destination_as(path: &Path, xml: bool) -> Result<()> {
    if path.as_os_str().is_empty() {
        return Err(AutomationError::new(
            "invalid_arguments",
            "output path cannot be empty",
        ));
    }
    if fs::symlink_metadata(path).is_ok() {
        return Err(AutomationError::new(
            "output_exists",
            format!("Destination already exists: {}", path.display()),
        ));
    }
    let ext_is = |e: &str| path.extension().is_some_and(|s| s.eq_ignore_ascii_case(e));
    if xml && !ext_is("xml") {
        return Err(AutomationError::new(
            "invalid_arguments",
            "A project XML output must end in .xml",
        ));
    }
    if !xml && !is_database_extension(path) && !ext_is("xer") {
        return Err(AutomationError::new(
            "invalid_arguments",
            "Output must end in .xer, .db, .sqlite, or .sqlite3",
        ));
    }
    if !parent(path).is_dir() {
        return Err(io("Output parent directory does not exist"));
    }
    Ok(())
}
pub(crate) fn parent(path: &Path) -> &Path {
    path.parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}
pub(crate) fn is_database(path: &Path) -> Result<bool> {
    let mut file =
        fs::File::open(path).map_err(|e| io(format!("Cannot open {}: {e}", path.display())))?;
    let mut prefix = [0u8; 16];
    let n = file
        .read(&mut prefix)
        .map_err(|e| io(format!("Cannot read {}: {e}", path.display())))?;
    Ok(n == 16 && &prefix == b"SQLite format 3\0")
}
pub(crate) fn read_database(path: &Path) -> Result<Connection> {
    let c = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(invalid)?;
    c.prepare("SELECT id,source_name,project_key,title,xer_content FROM projects LIMIT 0")
        .map_err(|_| invalid("Not an OpenXER workspace: expected projects table is missing"))?;
    Ok(c)
}
/// Whether a workspace keeps its projects' calendars, resources, codes and
/// the other dictionaries once, in a shared workspace dictionary (schema 3;
/// see `workspace_db`). A newer schema is refused, and so is the schema 2
/// an earlier build wrote until it is upgraded.
pub(crate) fn shared(c: &Connection) -> Result<bool> {
    let schema = crate::workspace_db::schema(c).map_err(invalid)?;
    if schema == crate::workspace_db::EARLIER_DICTIONARY_SCHEMA {
        return Err(AutomationError::new(
            "workspace_needs_migration",
            "This workspace was made by an earlier OpenXER (workspace schema 2). `workspace migrate --output NEW.db` upgrades a copy of it; OpenXER upgrades it when it opens it.",
        ));
    }
    if schema > crate::workspace_db::DICTIONARY_SCHEMA {
        return Err(AutomationError::new(
            "workspace_schema_unsupported",
            format!("This workspace uses workspace schema {schema}, which needs a newer OpenXER."),
        ));
    }
    Ok(schema == crate::workspace_db::DICTIONARY_SCHEMA)
}
fn columns(c: &Connection) -> Result<Vec<String>> {
    let mut stmt = c.prepare("PRAGMA table_info(projects)").map_err(invalid)?;
    let rows = stmt.query_map([], |row| row.get(1)).map_err(invalid)?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(invalid)
}
pub(crate) fn rows(c: &Connection) -> Result<Vec<Value>> {
    let mut stmt = c
        .prepare("SELECT id,project_key,title,source_name FROM projects ORDER BY id")
        .map_err(invalid)?;
    let rows = stmt.query_map([], |r| Ok(json!({"workspace_id":r.get::<_,i64>(0)?,"project_id":r.get::<_,i64>(1)?,"name":r.get::<_,String>(2)?,"source_name":r.get::<_,String>(3)?}))).map_err(invalid)?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(invalid)
}
pub(crate) fn inspect(input: &Path) -> Result<Value> {
    if is_database(input)? {
        let c = read_database(input)?;
        let schema = crate::workspace_db::schema(&c).map_err(invalid)?;
        let mut out = json!({"input":input,"format":"openxer_workspace","workspace_schema":schema,"projects":rows(&c)?});
        if schema >= crate::workspace_db::DICTIONARY_SCHEMA {
            let (dict, revision) = crate::workspace_db::read_dictionary(&c).map_err(invalid)?;
            let counts: serde_json::Map<String, Value> = dict
                .tables
                .iter()
                .filter(|t| !t.rows.is_empty())
                .map(|t| (t.name.to_ascii_uppercase(), json!(t.rows.len())))
                .collect();
            out["dictionary"] = json!({"revision": revision, "rows": counts});
        }
        Ok(out)
    } else {
        let data = fs::read(input)
            .map_err(|e| invalid(format!("Cannot read {}: {e}", input.display())))?;
        use openxer_interchange::project_xml;
        let xml = project_xml::is_xml(&data);
        let (doc, report) = if xml {
            let (doc, report) = project_xml::read_bytes_with_report(&data).map_err(invalid)?;
            (doc, Some(report))
        } else {
            (project_xml::read_bytes(&data).map_err(invalid)?, None)
        };
        let projects: Vec<_> = doc
            .projects()
            .iter()
            .map(|(id, name)| json!({"project_id":id,"name":name}))
            .collect();
        let tables: Vec<_> = doc
            .tables
            .iter()
            .map(|t| {
                // A project XML file has no element for a project's own root
                // WBS node, so its WBS count leaves those rows out.
                let nodes = if xml && t.name.eq_ignore_ascii_case("PROJWBS") {
                    t.rows
                        .iter()
                        .filter(|r| t.value(r, "proj_node_flag") == "Y")
                        .count()
                } else {
                    0
                };
                let mut row = json!({"name":t.name,"row_count":t.rows.len() - nodes});
                if nodes > 0 {
                    row["project_nodes"] = json!(nodes);
                }
                row
            })
            .collect();
        let format = if xml { "xml" } else { "xer" };
        let encoding = doc.windows_encoding;
        let mut out = json!({"input":input,"format":format,"windows_1252":encoding,
            "projects":projects,"tables":tables});
        if let Some(r) = report {
            // What the file held, what OpenXER has no place for (skipped),
            // and fields it does not read.
            out["xml"] = json!({"elements":r.elements,"unhandled":r.unhandled,
                "fields_not_read":r.fields_not_read});
        }
        Ok(out)
    }
}
pub(crate) fn load(
    input: &Path,
    project_id: Option<i64>,
    workspace_id: Option<i64>,
) -> Result<Loaded> {
    let (document, pid, source, wid, level, mark) = if is_database(input)? {
        let c = read_database(input)?;
        let shared = shared(&c)?;
        let candidates: Vec<_> = rows(&c)?
            .into_iter()
            .filter(|r| {
                project_id.is_none_or(|id| r["project_id"] == id)
                    && workspace_id.is_none_or(|id| r["workspace_id"] == id)
            })
            .collect();
        if candidates.is_empty() {
            return Err(AutomationError::new(
                "project_not_found",
                "No workspace project matches the selectors",
            ));
        }
        if candidates.len() != 1 {
            return Err(AutomationError::new(
                "ambiguous_project",
                "Select project_id or workspace_id from inspect; multiple projects match",
            ));
        }
        let wid = candidates[0]["workspace_id"]
            .as_i64()
            .expect("SQLite integer id");
        let cols = columns(&c)?;
        let encoding = if cols.iter().any(|s| s == "source_windows_encoding") {
            "source_windows_encoding"
        } else {
            "0"
        };
        let level = if cols.iter().any(|s| s == "level_resources") {
            "level_resources"
        } else {
            "0"
        };
        let sql = format!("SELECT source_name,project_key,xer_content,{encoding},{level} FROM projects WHERE id=?1");
        let (source, pid, text, legacy, level): (String, i64, String, bool, bool) = c
            .query_row(&sql, [wid], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })
            .map_err(invalid)?;
        let mut doc = Document::parse(&text).map_err(invalid)?;
        doc.windows_encoding = legacy;
        // The shared calendars, resources and codes come back from the
        // workspace dictionary, as the app opens the project.
        let mark = if shared {
            Some(crate::workspace_db::hydrate(&c, &mut doc, Some(wid)).map_err(invalid)?)
        } else {
            None
        };
        (doc, pid, source, Some(wid), Some(level), mark)
    } else {
        if workspace_id.is_some() {
            return Err(AutomationError::new(
                "invalid_arguments",
                "workspace_id applies only to OpenXER databases",
            ));
        }
        let doc = openxer_interchange::project_xml::read_file(input).map_err(invalid)?;
        let projects: Vec<_> = doc
            .projects()
            .into_iter()
            .filter(|(id, _)| project_id.is_none_or(|pid| *id == pid))
            .collect();
        if projects.is_empty() {
            return Err(AutomationError::new(
                "project_not_found",
                "No XER project matches the selector",
            ));
        }
        if projects.len() != 1 {
            return Err(AutomationError::new(
                "ambiguous_project",
                "Select project_id from inspect; the XER contains multiple projects",
            ));
        }
        (
            doc,
            projects[0].0,
            input
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            None,
            None,
            None,
        )
    };
    let mut project = ProjectData::from_document(document, pid, source).map_err(invalid)?;
    if let Some(level) = level {
        project.model.options.do_level = level;
    }
    project.model.validate().map_err(invalid)?;
    Ok(Loaded {
        project,
        workspace_id: wid,
        mark,
    })
}
pub(crate) fn schema(c: &Connection) -> Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS projects(id INTEGER PRIMARY KEY,source_name TEXT NOT NULL,project_key INTEGER NOT NULL,title TEXT NOT NULL,xer_content TEXT NOT NULL,updated_at TEXT DEFAULT CURRENT_TIMESTAMP,source_windows_encoding INTEGER NOT NULL DEFAULT 0,level_resources INTEGER NOT NULL DEFAULT 0,UNIQUE(source_name,project_key));CREATE TABLE IF NOT EXISTS preferences(key TEXT PRIMARY KEY,value TEXT NOT NULL);").map_err(io)?;
    let cols = columns(c)?;
    for col in ["source_windows_encoding", "level_resources"] {
        if !cols.iter().any(|s| s == col) {
            c.execute_batch(&format!(
                "ALTER TABLE projects ADD COLUMN {col} INTEGER NOT NULL DEFAULT 0"
            ))
            .map_err(io)?;
        }
    }
    Ok(())
}
/// A database `schema` has just created (its `projects` table did not
/// exist): a new workspace keeps one shared dictionary from the start.
pub(crate) fn new_database(c: &Connection) -> Result<bool> {
    let exists = c
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='projects'",
            [],
            |_| Ok(()),
        )
        .map(|_| true)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(false),
            e => Err(e),
        })
        .map_err(io)?;
    Ok(!exists)
}

/// Write project `p` into workspace row `id` (a new row when `None`).
/// With a shared dictionary its dictionary rows are merged into it first
/// (`mark`: what the project read from it) and left out of the stored text.
pub(crate) fn store(
    c: &Connection,
    p: &ProjectData,
    id: Option<i64>,
    mark: Option<&crate::workspace_db::Mark>,
) -> Result<i64> {
    store_with(c, p, id, mark, &crate::workspace_db::Choices::new()).map(|(id, _)| id)
}

/// [`store`] with an import's choices for dictionary rows whose name the
/// workspace has with different content; also returns the merge events.
pub(crate) fn store_with(
    c: &Connection,
    p: &ProjectData,
    id: Option<i64>,
    mark: Option<&crate::workspace_db::Mark>,
    choices: &crate::workspace_db::Choices,
) -> Result<(i64, Vec<openxer_xer::workspace_scope::merge::Event>)> {
    let shared = shared(c)?;
    let doc = p.updated_document();
    let id = match id {
        Some(id) => id,
        None => {
            c.execute("INSERT INTO projects(source_name,project_key,title,xer_content,source_windows_encoding,level_resources) VALUES(?1,?2,?3,?4,?5,?6)",params![p.source_name,p.model.project.proj_id,p.model.project.name,if shared { String::new() } else { doc.text() },p.document.windows_encoding,p.model.options.do_level]).map_err(io)?;
            let id = c.last_insert_rowid();
            if !shared {
                return Ok((id, vec![]));
            }
            id
        }
    };
    let (text, events) = if shared {
        let stored = crate::workspace_db::store_with(
            c,
            &doc,
            mark,
            Some(id),
            &openxer_xer::workspace_scope::merge::Policy::Save,
            choices,
        )
        .map_err(invalid)?;
        (stored.text, stored.outcome.events)
    } else {
        (doc.text(), vec![])
    };
    let count = c.execute("UPDATE projects SET title=?1,xer_content=?2,source_windows_encoding=?3,level_resources=?4,updated_at=CURRENT_TIMESTAMP WHERE id=?5",params![p.model.project.name,text,p.document.windows_encoding,p.model.options.do_level,id]).map_err(io)?;
    if count != 1 {
        return Err(invalid("Selected workspace row disappeared"));
    }
    Ok((id, events))
}

/// The XER text an export writes for project `pid` of `loaded`: from a
/// workspace with a shared dictionary, only the dictionary rows it uses.
pub(crate) fn export_document(loaded: &Loaded) -> Document {
    let doc = loaded.project.updated_document();
    if loaded.mark.is_some() {
        openxer_xer::workspace_scope::used_subset(&doc, loaded.project.model.project.proj_id)
    } else {
        doc
    }
}
pub(crate) fn publish(temp: tempfile::NamedTempFile, output: &Path) -> Result<()> {
    temp.as_file().sync_all().map_err(io)?;
    temp.persist_noclobber(output).map_err(|e| {
        AutomationError::new(
            if e.error.kind() == std::io::ErrorKind::AlreadyExists {
                "output_exists"
            } else {
                "io_error"
            },
            e.error.to_string(),
        )
    })?;
    Ok(())
}
pub(crate) fn save(input: &Path, output: &Path, loaded: &Loaded) -> Result<()> {
    check_destination(output)?;
    let temp = tempfile::NamedTempFile::new_in(parent(output)).map_err(io)?;
    if is_database_extension(output) {
        if loaded.workspace_id.is_some() {
            let source = read_database(input)?;
            source.backup("main", temp.path(), None).map_err(io)?;
        }
        let mut c = Connection::open(temp.path()).map_err(io)?;
        c.execute_batch("PRAGMA journal_mode=DELETE").map_err(io)?;
        let tx = c.transaction().map_err(io)?;
        let new = new_database(&tx)?;
        schema(&tx)?;
        if new {
            crate::workspace_db::start_new(&tx).map_err(io)?;
        }
        let id = store(
            &tx,
            &loaded.project,
            loaded.workspace_id,
            loaded.mark.as_ref(),
        )?;
        tx.execute("INSERT INTO preferences(key,value) VALUES('last_project',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[id.to_string()]).map_err(io)?;
        tx.commit().map_err(io)?;
        c.close().map_err(|(_, e)| io(e))?;
    } else {
        export_document(loaded).write(temp.path()).map_err(io)?;
    }
    publish(temp, output)
}
/// `workspace migrate`: plan (dry run) or write a copy of `input` that keeps
/// its workspace rows in a shared dictionary. The input is never changed and
/// stays as the backup of the migrated copy.
pub(crate) fn workspace_migrate(
    input: &Path,
    output: Option<&Path>,
    dry_run: bool,
) -> Result<Value> {
    if !is_database(input)? {
        return Err(invalid(
            "workspace migrate needs an OpenXER workspace database",
        ));
    }
    let failed = |e: String| AutomationError::new("migration_failed", e);
    match (output, dry_run) {
        (None, true) => {
            let mut c = read_database(input)?;
            let mut result = crate::workspace_db::migrate(&mut c, true, None).map_err(failed)?;
            result["input"] = json!(input);
            Ok(result)
        }
        (Some(output), false) => {
            check_destination(output)?;
            if !is_database_extension(output) {
                return Err(AutomationError::new(
                    "invalid_arguments",
                    "workspace migrate writes an OpenXER database (.db/.sqlite/.sqlite3)",
                ));
            }
            let temp = tempfile::NamedTempFile::new_in(parent(output)).map_err(io)?;
            read_database(input)?
                .backup("main", temp.path(), None)
                .map_err(io)?;
            let mut c = Connection::open(temp.path()).map_err(io)?;
            c.execute_batch("PRAGMA journal_mode=DELETE").map_err(io)?;
            let mut result = crate::workspace_db::migrate(&mut c, false, None).map_err(failed)?;
            c.close().map_err(|(_, e)| io(e))?;
            publish(temp, output)?;
            result["input"] = json!(input);
            result["output"] = json!(output);
            result["backup"] = json!(input);
            Ok(result)
        }
        _ => Err(AutomationError::new(
            "invalid_arguments",
            "workspace migrate needs exactly one of output or dry_run",
        )),
    }
}
pub(crate) fn import(input: &Path, output: &Path, project_id: Option<i64>) -> Result<Value> {
    check_destination(output)?;
    if !is_database_extension(output) {
        return Err(AutomationError::new(
            "invalid_arguments",
            "import requires an OpenXER database output (.db/.sqlite/.sqlite3)",
        ));
    }
    if is_database(input)? {
        return Err(AutomationError::new(
            "invalid_arguments",
            "import requires XER input; use schedule/export for an existing workspace",
        ));
    }
    let doc = openxer_interchange::project_xml::read_file(input).map_err(invalid)?;
    let selected: Vec<_> = doc
        .projects()
        .into_iter()
        .filter(|(id, _)| project_id.is_none_or(|pid| *id == pid))
        .collect();
    if selected.is_empty() {
        return Err(AutomationError::new(
            "project_not_found",
            "No XER project matches the selector",
        ));
    }
    let temp = tempfile::NamedTempFile::new_in(parent(output)).map_err(io)?;
    let mut c = Connection::open(temp.path()).map_err(io)?;
    let tx = c.transaction().map_err(io)?;
    schema(&tx)?;
    // A new workspace keeps one shared dictionary.
    crate::workspace_db::start_new(&tx).map_err(io)?;
    let mut summaries = vec![];
    for (pid, _) in selected {
        let p = load(input, Some(pid), None)?.project;
        let (id, events) = store_with(&tx, &p, None, None, &Default::default())?;
        summaries.push(
            json!({"workspace_id":id,"project":project_json(&p),"warnings":p.warnings,
            "dictionary":crate::workspace_db::import_report(&events)}),
        );
    }
    tx.execute(
        "INSERT INTO preferences(key,value) VALUES('last_project',?1)",
        [summaries[0]["workspace_id"].to_string()],
    )
    .map_err(io)?;
    tx.commit().map_err(io)?;
    c.close().map_err(|(_, e)| io(e))?;
    publish(temp, output)?;
    Ok(json!({"output":output,"projects":summaries}))
}
