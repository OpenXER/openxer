//! Read commands over the project registers and earned value: `records`
//! lists expense items, issues, risks or thresholds with their fields as the
//! edits name them; `earned_value` returns each activity's earned value
//! figures as the activity table's earned value columns compute them, with
//! the project totals.
use crate::{baselines, edit, storage, AutomationError, Result};
use openxer_xer::edit::project_records::{self, Kind};
use openxer_xer::ev::{Basis, Parts};
use serde_json::{json, Value};
use std::path::Path;

fn invalid(message: impl Into<String>) -> AutomationError {
    AutomationError::new("invalid_arguments", message)
}

/// The fields a kind of record takes, for the result.
fn field_names(kind: Kind) -> Vec<Value> {
    project_records::fields(kind)
        .iter()
        .map(|f| {
            json!({"field":f.name,"column":f.column,
                "editable":f.kind != project_records::FieldKind::ReadOnly})
        })
        .collect()
}

pub(crate) fn records(
    input: &Path,
    project_id: Option<i64>,
    workspace_id: Option<i64>,
    kind: &str,
    activity: Option<&str>,
) -> Result<Value> {
    let kind = Kind::parse(kind).map_err(invalid)?;
    let loaded = storage::load(input, project_id, workspace_id)?;
    let p = &loaded.project;
    let task = match activity {
        Some(code) => {
            let found: Vec<i64> = p
                .model
                .activities
                .iter()
                .filter(|a| a.task_code == code)
                .map(|a| a.task_id)
                .collect();
            match found.as_slice() {
                [id] => Some(*id),
                [] => return Err(invalid(format!("No activity {code:?}."))),
                _ => return Err(invalid(format!("Activity ID {code:?} is not unique."))),
            }
        }
        None => None,
    };
    let pid = p.model.project.proj_id;
    let list: Vec<Value> = project_records::ids(p, kind)
        .into_iter()
        .filter(|&id| {
            task.is_none_or(|t| match kind {
                Kind::Risk => project_records::risk_activities(&p.document, pid, id).contains(&t),
                _ => {
                    let table = kind.table(&p.document);
                    openxer_xer::edit::records::num(&project_records::value(
                        &p.document,
                        table,
                        kind.key(),
                        id,
                        "task_id",
                    )) == t
                }
            })
        })
        .map(|id| edit::register_json(p, kind, id))
        .collect();
    let mut out = json!({"kind":kind.name(),"count":list.len(),"records":list,
        "fields":field_names(kind),"workspace_id":loaded.workspace_id});
    if kind == Kind::Threshold {
        let parms: Vec<Value> = openxer_xer::edit::thresholds::parameters(&p.document)
            .into_iter()
            .map(|q| json!({"id":q.id,"name":q.name,"field":q.field,"wbs":q.wbs,"activities":q.task}))
            .collect();
        out["parameters"] = Value::Array(parms);
    }
    Ok(out)
}

fn round(v: f64) -> f64 {
    (v * 100.).round() / 100.
}

fn parts_json(x: &Parts) -> Value {
    let eac = x.ac + x.etc;
    let index = |v: Option<f64>| v.map(round);
    json!({"budget_at_completion":round(x.bac),"planned_value":round(x.pv),"earned_value":round(x.ev),
        "actual_cost":round(x.ac),"schedule_variance":round(x.ev - x.pv),"cost_variance":round(x.ev - x.ac),
        "schedule_performance_index":index(x.spi()),"cost_performance_index":index(x.cpi()),
        "estimate_to_complete":round(x.etc),"estimate_at_completion":round(eac),
        "variance_at_completion":round(x.bac - eac)})
}

pub(crate) fn earned_value(
    input: &Path,
    project_id: Option<i64>,
    workspace_id: Option<i64>,
    basis: Option<&str>,
    activity: Option<&str>,
) -> Result<Value> {
    let basis = match basis.unwrap_or("cost") {
        "cost" => Basis::Cost,
        "labor_units" | "units" => Basis::LaborUnits,
        other => {
            return Err(invalid(format!(
                "basis is cost or labor_units, not {other:?}"
            )))
        }
    };
    let loaded = storage::load(input, project_id, workspace_id)?;
    let p = &loaded.project;
    let ctx = baselines::context(input, loaded.workspace_id, p)?;
    let calc = ctx.calculator(p);
    let mut total = Parts::default();
    let mut rows = vec![];
    for (i, a) in p.model.activities.iter().enumerate() {
        let parts = calc.parts(i, basis);
        total.add(parts);
        if activity.is_some_and(|c| c != a.task_code) {
            continue;
        }
        let mut v = parts_json(&parts);
        v["activity"] = json!(a.task_code);
        v["performance_percent_complete"] = json!(round(calc.performance_percent(a)));
        v["schedule_percent_complete"] = json!(calc
            .ev_baseline(a)
            .and_then(|b| openxer_xer::ev::schedule_percent(p, a, &b))
            .map(round));
        rows.push(v);
    }
    if let Some(code) = activity {
        if rows.is_empty() {
            return Err(invalid(format!("No activity {code:?}.")));
        }
    }
    Ok(
        json!({"basis":match basis { Basis::Cost => "cost", Basis::LaborUnits => "labor_units" },
        "baselines":ctx.describe(),"project":parts_json(&total),"activities":rows,
        "workspace_id":loaded.workspace_id}),
    )
}
