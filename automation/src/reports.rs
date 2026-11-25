//! `export-scurve` and `export-earned-hours`: the S-curve and cumulative
//! earned hours workbooks of Tools > Reports, from the same figures
//! (`openxer_xer::scurve`) and writers (`openxer_interchange::scurve_xlsx`)
//! the app uses. The project's assigned baselines come from the workspace
//! as earned value reads them; an XER input is its own baseline.
use crate::storage::{self, io, parent, publish};
use crate::{baselines, project_json, AutomationError, Result};
use openxer_interchange::scurve_xlsx::{self, Style};
use openxer_xer::scurve::{self, Breakdown, Detection, Group, Measure, Options, Period, Source};
use serde_json::{json, Value};
use std::io::Write;
use std::path::Path;

fn args(message: impl Into<String>) -> AutomationError {
    AutomationError::new("invalid_arguments", message)
}

/// What the S-curve export is asked for.
pub struct CurveRequest<'a> {
    pub measure: Option<&'a str>,
    pub period: Option<&'a str>,
    pub breakdown: Option<&'a str>,
    pub late: bool,
    pub week_start: Option<&'a str>,
}

fn write(output: &Path, bytes: &[u8]) -> Result<()> {
    let mut temp = tempfile::Builder::new()
        .suffix(".xlsx")
        .tempfile_in(parent(output))
        .map_err(io)?;
    temp.write_all(bytes).map_err(io)?;
    publish(temp, output)
}

fn style(p: &openxer_xer::ProjectData) -> Style {
    Style {
        currency_format: scurve_xlsx::currency_format(&p.document),
        ..Style::default()
    }
}

/// The options asked for, filled in from what the project holds: the
/// detected measure, weekly periods from Sunday and the detected breakdown.
pub fn curve_options(d: &Detection, r: &CurveRequest) -> Result<Options> {
    let measure = match r.measure.filter(|m| *m != "auto") {
        None => d.measure(),
        Some(text) => {
            let m = Measure::from_key(text)
                .ok_or_else(|| args("measure takes auto, labor, cost or duration"))?;
            if !d.measures().contains(&m) {
                return Err(args(format!(
                    "The project has no {} to measure ({})",
                    match m {
                        Measure::Labor => "labor hours",
                        Measure::Cost => "costs",
                        Measure::Duration => "durations",
                    },
                    d.line(d.measure())
                )));
            }
            m
        }
    };
    let period = match r.period {
        None => Period::Week,
        Some(text) => Period::from_key(text)
            .ok_or_else(|| args("period takes day, week, month or financial"))?,
    };
    if period == Period::Financial && !d.financial {
        return Err(args("The project has no financial periods"));
    }
    let breakdown = match r.breakdown {
        None => d.breakdown(measure),
        Some(text) => {
            let b = Breakdown::from_key(text)
                .ok_or_else(|| args("breakdown takes total, resource, wbs1 or wbs2"))?;
            if !d.breakdowns(measure).contains(&b) {
                return Err(args(format!(
                    "{} cannot be broken down by resource",
                    measure.label()
                )));
            }
            b
        }
    };
    let week_starts_sunday = match r.week_start.unwrap_or("sunday") {
        "sunday" => true,
        "monday" => false,
        _ => return Err(args("week_start takes sunday or monday")),
    };
    Ok(Options {
        measure,
        period,
        breakdown,
        late: r.late,
        week_starts_sunday,
    })
}

/// `export-scurve`.
pub(crate) fn export_scurve(
    input: &Path,
    output: &Path,
    project_id: Option<i64>,
    workspace_id: Option<i64>,
    r: &CurveRequest,
) -> Result<Value> {
    crate::interchange::check_new_output(output, &["xlsx"])?;
    let loaded = storage::load(input, project_id, workspace_id)?;
    let p = &loaded.project;
    let ctx = baselines::context(input, loaded.workspace_id, p)?;
    let src = Source {
        project_baseline: ctx.project.clone(),
        primary_baseline: ctx.primary.clone(),
        basis: ctx.basis,
        ..Source::new(p)
    };
    let sources = [src];
    let detection = Detection::of(&sources);
    let options = curve_options(&detection, r)?;
    let mut c = scurve::curve(&sources, options).map_err(|e| AutomationError::new("no_data", e))?;
    c.warnings.extend(ctx.warnings.iter().cloned());
    let bytes = scurve_xlsx::curve_bytes(&c, &[], &style(p))
        .map_err(|e| AutomationError::new("write_failed", e))?;
    write(output, &bytes)?;
    let sum = |v: &[f64]| (v.iter().sum::<f64>() * 100.).round() / 100.;
    Ok(json!({
        "output": output,
        "measure": c.options.measure.key(),
        "period": c.options.period.key(),
        "breakdown": c.options.breakdown.key(),
        "late": c.options.late,
        "week_starts": if c.options.week_starts_sunday { "sunday" } else { "monday" },
        "detected": c.detection,
        "measures_available": detection.measures().iter().map(|m| m.key()).collect::<Vec<_>>(),
        "periods": c.periods.len(),
        "rows": c.lines.len(),
        "totals": {"budgeted": sum(&c.total.budget), "actual": sum(&c.total.actual),
                   "remaining": sum(&c.total.remaining)},
        "earned_to_data_date": c.earned.map(|e| (e * 100.).round() / 100.),
        "budgeted_from": c.budget_source,
        "warnings": c.warnings,
        "project": project_json(p),
    }))
}

/// `export-earned-hours`.
pub(crate) fn export_earned_hours(
    input: &Path,
    output: &Path,
    project_id: Option<i64>,
    workspace_id: Option<i64>,
    breakdown: Option<&str>,
) -> Result<Value> {
    crate::interchange::check_new_output(output, &["xlsx"])?;
    let group = match breakdown {
        None => Group::Resource,
        Some(text) => Group::from_key(text)
            .ok_or_else(|| args("breakdown takes resource, wbs or activity"))?,
    };
    let loaded = storage::load(input, project_id, workspace_id)?;
    let p = &loaded.project;
    let ctx = baselines::context(input, loaded.workspace_id, p)?;
    let src = Source {
        project_baseline: ctx.project.clone(),
        primary_baseline: ctx.primary.clone(),
        basis: ctx.basis,
        ..Source::new(p)
    };
    let mut e = scurve::earned_hours(&src, group).map_err(|e| {
        let code = if e == scurve::NO_LABOR {
            "no_labor_units"
        } else {
            "no_data"
        };
        AutomationError::new(code, e)
    })?;
    e.warnings.extend(ctx.warnings.iter().cloned());
    let bytes = scurve_xlsx::earned_bytes(&e, &[], &style(p))
        .map_err(|e| AutomationError::new("write_failed", e))?;
    write(output, &bytes)?;
    let total = |pick: fn(&scurve::EarnedRow) -> f64| {
        (e.rows.iter().map(pick).sum::<f64>() * 100.).round() / 100.
    };
    Ok(json!({
        "output": output,
        "breakdown": group.key(),
        "rows": e.rows.len(),
        "assignments": e.details.len(),
        "totals": {"budgeted": total(|r| r.budgeted), "earned": total(|r| r.earned),
                   "actual": total(|r| r.actual), "remaining": total(|r| r.remaining)},
        "budgeted_from": e.budget_source,
        "percent_complete": e.percent_rule,
        "warnings": e.warnings,
        "project": project_json(p),
    }))
}
