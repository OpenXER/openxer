//! `check_schedule`: Tools > Check Schedule for agents. The rules are the
//! app's own (`openxer_xer::schedule_check`, which the dialog runs too);
//! this only puts the results rows into JSON.
use crate::{AutomationError, Result};
use openxer_xer::schedule_check::{self as check, CheckParams, CheckResult, Item, CHECKS};
use openxer_xer::ProjectData;
use serde_json::{json, Value};

/// Most Activity IDs or relationships one row lists.
pub const MAX_ITEMS: usize = 500;
/// Listed when the request does not say.
pub const DEFAULT_ITEMS: usize = 50;
/// The offenders every row names (`first`), before a `check` lists more.
pub const ROW_SAMPLE: usize = 5;

fn round(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// The results rows for `p`, as the dialog shows them with its default
/// targets: name, count, of (the population), result (a percent, or the
/// index for BEI), target and pass. With `row` (a check's name, any case)
/// that row also lists its Activity IDs or relationships, up to `limit`.
/// Late Activities and BEI compare with the project's own planned dates when
/// no workspace project baseline is provided.
pub fn check_schedule(p: &ProjectData, row: Option<&str>, limit: Option<usize>) -> Result<Value> {
    check_schedule_with(p, None, row, limit, &crate::analysis::Thresholds::default())
}

/// Like [`check_schedule`], with optional threshold overrides and an optional
/// workspace baseline context for Late Activities and BEI.
pub fn check_schedule_with(
    p: &ProjectData,
    ctx: Option<&crate::baselines::EvContext>,
    row: Option<&str>,
    limit: Option<usize>,
    thresholds: &crate::analysis::Thresholds,
) -> Result<Value> {
    let limit = limit.unwrap_or(DEFAULT_ITEMS);
    if !(1..=MAX_ITEMS).contains(&limit) {
        return Err(AutomationError::new(
            "invalid_arguments",
            format!("limit must be 1..{MAX_ITEMS}"),
        ));
    }
    let wanted = match row {
        Some(name) => Some(
            CHECKS
                .iter()
                .copied()
                .find(|c| c.name().eq_ignore_ascii_case(name.trim()))
                .ok_or_else(|| {
                    AutomationError::new(
                        "invalid_arguments",
                        format!(
                            "No check named {name:?}; checks are {}",
                            CHECKS
                                .iter()
                                .map(|c| c.name())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                    )
                })?,
        ),
        None => None,
    };
    let mut params = CheckParams::default();
    for (value, slot) in [
        (thresholds.long_lag_hours, &mut params.long_lag_hours),
        (thresholds.large_float_hours, &mut params.large_float_hours),
        (
            thresholds.large_duration_hours,
            &mut params.large_duration_hours,
        ),
    ] {
        if let Some(v) = value {
            if !(v.is_finite() && v >= 0.0) {
                return Err(AutomationError::new(
                    "invalid_arguments",
                    "thresholds are hours of 0 or more",
                ));
            }
            *slot = v;
        }
    }
    let (finishes, baseline_label) = match ctx.and_then(|c| c.project.as_ref()) {
        Some(bl) => (
            p.model
                .activities
                .iter()
                .map(|a| bl.by_code.get(&a.task_code).and_then(|d| d.finish))
                .collect::<Vec<_>>(),
            bl.name.clone(),
        ),
        None => (
            check::current_finishes(p),
            "the current project (no project baseline in this input)".to_string(),
        ),
    };
    let results =
        check::run_checks_with(&p.model, &params, &check::expense_tasks(p), Some(&finishes));
    let failed = results.iter().filter(|r| r.passed == Some(false)).count();
    // Each row with offenders names its first few, so one call usually
    // answers without a call per row.
    let rows: Vec<Value> = results
        .iter()
        .map(|r| {
            let mut v = row_json(r);
            if !r.items.is_empty() {
                v["first"] = json!(r
                    .items
                    .iter()
                    .take(ROW_SAMPLE)
                    .map(|i| item_text(p, *i))
                    .collect::<Vec<_>>());
            }
            v
        })
        .collect();
    let mut out = json!({
        "summary": {"checks_run": rows.len(), "checks_failed": failed},
        "rows": rows,
        "thresholds_hours": {"long_lag": params.long_lag_hours, "large_float": params.large_float_hours, "large_duration": params.large_duration_hours},
        "baseline": baseline_label,
    });
    if let Some(c) = wanted {
        let r = results
            .iter()
            .find(|r| r.check == c)
            .expect("every check runs");
        let items: Vec<String> = r
            .items
            .iter()
            .take(limit)
            .map(|i| item_text(p, *i))
            .collect();
        out["check"] = json!(c.name());
        out["items"] = json!(items);
        out["items_total"] = json!(r.items.len());
    }
    Ok(out)
}

fn row_json(r: &CheckResult) -> Value {
    let mut v = json!({
        "check": r.check.name(),
        "count": r.items.len(),
        "of": r.population,
        "result": r.value.map(round),
        "target": r.target,
        "pass": r.passed,
    });
    if let Some(note) = r.note {
        v["note"] = json!(note);
    }
    v
}

/// An activity by its Activity ID; a relationship as `PRED -> SUCC TYPE`.
fn item_text(p: &ProjectData, item: Item) -> String {
    match item {
        Item::Activity(i) => p.model.activities[i].task_code.clone(),
        Item::Relationship(l) => {
            let link = &p.model.links[l];
            format!(
                "{} -> {} {}",
                p.model.activities[link.pred].task_code,
                p.model.activities[link.succ].task_code,
                link.link_type.as_str().trim_start_matches("PR_")
            )
        }
    }
}
