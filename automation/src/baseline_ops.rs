//! Maintain Baselines and Assign Baselines: what each command does to the
//! workspace, worked out once for the app and for `openxer-cli` and MCP.
//!
//! A command returns a [`Change`]: every workspace row it touches, as it was
//! and as it will be. The app keeps the change as its undo step; the CLI
//! writes it to a copy of the workspace. [`apply`] writes one side of it.
//!
//! The rules follow the observed behaviour of the scheduler users know:
//! - Add (save a copy) names the copy "<project ID> - B<n>" and
//!   "<project name> - B<n>", n the lowest number not in use; the copy keeps
//!   the project's data date, and its last update date is the time it was
//!   taken.
//! - Add (convert) turns a closed workspace project into a baseline that
//!   keeps its own ID and name, with a blank last update date; an open
//!   project is refused.
//! - Copy makes another baseline of the same project with the next number,
//!   keeping the source's data date, type and last update date.
//! - Restore detaches a baseline into a separate project with the same ID
//!   and name, filed in the owner's EPS folder.
//! - Delete and Restore refuse a baseline that is assigned (project
//!   baseline, or primary, secondary or tertiary user baseline).
//! - Revert Project to Baseline (an OpenXER command of its own) replaces
//!   the open project's data with a baseline's; the baseline stays.
//! - Update carries the project's data into a baseline by the Update
//!   Baseline options (`openxer_interchange::baseline_update`), with no
//!   question first, and sets its last update date; an assigned baseline
//!   may be updated. Its preview is the same call's list of changes.
use crate::baseline_store::{self as store, BaselineRow, Choice, ProjectRow, UserBaselines};
use openxer_interchange::baseline as xb;
use openxer_interchange::baseline_update as xu;
use openxer_xer::workspace::{Workspace, KEY};
use openxer_xer::{Document, ProjectData};
use rusqlite::{params, Connection, OptionalExtension};

/// Convert: the picked project is open.
pub const OPEN_REFUSED: &str =
    "A project that is open cannot become a baseline. Close the project first, then try again.";
/// Convert: the picked project has baselines of its own.
pub const HAS_BASELINES_REFUSED: &str =
    "This project has baselines of its own, so it cannot become a baseline.";
/// Delete of an assigned baseline.
pub const DELETE_ASSIGNED: &str = "The selected baseline is in use as the project baseline or as a primary, secondary or tertiary baseline. A baseline in use cannot be deleted.";
/// Restore of an assigned baseline.
pub const RESTORE_ASSIGNED: &str = "The selected baseline is in use as the project baseline or as a primary, secondary or tertiary baseline. A baseline in use cannot be restored.";
pub const DELETE_CONFIRM: &str = "Delete the selected baseline?";
pub const DELETED: &str = "The baseline was deleted.";
pub const RESTORE_CONFIRM: &str =
    "Detach the selected baseline from the current project and make it a separate project?";
pub const REVERT_CONFIRM: &str = "Replace the data of the current project with the selected baseline? The project keeps its ID, name and baselines, and the baseline stays in the list.";

/// Why a command did not run: `Refused` is a rule the user meets (shown
/// with OK only), `Failed` something that went wrong.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Refused(String),
    Failed(String),
}

impl Error {
    pub fn message(&self) -> &str {
        match self {
            Error::Refused(m) | Error::Failed(m) => m,
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message())
    }
}

impl From<String> for Error {
    fn from(e: String) -> Self {
        Error::Failed(e)
    }
}

impl From<&str> for Error {
    fn from(e: &str) -> Self {
        Error::Failed(e.into())
    }
}

fn refused(message: impl Into<String>) -> Error {
    Error::Refused(message.into())
}

pub type Rows = Vec<(i64, Option<BaselineRow>, Option<BaselineRow>)>;
pub type Projects = Vec<(i64, Option<ProjectRow>, Option<ProjectRow>)>;

/// One baseline command's effect on the workspace (`None` is absent).
#[derive(Clone, Debug, Default)]
pub struct Change {
    /// The workspace project whose baselines change.
    pub parent: i64,
    pub rows: Rows,
    pub projects: Projects,
    pub users: Option<(UserBaselines, UserBaselines)>,
    /// The parent's `sum_base_proj_id` before and after. [`apply`] leaves
    /// it to the caller, which holds the parent's document.
    pub project_baseline: Option<(String, String)>,
    /// The workspace EPS and portfolio text before and after.
    pub workspace: Option<(String, String)>,
    /// The baseline row (or, for Restore, the workspace project) made.
    pub made: Option<i64>,
}

impl Change {
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
            && self.projects.is_empty()
            && self.users.is_none()
            && self.project_baseline.is_none()
            && self.workspace.is_none()
    }
}

/// The project whose baselines a command works on, as it is now.
pub struct Owner<'a> {
    /// Its workspace row.
    pub id: i64,
    /// Its document, with the workspace dictionary (calendars, resources,
    /// baseline types) added back.
    pub doc: &'a Document,
    pub pid: i64,
    /// Project ID and name.
    pub project_id: &'a str,
    pub name: &'a str,
    pub windows_encoding: bool,
}

impl Owner<'_> {
    /// The project baseline field (`sum_base_proj_id`).
    pub fn project_baseline(&self) -> String {
        xb::project_value(self.doc, self.pid, "sum_base_proj_id")
    }
}

/// The time a baseline is taken or updated, as the file stores dates.
pub fn now_stamp() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M").to_string()
}

/// A baseline's document.
pub fn parse(row: &BaselineRow) -> Result<Document, String> {
    let mut doc = Document::parse(&row.xer)?;
    doc.windows_encoding = row.windows_encoding;
    Ok(doc)
}

/// The next free row id of `table`.
pub fn next_id(c: &Connection, table: &str) -> Result<i64, String> {
    c.query_row(
        &format!("SELECT COALESCE(MAX(id),0)+1 FROM {table}"),
        [],
        |r| r.get(0),
    )
    .map_err(|e| e.to_string())
}

/// A baseline project id unused by the owner's file, its baselines and the
/// id its project baseline field names.
pub fn next_key(doc: &Document, pid: i64, rows: &[BaselineRow]) -> i64 {
    doc.projects()
        .into_iter()
        .map(|(id, _)| id)
        .chain(rows.iter().map(|r| r.project_key))
        .chain([
            xb::project_value(doc, pid, "sum_base_proj_id")
                .trim()
                .parse()
                .unwrap_or(0),
            pid,
        ])
        .max()
        .unwrap_or(0)
        + 1
}

/// The lowest n for which no baseline of the project has the ID
/// "<project ID> - B<n>".
pub fn next_number(project_id: &str, rows: &[BaselineRow]) -> usize {
    (1..)
        .find(|n| {
            let id = format!("{project_id} - B{n}");
            !rows.iter().any(|r| r.short_name == id)
        })
        .expect("a free number")
}

/// What a baseline is assigned as: "project", "primary", "secondary",
/// "tertiary" (empty when it is free).
pub fn assigned_roles(c: &Connection, row: &BaselineRow, project_field: &str) -> Vec<&'static str> {
    let mut out = vec![];
    if project_field.trim() == row.project_key.to_string() {
        out.push("project");
    }
    let users = store::user_baselines(c, row.parent);
    for (k, role) in ["primary", "secondary", "tertiary"].into_iter().enumerate() {
        if users.0[k] == Choice::Baseline(row.id) {
            out.push(role);
        }
    }
    out
}

/// The baseline type stored as `stored` (an id; an older workspace kept the
/// name) as the id of a row of the document's BASETYPE table, or "".
pub fn type_id(doc: &Document, stored: &str) -> String {
    let stored = stored.trim();
    if stored.is_empty() {
        return String::new();
    }
    let Some(t) = doc.table("BASETYPE") else {
        return String::new();
    };
    let id = |r: &[String]| t.value(r, "base_type_id").trim().to_string();
    t.rows
        .iter()
        .find(|r| id(r) == stored)
        .or_else(|| t.rows.iter().find(|r| t.value(r, "base_type") == stored))
        .map_or(String::new(), |r| id(r))
}

/// The Baseline Type choices: "<None>" first, then the types by name, as
/// (id, name).
pub fn type_choices(doc: &Document) -> Vec<(String, String)> {
    let mut types: Vec<(String, String)> = doc
        .table("BASETYPE")
        .map(|t| {
            t.rows
                .iter()
                .map(|r| {
                    (
                        t.value(r, "base_type_id").trim().to_string(),
                        t.value(r, "base_type").to_string(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    types.sort_by(|a, b| {
        a.1.to_lowercase()
            .cmp(&b.1.to_lowercase())
            .then(a.1.cmp(&b.1))
    });
    let mut out = vec![(String::new(), "<None>".to_string())];
    out.extend(types);
    out
}

/// A type named by id or name ("" or "<None>" is no type).
fn type_ref(doc: &Document, text: &str) -> Result<String, Error> {
    let text = text.trim();
    if text.is_empty() || text.eq_ignore_ascii_case("<none>") || text.eq_ignore_ascii_case("none") {
        return Ok(String::new());
    }
    type_choices(doc)
        .into_iter()
        .skip(1)
        .find(|(id, name)| id == text || name.eq_ignore_ascii_case(text))
        .map(|(id, _)| id)
        .ok_or_else(|| refused(format!("No baseline type is named {text:?}.")))
}

fn clean(s: &str) -> String {
    s.trim().chars().filter(|c| !c.is_control()).collect()
}

fn get(c: &Connection, id: i64) -> Result<BaselineRow, Error> {
    store::get(c, id)?.ok_or_else(|| Error::Failed("The baseline no longer exists".into()))
}

/// A stored workspace project's document as the app opens it: on a
/// workspace with a shared dictionary its calendars, resources and codes
/// are added back.
pub fn source_document(c: &Connection, id: i64) -> Result<Document, String> {
    let row = store::project_row(c, id)?.ok_or("The project no longer exists")?;
    let mut doc = Document::parse(&row.xer)?;
    doc.windows_encoding = row.windows_encoding;
    if crate::workspace_db::schema(c)? >= crate::workspace_db::DICTIONARY_SCHEMA {
        crate::workspace_db::hydrate(c, &mut doc, Some(id))?;
    }
    Ok(doc)
}

/// Maintain Baselines > Add > Save a copy of the current project.
pub fn add_copy(c: &Connection, o: &Owner, stamp: &str) -> Result<Change, Error> {
    store::ensure(c)?;
    let rows = store::list(c, o.id)?;
    let key = next_key(o.doc, o.pid, &rows);
    let n = next_number(o.project_id, &rows);
    let short = format!("{} - B{n}", o.project_id);
    let name = format!("{} - B{n}", o.name);
    let copy = xb::copy_as_baseline(o.doc, o.pid, key, &short, &name, stamp);
    let info = xb::baseline_info(&copy, key);
    let id = next_id(c, "baselines")?;
    let row = BaselineRow {
        id,
        parent: o.id,
        project_key: key,
        short_name: short,
        name,
        baseline_type: String::new(),
        data_date: info.data_date,
        added: stamp.into(),
        updated: stamp.into(),
        xer: copy.text(),
        windows_encoding: o.windows_encoding,
        import_batch: None,
    };
    Ok(Change {
        parent: o.id,
        rows: vec![(id, None, Some(row))],
        made: Some(id),
        ..Default::default()
    })
}

/// Maintain Baselines > Add > Convert another project: workspace project
/// `source` (its document `source_doc` as [`source_document`] reads it)
/// becomes a baseline of the owner and leaves the project lists. `open` are
/// the workspace ids of the open projects, which are refused.
pub fn convert(
    c: &Connection,
    o: &Owner,
    source: i64,
    source_doc: &Document,
    open: &[i64],
    stamp: &str,
) -> Result<Change, Error> {
    if source == o.id || open.contains(&source) {
        return Err(refused(OPEN_REFUSED));
    }
    store::ensure(c)?;
    if store::count(c, source) > 0 {
        return Err(refused(HAS_BASELINES_REFUSED));
    }
    let src = store::project_row(c, source)?.ok_or("The project no longer exists")?;
    let rows = store::list(c, o.id)?;
    let key = next_key(o.doc, o.pid, &rows);
    let mut copy = xb::extract_project(source_doc, src.project_key);
    let key = xb::renumber_project(&mut copy, src.project_key, &[o.doc], Some(key));
    // It keeps its own ID and name.
    let short = xb::project_value(&copy, key, "proj_short_name");
    let name = xb::project_name(&copy, key);
    xb::mark_baseline(&mut copy, key, o.pid, &short, &name, stamp);
    // Never updated as a baseline yet.
    xb::set_project_value(&mut copy, key, "last_baseline_update_date", "");
    ProjectData::from_document(copy.clone(), key, String::new())?;
    let info = xb::baseline_info(&copy, key);
    let id = next_id(c, "baselines")?;
    let row = BaselineRow {
        id,
        parent: o.id,
        project_key: key,
        short_name: short,
        name,
        baseline_type: String::new(),
        data_date: info.data_date,
        added: stamp.into(),
        updated: String::new(),
        xer: copy.text(),
        windows_encoding: src.windows_encoding,
        import_batch: None,
    };
    Ok(Change {
        parent: o.id,
        rows: vec![(id, None, Some(row))],
        projects: vec![(source, Some(src), None)],
        made: Some(id),
        ..Default::default()
    })
}

/// Maintain Baselines > Copy: another baseline of the same project with the
/// next number, the source's data, data date, type and last update date.
pub fn copy(c: &Connection, o: &Owner, id: i64, stamp: &str) -> Result<Change, Error> {
    store::ensure(c)?;
    let src = get(c, id)?;
    if src.parent != o.id {
        return Err("The baseline belongs to another project".into());
    }
    let rows = store::list(c, o.id)?;
    let key = next_key(o.doc, o.pid, &rows);
    let bl = parse(&src)?;
    let mut doc = xb::extract_project(&bl, src.project_key);
    let key = xb::renumber_project(&mut doc, src.project_key, &[o.doc, &bl], Some(key));
    let n = next_number(o.project_id, &rows);
    let short = format!("{} - B{n}", o.project_id);
    let name = format!("{} - B{n}", o.name);
    xb::mark_baseline(&mut doc, key, o.pid, &short, &name, stamp);
    xb::set_project_value(&mut doc, key, "last_baseline_update_date", &src.updated);
    let new = next_id(c, "baselines")?;
    let row = BaselineRow {
        id: new,
        parent: o.id,
        project_key: key,
        short_name: short,
        name,
        baseline_type: src.baseline_type.clone(),
        data_date: src.data_date.clone(),
        added: stamp.into(),
        updated: src.updated.clone(),
        xer: doc.text(),
        windows_encoding: src.windows_encoding,
        import_batch: None,
    };
    Ok(Change {
        parent: o.id,
        rows: vec![(new, None, Some(row))],
        made: Some(new),
        ..Default::default()
    })
}

/// Maintain Baselines > Delete. A baseline in use is refused.
pub fn delete(c: &Connection, o: &Owner, id: i64) -> Result<Change, Error> {
    store::ensure(c)?;
    let row = get(c, id)?;
    if !assigned_roles(c, &row, &o.project_baseline()).is_empty() {
        return Err(refused(DELETE_ASSIGNED));
    }
    Ok(Change {
        parent: row.parent,
        rows: vec![(id, Some(row), None)],
        ..Default::default()
    })
}

/// The workspace's EPS text as stored ("" when it has none yet).
fn workspace_text(c: &Connection) -> Result<String, String> {
    let has: bool = c
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='preferences'",
            [],
            |_| Ok(()),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .is_some();
    if !has {
        return Ok(String::new());
    }
    c.query_row("SELECT value FROM preferences WHERE key=?1", [KEY], |r| {
        r.get::<_, String>(0)
    })
    .optional()
    .map(Option::unwrap_or_default)
    .map_err(|e| e.to_string())
}

/// Maintain Baselines > Restore: the baseline leaves the list and becomes a
/// separate workspace project with its own ID and name, filed in the
/// owner's EPS folder. A baseline in use is refused, and so is one whose ID
/// another workspace project already has.
pub fn restore(c: &Connection, o: &Owner, id: i64) -> Result<Change, Error> {
    store::ensure(c)?;
    let row = get(c, id)?;
    if !assigned_roles(c, &row, &o.project_baseline()).is_empty() {
        return Err(refused(RESTORE_ASSIGNED));
    }
    let mut doc = parse(&row)?;
    xb::set_project_value(&mut doc, row.project_key, "orig_proj_id", "");
    // The new project gets an internal number of its own: the baseline's
    // number is unique only among its owner's baselines.
    let key = store::next_project_number(c, &openxer_xer::workspace::numbering::in_document(&doc))?;
    openxer_xer::workspace::numbering::renumber(&mut doc, row.project_key, key);
    let p = ProjectData::from_document(doc.clone(), key, String::new())?;
    let title = p.model.project.name.clone();
    let taken = c
        .query_row(
            "SELECT 1 FROM projects WHERE title=?1",
            [&title],
            |_| Ok(()),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .is_some();
    if taken {
        return Err(refused(format!(
            "A project with the ID {title} already exists. Give the baseline another Baseline ID first."
        )));
    }
    let new = next_id(c, "projects")?;
    let project = ProjectRow {
        id: new,
        source_name: store::unique_source(c, &title, key),
        project_key: key,
        title,
        xer: doc.text(),
        windows_encoding: row.windows_encoding,
        level_resources: false,
    };
    let before = workspace_text(c)?;
    let mut ws = Workspace::from_text(&before);
    let workspace = if ws.eps().is_empty() {
        None
    } else {
        let node = ws.node_of(Some(o.id));
        ws.place(new, node)?;
        Some((before, ws.to_text()))
    };
    Ok(Change {
        parent: row.parent,
        rows: vec![(id, Some(row), None)],
        projects: vec![(new, None, Some(project))],
        workspace,
        made: Some(new),
        ..Default::default()
    })
}

/// The Maintain Baselines fields under the list: Baseline ID, Baseline Name
/// and Baseline Type (`None` leaves a field as it is). An unchanged
/// baseline gives an empty change.
pub fn set(
    c: &Connection,
    o: &Owner,
    id: i64,
    baseline_id: Option<&str>,
    name: Option<&str>,
    kind: Option<&str>,
) -> Result<Change, Error> {
    store::ensure(c)?;
    let before = get(c, id)?;
    let short = baseline_id.map_or_else(|| before.short_name.clone(), clean);
    let name = name.map_or_else(|| before.name.clone(), clean);
    let kind = match kind {
        Some(t) => type_ref(o.doc, t)?,
        None => type_id(o.doc, &before.baseline_type),
    };
    if short == before.short_name
        && name == before.name
        && kind == type_id(o.doc, &before.baseline_type)
    {
        return Ok(Change {
            parent: before.parent,
            ..Default::default()
        });
    }
    if short.is_empty() {
        return Err(refused("A baseline needs a Baseline ID."));
    }
    if name.is_empty() {
        return Err(refused("A baseline needs a name."));
    }
    let mut doc = parse(&before)?;
    xb::set_project_names(&mut doc, before.project_key, &short, &name);
    let after = BaselineRow {
        short_name: short,
        name,
        baseline_type: kind,
        xer: doc.text(),
        ..before.clone()
    };
    Ok(Change {
        parent: before.parent,
        rows: vec![(id, Some(before), Some(after))],
        ..Default::default()
    })
}

/// The owner's baseline choices: project baseline, then the user baselines.
pub fn choices(c: &Connection, o: &Owner) -> (Choice, UserBaselines) {
    let field = o.project_baseline();
    let project = store::list(c, o.id)
        .unwrap_or_default()
        .iter()
        .find(|r| field.trim() == r.project_key.to_string())
        .map_or(Choice::Current, |r| Choice::Baseline(r.id));
    (project, store::user_baselines(c, o.id))
}

/// Assign Baselines > OK. `project` `None` leaves the project baseline as it
/// is; `Choice::None` there means the current project.
pub fn assign(
    c: &Connection,
    o: &Owner,
    project: Option<Choice>,
    users: UserBaselines,
) -> Result<Change, Error> {
    store::ensure(c)?;
    let rows = store::list(c, o.id)?;
    let known = |choice: Choice| match choice {
        Choice::Baseline(id) => rows.iter().any(|r| r.id == id),
        _ => true,
    };
    if !users.0.iter().all(|u| known(*u)) || !project.is_none_or(known) {
        return Err("The baseline no longer exists".into());
    }
    let (old_project, old_users) = choices(c, o);
    let mut change = Change {
        parent: o.id,
        ..Default::default()
    };
    if users != old_users {
        change.users = Some((old_users, users));
    }
    let project = project.map(|p| match p {
        Choice::None => Choice::Current,
        p => p,
    });
    if let Some(choice) = project.filter(|p| *p != old_project) {
        let value = match choice {
            Choice::Baseline(id) => rows
                .iter()
                .find(|r| r.id == id)
                .map(|r| r.project_key.to_string())
                .unwrap_or_default(),
            _ => String::new(),
        };
        change.project_baseline = Some((o.project_baseline(), value));
    }
    Ok(change)
}

/// Revert Project to Baseline: the owner's document with its own project
/// rows replaced by baseline `row`'s. The project keeps its id, ID, name,
/// project baseline and dates added; the baseline is not changed.
pub fn reverted(o: &Owner, row: &BaselineRow) -> Result<Document, String> {
    let bl = parse(row)?;
    let current = o.doc;
    let pid = o.pid;
    let mut out = current.clone();
    xb::remove_project_rows(&mut out, pid);
    let mut copy = xb::extract_project(&bl, row.project_key);
    xb::renumber_project(&mut copy, row.project_key, &[current], Some(pid));
    xb::set_project_value(&mut copy, pid, "orig_proj_id", "");
    xb::set_project_names(&mut copy, pid, o.project_id, o.name);
    for field in [
        "sum_base_proj_id",
        "add_date",
        "last_baseline_update_date",
        "base_type_id",
    ] {
        let v = xb::project_value(current, pid, field);
        xb::set_project_value(&mut copy, pid, field, &v);
    }
    xb::append_project(&mut out, &copy, pid);
    out.windows_encoding = current.windows_encoding;
    Ok(out)
}

/// Maintain Baselines > Update: baseline `id` of the owner updated from the
/// owner's document by `options`, for the activities in `only` (Activity
/// IDs, the filter) or all. Returns the change (the baseline row before and
/// after, its Last Update Date set to `stamp`) and the list of changes it
/// holds. The Preview window and a dry run show that list; Update stores
/// the change. An assigned baseline may be updated.
pub fn update(
    c: &Connection,
    o: &Owner,
    id: i64,
    options: &xu::UpdateOptions,
    only: Option<&std::collections::BTreeSet<String>>,
    stamp: &str,
) -> Result<(Change, xu::Preview), Error> {
    store::ensure(c)?;
    let before = get(c, id)?;
    if before.parent != o.id {
        return Err("The baseline belongs to another project".into());
    }
    let doc = parse(&before)?;
    let key = before.project_key;
    let (mut merged, preview) = xu::update_baseline(&doc, key, o.doc, o.pid, options, only)?;
    xb::mark_baseline(
        &mut merged,
        key,
        o.pid,
        &before.short_name,
        &before.name,
        &before.added,
    );
    xb::set_project_value(&mut merged, key, "last_baseline_update_date", stamp);
    let check = ProjectData::from_document(merged.clone(), key, String::new())
        .map_err(|e| format!("The updated baseline would not load: {e}"))?;
    check
        .model
        .validate()
        .map_err(|e| format!("The updated baseline is not valid: {e}"))?;
    let info = xb::baseline_info(&merged, key);
    let after = BaselineRow {
        xer: merged.text(),
        updated: stamp.into(),
        data_date: info.data_date,
        ..before.clone()
    };
    Ok((
        Change {
            parent: before.parent,
            rows: vec![(id, Some(before), Some(after))],
            ..Default::default()
        },
        preview,
    ))
}

/// The text of the update log: when, which baseline from which project,
/// the options that were on, and every change.
pub fn update_log(
    o: &Owner,
    row: &BaselineRow,
    options: &xu::UpdateOptions,
    preview: &xu::Preview,
    stamp: &str,
) -> String {
    let (added, deleted, updated) = preview.counts();
    let mut lines = vec![
        "Update Baseline log".to_string(),
        format!("Run: {stamp}"),
        format!("Project: {} {}", o.project_id, o.name),
        format!("Baseline: {} {}", row.short_name, row.name),
        format!(
            "Activities: {added} added, {deleted} deleted, {updated} updated; WBS elements changed: {}; project changed: {}",
            preview.wbs.len(),
            if preview.project.is_empty() { "no" } else { "yes" }
        ),
        String::new(),
        "Options:".to_string(),
    ];
    if let Ok(serde_json::Value::Object(map)) = serde_json::to_value(options.effective()) {
        for (k, v) in map {
            if k != "log_path" {
                lines.push(format!("  {k}: {v}"));
            }
        }
    }
    lines.push(String::new());
    lines.push("Changes:".to_string());
    lines.extend(preview.lines());
    lines.push(String::new());
    lines.join("\r\n")
}

/// Write one side of a change (`forward`: the state after) to the
/// workspace: projects, baselines, user baselines and the EPS text. The
/// project baseline field is the caller's, which holds the document.
pub fn apply(c: &Connection, ch: &Change, forward: bool) -> Result<(), String> {
    for (id, before, after) in &ch.projects {
        let image = if forward { after } else { before };
        store::put_project_row(c, *id, image.as_ref())?;
    }
    for (id, before, after) in &ch.rows {
        match if forward { after } else { before } {
            Some(row) => {
                store::put(c, row)?;
            }
            None => store::delete(c, *id)?,
        }
    }
    if let Some((before, after)) = ch.users {
        // A deleted project takes its user baseline choices with it (the
        // row cascades), so there is nothing to write then.
        if store::project_row(c, ch.parent)?.is_some() {
            store::set_user_baselines(c, ch.parent, if forward { after } else { before })?;
        }
    }
    if let Some((before, after)) = &ch.workspace {
        let text = if forward { after } else { before };
        c.execute(
            "CREATE TABLE IF NOT EXISTS preferences(key TEXT PRIMARY KEY,value TEXT NOT NULL)",
            [],
        )
        .map_err(|e| e.to_string())?;
        c.execute(
            "INSERT INTO preferences(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![KEY, text],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}
