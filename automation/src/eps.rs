//! `eps list` and `eps` edits: the workspace EPS (project folders) with each
//! project's and folder's rollups, and folder and project filing edits.
//! Every rule comes from `openxer_xer::workspace`, the code the desktop app
//! uses, so a folder deleted here takes its folders and projects exactly as
//! the app does and a project created here passes the New Project wizard's
//! checks.
use crate::storage::{self, invalid, io, parent};
use crate::{AutomationError, Result};
use openxer_xer::workspace::new_project::{self, Entry};
use openxer_xer::workspace::rollup::{self, Rollup};
use openxer_xer::workspace::{Workspace, KEY};
use openxer_xer::{date_string, parse_absolute, Document};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// One EPS edit.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum EpsOperation {
    /// Add a folder under `parent` (an EPS ID; the top level when omitted).
    AddFolder {
        name: String,
        #[serde(default)]
        eps_id: Option<String>,
        #[serde(default)]
        parent: Option<String>,
    },
    /// Rename a folder or change its EPS ID.
    RenameFolder {
        folder: String,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        new_eps_id: Option<String>,
    },
    /// Move a folder under `parent` (the top level when omitted).
    MoveFolder {
        folder: String,
        #[serde(default)]
        parent: Option<String>,
    },
    /// Delete a folder with the folders under it and every project filed in
    /// them, with those projects' baselines. A folder that holds projects
    /// is deleted only with `delete_projects` true (a dry run reports what
    /// would go without it).
    DeleteFolder {
        folder: String,
        #[serde(default)]
        delete_projects: bool,
    },
    /// File a workspace project under a folder.
    FileProject { workspace_id: i64, folder: String },
    /// Create an empty project in a folder with the New Project wizard's
    /// rules.
    CreateProject {
        folder: String,
        project_id: String,
        #[serde(default)]
        name: Option<String>,
        planned_start: String,
        #[serde(default)]
        must_finish: Option<String>,
        /// Responsible manager: an OBS name or id.
        #[serde(default)]
        manager: Option<String>,
        /// COST_PER_QTY .. COST_PER_QTY5, or a label such as "Price / Unit2".
        #[serde(default)]
        rate_type: Option<String>,
    },
}

fn args(message: impl Into<String>) -> AutomationError {
    AutomationError::new("invalid_arguments", message)
}

fn not_found(message: impl Into<String>) -> AutomationError {
    AutomationError::new("not_found", message)
}

fn has_table(c: &Connection, name: &str) -> bool {
    c.query_row(
        "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1",
        [name],
        |_| Ok(()),
    )
    .optional()
    .ok()
    .flatten()
    .is_some()
}

fn read_workspace(c: &Connection) -> Result<Workspace> {
    if !has_table(c, "preferences") {
        return Ok(Workspace::default());
    }
    let text: Option<String> = c
        .query_row("SELECT value FROM preferences WHERE key=?1", [KEY], |r| {
            r.get(0)
        })
        .optional()
        .map_err(invalid)?;
    Ok(Workspace::from_text(&text.unwrap_or_default()))
}

/// A workspace project row as `eps list` reads it.
struct Stored {
    id: i64,
    key: i64,
    title: String,
    text: String,
}

fn stored_projects(c: &Connection) -> Result<Vec<Stored>> {
    let mut stmt = c
        .prepare("SELECT id,project_key,title,xer_content FROM projects ORDER BY id")
        .map_err(invalid)?;
    let rows = stmt
        .query_map([], |r| {
            Ok(Stored {
                id: r.get(0)?,
                key: r.get(1)?,
                title: r.get(2)?,
                text: r.get(3)?,
            })
        })
        .map_err(invalid)?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(invalid)
}

/// Baseline (start, finish) of workspace project `id`, chosen as the app
/// chooses: its project baseline, else a stored Primary baseline.
fn baseline_dates(
    c: &Connection,
    id: i64,
    doc: &Document,
    key: i64,
) -> Option<(Option<i64>, Option<i64>)> {
    if !has_table(c, "baselines") {
        return None;
    }
    let mut stmt = c
        .prepare("SELECT id,project_key FROM baselines WHERE parent=?1 ORDER BY id")
        .ok()?;
    let pairs: Vec<(i64, i64)> = stmt
        .query_map([id], |r| Ok((r.get(0)?, r.get(1)?)))
        .ok()?
        .filter_map(|r| r.ok())
        .collect();
    if pairs.is_empty() {
        return None;
    }
    let primary = has_table(c, "baseline_assign")
        .then(|| {
            c.query_row(
                "SELECT primary_bl FROM baseline_assign WHERE parent=?1",
                [id],
                |r| r.get::<_, String>(0),
            )
            .ok()
        })
        .flatten()
        .and_then(|s| s.trim().parse::<i64>().ok());
    let pick = rollup::pick_baseline(rollup::assigned_baseline(doc, key), &pairs, primary)?;
    let (text, bkey): (String, i64) = c
        .query_row(
            "SELECT xer_content,project_key FROM baselines WHERE id=?1",
            [pick],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok()?;
    let bl = Document::parse(&text).ok()?;
    rollup::of_document(&bl, bkey).map(|r| (r.start, r.finish))
}

fn date(v: Option<i64>) -> Value {
    v.map_or(Value::Null, |v| json!(date_string(0, Some(v))))
}

fn hours(v: Option<i64>) -> Value {
    v.map_or(Value::Null, |m| json!(m as f64 / 60.))
}

fn round(v: f64, places: i32) -> f64 {
    let k = 10f64.powi(places);
    (v * k).round() / k
}

fn amounts(a: &rollup::Amounts) -> Value {
    json!({"budgeted": round(a.budgeted, 4), "actual": round(a.actual, 4), "remaining": round(a.remaining, 4)})
}

/// The rollup as JSON: dates `YYYY-MM-DD HH:MM`, float and variance in
/// working hours, the percent with two decimals.
pub fn rollup_json(r: &Rollup) -> Value {
    json!({
        "start": date(r.start),
        "finish": date(r.finish),
        "late_start": date(r.late_start),
        "late_finish": date(r.late_finish),
        "actual_start": date(r.actual_start),
        "actual_finish": date(r.actual_finish),
        "data_date": date(r.data_date),
        "must_finish_by": date(r.must_finish),
        "baseline_finish": date(r.baseline_finish),
        "finish_variance_hours": hours(r.finish_variance()),
        "total_float_hours": hours(r.total_float),
        "schedule_percent_complete": r.schedule_percent().map(|v| round(v, 2)),
        "labor_units_hours": amounts(&r.labor_units),
        "costs": amounts(&r.costs),
        "activities": r.activities,
        "not_started": r.not_started,
        "in_progress": r.in_progress,
        "completed": r.completed,
        "projects": r.projects,
        "working_time_calendar": if r.mixed_calendars { json!(null) } else { r.calendar.as_ref().map(|c| json!(c.name)).unwrap_or(Value::Null) },
    })
}

const RULES: &str = "Dates are the earliest start and latest finish (actual, else early) of the activities; a folder shows its projects' earliest start, latest finish, latest data date and latest Must Finish By. Total float is the lowest of the unfinished activities (a folder: the lowest of its projects). Units and costs add up; labor units count labor resources, costs every assignment plus expenses. Schedule percent complete is duration based: (planned span - remaining span) / planned span, in working time of the shared default calendar (else the standard calendar). Values are read from the stored projects; nothing needs summarizing first.";

/// The EPS tree of a workspace with every project and folder rollup.
pub fn list(input: &Path) -> Result<Value> {
    if !storage::is_database(input)? {
        return Err(args("eps list needs an OpenXER workspace database (.db)"));
    }
    let c = storage::read_database(input)?;
    let ws = read_workspace(&c)?;
    let mut rollups: BTreeMap<i64, Rollup> = BTreeMap::new();
    let mut projects = vec![];
    // A schema 3 workspace keeps the shared calendars, resources and codes
    // in its dictionary; each project gets them back, as the app opens it.
    let shared = crate::storage::shared(&c)?;
    for p in stored_projects(&c)? {
        let Ok(mut doc) = Document::parse(&p.text) else {
            continue;
        };
        if shared {
            crate::workspace_db::hydrate(&c, &mut doc, Some(p.id)).map_err(invalid)?;
        }
        let mut r = rollup::of_document(&doc, p.key).unwrap_or_default();
        if let Some((s, f)) = baseline_dates(&c, p.id, &doc, p.key) {
            r.set_baseline(s, f);
        }
        let node = doc.table("PROJWBS").and_then(|t| {
            t.rows
                .iter()
                .find(|row| {
                    t.value(row, "proj_id").trim() == p.key.to_string()
                        && t.value(row, "proj_node_flag") == "Y"
                })
                .map(|row| {
                    (
                        t.value(row, "wbs_name").to_string(),
                        t.value(row, "status_code").to_string(),
                    )
                })
        });
        let folder = ws.node_of(Some(p.id));
        projects.push(json!({
            "workspace_id": p.id,
            "project_id": p.title,
            "project_key": p.key,
            "name": node.as_ref().map(|n| n.0.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| p.title.clone()),
            "status": node.as_ref().map(|n| n.1.clone()).unwrap_or_default(),
            "folder": ws.eps_value(folder, "eps_short_name"),
            "rollup": rollup_json(&r),
        }));
        rollups.insert(p.id, r);
    }
    let ids: Vec<i64> = rollups.keys().copied().collect();
    let nodes = ws.eps();
    let folders: Vec<Value> = nodes
        .iter()
        .map(|n| {
            let mut total = Rollup::default();
            for id in ws.projects_under(n.id, &ids) {
                total.add(&rollups[&id]);
            }
            let own: Vec<i64> = ids
                .iter()
                .copied()
                .filter(|id| ws.node_of(Some(*id)) == n.id)
                .collect();
            json!({
                "eps_id": ws.eps_value(n.id, "eps_short_name"),
                "name": ws.eps_value(n.id, "eps_name"),
                "parent": n.parent.map(|p| ws.eps_value(p, "eps_short_name")),
                "depth": n.depth,
                "path": ws.path(n.id),
                "projects": own,
                "rollup": rollup_json(&total),
            })
        })
        .collect();
    Ok(json!({"input": input, "folders": folders, "projects": projects, "rules": RULES}))
}

/// Rows of `table` whose `parent` is workspace project `id` (0 when the
/// table does not exist).
fn count_rows(c: &Connection, table: &str, id: i64) -> Result<i64> {
    if !has_table(c, table) {
        return Ok(0);
    }
    c.query_row(
        &format!("SELECT COUNT(*) FROM {table} WHERE parent=?1"),
        [id],
        |r| r.get(0),
    )
    .map_err(invalid)
}

/// Remove workspace project `id` with its baselines and Assign Baselines
/// choices, as the app's project delete does.
fn remove_project(c: &Connection, id: i64) -> Result<()> {
    for table in ["baselines", "baseline_assign"] {
        if has_table(c, table) {
            c.execute(&format!("DELETE FROM {table} WHERE parent=?1"), [id])
                .map_err(io)?;
        }
    }
    c.execute("DELETE FROM projects WHERE id=?1", [id])
        .map_err(io)?;
    Ok(())
}

fn folder(ws: &Workspace, code: &str) -> Result<i64> {
    ws.find(code).ok_or_else(|| {
        not_found(format!(
            "No EPS folder has the EPS ID {code:?}; see eps list"
        ))
    })
}

fn folder_opt(ws: &Workspace, code: Option<&str>) -> Result<Option<i64>> {
    code.map(|c| folder(ws, c)).transpose()
}

/// An OBS id from a name or id in `obs`.
fn manager_id(obs: Option<&openxer_xer::Table>, text: &str) -> Result<String> {
    let t = obs.ok_or_else(|| {
        not_found("The workspace has no responsible managers (OBS) to choose from")
    })?;
    t.rows
        .iter()
        .find(|r| {
            t.value(r, "obs_id").trim() == text.trim()
                || t.value(r, "obs_name")
                    .trim()
                    .eq_ignore_ascii_case(text.trim())
        })
        .map(|r| t.value(r, "obs_id").trim().to_string())
        .ok_or_else(|| not_found(format!("No responsible manager is named {text:?}")))
}

/// Run one EPS edit on a copy of `input` written to `output` (or, with
/// `dry_run`, only report what it would do).
pub fn edit(
    input: &Path,
    output: Option<PathBuf>,
    dry_run: bool,
    op: &EpsOperation,
) -> Result<Value> {
    match (&output, dry_run) {
        (Some(_), true) => return Err(args("dry_run writes nothing; omit output")),
        (None, false) => return Err(args("output is required unless dry_run is true")),
        (Some(path), false) => {
            storage::check_destination(path)?;
            if !storage::is_database_extension(path) {
                return Err(args(
                    "eps edits write an OpenXER workspace (.db/.sqlite/.sqlite3)",
                ));
            }
        }
        (None, true) => {}
    }
    if !storage::is_database(input)? {
        return Err(args("eps edits need an OpenXER workspace database input"));
    }
    let source = storage::read_database(input)?;
    let temp_dir;
    let temp = match &output {
        Some(path) => tempfile::NamedTempFile::new_in(parent(path)).map_err(io)?,
        None => {
            temp_dir = std::env::temp_dir();
            tempfile::NamedTempFile::new_in(&temp_dir).map_err(io)?
        }
    };
    source.backup("main", temp.path(), None).map_err(io)?;
    drop(source);
    let mut c = Connection::open(temp.path()).map_err(io)?;
    c.execute_batch("PRAGMA journal_mode=DELETE").map_err(io)?;
    let tx = c.transaction().map_err(io)?;
    storage::schema(&tx)?;
    let mut ws = read_workspace(&tx)?;
    let ids: Vec<i64> = stored_projects(&tx)?.iter().map(|p| p.id).collect();
    let fail = |e: String| AutomationError::new("refused", e);
    let result = match op {
        EpsOperation::AddFolder {
            name,
            eps_id,
            parent,
        } => {
            let parent = folder_opt(&ws, parent.as_deref())?;
            let id = ws
                .add_folder(parent, name, eps_id.as_deref())
                .map_err(fail)?;
            json!({"added": ws.eps_value(id, "eps_short_name"), "path": ws.path(id)})
        }
        EpsOperation::RenameFolder {
            folder: f,
            name,
            new_eps_id,
        } => {
            if name.is_none() && new_eps_id.is_none() {
                return Err(args("rename_folder needs name or new_eps_id"));
            }
            let id = folder(&ws, f)?;
            let changed = ws
                .rename_folder(id, name.as_deref(), new_eps_id.as_deref())
                .map_err(fail)?;
            json!({"folder": ws.eps_value(id, "eps_short_name"), "path": ws.path(id), "changed": changed})
        }
        EpsOperation::MoveFolder { folder: f, parent } => {
            let id = folder(&ws, f)?;
            let to = folder_opt(&ws, parent.as_deref())?;
            let changed = ws.move_folder(id, to).map_err(fail)?;
            json!({"folder": ws.eps_value(id, "eps_short_name"), "path": ws.path(id), "changed": changed})
        }
        EpsOperation::DeleteFolder {
            folder: f,
            delete_projects,
        } => {
            let id = folder(&ws, f)?;
            let (nodes, going) = ws.folder_contents(id, &ids);
            let gone: Vec<String> = nodes
                .iter()
                .map(|n| ws.eps_value(*n, "eps_short_name"))
                .collect();
            let titles = stored_projects(&tx)?;
            let projects: Vec<Value> = going
                .iter()
                .map(|p| {
                    let title = titles
                        .iter()
                        .find(|s| s.id == *p)
                        .map_or("", |s| s.title.as_str());
                    json!({"workspace_id": p, "project_id": title})
                })
                .collect();
            let baselines = going
                .iter()
                .map(|p| count_rows(&tx, "baselines", *p))
                .sum::<Result<i64>>()?;
            if !going.is_empty() && !delete_projects && !dry_run {
                return Err(AutomationError::new(
                    "refused",
                    format!(
                        "Folder {f} holds {} project(s) ({} item(s) with its folders); deleting it deletes them with their {baselines} baseline(s). Pass delete_projects true (--delete-projects) to delete them, or file them elsewhere first.",
                        going.len(),
                        nodes.len() + going.len()
                    ),
                ));
            }
            ws.delete_eps(id, &ids).map_err(fail)?;
            for p in &going {
                remove_project(&tx, *p)?;
            }
            json!({"deleted": gone, "projects_deleted": projects,
                   "baselines_deleted": baselines,
                   "items": nodes.len() + going.len()})
        }
        EpsOperation::FileProject {
            workspace_id,
            folder: f,
        } => {
            if !ids.contains(workspace_id) {
                return Err(not_found(format!(
                    "No workspace project has the id {workspace_id}"
                )));
            }
            let id = folder(&ws, f)?;
            let changed = ws.place(*workspace_id, id).map_err(fail)?;
            json!({"workspace_id": workspace_id, "folder": ws.eps_value(id, "eps_short_name"), "changed": changed})
        }
        EpsOperation::CreateProject {
            folder: f,
            project_id,
            name,
            planned_start,
            must_finish,
            manager,
            rate_type,
        } => {
            let shared = storage::shared(&tx)?;
            let eps = folder(&ws, f)?;
            let origin = new_project::origin();
            let at = |text: &str, hour: i64| -> Result<Option<i64>> {
                if text.trim().is_empty() {
                    return Ok(None);
                }
                let has_time = text.trim().contains(':');
                let v = parse_absolute(text).ok_or_else(|| {
                    args(format!(
                        "{text:?} is not a date; use YYYY-MM-DD or YYYY-MM-DD HH:MM"
                    ))
                })?;
                Ok(Some(
                    v - origin * openxer_engine::calendar::DAY
                        + if has_time { 0 } else { hour * 60 },
                ))
            };
            let start = at(planned_start, new_project::START_HOUR)?;
            let finish = must_finish
                .as_deref()
                .map(|t| at(t, new_project::FINISH_HOUR))
                .transpose()?
                .flatten();
            let titles: Vec<String> = stored_projects(&tx)?.into_iter().map(|p| p.title).collect();
            // The managers the app's wizard offers: those of the project
            // last open in the app, else of the first project.
            let last: Option<String> = tx
                .query_row(
                    "SELECT value FROM preferences WHERE key='last_project'",
                    [],
                    |r| r.get(0),
                )
                .optional()
                .map_err(invalid)?;
            // With a shared dictionary the managers are in it.
            let dictionary = if shared {
                crate::workspace_db::read_dictionary(&tx)
                    .map_err(invalid)?
                    .0
                    .table("OBS")
                    .cloned()
            } else {
                last.and_then(|v| v.parse::<i64>().ok())
                    .or_else(|| ids.first().copied())
                    .and_then(|id| {
                        tx.query_row("SELECT xer_content FROM projects WHERE id=?1", [id], |r| {
                            r.get::<_, String>(0)
                        })
                        .ok()
                    })
                    .and_then(|text| Document::parse(&text).ok())
                    .and_then(|d| d.table("OBS").cloned())
            };
            let manager = match manager {
                Some(m) => manager_id(dictionary.as_ref(), m)?,
                None => String::new(),
            };
            let nodes: Vec<i64> = ws.eps().iter().map(|n| n.id).collect();
            let spec = new_project::check(
                &Entry {
                    eps,
                    project_id,
                    name: name.as_deref().unwrap_or(""),
                    planned_start: start,
                    must_finish: finish,
                    manager: &manager,
                    rate_type: rate_type.as_deref().unwrap_or(""),
                },
                &titles,
                &nodes,
            )
            .map_err(fail)?;
            // An internal number of its own, as File > New gives.
            let number = crate::baseline_store::next_project_number(&tx, &[]).map_err(invalid)?;
            let mut p = new_project::build(&spec, dictionary.as_ref(), number).map_err(fail)?;
            let key = p.model.project.proj_id;
            let mut source = spec.project_id.clone();
            let mut n = 2;
            while tx
                .query_row(
                    "SELECT 1 FROM projects WHERE source_name=?1 AND project_key=?2",
                    params![source, key],
                    |_| Ok(()),
                )
                .optional()
                .map_err(io)?
                .is_some()
            {
                source = format!("{} ({n})", spec.project_id);
                n += 1;
            }
            p.source_name = source;
            let id = storage::store(&tx, &p, None, None)?;
            ws.place(id, eps).map_err(fail)?;
            json!({"workspace_id": id, "project_id": spec.project_id, "name": spec.name,
                   "project_number": p.model.project.proj_id,
                   "folder": ws.eps_value(eps, "eps_short_name"),
                   "planned_start": date_string(p.model.origin_serial, Some(spec.planned_start)),
                   "must_finish_by": spec.must_finish.map(|f| date_string(p.model.origin_serial, Some(f))),
                   "rate_type": spec.rate_type})
        }
    };
    tx.execute(
        "INSERT INTO preferences(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![KEY, ws.to_text()],
    )
    .map_err(io)?;
    tx.commit().map_err(io)?;
    c.close().map_err(|(_, e)| io(e))?;
    let summary: Vec<Value> = ws
        .eps()
        .iter()
        .map(|n| json!({"eps_id": ws.eps_value(n.id, "eps_short_name"), "name": ws.eps_value(n.id, "eps_name"), "depth": n.depth}))
        .collect();
    if let Some(path) = &output {
        storage::publish(temp, path)?;
    }
    Ok(json!({"output": output, "dry_run": dry_run, "result": result, "folders": summary}))
}
