//! Pure schedule diagnostics: the out-of-sequence rule, the schedule-log
//! warnings, and the Check Schedule metrics. Nothing here touches the UI or
//! changes the model. The desktop's Check Schedule dialog and the
//! `check_schedule` automation command both run these, with the inputs
//! [`expense_tasks`] and [`current_finishes`] give.
//!
//! Evidence: the out-of-sequence rule, missing-logic lists, constraint list,
//! atypical milestones and finish-milestone calendar pairs reproduce the saved
//! reference schedule log for project 369 exactly (see the fixture test below).
use openxer_engine::model::{CstrType, LinkType, Model, StatusType, TaskType};
use std::collections::BTreeSet;

/// Whether a relationship is out of sequence: the successor already has
/// progress that the relationship says cannot have happened yet. Lag and
/// date order are ignored; only the presence of actual dates counts.
pub fn link_out_of_sequence(model: &Model, link: usize) -> bool {
    let Some(link) = model.links.get(link) else {
        return false;
    };
    let pred = &model.activities[link.pred];
    let succ = &model.activities[link.succ];
    if succ.act_start.is_none() {
        return false;
    }
    match link.link_type {
        LinkType::FS => pred.act_finish.is_none(),
        LinkType::SS => pred.act_start.is_none(),
        LinkType::FF => succ.act_finish.is_some() && pred.act_finish.is_none(),
        LinkType::SF => succ.act_finish.is_some() && pred.act_start.is_none(),
        LinkType::None => false,
    }
}

/// Indices of all out-of-sequence relationships, in relationship order.
pub fn out_of_sequence_links(model: &Model) -> Vec<usize> {
    (0..model.links.len())
        .filter(|&i| link_out_of_sequence(model, i))
        .collect()
}

/// Activities with at least one out-of-sequence predecessor relationship.
pub fn out_of_sequence_activities(model: &Model) -> Vec<usize> {
    let set: BTreeSet<usize> = out_of_sequence_links(model)
        .into_iter()
        .map(|i| model.links[i].succ)
        .collect();
    set.into_iter().collect()
}

pub fn without_predecessors(model: &Model) -> Vec<usize> {
    (0..model.activities.len())
        .filter(|&i| model.activities[i].preds.is_empty())
        .collect()
}

pub fn without_successors(model: &Model) -> Vec<usize> {
    (0..model.activities.len())
        .filter(|&i| model.activities[i].succs.is_empty())
        .collect()
}

pub fn constrained(model: &Model) -> Vec<usize> {
    (0..model.activities.len())
        .filter(|&i| {
            let a = &model.activities[i];
            a.cstr != CstrType::None || a.cstr2 != CstrType::None
        })
        .collect()
}

/// Actual start or finish later than the data date.
pub fn actuals_after_data_date(model: &Model) -> Vec<usize> {
    let Some(data_date) = model.project.data_date else {
        return vec![];
    };
    (0..model.activities.len())
        .filter(|&i| {
            let a = &model.activities[i];
            a.act_start.is_some_and(|d| d > data_date)
                || a.act_finish.is_some_and(|d| d > data_date)
        })
        .collect()
}

/// Milestones driven at an endpoint they do not have: a finish milestone
/// with a start-to-start predecessor, or a start milestone with a
/// finish-to-finish predecessor.
pub fn atypical_milestones(model: &Model) -> Vec<usize> {
    (0..model.activities.len())
        .filter(|&i| {
            let a = &model.activities[i];
            let bad = match a.task_type {
                TaskType::FinMile => LinkType::SS,
                TaskType::Mile => LinkType::FF,
                _ => return false,
            };
            a.preds.iter().any(|&l| model.links[l].link_type == bad)
        })
        .collect()
}

/// Relationships into a finish milestone from a predecessor on another
/// calendar, as (relationship index) in relationship order.
pub fn finish_milestone_calendar_mismatches(model: &Model) -> Vec<usize> {
    (0..model.links.len())
        .filter(|&i| {
            let l = &model.links[i];
            let succ = &model.activities[l.succ];
            succ.task_type == TaskType::FinMile && model.activities[l.pred].cal != succ.cal
        })
        .collect()
}

/// Critical activities by the project's definition: total float at or below
/// the threshold (minutes), or on the longest path.
pub fn critical_activities(model: &Model, longest_path: bool, threshold: i64) -> Vec<usize> {
    (0..model.activities.len())
        .filter(|&i| {
            let a = &model.activities[i];
            if a.is_completed() {
                return false;
            }
            if longest_path {
                a.driving_path
            } else {
                a.total_float.is_some_and(|f| f <= threshold)
            }
        })
        .collect()
}

// ---------------------------------------------------------------- Check Schedule

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Check {
    Logic,
    NegativeLags,
    PositiveLags,
    LongLags,
    RelationshipTypes,
    HardConstraints,
    SoftConstraints,
    LargeFloat,
    NegativeFloat,
    LargeDurations,
    InvalidDates,
    ResourcesCost,
    LateActivities,
    Bei,
    OutOfSequence,
}

pub const CHECKS: [Check; 15] = [
    Check::Logic,
    Check::NegativeLags,
    Check::PositiveLags,
    Check::LongLags,
    Check::RelationshipTypes,
    Check::HardConstraints,
    Check::SoftConstraints,
    Check::LargeFloat,
    Check::NegativeFloat,
    Check::LargeDurations,
    Check::InvalidDates,
    Check::ResourcesCost,
    Check::LateActivities,
    Check::Bei,
    Check::OutOfSequence,
];

impl Check {
    pub fn caption(self) -> &'static str {
        match self {
            Self::Logic => "Logic - Activities missing predecessors or successors.",
            Self::NegativeLags => {
                "Negative lags - Relationships with a lag duration of less than 0"
            }
            Self::PositiveLags => "Positive Lags - Relationships with a positive lag duration",
            Self::LongLags => "Long Lags - Relationships with a lag duration greater than",
            Self::RelationshipTypes => {
                "Relationship Types - The majority of the relationships should be Finish to Start"
            }
            Self::HardConstraints => {
                "Hard Constraints - Constraints that prevent activities from being moved"
            }
            Self::SoftConstraints => {
                "Soft Constraints - Constraints that do not prevent activities from being moved"
            }
            Self::LargeFloat => "Large Float - Activities with total float greater than",
            Self::NegativeFloat => "Negative Float - Activities with a total float less than 0",
            Self::LargeDurations => {
                "Large Durations - Activities that have a remaining duration greater than"
            }
            Self::InvalidDates => "Activities with Invalid progress dates",
            Self::ResourcesCost => {
                "Resources / Cost - Activities that do not have an expense or resource assigned"
            }
            Self::LateActivities => {
                "Late Activities - Activities scheduled to finish later than the project baseline"
            }
            Self::Bei => "BEI - Baseline Execution Index",
            Self::OutOfSequence => {
                "Out Of Sequence - Activities which are completed or in-progress before predecessor"
            }
        }
    }
    /// Short name for the results list.
    pub fn name(self) -> &'static str {
        self.caption().split(" - ").next().unwrap_or("")
    }
    /// Checks with an hour parameter beside the caption.
    pub fn has_hours(self) -> bool {
        matches!(
            self,
            Self::LongLags | Self::LargeFloat | Self::LargeDurations
        )
    }
    /// The target comparison: `true` means the result must exceed the target.
    pub fn target_is_minimum(self) -> bool {
        matches!(self, Self::RelationshipTypes | Self::Bei)
    }
    /// Whether the check compares against the project baseline.
    pub fn needs_baseline(self) -> bool {
        matches!(self, Self::LateActivities | Self::Bei)
    }
    /// Why a check cannot run on this data, if it cannot. Every check runs;
    /// the baseline checks report a note when no baseline dates are given.
    pub fn unsupported(self) -> Option<&'static str> {
        None
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CheckParams {
    pub enabled: [bool; 15],
    /// Target per check; `None` leaves the target blank (no pass/fail).
    pub targets: [Option<f64>; 15],
    pub long_lag_hours: f64,
    pub large_float_hours: f64,
    pub large_duration_hours: f64,
}

impl Default for CheckParams {
    /// The defaults shown by the reference dialog.
    fn default() -> Self {
        let mut targets = [Some(5.); 15];
        for (check, value) in [
            (Check::NegativeLags, Some(1.)),
            (Check::LongLags, None),
            (Check::RelationshipTypes, Some(90.)),
            (Check::HardConstraints, Some(1.)),
            (Check::LargeFloat, Some(1.)),
            (Check::NegativeFloat, Some(1.)),
            (Check::InvalidDates, Some(1.)),
            (Check::ResourcesCost, Some(1.)),
            (Check::Bei, Some(0.95)),
            (Check::OutOfSequence, Some(2.)),
        ] {
            targets[check as usize] = value;
        }
        Self {
            enabled: [true; 15],
            targets,
            long_lag_hours: 352.,
            large_float_hours: 352.,
            large_duration_hours: 352.,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    Activity(usize),
    Relationship(usize),
}

#[derive(Clone, Debug, PartialEq)]
pub struct CheckResult {
    pub check: Check,
    pub items: Vec<Item>,
    /// Activities or relationships the percentage is taken over.
    pub population: usize,
    /// Percentage, or the index value for BEI. `None` when not computable.
    pub value: Option<f64>,
    pub target: Option<f64>,
    pub passed: Option<bool>,
    pub note: Option<&'static str>,
}

fn hard(c: CstrType) -> bool {
    matches!(
        c,
        CstrType::StartOn
            | CstrType::StartOnBefore
            | CstrType::FinishOn
            | CstrType::FinishOnBefore
            | CstrType::MandatoryStart
            | CstrType::MandatoryFinish
    )
}

fn soft(c: CstrType) -> bool {
    matches!(
        c,
        CstrType::StartOnAfter | CstrType::FinishOnAfter | CstrType::Alap
    )
}

/// Activities the activity-based checks look at: not completed, and not a
/// level of effort or WBS summary.
fn open_work(model: &Model) -> Vec<usize> {
    (0..model.activities.len())
        .filter(|&i| {
            let a = &model.activities[i];
            !a.is_completed() && !matches!(a.task_type, TaskType::Loe | TaskType::Wbs)
        })
        .collect()
}

/// Run one check. `with_expenses` lists task ids that carry an expense.
#[cfg(test)]
pub fn run_check(
    model: &Model,
    params: &CheckParams,
    check: Check,
    with_expenses: &BTreeSet<i64>,
) -> CheckResult {
    run_check_with(model, params, check, with_expenses, None)
}

/// Run one check. `baseline` holds each activity's project baseline finish
/// (the planned finish for an activity the baseline lacks); without it the
/// Late Activities and BEI checks report that a baseline is needed.
pub fn run_check_with(
    model: &Model,
    params: &CheckParams,
    check: Check,
    with_expenses: &BTreeSet<i64>,
    baseline: Option<&[Option<i64>]>,
) -> CheckResult {
    let open = open_work(model);
    let mut index = None;
    let hours = |h: f64| (h * 60.).round() as i64;
    let links = 0..model.links.len();
    let (items, population): (Vec<Item>, usize) = match check {
        Check::Logic => (
            open.iter()
                .copied()
                .filter(|&i| {
                    let a = &model.activities[i];
                    a.preds.is_empty() || a.succs.is_empty()
                })
                .map(Item::Activity)
                .collect(),
            open.len(),
        ),
        Check::NegativeLags => (
            links
                .clone()
                .filter(|&l| model.links[l].lag < 0)
                .map(Item::Relationship)
                .collect(),
            model.links.len(),
        ),
        Check::PositiveLags => (
            links
                .clone()
                .filter(|&l| model.links[l].lag > 0)
                .map(Item::Relationship)
                .collect(),
            model.links.len(),
        ),
        Check::LongLags => {
            let limit = hours(params.long_lag_hours);
            (
                links
                    .clone()
                    .filter(|&l| model.links[l].lag > limit)
                    .map(Item::Relationship)
                    .collect(),
                model.links.len(),
            )
        }
        Check::RelationshipTypes => (
            links
                .clone()
                .filter(|&l| model.links[l].link_type != LinkType::FS)
                .map(Item::Relationship)
                .collect(),
            model.links.len(),
        ),
        Check::HardConstraints | Check::SoftConstraints => {
            let test = if check == Check::HardConstraints {
                hard
            } else {
                soft
            };
            (
                open.iter()
                    .copied()
                    .filter(|&i| {
                        let a = &model.activities[i];
                        test(a.cstr) || test(a.cstr2)
                    })
                    .map(Item::Activity)
                    .collect(),
                open.len(),
            )
        }
        Check::LargeFloat => {
            let limit = hours(params.large_float_hours);
            (
                open.iter()
                    .copied()
                    .filter(|&i| model.activities[i].total_float.is_some_and(|f| f > limit))
                    .map(Item::Activity)
                    .collect(),
                open.len(),
            )
        }
        Check::NegativeFloat => (
            open.iter()
                .copied()
                .filter(|&i| model.activities[i].total_float.is_some_and(|f| f < 0))
                .map(Item::Activity)
                .collect(),
            open.len(),
        ),
        Check::LargeDurations => {
            let limit = hours(params.large_duration_hours);
            let tasks: Vec<usize> = open
                .iter()
                .copied()
                .filter(|&i| {
                    !matches!(
                        model.activities[i].task_type,
                        TaskType::Mile | TaskType::FinMile
                    )
                })
                .collect();
            (
                tasks
                    .iter()
                    .copied()
                    .filter(|&i| model.activities[i].rem > limit)
                    .map(Item::Activity)
                    .collect(),
                tasks.len(),
            )
        }
        Check::InvalidDates => {
            let data_date = model.project.data_date;
            (
                (0..model.activities.len())
                    .filter(|&i| invalid_dates(model, i, data_date))
                    .map(Item::Activity)
                    .collect(),
                model.activities.len(),
            )
        }
        Check::ResourcesCost => {
            let tasks: Vec<usize> = open
                .iter()
                .copied()
                .filter(|&i| {
                    !matches!(
                        model.activities[i].task_type,
                        TaskType::Mile | TaskType::FinMile
                    )
                })
                .collect();
            (
                tasks
                    .iter()
                    .copied()
                    .filter(|&i| {
                        let a = &model.activities[i];
                        a.assignments.is_empty() && !with_expenses.contains(&a.task_id)
                    })
                    .map(Item::Activity)
                    .collect(),
                tasks.len(),
            )
        }
        Check::OutOfSequence => {
            let started = (0..model.activities.len())
                .filter(|&i| model.activities[i].act_start.is_some())
                .count();
            (
                out_of_sequence_activities(model)
                    .into_iter()
                    .map(Item::Activity)
                    .collect(),
                started,
            )
        }
        Check::LateActivities => {
            // Remaining early finish after the baseline finish; summaries,
            // level of effort, milestones and completed work are left out.
            let considered: Vec<usize> = (0..model.activities.len())
                .filter(|&i| {
                    let a = &model.activities[i];
                    !a.is_completed()
                        && !matches!(
                            a.task_type,
                            TaskType::Loe | TaskType::Wbs | TaskType::Mile | TaskType::FinMile
                        )
                })
                .collect();
            let late = |i: usize| {
                let a = &model.activities[i];
                let finish = a.reend.or(a.early_finish);
                let bl = baseline.and_then(|b| b.get(i).copied().flatten());
                matches!((finish, bl), (Some(f), Some(b)) if f > b)
            };
            (
                considered
                    .iter()
                    .copied()
                    .filter(|&i| late(i))
                    .map(Item::Activity)
                    .collect(),
                considered.len(),
            )
        }
        Check::Bei => {
            // Completed activities over those the baseline has finishing by
            // the data date; the items are the due ones still open.
            let tasks: Vec<usize> = (0..model.activities.len())
                .filter(|&i| {
                    !matches!(model.activities[i].task_type, TaskType::Loe | TaskType::Wbs)
                })
                .collect();
            let data_date = model.project.data_date;
            let due: Vec<usize> = tasks
                .iter()
                .copied()
                .filter(|&i| {
                    let bl = baseline.and_then(|b| b.get(i).copied().flatten());
                    matches!((bl, data_date), (Some(b), Some(d)) if b <= d)
                })
                .collect();
            let completed = tasks
                .iter()
                .filter(|&&i| model.activities[i].act_finish.is_some())
                .count();
            index = (!due.is_empty()).then(|| completed as f64 / due.len() as f64);
            (
                due.iter()
                    .copied()
                    .filter(|&i| model.activities[i].act_finish.is_none())
                    .map(Item::Activity)
                    .collect(),
                due.len(),
            )
        }
    };
    let target = params.targets[check as usize];
    let missing =
        (check.needs_baseline() && baseline.is_none()).then_some("Requires a project baseline.");
    if let Some(note) = check.unsupported().or(missing) {
        return CheckResult {
            check,
            items,
            population,
            value: None,
            target,
            passed: None,
            note: Some(note),
        };
    }
    let value = if check == Check::Bei {
        index
    } else {
        (population > 0).then(|| {
            let count = if check == Check::RelationshipTypes {
                population - items.len()
            } else {
                items.len()
            };
            count as f64 * 100. / population as f64
        })
    };
    let passed = match (value, target) {
        (Some(v), Some(t)) => Some(if check.target_is_minimum() {
            v > t
        } else {
            v < t
        }),
        _ => None,
    };
    CheckResult {
        check,
        items,
        population,
        value,
        target,
        passed,
        note: None,
    }
}

/// Progress dates that cannot be right at the data date: actuals after it,
/// or remaining work forecast to start or finish before it.
fn invalid_dates(model: &Model, i: usize, data_date: Option<i64>) -> bool {
    let Some(data_date) = data_date else {
        return false;
    };
    let a = &model.activities[i];
    if a.act_start.is_some_and(|d| d > data_date) || a.act_finish.is_some_and(|d| d > data_date) {
        return true;
    }
    match a.status {
        StatusType::Completed => false,
        StatusType::InProgress => a.early_finish.is_some_and(|d| d < data_date),
        StatusType::NotStarted => a.early_start.is_some_and(|d| d < data_date),
    }
}

/// Run every enabled check.
#[cfg(test)]
pub fn run_checks(
    model: &Model,
    params: &CheckParams,
    with_expenses: &BTreeSet<i64>,
) -> Vec<CheckResult> {
    run_checks_with(model, params, with_expenses, None)
}

/// The task ids that carry an expense item (the Resources / Cost check
/// counts an activity with an expense as resourced).
pub fn expense_tasks(p: &crate::ProjectData) -> BTreeSet<i64> {
    p.document
        .table("PROJCOST")
        .map(|t| {
            t.rows
                .iter()
                .filter_map(|r| t.value(r, "task_id").parse().ok())
                .collect()
        })
        .unwrap_or_default()
}

/// Each activity's own planned finish: what the Late Activities and BEI
/// checks compare with when the project has no project baseline (the
/// current project stands in for it, as Assign Baselines' "Current
/// Project" does).
pub fn current_finishes(p: &crate::ProjectData) -> Vec<Option<i64>> {
    p.model
        .activities
        .iter()
        .map(|a| crate::ev::current(p, a).finish)
        .collect()
}

/// Run every enabled check with the project baseline finishes.
pub fn run_checks_with(
    model: &Model,
    params: &CheckParams,
    with_expenses: &BTreeSet<i64>,
    baseline: Option<&[Option<i64>]>,
) -> Vec<CheckResult> {
    CHECKS
        .iter()
        .filter(|c| params.enabled[**c as usize])
        .map(|&c| run_check_with(model, params, c, with_expenses, baseline))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use openxer_engine::model::{Activity, Link, ProjectSettings};
    use openxer_engine::options::ScheduleOptions;

    fn activity(id: i64) -> Activity {
        Activity {
            task_id: id,
            task_code: format!("A{id}"),
            rem: 480,
            ..Activity::default()
        }
    }

    fn started(mut a: Activity, start: i64) -> Activity {
        a.status = StatusType::InProgress;
        a.act_start = Some(start);
        a
    }

    fn finished(mut a: Activity, start: i64, finish: i64) -> Activity {
        a.status = StatusType::Completed;
        a.act_start = Some(start);
        a.act_finish = Some(finish);
        a.rem = 0;
        a
    }

    fn model(activities: Vec<Activity>, links: &[(usize, usize, LinkType, i64)]) -> Model {
        let mut m = Model {
            origin_serial: 0,
            project_calendar: 0,
            calendars: vec![],
            activities,
            links: links
                .iter()
                .map(|&(pred, succ, link_type, lag)| Link {
                    pred,
                    succ,
                    link_type,
                    lag,
                    driving_path: false,
                })
                .collect(),
            project: ProjectSettings {
                data_date: Some(10_000),
                ..ProjectSettings::default()
            },
            options: ScheduleOptions::default(),
        };
        m.rebuild_links().unwrap();
        m
    }

    #[test]
    fn out_of_sequence_follows_relationship_endpoints() {
        // pred not started, succ started: FS and SS are out of sequence.
        for (kind, expected) in [
            (LinkType::FS, true),
            (LinkType::SS, true),
            (LinkType::FF, false),
            (LinkType::SF, false),
        ] {
            let m = model(
                vec![activity(1), started(activity(2), 100)],
                &[(0, 1, kind, 0)],
            );
            assert_eq!(link_out_of_sequence(&m, 0), expected, "{kind:?}");
        }
        // pred started, succ started: only FS (needs the pred finish).
        for (kind, expected) in [(LinkType::FS, true), (LinkType::SS, false)] {
            let m = model(
                vec![started(activity(1), 50), started(activity(2), 100)],
                &[(0, 1, kind, 0)],
            );
            assert_eq!(link_out_of_sequence(&m, 0), expected, "{kind:?}");
        }
        // succ finished before pred finished: FF and FS; SF needs pred start.
        for (kind, pred_started, expected) in [
            (LinkType::FF, true, true),
            (LinkType::SF, true, false),
            (LinkType::SF, false, true),
        ] {
            let pred = if pred_started {
                started(activity(1), 50)
            } else {
                activity(1)
            };
            let m = model(
                vec![pred, finished(activity(2), 60, 200)],
                &[(0, 1, kind, 0)],
            );
            assert_eq!(link_out_of_sequence(&m, 0), expected, "{kind:?}");
        }
        // Completed predecessor and not-started successors are in sequence.
        let m = model(
            vec![
                finished(activity(1), 0, 50),
                started(activity(2), 10),
                activity(3),
            ],
            &[(0, 1, LinkType::FS, 0), (0, 2, LinkType::FS, 0)],
        );
        assert!(out_of_sequence_links(&m).is_empty());
        // An activity appears once even with several out-of-sequence links.
        let m = model(
            vec![activity(1), activity(2), started(activity(3), 5)],
            &[(0, 2, LinkType::FS, 0), (1, 2, LinkType::SS, 0)],
        );
        assert_eq!(out_of_sequence_links(&m), vec![0, 1]);
        assert_eq!(out_of_sequence_activities(&m), vec![2]);
    }

    #[test]
    fn log_warnings_cover_logic_constraints_milestones_and_actuals() {
        let mut mile = activity(4);
        mile.task_type = TaskType::FinMile;
        mile.cal = 1;
        let mut start_mile = activity(5);
        start_mile.task_type = TaskType::Mile;
        let mut constrained_one = activity(2);
        constrained_one.cstr2 = CstrType::FinishOnBefore;
        let late_actual = started(activity(3), 20_000);
        let m = model(
            vec![activity(1), constrained_one, late_actual, mile, start_mile],
            &[
                (0, 1, LinkType::FS, 0),
                (1, 3, LinkType::SS, 0),
                (0, 4, LinkType::FF, 0),
                (2, 3, LinkType::FS, 0),
            ],
        );
        assert_eq!(without_predecessors(&m), vec![0, 2]);
        assert_eq!(without_successors(&m), vec![3, 4]);
        assert_eq!(constrained(&m), vec![1]);
        assert_eq!(actuals_after_data_date(&m), vec![2]);
        assert_eq!(atypical_milestones(&m), vec![3, 4]);
        assert_eq!(finish_milestone_calendar_mismatches(&m), vec![1, 3]);
    }

    #[test]
    fn critical_uses_threshold_or_longest_path() {
        let mut a = activity(1);
        a.total_float = Some(600);
        let mut b = activity(2);
        b.total_float = Some(0);
        b.driving_path = true;
        let mut c = finished(activity(3), 0, 10);
        c.total_float = Some(0);
        let m = model(vec![a, b, c], &[]);
        assert_eq!(critical_activities(&m, false, 0), vec![1]);
        assert_eq!(critical_activities(&m, false, 600), vec![0, 1]);
        assert_eq!(critical_activities(&m, true, 0), vec![1]);
    }

    fn result(m: &Model, check: Check) -> CheckResult {
        run_check(m, &CheckParams::default(), check, &BTreeSet::new())
    }

    fn activities(r: &CheckResult) -> Vec<usize> {
        r.items
            .iter()
            .map(|i| match i {
                Item::Activity(a) => *a,
                Item::Relationship(l) => *l,
            })
            .collect()
    }

    #[test]
    fn logic_check_counts_open_work_only() {
        let mut loe = activity(4);
        loe.task_type = TaskType::Loe;
        let m = model(
            vec![
                activity(1),
                activity(2),
                activity(3),
                loe,
                finished(activity(5), 0, 10),
            ],
            &[(0, 1, LinkType::FS, 0), (1, 2, LinkType::FS, 0)],
        );
        let r = result(&m, Check::Logic);
        assert_eq!(activities(&r), vec![0, 2]);
        assert_eq!(r.population, 3);
        assert_eq!(r.passed, Some(false));
    }

    #[test]
    fn lag_and_type_checks_count_relationships() {
        let m = model(
            vec![activity(1), activity(2), activity(3), activity(4)],
            &[
                (0, 1, LinkType::FS, -60),
                (1, 2, LinkType::SS, 120),
                (2, 3, LinkType::FS, 400 * 60),
                (0, 3, LinkType::FF, 0),
            ],
        );
        assert_eq!(activities(&result(&m, Check::NegativeLags)), vec![0]);
        assert_eq!(activities(&result(&m, Check::PositiveLags)), vec![1, 2]);
        let long = result(&m, Check::LongLags);
        assert_eq!(activities(&long), vec![2]);
        assert_eq!(long.value, Some(25.));
        assert_eq!(long.passed, None, "Long Lags has no default target");
        let types = result(&m, Check::RelationshipTypes);
        assert_eq!(activities(&types), vec![1, 3]);
        assert_eq!(types.value, Some(50.));
        assert_eq!(types.passed, Some(false));
    }

    #[test]
    fn constraint_checks_split_hard_and_soft() {
        let kinds = [
            CstrType::StartOn,
            CstrType::FinishOnBefore,
            CstrType::MandatoryFinish,
            CstrType::StartOnAfter,
            CstrType::Alap,
            CstrType::None,
        ];
        let acts = kinds
            .iter()
            .enumerate()
            .map(|(i, &c)| {
                let mut a = activity(i as i64 + 1);
                a.cstr = c;
                a
            })
            .collect();
        let m = model(acts, &[]);
        assert_eq!(
            activities(&result(&m, Check::HardConstraints)),
            vec![0, 1, 2]
        );
        assert_eq!(activities(&result(&m, Check::SoftConstraints)), vec![3, 4]);
    }

    #[test]
    fn float_and_duration_checks_use_hour_parameters() {
        let mut a = activity(1);
        a.total_float = Some(353 * 60);
        let mut b = activity(2);
        b.total_float = Some(-1);
        b.rem = 353 * 60;
        let mut c = activity(3);
        c.total_float = Some(352 * 60);
        c.rem = 352 * 60;
        let mut d = activity(4);
        d.task_type = TaskType::FinMile;
        d.rem = 1000 * 60;
        let m = model(vec![a, b, c, d], &[]);
        assert_eq!(activities(&result(&m, Check::LargeFloat)), vec![0]);
        assert_eq!(activities(&result(&m, Check::NegativeFloat)), vec![1]);
        assert_eq!(activities(&result(&m, Check::LargeDurations)), vec![1]);
        let params = CheckParams {
            large_float_hours: 300.,
            ..CheckParams::default()
        };
        let r = run_check(&m, &params, Check::LargeFloat, &BTreeSet::new());
        assert_eq!(activities(&r), vec![0, 2]);
    }

    #[test]
    fn invalid_dates_flag_future_actuals_and_past_forecasts() {
        let future = finished(activity(1), 0, 20_000);
        let mut stale = activity(2);
        stale.early_start = Some(5_000);
        let mut fine = activity(3);
        fine.early_start = Some(10_000);
        let mut running = started(activity(4), 1_000);
        running.early_finish = Some(9_000);
        let m = model(vec![future, stale, fine, running], &[]);
        assert_eq!(activities(&result(&m, Check::InvalidDates)), vec![0, 1, 3]);
    }

    #[test]
    fn resource_check_accepts_assignments_or_expenses() {
        let mut assigned = activity(1);
        assigned
            .assignments
            .push(openxer_engine::model::Assignment::default());
        let mut mile = activity(4);
        mile.task_type = TaskType::Mile;
        let m = model(vec![assigned, activity(2), activity(3), mile], &[]);
        let expenses = BTreeSet::from([3]);
        let r = run_check(&m, &CheckParams::default(), Check::ResourcesCost, &expenses);
        assert_eq!(activities(&r), vec![1]);
        assert_eq!(r.population, 3);
    }

    #[test]
    fn out_of_sequence_check_and_baseline_checks() {
        let m = model(
            vec![
                activity(1),
                started(activity(2), 5),
                started(activity(3), 5),
            ],
            &[(0, 1, LinkType::FS, 0)],
        );
        let r = result(&m, Check::OutOfSequence);
        assert_eq!(activities(&r), vec![1]);
        assert_eq!(r.value, Some(50.));
        assert_eq!(r.passed, Some(false));
        for check in [Check::LateActivities, Check::Bei] {
            let r = result(&m, check);
            assert!(r.note.is_some() && r.value.is_none() && r.passed.is_none());
        }
        let mut params = CheckParams::default();
        params.enabled[Check::Logic as usize] = false;
        let all = run_checks(&m, &params, &BTreeSet::new());
        assert_eq!(all.len(), 14);
        assert!(all.iter().all(|r| r.check != Check::Logic));
    }

    #[test]
    fn late_activities_and_bei_compare_with_the_baseline_finish() {
        use openxer_engine::calendar::DAY;
        let mut m = model(
            vec![
                finished(activity(1), 0, 2 * DAY),
                activity(2),
                activity(3),
                activity(4),
            ],
            &[],
        );
        m.project.data_date = Some(5 * DAY);
        for (i, finish) in [(1, 8 * DAY), (2, 6 * DAY), (3, 9 * DAY)] {
            m.activities[i].early_finish = Some(finish);
            m.activities[i].reend = Some(finish);
        }
        // Baseline finishes: 1 due by the data date and done, 2 due and open,
        // 3 not due, 4 has no baseline activity.
        let bl = [Some(DAY), Some(4 * DAY), Some(7 * DAY), None];
        let params = CheckParams::default();
        let none = BTreeSet::new();
        let late = run_check_with(&m, &params, Check::LateActivities, &none, Some(&bl));
        assert_eq!(activities(&late), vec![1]);
        assert_eq!(late.population, 3);
        assert!(late.note.is_none());
        let bei = run_check_with(&m, &params, Check::Bei, &none, Some(&bl));
        assert_eq!(activities(&bei), vec![1]);
        assert_eq!(bei.population, 2);
        assert_eq!(bei.value, Some(0.5));
        assert_eq!(bei.passed, Some(false));
        let all = run_checks_with(&m, &params, &none, Some(&bl));
        assert!(all.iter().all(|r| r.note.is_none()));
    }

    /// The saved project 369 reference run: its schedule log lists 60
    /// out-of-sequence activities, 17 without predecessors, 17 without
    /// successors, 23 constrained, 2 atypical milestones and 312
    /// finish-milestone calendar pairs. Fixtures are local-only; the test is
    /// skipped when they are absent.
    #[test]
    fn reproduces_saved_reference_schedule_log_for_project_369() {
        let Some(path) = openxer_engine::fixture::fixture_path("p6-bench/369.txt") else {
            return;
        };
        let text = std::fs::read_to_string(path).unwrap();
        let dump = openxer_engine::fixture::parse_model(&text).unwrap();
        let m = dump.model;
        let codes = |v: Vec<usize>| -> Vec<String> {
            v.into_iter()
                .map(|i| m.activities[i].task_code.clone())
                .collect()
        };
        let oos = codes(out_of_sequence_activities(&m));
        assert_eq!(oos.len(), 60);
        for code in ["A13690", "A13700", "A13710", "A15360", "A15520"] {
            assert!(oos.iter().any(|c| c == code), "{code}");
        }
        let no_preds = codes(without_predecessors(&m));
        assert_eq!(no_preds.len(), 17);
        assert!(no_preds.iter().any(|c| c == "MIL.460"));
        let no_succs = codes(without_successors(&m));
        assert_eq!(no_succs.len(), 17);
        assert!(no_succs.iter().any(|c| c == "MIL.900"));
        assert_eq!(constrained(&m).len(), 23);
        assert_eq!(actuals_after_data_date(&m).len(), 0);
        assert_eq!(codes(atypical_milestones(&m)), vec!["MIL.600", "MIL.610"]);
        assert_eq!(finish_milestone_calendar_mismatches(&m).len(), 312);
    }
}
