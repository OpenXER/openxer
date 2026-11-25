//! Project Checker rules: business rule checks on the schedule logic and
//! progress, foreign key checks on the records that point at other records,
//! and unique key checks. Pure: reads a project, changes nothing.
use super::doc::{self, Cell, Column, Report, Row, Section, Table, Tone};
use crate::format;
use crate::schedule_check;
use openxer_engine::model::{CstrType, LinkType, StatusType, TaskType};
use openxer_xer::ProjectData;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Group {
    BusinessRule,
    ForeignKey,
    UniqueKey,
}

impl Group {
    pub const ALL: [Group; 3] = [Group::BusinessRule, Group::ForeignKey, Group::UniqueKey];
    pub fn caption(self) -> &'static str {
        match self {
            Group::BusinessRule => "Business Rule Checks",
            Group::ForeignKey => "Foreign Key Checks",
            Group::UniqueKey => "Unique Key Checks",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Rule {
    MissingPredecessor,
    MissingSuccessor,
    HardConstraint,
    SoftConstraint,
    NegativeLag,
    LongLag,
    StartToFinish,
    NonFinishToStart,
    LongDuration,
    ZeroDurationTask,
    NegativeFloat,
    LargeFloat,
    MissingCode,
    ActualAfterDataDate,
    ForecastBeforeDataDate,
    OutOfSequence,
    RelationshipMissingActivity,
    ActivityMissingWbs,
    ActivityMissingCalendar,
    AssignmentMissingActivity,
    AssignmentMissingResource,
    CodeAssignmentMissingValue,
    NotebookMissingTopic,
    DuplicateActivityId,
    DuplicateRelationship,
    DuplicateWbsCode,
    MilestoneWithAssignments,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

impl Rule {
    pub const ALL: [Rule; 27] = [
        Rule::MissingPredecessor,
        Rule::MissingSuccessor,
        Rule::HardConstraint,
        Rule::SoftConstraint,
        Rule::NegativeLag,
        Rule::LongLag,
        Rule::StartToFinish,
        Rule::NonFinishToStart,
        Rule::LongDuration,
        Rule::ZeroDurationTask,
        Rule::NegativeFloat,
        Rule::LargeFloat,
        Rule::MissingCode,
        Rule::ActualAfterDataDate,
        Rule::ForecastBeforeDataDate,
        Rule::OutOfSequence,
        Rule::RelationshipMissingActivity,
        Rule::ActivityMissingWbs,
        Rule::ActivityMissingCalendar,
        Rule::AssignmentMissingActivity,
        Rule::AssignmentMissingResource,
        Rule::CodeAssignmentMissingValue,
        Rule::NotebookMissingTopic,
        Rule::DuplicateActivityId,
        Rule::DuplicateRelationship,
        Rule::DuplicateWbsCode,
        Rule::MilestoneWithAssignments,
    ];
    pub fn group(self) -> Group {
        use Rule::*;
        match self {
            RelationshipMissingActivity
            | ActivityMissingWbs
            | ActivityMissingCalendar
            | AssignmentMissingActivity
            | AssignmentMissingResource
            | CodeAssignmentMissingValue
            | NotebookMissingTopic => Group::ForeignKey,
            DuplicateActivityId | DuplicateRelationship | DuplicateWbsCode => Group::UniqueKey,
            _ => Group::BusinessRule,
        }
    }
    pub fn caption(self) -> &'static str {
        use Rule::*;
        match self {
            MissingPredecessor => "Activities without predecessors",
            MissingSuccessor => "Activities without successors",
            HardConstraint => "Hard constraints",
            SoftConstraint => "Soft constraints",
            NegativeLag => "Negative lags (leads)",
            LongLag => "Lags longer than the limit",
            StartToFinish => "Start to Finish relationships",
            NonFinishToStart => "Start to Start and Finish to Finish relationships",
            LongDuration => "Remaining durations longer than the limit",
            ZeroDurationTask => "Task activities with no duration",
            NegativeFloat => "Negative total float",
            LargeFloat => "Total float larger than the limit",
            MissingCode => "Activities missing activity code values",
            ActualAfterDataDate => "Actual dates after the data date",
            ForecastBeforeDataDate => "Remaining work forecast before the data date",
            OutOfSequence => "Out-of-sequence progress",
            RelationshipMissingActivity => "Relationships to missing activities",
            ActivityMissingWbs => "Activities with a missing WBS",
            ActivityMissingCalendar => "Activities with a missing calendar",
            AssignmentMissingActivity => "Resource assignments to missing activities",
            AssignmentMissingResource => "Resource assignments with a missing resource",
            CodeAssignmentMissingValue => "Activity code assignments with a missing value",
            NotebookMissingTopic => "Notebooks with a missing topic",
            DuplicateActivityId => "Duplicate Activity IDs",
            DuplicateRelationship => "Duplicate relationships",
            DuplicateWbsCode => "Duplicate WBS codes under one parent",
            MilestoneWithAssignments => "Milestone with resource assignments",
        }
    }
    /// What the finding means and what to do, in one line for the report.
    pub fn advice(self) -> &'static str {
        use Rule::*;
        match self {
            MissingPredecessor => "Open starts: tie each activity to the work that has to happen first, so a slip upstream moves it.",
            MissingSuccessor => "Open finishes: tie each activity to the work that follows it, or to a finish milestone.",
            HardConstraint => "Hard constraints hold dates against the logic and can hide slips. Use them only for real contract dates.",
            SoftConstraint => "Soft constraints only push dates later. Check they are still needed.",
            NegativeLag => "Leads overlap work in a way crews cannot see in the field. Split the activity or use a start-to-start relationship.",
            LongLag => "Long lags often stand in for real work (cure, delivery, review). Make that work an activity.",
            StartToFinish => "Start to Finish relationships are rare and usually a mistake. Check the direction of the link.",
            NonFinishToStart => "Overlapping relationships are fine when crews really work in parallel. Check each one has a lag that matches the field sequence.",
            LongDuration => "Long activities are hard to status. Break them into pieces a foreman can report on.",
            ZeroDurationTask => "A task with no duration should be a milestone, or needs its duration.",
            NegativeFloat => "The schedule misses a constraint or deadline on these paths. Recover the time or move the date.",
            LargeFloat => "Very large float usually means missing logic.",
            MissingCode => "Coding drives filters, layouts and reports. Fill in the missing values.",
            ActualAfterDataDate => "Actual dates cannot be in the future. Correct the actuals or move the data date.",
            ForecastBeforeDataDate => "Remaining work is shown in the past. Reschedule (F9) at the current data date or status the activity.",
            OutOfSequence => "Work started before its predecessor allows. Confirm the progress or fix the logic.",
            RelationshipMissingActivity => "The relationship points at an activity that is not in the file. Delete it or bring the activity back.",
            ActivityMissingWbs => "The activity's WBS node is not in the file. Move the activity to an existing WBS node.",
            ActivityMissingCalendar => "The activity's calendar is not in the file. Assign an existing calendar.",
            AssignmentMissingActivity => "The assignment points at an activity that is not in the file.",
            AssignmentMissingResource => "The assignment's resource is not in the file. Reassign it.",
            CodeAssignmentMissingValue => "The code value is not in the file. Pick an existing value.",
            NotebookMissingTopic => "The notebook topic is not in the file.",
            DuplicateActivityId => "Activity IDs must be unique. Renumber the duplicates.",
            DuplicateRelationship => "The same relationship appears more than once. Delete the copies.",
            DuplicateWbsCode => "Two WBS nodes under one parent share a code. Rename one.",
            MilestoneWithAssignments => "Milestones cannot carry resource or role assignments. Remove them, or make the activity a task; a milestone may keep a primary resource.",
        }
    }
    pub fn severity(self) -> Severity {
        use Rule::*;
        match self {
            SoftConstraint | NonFinishToStart | LargeFloat | MissingCode | LongDuration
            | LongLag => Severity::Warning,
            _ => Severity::Error,
        }
    }
    /// Checked when "Run Priority Checks Only" is set.
    pub fn priority(self) -> bool {
        use Rule::*;
        matches!(
            self,
            MissingPredecessor
                | MissingSuccessor
                | HardConstraint
                | NegativeLag
                | NegativeFloat
                | ActualAfterDataDate
                | ForecastBeforeDataDate
                | RelationshipMissingActivity
                | ActivityMissingWbs
                | ActivityMissingCalendar
                | DuplicateActivityId
        )
    }
    pub fn key(self) -> String {
        format!("{self:?}")
    }
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|r| r.key() == key)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroupBy {
    Check,
    Activity,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub rules: BTreeSet<Rule>,
    pub priority_only: bool,
    pub errors_only: bool,
    pub group_by: GroupBy,
    pub long_lag_days: f64,
    pub long_duration_days: f64,
    pub large_float_days: f64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            rules: Rule::ALL.into_iter().collect(),
            priority_only: false,
            errors_only: false,
            group_by: GroupBy::Check,
            long_lag_days: 5.,
            long_duration_days: 44.,
            large_float_days: 44.,
        }
    }
}

impl Config {
    pub fn runs(&self, rule: Rule) -> bool {
        self.rules.contains(&rule) && (!self.priority_only || rule.priority())
    }
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "rules": self.rules.iter().map(|r| r.key()).collect::<Vec<_>>(),
            "priority_only": self.priority_only,
            "errors_only": self.errors_only,
            "group_by_activity": self.group_by == GroupBy::Activity,
            "long_lag_days": self.long_lag_days,
            "long_duration_days": self.long_duration_days,
            "large_float_days": self.large_float_days,
        })
    }
    pub fn from_json(v: &serde_json::Value) -> Self {
        let mut c = Self::default();
        if let Some(list) = v["rules"].as_array() {
            c.rules = list
                .iter()
                .filter_map(|k| k.as_str().and_then(Rule::from_key))
                .collect();
        }
        c.priority_only = v["priority_only"].as_bool().unwrap_or(false);
        c.errors_only = v["errors_only"].as_bool().unwrap_or(false);
        if v["group_by_activity"].as_bool() == Some(true) {
            c.group_by = GroupBy::Activity;
        }
        let days = |key: &str, default: f64| {
            v[key]
                .as_f64()
                .filter(|d| d.is_finite() && *d >= 0.)
                .unwrap_or(default)
        };
        c.long_lag_days = days("long_lag_days", c.long_lag_days);
        c.long_duration_days = days("long_duration_days", c.long_duration_days);
        c.large_float_days = days("large_float_days", c.large_float_days);
        c
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Finding {
    /// Activity ID the finding is about (empty for records without one).
    pub code: String,
    pub name: String,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RuleResult {
    pub rule: Rule,
    /// How many records the rule looked at.
    pub checked: usize,
    pub findings: Vec<Finding>,
}

impl RuleResult {
    pub fn passed(&self) -> bool {
        self.findings.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProjectResult {
    pub label: String,
    pub results: Vec<RuleResult>,
}

fn num(s: &str) -> i64 {
    s.trim().parse().unwrap_or(0)
}

fn ids(p: &ProjectData, table: &str, key: &str) -> HashSet<i64> {
    p.document.table(table).map_or_else(HashSet::new, |t| {
        t.rows.iter().map(|r| num(t.value(r, key))).collect()
    })
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

fn link_short(t: LinkType) -> &'static str {
    match t {
        LinkType::FS => "FS",
        LinkType::SS => "SS",
        LinkType::FF => "FF",
        LinkType::SF => "SF",
        LinkType::None => "",
    }
}

struct Ctx<'p> {
    p: &'p ProjectData,
    pid: i64,
    hours_per_day: f64,
    /// task_id -> Activity ID for every TASK row of the project.
    codes: HashMap<i64, String>,
}

impl Ctx<'_> {
    fn activity(&self, i: usize, detail: impl Into<String>) -> Finding {
        Finding {
            code: self.p.model.activities[i].task_code.clone(),
            name: self.p.name(i).to_string(),
            detail: detail.into(),
        }
    }
    fn task(&self, task_id: i64, detail: impl Into<String>) -> Finding {
        let code = self.codes.get(&task_id).cloned().unwrap_or_default();
        let name = self
            .p
            .details
            .get(&task_id)
            .map_or(String::new(), |d| d.name.clone());
        Finding {
            code,
            name,
            detail: detail.into(),
        }
    }
    fn days(&self, minutes: i64) -> f64 {
        minutes as f64 / 60. / self.hours_per_day
    }
    fn duration(&self, minutes: i64) -> String {
        format::duration(minutes, format::project_periods(self.p))
    }
    fn date(&self, minute: Option<i64>) -> String {
        format::date(self.p.model.origin_serial, minute)
    }
    fn rows(&self, table: &str) -> Vec<(usize, &Vec<String>)> {
        self.p.document.table(table).map_or(vec![], |t| {
            let has_project = t.field("proj_id").is_some();
            t.rows
                .iter()
                .enumerate()
                .filter(|(_, r)| !has_project || num(t.value(r, "proj_id")) == self.pid)
                .collect()
        })
    }
    fn value(&self, table: &str, row: &[String], field: &str) -> String {
        self.p
            .document
            .table(table)
            .map_or(String::new(), |t| t.value(row, field).to_string())
    }
}

fn check(c: &Ctx, rule: Rule, config: &Config) -> RuleResult {
    let m = &c.p.model;
    let n = m.activities.len();
    let open: Vec<usize> = (0..n)
        .filter(|&i| {
            let a = &m.activities[i];
            !a.is_completed() && !matches!(a.task_type, TaskType::Loe | TaskType::Wbs)
        })
        .collect();
    let link_text = |l: usize| {
        let link = &m.links[l];
        let lag = if link.lag == 0 {
            String::new()
        } else {
            format!(" lag {}", c.duration(link.lag))
        };
        format!(
            "{} {} {}{lag}",
            m.activities[link.pred].task_code,
            link_short(link.link_type),
            m.activities[link.succ].task_code
        )
    };
    let link_finding = |l: usize| {
        let mut f = c.activity(m.links[l].succ, link_text(l));
        f.detail = format!("Relationship {}", f.detail);
        f
    };
    let data_date = m.project.data_date;
    let (checked, findings): (usize, Vec<Finding>) = match rule {
        Rule::MissingPredecessor => (
            open.len(),
            open.iter()
                .filter(|&&i| m.activities[i].preds.is_empty())
                .map(|&i| c.activity(i, "No predecessor"))
                .collect(),
        ),
        Rule::MissingSuccessor => (
            open.len(),
            open.iter()
                .filter(|&&i| m.activities[i].succs.is_empty())
                .map(|&i| c.activity(i, "No successor"))
                .collect(),
        ),
        Rule::HardConstraint | Rule::SoftConstraint => {
            let wanted = if rule == Rule::HardConstraint {
                hard
            } else {
                soft
            };
            (
                open.len(),
                open.iter()
                    .filter_map(|&i| {
                        let a = &m.activities[i];
                        let mut parts = vec![];
                        for (t, d) in [(a.cstr, a.cstr_date), (a.cstr2, a.cstr_date2)] {
                            if wanted(t) {
                                parts.push(format!(
                                    "{} {}",
                                    openxer_xer::constraint_label(t),
                                    c.date(d)
                                ));
                            }
                        }
                        (!parts.is_empty())
                            .then(|| c.activity(i, parts.join("; ").trim().to_string()))
                    })
                    .collect(),
            )
        }
        Rule::NegativeLag => (
            m.links.len(),
            (0..m.links.len())
                .filter(|&l| m.links[l].lag < 0)
                .map(link_finding)
                .collect(),
        ),
        Rule::LongLag => (
            m.links.len(),
            (0..m.links.len())
                .filter(|&l| c.days(m.links[l].lag) > config.long_lag_days + 1e-9)
                .map(link_finding)
                .collect(),
        ),
        Rule::StartToFinish | Rule::NonFinishToStart => (
            m.links.len(),
            (0..m.links.len())
                .filter(|&l| {
                    let t = m.links[l].link_type;
                    if rule == Rule::StartToFinish {
                        t == LinkType::SF
                    } else {
                        matches!(t, LinkType::SS | LinkType::FF)
                    }
                })
                .map(link_finding)
                .collect(),
        ),
        Rule::LongDuration => (
            open.len(),
            open.iter()
                .filter(|&&i| c.days(m.activities[i].rem) > config.long_duration_days + 1e-9)
                .map(|&i| {
                    c.activity(
                        i,
                        format!("Remaining duration {}", c.duration(m.activities[i].rem)),
                    )
                })
                .collect(),
        ),
        Rule::ZeroDurationTask => (
            open.len(),
            open.iter()
                .filter(|&&i| {
                    let a = &m.activities[i];
                    matches!(a.task_type, TaskType::Task | TaskType::Resource)
                        && a.status == StatusType::NotStarted
                        && a.rem == 0
                })
                .map(|&i| c.activity(i, "Duration 0 on a task"))
                .collect(),
        ),
        Rule::NegativeFloat | Rule::LargeFloat => (
            open.len(),
            open.iter()
                .filter_map(|&i| {
                    let tf = m.activities[i].total_float?;
                    let hit = if rule == Rule::NegativeFloat {
                        tf < 0
                    } else {
                        c.days(tf) > config.large_float_days + 1e-9
                    };
                    hit.then(|| c.activity(i, format!("Total float {}", c.duration(tf))))
                })
                .collect(),
        ),
        Rule::MissingCode => {
            let types = crate::grid::columns::code_types(c.p);
            let ix = crate::grid::columns::Index::new((false, 0.));
            (
                if types.is_empty() { 0 } else { n },
                (0..n)
                    .filter_map(|i| {
                        let task_id = m.activities[i].task_id;
                        let missing: Vec<&str> = types
                            .iter()
                            .filter(|(id, _)| ix.code_of(c.p, task_id, *id).is_none())
                            .map(|(_, name)| name.as_str())
                            .collect();
                        (!missing.is_empty())
                            .then(|| c.activity(i, format!("No value for {}", missing.join(", "))))
                    })
                    .collect(),
            )
        }
        Rule::ActualAfterDataDate => (
            n,
            schedule_check::actuals_after_data_date(m)
                .into_iter()
                .map(|i| {
                    let a = &m.activities[i];
                    let which = if a.act_finish.zip(data_date).is_some_and(|(f, d)| f > d) {
                        format!("Actual finish {}", c.date(a.act_finish))
                    } else {
                        format!("Actual start {}", c.date(a.act_start))
                    };
                    c.activity(i, format!("{which} after data date {}", c.date(data_date)))
                })
                .collect(),
        ),
        Rule::ForecastBeforeDataDate => (
            open.len(),
            match data_date {
                None => vec![],
                Some(dd) => open
                    .iter()
                    .filter_map(|&i| {
                        let a = &m.activities[i];
                        let (label, d) = match a.status {
                            StatusType::NotStarted => ("Start", a.early_start),
                            _ => ("Finish", a.early_finish),
                        };
                        d.filter(|d| *d < dd).map(|d| {
                            c.activity(
                                i,
                                format!(
                                    "{label} {} before data date {}",
                                    c.date(Some(d)),
                                    c.date(data_date)
                                ),
                            )
                        })
                    })
                    .collect(),
            },
        ),
        Rule::OutOfSequence => (
            m.links.len(),
            schedule_check::out_of_sequence_links(m)
                .into_iter()
                .map(link_finding)
                .collect(),
        ),
        Rule::RelationshipMissingActivity => {
            let rows = c.rows("TASKPRED");
            let tasks = ids(c.p, "TASK", "task_id");
            (
                rows.len(),
                rows.iter()
                    .filter_map(|(_, r)| {
                        let succ = num(&c.value("TASKPRED", r, "task_id"));
                        let pred = num(&c.value("TASKPRED", r, "pred_task_id"));
                        let pred_project = c.value("TASKPRED", r, "pred_proj_id");
                        let own = pred_project.is_empty() || num(&pred_project) == c.pid;
                        if !tasks.contains(&succ) {
                            Some(c.task(
                                succ,
                                format!(
                                    "Successor task {succ} is missing (predecessor {})",
                                    c.codes.get(&pred).cloned().unwrap_or(pred.to_string())
                                ),
                            ))
                        } else if own && !tasks.contains(&pred) {
                            Some(c.task(succ, format!("Predecessor task {pred} is missing")))
                        } else {
                            None
                        }
                    })
                    .collect(),
            )
        }
        Rule::ActivityMissingWbs | Rule::ActivityMissingCalendar => {
            let (field, table, key, label) = if rule == Rule::ActivityMissingWbs {
                ("wbs_id", "PROJWBS", "wbs_id", "WBS")
            } else {
                ("clndr_id", "CALENDAR", "clndr_id", "Calendar")
            };
            let known = ids(c.p, table, key);
            let rows = c.rows("TASK");
            (
                rows.len(),
                rows.iter()
                    .filter_map(|(_, r)| {
                        let v = c.value("TASK", r, field);
                        (!v.trim().is_empty() && !known.contains(&num(&v))).then(|| {
                            c.task(
                                num(&c.value("TASK", r, "task_id")),
                                format!("{label} {v} is not in the file"),
                            )
                        })
                    })
                    .collect(),
            )
        }
        Rule::AssignmentMissingActivity | Rule::AssignmentMissingResource => {
            let rows = c.rows("TASKRSRC");
            let tasks = ids(c.p, "TASK", "task_id");
            let resources = ids(c.p, "RSRC", "rsrc_id");
            (
                rows.len(),
                rows.iter()
                    .filter_map(|(_, r)| {
                        let task = num(&c.value("TASKRSRC", r, "task_id"));
                        let rsrc = c.value("TASKRSRC", r, "rsrc_id");
                        let id = c.value("TASKRSRC", r, "taskrsrc_id");
                        if rule == Rule::AssignmentMissingActivity {
                            (!tasks.contains(&task)).then(|| {
                                c.task(task, format!("Assignment {id}: task {task} is missing"))
                            })
                        } else {
                            (!rsrc.trim().is_empty() && !resources.contains(&num(&rsrc))).then(
                                || {
                                    c.task(
                                        task,
                                        format!("Assignment {id}: resource {rsrc} is missing"),
                                    )
                                },
                            )
                        }
                    })
                    .collect(),
            )
        }
        Rule::MilestoneWithAssignments => (
            m.activities
                .iter()
                .filter(|a| openxer_xer::assignments::is_milestone(a.task_type))
                .count(),
            openxer_xer::assignments::milestones_with_assignments(c.p)
                .into_iter()
                .map(|(i, labels)| c.activity(i, format!("Assigned: {}", labels.join(", "))))
                .collect(),
        ),
        Rule::CodeAssignmentMissingValue => {
            let rows = c.rows("TASKACTV");
            let values = ids(c.p, "ACTVCODE", "actv_code_id");
            (
                rows.len(),
                rows.iter()
                    .filter_map(|(_, r)| {
                        let v = num(&c.value("TASKACTV", r, "actv_code_id"));
                        (!values.contains(&v)).then(|| {
                            c.task(
                                num(&c.value("TASKACTV", r, "task_id")),
                                format!("Code value {v} is not in the file"),
                            )
                        })
                    })
                    .collect(),
            )
        }
        Rule::NotebookMissingTopic => {
            let rows = c.rows("TASKMEMO");
            let topics = ids(c.p, "MEMOTYPE", "memo_type_id");
            (
                rows.len(),
                rows.iter()
                    .filter_map(|(_, r)| {
                        let v = num(&c.value("TASKMEMO", r, "memo_type_id"));
                        (!topics.contains(&v)).then(|| {
                            c.task(
                                num(&c.value("TASKMEMO", r, "task_id")),
                                format!("Notebook topic {v} is not in the file"),
                            )
                        })
                    })
                    .collect(),
            )
        }
        Rule::DuplicateActivityId => {
            let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
            for a in &m.activities {
                *seen.entry(a.task_code.as_str()).or_default() += 1;
            }
            (
                n,
                (0..n)
                    .filter(|&i| seen[m.activities[i].task_code.as_str()] > 1)
                    .map(|i| {
                        c.activity(
                            i,
                            format!(
                                "Activity ID used {} times",
                                seen[m.activities[i].task_code.as_str()]
                            ),
                        )
                    })
                    .collect(),
            )
        }
        Rule::DuplicateRelationship => {
            let mut seen: HashSet<(usize, usize, &str)> = HashSet::new();
            (
                m.links.len(),
                (0..m.links.len())
                    .filter(|&l| {
                        let k = &m.links[l];
                        !seen.insert((k.pred, k.succ, link_short(k.link_type)))
                    })
                    .map(link_finding)
                    .collect(),
            )
        }
        Rule::DuplicateWbsCode => {
            let mut seen: HashMap<(Option<i64>, String), usize> = HashMap::new();
            for w in &c.p.wbs {
                *seen.entry((w.parent, w.code.clone())).or_default() += 1;
            }
            (
                c.p.wbs.len(),
                c.p.wbs
                    .iter()
                    .filter(|w| seen[&(w.parent, w.code.clone())] > 1)
                    .map(|w| Finding {
                        code: String::new(),
                        name: w.name.clone(),
                        detail: format!(
                            "WBS code {} is used more than once under its parent",
                            w.code
                        ),
                    })
                    .collect(),
            )
        }
    };
    RuleResult {
        rule,
        checked,
        findings,
    }
}

/// Run every rule the configuration turns on.
pub fn run(p: &ProjectData, config: &Config) -> Vec<RuleResult> {
    let pid = p.model.project.proj_id;
    let codes = p.document.table("TASK").map_or_else(HashMap::new, |t| {
        t.rows
            .iter()
            .map(|r| {
                (
                    num(t.value(r, "task_id")),
                    t.value(r, "task_code").to_string(),
                )
            })
            .collect()
    });
    let c = Ctx {
        p,
        pid,
        hours_per_day: (format::project_periods(p).day as f64 / 60.).max(0.1),
        codes,
    };
    Rule::ALL
        .into_iter()
        .filter(|r| config.runs(*r))
        .map(|r| check(&c, r, config))
        .collect()
}

fn severity_text(rule: Rule) -> &'static str {
    match rule.severity() {
        Severity::Error => "Error",
        Severity::Warning => "Warning",
    }
}

/// The report for one or more checked projects.
pub fn report(projects: &[ProjectResult], config: &Config) -> Report {
    let mut sections = vec![];
    let many = projects.len() > 1;
    for pr in projects {
        let failed = pr.results.iter().filter(|r| !r.passed()).count();
        let mut summary_rows = vec![];
        for g in Group::ALL {
            let in_group: Vec<&RuleResult> = pr
                .results
                .iter()
                .filter(|r| r.rule.group() == g && (!config.errors_only || !r.passed()))
                .collect();
            if in_group.is_empty() {
                continue;
            }
            summary_rows.push(Row::band(1, g.caption(), 6));
            for r in in_group {
                let result = if r.passed() {
                    "Passed"
                } else {
                    severity_text(r.rule)
                };
                summary_rows.push(Row::new(
                    vec![
                        Cell::Text(r.rule.caption().into()),
                        Cell::Text(severity_text(r.rule).into()),
                        Cell::number(r.checked as f64, r.checked.to_string()),
                        Cell::number(r.findings.len() as f64, r.findings.len().to_string()),
                        Cell::Text(result.into()),
                        Cell::Text(if r.passed() {
                            String::new()
                        } else {
                            r.rule.advice().into()
                        }),
                    ],
                    if r.passed() {
                        Tone::Good
                    } else if r.rule.severity() == Severity::Error {
                        Tone::Bad
                    } else {
                        Tone::Changed
                    },
                ));
            }
        }
        let prefix = if many {
            format!("{}: ", pr.label)
        } else {
            String::new()
        };
        sections.push(Section {
            heading: format!("{prefix}Summary"),
            paragraphs: vec![format!(
                "{} checks run, {} found problems ({} errors, {} warnings).",
                pr.results.len(),
                failed,
                pr.results
                    .iter()
                    .filter(|r| !r.passed() && r.rule.severity() == Severity::Error)
                    .count(),
                pr.results
                    .iter()
                    .filter(|r| !r.passed() && r.rule.severity() == Severity::Warning)
                    .count()
            )],
            tables: vec![Table {
                name: format!("{prefix}Summary"),
                caption: String::new(),
                columns: vec![
                    Column::left("Check", 34.),
                    Column::left("Severity", 8.),
                    Column::right("Checked", 7.),
                    Column::right("Found", 6.),
                    Column::left("Result", 8.),
                    Column::left("What to do", 50.),
                ],
                rows: summary_rows,
            }],
        });
        let failing: Vec<&RuleResult> = pr.results.iter().filter(|r| !r.passed()).collect();
        if failing.is_empty() {
            continue;
        }
        let columns = vec![
            Column::left("Activity ID", 12.),
            Column::left("Activity Name", 34.),
            Column::left("Detail", 50.),
        ];
        match config.group_by {
            GroupBy::Check => {
                for r in failing {
                    sections.push(Section {
                        heading: format!("{prefix}{} ({})", r.rule.caption(), r.findings.len()),
                        paragraphs: vec![r.rule.advice().into()],
                        tables: vec![Table {
                            name: r.rule.caption().into(),
                            caption: String::new(),
                            columns: columns.clone(),
                            rows: r
                                .findings
                                .iter()
                                .map(|f| {
                                    Row::new(
                                        vec![
                                            Cell::Text(f.code.clone()),
                                            Cell::Text(f.name.clone()),
                                            Cell::Text(f.detail.clone()),
                                        ],
                                        Tone::Normal,
                                    )
                                })
                                .collect(),
                        }],
                    });
                }
            }
            GroupBy::Activity => {
                let mut by: BTreeMap<(String, String), Vec<(Rule, String)>> = BTreeMap::new();
                for r in failing {
                    for f in &r.findings {
                        by.entry((f.code.clone(), f.name.clone()))
                            .or_default()
                            .push((r.rule, f.detail.clone()));
                    }
                }
                let mut rows = vec![];
                for ((code, name), list) in by {
                    rows.push(Row::band(3, format!("{code}  {name}"), 3));
                    for (rule, detail) in list {
                        rows.push(Row::new(
                            vec![
                                Cell::Text(rule.caption().into()),
                                Cell::Text(severity_text(rule).into()),
                                Cell::Text(detail),
                            ],
                            if rule.severity() == Severity::Error {
                                Tone::Bad
                            } else {
                                Tone::Changed
                            },
                        ));
                    }
                }
                sections.push(Section {
                    heading: format!("{prefix}Findings by Activity"),
                    paragraphs: vec![],
                    tables: vec![Table {
                        name: format!("{prefix}Findings"),
                        caption: String::new(),
                        columns: vec![
                            Column::left("Check", 34.),
                            Column::left("Severity", 8.),
                            Column::left("Detail", 54.),
                        ],
                        rows,
                    }],
                });
            }
        }
    }
    let labels: Vec<&str> = projects.iter().map(|p| p.label.as_str()).collect();
    Report {
        title: "Project Checker".into(),
        header: vec![
            format!("Projects: {}", labels.join(", ")),
            format!("Executed by {}", doc::user_name()),
            format!("Date {}", doc::run_stamp()),
            format!(
                "Limits: lags over {} days, remaining durations over {} days, total float over {} days.",
                config.long_lag_days, config.long_duration_days, config.large_float_days
            ),
        ],
        sections,
        footer: "Project Checker report".into(),
    }
}
