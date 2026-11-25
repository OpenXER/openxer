//! `baseline list` and the `baseline` edits on an OpenXER workspace:
//! Project > Maintain Baselines (add a copy, convert a project, copy,
//! delete, restore, the Baseline ID, Name and Type fields), Project >
//! Assign Baselines and Project > Revert Project to Baseline. The rules are
//! the app's own (`baseline_ops`); an edit works on a copy of the workspace
//! written to `output`. The command line and the MCP tools come from the
//! definitions here.
use crate::baseline_ops::{self as ops, Owner};
use crate::baseline_store::{self as store, BaselineRow, Choice, UserBaselines};
use crate::storage::{self, invalid, io, parent};
use crate::{AutomationError, Result};
use openxer_interchange::baseline as xb;
use openxer_xer::ProjectData;
use rusqlite::{Connection, OptionalExtension};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// One baseline edit. A baseline is named by its row id (from `baseline
/// list`), its Baseline ID or its name.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum BaselineOperation {
    /// Add > Save a copy of the current project.
    Add,
    /// Add > Convert another project: a closed workspace project (by
    /// workspace id) becomes a baseline of this one.
    Convert { project: i64 },
    /// Copy: another baseline of the project with the next number.
    Copy { baseline: String },
    /// Delete an unassigned baseline.
    Delete { baseline: String },
    /// Restore an unassigned baseline as a separate project.
    Restore { baseline: String },
    /// Revert Project to Baseline: the project takes the baseline's data.
    Revert { baseline: String },
    /// The Baseline ID, Baseline Name and Baseline Type fields.
    Set {
        baseline: String,
        #[serde(default)]
        baseline_id: Option<String>,
        #[serde(default)]
        name: Option<String>,
        /// A baseline type name, or "none".
        #[serde(default, rename = "type")]
        kind: Option<String>,
    },
    /// Assign Baselines: each slot "current", "none" (user baselines) or a
    /// baseline; an omitted slot keeps its choice.
    Assign {
        #[serde(default)]
        project_baseline: Option<String>,
        #[serde(default)]
        primary: Option<String>,
        #[serde(default)]
        secondary: Option<String>,
        #[serde(default)]
        tertiary: Option<String>,
    },
}

impl BaselineOperation {
    fn name(&self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Convert { .. } => "convert",
            Self::Copy { .. } => "copy",
            Self::Delete { .. } => "delete",
            Self::Restore { .. } => "restore",
            Self::Revert { .. } => "revert",
            Self::Set { .. } => "set",
            Self::Assign { .. } => "assign",
        }
    }
}

fn args(message: impl Into<String>) -> AutomationError {
    AutomationError::new("invalid_arguments", message)
}

fn refusal(e: ops::Error) -> AutomationError {
    match e {
        ops::Error::Refused(m) => AutomationError::new("refused", m),
        ops::Error::Failed(m) => AutomationError::new("failed", m),
    }
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

/// The baseline `text` names among `rows`: a row id, a Baseline ID or a
/// name.
fn find(rows: &[BaselineRow], text: &str) -> Result<i64> {
    let text = text.trim();
    if let Ok(id) = text.parse::<i64>() {
        if rows.iter().any(|r| r.id == id) {
            return Ok(id);
        }
    }
    let picks: [fn(&BaselineRow, &str) -> bool; 3] = [
        |r, t| r.short_name == t,
        |r, t| r.name == t,
        |r, t| r.short_name.eq_ignore_ascii_case(t) || r.name.eq_ignore_ascii_case(t),
    ];
    for pick in picks {
        let hits: Vec<i64> = rows
            .iter()
            .filter(|r| pick(r, text))
            .map(|r| r.id)
            .collect();
        match hits.as_slice() {
            [one] => return Ok(*one),
            [] => {}
            _ => {
                return Err(AutomationError::new(
                    "ambiguous",
                    format!(
                        "Several baselines match {text:?}; name it by its id from baseline list"
                    ),
                ))
            }
        }
    }
    Err(AutomationError::new(
        "not_found",
        format!("The project has no baseline {text:?}; see baseline list"),
    ))
}

/// An Assign Baselines slot: "current", "none" or a baseline.
fn choice(rows: &[BaselineRow], text: &str) -> Result<Choice> {
    match text.trim().to_ascii_lowercase().as_str() {
        "current" | "<current project>" | "current project" => Ok(Choice::Current),
        "none" | "<none>" => Ok(Choice::None),
        _ => find(rows, text).map(Choice::Baseline),
    }
}

fn choice_json(rows: &[BaselineRow], choice: Choice) -> Value {
    match choice {
        Choice::None => json!("none"),
        Choice::Current => json!("current"),
        Choice::Baseline(id) => rows.iter().find(|r| r.id == id).map_or(
            json!("none"),
            |r| json!({"id": r.id, "baseline_id": r.short_name, "name": r.name}),
        ),
    }
}

fn type_name(doc: &openxer_xer::Document, stored: &str) -> String {
    let id = ops::type_id(doc, stored);
    ops::type_choices(doc)
        .into_iter()
        .skip(1)
        .find(|(k, _)| *k == id)
        .map(|(_, n)| n)
        .unwrap_or_default()
}

/// One project's band: its baselines (columns of Maintain Baselines) and
/// its assignments (Assign Baselines).
fn band(
    c: &Connection,
    id: i64,
    title: &str,
    doc: &openxer_xer::Document,
    key: i64,
) -> Result<Value> {
    let rows = if has_table(c, "baselines") {
        store::list(c, id).map_err(invalid)?
    } else {
        vec![]
    };
    let field = xb::project_value(doc, key, "sum_base_proj_id");
    let users = if has_table(c, "baseline_assign") {
        store::user_baselines(c, id)
    } else {
        UserBaselines::default()
    };
    let project = rows
        .iter()
        .find(|r| field.trim() == r.project_key.to_string())
        .map_or(Choice::Current, |r| Choice::Baseline(r.id));
    let baselines: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.id,
                "baseline_id": r.short_name,
                "name": r.name,
                "type": type_name(doc, &r.baseline_type),
                "data_date": r.data_date,
                "date_added": r.added,
                "last_update_date": r.updated,
                "assigned": ops::assigned_roles(c, r, &field),
            })
        })
        .collect();
    Ok(json!({
        "workspace_id": id,
        "project_id": title,
        "name": xb::project_name(doc, key),
        "baselines": baselines,
        "assigned": {
            "project_baseline": choice_json(&rows, project),
            "primary": choice_json(&rows, users.0[0]),
            "secondary": choice_json(&rows, users.0[1]),
            "tertiary": choice_json(&rows, users.0[2]),
        },
        "baseline_types": ops::type_choices(doc).into_iter().skip(1).map(|(_, n)| n).collect::<Vec<_>>(),
    }))
}

/// Projects of a workspace with their stored text: (id, title, key, text).
fn projects(c: &Connection) -> Result<Vec<(i64, String, i64, String)>> {
    let mut stmt = c
        .prepare("SELECT id,title,project_key,xer_content FROM projects ORDER BY id")
        .map_err(invalid)?;
    let rows = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .map_err(invalid)?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(invalid)
}

/// A project's band as the workspace `c` holds it now.
fn band_of(c: &Connection, id: i64) -> Result<Value> {
    let p = projects(c)?
        .into_iter()
        .find(|p| p.0 == id)
        .ok_or_else(|| {
            AutomationError::new(
                "project_not_found",
                format!("No workspace project has the id {id}"),
            )
        })?;
    let mut doc = openxer_xer::Document::parse(&p.3).map_err(invalid)?;
    if storage::shared(c)? {
        crate::workspace_db::hydrate(c, &mut doc, Some(id)).map_err(invalid)?;
    }
    band(c, id, &p.1, &doc, p.2)
}

/// `baseline list`: every workspace project (or one) with its baselines.
/// Baselines are not projects: they never appear in the project lists.
pub fn list(input: &Path, workspace_id: Option<i64>) -> Result<Value> {
    if !storage::is_database(input)? {
        return Err(args(
            "baseline list needs an OpenXER workspace database (.db)",
        ));
    }
    let c = storage::read_database(input)?;
    let mut out = vec![];
    for (id, _, _, _) in projects(&c)? {
        if workspace_id.is_some_and(|w| w != id) {
            continue;
        }
        out.push(band_of(&c, id)?);
    }
    if out.is_empty() {
        if let Some(w) = workspace_id {
            return Err(AutomationError::new(
                "project_not_found",
                format!("No workspace project has the id {w}"),
            ));
        }
    }
    Ok(json!({"input": input, "projects": out}))
}

/// Run one baseline edit of workspace project `workspace_id` on a copy of
/// `input` written to `output` (with `dry_run`, only report it).
pub fn edit(
    input: &Path,
    output: Option<PathBuf>,
    dry_run: bool,
    workspace_id: i64,
    op: &BaselineOperation,
) -> Result<Value> {
    match (&output, dry_run) {
        (Some(_), true) => return Err(args("dry_run writes nothing; omit output")),
        (None, false) => return Err(args("output is required unless dry_run is true")),
        (Some(path), false) => {
            storage::check_destination(path)?;
            if !storage::is_database_extension(path) {
                return Err(args(
                    "baseline edits write an OpenXER workspace (.db/.sqlite/.sqlite3)",
                ));
            }
        }
        (None, true) => {}
    }
    if !storage::is_database(input)? {
        return Err(args(
            "baseline edits need an OpenXER workspace database input",
        ));
    }
    let mut loaded = storage::load(input, None, Some(workspace_id))?;
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
    c.execute_batch("PRAGMA journal_mode=DELETE;PRAGMA foreign_keys=ON")
        .map_err(io)?;
    let tx = c.transaction().map_err(io)?;
    storage::schema(&tx)?;
    store::ensure(&tx).map_err(io)?;
    let rows = store::list(&tx, workspace_id).map_err(invalid)?;
    let doc = loaded.project.updated_document();
    let pid = loaded.project.model.project.proj_id;
    let project_id = loaded.project.model.project.name.clone();
    let name = xb::project_name(&doc, pid);
    let owner = Owner {
        id: workspace_id,
        doc: &doc,
        pid,
        project_id: &project_id,
        name: &name,
        windows_encoding: loaded.project.document.windows_encoding,
    };
    let stamp = ops::now_stamp();
    let mut reverted = None;
    let change = match op {
        BaselineOperation::Add => ops::add_copy(&tx, &owner, &stamp),
        BaselineOperation::Convert { project } => {
            if store::project_row(&tx, *project)
                .map_err(invalid)?
                .is_none()
            {
                return Err(AutomationError::new(
                    "project_not_found",
                    format!("No workspace project has the id {project}"),
                ));
            }
            let src = if *project == workspace_id {
                doc.clone()
            } else {
                ops::source_document(&tx, *project).map_err(invalid)?
            };
            // From the command line the project worked on counts as open.
            ops::convert(&tx, &owner, *project, &src, &[workspace_id], &stamp)
        }
        BaselineOperation::Copy { baseline } => {
            ops::copy(&tx, &owner, find(&rows, baseline)?, &stamp)
        }
        BaselineOperation::Delete { baseline } => ops::delete(&tx, &owner, find(&rows, baseline)?),
        BaselineOperation::Restore { baseline } => {
            ops::restore(&tx, &owner, find(&rows, baseline)?)
        }
        BaselineOperation::Set {
            baseline,
            baseline_id,
            name,
            kind,
        } => {
            if baseline_id.is_none() && name.is_none() && kind.is_none() {
                return Err(args("set needs baseline_id, name or type"));
            }
            ops::set(
                &tx,
                &owner,
                find(&rows, baseline)?,
                baseline_id.as_deref(),
                name.as_deref(),
                kind.as_deref(),
            )
        }
        BaselineOperation::Assign {
            project_baseline,
            primary,
            secondary,
            tertiary,
        } => {
            let (_, mut users) = ops::choices(&tx, &owner);
            for (k, slot) in [primary, secondary, tertiary].into_iter().enumerate() {
                if let Some(t) = slot {
                    users.0[k] = choice(&rows, t)?;
                }
            }
            let project = project_baseline
                .as_deref()
                .map(|t| choice(&rows, t))
                .transpose()?;
            if project == Some(Choice::None) {
                return Err(args(
                    "project_baseline is current or a baseline; none is for the user baselines",
                ));
            }
            ops::assign(&tx, &owner, project, users)
        }
        BaselineOperation::Revert { baseline } => {
            let id = find(&rows, baseline)?;
            let row = store::get(&tx, id).map_err(invalid)?.expect("listed");
            let doc = ops::reverted(&owner, &row).map_err(invalid)?;
            let mut p = ProjectData::from_document(doc, pid, loaded.project.source_name.clone())
                .map_err(invalid)?;
            p.model.options.do_level = loaded.project.model.options.do_level;
            p.model.validate().map_err(invalid)?;
            reverted = Some((row.short_name.clone(), p));
            Ok(ops::Change::default())
        }
    }
    .map_err(refusal)?;
    ops::apply(&tx, &change, true).map_err(io)?;
    if let Some((_, after)) = &change.project_baseline {
        xb::set_project_value(&mut loaded.project.document, pid, "sum_base_proj_id", after);
        storage::store(
            &tx,
            &loaded.project,
            Some(workspace_id),
            loaded.mark.as_ref(),
        )?;
    }
    let result = match (op, change.made) {
        (BaselineOperation::Restore { .. }, Some(new)) => {
            let ws = openxer_xer::workspace::Workspace::from_text(
                &tx.query_row(
                    "SELECT value FROM preferences WHERE key=?1",
                    [openxer_xer::workspace::KEY],
                    |r| r.get::<_, String>(0),
                )
                .optional()
                .map_err(io)?
                .unwrap_or_default(),
            );
            let row = store::project_row(&tx, new)
                .map_err(invalid)?
                .expect("restored");
            json!({"workspace_id": new, "project_id": row.title,
                   "folder": ws.eps_value(ws.node_of(Some(new)), "eps_short_name")})
        }
        (_, Some(made)) => {
            let row = store::get(&tx, made).map_err(invalid)?.expect("made");
            json!({"id": row.id, "baseline_id": row.short_name, "name": row.name})
        }
        (BaselineOperation::Revert { .. }, None) => {
            let (from, p) = reverted.expect("reverted");
            let activities = p.model.activities.len();
            storage::store(&tx, &p, Some(workspace_id), loaded.mark.as_ref())?;
            json!({"reverted_to": from, "activities": activities})
        }
        (_, None) => json!({"changed": !change.is_empty()}),
    };
    let band = band_of(&tx, workspace_id)?;
    tx.commit().map_err(io)?;
    c.close().map_err(|(_, e)| io(e))?;
    if let Some(path) = &output {
        storage::publish(temp, path)?;
    }
    Ok(
        json!({"output": output, "dry_run": dry_run, "op": op.name(), "result": result, "project": band}),
    )
}

/// The preview as JSON: the Project, WBS and Activities groups, each change
/// with its fields (project value, baseline value).
fn preview_json(p: &openxer_interchange::baseline_update::Preview) -> Value {
    let (added, deleted, updated) = p.counts();
    json!({
        "activities_added": added,
        "activities_deleted": deleted,
        "activities_updated": updated,
        "project": p.project,
        "wbs": p.wbs,
        "activities": p.activities,
    })
}

/// `baseline update`: Maintain Baselines > Update of baseline `baseline` of
/// workspace project `workspace_id` with `options` (the Update Baseline
/// options; omitted ones take the dialog's defaults). `activities` limits
/// the activities added or updated to those Activity IDs, as a filter does
/// in the app. With `dry_run` it returns the preview and writes nothing;
/// otherwise the updated workspace goes to `output`, and the log to the
/// options' `log_path` when `log_to_file` is on and a path is given.
pub fn update(
    input: &Path,
    output: Option<PathBuf>,
    dry_run: bool,
    workspace_id: i64,
    baseline: &str,
    options: &openxer_interchange::baseline_update::UpdateOptions,
    activities: Option<&[String]>,
) -> Result<Value> {
    match (&output, dry_run) {
        (Some(_), true) => return Err(args("dry_run writes nothing; omit output")),
        (None, false) => return Err(args("output is required unless dry_run is true")),
        (Some(path), false) => {
            storage::check_destination(path)?;
            if !storage::is_database_extension(path) {
                return Err(args(
                    "baseline update writes an OpenXER workspace (.db/.sqlite/.sqlite3)",
                ));
            }
        }
        (None, true) => {}
    }
    if !storage::is_database(input)? {
        return Err(args(
            "baseline update needs an OpenXER workspace database input",
        ));
    }
    if options.use_filter {
        return Err(args(
            "filters (use_filter, filters, filter_match_all) are evaluated by the app; pass the Activity IDs in activities instead",
        ));
    }
    let loaded = storage::load(input, None, Some(workspace_id))?;
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
    c.execute_batch("PRAGMA journal_mode=DELETE;PRAGMA foreign_keys=ON")
        .map_err(io)?;
    let tx = c.transaction().map_err(io)?;
    storage::schema(&tx)?;
    store::ensure(&tx).map_err(io)?;
    let rows = store::list(&tx, workspace_id).map_err(invalid)?;
    let id = find(&rows, baseline)?;
    let doc = loaded.project.updated_document();
    let pid = loaded.project.model.project.proj_id;
    let project_id = loaded.project.model.project.name.clone();
    let name = xb::project_name(&doc, pid);
    let owner = Owner {
        id: workspace_id,
        doc: &doc,
        pid,
        project_id: &project_id,
        name: &name,
        windows_encoding: loaded.project.document.windows_encoding,
    };
    let only: Option<std::collections::BTreeSet<String>> =
        activities.map(|a| a.iter().map(|s| s.trim().to_string()).collect());
    let stamp = ops::now_stamp();
    let (change, preview) =
        ops::update(&tx, &owner, id, options, only.as_ref(), &stamp).map_err(refusal)?;
    let row = rows.iter().find(|r| r.id == id).expect("found");
    let mut log = None;
    if !dry_run {
        ops::apply(&tx, &change, true).map_err(io)?;
        if options.log_to_file && !options.log_path.trim().is_empty() {
            let path = PathBuf::from(options.log_path.trim());
            std::fs::write(
                &path,
                ops::update_log(&owner, row, options, &preview, &stamp),
            )
            .map_err(io)?;
            log = Some(path);
        }
    }
    let band = band_of(&tx, workspace_id)?;
    tx.commit().map_err(io)?;
    c.close().map_err(|(_, e)| io(e))?;
    if let Some(path) = &output {
        storage::publish(temp, path)?;
    }
    Ok(json!({
        "output": output,
        "dry_run": dry_run,
        "op": "update",
        "result": {
            "baseline": {"id": row.id, "baseline_id": row.short_name, "name": row.name},
            "last_update_date": if dry_run { row.updated.clone() } else { stamp },
            "log": log,
            "options": options.effective(),
            "preview": preview_json(&preview),
        },
        "project": band,
    }))
}

/// The `baseline` command line: `list` and the edits. Returns the request
/// for the shared dispatcher.
pub fn parse_cli(
    args_in: &[String],
    flags: impl Fn(&[String], &[&str]) -> Result<BTreeMap<String, String>>,
) -> Result<Value> {
    let sub = args_in.first().map(String::as_str).unwrap_or("");
    let rest = args_in.get(1..).unwrap_or(&[]);
    let (op, own, required): (&str, &[&str], &[&str]) = match sub {
        "list" => ("", &[], &[]),
        "add" => ("add", &[], &[]),
        "convert" => ("convert", &["project"], &["project"]),
        "copy" | "delete" | "restore" | "revert" => (sub, &["baseline"], &["baseline"]),
        "set" => (
            "set",
            &["baseline", "baseline-id", "name", "type"],
            &["baseline"],
        ),
        "assign" => (
            "assign",
            &["project-baseline", "primary", "secondary", "tertiary"],
            &[],
        ),
        "update" => ("update", &["baseline", "options", "activities"], &["baseline"]),
        _ => return Err(args(
            "baseline requires list, add, convert, copy, delete, restore, revert, set, assign or update",
        )),
    };
    let common: &[&str] = if sub == "list" {
        &["input", "workspace-id"]
    } else {
        &["input", "workspace-id", "output", "dry-run"]
    };
    let allowed: Vec<&str> = common.iter().chain(own).copied().collect();
    let fields = flags(rest, &allowed)?;
    for flag in ["input"]
        .iter()
        .chain(if sub == "list" {
            &[][..]
        } else {
            &["workspace-id"][..]
        })
        .chain(required)
    {
        if !fields.contains_key(*flag) {
            return Err(args(format!("baseline {sub} requires --{flag}")));
        }
    }
    let integer = |name: &str, text: &str| -> Result<Value> {
        text.parse::<i64>()
            .map(Value::from)
            .map_err(|_| args(format!("--{name} {text:?} is not an integer")))
    };
    let mut request = Map::new();
    let mut operation = Map::new();
    for (name, value) in &fields {
        match name.as_str() {
            "input" | "output" => {
                request.insert(name.clone(), Value::from(value.as_str()));
            }
            "workspace-id" => {
                request.insert("workspace_id".into(), integer(name, value)?);
            }
            "dry-run" => {
                let flag = value
                    .parse::<bool>()
                    .map_err(|_| args("--dry-run accepts true or false"))?;
                request.insert("dry_run".into(), Value::Bool(flag));
            }
            "project" => {
                operation.insert("project".into(), integer(name, value)?);
            }
            "options" if sub == "update" => {
                let parsed: Value = serde_json::from_str(value)
                    .map_err(|e| args(format!("--options is not a JSON object: {e}")))?;
                if !parsed.is_object() {
                    return Err(args(
                        "--options is a JSON object of Update Baseline options",
                    ));
                }
                request.insert("options".into(), parsed);
            }
            "activities" if sub == "update" => {
                let ids: Vec<Value> = value
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(Value::from)
                    .collect();
                request.insert("activities".into(), Value::Array(ids));
            }
            "baseline" if sub == "update" => {
                request.insert("baseline".into(), Value::from(value.as_str()));
            }
            _ => {
                operation.insert(name.replace('-', "_"), Value::from(value.as_str()));
            }
        }
    }
    if sub == "list" {
        request.insert("command".into(), Value::from("baseline_list"));
    } else if sub == "update" {
        request.insert("command".into(), Value::from("baseline_update"));
    } else {
        operation.insert("op".into(), Value::from(op));
        request.insert("command".into(), Value::from("baseline"));
        request.insert("operation".into(), Value::Object(operation));
    }
    Ok(Value::Object(request))
}

/// `openxer_baseline_edit` takes the operation's fields at the top; they go
/// into the `operation` of a `baseline` command.
pub fn mcp_arguments(args: Map<String, Value>) -> Map<String, Value> {
    let mut top = Map::new();
    let mut operation = Map::new();
    for (k, v) in args {
        if matches!(k.as_str(), "input" | "output" | "dry_run" | "workspace_id") {
            top.insert(k, v);
        } else {
            operation.insert(k, v);
        }
    }
    top.insert("operation".into(), Value::Object(operation));
    top
}

/// The MCP tool name of each `baseline` command.
pub const TOOLS: &[(&str, &str)] = &[
    ("openxer_baseline_list", "baseline_list"),
    ("openxer_baseline_edit", "baseline"),
    ("openxer_baseline_update", "baseline_update"),
];

/// The MCP tool definitions of the `baseline` commands.
pub fn tool_definitions() -> Vec<Value> {
    let output = json!({"type":"object","properties":{"schema_version":{"type":"string"},"ok":{"type":"boolean"},"result":{"type":"object"},"error":{"type":"object"}},"required":["schema_version","ok"]});
    let input = json!({"type":"string","minLength":1,"description":"OpenXER workspace database path; read-only"});
    let text = json!({"type":"string","minLength":1});
    vec![
        json!({"name":"openxer_baseline_list",
            "description":"List the stored baselines of each workspace project (or of workspace_id) as Project > Maintain Baselines shows them: id, Baseline ID, name, type, data date, date added, last update date and what each is assigned as; with the project's Assign Baselines choices (project baseline, primary, secondary, tertiary) and the baseline types. Baselines are not projects and never appear in the project lists. Read-only.",
            "inputSchema":{"type":"object","additionalProperties":false,"properties":{"input":input,"workspace_id":{"type":"integer"}},"required":["input"]},
            "outputSchema":output,
            "annotations":{"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}}),
        json!({"name":"openxer_baseline_edit",
            "description":"Change the baselines of workspace project workspace_id on a copy written to output (or dry_run), with the app's rules: add (a copy of the project named \"<project ID> - B<n>\" / \"<project name> - B<n>\", n the lowest free number), convert (project: the workspace id of a closed project; it becomes a baseline keeping its ID and name, last update date blank), copy (another baseline with the next number, the source's last update date), delete and restore (refused while the baseline is assigned as project, primary, secondary or tertiary baseline; restore makes it a separate project with its ID and name in the owner's EPS folder), revert (Revert Project to Baseline: the project takes the baseline's data and keeps its ID, name and baselines), set (baseline_id, name, type: a baseline type name or none) and assign (project_baseline: current or a baseline; primary, secondary, tertiary: current, none or a baseline; omitted slots keep their choice). A baseline is named by its id from openxer_baseline_list, its Baseline ID or its name. A refused edit fails with the code refused and writes nothing.",
            "inputSchema":{"type":"object","additionalProperties":false,"properties":{
                "input":input,
                "output":{"type":"string","minLength":1,"description":"New workspace path; existing files are refused. Omit with dry_run"},
                "dry_run":{"type":"boolean","default":false},
                "workspace_id":{"type":"integer","description":"The project whose baselines change"},
                "op":{"type":"string","enum":["add","convert","copy","delete","restore","revert","set","assign"]},
                "project":{"type":"integer","description":"convert: the workspace id of the project to convert"},
                "baseline":text,"baseline_id":text,"name":text,"type":text,
                "project_baseline":text,"primary":text,"secondary":text,"tertiary":text
            },"required":["input","workspace_id","op"]},
            "outputSchema":output,
            "annotations":{"readOnlyHint":false,"destructiveHint":false,"idempotentHint":false,"openWorldHint":false}}),
        json!({"name":"openxer_baseline_update",
            "description":"Maintain Baselines > Update: carry data from workspace project workspace_id into its baseline (id from openxer_baseline_list, Baseline ID or name) by the Update Baseline options, with the app's rules; an assigned baseline may be updated and Last Update Date is set. options holds any of the dialog's options (omitted ones take its defaults): General project_details (on), project_udfs, risks_issues, risk_udfs, issue_udfs, documents, document_udfs, wbs_udfs, log_to_file (on) with log_path (a log is written only when a path is given), ignore_last_update and run_optimized (accepted, no effect); Activities add_new, delete_missing, update_existing (on); Activity general (on), relationships, constraints, dates, wbs (on), risks, activity_udfs, codes (on), notebooks (on), steps (on), step_udfs, expenses, expense_udfs, budget_no_assignments, actual_no_assignments; Resource Assignment update_assignments, update_assignment_budget, update_assignment_actual, add_assignments (on), add_assignment_budget (on), add_assignment_actual (on), assignment_udfs, assignment_codes. activities limits the activities added or updated to those Activity IDs (the app's filter). Activities are matched by GUID, then Activity ID. dry_run returns the preview (each changed project, WBS element and activity as Added, Deleted or Updated with field, project value and baseline value) and writes nothing; otherwise the result goes to output with the same preview.",
            "inputSchema":{"type":"object","additionalProperties":false,"properties":{
                "input":input,
                "output":{"type":"string","minLength":1,"description":"New workspace path; existing files are refused. Omit with dry_run"},
                "dry_run":{"type":"boolean","default":false},
                "workspace_id":{"type":"integer","description":"The project whose baseline is updated"},
                "baseline":text,
                "options":{"type":"object","description":"Update Baseline options by name; omitted ones take the dialog's defaults"},
                "activities":{"type":"array","items":{"type":"string"},"description":"Only these Activity IDs are added or updated"}
            },"required":["input","workspace_id","baseline"]},
            "outputSchema":output,
            "annotations":{"readOnlyHint":false,"destructiveHint":false,"idempotentHint":false,"openWorldHint":false}}),
    ]
}
