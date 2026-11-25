//! View > Check Schedule filters: each shows the activities one schedule
//! check finds, with the thresholds of the Check Schedule report
//! (`schedule_check`). Relationship checks show the activities at either
//! end of the relationships they find.
use crate::schedule_check::{self as check, Check, CheckParams, Item};
use openxer_engine::model::{Model, StatusType};
use openxer_xer::ProjectData;
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CheckFilter {
    RelationshipType,
    Logic,
    NegativeLags,
    PositiveLags,
    LongLags,
    HardConstraints,
    SoftConstraints,
    LargeFloat,
    NegativeFloat,
    LargeDurations,
    InvalidBefore,
    InvalidAfter,
    NoResourcesNoCost,
    Late,
    MissingInBaseline,
    OutOfSequence,
}

impl CheckFilter {
    /// View > Check Schedule order.
    pub const ALL: [CheckFilter; 16] = [
        CheckFilter::RelationshipType,
        CheckFilter::Logic,
        CheckFilter::NegativeLags,
        CheckFilter::PositiveLags,
        CheckFilter::LongLags,
        CheckFilter::HardConstraints,
        CheckFilter::SoftConstraints,
        CheckFilter::LargeFloat,
        CheckFilter::NegativeFloat,
        CheckFilter::LargeDurations,
        CheckFilter::InvalidBefore,
        CheckFilter::InvalidAfter,
        CheckFilter::NoResourcesNoCost,
        CheckFilter::Late,
        CheckFilter::MissingInBaseline,
        CheckFilter::OutOfSequence,
    ];

    pub fn caption(self) -> &'static str {
        match self {
            CheckFilter::RelationshipType => "Relationship Type",
            CheckFilter::Logic => "Logic",
            CheckFilter::NegativeLags => "Negative Lags",
            CheckFilter::PositiveLags => "Positive Lags",
            CheckFilter::LongLags => "Long Lags",
            CheckFilter::HardConstraints => "Hard Constraints",
            CheckFilter::SoftConstraints => "Soft Constraints",
            CheckFilter::LargeFloat => "Large Float",
            CheckFilter::NegativeFloat => "Negative Float",
            CheckFilter::LargeDurations => "Large Durations",
            CheckFilter::InvalidBefore => "Invalid Progress Date Before Data Date",
            CheckFilter::InvalidAfter => "Invalid Progress Date After Data Date",
            CheckFilter::NoResourcesNoCost => "No Resources And No Cost",
            CheckFilter::Late => "Late Activities",
            CheckFilter::MissingInBaseline => "Activities Missing in Assigned Baseline",
            CheckFilter::OutOfSequence => "Out of Sequence Activities",
        }
    }

    /// Filter key stored in layouts (`check:large_float`).
    pub fn key(self) -> String {
        format!(
            "check:{}",
            self.caption().to_lowercase().replace([' ', '/'], "_")
        )
    }

    pub fn from_key(key: &str) -> Option<Self> {
        key.starts_with("check:")
            .then(|| Self::ALL.into_iter().find(|c| c.key() == key))
            .flatten()
    }

    /// The report check this filter reuses, if one matches it exactly.
    fn report_check(self) -> Option<Check> {
        Some(match self {
            CheckFilter::RelationshipType => Check::RelationshipTypes,
            CheckFilter::Logic => Check::Logic,
            CheckFilter::NegativeLags => Check::NegativeLags,
            CheckFilter::PositiveLags => Check::PositiveLags,
            CheckFilter::LongLags => Check::LongLags,
            CheckFilter::HardConstraints => Check::HardConstraints,
            CheckFilter::SoftConstraints => Check::SoftConstraints,
            CheckFilter::LargeFloat => Check::LargeFloat,
            CheckFilter::NegativeFloat => Check::NegativeFloat,
            CheckFilter::LargeDurations => Check::LargeDurations,
            CheckFilter::NoResourcesNoCost => Check::ResourcesCost,
            CheckFilter::Late => Check::LateActivities,
            CheckFilter::OutOfSequence => Check::OutOfSequence,
            CheckFilter::InvalidBefore
            | CheckFilter::InvalidAfter
            | CheckFilter::MissingInBaseline => return None,
        })
    }
}

/// Task ids that carry an expense (the Resources / Cost check counts them).
pub fn expense_tasks(p: &ProjectData) -> BTreeSet<i64> {
    check::expense_tasks(p)
}

/// Actual dates after the data date.
fn actual_after(model: &Model, i: usize, data_date: i64) -> bool {
    let a = &model.activities[i];
    a.act_start.is_some_and(|d| d > data_date) || a.act_finish.is_some_and(|d| d > data_date)
}

/// Remaining work forecast to start or finish before the data date.
fn forecast_before(model: &Model, i: usize, data_date: i64) -> bool {
    let a = &model.activities[i];
    match a.status {
        StatusType::Completed => false,
        StatusType::InProgress => a.early_finish.is_some_and(|d| d < data_date),
        StatusType::NotStarted => a.early_start.is_some_and(|d| d < data_date),
    }
}

/// The activities `filter` shows. `baselines` are the open project's
/// assigned baselines (Late Activities and the missing check use the
/// project baseline).
pub fn matching(
    p: &ProjectData,
    params: &CheckParams,
    baselines: Option<&crate::baselines::Compare>,
    filter: CheckFilter,
) -> BTreeSet<usize> {
    let model = &p.model;
    if let Some(c) = filter.report_check() {
        let finishes = baselines.map(|b| b.project_finishes(p));
        let expenses = if c == Check::ResourcesCost {
            expense_tasks(p)
        } else {
            BTreeSet::new()
        };
        let result = check::run_check_with(model, params, c, &expenses, finishes.as_deref());
        let mut out = BTreeSet::new();
        for item in result.items {
            match item {
                Item::Activity(i) => {
                    out.insert(i);
                }
                Item::Relationship(l) => {
                    if let Some(link) = model.links.get(l) {
                        out.insert(link.pred);
                        out.insert(link.succ);
                    }
                }
            }
        }
        return out;
    }
    let data_date = model.project.data_date;
    match filter {
        CheckFilter::InvalidBefore => (0..model.activities.len())
            .filter(|&i| data_date.is_some_and(|d| forecast_before(model, i, d)))
            .collect(),
        CheckFilter::InvalidAfter => (0..model.activities.len())
            .filter(|&i| data_date.is_some_and(|d| actual_after(model, i, d)))
            .collect(),
        CheckFilter::MissingInBaseline => {
            let Some(crate::baselines::data::Source::Baseline(b)) = baselines
                .map(|c| c.source(crate::baselines::data::Slot::Project))
                .cloned()
            else {
                return BTreeSet::new();
            };
            model
                .activities
                .iter()
                .enumerate()
                .filter(|(_, a)| !b.by_code.contains_key(&a.task_code))
                .map(|(i, _)| i)
                .collect()
        }
        _ => BTreeSet::new(),
    }
}
