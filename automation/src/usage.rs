//! `usage`: an assignment's or a resource's time-phased units (or costs) by
//! day, week or month, spread as the desktop's usage views spread them
//! (`openxer_xer::spread`, so resource curves shape it). Read-only.
use crate::{AutomationError, Result};
use openxer_xer::spread::{self, Item, Lookup, Plan, Shaping};
use openxer_xer::{date_string, parse_relative, periods, ProjectData};
use serde_json::{json, Value};

/// Most periods one answer holds.
pub const MAX_PERIODS: usize = 1000;

/// What `usage` reads: the project, an optional activity and resource
/// filter, the interval and an optional date range.
pub struct Request<'a> {
    pub activity: Option<&'a str>,
    pub resource: Option<&'a str>,
    pub interval: Option<&'a str>,
    pub from: Option<&'a str>,
    pub to: Option<&'a str>,
    pub costs: bool,
}

fn invalid(message: impl Into<String>) -> AutomationError {
    AutomationError::new("invalid_arguments", message)
}

fn num(s: &str) -> i64 {
    s.trim().parse().unwrap_or(0)
}

struct Row {
    activity: String,
    resource: String,
    curve: Option<String>,
    duration_type: String,
    shaping: Option<Shaping>,
    task: usize,
    original: i64,
    quantities: [f64; 3],
}

/// The `usage` answer.
pub fn usage(p: &ProjectData, r: &Request) -> Result<Value> {
    let interval = r.interval.unwrap_or("week");
    if !matches!(interval, "day" | "week" | "month") {
        return Err(invalid(format!(
            "interval takes day, week or month, not {interval:?}"
        )));
    }
    if r.activity.is_none() && r.resource.is_none() {
        return Err(invalid(
            "usage needs an activity (Activity ID), a resource (short name) or both",
        ));
    }
    let m = &p.model;
    let origin = m.origin_serial;
    let doc = &p.document;
    let rsrc_id = match r.resource {
        None => None,
        Some(code) => Some(
            doc.table("RSRC")
                .and_then(|t| {
                    t.rows
                        .iter()
                        .find(|row| t.value(row, "rsrc_short_name").trim() == code.trim())
                        .map(|row| num(t.value(row, "rsrc_id")))
                })
                .ok_or_else(|| {
                    AutomationError::new("not_found", format!("No resource {code:?}"))
                })?,
        ),
    };
    let task = match r.activity {
        None => None,
        Some(code) => Some(
            m.activities
                .iter()
                .position(|a| a.task_code == code)
                .ok_or_else(|| {
                    AutomationError::new("not_found", format!("No activity {code:?}"))
                })?,
        ),
    };
    let names: std::collections::HashMap<i64, String> =
        doc.table("RSRC").map_or_else(Default::default, |t| {
            t.rows
                .iter()
                .map(|row| {
                    (
                        num(t.value(row, "rsrc_id")),
                        t.value(row, "rsrc_short_name").to_string(),
                    )
                })
                .collect()
        });
    let lookup = Lookup::new(p);
    let mut rows: Vec<Row> = vec![];
    for x in spread::assignment_rows(p) {
        if task.is_some_and(|k| k != x.task) || rsrc_id.is_some_and(|id| id != x.rsrc) {
            continue;
        }
        let a = &m.activities[x.task];
        rows.push(Row {
            activity: a.task_code.clone(),
            resource: names.get(&x.rsrc).cloned().unwrap_or_default(),
            curve: openxer_xer::edit::curves::curve_label(doc, x.row),
            duration_type: lookup.duration_type(a.task_id).to_string(),
            shaping: x.shaping,
            task: x.task,
            original: x.original,
            quantities: if r.costs { x.costs } else { x.units },
        });
    }
    fn item<'a>(m: &'a openxer_engine::model::Model, x: &'a Row) -> Item<'a> {
        Item {
            activity: &m.activities[x.task],
            original: x.original,
            shaping: x.shaping.as_ref(),
        }
    }
    // Each row's windows and curve position, worked out once.
    let plans: Vec<Plan> = rows.iter().map(|x| Plan::of(m, &item(m, x))).collect();
    // The range: what the caller asked for, else every window of the rows.
    // A window without working time still needs the period holding its
    // start, so it counts as reaching one minute past it.
    let mut first = i64::MAX;
    let mut last = i64::MIN;
    for plan in &plans {
        let w = &plan.windows;
        for (s, f) in [w.budget, w.actual, w.remaining, w.remaining_late]
            .into_iter()
            .flatten()
        {
            first = first.min(s);
            last = last.max(f.max(s + 1));
        }
    }
    let date = |name: &str, text: Option<&str>| -> Result<Option<i64>> {
        match text {
            None => Ok(None),
            Some(t) => parse_relative(origin, t).map_err(|e| invalid(format!("{name}: {e}"))),
        }
    };
    if let Some(v) = date("from", r.from)? {
        first = v;
    }
    if let Some(v) = date("to", r.to)? {
        last = v;
    }
    // Weeks start on Monday here.
    let length = match interval {
        "day" => periods::Interval::Day,
        "week" => periods::Interval::Week { sunday: false },
        _ => periods::Interval::Month,
    };
    let periods = if first != i64::MAX {
        periods::sweep(origin, length, first, last, MAX_PERIODS).map_err(|_| {
            invalid(format!(
                "More than {MAX_PERIODS} periods; use a longer interval or from and to"
            ))
        })?
    } else {
        vec![]
    };
    let n = periods.len();
    let mut totals = [vec![0.; n], vec![0.; n], vec![0.; n], vec![0.; n]];
    let mut stored_totals = [0.; 4];
    // `unplaced`: the part of the stored quantity no period holds (outside
    // the periods, or no window to spread over, such as actual units
    // without an actual start or remaining units on a completed activity),
    // so that total + unplaced = stored.
    let series = |values: &[f64], stored: f64| {
        let total = values.iter().sum::<f64>() + 0.;
        json!({"total": total, "unplaced": stored - total + 0., "per_period": values})
    };
    let mut assignments = vec![];
    for (x, plan) in rows.iter().zip(&plans) {
        let it = item(m, x);
        let [budget, actual, remaining] = x.quantities;
        let stored = [budget, actual, remaining, remaining];
        let mut out = [vec![0.; n], vec![0.; n], vec![0.; n], vec![0.; n]];
        spread::budget(m, &it, plan, budget, &periods, &mut out[0]);
        spread::actual(m, &it, plan, actual, &periods, &mut out[1]);
        spread::remaining(m, &it, plan, remaining, &periods, &mut out[2]);
        spread::remaining_late(m, &it, plan, remaining, &periods, &mut out[3]);
        for (t, o) in totals.iter_mut().zip(&out) {
            for (a, b) in t.iter_mut().zip(o) {
                *a += b;
            }
        }
        for (t, q) in stored_totals.iter_mut().zip(stored) {
            *t += q;
        }
        let w = &plan.windows;
        let window = |v: Option<(i64, i64)>| {
            v.map_or(Value::Null, |(s, f)| {
                json!({"start": date_string(origin, Some(s)), "finish": date_string(origin, Some(f))})
            })
        };
        assignments.push(json!({
            "activity": x.activity,
            "resource": x.resource,
            "curve": x.curve,
            "duration_type": x.duration_type,
            "curve_applies": spread::curve_applies(&x.duration_type),
            "shaped": x.shaping.as_ref().is_some_and(|s| s.shape.is_some()),
            "manual": x.shaping.as_ref().is_some_and(|s| s.manual.is_some()),
            "curve_position": x.shaping.as_ref().and_then(|s| s.shape).map(|_| plan.position),
            "windows": {"budget": window(w.budget), "actual": window(w.actual),
                        "remaining": window(w.remaining), "remaining_late": window(w.remaining_late)},
            "budget": series(&out[0], budget),
            "actual": series(&out[1], actual),
            "remaining": series(&out[2], remaining),
            "remaining_late": series(&out[3], remaining),
            "stored": {"budget": budget, "actual": actual, "remaining": remaining},
        }));
    }
    Ok(json!({
        "measure": if r.costs { "costs" } else { "units" },
        "interval": interval,
        "periods": periods.iter().map(|&(s, e)| json!({
            "start": date_string(origin, Some(s)), "finish": date_string(origin, Some(e))
        })).collect::<Vec<_>>(),
        "assignments": assignments,
        "totals": {"budget": series(&totals[0], stored_totals[0]),
                   "actual": series(&totals[1], stored_totals[1]),
                   "remaining": series(&totals[2], stored_totals[2]),
                   "remaining_late": series(&totals[3], stored_totals[3])},
        "rules": "Periods are [start, finish). Every assignment spreads over its own window: its lag after the activity's start (remaining lag only before the activity starts) and its own duration (units over units/time; without units or without a units/time it spans the activity), its own valid actual dates, on the resource's calendar on a resource dependent activity and the activity's calendar otherwise. Budget spreads over the planned dates, remaining over the early dates (leveled dates when the project levels) from the data date, remaining late over the late dates. Without a curve, or on a duration type other than DT_FixedDrtn and DT_FixedDUR2, a quantity spreads evenly over the window's working time. With a curve it follows the curve over the window: point k (1 to 20) weighs the k-th twentieth of the working duration, point 0 counts only for actuals (at the actual start); actuals follow the curve to the current position, remaining the rest of it. A window without working time puts everything in the period holding its start. When the periods cover a window, its periods add up to the stored quantity exactly; unplaced is the part no period holds (total + unplaced = stored). Leveling ignores curves.",
        "verified": false,
        "verification": "Curve spreading follows OpenXER's reading of the documented behavior; the exact per-period numbers have not yet been measured against other software.",
    }))
}
