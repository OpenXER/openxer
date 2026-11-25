//! Read-only schedule analysis for agents: the numbers the skills need, so
//! an agent never works one out itself. Each answer is built from what the
//! app already computes: Check Schedule (`schedule_check`), the multiple
//! float paths of the scheduler (`openxer_engine::features::activity_paths`),
//! the stored baselines (`baselines`), the usage spread (`usage`) and the
//! stored dates and float of the project. Nothing here changes a file.
use crate::schedule_check::{self as check, CheckParams, Item};
use crate::{AutomationError, Result};
use openxer_engine::calendar::DAY;
use openxer_engine::model::{Activity, CstrType, LinkType, StatusType, TaskType};
use openxer_xer::edit::project_records::wbs_code_path;
use openxer_xer::ev::{displayed_finish, displayed_start};
use openxer_xer::{date_string, parse_relative, status_label, ProjectData};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Most items listed per finding; the rest are counted in `more`.
pub const LIST_CAP: usize = 25;

fn invalid(message: impl Into<String>) -> AutomationError {
    AutomationError::new("invalid_arguments", message)
}

fn round(v: f64) -> f64 {
    (v * 100.).round() / 100. + 0.
}

fn hours(minutes: i64) -> f64 {
    round(minutes as f64 / 60.)
}

/// Average working minutes of a working day on calendar `cal` (a continuous
/// calendar counts every day as 24 hours; 8 hours when the week is empty).
pub fn day_minutes(p: &ProjectData, cal: usize) -> i64 {
    let Some(c) = p.model.calendars.get(cal) else {
        return 480;
    };
    if c.cal.continuous {
        return DAY;
    }
    let days: Vec<i64> = c
        .cal
        .template
        .iter()
        .map(|s| s.iter().map(|(a, b)| b - a).sum::<i64>())
        .filter(|&m| m > 0)
        .collect();
    if days.is_empty() {
        480
    } else {
        days.iter().sum::<i64>() / days.len() as i64
    }
}

fn days_on(p: &ProjectData, cal: usize, minutes: i64) -> f64 {
    round(minutes as f64 / day_minutes(p, cal).max(1) as f64)
}

fn date(p: &ProjectData, v: Option<i64>) -> Value {
    v.map_or(Value::Null, |v| {
        json!(date_string(p.model.origin_serial, Some(v)))
    })
}

/// A model minute on one absolute clock, so two projects compare.
fn absolute(p: &ProjectData, v: i64) -> i64 {
    v + p.model.origin_serial * DAY
}

fn calendar_days(minutes: i64) -> f64 {
    (minutes as f64 / DAY as f64 * 10.).round() / 10. + 0.
}

fn name(p: &ProjectData, a: &Activity) -> String {
    p.details
        .get(&a.task_id)
        .map(|d| d.name.clone())
        .unwrap_or_default()
}

/// A calendar's name (its id when the file gives none).
fn calendar(p: &ProjectData, cal: usize) -> String {
    p.calendar_names
        .get(cal)
        .filter(|n| !n.is_empty())
        .cloned()
        .or_else(|| p.model.calendars.get(cal).map(|c| c.id.clone()))
        .unwrap_or_default()
}

/// The WBS elements from the top level down to `id`, below the project
/// node (codes may themselves hold dots, so paths are not split).
fn wbs_chain(p: &ProjectData, id: i64) -> Vec<i64> {
    let project = openxer_xer::edit::wbs::project_node(p);
    let mut out = vec![];
    let mut at = Some(id).filter(|w| *w != 0);
    while let Some(w) = at.filter(|w| Some(*w) != project) {
        if out.contains(&w) || out.len() > 10_000 {
            break;
        }
        out.push(w);
        at = p.wbs.iter().find(|e| e.id == w).and_then(|e| e.parent);
    }
    out.reverse();
    out
}

fn wbs_name(p: &ProjectData, id: i64) -> String {
    p.wbs
        .iter()
        .find(|w| w.id == id)
        .map_or(String::new(), |w| w.name.clone())
}

fn wbs_of(p: &ProjectData, a: &Activity) -> String {
    p.details
        .get(&a.task_id)
        .map(|d| wbs_code_path(p, d.wbs))
        .unwrap_or_default()
}

pub fn constraint_label(c: CstrType) -> &'static str {
    match c {
        CstrType::None => "",
        CstrType::StartOn => "Start On",
        CstrType::StartOnBefore => "Start On or Before",
        CstrType::StartOnAfter => "Start On or After",
        CstrType::Alap => "As Late As Possible",
        CstrType::FinishOn => "Finish On",
        CstrType::FinishOnBefore => "Finish On or Before",
        CstrType::FinishOnAfter => "Finish On or After",
        CstrType::MandatoryStart => "Mandatory Start",
        CstrType::MandatoryFinish => "Mandatory Finish",
    }
}

fn type_label(t: TaskType) -> &'static str {
    match t {
        TaskType::Task => "Task Dependent",
        TaskType::Mile => "Start Milestone",
        TaskType::FinMile => "Finish Milestone",
        TaskType::Loe => "Level of Effort",
        TaskType::Wbs => "WBS Summary",
        TaskType::Hamwk => "Hammock",
        TaskType::Resource => "Resource Dependent",
    }
}

fn link_label(t: LinkType) -> &'static str {
    t.as_str().trim_start_matches("PR_")
}

fn constraints(p: &ProjectData, a: &Activity) -> Vec<Value> {
    [(a.cstr, a.cstr_date), (a.cstr2, a.cstr_date2)]
        .into_iter()
        .filter(|(c, _)| *c != CstrType::None)
        .map(|(c, d)| json!({"type": constraint_label(c), "date": date(p, d)}))
        .collect()
}

fn leveled(p: &ProjectData) -> bool {
    p.model.options.do_level
}

/// One activity as every analysis lists it.
pub fn activity_row(p: &ProjectData, i: usize) -> Value {
    let a = &p.model.activities[i];
    let original = p.details.get(&a.task_id).map_or(0, |d| d.original_duration);
    json!({
        "activity": a.task_code,
        "name": name(p, a),
        "type": type_label(a.task_type),
        "status": status_label(a.status),
        "wbs": wbs_of(p, a),
        "start": date(p, displayed_start(a, leveled(p))),
        "finish": date(p, displayed_finish(a, leveled(p))),
        "original_duration_hours": hours(original),
        "original_duration_days": days_on(p, a.cal, original),
        "remaining_duration_hours": hours(a.rem),
        "total_float_hours": a.total_float.map(hours),
        "total_float_days": a.total_float.map(|f| days_on(p, a.cal, f)),
        "calendar": calendar(p, a.cal),
        "constraints": constraints(p, a),
    })
}

fn link_row(p: &ProjectData, l: usize) -> Value {
    let link = &p.model.links[l];
    let pred = &p.model.activities[link.pred];
    let succ = &p.model.activities[link.succ];
    json!({
        "predecessor": pred.task_code,
        "predecessor_name": name(p, pred),
        "successor": succ.task_code,
        "successor_name": name(p, succ),
        "type": link_label(link.link_type),
        "lag_hours": hours(link.lag),
        "lag_days": days_on(p, succ.cal, link.lag),
    })
}

fn capped(items: Vec<Value>) -> Value {
    let more = items.len().saturating_sub(LIST_CAP);
    let list: Vec<Value> = items.into_iter().take(LIST_CAP).collect();
    json!({"items": list, "more": more})
}

fn basis(p: &ProjectData) -> Value {
    json!({
        "project": p.model.project.name,
        "data_date": date(p, p.model.project.data_date),
        "scheduled_finish": date(p, p.model.project.scd_end),
        "must_finish_by": date(p, p.model.project.plan_end),
        "leveled": leveled(p),
        "dates": "as last scheduled and stored in the file; schedule the project first when it was changed since",
    })
}

/// The project baseline finish of every activity, or None.
fn baseline_finishes(
    p: &ProjectData,
    ctx: &crate::baselines::EvContext,
) -> (Option<Vec<Option<i64>>>, String) {
    match &ctx.project {
        Some(bl) => (
            Some(
                p.model
                    .activities
                    .iter()
                    .map(|a| bl.by_code.get(&a.task_code).and_then(|d| d.finish))
                    .collect(),
            ),
            bl.name.clone(),
        ),
        None => (None, String::new()),
    }
}

// ------------------------------------------------------------- check schedule

/// Thresholds a caller may give, in hours.
#[derive(Clone, Debug, Default)]
pub struct Thresholds {
    pub long_lag_hours: Option<f64>,
    pub large_float_hours: Option<f64>,
    pub large_duration_hours: Option<f64>,
}

fn with_expenses(p: &ProjectData) -> BTreeSet<i64> {
    p.document
        .table("PROJCOST")
        .map_or_else(Default::default, |t| {
            t.rows
                .iter()
                .filter_map(|r| t.value(r, "task_id").trim().parse().ok())
                .collect()
        })
}

/// Check Schedule, as Tools > Check Schedule runs it, with the activities
/// and relationships each check finds, and the logic warnings the schedule
/// log lists.
pub fn check_schedule(
    p: &ProjectData,
    ctx: &crate::baselines::EvContext,
    t: &Thresholds,
) -> Result<Value> {
    let mut params = CheckParams::default();
    for (value, slot) in [
        (t.long_lag_hours, &mut params.long_lag_hours),
        (t.large_float_hours, &mut params.large_float_hours),
        (t.large_duration_hours, &mut params.large_duration_hours),
    ] {
        if let Some(v) = value {
            if !(v.is_finite() && v >= 0.) {
                return Err(invalid("thresholds are hours of 0 or more"));
            }
            *slot = v;
        }
    }
    let (finishes, baseline) = baseline_finishes(p, ctx);
    let results = check::run_checks_with(&p.model, &params, &with_expenses(p), finishes.as_deref());
    let item = |it: &Item| match *it {
        Item::Activity(i) => activity_row(p, i),
        Item::Relationship(l) => link_row(p, l),
    };
    let mut flagged = 0;
    let checks: Vec<Value> = results
        .iter()
        .map(|r| {
            if r.passed == Some(false) {
                flagged += 1;
            }
            let items: Vec<Value> = r.items.iter().map(item).collect();
            let threshold_hours = match r.check {
                check::Check::LongLags => Some(params.long_lag_hours),
                check::Check::LargeFloat => Some(params.large_float_hours),
                check::Check::LargeDurations => Some(params.large_duration_hours),
                _ => None,
            };
            json!({
                "check": r.check.name(),
                "what": r.check.caption(),
                "count": r.items.len(),
                "out_of": r.population,
                "value": r.value.map(round),
                "value_is": if r.check == check::Check::Bei { "index (completed over due by the baseline)" } else if r.check == check::Check::RelationshipTypes { "percent Finish to Start" } else { "percent" },
                "target": r.target,
                "target_is": if r.check.target_is_minimum() { "the value must be above it" } else { "the value must be below it" },
                "threshold_hours": threshold_hours,
                "passed": r.passed,
                "note": r.note,
                "found": capped(items),
            })
        })
        .collect();
    let list = |ids: Vec<usize>| capped(ids.into_iter().map(|i| activity_row(p, i)).collect());
    let links = |ids: Vec<usize>| capped(ids.into_iter().map(|l| link_row(p, l)).collect());
    let m = &p.model;
    Ok(json!({
        "basis": basis(p),
        "baseline": if finishes.is_some() { json!(baseline) } else { Value::Null },
        "summary": {"checks_run": checks.len(), "checks_failed": flagged},
        "checks": checks,
        "logic_warnings": {
            "without_predecessors": list(check::without_predecessors(m)),
            "without_successors": list(check::without_successors(m)),
            "constrained": list(check::constrained(m)),
            "actuals_after_data_date": list(check::actuals_after_data_date(m)),
            "milestones_with_a_duration_or_odd_ties": list(check::atypical_milestones(m)),
            "finish_milestones_fed_from_another_calendar": links(check::finish_milestone_calendar_mismatches(m)),
            "out_of_sequence_relationships": links(check::out_of_sequence_links(m)),
        },
        "thresholds_note": "Targets and hour limits are the commonly used defaults of Check Schedule unless the caller gave others; durations, lags and float count working time on each activity's own calendar (days in the item rows use that calendar's working day).",
        "not_checked": ["whether ties describe how the work is really built", "resource limits (see openxer_resource_loading)", "the critical path test of delaying one activity (see openxer_paths)"],
    }))
}

// ---------------------------------------------------------------- paths

fn is_open_work(a: &Activity) -> bool {
    !a.is_completed() && !matches!(a.task_type, TaskType::Loe | TaskType::Wbs)
}

fn find(p: &ProjectData, code: &str) -> Result<usize> {
    p.model
        .activities
        .iter()
        .position(|a| a.task_code == code)
        .ok_or_else(|| AutomationError::new("not_found", format!("No activity {code:?}")))
}

/// The open activity that finishes last: the project finish.
fn finish_activity(p: &ProjectData) -> Option<usize> {
    let lv = leveled(p);
    (0..p.model.activities.len())
        .filter(|&i| is_open_work(&p.model.activities[i]))
        .max_by_key(|&i| displayed_finish(&p.model.activities[i], lv).unwrap_or(i64::MIN))
}

/// What holds an activity at the head of a path.
fn held_by(p: &ProjectData, i: usize) -> Value {
    let a = &p.model.activities[i];
    let c = constraints(p, a);
    if !c.is_empty() {
        return json!({"by": "constraint", "constraints": c});
    }
    if a.act_start.is_some() {
        return json!({"by": "progress (started)"});
    }
    if a.preds.is_empty() {
        return json!({"by": "nothing before it (no predecessor): it starts at the data date or project start"});
    }
    json!({"by": "its predecessors (the path ends here because the next tie back carries more float)"})
}

/// The paths into `activity` (or the project finish), ranked by float: path
/// 1 drives it, the next ones are the chains closest behind.
pub fn paths(p: &ProjectData, activity: Option<&str>, count: usize) -> Result<Value> {
    if !(1..=10).contains(&count) {
        return Err(invalid("count is 1 to 10"));
    }
    let target = match activity {
        Some(code) => find(p, code)?,
        None => finish_activity(p)
            .ok_or_else(|| AutomationError::new("not_found", "The project has no open activity"))?,
    };
    let found = openxer_engine::features::activity_paths(&p.model, target, false, count)
        .map_err(|e| AutomationError::new("schedule_rejected", e))?;
    let m = &p.model;
    let mut first_float: Option<i64> = None;
    let mut list = vec![];
    for (n, path) in found.iter().enumerate() {
        if path.is_empty() {
            continue;
        }
        let float = path
            .iter()
            .filter_map(|&i| {
                m.activities[i]
                    .total_float
                    .filter(|_| is_open_work(&m.activities[i]))
            })
            .min();
        if n == 0 {
            first_float = float;
        }
        let gap = float.zip(first_float).map(|(f, g)| f - g);
        let activities: Vec<Value> = path.iter().map(|&i| activity_row(p, i)).collect();
        let shown = activities.len().min(40);
        list.push(json!({
            "path": n + 1,
            "lowest_total_float_hours": float.map(hours),
            "behind_path_1_hours": gap.map(hours),
            "activity_count": path.len(),
            "calendars": path.iter().map(|&i| calendar(p, m.activities[i].cal)).collect::<BTreeSet<_>>(),
            "first_activity_held_by": held_by(p, path[0]),
            "activities": activities.into_iter().take(shown).collect::<Vec<_>>(),
            "more": path.len() - shown,
        }));
    }
    let open: Vec<&Activity> = m.activities.iter().filter(|a| is_open_work(a)).collect();
    let longest = open.iter().filter(|a| a.driving_path).count();
    let zero = open
        .iter()
        .filter(|a| a.total_float.is_some_and(|f| f <= 0))
        .count();
    let negative = open
        .iter()
        .filter(|a| a.total_float.is_some_and(|f| f < 0))
        .count();
    let merges: Vec<Value> = {
        let mut on: HashMap<usize, BTreeSet<usize>> = HashMap::new();
        for (n, path) in found.iter().enumerate() {
            for &i in path {
                on.entry(i).or_default().insert(n + 1);
            }
        }
        let mut points: Vec<(usize, Vec<usize>)> = on
            .into_iter()
            .filter(|(i, set)| {
                set.len() > 1 || m.activities[*i].preds.len() > 1 && set.contains(&1)
            })
            .map(|(i, set)| (i, set.into_iter().collect()))
            .filter(|(i, _)| m.activities[*i].preds.len() > 1)
            .collect();
        points.sort();
        points
            .into_iter()
            .take(10)
            .map(|(i, set)| json!({"activity": m.activities[i].task_code, "name": name(p, &m.activities[i]), "predecessor_count": m.activities[i].preds.len(), "on_paths": set}))
            .collect()
    };
    Ok(json!({
        "basis": basis(p),
        "target": activity_row(p, target),
        "target_is": if activity.is_some() { "the activity asked for" } else { "the project finish (the open activity that finishes last)" },
        "paths": list,
        "merge_points": merges,
        "critical_counts": {
            "open_activities": open.len(),
            "on_the_longest_path": longest,
            "total_float_zero_or_less": zero,
            "negative_total_float": negative,
            "note": "Two common ways to call work critical: on the longest path (the chain that drives the finish), or total float at or below a limit (zero here). They differ when a Must Finish By date, constraints or mixed calendars change float without changing the driving chain.",
        },
        "method": "Paths are the scheduler's multiple float paths ending at the target, from the stored dates and float: path 1 drives it; each later path is the next chain by float. Float of a path on mixed calendars is approximate.",
    }))
}

// ---------------------------------------------------------------- compare

/// What a comparison reads besides the current project.
pub struct Reference {
    pub name: String,
    pub kind: &'static str,
    pub project: ProjectData,
}

struct Side<'a> {
    p: &'a ProjectData,
}

impl<'a> Side<'a> {
    fn new(p: &'a ProjectData) -> Self {
        Self { p }
    }
    fn start(&self, i: usize) -> Option<i64> {
        displayed_start(&self.p.model.activities[i], leveled(self.p)).map(|v| absolute(self.p, v))
    }
    fn finish(&self, i: usize) -> Option<i64> {
        displayed_finish(&self.p.model.activities[i], leveled(self.p)).map(|v| absolute(self.p, v))
    }
}

fn section(mut items: Vec<(f64, Value)>) -> Value {
    items.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let count = items.len();
    capped(items.into_iter().map(|(_, v)| v).collect())
        .as_object()
        .cloned()
        .map_or(Value::Null, |mut o| {
            o.insert("count".into(), json!(count));
            Value::Object(o)
        })
}

/// The current project against a reference (a baseline or an earlier file):
/// dates, activities, logic, durations, constraints, calendars, progress.
pub fn compare(p: &ProjectData, r: &Reference) -> Result<Value> {
    let now = Side::new(p);
    let then = Side::new(&r.project);
    let mn = &p.model;
    let mt = &r.project.model;
    let project_finish = |s: &Side| {
        (0..s.p.model.activities.len())
            .filter_map(|i| s.finish(i))
            .max()
    };
    let (f_then, f_now) = (project_finish(&then), project_finish(&now));
    let mut added = vec![];
    let mut moved = vec![];
    let mut milestones = vec![];
    let mut durations = vec![];
    let mut remaining_up = vec![];
    let mut constraint_changes = vec![];
    let mut calendar_changes = vec![];
    let mut renamed = vec![];
    let mut started = vec![];
    let mut finished = vec![];
    let mut actuals_changed = vec![];
    let mut reverted = vec![];
    let (mut slipped, mut gained) = (0usize, 0usize);
    // Which activities match, were added or deleted, and how the logic
    // changed: the shared core the app's Schedule Comparison reads too.
    let matches = openxer_xer::compare::match_activities(&r.project, p);
    for &i in &matches.added {
        added.push((0., activity_row(p, i)));
    }
    for &(j, i) in &matches.pairs {
        let a = &mn.activities[i];
        let b = &mt.activities[j];
        let key = || json!({"activity": a.task_code, "name": name(p, a)});
        let mv = |x: Option<i64>, y: Option<i64>| x.zip(y).map(|(x, y)| calendar_days(y - x));
        let start_move = mv(then.start(j), now.start(i));
        let finish_move = mv(then.finish(j), now.finish(i));
        if let Some(f) = finish_move.filter(|f| *f != 0.) {
            if f > 0. {
                slipped += 1
            } else {
                gained += 1
            }
            let mut v = key();
            v["finish_then"] = date(&r.project, displayed_finish(b, leveled(&r.project)));
            v["finish_now"] = date(p, displayed_finish(a, leveled(p)));
            v["finish_moved_days"] = json!(f);
            v["start_moved_days"] = json!(start_move);
            v["total_float_hours_then"] = json!(b.total_float.map(hours));
            v["total_float_hours_now"] = json!(a.total_float.map(hours));
            if matches!(a.task_type, TaskType::Mile | TaskType::FinMile) {
                milestones.push((f.abs(), v.clone()));
            }
            moved.push((f.abs(), v));
        }
        let od = |pp: &ProjectData, x: &Activity| {
            pp.details
                .get(&x.task_id)
                .map_or(0, |d| d.original_duration)
        };
        let (o_then, o_now) = (od(&r.project, b), od(p, a));
        if o_then != o_now {
            let mut v = key();
            v["original_hours_then"] = json!(hours(o_then));
            v["original_hours_now"] = json!(hours(o_now));
            durations.push(((o_now - o_then).abs() as f64, v));
        }
        if a.status == StatusType::InProgress && b.status == StatusType::InProgress && a.rem > b.rem
        {
            let mut v = key();
            v["remaining_hours_then"] = json!(hours(b.rem));
            v["remaining_hours_now"] = json!(hours(a.rem));
            remaining_up.push(((a.rem - b.rem) as f64, v));
        }
        let cs = |pp: &ProjectData, x: &Activity| constraints(pp, x);
        if cs(&r.project, b) != cs(p, a) {
            let mut v = key();
            v["then"] = json!(cs(&r.project, b));
            v["now"] = json!(cs(p, a));
            constraint_changes.push((0., v));
        }
        if calendar(&r.project, b.cal) != calendar(p, a.cal) {
            let mut v = key();
            v["then"] = json!(calendar(&r.project, b.cal));
            v["now"] = json!(calendar(p, a.cal));
            calendar_changes.push((0., v));
        }
        if name(&r.project, b) != name(p, a) {
            let mut v = key();
            v["name_then"] = json!(name(&r.project, b));
            renamed.push((0., v));
        }
        if b.act_start.is_none() && a.act_start.is_some() {
            started.push((0., key()));
        }
        if b.act_finish.is_none() && a.act_finish.is_some() {
            finished.push((0., key()));
        }
        let abs_t = |x: Option<i64>| x.map(|v| absolute(&r.project, v));
        let abs_n = |x: Option<i64>| x.map(|v| absolute(p, v));
        if (b.act_start.is_some()
            && a.act_start.is_some()
            && abs_t(b.act_start) != abs_n(a.act_start))
            || (b.act_finish.is_some()
                && a.act_finish.is_some()
                && abs_t(b.act_finish) != abs_n(a.act_finish))
        {
            let mut v = key();
            v["actual_start_then"] = date(&r.project, b.act_start);
            v["actual_start_now"] = date(p, a.act_start);
            v["actual_finish_then"] = date(&r.project, b.act_finish);
            v["actual_finish_now"] = date(p, a.act_finish);
            actuals_changed.push((0., v));
        }
        let rank = |s: StatusType| match s {
            StatusType::NotStarted => 0,
            StatusType::InProgress => 1,
            StatusType::Completed => 2,
        };
        if rank(a.status) < rank(b.status) {
            let mut v = key();
            v["status_then"] = json!(status_label(b.status));
            v["status_now"] = json!(status_label(a.status));
            reverted.push((0., v));
        }
    }
    let deleted: Vec<(f64, Value)> = matches
        .deleted
        .iter()
        .map(|&j| (0., activity_row(&r.project, j)))
        .collect();
    let (mut ties_added, mut ties_removed, mut ties_changed) = (vec![], vec![], vec![]);
    for c in openxer_xer::compare::tie_changes(&r.project, p) {
        let mut v = json!({"predecessor": c.pred, "successor": c.succ});
        if let Some(o) = c.old {
            v["type_then"] = json!(link_label(o.link_type));
            v["lag_hours_then"] = json!(hours(o.lag));
        }
        if let Some(n) = c.new {
            v["type_now"] = json!(link_label(n.link_type));
            v["lag_hours_now"] = json!(hours(n.lag));
        }
        match c.change {
            openxer_xer::compare::Change::Added => ties_added.push((0., v)),
            openxer_xer::compare::Change::Deleted => ties_removed.push((0., v)),
            openxer_xer::compare::Change::Changed => ties_changed.push((0., v)),
        }
    }
    let dd = |pp: &ProjectData| pp.model.project.data_date;
    let summary = json!({
        "activities_added": added.len(), "activities_deleted": deleted.len(),
        "finishes_later": slipped, "finishes_earlier": gained,
        "milestones_moved": milestones.len(),
        "ties_added": ties_added.len(), "ties_removed": ties_removed.len(), "ties_changed": ties_changed.len(),
        "original_durations_changed": durations.len(), "remaining_up_on_work_in_progress": remaining_up.len(),
        "constraints_changed": constraint_changes.len(), "calendars_changed": calendar_changes.len(),
        "renamed": renamed.len(), "started": started.len(), "finished": finished.len(),
        "actual_dates_rewritten": actuals_changed.len(), "status_went_back": reverted.len(),
    });
    Ok(json!({
        "reference": {"name": r.name, "kind": r.kind, "data_date": date(&r.project, dd(&r.project))},
        "current": basis(p),
        "project_finish": {
            "then": f_then.map(|v| date_string(0, Some(v))),
            "now": f_now.map(|v| date_string(0, Some(v))),
            "moved_days": f_then.zip(f_now).map(|(a, b)| calendar_days(b - a)),
        },
        "summary": summary,
        "milestones_moved": section(milestones),
        "biggest_finish_moves": section(moved),
        "activities_added": section(added),
        "activities_deleted": section(deleted),
        "ties_added": section(ties_added),
        "ties_removed": section(ties_removed),
        "ties_changed": section(ties_changed),
        "original_durations_changed": section(durations),
        "remaining_up_on_work_in_progress": section(remaining_up),
        "constraints_changed": section(constraint_changes),
        "calendars_changed": section(calendar_changes),
        "renamed": section(renamed),
        "progress": {
            "started": section(started),
            "finished": section(finished),
            "actual_dates_rewritten": section(actuals_changed),
            "status_went_back": section(reverted),
        },
        "method": "Activities match by Activity ID and ties by predecessor and successor, as Tools > Schedule Comparison matches them (one shared core): between one pair a lag or type change is a changed tie. Moves are calendar days between the dates each file shows (actual, else scheduled). Lists are ranked by size of the change, biggest first.",
    }))
}

// ---------------------------------------------------------------- look-ahead

/// The work in progress or due to start or finish in the next `weeks`
/// weeks from the data date (or `from`), by WBS, each with whether it is
/// ready to work or waiting, and on what.
pub fn lookahead(p: &ProjectData, weeks: u32, from: Option<&str>) -> Result<Value> {
    if !(1..=26).contains(&weeks) {
        return Err(invalid("weeks is 1 to 26"));
    }
    let m = &p.model;
    let start = match from {
        Some(t) => parse_relative(m.origin_serial, t)
            .map_err(|e| invalid(format!("from: {e}")))?
            .ok_or_else(|| invalid("from is empty"))?,
        None => m
            .project
            .data_date
            .ok_or_else(|| invalid("The project has no data date; give from"))?,
    };
    let end = start + weeks as i64 * 7 * DAY;
    let lv = leveled(p);
    let resources = assigned_resources(p);
    let mut groups: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    let (mut ready, mut waiting, mut going) = (0, 0, 0);
    for (i, a) in m.activities.iter().enumerate() {
        if !is_open_work(a) {
            continue;
        }
        let (Some(s), Some(f)) = (displayed_start(a, lv), displayed_finish(a, lv)) else {
            continue;
        };
        if s >= end || f < start {
            continue;
        }
        let mut row = activity_row(p, i);
        let blockers: Vec<Value> = m
            .links
            .iter()
            .filter(|l| l.succ == i)
            .filter(|l| {
                let pr = &m.activities[l.pred];
                match l.link_type {
                    LinkType::SS | LinkType::SF => pr.act_start.is_none(),
                    _ => !pr.is_completed(),
                }
            })
            .map(|l| {
                let pr = &m.activities[l.pred];
                json!({"activity": pr.task_code, "name": name(p, pr), "status": status_label(pr.status),
                       "type": link_label(l.link_type), "expected_finish": date(p, displayed_finish(pr, lv))})
            })
            .collect();
        let state = if a.act_start.is_some() {
            going += 1;
            "in progress"
        } else if blockers.is_empty() {
            ready += 1;
            "ready: nothing before it is open"
        } else {
            waiting += 1;
            "waiting on predecessors"
        };
        row["state"] = json!(state);
        row["waiting_on"] = json!(blockers);
        row["resources"] = json!(resources.get(&a.task_id).cloned().unwrap_or_default());
        groups.entry(wbs_of(p, a)).or_default().push(row);
    }
    let total: usize = groups.values().map(Vec::len).sum();
    let mut shown = 0;
    let areas: Vec<Value> = groups
        .into_iter()
        .map(|(wbs, mut rows)| {
            let id = rows
                .first()
                .and_then(|r| r["activity"].as_str())
                .and_then(|code| p.model.activities.iter().find(|a| a.task_code == code))
                .and_then(|a| p.details.get(&a.task_id))
                .map_or(0, |d| d.wbs);
            let wbs_name = wbs_name(p, id);
            let count = rows.len();
            let room = 300usize.saturating_sub(shown);
            rows.truncate(room);
            shown += rows.len();
            json!({"wbs": wbs, "wbs_name": wbs_name, "count": count, "activities": rows})
        })
        .collect();
    Ok(json!({
        "basis": basis(p),
        "window": {"from": date(p, Some(start)), "to": date(p, Some(end)), "weeks": weeks},
        "summary": {"activities": total, "in_progress": going, "ready": ready, "waiting": waiting, "listed": shown},
        "areas": areas,
        "rule": "An activity is in the window when its start is before the window's end and its finish is not before its start. Ready means every predecessor it waits on is done (started, for start ties). Waiting lists the open predecessors with their expected finish.",
    }))
}

fn assigned_resources(p: &ProjectData) -> HashMap<i64, Vec<String>> {
    let mut out: HashMap<i64, Vec<String>> = HashMap::new();
    let names: HashMap<i64, String> = p.document.table("RSRC").map_or_else(Default::default, |t| {
        t.rows
            .iter()
            .map(|r| {
                (
                    t.value(r, "rsrc_id").trim().parse().unwrap_or(0),
                    t.value(r, "rsrc_short_name").to_string(),
                )
            })
            .collect()
    });
    if let Some(t) = p.document.table("TASKRSRC") {
        for r in &t.rows {
            let task: i64 = t.value(r, "task_id").trim().parse().unwrap_or(0);
            let rsrc: i64 = t.value(r, "rsrc_id").trim().parse().unwrap_or(0);
            if let Some(n) = names.get(&rsrc) {
                out.entry(task).or_default().push(n.clone());
            }
        }
    }
    out
}

// ---------------------------------------------------------------- milestones

/// Every milestone with its date, float, constraint, baseline date and what
/// drives it.
pub fn milestones(p: &ProjectData, ctx: &crate::baselines::EvContext) -> Result<Value> {
    let m = &p.model;
    let lv = leveled(p);
    let (finishes, baseline) = baseline_finishes(p, ctx);
    // Driving ties as the Relationships tab shows them: the engine's own
    // relationship float on the stored dates.
    let mut copy = m.clone();
    let scheduler = openxer_engine::scheduler::Scheduler::new(&mut copy);
    let driving: Vec<Option<bool>> = (0..m.links.len())
        .map(|r| scheduler.relationship_driving(r))
        .collect();
    drop(scheduler);
    let mut rows = vec![];
    for (i, a) in m.activities.iter().enumerate() {
        if !matches!(a.task_type, TaskType::Mile | TaskType::FinMile) {
            continue;
        }
        let mut row = activity_row(p, i);
        let when = if a.task_type == TaskType::Mile {
            displayed_start(a, lv)
        } else {
            displayed_finish(a, lv)
        };
        row["date"] = date(p, when);
        if let Some(f) = &finishes {
            let bl = f[i];
            row["baseline_date"] = date(p, bl);
            row["moved_days_since_baseline"] =
                json!(bl.zip(when).map(|(b, w)| calendar_days(w - b)));
        }
        let preds: Vec<usize> = (0..m.links.len())
            .filter(|&r| m.links[r].succ == i)
            .collect();
        row["driven_by"] = json!(preds
            .iter()
            .filter(|&&r| driving[r] == Some(true))
            .map(|&r| {
                let l = &m.links[r];
                let pr = &m.activities[l.pred];
                json!({"activity": pr.task_code, "name": name(p, pr), "type": link_label(l.link_type), "lag_hours": hours(l.lag), "finish": date(p, displayed_finish(pr, lv))})
            })
            .collect::<Vec<_>>());
        row["predecessor_count"] = json!(preds.len());
        rows.push((when.unwrap_or(i64::MAX), row));
    }
    rows.sort_by_key(|(w, _)| *w);
    Ok(json!({
        "basis": basis(p),
        "baseline": if finishes.is_some() { json!(baseline) } else { Value::Null },
        "count": rows.len(),
        "milestones": rows.into_iter().map(|(_, r)| r).take(200).collect::<Vec<_>>(),
        "rule": "driven_by lists the driving ties into the milestone (no relationship float: the predecessor that sets the date), as the Relationships tab shows them. Empty with predecessors means the date is held by a constraint, the data date or a tie with spare time; empty without predecessors means nothing ties it in. Moves are calendar days.",
    }))
}

// ---------------------------------------------------------------- statistics

fn percentile(sorted: &[f64], q: f64) -> Option<f64> {
    if sorted.is_empty() {
        return None;
    }
    let k = ((sorted.len() - 1) as f64 * q).round() as usize;
    Some(round(sorted[k]))
}

/// Counts and spreads the duration, calendar, activity and WBS reviews
/// read: activity types, calendars in use, durations in working days, the
/// longest activities, zero-duration tasks, repeated and short names,
/// activities with no resource, and the WBS depth.
pub fn schedule_stats(p: &ProjectData, update_period_days: Option<f64>) -> Result<Value> {
    if update_period_days.is_some_and(|d| !(d.is_finite() && d > 0.)) {
        return Err(invalid("update_period_days must be more than 0"));
    }
    let m = &p.model;
    let mut by_type: BTreeMap<&str, usize> = BTreeMap::new();
    let mut by_status: BTreeMap<&str, usize> = BTreeMap::new();
    for a in &m.activities {
        *by_type.entry(type_label(a.task_type)).or_default() += 1;
        *by_status.entry(status_label(a.status)).or_default() += 1;
    }
    // Calendars in use.
    let mut use_count: BTreeMap<usize, usize> = BTreeMap::new();
    for a in &m.activities {
        *use_count.entry(a.cal).or_default() += 1;
    }
    let calendars: Vec<Value> = m
        .calendars
        .iter()
        .enumerate()
        .map(|(c, cal)| {
            let week: Vec<f64> = cal
                .cal
                .template
                .iter()
                .map(|s| hours(s.iter().map(|(a, b)| b - a).sum()))
                .collect();
            let nonwork_dates = cal.cal.exc.values().filter(|s| s.is_empty()).count();
            let changed_dates = cal.cal.exc.values().filter(|s| !s.is_empty()).count();
            let (first, last) = (cal.cal.exc.keys().next(), cal.cal.exc.keys().next_back());
            json!({
                "calendar": calendar(p, c),
                "activities": use_count.get(&c).copied().unwrap_or(0),
                "continuous": cal.cal.continuous,
                "hours_sunday_to_saturday": week,
                "working_days_per_week": week.iter().filter(|h| **h > 0.).count(),
                "hours_per_week": round(week.iter().sum()),
                "hours_per_working_day": hours(day_minutes(p, c)),
                "nonwork_exception_dates": nonwork_dates,
                "changed_hours_exception_dates": changed_dates,
                "exceptions_from": first.map(|d| date_string(*d, Some(0))),
                "exceptions_to": last.map(|d| date_string(*d, Some(0))),
            })
        })
        .collect();
    // Durations of open work that has one.
    let work: Vec<usize> = (0..m.activities.len())
        .filter(|&i| {
            let a = &m.activities[i];
            matches!(a.task_type, TaskType::Task | TaskType::Resource)
        })
        .collect();
    let original = |i: usize| {
        p.details
            .get(&m.activities[i].task_id)
            .map_or(0, |d| d.original_duration)
    };
    let mut days: Vec<(f64, usize)> = work
        .iter()
        .map(|&i| (days_on(p, m.activities[i].cal, original(i)), i))
        .collect();
    days.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let values: Vec<f64> = days.iter().map(|d| d.0).collect();
    let buckets = [
        ("0 days", 0., 0.),
        ("under 1 day", 0.000001, 0.999999),
        ("1 to 5 days", 1., 5.),
        ("over 5 to 10 days", 5.000001, 10.),
        ("over 10 to 20 days", 10.000001, 20.),
        ("over 20 to 44 days", 20.000001, 44.),
        ("over 44 days", 44.000001, f64::MAX),
    ];
    let spread: Vec<Value> = buckets
        .iter()
        .map(|(label, lo, hi)| json!({"range": label, "count": values.iter().filter(|v| **v >= *lo && **v <= *hi).count()}))
        .collect();
    let longest: Vec<Value> = days
        .iter()
        .rev()
        .take(15)
        .map(|(_, i)| activity_row(p, *i))
        .collect();
    let zero: Vec<Value> = days
        .iter()
        .filter(|(d, _)| *d == 0.)
        .map(|(_, i)| activity_row(p, *i))
        .collect();
    let over_period = update_period_days.map(|period| {
        let list: Vec<Value> = days
            .iter()
            .rev()
            .filter(|(d, i)| *d > 2. * period && !m.activities[*i].is_completed())
            .map(|(_, i)| activity_row(p, *i))
            .collect();
        json!({"update_period_days": period, "rule": "open activities longer than two update periods", "found": capped(list)})
    });
    // Names.
    let mut names: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, a) in m.activities.iter().enumerate() {
        names
            .entry(name(p, a).trim().to_lowercase())
            .or_default()
            .push(i);
    }
    let mut repeated: Vec<Value> = names
        .iter()
        .filter(|(n, v)| v.len() > 1 && !n.is_empty())
        .map(|(_, v)| json!({"name": name(p, &m.activities[v[0]]), "count": v.len(), "activities": v.iter().take(8).map(|&i| m.activities[i].task_code.clone()).collect::<Vec<_>>()}))
        .collect();
    repeated.sort_by(|a, b| b["count"].as_u64().cmp(&a["count"].as_u64()));
    let short: Vec<Value> = (0..m.activities.len())
        .filter(|&i| name(p, &m.activities[i]).split_whitespace().count() < 2)
        .map(|i| activity_row(p, i))
        .collect();
    let resources = assigned_resources(p);
    let expenses = with_expenses(p);
    let unresourced: Vec<Value> = work
        .iter()
        .filter(|&&i| {
            let a = &m.activities[i];
            !a.is_completed()
                && !resources.contains_key(&a.task_id)
                && !expenses.contains(&a.task_id)
        })
        .map(|&i| activity_row(p, i))
        .collect();
    // WBS depth.
    let mut depth: BTreeMap<usize, usize> = BTreeMap::new();
    let mut per_element: HashMap<i64, usize> = HashMap::new();
    for a in &m.activities {
        let w = p.details.get(&a.task_id).map_or(0, |d| d.wbs);
        *per_element.entry(w).or_default() += 1;
        *depth.entry(wbs_chain(p, w).len()).or_default() += 1;
    }
    let project_node = openxer_xer::edit::wbs::project_node(p);
    let empty: Vec<Value> = p
        .wbs
        .iter()
        .filter(|w| Some(w.id) != project_node)
        .filter(|w| {
            !per_element.contains_key(&w.id) && !p.wbs.iter().any(|c| c.parent == Some(w.id))
        })
        .map(|w| json!({"wbs": wbs_code_path(p, w.id), "name": w.name}))
        .collect();
    let biggest = per_element
        .iter()
        .filter(|(w, _)| **w != 0)
        .max_by_key(|(_, n)| **n)
        .map(|(w, n)| json!({"wbs": wbs_code_path(p, *w), "activities": n}));
    let lags = m.links.iter().filter(|l| l.lag != 0).count();
    Ok(json!({
        "basis": basis(p),
        "activities": m.activities.len(),
        "relationships": m.links.len(),
        "relationships_with_lag": lags,
        "by_type": by_type,
        "by_status": by_status,
        "calendars": calendars,
        "durations": {
            "of": "Task and Resource Dependent activities, original duration in working days on each one's own calendar",
            "count": values.len(),
            "shortest": values.first(),
            "median": percentile(&values, 0.5),
            "ninetieth_percentile": percentile(&values, 0.9),
            "longest": values.last(),
            "spread": spread,
            "longest_activities": longest,
            "zero_duration_tasks": capped(zero),
            "longer_than_two_update_periods": over_period,
        },
        "names": {
            "repeated": repeated.into_iter().take(LIST_CAP).collect::<Vec<_>>(),
            "one_word": capped(short),
        },
        "open_work_without_resources_or_expenses": capped(unresourced),
        "wbs": {
            "elements": p.wbs.len(),
            "activities_by_level": depth.into_iter().map(|(l, n)| json!({"level": l, "activities": n})).collect::<Vec<_>>(),
            "elements_with_no_activities": capped(empty),
            "element_with_most_activities": biggest,
        },
    }))
}

// ---------------------------------------------------------------- resources

/// Each resource's remaining (and actual) units per period, its peak, its
/// limit and the periods over it.
pub fn resource_loading(p: &ProjectData, interval: Option<&str>) -> Result<Value> {
    let interval = interval.unwrap_or("week");
    let resources: BTreeSet<String> = assigned_resources(p).into_values().flatten().collect();
    let ids: HashMap<String, i64> = p.document.table("RSRC").map_or_else(Default::default, |t| {
        t.rows
            .iter()
            .map(|r| {
                (
                    t.value(r, "rsrc_short_name").to_string(),
                    t.value(r, "rsrc_id").trim().parse().unwrap_or(0),
                )
            })
            .collect()
    });
    let origin = p.model.origin_serial;
    let mut out = vec![];
    for short in &resources {
        // The units: the usage spread (shared periods and curves).
        let spread = crate::usage::usage(
            p,
            &crate::usage::Request {
                activity: None,
                resource: Some(short),
                interval: Some(interval),
                from: None,
                to: None,
                costs: false,
            },
        )?;
        let periods: Vec<(String, i64, i64)> = spread["periods"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| {
                let s = parse_relative(origin, v["start"].as_str()?).ok()??;
                let f = parse_relative(origin, v["finish"].as_str()?).ok()??;
                Some((v["start"].as_str()?.to_string(), s, f))
            })
            .collect();
        let series = |key: &str| -> Vec<f64> {
            spread["totals"][key]["per_period"]
                .as_array()
                .map(|a| a.iter().map(|v| v.as_f64().unwrap_or(0.)).collect())
                .unwrap_or_default()
        };
        let remaining = series("remaining");
        let actual = series("actual");
        // The limit: the line the Resource Usage Profile draws, from the
        // same function (rate rows times working hours on the resource's
        // own calendar).
        let rsrc = ids.get(short).copied();
        let bounds: Vec<(i64, i64)> = periods.iter().map(|(_, s, f)| (*s, *f)).collect();
        let limits = rsrc.and_then(|id| openxer_xer::limits::per_period(p, id, &bounds));
        let max_per_hour = rsrc.and_then(|id| {
            let rates = openxer_xer::limits::rates(p, id);
            let at = p.model.project.data_date.unwrap_or(0) + origin * DAY;
            openxer_xer::limits::rate_at(&rates, at).map(|r| r.0)
        });
        let calendar_name = rsrc
            .map(|id| calendar(p, openxer_xer::limits::resource_calendar(p, id)))
            .unwrap_or_default();
        let mut rows = vec![];
        let mut over = vec![];
        let mut peak: Option<(f64, String)> = None;
        for (k, (label, _, _)) in periods.iter().enumerate() {
            let units =
                remaining.get(k).copied().unwrap_or(0.) + actual.get(k).copied().unwrap_or(0.);
            let limit = limits.as_ref().and_then(|l| l.get(k).copied());
            if peak.as_ref().is_none_or(|(u, _)| units > *u) {
                peak = Some((units, label.clone()));
            }
            if let Some(l) = limit {
                if units > l + 1e-9 {
                    over.push(json!({"period": label, "units": round(units), "limit": round(l), "over_by": round(units - l)}));
                }
            }
            rows.push(json!({"period": label, "units": round(units), "limit": limit.map(round)}));
        }
        out.push(json!({
            "resource": short,
            "calendar": calendar_name,
            "limit_units_per_hour": if limits.is_some() { json!(max_per_hour) } else { Value::Null },
            "has_limit": limits.is_some(),
            "peak": peak.map(|(u, l)| json!({"period": l, "units": round(u)})),
            "total_remaining": spread["totals"]["remaining"]["total"],
            "total_actual": spread["totals"]["actual"]["total"],
            "periods_over_limit": over.len(),
            "over_limit": over.into_iter().take(LIST_CAP).collect::<Vec<_>>(),
            "per_period": rows,
        }));
    }
    Ok(json!({
        "basis": basis(p),
        "interval": interval,
        "resources": out,
        "rule": "Units per period are actual plus remaining as the usage views spread them. The limit per period is the line the Resource Usage Profile draws: the resource's max units per hour in effect at the period's start times the period's working hours on the resource's own calendar (the project's default calendar when it has none). A resource with no limit row has no limit (has_limit false). Leveling is not run: schedule a copy with level_resources to see what it would move.",
    }))
}

// ---------------------------------------------------------------- WBS summary

/// The schedule rolled up by WBS element down to `level`: dates, float,
/// counts and baseline finish, for a summary schedule and for checking that
/// the levels agree.
pub fn wbs_summary(
    p: &ProjectData,
    ctx: &crate::baselines::EvContext,
    level: usize,
) -> Result<Value> {
    if !(1..=10).contains(&level) {
        return Err(invalid("level is 1 to 10"));
    }
    let m = &p.model;
    let lv = leveled(p);
    let (finishes, baseline) = baseline_finishes(p, ctx);
    #[derive(Default)]
    struct Roll {
        count: usize,
        completed: usize,
        start: Option<i64>,
        finish: Option<i64>,
        float: Option<i64>,
        baseline: Option<i64>,
        milestones: Vec<Value>,
    }
    // Keyed by the chain of elements from the top, so the rows come out in
    // outline order.
    let mut rolls: BTreeMap<Vec<(i64, i64)>, Roll> = BTreeMap::new();
    let seq = |id: i64| p.wbs.iter().find(|w| w.id == id).map_or(0, |w| w.sequence);
    for (i, a) in m.activities.iter().enumerate() {
        let parts: Vec<(i64, i64)> = wbs_chain(p, p.details.get(&a.task_id).map_or(0, |d| d.wbs))
            .into_iter()
            .map(|id| (seq(id), id))
            .collect();
        for depth in 1..=parts.len().min(level) {
            let key = parts[..depth].to_vec();
            let r = rolls.entry(key).or_default();
            r.count += 1;
            if a.is_completed() {
                r.completed += 1;
            }
            if let Some(s) = displayed_start(a, lv) {
                r.start = Some(r.start.map_or(s, |x| x.min(s)));
            }
            if let Some(f) = displayed_finish(a, lv) {
                r.finish = Some(r.finish.map_or(f, |x| x.max(f)));
            }
            if is_open_work(a) {
                if let Some(tf) = a.total_float {
                    r.float = Some(r.float.map_or(tf, |x| x.min(tf)));
                }
            }
            if let Some(b) = finishes.as_ref().and_then(|f| f[i]) {
                r.baseline = Some(r.baseline.map_or(b, |x| x.max(b)));
            }
            if depth == parts.len().min(level)
                && matches!(a.task_type, TaskType::Mile | TaskType::FinMile)
                && r.milestones.len() < 10
            {
                r.milestones.push(json!({"activity": a.task_code, "name": name(p, a), "date": date(p, displayed_finish(a, lv).or(displayed_start(a, lv)))}));
            }
        }
    }
    let rows: Vec<Value> = rolls
        .into_iter()
        .map(|(chain, r)| {
            let id = chain.last().map_or(0, |(_, id)| *id);
            json!({
                "wbs": wbs_code_path(p, id), "level": chain.len(), "name": wbs_name(p, id),
                "activities": r.count, "completed": r.completed,
                "start": date(p, r.start), "finish": date(p, r.finish),
                "lowest_total_float_hours": r.float.map(hours),
                "baseline_finish": date(p, r.baseline),
                "finish_moved_days_since_baseline": r.baseline.zip(r.finish).map(|(b, f)| calendar_days(f - b)),
                "milestones": r.milestones,
            })
        })
        .collect();
    Ok(json!({
        "basis": basis(p),
        "baseline": if finishes.is_some() { json!(baseline) } else { Value::Null },
        "level": level,
        "elements": rows,
        "rule": "Each element rolls up every activity below it: earliest start, latest finish, lowest total float of open work, latest baseline finish. A parent's dates always span its children's; an element whose dates do not is a sign of activities filed on the wrong element.",
    }))
}
