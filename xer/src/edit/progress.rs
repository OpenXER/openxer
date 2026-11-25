//! Update Progress, Apply Actuals and Recalculate Assignment Costs, shared
//! by the Tools menu and the command line. The rules are pure functions of
//! the project; the caller makes them one undo step and saves.
use super::records;
use crate::assignments::{cost_in, quantity_text};
use crate::{Document, ProjectData};
use openxer_engine::calendar::CalendarAdapter;
use openxer_engine::model::{StatusType, TaskType};
use std::collections::{BTreeSet, HashMap};

/// How remaining durations are recalculated when progress is applied.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RemainingRule {
    /// Remaining work from the new data date to the scheduled finish.
    #[default]
    Always,
    /// Fixed Duration types keep the scheduled finish; Fixed Units types
    /// consume the elapsed working time from the remaining duration.
    DurationType,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// The activity duration type (all are "Fixed ..." in the reference).
pub enum DurationKind {
    Units,
    UnitsTime,
    DurationAndUnitsTime,
    DurationAndUnits,
}
impl DurationKind {
    pub fn from_code(code: &str) -> Self {
        match code {
            "DT_FixedQty" => Self::Units,
            "DT_FixedRate" => Self::UnitsTime,
            "DT_FixedDrtn" => Self::DurationAndUnitsTime,
            _ => Self::DurationAndUnits,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PercentKind {
    Duration,
    Units,
    Physical,
}
impl PercentKind {
    pub fn from_code(code: &str) -> Self {
        match code {
            "CP_Units" => Self::Units,
            "CP_Phys" => Self::Physical,
            _ => Self::Duration,
        }
    }
}

/// One activity as auto progress sees it.
#[derive(Clone, Debug)]
pub struct ProgressInput {
    pub task_type: TaskType,
    pub status: StatusType,
    pub act_start: Option<i64>,
    pub act_finish: Option<i64>,
    pub rem: i64,
    pub original: i64,
    /// Scheduled (remaining) start and finish of the current schedule.
    pub start: Option<i64>,
    pub finish: Option<i64>,
    pub duration: DurationKind,
    pub percent: PercentKind,
    pub physical: f64,
    /// Physical percent is derived from steps and is not set here.
    pub physical_from_steps: bool,
}

/// The progress auto progress records.
#[derive(Clone, Debug, PartialEq)]
pub struct ProgressOutput {
    pub status: StatusType,
    pub act_start: Option<i64>,
    pub act_finish: Option<i64>,
    pub rem: i64,
    pub physical: f64,
}

fn remaining(
    input: &ProgressInput,
    cal: &CalendarAdapter,
    from: i64,
    new_dd: i64,
    rule: RemainingRule,
) -> i64 {
    let to_finish = input
        .finish
        .map_or(input.rem, |f| cal.between(new_dd, f).max(0));
    match (rule, input.duration) {
        (RemainingRule::Always, _)
        | (_, DurationKind::DurationAndUnits | DurationKind::DurationAndUnitsTime) => to_finish,
        (RemainingRule::DurationType, DurationKind::Units | DurationKind::UnitsTime) => {
            (input.rem - cal.between(from, new_dd).max(0)).max(0)
        }
    }
}

/// Record progress as if the activity ran exactly as scheduled up to the new
/// data date. Activities that should have started before the new data date
/// start on their scheduled start (never before the current data date);
/// those that should have finished by then finish on their scheduled finish.
/// Returns `None` when nothing changes. LOE and WBS summary activities take
/// their dates from their relationships and are left alone.
pub fn auto_progress(
    input: &ProgressInput,
    cal: &CalendarAdapter,
    old_dd: i64,
    new_dd: i64,
    rule: RemainingRule,
) -> Option<ProgressOutput> {
    if matches!(
        input.task_type,
        TaskType::Loe | TaskType::Wbs | TaskType::Hamwk
    ) || input.status == StatusType::Completed
        || new_dd <= old_dd
    {
        return None;
    }
    let mut out = ProgressOutput {
        status: input.status,
        act_start: input.act_start,
        act_finish: input.act_finish,
        rem: input.rem,
        physical: input.physical,
    };
    let complete = |out: &mut ProgressOutput, start: i64, finish: i64| {
        out.status = StatusType::Completed;
        out.act_start = Some(start);
        out.act_finish = Some(finish.max(start));
        out.rem = 0;
    };
    match input.status {
        StatusType::NotStarted => match input.task_type {
            TaskType::Mile | TaskType::FinMile => {
                let finish_milestone = input.task_type == TaskType::FinMile;
                let date = if finish_milestone {
                    input.finish.or(input.start)
                } else {
                    input.start.or(input.finish)
                }?;
                let happened = if finish_milestone {
                    date <= new_dd
                } else {
                    date < new_dd
                };
                if !happened {
                    return None;
                }
                let date = date.max(old_dd);
                complete(&mut out, date, date);
            }
            _ => {
                let start = input.start?;
                if start >= new_dd {
                    return None;
                }
                let start = start.max(old_dd);
                match input.finish {
                    Some(f) if f <= new_dd => complete(&mut out, start, f),
                    _ => {
                        out.status = StatusType::InProgress;
                        out.act_start = Some(start);
                        out.rem = remaining(input, cal, start, new_dd, rule);
                    }
                }
            }
        },
        StatusType::InProgress => {
            let finish = input.finish?;
            let start = input.act_start.unwrap_or(old_dd);
            if finish <= new_dd {
                complete(&mut out, start, finish);
            } else {
                out.rem = remaining(input, cal, start.max(old_dd), new_dd, rule);
            }
        }
        StatusType::Completed => return None,
    }
    if input.percent == PercentKind::Physical && !input.physical_from_steps {
        out.physical = if out.status == StatusType::Completed {
            100.
        } else if input.original > 0 {
            let pct = (input.original - out.rem) as f64 / input.original as f64 * 100.;
            (pct.clamp(0., 100.) * 100.).round() / 100.
        } else {
            0.
        };
    }
    let unchanged = out.status == input.status
        && out.act_start == input.act_start
        && out.act_finish == input.act_finish
        && out.rem == input.rem
        && out.physical == input.physical;
    (!unchanged).then_some(out)
}

/// Which activities a progress run may touch.
pub enum ProgressScope<'a> {
    All,
    Selected(&'a BTreeSet<usize>),
    /// Apply Actuals: activities flagged to compute actuals automatically.
    AutoComputeActuals,
}

/// What Apply Actuals does to one activity, from the Auto Compute Actuals
/// flags as observed in the reference: the activity's own flag, or every
/// assignment's resource flagged, progresses the activity and all its
/// assignments; resource flags on only some assignments give only those
/// assignments actuals and leave the activity's dates and duration alone.
struct ActualsPlan {
    activity: bool,
    rows: Vec<usize>,
}

fn actuals_plan(p: &ProjectData, task_id: i64, flagged: &BTreeSet<i64>) -> ActualsPlan {
    let pid = p.model.project.proj_id;
    let rows = p.assignment_rows(task_id);
    let own = records::task_value(&p.document, task_id, pid, "auto_compute_act_flag") == "Y";
    let resource = |&row: &usize| {
        let id = records::get(&p.document, "TASKRSRC", row, "rsrc_id");
        flagged.contains(&records::num(&id))
    };
    let some: Vec<usize> = rows.iter().copied().filter(resource).collect();
    if own || (!rows.is_empty() && some.len() == rows.len()) {
        ActualsPlan {
            activity: true,
            rows,
        }
    } else {
        ActualsPlan {
            activity: false,
            rows: some,
        }
    }
}

/// Resources whose Auto Compute Actuals flag is on.
fn flagged_resources(p: &ProjectData) -> BTreeSet<i64> {
    let Some(t) = p.document.table("RSRC") else {
        return BTreeSet::new();
    };
    t.rows
        .iter()
        .filter(|r| t.value(r, "auto_compute_act_flag") == "Y")
        .map(|r| records::num(t.value(r, "rsrc_id")))
        .collect()
}

/// Working minutes from `from` to `to` on `cal`, none when `to` is earlier.
fn work_between(cal: &CalendarAdapter, from: i64, to: i64) -> i64 {
    if to > from {
        cal.between(from, to).max(0)
    } else {
        0
    }
}

/// Apply Actuals on assignment rows: each gains its budgeted units/time
/// times the working time from its remaining start (never before the old
/// data date) to the new data date, at most its remaining units. Remaining
/// units drop by the same, costs follow at the price, and with the project's
/// "link actual to date and actual this period" option the actual this
/// period grows by it too. `starts` pairs each row with its remaining start
/// before the run. Returns whether a row changed.
fn advance_assignments(
    p: &mut ProjectData,
    cal: &CalendarAdapter,
    starts: &[(usize, i64)],
    old_dd: i64,
    new_dd: i64,
) -> bool {
    let places = crate::assignments::cost_places(&p.document);
    let this_period = p.project_option("act_this_per_link_flag") == "Y";
    let mut changed = false;
    for &(row, start) in starts {
        let Some(view) = p.assignment_view(row) else {
            continue;
        };
        let minutes = work_between(cal, start.max(old_dd), new_dd);
        let rate = if view.budgeted.per_hour > 0. {
            view.budgeted.per_hour
        } else {
            view.remaining.per_hour
        };
        let add = (rate * minutes as f64 / 60.).min(view.remaining.units);
        if add <= 1e-9 {
            continue;
        }
        let linked = records::get(&p.document, "TASKRSRC", row, "cost_qty_link_flag") != "N";
        let regular = view.actual_regular_units + add;
        let mut sets = vec![
            ("act_reg_qty", quantity_text(regular)),
            ("remain_qty", quantity_text(view.remaining.units - add)),
        ];
        if linked {
            sets.push(("act_reg_cost", cost_in(places, regular * view.price)));
        }
        if this_period {
            let units = view.this_period_units + add;
            sets.push(("act_this_per_qty", quantity_text(units)));
            if linked {
                let cost = view.this_period_cost + add * view.price;
                sets.push(("act_this_per_cost", cost_in(places, cost)));
            }
        }
        for (column, value) in sets {
            records::set(&mut p.document, "TASKRSRC", row, column, value);
        }
        p.update_costs(row);
        changed = true;
    }
    changed
}

/// Expenses of activity `i` with Auto Compute Actuals and uniform accrual,
/// once the activity is under way: actual cost is the budget times the
/// share of the activity's original duration worked from its actual start
/// to the new data date (all of it once the activity has finished), the
/// remaining cost the rest. The activity itself is not changed. Returns
/// whether an expense changed.
fn advance_expenses(p: &mut ProjectData, i: usize, cal: &CalendarAdapter, new_dd: i64) -> bool {
    let a = &p.model.activities[i];
    if a.status == StatusType::NotStarted {
        return false;
    }
    let (task_id, pid) = (a.task_id, p.model.project.proj_id);
    let original = p.details.get(&task_id).map_or(0, |d| d.original_duration);
    let share = if a.status == StatusType::Completed {
        1.
    } else if original > 0 {
        let worked = a.act_start.map_or(0, |s| work_between(cal, s, new_dd));
        (worked as f64 / original as f64).clamp(0., 1.)
    } else {
        0.
    };
    let rows = records::rows_for(&p.document, "PROJCOST", task_id, pid);
    let mut changed = false;
    for row in rows {
        let value = |k: &str| records::get(&p.document, "PROJCOST", row, k);
        let uniform = matches!(value("cost_load_type").as_str(), "" | "CL_Uniform");
        if value("auto_compute_act_flag") != "Y" || !uniform {
            continue;
        }
        let budget = records::float(&value("target_cost"));
        let actual = (budget * share * 100.).round() / 100.;
        let mut writes = vec![];
        for (column, v) in [("act_cost", actual), ("remain_cost", budget - actual)] {
            if (records::float(&value(column)) - v).abs() > 0.005 {
                writes.push((column, records::format_number(v)));
            }
        }
        for (column, text) in writes {
            records::set(&mut p.document, "PROJCOST", row, column, text);
            changed = true;
        }
    }
    changed
}

/// Apply auto progress to a project and move its data date. Returns the
/// number of activities that changed.
///
/// Any new data date is accepted. A date equal to or earlier than the
/// current one only sets the data date: no progress is added and none that
/// is already recorded is removed.
///
/// Update Progress (`All`, `Selected`) records the activities' progress as
/// scheduled and spends their Auto Compute Actuals expenses; Apply Actuals
/// (`AutoComputeActuals`) follows the flags as [`ActualsPlan`] describes,
/// with the assignments' actual units and costs as well.
pub fn apply_progress(
    p: &mut ProjectData,
    new_dd: i64,
    scope: ProgressScope,
    rule: RemainingRule,
) -> Result<usize, String> {
    let old_dd = p
        .model
        .project
        .data_date
        .ok_or("The project has no data date.")?;
    if new_dd <= old_dd {
        // The data date may be any date the user sets. One that is not
        // later adds no progress and removes none: actual dates, remaining
        // durations, assignments and expenses stay as they are; only the
        // data date moves (or stays, when it is the same).
        p.model.project.data_date = Some(new_dd);
        return Ok(0);
    }
    let pid = p.model.project.proj_id;
    let steps_flag = p
        .document
        .table("PROJECT")
        .and_then(|t| {
            t.rows
                .iter()
                .find(|r| records::num(t.value(r, "proj_id")) == pid)
                .map(|r| t.value(r, "step_complete_flag") == "Y")
        })
        .unwrap_or(false);
    let leveled = p.model.options.do_level;
    let flagged = match scope {
        ProgressScope::AutoComputeActuals => flagged_resources(p),
        _ => BTreeSet::new(),
    };
    let calendars = p.model.calendars.clone();
    let mut changed = 0;
    let mut assignments_changed = false;
    for i in 0..p.model.activities.len() {
        let a = &p.model.activities[i];
        let task_id = a.task_id;
        let plan = match &scope {
            ProgressScope::All => ActualsPlan {
                activity: true,
                rows: vec![],
            },
            ProgressScope::Selected(set) if set.contains(&i) => ActualsPlan {
                activity: true,
                rows: vec![],
            },
            ProgressScope::Selected(_) => continue,
            ProgressScope::AutoComputeActuals => actuals_plan(p, task_id, &flagged),
        };
        let a = &p.model.activities[i];
        let own_work = matches!(a.task_type, TaskType::Task | TaskType::Resource);
        let open = a.status != StatusType::Completed;
        // Remaining starts before the run: the actuals count from there.
        let mut starts = vec![];
        if own_work && open {
            let fallback = if leveled { a.restart } else { a.early_start }.or(a.target_start);
            for &row in &plan.rows {
                let view = p.assignment_view(row);
                if let Some(start) = view.and_then(|v| v.dates.rem_start).or(fallback) {
                    starts.push((row, start));
                }
            }
        }
        let Some(cal) = calendars.get(a.cal) else {
            continue;
        };
        let mut touched = false;
        if plan.activity {
            touched = progress_activity(p, i, cal, old_dd, new_dd, rule, steps_flag, leveled);
        }
        let started = p.model.activities[i].status != StatusType::NotStarted;
        if started && advance_assignments(p, cal, &starts, old_dd, new_dd) {
            p.sync_task_rollups(task_id);
            assignments_changed = true;
            touched = true;
        }
        if advance_expenses(p, i, cal, new_dd) {
            touched = true;
        }
        if touched {
            changed += 1;
        }
    }
    if assignments_changed {
        p.rebuild_assignments();
    }
    p.model.project.data_date = Some(new_dd);
    Ok(changed)
}

/// Auto progress of activity `i` (status, actual dates, remaining duration
/// and the physical percent it derives). Returns whether it changed.
#[allow(clippy::too_many_arguments)]
fn progress_activity(
    p: &mut ProjectData,
    i: usize,
    cal: &CalendarAdapter,
    old_dd: i64,
    new_dd: i64,
    rule: RemainingRule,
    steps_flag: bool,
    leveled: bool,
) -> bool {
    let pid = p.model.project.proj_id;
    let a = &p.model.activities[i];
    let task_id = a.task_id;
    let value = |column: &str| records::task_value(&p.document, task_id, pid, column);
    let details = p.details.get(&task_id).cloned().unwrap_or_default();
    let percent = PercentKind::from_code(&value("complete_pct_type"));
    let input = ProgressInput {
        task_type: a.task_type,
        status: a.status,
        act_start: a.act_start,
        act_finish: a.act_finish,
        rem: a.rem,
        original: details.original_duration,
        start: if leveled { a.restart } else { a.early_start }.or(a.target_start),
        finish: if leveled { a.reend } else { a.early_finish }.or(a.target_finish),
        duration: DurationKind::from_code(&value("duration_type")),
        percent,
        physical: details.percent,
        physical_from_steps: steps_flag && percent == PercentKind::Physical,
    };
    let Some(out) = auto_progress(&input, cal, old_dd, new_dd, rule) else {
        return false;
    };
    let a = &mut p.model.activities[i];
    a.status = out.status;
    a.act_start = out.act_start;
    a.act_finish = out.act_finish;
    a.rem = out.rem;
    if out.physical != details.percent {
        p.details.entry(task_id).or_default().percent = out.physical;
    }
    true
}

// ======================================================= assignment costs

/// The outcome of Recalculate Assignment Costs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RecalcReport {
    pub updated: usize,
    pub unchanged: usize,
    /// Assignments left alone, with the reason.
    pub skipped: Vec<String>,
}

fn round_cents(v: f64) -> f64 {
    (v * 100.).round() / 100.
}

/// Recalculate assignment prices and costs from the resources' prices:
/// price per unit from the resource rate in effect (by the assignment's rate
/// type), then budgeted, actual regular, actual overtime and remaining cost
/// as units times price. Assignments priced from a role or an override,
/// with costs not linked to units, or whose resource price changes during
/// the assignment are skipped and listed.
pub fn recalculate_assignment_costs(
    doc: &mut Document,
    pid: i64,
    active_only: bool,
    sync_overtime: bool,
) -> RecalcReport {
    let mut report = RecalcReport::default();
    let Some(assignments) = doc.table("TASKRSRC") else {
        return report;
    };
    let resources: HashMap<i64, (String, bool, String)> = doc
        .table("RSRC")
        .map(|t| {
            t.rows
                .iter()
                .map(|r| {
                    (
                        records::num(t.value(r, "rsrc_id")),
                        (
                            t.value(r, "rsrc_short_name").to_string(),
                            t.value(r, "active_flag") != "N",
                            t.value(r, "ot_factor").to_string(),
                        ),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let rates: Vec<(i64, Option<i64>, Vec<String>)> = doc
        .table("RSRCRATE")
        .map(|t| {
            t.rows
                .iter()
                .map(|r| {
                    (
                        records::num(t.value(r, "rsrc_id")),
                        crate::parse_absolute(t.value(r, "start_date")),
                        [
                            "cost_per_qty",
                            "cost_per_qty2",
                            "cost_per_qty3",
                            "cost_per_qty4",
                            "cost_per_qty5",
                        ]
                        .iter()
                        .map(|k| t.value(r, k).to_string())
                        .collect(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let tasks: HashMap<i64, String> = doc
        .table("TASK")
        .map(|t| {
            t.rows
                .iter()
                .map(|r| {
                    (
                        records::num(t.value(r, "task_id")),
                        t.value(r, "task_code").to_string(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let mut writes: Vec<(usize, &'static str, String)> = vec![];
    for (row, r) in assignments.rows.iter().enumerate() {
        let v = |k: &str| assignments.value(r, k);
        if records::num(v("proj_id")) != pid {
            continue;
        }
        let rsrc = records::num(v("rsrc_id"));
        let activity = tasks
            .get(&records::num(v("task_id")))
            .cloned()
            .unwrap_or_default();
        let Some((short, active, rsrc_ot)) = resources.get(&rsrc) else {
            report
                .skipped
                .push(format!("{activity}: the assignment has no resource"));
            continue;
        };
        let what = format!("{activity} / {short}");
        if active_only && !active {
            continue;
        }
        let source = v("cost_per_qty_source_type");
        if !source.is_empty() && source != "ST_Rsrc" {
            report
                .skipped
                .push(format!("{what}: priced from a role or an override"));
            continue;
        }
        if v("cost_qty_link_flag") == "N" {
            report
                .skipped
                .push(format!("{what}: costs are not linked to units"));
            continue;
        }
        let column = match v("rate_type") {
            "COST_PER_QTY2" => 1,
            "COST_PER_QTY3" => 2,
            "COST_PER_QTY4" => 3,
            "COST_PER_QTY5" => 4,
            _ => 0,
        };
        let date = |keys: &[&str]| keys.iter().find_map(|k| crate::parse_absolute(v(k)));
        let start = date(&["act_start_date", "restart_date", "target_start_date"]);
        let end = date(&["act_end_date", "reend_date", "target_end_date"]).or(start);
        let mut own: Vec<(Option<i64>, f64)> = rates
            .iter()
            .filter(|(id, _, _)| *id == rsrc)
            .map(|(_, d, prices)| (*d, records::float(&prices[column])))
            .collect();
        own.sort_by_key(|(d, _)| d.unwrap_or(i64::MIN));
        let in_effect: Vec<f64> = match (start, end) {
            (Some(s), Some(e)) => {
                let first = own.iter().rposition(|(d, _)| d.is_none_or(|d| d <= s));
                own.iter()
                    .enumerate()
                    .filter(|(i, (d, _))| Some(*i) == first || d.is_some_and(|d| d > s && d <= e))
                    .map(|(_, (_, p))| *p)
                    .collect()
            }
            _ => own.iter().map(|(_, p)| *p).collect(),
        };
        let Some(&price) = in_effect.first() else {
            report
                .skipped
                .push(format!("{what}: the resource has no price"));
            continue;
        };
        if in_effect.iter().any(|p| (p - price).abs() > 1e-9) {
            report.skipped.push(format!(
                "{what}: the resource price changes during the assignment"
            ));
            continue;
        }
        let before = writes.len();
        let mut ot = v("ot_factor").to_string();
        if sync_overtime && !rsrc_ot.is_empty() && records::float(rsrc_ot) != records::float(&ot) {
            ot = rsrc_ot.clone();
            writes.push((row, "ot_factor", rsrc_ot.clone()));
        }
        let factor = if ot.trim().is_empty() {
            1.
        } else {
            records::float(&ot)
        };
        let qty = |k: &str| records::float(v(k));
        let mut put = |key: &'static str, value: f64, money: bool| {
            let old = records::float(v(key));
            let value = if money { round_cents(value) } else { value };
            if (old - value).abs() > 0.005 || (v(key).is_empty() && value != 0.) {
                writes.push((row, key, records::format_number(value)));
            }
        };
        put("cost_per_qty", price, false);
        put("target_cost", qty("target_qty") * price, true);
        put("act_reg_cost", qty("act_reg_qty") * price, true);
        put("act_ot_cost", qty("act_ot_qty") * price * factor, true);
        put("remain_cost", qty("remain_qty") * price, true);
        if writes.len() > before {
            report.updated += 1;
        } else {
            report.unchanged += 1;
        }
    }
    let table = doc.table_mut("TASKRSRC");
    for (row, key, value) in writes {
        table.set(row, key, value);
    }
    report
}

/// Apply Actuals: auto progress for the activities set to compute actuals,
/// the data date moved to `new_dd` and the project's `apply_actuals_date`
/// stamped. Returns the number of activities that changed. A project whose
/// new data date is its current one is left as it is, as the reference
/// leaves an open project kept at its own date. An earlier date moves the
/// data date back and stamps it, with no progress added or removed.
pub fn apply_actuals(
    p: &mut ProjectData,
    new_dd: i64,
    rule: RemainingRule,
) -> Result<usize, String> {
    if p.model.project.data_date == Some(new_dd) {
        return Ok(0);
    }
    let count = apply_progress(p, new_dd, ProgressScope::AutoComputeActuals, rule)?;
    let pid = p.model.project.proj_id;
    let origin = p.model.origin_serial;
    if let Some(table) = p.document.table("PROJECT") {
        if let Some(i) = table
            .rows
            .iter()
            .position(|r| records::num(table.value(r, "proj_id")) == pid)
        {
            records::set(
                &mut p.document,
                "PROJECT",
                i,
                "apply_actuals_date",
                crate::date_string(origin, Some(new_dd)),
            );
        }
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openxer_engine::scheduler::schedule;

    const DAY: i64 = 24 * 60;

    /// The starter project scheduled and progressed two weeks: some
    /// activities complete, some under way, the rest not started.
    fn progressed() -> ProjectData {
        let mut p = crate::starter();
        schedule(&mut p.model).unwrap();
        let dd = p.model.project.data_date.unwrap();
        let n = apply_progress(
            &mut p,
            dd + 14 * DAY,
            ProgressScope::All,
            RemainingRule::Always,
        )
        .unwrap();
        assert!(n > 0, "the later date records progress");
        schedule(&mut p.model).unwrap();
        p
    }

    type Progress = Vec<(StatusType, Option<i64>, Option<i64>, i64)>;

    fn progress_of(p: &ProjectData) -> Progress {
        p.model
            .activities
            .iter()
            .map(|a| (a.status, a.act_start, a.act_finish, a.rem))
            .collect()
    }

    #[test]
    fn a_later_data_date_records_progress_and_moves_the_data_date() {
        let p = progressed();
        let statuses: Vec<StatusType> = p.model.activities.iter().map(|a| a.status).collect();
        assert!(statuses.contains(&StatusType::Completed), "{statuses:?}");
        assert!(statuses.contains(&StatusType::InProgress), "{statuses:?}");
        assert!(statuses.contains(&StatusType::NotStarted), "{statuses:?}");
    }

    #[test]
    fn the_same_data_date_is_accepted_and_progresses_nothing() {
        let mut p = progressed();
        let dd = p.model.project.data_date.unwrap();
        let (before, document) = (progress_of(&p), p.document.text());
        for scope in [ProgressScope::All, ProgressScope::AutoComputeActuals] {
            assert_eq!(
                apply_progress(&mut p, dd, scope, RemainingRule::Always),
                Ok(0)
            );
        }
        assert_eq!(apply_actuals(&mut p, dd, RemainingRule::Always), Ok(0));
        assert_eq!(p.model.project.data_date, Some(dd));
        assert_eq!(progress_of(&p), before);
        assert_eq!(p.document.text(), document, "nothing else moved");
    }

    #[test]
    fn an_earlier_data_date_moves_back_and_keeps_recorded_progress() {
        let mut p = progressed();
        let dd = p.model.project.data_date.unwrap();
        let (before, document) = (progress_of(&p), p.document.text());
        let earlier = dd - 10 * DAY;
        assert_eq!(
            apply_progress(&mut p, earlier, ProgressScope::All, RemainingRule::Always),
            Ok(0)
        );
        assert_eq!(p.model.project.data_date, Some(earlier));
        assert_eq!(progress_of(&p), before, "nothing is un-progressed");
        assert_eq!(p.document.text(), document, "nothing else moved");
        // Apply Actuals moves it back further and stamps the date it ran to.
        let more = earlier - 2 * DAY;
        assert_eq!(apply_actuals(&mut p, more, RemainingRule::Always), Ok(0));
        assert_eq!(p.model.project.data_date, Some(more));
        assert_eq!(progress_of(&p), before);
        let pid = p.model.project.proj_id;
        let stamped = p
            .document
            .table("PROJECT")
            .and_then(|t| {
                t.rows
                    .iter()
                    .find(|r| records::num(t.value(r, "proj_id")) == pid)
                    .map(|r| t.value(r, "apply_actuals_date").to_string())
            })
            .unwrap();
        let origin = p.model.origin_serial;
        assert_eq!(stamped, crate::date_string(origin, Some(more)));
        // The schedule runs from the earlier date and keeps every actual.
        schedule(&mut p.model).unwrap();
        for (a, (status, start, finish, _)) in p.model.activities.iter().zip(&before) {
            let actual = (a.status, a.act_start, a.act_finish);
            assert_eq!(actual, (*status, *start, *finish));
        }
        // A later date again progresses from there as usual.
        let n = apply_progress(
            &mut p,
            dd + 7 * DAY,
            ProgressScope::All,
            RemainingRule::Always,
        )
        .unwrap();
        assert!(n > 0);
        assert_eq!(p.model.project.data_date, Some(dd + 7 * DAY));
    }
}
