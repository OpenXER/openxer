//! The rules for a new empty workspace project, shared by the New Project
//! wizard (File > New, the Projects view Add) and `openxer-cli eps
//! create-project`: the entry checks and the project they build.
use crate::assignments::RATE_TYPES;
use crate::{ProjectData, Table, Wbs};

/// Longest Project ID.
pub const MAX_ID: usize = 20;
/// Longest Project Name.
pub const MAX_NAME: usize = 100;
/// Hour of day a Planned Start given without a time takes.
pub const START_HOUR: i64 = 8;
/// Hour of day a Must Finish By given without a time takes.
pub const FINISH_HOUR: i64 = 17;
/// Rate type new assignments take when none is chosen.
pub const DEFAULT_RATE_TYPE: &str = "COST_PER_QTY";

/// A checked entry, ready to build. Dates are minutes from the model origin
/// every new project starts from ([`origin`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Spec {
    pub eps: i64,
    pub project_id: String,
    pub name: String,
    pub planned_start: i64,
    pub must_finish: Option<i64>,
    /// `obs_id` of the responsible manager; empty for none.
    pub manager: String,
    /// Default rate type of new assignments (`COST_PER_QTY` .. `COST_PER_QTY5`).
    pub rate_type: String,
}

/// The model origin every new project starts from.
pub fn origin() -> i64 {
    crate::starter().model.origin_serial
}

/// What was entered, dates already read.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Entry<'a> {
    pub eps: i64,
    pub project_id: &'a str,
    pub name: &'a str,
    pub planned_start: Option<i64>,
    pub must_finish: Option<i64>,
    pub manager: &'a str,
    pub rate_type: &'a str,
}

/// Checks an entry against the workspace's Project IDs (`taken`) and EPS
/// nodes.
pub fn check(e: &Entry, taken: &[String], eps_nodes: &[i64]) -> Result<Spec, String> {
    let id = e.project_id.trim();
    if id.is_empty() {
        return Err("Enter a Project ID.".into());
    }
    if id.chars().count() > MAX_ID {
        return Err(format!(
            "The Project ID is longer than {MAX_ID} characters."
        ));
    }
    if id.chars().any(char::is_control) {
        return Err("The Project ID cannot hold control characters.".into());
    }
    if taken.iter().any(|t| t.trim().eq_ignore_ascii_case(id)) {
        return Err(format!("A project with the ID {id} already exists."));
    }
    let name = e.name.trim();
    if name.chars().count() > MAX_NAME {
        return Err(format!(
            "The Project Name is longer than {MAX_NAME} characters."
        ));
    }
    if name.chars().any(char::is_control) {
        return Err("The Project Name cannot hold control characters.".into());
    }
    if !eps_nodes.contains(&e.eps) {
        return Err("Choose an EPS node.".into());
    }
    let start = e.planned_start.ok_or("Enter a Planned Start date.")?;
    if e.must_finish.is_some_and(|f| f < start) {
        return Err("Must Finish By is before the Planned Start.".into());
    }
    let rate = if e.rate_type.trim().is_empty() {
        DEFAULT_RATE_TYPE
    } else {
        e.rate_type.trim()
    };
    let rate = RATE_TYPES
        .iter()
        .find(|(code, label, _)| {
            code.eq_ignore_ascii_case(rate) || label.eq_ignore_ascii_case(rate)
        })
        .map(|(code, _, _)| code.to_string())
        .ok_or_else(|| format!("Unknown rate type {rate}."))?;
    Ok(Spec {
        eps: e.eps,
        project_id: id.to_string(),
        name: if name.is_empty() { id } else { name }.to_string(),
        planned_start: start,
        must_finish: e.must_finish,
        manager: e.manager.to_string(),
        rate_type: rate,
    })
}

/// An empty project for `spec`: one project WBS node named after the
/// project, the standard calendar, no activities, and the starting values
/// of the Defaults tab (`NEW_PROJECT_DEFAULTS`). `obs` is the dictionary
/// of the responsible managers the new project carries. `number` is its
/// internal project number, one no workspace project or baseline uses
/// ([`super::numbering::next`]).
pub fn build(spec: &Spec, obs: Option<&Table>, number: i64) -> Result<ProjectData, String> {
    let mut p = crate::starter();
    let all: Vec<i64> = p.model.activities.iter().map(|a| a.task_id).collect();
    p.delete_activities(&all)?;
    p.renumber_project(number)?;
    p.model.project.name = spec.project_id.clone();
    p.model.project.plan_start = Some(spec.planned_start);
    p.model.project.data_date = Some(spec.planned_start);
    p.model.project.plan_end = spec.must_finish;
    p.wbs = vec![Wbs {
        id: 1,
        parent: None,
        code: spec.project_id.clone(),
        name: spec.name.clone(),
        sequence: 0,
    }];
    crate::edit::wbs::sync_projwbs(&mut p);
    let table = p.document.table_mut("PROJWBS");
    if let Some(i) = table
        .rows
        .iter()
        .position(|r| table.value(r, "wbs_id") == "1")
    {
        table.set(i, "proj_node_flag", "Y".into());
        table.set(i, "obs_id", spec.manager.clone());
    }
    if let Some(obs) = obs {
        *p.document.table_mut("OBS") = obs.clone();
    }
    let pid = p.model.project.proj_id;
    let project = p.document.table_mut("PROJECT");
    if let Some(i) = project
        .rows
        .iter()
        .position(|r| project.value(r, "proj_id").trim() == pid.to_string())
    {
        project.set(i, "def_rate_type", spec.rate_type.clone());
    }
    // The Defaults tab: what the project's new activities take.
    crate::edit::new_activity::new_project_defaults(&mut p);
    p.model.validate()?;
    Ok(p)
}
