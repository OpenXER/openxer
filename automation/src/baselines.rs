//! The stored baselines earned value and Monitor Thresholds read, from an
//! OpenXER workspace (read only): the project baseline (the project's
//! `sum_base_proj_id` names one of its stored baselines), the primary user
//! baseline (`baseline_assign`) and the workspace's earned value date choice.
//! An XER input, or a workspace project without them, stands in as its own
//! baseline, as the app does.
use crate::{AutomationError, Result};
use openxer_xer::ev::{self, DateBasis, Loaded};
use openxer_xer::{Document, ProjectData};
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use std::path::Path;
use std::sync::Arc;

/// What earned value reads besides the project itself.
#[derive(Clone, Debug, Default)]
pub struct EvContext {
    pub project: Option<Arc<Loaded>>,
    pub primary: Option<Arc<Loaded>>,
    pub basis: DateBasis,
    /// Baselines assigned but not readable; the project stands in for them.
    pub warnings: Vec<String>,
}

impl EvContext {
    pub fn calculator<'a>(&self, p: &'a ProjectData) -> ev::Calculator<'a> {
        ev::Calculator::new(p, self.project.clone(), self.primary.clone(), self.basis)
    }
    /// The names of the baselines in use, for reports.
    pub fn describe(&self) -> serde_json::Value {
        let name = |b: &Option<Arc<Loaded>>| {
            b.as_ref()
                .map_or("Current Project".to_string(), |l| l.name.clone())
        };
        let mut out = serde_json::json!({"project_baseline":name(&self.project),"primary_baseline":name(&self.primary),
            "earned_value_dates":self.basis.key()});
        if !self.warnings.is_empty() {
            out["warnings"] = serde_json::json!(self.warnings);
        }
        out
    }
}

fn invalid(e: impl std::fmt::Display) -> AutomationError {
    AutomationError::new("invalid_source", e.to_string())
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

/// The baselines of workspace row `workspace_id` in `input`; the default
/// (the project as its own baseline) for an XER input.
pub fn context(input: &Path, workspace_id: Option<i64>, p: &ProjectData) -> Result<EvContext> {
    let Some(parent) = workspace_id else {
        return Ok(EvContext::default());
    };
    let c =
        Connection::open_with_flags(input, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(invalid)?;
    let mut out = EvContext::default();
    if has_table(&c, "preferences") {
        let basis: Option<String> = c
            .query_row(
                "SELECT value FROM preferences WHERE key='earned_value_baseline_dates'",
                [],
                |r| r.get(0),
            )
            .optional()
            .map_err(invalid)?;
        out.basis = DateBasis::from_key(basis.as_deref().unwrap_or(""));
    }
    if !has_table(&c, "baselines") {
        return Ok(out);
    }
    let origin = p.model.origin_serial;
    // A stored baseline that cannot be read leaves the current project in
    // its place, as the app does, and says so.
    let load = |id: i64, warnings: &mut Vec<String>| -> Result<Option<Arc<Loaded>>> {
        let Some((name, key, doc)) = read_row(&c, id, parent)? else {
            return Ok(None);
        };
        match ev::load(&name, doc, key, origin) {
            Ok(loaded) => Ok(Some(Arc::new(loaded))),
            Err(e) => {
                warnings.push(format!(
                    "Baseline {name:?} could not be read ({e}); the current project stands in for it."
                ));
                Ok(None)
            }
        }
    };
    let (project, primary) = assigned_ids(&c, parent, p)?;
    if let Some(id) = project {
        out.project = load(id, &mut out.warnings)?;
    }
    if let Some(id) = primary {
        out.primary = load(id, &mut out.warnings)?;
    }
    Ok(out)
}

/// The stored baseline row `id` of workspace row `parent`: its name,
/// project key and document.
fn read_row(c: &Connection, id: i64, parent: i64) -> Result<Option<(String, i64, Document)>> {
    let row: Option<(String, i64, String, bool)> = c
        .query_row(
            "SELECT name,project_key,xer_content,source_windows_encoding FROM baselines WHERE id=?1 AND parent=?2",
            [id, parent],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()
        .map_err(invalid)?;
    let Some((name, key, text, legacy)) = row else {
        return Ok(None);
    };
    let mut doc = Document::parse(&text).map_err(invalid)?;
    doc.windows_encoding = legacy;
    Ok(Some((name, key, doc)))
}

/// The ids of the project baseline (the project's `sum_base_proj_id` names
/// one of its stored baselines) and the primary user baseline.
fn assigned_ids(
    c: &Connection,
    parent: i64,
    p: &ProjectData,
) -> Result<(Option<i64>, Option<i64>)> {
    let mut project = None;
    let field = openxer_interchange::baseline::project_value(
        &p.document,
        p.model.project.proj_id,
        "sum_base_proj_id",
    );
    if let Ok(key) = field.trim().parse::<i64>() {
        project = c
            .query_row(
                "SELECT id FROM baselines WHERE parent=?1 AND project_key=?2 ORDER BY id",
                [parent, key],
                |r| r.get(0),
            )
            .optional()
            .map_err(invalid)?;
    }
    let mut primary = None;
    if has_table(c, "baseline_assign") {
        let text: Option<String> = c
            .query_row(
                "SELECT primary_bl FROM baseline_assign WHERE parent=?1",
                [parent],
                |r| r.get(0),
            )
            .optional()
            .map_err(invalid)?;
        primary = text.and_then(|s| s.trim().parse::<i64>().ok());
    }
    Ok((project, primary))
}

/// A stored baseline of workspace row `workspace_id` read as a whole
/// project, for comparisons: `primary` false is the project baseline, true
/// the primary user baseline. None for an XER input or when none is
/// assigned.
pub fn stored_project(
    input: &Path,
    workspace_id: Option<i64>,
    p: &ProjectData,
    primary: bool,
) -> Result<Option<(String, ProjectData)>> {
    let Some(parent) = workspace_id else {
        return Ok(None);
    };
    let c =
        Connection::open_with_flags(input, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(invalid)?;
    if !has_table(&c, "baselines") {
        return Ok(None);
    }
    let (project, user) = assigned_ids(&c, parent, p)?;
    let Some(id) = (if primary { user } else { project }) else {
        return Ok(None);
    };
    let Some((name, key, doc)) = read_row(&c, id, parent)? else {
        return Ok(None);
    };
    let project = ProjectData::from_document(doc, key, String::new()).map_err(invalid)?;
    Ok(Some((name, project)))
}
