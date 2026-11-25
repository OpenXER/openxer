//! Typed edit operations for `openxer-cli apply`, the single-operation edit
//! commands and their MCP tools (format `openxer-changes/1`).
//!
//! Operations name things the way an agent sees them: Activity ID, resource
//! short name, calendar name and WBS path. Durations are hours, dates are
//! project-local `YYYY-MM-DD HH:MM`, and enum values accept XER codes
//! (`TT_Task`, `CS_ALAP`, `DT_FixedDUR2`) or display names.
//!
//! Every edit goes through `openxer_xer::edit`, the same setters the desktop
//! uses, so the command line, MCP and the app cannot behave differently. A
//! batch runs on a copy of the project in file order; the first failure
//! leaves the caller's project unchanged.
//!
//! With `recalculate` false ("apply exactly") `set_activity` and
//! `set_assignment` write each value into its XER column as given, with no
//! duration type, units or cost knock-on; the project is then re-read from
//! those columns. Scheduling still runs normally.
use crate::{schedule_project, AutomationError, Result, SchedulePatch};
use openxer_engine::model::LinkType;
use openxer_xer::assignments::AssignmentEdit as AE;
use openxer_xer::calendar_data::Shift;
use openxer_xer::edit::activity::{self as act, Edit};
use openxer_xer::edit::project_records::{self, Kind};
use openxer_xer::edit::tables::{self, serde_like::Value as Cell};
use openxer_xer::edit::{
    calendars, curves, documents, earned_value, funding, progress, records, relationships,
    resources, steps, thresholds, wbs,
};
use openxer_xer::{parse_relative, Document, ProjectData};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};

pub const FORMAT: &str = "openxer-changes/1";
/// Largest batch one apply takes.
pub const MAX_OPERATIONS: usize = 100_000;

/// A changes file. Other top-level keys (`source`, `expect`, notes) are
/// carried for people and checks and never applied.
#[derive(Debug, Clone, Deserialize)]
pub struct Changes {
    pub format: String,
    /// Default for every operation: true runs the setters' rules, false
    /// writes values exactly.
    #[serde(default)]
    pub recalculate: Option<bool>,
    #[serde(deserialize_with = "operations")]
    pub operations: Vec<Operation>,
}

/// Each operation may carry a free-form `meta` object (provenance: source,
/// evidence, written_by, basis...). It is kept out of the operation itself;
/// every other unknown field is refused.
pub fn operation(mut value: Value) -> std::result::Result<Operation, String> {
    if let Some(map) = value.as_object_mut() {
        if let Some(meta) = map.remove("meta") {
            if !meta.is_object() {
                return Err("meta must be an object".into());
            }
        }
    }
    serde_json::from_value(value).map_err(|e| e.to_string())
}

fn operations<'de, D>(d: D) -> std::result::Result<Vec<Operation>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let list = Vec::<Value>::deserialize(d)?;
    list.into_iter()
        .enumerate()
        .map(|(n, v)| {
            operation(v).map_err(|e| serde::de::Error::custom(format!("operation {}: {e}", n + 1)))
        })
        .collect()
}

/// Absent stays `None`; a present value, null included, is `Some`.
fn present<'de, D>(d: D) -> std::result::Result<Option<Value>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Value::deserialize(d).map(Some)
}

/// Every operation name, in the order of [`Operation`] (the MCP
/// `openxer_apply` description lists them; a test checks the two agree).
pub const OPERATIONS: &[&str] = &[
    "set_project",
    "add_wbs",
    "add_activity",
    "delete_activity",
    "dissolve_activity",
    "set_activity",
    "assign_resource",
    "remove_assignment",
    "set_assignment",
    "relationship",
    "calendar_day",
    "calendar_weekday",
    "schedule",
    "set_resource_limit",
    "set_activity_code",
    "set_project_code",
    "add_code_type",
    "add_code_value",
    "set_code_value",
    "delete_code_value",
    "set_notebook",
    "set_wbs_notebook",
    "set_udf",
    "add_calendar",
    "set_calendar",
    "delete_calendar",
    "add_resource",
    "set_resource",
    "set_wbs",
    "set_schedule_option",
    "add_project_funding",
    "set_project_funding",
    "delete_project_funding",
    "delete_resource",
    "merge_resources",
    "set_wbs_earned_value",
    "add_document",
    "set_document",
    "delete_document",
    "link_document",
    "unlink_document",
    "add_expense",
    "set_expense",
    "delete_expense",
    "add_issue",
    "set_issue",
    "delete_issue",
    "add_risk",
    "set_risk",
    "delete_risk",
    "assign_risk",
    "unassign_risk",
    "add_threshold",
    "set_threshold",
    "delete_threshold",
    "monitor_thresholds",
    "update_progress",
    "top_down_estimate",
    "shift_wbs",
    "rename_wbs_code",
    "rename_calendar",
    "set_default_calendar",
    "convert_calendar",
    "copy_calendar",
    "add_step",
    "set_step",
    "delete_step",
    "move_step",
    "add_steps_from_template",
    "create_step_template",
    "set_feedback",
    "apply_actuals",
    "recalculate_costs",
    "delete_financial_period",
    "global_change",
];

/// One edit. New kinds of edits (activity codes, UDF values, notebook
/// topics, resource limits) are new variants here and in [`apply_one`],
/// named the same way: resource short name, code type name and value,
/// topic name.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    /// `data_date` stores the new data date; `schedule: true` also schedules
    /// at it right away (as a schedule run before progress is entered).
    SetProject {
        field: String,
        value: Value,
        #[serde(default)]
        schedule: Option<bool>,
    },
    AddWbs {
        wbs: String,
        #[serde(default)]
        name: Option<String>,
    },
    /// Without `wbs` the activity goes under the WBS element of `selected`,
    /// else the project's first WBS element until a `set_activity wbs`
    /// moves it. Without `activity` its ID comes from the project's
    /// auto-numbering defaults, `selected` standing for the activity
    /// selected in the app.
    AddActivity {
        #[serde(default)]
        activity: Option<String>,
        #[serde(default)]
        selected: Option<String>,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        wbs: Option<String>,
    },
    DeleteActivity {
        activity: String,
    },
    /// Edit > Dissolve: remove the activity and tie each predecessor to each
    /// successor (`openxer_xer::edit::dissolve`). `retain_lag` (default
    /// false) gives the new relationships the two old lags added.
    DissolveActivity {
        activity: String,
        #[serde(default)]
        retain_lag: Option<bool>,
    },
    SetActivity {
        activity: String,
        field: String,
        value: Value,
        #[serde(default)]
        recalculate: Option<bool>,
    },
    AssignResource {
        activity: String,
        resource: String,
    },
    RemoveAssignment {
        activity: String,
        resource: String,
    },
    SetAssignment {
        activity: String,
        resource: String,
        field: String,
        value: Value,
        #[serde(default)]
        recalculate: Option<bool>,
    },
    Relationship {
        action: String,
        predecessor: String,
        successor: String,
        kind: String,
        #[serde(default)]
        lag_hours: Option<f64>,
        #[serde(default)]
        comments: Option<String>,
    },
    /// `shifts` null puts the date back on its weekday pattern; `[]` makes
    /// it nonwork.
    CalendarDay {
        calendar: String,
        date: String,
        #[serde(default)]
        shifts: Option<Vec<[String; 2]>>,
    },
    /// `weekday` 0 is Sunday.
    CalendarWeekday {
        calendar: String,
        weekday: usize,
        shifts: Vec<[String; 2]>,
    },
    /// `level_resources` left out keeps the loaded setting.
    Schedule {
        #[serde(default)]
        level_resources: Option<bool>,
    },
    /// One resource rate row: the limit and prices from a date.
    /// `max_units_per_hour: null` removes the row.
    SetResourceLimit {
        resource: String,
        from: String,
        #[serde(default)]
        shift_period: Option<String>,
        #[serde(default, deserialize_with = "present")]
        max_units_per_hour: Option<Value>,
        #[serde(default, deserialize_with = "present")]
        price_per_unit: Option<Value>,
        #[serde(default, deserialize_with = "present")]
        price_per_unit_2: Option<Value>,
        #[serde(default, deserialize_with = "present")]
        price_per_unit_3: Option<Value>,
        #[serde(default, deserialize_with = "present")]
        price_per_unit_4: Option<Value>,
        #[serde(default, deserialize_with = "present")]
        price_per_unit_5: Option<Value>,
    },
    /// The value (short name) of a code type on an activity; null removes
    /// it and a missing value changes nothing.
    SetActivityCode {
        activity: String,
        code_type: String,
        #[serde(default, deserialize_with = "present")]
        value: Option<Value>,
    },
    /// The value (short name) of a project code type on the project; null
    /// removes it and a missing value changes nothing.
    SetProjectCode {
        code_type: String,
        #[serde(default, deserialize_with = "present")]
        value: Option<Value>,
    },
    /// A new activity code type; scope is "project" (default), "eps" or
    /// "global".
    AddCodeType {
        code_type: String,
        #[serde(default)]
        scope: Option<String>,
        #[serde(default)]
        max_length: Option<i64>,
    },
    AddCodeValue {
        code_type: String,
        code: String,
    },
    /// An ACTVCODE column (`parent` by short name).
    SetCodeValue {
        code_type: String,
        code: String,
        field: String,
        value: Value,
    },
    DeleteCodeValue {
        code_type: String,
        code: String,
    },
    /// A notebook topic's text (HTML as stored); null removes it and a
    /// missing text changes nothing.
    SetNotebook {
        activity: String,
        topic: String,
        #[serde(default, deserialize_with = "present")]
        text: Option<Value>,
    },
    SetWbsNotebook {
        wbs: String,
        topic: String,
        #[serde(default, deserialize_with = "present")]
        text: Option<Value>,
    },
    /// A user defined field value by label, on one record: an activity, a
    /// resource, a resource assignment (activity and resource), an activity
    /// step (activity and step), a WBS element, the project, an expense, an
    /// issue, a risk or a work product or document.
    SetUdf {
        #[serde(default)]
        activity: Option<String>,
        #[serde(default)]
        resource: Option<String>,
        #[serde(default)]
        step: Option<Value>,
        #[serde(default)]
        wbs: Option<String>,
        /// The open project's own value (`true`).
        #[serde(default)]
        project: Option<bool>,
        #[serde(default)]
        expense: Option<String>,
        #[serde(default)]
        issue: Option<String>,
        #[serde(default)]
        risk: Option<String>,
        #[serde(default)]
        document: Option<String>,
        udf: String,
        value: Value,
    },
    AddCalendar {
        calendar: String,
    },
    /// A CALENDAR column, `base_calendar` (a name) or `project_calendar`.
    SetCalendar {
        calendar: String,
        field: String,
        value: Value,
    },
    /// Without `replacement` or `default_global` only a calendar nothing
    /// uses is deleted. With `replacement` (a global calendar's name) or
    /// `default_global`, everything that uses it moves there first.
    DeleteCalendar {
        calendar: String,
        #[serde(default)]
        replacement: Option<String>,
        #[serde(default)]
        default_global: bool,
    },
    AddResource {
        resource: String,
    },
    /// An RSRC column, `parent` (short name) or `calendar` (name).
    SetResource {
        resource: String,
        field: String,
        value: Value,
    },
    /// A PROJWBS column of the element at a WBS path.
    SetWbs {
        wbs: String,
        field: String,
        value: Value,
    },
    /// A SCHEDOPTIONS column.
    SetScheduleOption {
        field: String,
        value: Value,
    },
    /// Funding of the project from a funding source (by name, from the
    /// FUNDSRC dictionary), with an amount and a fund share (percent).
    AddProjectFunding {
        source: String,
        #[serde(default)]
        amount: Option<Value>,
        #[serde(default)]
        share: Option<Value>,
    },
    /// `field` is `source` (another funding source's name), `amount` or
    /// `share`.
    SetProjectFunding {
        source: String,
        field: String,
        value: Value,
    },
    DeleteProjectFunding {
        source: String,
    },
    /// Delete a resource (short name) that has no assignments and no child
    /// resources.
    DeleteResource {
        resource: String,
    },
    /// Merge the resources `from` (short names) into `into`: assignments in
    /// every project, rates, codes, user defined field values, role links,
    /// child resources and the calendar move to `into` (assignments of one
    /// activity are combined), then the `from` resources are removed.
    MergeResources {
        from: Vec<String>,
        into: String,
    },
    /// Earned value techniques of a WBS element (`wbs` path; "" is the
    /// project node). `performance`: activity_percent, wbs_milestones,
    /// 0/100, 50/50 or custom; `estimate_to_complete`: remaining, pf_1,
    /// pf_cpi, pf_cpi_spi or pf.
    SetWbsEarnedValue {
        wbs: String,
        #[serde(default)]
        performance: Option<String>,
        #[serde(default)]
        custom_percent: Option<Value>,
        #[serde(default)]
        estimate_to_complete: Option<String>,
        #[serde(default)]
        performance_factor: Option<Value>,
    },
    /// A work product or document (the WPs & Docs view), titled `title`,
    /// under the document `parent` (reference number, title or id) or at
    /// the top level, with any other fields given.
    AddDocument {
        title: String,
        #[serde(default)]
        parent: Option<String>,
        #[serde(default)]
        reference: Option<String>,
        #[serde(default)]
        fields: Option<Map<String, Value>>,
    },
    /// One field of a document: title, reference, deliverable, status,
    /// category, author, version, date, private_location, public_location
    /// or description.
    SetDocument {
        document: String,
        field: String,
        value: Value,
    },
    /// Delete a document with the documents under it and their links.
    DeleteDocument {
        document: String,
    },
    /// Link a document to an activity (Activity ID) or a WBS element (path).
    LinkDocument {
        document: String,
        #[serde(default)]
        activity: Option<String>,
        #[serde(default)]
        wbs: Option<String>,
        #[serde(default)]
        work_product: Option<bool>,
    },
    UnlinkDocument {
        document: String,
        #[serde(default)]
        activity: Option<String>,
        #[serde(default)]
        wbs: Option<String>,
    },
    // ------------------------------------------------ project registers
    /// An expense item on an activity, with any fields given (`name`,
    /// `category`, `budgeted_cost`, ...; see `set_expense`).
    AddExpense {
        activity: String,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        fields: Option<Map<String, Value>>,
    },
    /// One field of an expense item (named by id or name).
    SetExpense {
        expense: String,
        field: String,
        value: Value,
    },
    DeleteExpense {
        expense: String,
    },
    /// An open issue identified today (or `fields.identified_on`).
    AddIssue {
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        fields: Option<Map<String, Value>>,
    },
    SetIssue {
        issue: String,
        field: String,
        value: Value,
    },
    /// Delete an issue with its history.
    DeleteIssue {
        issue: String,
    },
    /// A proposed threat identified today, with the next free Risk ID.
    AddRisk {
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        fields: Option<Map<String, Value>>,
    },
    SetRisk {
        risk: String,
        field: String,
        value: Value,
    },
    /// Delete a risk with its activity assignments.
    DeleteRisk {
        risk: String,
    },
    AssignRisk {
        risk: String,
        activity: String,
    },
    UnassignRisk {
        risk: String,
        activity: String,
    },
    /// An enabled threshold (total float below zero days unless `parameter`
    /// and `fields` say otherwise).
    AddThreshold {
        #[serde(default)]
        parameter: Option<String>,
        #[serde(default)]
        fields: Option<Map<String, Value>>,
    },
    SetThreshold {
        threshold: String,
        field: String,
        value: Value,
    },
    /// Delete a threshold; the issues it raised stay.
    DeleteThreshold {
        threshold: String,
    },
    /// A step on an activity, named `name`, at position `at` (1 is first;
    /// the end when omitted).
    AddStep {
        activity: String,
        name: String,
        #[serde(default)]
        at: Option<usize>,
    },
    /// One field of a step (by number from 1, or name): `name`, `complete`,
    /// `percent_complete`, `weight` or `description`.
    SetStep {
        activity: String,
        step: Value,
        field: String,
        value: Value,
    },
    DeleteStep {
        activity: String,
        step: Value,
    },
    /// Move a step to position `to` (1 is first).
    MoveStep {
        activity: String,
        step: Value,
        to: usize,
    },
    /// Add a step template's steps after the activity's step `after` (by
    /// number from 1, or name), or after its last step (Add from Template,
    /// Assign > Steps).
    AddStepsFromTemplate {
        activity: String,
        template: String,
        #[serde(default)]
        after: Option<Value>,
    },
    /// A new step template named `name` from the activity's steps (Create
    /// Template).
    CreateStepTemplate {
        activity: String,
        name: String,
    },
    /// The Feedback tab: notes to resources (`null` clears) and whether
    /// feedback needs review.
    SetFeedback {
        activity: String,
        #[serde(default, deserialize_with = "present")]
        notes: Option<Value>,
        #[serde(default)]
        review_required: Option<bool>,
    },
    /// Tools > Top Down Estimation: spread `labor_units` over the not started
    /// Task and Resource Dependent activities below `wbs` (the whole project
    /// when omitted) by the WBS estimation weights (`weights`, by WBS path,
    /// are stored first).
    TopDownEstimate {
        labor_units: f64,
        #[serde(default)]
        wbs: Option<String>,
        #[serde(default)]
        not_started_only: Option<bool>,
        #[serde(default)]
        weights: Option<BTreeMap<String, f64>>,
    },
    /// Move a WBS element: `up` or `down` among its siblings, `left` (out to
    /// its parent's level) or `right` (into the sibling above).
    ShiftWbs {
        wbs: String,
        direction: String,
    },
    /// Change a WBS element's own code (unique among its siblings).
    RenameWbsCode {
        wbs: String,
        code: String,
    },
    /// Rename a calendar.
    RenameCalendar {
        calendar: String,
        name: String,
    },
    /// Make a global calendar the default for new projects and resources.
    SetDefaultCalendar {
        calendar: String,
    },
    /// Convert a project calendar to `global`, or a resource calendar to
    /// `shared` or `personal`.
    ConvertCalendar {
        calendar: String,
        to: String,
    },
    /// A new calendar of `kind` (`project` default, `global`, `resource`),
    /// a copy of `source` when given (a standard 5-day week otherwise),
    /// named `calendar`.
    CopyCalendar {
        calendar: String,
        #[serde(default)]
        source: Option<String>,
        #[serde(default)]
        kind: Option<String>,
    },
    /// Update Progress: record progress as scheduled up to the new data date
    /// (every activity, or only `activities`), then move the data date.
    /// `remaining` is `always` (default: remaining work to the scheduled
    /// finish) or `duration_type` (fixed units types use up elapsed time).
    UpdateProgress {
        data_date: String,
        #[serde(default)]
        activities: Option<Vec<String>>,
        #[serde(default)]
        remaining: Option<String>,
    },
    /// Apply Actuals: Update Progress for the activities set to compute
    /// actuals; stamps the project's apply actuals date.
    ApplyActuals {
        data_date: String,
        #[serde(default)]
        remaining: Option<String>,
    },
    /// Recalculate Assignment Costs from the resources' prices.
    RecalculateCosts {
        #[serde(default)]
        active_only: bool,
        #[serde(default)]
        sync_overtime: bool,
    },
    /// Monitor Thresholds: evaluate every enabled threshold and raise or
    /// update issues. `from` and `to` replace every threshold's own window.
    MonitorThresholds {
        #[serde(default)]
        from: Option<String>,
        #[serde(default)]
        to: Option<String>,
        /// The date new issues are identified on (default today).
        #[serde(default)]
        today: Option<String>,
    },
    /// Delete a financial period (by name; `calendar` names its financial
    /// period calendar when two share the name). Refused while stored period
    /// performance refers to it, as in the app.
    DeleteFinancialPeriod {
        period: String,
        #[serde(default)]
        calendar: Option<String>,
    },
    /// Tools > Global Change on every activity of the project: a definition
    /// saved in the workspace input by `name`, or `rule` given inline (the
    /// saved form: `conditions`, `then`, `otherwise`; `match` all or any).
    /// The result lists the report's lines (activity, field, old and new
    /// value, and why a line cannot change); the lines without an error
    /// are written. A dry run returns the lines and writes nothing.
    GlobalChange {
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        rule: Option<Value>,
    },
}

impl Operation {
    /// What a record edit names, for messages.
    fn subject(&self) -> String {
        match self {
            Operation::SetResourceLimit { resource, from, .. } => format!("{resource} {from}"),
            Operation::SetActivityCode {
                activity,
                code_type,
                ..
            } => format!("{activity} {code_type}"),
            Operation::SetProjectCode { code_type, .. } => code_type.clone(),
            Operation::AddCodeType { code_type, .. } => code_type.clone(),
            Operation::AddCodeValue { code_type, code }
            | Operation::DeleteCodeValue { code_type, code } => format!("{code_type} {code}"),
            Operation::SetCodeValue {
                code_type,
                code,
                field,
                ..
            } => format!("{code_type} {code} {field}"),
            Operation::SetNotebook {
                activity, topic, ..
            } => format!("{activity} {topic}"),
            Operation::SetWbsNotebook { wbs, topic, .. } => format!("{wbs} {topic}"),
            Operation::SetUdf {
                activity,
                resource,
                wbs,
                expense,
                issue,
                risk,
                document,
                project,
                udf,
                ..
            } => format!(
                "{} {udf}",
                [activity, resource, wbs, expense, issue, risk, document]
                    .into_iter()
                    .find_map(|o| o.as_deref())
                    .or((*project == Some(true)).then_some("project"))
                    .unwrap_or("")
            ),
            Operation::AddCalendar { calendar } | Operation::DeleteCalendar { calendar, .. } => {
                calendar.clone()
            }
            Operation::SetCalendar {
                calendar, field, ..
            } => format!("{calendar} {field}"),
            Operation::AddResource { resource } => resource.clone(),
            Operation::SetResource {
                resource, field, ..
            } => format!("{resource} {field}"),
            Operation::SetWbs { wbs, field, .. } => format!("{wbs} {field}"),
            Operation::SetScheduleOption { field, .. } => field.clone(),
            Operation::SetProject { field, .. } => field.clone(),
            Operation::AddProjectFunding { source, .. }
            | Operation::DeleteProjectFunding { source } => source.clone(),
            Operation::SetProjectFunding { source, field, .. } => format!("{source} {field}"),
            Operation::DeleteResource { resource } => resource.clone(),
            Operation::DeleteFinancialPeriod { period, .. } => period.clone(),
            Operation::GlobalChange { name, .. } => name.clone().unwrap_or_default(),
            Operation::MergeResources { from, into } => format!("{} into {into}", from.join(", ")),
            Operation::SetWbsEarnedValue { wbs, .. } => wbs.clone(),
            Operation::AddDocument { title, .. } => title.clone(),
            Operation::SetDocument {
                document, field, ..
            } => format!("{document} {field}"),
            Operation::DeleteDocument { document } => document.clone(),
            Operation::LinkDocument {
                document,
                activity,
                wbs,
                ..
            }
            | Operation::UnlinkDocument {
                document,
                activity,
                wbs,
            } => format!(
                "{document} {}",
                activity.as_deref().or(wbs.as_deref()).unwrap_or("")
            ),
            Operation::AddExpense { activity, .. } => activity.clone(),
            Operation::AddStep { activity, name, .. } => format!("{activity} {name}"),
            Operation::SetStep {
                activity,
                step,
                field,
                ..
            } => format!("{activity} {step} {field}"),
            Operation::DeleteStep { activity, step }
            | Operation::MoveStep { activity, step, .. } => {
                format!("{activity} {step}")
            }
            Operation::AddStepsFromTemplate {
                activity,
                template,
                after: _,
            } => format!("{activity} {template}"),
            Operation::CreateStepTemplate { activity, name } => format!("{activity} {name}"),
            Operation::SetFeedback { activity, .. } => activity.clone(),
            Operation::TopDownEstimate { wbs, .. } => wbs.clone().unwrap_or_default(),
            Operation::ShiftWbs { wbs, direction } => format!("{wbs} {direction}"),
            Operation::RenameWbsCode { wbs, code } => format!("{wbs} {code}"),
            Operation::RenameCalendar { calendar, .. }
            | Operation::SetDefaultCalendar { calendar }
            | Operation::ConvertCalendar { calendar, .. }
            | Operation::CopyCalendar { calendar, .. } => calendar.clone(),
            Operation::SetExpense {
                expense: r, field, ..
            }
            | Operation::SetIssue {
                issue: r, field, ..
            }
            | Operation::SetRisk { risk: r, field, .. }
            | Operation::SetThreshold {
                threshold: r,
                field,
                ..
            } => format!("{r} {field}"),
            Operation::DeleteExpense { expense: r }
            | Operation::DeleteIssue { issue: r }
            | Operation::DeleteRisk { risk: r }
            | Operation::DeleteThreshold { threshold: r } => r.clone(),
            Operation::AddIssue { name, .. } | Operation::AddRisk { name, .. } => {
                name.clone().unwrap_or_default()
            }
            Operation::AddThreshold { parameter, .. } => parameter.clone().unwrap_or_default(),
            Operation::AssignRisk { risk, activity }
            | Operation::UnassignRisk { risk, activity } => {
                format!("{risk} {activity}")
            }
            _ => String::new(),
        }
    }
    pub fn name(&self) -> &'static str {
        match self {
            Operation::SetProject { .. } => "set_project",
            Operation::AddWbs { .. } => "add_wbs",
            Operation::AddActivity { .. } => "add_activity",
            Operation::DeleteActivity { .. } => "delete_activity",
            Operation::DissolveActivity { .. } => "dissolve_activity",
            Operation::SetActivity { .. } => "set_activity",
            Operation::AssignResource { .. } => "assign_resource",
            Operation::RemoveAssignment { .. } => "remove_assignment",
            Operation::SetAssignment { .. } => "set_assignment",
            Operation::Relationship { .. } => "relationship",
            Operation::CalendarDay { .. } => "calendar_day",
            Operation::CalendarWeekday { .. } => "calendar_weekday",
            Operation::Schedule { .. } => "schedule",
            Operation::SetResourceLimit { .. } => "set_resource_limit",
            Operation::SetActivityCode { .. } => "set_activity_code",
            Operation::SetProjectCode { .. } => "set_project_code",
            Operation::AddCodeType { .. } => "add_code_type",
            Operation::AddCodeValue { .. } => "add_code_value",
            Operation::SetCodeValue { .. } => "set_code_value",
            Operation::DeleteCodeValue { .. } => "delete_code_value",
            Operation::SetNotebook { .. } => "set_notebook",
            Operation::SetWbsNotebook { .. } => "set_wbs_notebook",
            Operation::SetUdf { .. } => "set_udf",
            Operation::AddCalendar { .. } => "add_calendar",
            Operation::SetCalendar { .. } => "set_calendar",
            Operation::DeleteCalendar { .. } => "delete_calendar",
            Operation::AddResource { .. } => "add_resource",
            Operation::SetResource { .. } => "set_resource",
            Operation::SetWbs { .. } => "set_wbs",
            Operation::SetScheduleOption { .. } => "set_schedule_option",
            Operation::AddProjectFunding { .. } => "add_project_funding",
            Operation::SetProjectFunding { .. } => "set_project_funding",
            Operation::DeleteProjectFunding { .. } => "delete_project_funding",
            Operation::DeleteResource { .. } => "delete_resource",
            Operation::MergeResources { .. } => "merge_resources",
            Operation::SetWbsEarnedValue { .. } => "set_wbs_earned_value",
            Operation::AddDocument { .. } => "add_document",
            Operation::SetDocument { .. } => "set_document",
            Operation::DeleteDocument { .. } => "delete_document",
            Operation::LinkDocument { .. } => "link_document",
            Operation::UnlinkDocument { .. } => "unlink_document",
            Operation::AddExpense { .. } => "add_expense",
            Operation::SetExpense { .. } => "set_expense",
            Operation::DeleteExpense { .. } => "delete_expense",
            Operation::AddIssue { .. } => "add_issue",
            Operation::SetIssue { .. } => "set_issue",
            Operation::DeleteIssue { .. } => "delete_issue",
            Operation::AddRisk { .. } => "add_risk",
            Operation::SetRisk { .. } => "set_risk",
            Operation::DeleteRisk { .. } => "delete_risk",
            Operation::AssignRisk { .. } => "assign_risk",
            Operation::UnassignRisk { .. } => "unassign_risk",
            Operation::AddThreshold { .. } => "add_threshold",
            Operation::SetThreshold { .. } => "set_threshold",
            Operation::DeleteThreshold { .. } => "delete_threshold",
            Operation::MonitorThresholds { .. } => "monitor_thresholds",
            Operation::UpdateProgress { .. } => "update_progress",
            Operation::TopDownEstimate { .. } => "top_down_estimate",
            Operation::ShiftWbs { .. } => "shift_wbs",
            Operation::RenameWbsCode { .. } => "rename_wbs_code",
            Operation::RenameCalendar { .. } => "rename_calendar",
            Operation::SetDefaultCalendar { .. } => "set_default_calendar",
            Operation::ConvertCalendar { .. } => "convert_calendar",
            Operation::CopyCalendar { .. } => "copy_calendar",
            Operation::AddStep { .. } => "add_step",
            Operation::SetStep { .. } => "set_step",
            Operation::DeleteStep { .. } => "delete_step",
            Operation::MoveStep { .. } => "move_step",
            Operation::AddStepsFromTemplate { .. } => "add_steps_from_template",
            Operation::CreateStepTemplate { .. } => "create_step_template",
            Operation::SetFeedback { .. } => "set_feedback",
            Operation::ApplyActuals { .. } => "apply_actuals",
            Operation::RecalculateCosts { .. } => "recalculate_costs",
            Operation::DeleteFinancialPeriod { .. } => "delete_financial_period",
            Operation::GlobalChange { .. } => "global_change",
        }
    }
    /// Record edits written to the document as given.
    fn is_record(&self) -> bool {
        matches!(
            self,
            Operation::SetResourceLimit { .. }
                | Operation::SetActivityCode { .. }
                | Operation::SetProjectCode { .. }
                | Operation::AddCodeType { .. }
                | Operation::AddCodeValue { .. }
                | Operation::SetCodeValue { .. }
                | Operation::DeleteCodeValue { .. }
                | Operation::SetNotebook { .. }
                | Operation::SetWbsNotebook { .. }
                | Operation::SetUdf { .. }
                | Operation::AddCalendar { .. }
                | Operation::SetCalendar { .. }
                | Operation::DeleteCalendar { .. }
                | Operation::AddResource { .. }
                | Operation::SetResource { .. }
                | Operation::SetWbs { .. }
                | Operation::SetScheduleOption { .. }
                | Operation::AddProjectFunding { .. }
                | Operation::SetProjectFunding { .. }
                | Operation::DeleteProjectFunding { .. }
                | Operation::DeleteResource { .. }
                | Operation::MergeResources { .. }
                | Operation::SetWbsEarnedValue { .. }
                | Operation::AddDocument { .. }
                | Operation::SetDocument { .. }
                | Operation::DeleteDocument { .. }
                | Operation::LinkDocument { .. }
                | Operation::UnlinkDocument { .. }
                | Operation::AddExpense { .. }
                | Operation::SetExpense { .. }
                | Operation::DeleteExpense { .. }
                | Operation::AddIssue { .. }
                | Operation::SetIssue { .. }
                | Operation::DeleteIssue { .. }
                | Operation::AddRisk { .. }
                | Operation::SetRisk { .. }
                | Operation::DeleteRisk { .. }
                | Operation::AssignRisk { .. }
                | Operation::UnassignRisk { .. }
                | Operation::AddThreshold { .. }
                | Operation::SetThreshold { .. }
                | Operation::DeleteThreshold { .. }
                | Operation::RecalculateCosts { .. }
                | Operation::CreateStepTemplate { .. }
                | Operation::SetFeedback { .. }
                | Operation::DeleteFinancialPeriod { .. }
        ) || matches!(self, Operation::SetProject { field, .. } if !matches!(field.as_str(), "data_date" | "must_finish" | "must_finish_by"))
    }
    /// A short readable description for messages.
    fn describe(&self) -> String {
        match self {
            Operation::SetProject { field, .. } => format!("set_project {field}"),
            Operation::AddWbs { wbs, .. } => format!("add_wbs {wbs}"),
            Operation::AddActivity { activity, .. } => {
                format!(
                    "add_activity {}",
                    activity.as_deref().unwrap_or("(next ID)")
                )
            }
            Operation::DeleteActivity { activity } => format!("delete_activity {activity}"),
            Operation::DissolveActivity { activity, .. } => format!("dissolve_activity {activity}"),
            Operation::SetActivity {
                activity, field, ..
            } => format!("set_activity {activity} {field}"),
            Operation::AssignResource { activity, resource } => {
                format!("assign_resource {activity} {resource}")
            }
            Operation::RemoveAssignment { activity, resource } => {
                format!("remove_assignment {activity} {resource}")
            }
            Operation::SetAssignment {
                activity,
                resource,
                field,
                ..
            } => format!("set_assignment {activity} {resource} {field}"),
            Operation::Relationship {
                action,
                predecessor,
                successor,
                kind,
                ..
            } => format!("relationship {action} {predecessor} {kind} {successor}"),
            Operation::CalendarDay { calendar, date, .. } => {
                format!("calendar_day {calendar} {date}")
            }
            Operation::CalendarWeekday {
                calendar, weekday, ..
            } => format!("calendar_weekday {calendar} {weekday}"),
            Operation::Schedule { .. } => "schedule".into(),
            other => format!("{} {}", other.name(), other.subject()),
        }
    }
}

/// Parse a changes document.
pub fn parse_changes(value: Value) -> Result<Changes> {
    let changes: Changes = serde_json::from_value(value)
        .map_err(|e| AutomationError::new("invalid_changes", e.to_string()))?;
    if changes.format != FORMAT {
        return Err(AutomationError::new(
            "invalid_changes",
            format!("format must be {FORMAT:?}"),
        ));
    }
    if changes.operations.is_empty() || changes.operations.len() > MAX_OPERATIONS {
        return Err(AutomationError::new(
            "invalid_changes",
            format!("operations must hold 1 to {MAX_OPERATIONS} operations"),
        ));
    }
    Ok(changes)
}

/// The order operations run in: file order, except that a primary resource
/// set before the batch assigns that resource runs right after the
/// assignment (a primary resource must be assigned first).
fn order(operations: &[Operation]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..operations.len()).collect();
    for (k, op) in operations.iter().enumerate() {
        let Operation::SetActivity {
            activity,
            field,
            value,
            ..
        } = op
        else {
            continue;
        };
        let Some(name) = value.as_str().filter(|_| field == "primary_resource") else {
            continue;
        };
        let later = operations.iter().enumerate().skip(k + 1).find(|(_, o)| {
            matches!(o, Operation::AssignResource { activity: a, resource: r } if a == activity && r == name)
        });
        if let Some((j, _)) = later {
            order.retain(|&x| x != k);
            let at = order
                .iter()
                .position(|&x| x == j)
                .expect("assignment is ordered")
                + 1;
            order.insert(at, k);
        }
    }
    order
}

/// The activity an operation edits a field of, when it names one.
fn edited_activity(op: &Operation) -> Option<&str> {
    match op {
        Operation::SetActivity { activity, .. }
        | Operation::SetAssignment { activity, .. }
        | Operation::SetActivityCode { activity, .. }
        | Operation::SetNotebook { activity, .. }
        | Operation::AssignResource { activity, .. } => Some(activity),
        Operation::SetUdf { activity, .. } => activity.as_deref(),
        _ => None,
    }
}

/// Within a run of field edits, new activities and new assignments are
/// created at the start of the run, so the project is not re-read between
/// each one and its fields. An addition never moves past an operation on
/// the same activity, so every edit still sees what it saw in file order.
fn hoist(operations: &[Operation], order: Vec<usize>) -> Vec<usize> {
    let structural = |op: &Operation| {
        matches!(
            op,
            Operation::AddActivity { .. } | Operation::AssignResource { .. }
        )
    };
    let in_run = |op: &Operation| {
        structural(op)
            || op.is_record()
            || matches!(
                op,
                Operation::SetActivity { .. } | Operation::SetAssignment { .. }
            )
    };
    // An assignment reads the resource dictionary (by short name, priced from
    // its rates); a new activity reads the WBS paths.
    let depends = |op: &Operation, prev: &Operation| match op {
        Operation::AssignResource { .. } => matches!(
            prev,
            Operation::AddResource { .. }
                | Operation::SetResource { .. }
                | Operation::DeleteResource { .. }
                | Operation::MergeResources { .. }
                | Operation::SetResourceLimit { .. }
        ),
        // ... and the project's defaults for new activities.
        Operation::AddActivity { .. } => {
            matches!(
                prev,
                Operation::SetWbs { .. } | Operation::SetProject { .. }
            )
        }
        _ => false,
    };
    let mut out: Vec<usize> = Vec::with_capacity(order.len());
    let mut run_start = 0;
    for n in order {
        let op = &operations[n];
        if !in_run(op) {
            out.push(n);
            run_start = out.len();
            continue;
        }
        if !structural(op) {
            out.push(n);
            continue;
        }
        // A new activity numbered from the defaults takes the IDs in use
        // where it stands, so it stays there and nothing moves past it.
        if matches!(op, Operation::AddActivity { activity: None, .. }) {
            out.push(n);
            run_start = out.len();
            continue;
        }
        let name = match op {
            Operation::AddActivity { activity, .. } => activity.as_deref().unwrap_or(""),
            Operation::AssignResource { activity, .. } => activity.as_str(),
            _ => unreachable!(),
        };
        // Move up past field edits of other activities, behind the run's
        // earlier additions and any edit of what the addition names (its
        // resource, or the WBS element it goes under).
        let mut at = out.len();
        while at > run_start {
            let prev = &operations[out[at - 1]];
            if structural(prev) || edited_activity(prev) == Some(name) || depends(op, prev) {
                break;
            }
            at -= 1;
        }
        out.insert(at, n);
    }
    out
}

/// Apply `operations` to `project` in order, all or nothing. `recalculate`
/// is the batch default. Returns the report (operation results and the
/// field differences). On `Err` `project` is unchanged.
pub fn apply(
    project: &mut ProjectData,
    operations: &[Operation],
    recalculate: bool,
) -> Result<Value> {
    apply_with(project, operations, recalculate, &Default::default())
}

/// Apply an `openxer-changes/1` document (as `openxer_apply` takes it) to a
/// project held in memory, such as the one open in the app: the same
/// parsing and the same setters as the file-based apply, all or nothing.
/// `recalculate` overrides the document's own default (true when neither
/// says). `ev` holds the stored baselines `monitor_thresholds` reads
/// (`Default` when the caller has none). Returns the same report the
/// apply command gives (operation results and the field diff); on `Err`
/// `project` is unchanged.
pub fn apply_changes(
    project: &mut ProjectData,
    changes: Value,
    recalculate: Option<bool>,
    ev: &crate::baselines::EvContext,
) -> Result<Value> {
    let changes = parse_changes(changes)?;
    let recalculate = recalculate.or(changes.recalculate).unwrap_or(true);
    apply_with(project, &changes.operations, recalculate, ev)
}

/// [`apply`] with the stored baselines earned value reads (`monitor_thresholds`).
pub fn apply_with(
    project: &mut ProjectData,
    operations: &[Operation],
    recalculate: bool,
    ev: &crate::baselines::EvContext,
) -> Result<Value> {
    project
        .model
        .validate()
        .map_err(|e| AutomationError::new("invalid_source", e))?;
    let before = project.updated_document();
    let mut next = project.clone();
    let mut state = State {
        raw: false,
        ev: ev.clone(),
        schedules: vec![],
        comments: vec![],
    };
    let mut results = vec![];
    let mut warnings = vec![];
    for n in hoist(operations, order(operations)) {
        let op = &operations[n];
        let fail = |e: String| {
            // A milestone refuses assignments everywhere; agents get a code.
            let code = if e.contains(openxer_xer::assignments::MILESTONE_RULE) {
                "milestone_assignment"
            } else {
                "operation_failed"
            };
            AutomationError::new(
                code,
                format!("Operation {} ({}): {e}", n + 1, op.describe()),
            )
        };
        let result = apply_one(&mut next, op, recalculate, &mut state).map_err(fail)?;
        // An operation that wrote nothing says why; the report lists it so
        // a batch that succeeds is not taken for one that changed things.
        if let Some(why) = result.get("reason").and_then(Value::as_str) {
            warnings.push(json!(format!(
                "Operation {} ({}) wrote nothing: {why}",
                n + 1,
                op.describe()
            )));
        }
        results.push(result);
    }
    if !state.comments.is_empty() {
        state.begin_raw(&mut next);
    }
    state.settle(&mut next).map_err(|e| {
        AutomationError::new("operation_failed", format!("Re-reading exact values: {e}"))
    })?;
    next.model
        .validate()
        .map_err(|e| AutomationError::new("operation_failed", e))?;
    let after = next.updated_document();
    let diff = diff(&before, &after, next.model.project.proj_id);
    *project = next;
    let mut report = json!({"operations":operations.len(),"results":results,"diff":diff});
    if !state.schedules.is_empty() {
        report["schedule"] = Value::Array(state.schedules);
    }
    if !warnings.is_empty() {
        report["warnings"] = Value::Array(warnings);
    }
    Ok(report)
}

struct State {
    /// Exact writes went to the document; the project must be re-read.
    raw: bool,
    /// The baselines earned value reads (Monitor Thresholds).
    ev: crate::baselines::EvContext,
    schedules: Vec<Value>,
    /// Relationship comments waiting for their rows: (predecessor task id,
    /// successor task id, PR_ type, text).
    comments: Vec<(i64, i64, String, String)>,
}

impl State {
    /// Before an exact write: put the model into the document.
    fn begin_raw(&mut self, p: &mut ProjectData) {
        if !self.raw {
            p.document = p.updated_document();
            self.raw = true;
        }
        for (a, b, kind, text) in std::mem::take(&mut self.comments) {
            // A relationship removed again later in the batch has no row.
            let _ = tables::set_relationship_comments(p, a, b, &kind, &text);
        }
    }
    /// After exact writes: re-read the project from its document.
    fn settle(&mut self, p: &mut ProjectData) -> std::result::Result<(), String> {
        if !self.raw {
            return Ok(());
        }
        self.begin_raw(p);
        self.raw = false;
        let warnings = p.warnings.clone();
        let mut fresh = ProjectData::from_document(
            p.document.clone(),
            p.model.project.proj_id,
            p.source_name.clone(),
        )?;
        fresh.warnings = warnings;
        *p = fresh;
        Ok(())
    }
}

fn apply_one(
    p: &mut ProjectData,
    op: &Operation,
    batch_recalculate: bool,
    state: &mut State,
) -> std::result::Result<Value, String> {
    let exact = match op {
        Operation::SetActivity { recalculate, .. }
        | Operation::SetAssignment { recalculate, .. } => !recalculate.unwrap_or(batch_recalculate),
        other => other.is_record(),
    };
    if exact {
        state.begin_raw(p);
    } else {
        state.settle(p)?;
    }
    if op.is_record() {
        return record(p, op);
    }
    match op {
        Operation::SetProject {
            field,
            value,
            schedule,
        } => {
            let origin = p.model.origin_serial;
            match field.as_str() {
                "data_date" => {
                    let d = date(origin, value)?.ok_or("A data date is required.")?;
                    // The same rule as Project Details, Dates tab: nothing
                    // is scheduled unless `schedule` asks for it.
                    openxer_xer::edit::project_dates::set_data_date(p, Some(d))?;
                    if schedule.unwrap_or(false) {
                        let report = schedule_project(p, &SchedulePatch::default())
                            .map_err(|e| e.message)?;
                        state.schedules.push(json!({
                            "at": "data_date",
                            "changed_activity_count": report["changed_activity_count"],
                        }));
                    }
                }
                "must_finish" | "must_finish_by" => p.model.project.plan_end = date(origin, value)?,
                _ => {
                    return Err(format!(
                        "Unknown project field {field:?}; use data_date or must_finish."
                    ))
                }
            }
            Ok(json!({"op":"set_project","field":field}))
        }
        Operation::AddWbs { wbs: path, name } => {
            let (id, added) = wbs::ensure_path(p, path, name.as_deref())?;
            Ok(json!({"op":"add_wbs","wbs":path,"wbs_id":id,"added":added}))
        }
        Operation::AddActivity {
            activity,
            selected,
            name,
            wbs: path,
        } => {
            if let Some(activity) = activity {
                if find_activity(p, activity).is_ok() {
                    return Err(format!("Activity {activity} already exists."));
                }
            }
            let selected = selected
                .as_deref()
                .map(|code| find_activity(p, code))
                .transpose()?;
            let selected_wbs =
                selected.and_then(|i| p.details.get(&p.model.activities[i].task_id).map(|d| d.wbs));
            let w = match path {
                Some(path) => {
                    wbs::find_path(p, path).ok_or_else(|| format!("No WBS element {path:?}."))?
                }
                None => selected_wbs
                    .or_else(|| wbs::project_node(p))
                    .or_else(|| p.wbs.first().map(|w| w.id))
                    .ok_or("The project has no WBS element for a new activity.")?,
            };
            let i = openxer_xer::edit::new_activity::add_activity(p, w, selected);
            if let Some(activity) = activity {
                act::set_activity_id(p, i, activity)?;
            }
            if let Some(name) = name {
                act::set_activity_name(p, i, name)?;
            }
            let a = &p.model.activities[i];
            Ok(json!({"op":"add_activity","activity":a.task_code,"task_id":a.task_id}))
        }
        Operation::DeleteActivity { activity } => {
            let i = find_activity(p, activity)?;
            let task_id = p.model.activities[i].task_id;
            p.delete_activities(&[task_id])?;
            Ok(json!({"op":"delete_activity","activity":activity}))
        }
        Operation::DissolveActivity {
            activity,
            retain_lag,
        } => {
            let i = find_activity(p, activity)?;
            let task_id = p.model.activities[i].task_id;
            let retain_lag = retain_lag.unwrap_or(false);
            let done = openxer_xer::edit::dissolve::dissolve(p, &[task_id], retain_lag)?;
            Ok(
                json!({"op":"dissolve_activity","activity":activity,"retain_lag":retain_lag,
                "relationships_added":done.added,"relationships_kept":done.kept}),
            )
        }
        Operation::SetActivity {
            activity,
            field,
            value,
            ..
        } => {
            let i = find_activity(p, activity)?;
            if exact {
                state.begin_raw(p);
                let column = set_task_exact(p, i, field, value)?;
                return Ok(
                    json!({"op":"set_activity","activity":activity,"field":field,"exact":column}),
                );
            }
            let changed = set_activity(p, i, field, value)?;
            Ok(
                json!({"op":"set_activity","activity":activity,"field":field,"changed":changed.0,"warning":changed.1}),
            )
        }
        Operation::AssignResource { activity, resource } => {
            let i = find_activity(p, activity)?;
            let task_id = p.model.activities[i].task_id;
            let rsrc = find_resource(p, resource)?;
            if assignment_row(p, task_id, rsrc).is_ok() {
                return Err(format!("{resource} is already assigned to {activity}."));
            }
            p.add_assignment_with(task_id, Some(rsrc), None, false)?;
            Ok(json!({"op":"assign_resource","activity":activity,"resource":resource}))
        }
        Operation::RemoveAssignment { activity, resource } => {
            let i = find_activity(p, activity)?;
            let task_id = p.model.activities[i].task_id;
            let row = assignment_row(p, task_id, find_resource(p, resource)?)?;
            p.remove_assignment_with(row, false)?;
            Ok(json!({"op":"remove_assignment","activity":activity,"resource":resource}))
        }
        Operation::SetAssignment {
            activity,
            resource,
            field,
            value,
            ..
        } => {
            let i = find_activity(p, activity)?;
            let task_id = p.model.activities[i].task_id;
            let rsrc = find_resource(p, resource)?;
            // The curve is a document value in both modes: named by curve
            // name (a preset is copied into the file when first used).
            if field == "curve" {
                state.begin_raw(p);
                let row = assignment_row(p, task_id, rsrc)?;
                let name = text(value);
                let changed = curves::set_assignment_curve(&mut p.document, row, name.as_deref())?;
                // A curve is accepted on any activity (as the desktop picker
                // does) but shapes the spread only on the Fixed Duration
                // types; the result says whether it does here.
                let pid = p.model.project.proj_id;
                let kind = openxer_xer::edit::records::task_value(
                    &p.document,
                    task_id,
                    pid,
                    "duration_type",
                );
                let applies = openxer_xer::spread::curve_applies(&kind);
                let mut out = json!({"op":"set_assignment","activity":activity,"resource":resource,"field":field,"curve":name,"changed":changed,"curv_id":curves::of_assignment(&p.document, row),"curve_applies":applies});
                if !applies && name.is_some() {
                    out["note"] = json!("This activity's duration type ignores resource curves, so its units spread evenly. Curves shape the spread only on Fixed Duration and Units/Time (DT_FixedDrtn) and Fixed Duration and Units (DT_FixedDUR2) activities.");
                }
                return Ok(out);
            }
            // Typed period units, as in the usage spreadsheet: the same
            // edit in both modes.
            if field == "period_remaining_units" {
                let row = assignment_row(p, task_id, rsrc)?;
                let (period, units) = period_value(p, value)?;
                p.set_period_units(row, period, units)?;
                let runs = records::get(&p.document, "TASKRSRC", row, "remain_crv");
                return Ok(
                    json!({"op":"set_assignment","activity":activity,"resource":resource,"field":field,"remain_crv":runs,"remaining_units":records::get(&p.document, "TASKRSRC", row, "remain_qty")}),
                );
            }
            if exact {
                state.begin_raw(p);
                let row = assignment_row(p, task_id, rsrc)?;
                let column = set_assignment_exact(p, row, field, value)?;
                return Ok(
                    json!({"op":"set_assignment","activity":activity,"resource":resource,"field":field,"exact":column}),
                );
            }
            let row = assignment_row(p, task_id, rsrc)?;
            // A value the assignment already holds is no change: nothing is
            // recalculated (an equal price keeps its source) and a locked
            // field accepts it.
            if !assignment_holds(p, row, field, value) {
                p.edit_assignment(row, assignment_edit_in(p, field, value)?)?;
            }
            Ok(json!({"op":"set_assignment","activity":activity,"resource":resource,"field":field}))
        }
        Operation::Relationship {
            action,
            predecessor,
            successor,
            kind,
            lag_hours,
            comments,
        } => {
            let pred = find_activity(p, predecessor)?;
            let succ = find_activity(p, successor)?;
            let kind = link_type(kind)?;
            if lag_hours.is_some_and(|l| !l.is_finite() || !(-100_000.0..=100_000.0).contains(&l)) {
                return Err("lag_hours must be from -100000 to 100000.".into());
            }
            let index = p
                .model
                .links
                .iter()
                .position(|l| l.pred == pred && l.succ == succ && l.link_type == kind);
            match action.as_str() {
                // Replaying stored data keeps a relationship that closes a
                // loop; the schedule decides.
                "add" if !batch_recalculate => relationships::add_keeping_loops(
                    &mut p.model,
                    pred,
                    succ,
                    kind,
                    (lag_hours.unwrap_or(0.) * 60.).round() as i64,
                )?,
                "add" => relationships::add(
                    &mut p.model,
                    pred,
                    succ,
                    kind,
                    (lag_hours.unwrap_or(0.) * 60.).round() as i64,
                )?,
                "edit" | "update" => {
                    let index = index.ok_or("Relationship not found.")?;
                    let lag = lag_hours.unwrap_or(p.model.links[index].lag as f64 / 60.);
                    relationships::edit(&mut p.model, index, kind, lag)?
                }
                "remove" | "delete" => {
                    relationships::remove(&mut p.model, index.ok_or("Relationship not found.")?)?
                }
                other => {
                    return Err(format!(
                        "Unknown relationship action {other:?}; use add, edit or remove."
                    ))
                }
            }
            if let (Some(text), false) = (comments, matches!(action.as_str(), "remove" | "delete"))
            {
                let (a, b) = (
                    p.model.activities[pred].task_id,
                    p.model.activities[succ].task_id,
                );
                state
                    .comments
                    .push((a, b, kind.as_str().to_string(), text.clone()));
            }
            Ok(json!({"op":"relationship","action":action}))
        }
        Operation::CalendarDay {
            calendar,
            date: day,
            shifts,
        } => {
            let id = calendar_id(p, calendar)?;
            let relative = parse_relative(p.model.origin_serial, day)?
                .ok_or("A calendar date is required.")?;
            let serial = p.model.origin_serial + relative.div_euclid(openxer_engine::calendar::DAY);
            let mut data = calendars::data(&p.document, &id)?;
            match shifts {
                None => data.set_standard(serial),
                Some(list) => data.set_day(serial, parse_shifts(list)?),
            }
            let hours = calendars::hours(&p.document, &id);
            let changed = calendars::commit(p, &id, &data, hours, &calendars::now_stamp())?;
            Ok(json!({"op":"calendar_day","calendar":calendar,"date":day,"changed":changed}))
        }
        Operation::CalendarWeekday {
            calendar,
            weekday,
            shifts,
        } => {
            if *weekday > 6 {
                return Err("weekday must be 0 (Sunday) through 6 (Saturday).".into());
            }
            let id = calendar_id(p, calendar)?;
            let mut data = calendars::data(&p.document, &id)?;
            data.week[*weekday] = parse_shifts(shifts)?;
            let hours = calendars::hours(&p.document, &id);
            let changed = calendars::commit(p, &id, &data, hours, &calendars::now_stamp())?;
            Ok(
                json!({"op":"calendar_weekday","calendar":calendar,"weekday":weekday,"changed":changed}),
            )
        }
        Operation::AddStep { activity, name, at } => {
            let t = p.model.activities[find_activity(p, activity)?].task_id;
            let n = steps::add_step(p, t, name, *at);
            Ok(json!({"op":"add_step","activity":activity,"step":n}))
        }
        Operation::SetStep {
            activity,
            step,
            field,
            value,
        } => {
            let t = p.model.activities[find_activity(p, activity)?].task_id;
            let r = steps::find_step(p, t, &text(step).unwrap_or_default())?;
            steps::set_step(p, t, r, field, &text(value).unwrap_or_default())?;
            Ok(json!({"op":"set_step","activity":activity,"step":step,"field":field}))
        }
        Operation::DeleteStep { activity, step } => {
            let t = p.model.activities[find_activity(p, activity)?].task_id;
            let r = steps::find_step(p, t, &text(step).unwrap_or_default())?;
            steps::delete_step(p, t, r);
            Ok(json!({"op":"delete_step","activity":activity,"step":step}))
        }
        Operation::MoveStep { activity, step, to } => {
            let t = p.model.activities[find_activity(p, activity)?].task_id;
            let r = steps::find_step(p, t, &text(step).unwrap_or_default())?;
            if *to == 0 {
                return Err("to is a step position from 1.".into());
            }
            steps::move_step(p, t, r, *to);
            Ok(json!({"op":"move_step","activity":activity,"step":step,"to":to}))
        }
        Operation::AddStepsFromTemplate {
            activity,
            template,
            after,
        } => {
            let t = p.model.activities[find_activity(p, activity)?].task_id;
            let list = steps::templates(&p.document);
            let found: Vec<i64> = list
                .iter()
                .filter(|x| {
                    x.name.eq_ignore_ascii_case(template.trim())
                        || x.id.to_string() == template.trim()
                })
                .map(|x| x.id)
                .collect();
            let group = match found.as_slice() {
                [g] => *g,
                [] => return Err(format!("No step template {template:?}.")),
                _ => return Err(format!("Step template {template:?} is not unique.")),
            };
            let pid = p.model.project.proj_id;
            let after = match after.as_ref().and_then(text) {
                Some(step) => Some(steps::step_id(&p.document, steps::find_step(p, t, &step)?)),
                None => None,
            };
            let added = steps::apply_template_after(&mut p.document, group, t, pid, after);
            steps::after_steps_change(p, t);
            Ok(json!({"op":"add_steps_from_template","activity":activity,"added":added}))
        }
        Operation::TopDownEstimate {
            labor_units,
            wbs: root,
            not_started_only,
            weights,
        } => {
            let root = match root {
                Some(path) => Some(
                    wbs::find_path(p, path).ok_or_else(|| format!("No WBS element {path:?}."))?,
                ),
                None => None,
            };
            let mut by_id = std::collections::HashMap::new();
            for (path, w) in weights.iter().flatten() {
                if !w.is_finite() || *w < 0. {
                    return Err(format!("The weight of {path:?} must be 0 or more."));
                }
                let id =
                    wbs::find_path(p, path).ok_or_else(|| format!("No WBS element {path:?}."))?;
                by_id.insert(id, *w);
            }
            if !labor_units.is_finite() || *labor_units <= 0. {
                return Err("labor_units must be more than 0.".into());
            }
            let lines = p.top_down_distribution(
                *labor_units,
                root,
                not_started_only.unwrap_or(true),
                &by_id,
            );
            if lines.is_empty() {
                return Err("No activities can take labor units here: only not started Task Dependent and Resource Dependent activities receive them.".into());
            }
            for (&id, &w) in &by_id {
                p.set_wbs_estimation_weight(id, w);
            }
            let changed = p.apply_labor_estimate(&lines);
            Ok(json!({"op":"top_down_estimate","activities":lines.len(),"changed":changed}))
        }
        Operation::ShiftWbs {
            wbs: path,
            direction,
        } => {
            let id = wbs::find_path(p, path).ok_or_else(|| format!("No WBS element {path:?}."))?;
            match direction.trim().to_ascii_lowercase().as_str() {
                "up" => wbs::shift_up(p, id)?,
                "down" => wbs::shift_down(p, id)?,
                "left" => wbs::shift_left(p, id)?,
                "right" => wbs::shift_right(p, id)?,
                other => {
                    return Err(format!(
                        "direction is up, down, left or right, not {other:?}."
                    ))
                }
            }
            Ok(json!({"op":"shift_wbs","wbs_id":id,"path":project_records::wbs_code_path(p, id)}))
        }
        Operation::RenameWbsCode { wbs: path, code } => {
            let id = wbs::find_path(p, path).ok_or_else(|| format!("No WBS element {path:?}."))?;
            wbs::rename_code(p, id, code)?;
            Ok(
                json!({"op":"rename_wbs_code","wbs_id":id,"path":project_records::wbs_code_path(p, id)}),
            )
        }
        Operation::RenameCalendar { calendar, name } => {
            let id = calendar_id(p, calendar)?;
            calendars::rename(p, &id, name)?;
            Ok(json!({"op":"rename_calendar","clndr_id":id}))
        }
        Operation::SetDefaultCalendar { calendar } => {
            let id = calendar_id(p, calendar)?;
            calendars::set_default(p, &id)?;
            Ok(json!({"op":"set_default_calendar","clndr_id":id}))
        }
        Operation::ConvertCalendar { calendar, to } => {
            let id = calendar_id(p, calendar)?;
            let to = match to.trim().to_ascii_lowercase().as_str() {
                "global" => calendars::Convert::ToGlobal,
                "shared" => calendars::Convert::Shared,
                "personal" => calendars::Convert::Personal,
                other => return Err(format!("to is global, shared or personal, not {other:?}.")),
            };
            calendars::convert(p, &id, to, &calendars::now_stamp())?;
            Ok(json!({"op":"convert_calendar","clndr_id":id}))
        }
        Operation::CopyCalendar {
            calendar,
            source,
            kind,
        } => {
            let kind = match kind
                .as_deref()
                .unwrap_or("project")
                .trim()
                .to_ascii_lowercase()
                .as_str()
            {
                "project" => calendars::Kind::Project,
                "global" => calendars::Kind::Global,
                "resource" => calendars::Kind::Resource,
                other => {
                    return Err(format!(
                        "kind is project, global or resource, not {other:?}."
                    ))
                }
            };
            let source = match source {
                Some(name) => Some(calendar_id(p, name)?),
                None => None,
            };
            let id = calendars::add(p, kind, source.as_deref(), &calendars::now_stamp())?;
            calendars::rename(p, &id, calendar)?;
            Ok(json!({"op":"copy_calendar","clndr_id":id}))
        }
        Operation::UpdateProgress {
            data_date,
            activities,
            remaining,
        } => progress_op(
            p,
            op,
            data_date,
            activities.as_deref(),
            remaining.as_deref(),
        ),
        Operation::ApplyActuals {
            data_date,
            remaining,
        } => progress_op(p, op, data_date, None, remaining.as_deref()),
        Operation::MonitorThresholds { from, to, today } => {
            // Read the model as the batch left it, then write issues to
            // the document like the other record edits.
            state.begin_raw(p);
            let origin = p.model.origin_serial;
            let bound = |v: &Option<String>| -> std::result::Result<Option<i64>, String> {
                v.as_deref().map_or(Ok(None), |t| parse_relative(origin, t))
            };
            let window = if from.is_some() || to.is_some() {
                Some((bound(from)?, bound(to)?))
            } else {
                None
            };
            let today = match today {
                Some(t) => {
                    let day = openxer_engine::calendar::DAY;
                    let at = parse_relative(origin, t)?.ok_or("today needs a date.")?;
                    openxer_xer::date_string(origin, Some(at.div_euclid(day) * day))
                }
                None => project_records::today(),
            };
            let (baseline, figures) = {
                let calc = state.ev.calculator(p);
                thresholds::inputs(p, &calc)
            };
            let result = thresholds::monitor(p, &baseline, &figures, window, &today, &|p, id| {
                project_records::wbs_code_path(p, id)
            });
            Ok(
                json!({"op":"monitor_thresholds","thresholds":result.thresholds,
                "issues_raised":result.created,"issues_updated":result.updated,
                "skipped":result.skipped,"baselines":state.ev.describe()}),
            )
        }
        Operation::Schedule { level_resources } => {
            let patch = SchedulePatch {
                level_resources: *level_resources,
                ..SchedulePatch::default()
            };
            let report = schedule_project(p, &patch).map_err(|e| e.message)?;
            let level_resources = p.model.options.do_level;
            state.schedules.push(json!({
                "changed_activity_count": report["changed_activity_count"],
                "generated_field_count": report["generated_field_count"],
                "changed_field_count": report["changed_field_count"],
            }));
            Ok(json!({"op":"schedule","level_resources":level_resources}))
        }
        Operation::GlobalChange { name, rule } => global_change(p, name.as_deref(), rule.as_ref()),
        other => record(p, other),
    }
}

/// Tools > Global Change on every activity: work out the report's lines on
/// a copy (the shared run the app's report shows), then write the lines
/// that carry no error. Durations show in hours and dates as
/// `YYYY-MM-DD HH:MM`.
fn global_change(
    p: &mut ProjectData,
    name: Option<&str>,
    rule: Option<&Value>,
) -> std::result::Result<Value, String> {
    use openxer_xer::edit::global_change::{self as gc, ProjectHost};
    let rule = rule.cloned().ok_or_else(|| match name {
        Some(name) => format!(
            "No saved global change named {name:?}; saved global changes are read from an \
             OpenXER workspace input, or give the definition as rule."
        ),
        None => "global_change needs name or rule.".to_string(),
    })?;
    let mut g = gc::from_value(rule)?;
    if let Some(name) = name {
        g.name = name.to_string();
    }
    let everything = vec![true; p.model.activities.len()];
    let lines = {
        let mut scratch = p.clone();
        let mut host = ProjectHost::new(&mut scratch);
        gc::preview(&mut host, &g, &everything)
    };
    let (changed, failed) = {
        let mut host = ProjectHost::new(p);
        gc::commit(&mut host, &lines)
    };
    let rows: Vec<Value> = lines
        .iter()
        .map(|c| {
            json!({"project":c.project,"wbs":c.wbs,"activity":c.activity,"field":c.title,
                "field_id":c.column,"old":c.old,"new":c.new,"exception":c.error})
        })
        .collect();
    let refused = lines.iter().filter(|c| c.error.is_some()).count();
    Ok(json!({"op":"global_change","name":g.name,"rows":rows,
        "changed":changed,"refused":refused + failed}))
}

/// Update Progress or Apply Actuals: model edits, like the data date.
fn progress_op(
    p: &mut ProjectData,
    op: &Operation,
    data_date: &str,
    activities: Option<&[String]>,
    remaining: Option<&str>,
) -> std::result::Result<Value, String> {
    let rule = match remaining.unwrap_or("always") {
        "always" => progress::RemainingRule::Always,
        "duration_type" => progress::RemainingRule::DurationType,
        other => {
            return Err(format!(
                "remaining is always or duration_type, not {other:?}."
            ))
        }
    };
    let origin = p.model.origin_serial;
    let dd = parse_relative(origin, data_date)?.ok_or("A new data date is required.")?;
    let changed = if matches!(op, Operation::ApplyActuals { .. }) {
        progress::apply_actuals(p, dd, rule)?
    } else {
        match activities {
            Some(list) => {
                let set = list
                    .iter()
                    .map(|a| find_activity(p, a))
                    .collect::<std::result::Result<std::collections::BTreeSet<usize>, String>>()?;
                progress::apply_progress(p, dd, progress::ProgressScope::Selected(&set), rule)?
            }
            None => progress::apply_progress(p, dd, progress::ProgressScope::All, rule)?,
        }
    };
    Ok(json!({"op":op.name(),"data_date":data_date,"changed_activities":changed}))
}

// ------------------------------------------------------------ identifiers

/// What a document link names: exactly one of an activity or a WBS path.
fn document_target(
    p: &ProjectData,
    activity: &Option<String>,
    wbs: &Option<String>,
) -> std::result::Result<(Option<i64>, Option<i64>), String> {
    match (activity, wbs) {
        (Some(a), None) => {
            let i = find_activity(p, a)?;
            Ok((Some(p.model.activities[i].task_id), None))
        }
        (None, Some(w)) => {
            let id = wbs::find_path(p, w).ok_or_else(|| format!("No WBS element {w:?}."))?;
            Ok((None, Some(id)))
        }
        _ => Err("Name exactly one of activity or wbs.".into()),
    }
}

fn find_activity(p: &ProjectData, code: &str) -> std::result::Result<usize, String> {
    let found: Vec<usize> = p
        .model
        .activities
        .iter()
        .enumerate()
        .filter(|(_, a)| a.task_code == code)
        .map(|(i, _)| i)
        .collect();
    match found.as_slice() {
        [i] => Ok(*i),
        [] => Err(format!("No activity {code:?}.")),
        _ => Err(format!("Activity ID {code:?} is not unique.")),
    }
}

fn find_resource(p: &ProjectData, name: &str) -> std::result::Result<i64, String> {
    let t = p
        .document
        .table("RSRC")
        .ok_or("The project has no resources.")?;
    let ids = |exact: bool| -> Vec<i64> {
        t.rows
            .iter()
            .filter(|r| {
                let short = t.value(r, "rsrc_short_name");
                if exact {
                    short == name
                } else {
                    short.eq_ignore_ascii_case(name.trim())
                }
            })
            .map(|r| records::num(t.value(r, "rsrc_id")))
            .collect()
    };
    let mut found = ids(true);
    if found.is_empty() {
        found = ids(false);
    }
    match found.as_slice() {
        [id] => Ok(*id),
        [] => Err(format!("No resource with short name {name:?}.")),
        _ => Err(format!("Resource short name {name:?} is not unique.")),
    }
}

fn assignment_row(p: &ProjectData, task_id: i64, rsrc: i64) -> std::result::Result<usize, String> {
    let pid = p.model.project.proj_id;
    let rows: Vec<usize> = records::rows_for(&p.document, "TASKRSRC", task_id, pid)
        .into_iter()
        .filter(|&r| records::num(&records::get(&p.document, "TASKRSRC", r, "rsrc_id")) == rsrc)
        .collect();
    match rows.as_slice() {
        [r] => Ok(*r),
        [] => Err("The resource is not assigned to the activity.".into()),
        _ => Err("The resource is assigned to the activity more than once.".into()),
    }
}

/// The CALENDAR row id for a calendar name (" [project]", " [global]" or
/// " [resource]" tell two of the same name apart).
fn calendar_id(p: &ProjectData, name: &str) -> std::result::Result<String, String> {
    tables::calendar_id(p, name)
}

/// The model calendar an activity can be assigned, by name.
fn calendar_index(p: &ProjectData, i: usize, name: &str) -> std::result::Result<usize, String> {
    let id = calendar_id(p, name)?;
    let current = p.model.activities[i].cal;
    calendars::assignable(p, current)
        .into_iter()
        .find(|&c| p.model.calendars[c].id == id)
        .ok_or_else(|| format!("Calendar {name:?} cannot be assigned in this project."))
}

fn link_type(kind: &str) -> std::result::Result<LinkType, String> {
    Ok(
        match kind.trim().to_ascii_uppercase().trim_start_matches("PR_") {
            "FS" => LinkType::FS,
            "SS" => LinkType::SS,
            "FF" => LinkType::FF,
            "SF" => LinkType::SF,
            _ => {
                return Err(format!(
                    "Unknown relationship type {kind:?}; use FS, SS, FF or SF."
                ))
            }
        },
    )
}

fn parse_time(text: &str) -> std::result::Result<i64, String> {
    let (h, m) = text
        .trim()
        .split_once(':')
        .ok_or_else(|| format!("Shift time {text:?} must be HH:MM."))?;
    let (h, m): (i64, i64) = (
        h.parse()
            .map_err(|_| format!("Shift time {text:?} must be HH:MM."))?,
        m.parse()
            .map_err(|_| format!("Shift time {text:?} must be HH:MM."))?,
    );
    if !(0..=24).contains(&h) || !(0..60).contains(&m) || (h == 24 && m != 0) {
        return Err(format!("Shift time {text:?} is not a time of day."));
    }
    Ok(h * 60 + m)
}

/// Shifts as `[["08:00","12:00"],...]`: ordered, not overlapping; a finish
/// at or before its start (`00:00`, or `07:00` after `19:00`) runs past
/// midnight.
fn parse_shifts(list: &[[String; 2]]) -> std::result::Result<Vec<Shift>, String> {
    if list.len() > 10 {
        return Err("At most ten shifts per day.".into());
    }
    let mut out = vec![];
    let mut previous = 0;
    for [start, finish] in list {
        let s = parse_time(start)?;
        let mut f = parse_time(finish)?;
        // A finish at or before the start runs past midnight.
        if f <= s {
            f += 1440;
        }
        if s < previous || s >= 1440 || f - s > 1440 {
            return Err("Shifts must be ordered, non-overlapping periods within the day.".into());
        }
        previous = f;
        out.push((s, f));
    }
    Ok(out)
}

// ----------------------------------------------------------------- values

fn text(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(s) => Some(s.clone()),
        other => Some(other.to_string()),
    }
}

fn number(value: &Value, what: &str) -> std::result::Result<f64, String> {
    let n = match value {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse::<f64>().ok(),
        _ => None,
    };
    n.filter(|n| n.is_finite())
        .ok_or_else(|| format!("{what} must be a number."))
}

fn hours(value: &Value, what: &str) -> std::result::Result<i64, String> {
    Ok((number(value, what)? * 60.).round() as i64)
}

fn date(origin: i64, value: &Value) -> std::result::Result<Option<i64>, String> {
    match text(value) {
        None => Ok(None),
        Some(s) => parse_relative(origin, &s),
    }
}

fn needed(origin: i64, value: &Value, what: &str) -> std::result::Result<i64, String> {
    date(origin, value)?.ok_or_else(|| format!("{what} needs a date."))
}

/// The activity fields `set_activity` takes, by OpenXER column key.
pub const ACTIVITY_FIELDS: &[&str] = &[
    "activity_id",
    "activity_name",
    "wbs",
    "calendar",
    "activity_type",
    "activity_status",
    "duration_type",
    "percent_complete_type",
    "activity_priority",
    "primary_resource",
    "original_duration",
    "remaining_duration",
    "actual_duration",
    "at_completion_duration",
    "actual_start",
    "actual_finish",
    "start",
    "start_without_constraint",
    "finish",
    "planned_start",
    "planned_finish",
    "expected_finish",
    "suspend_date",
    "resume_date",
    "physical_percent_complete",
    "duration_percent_complete",
    "activity_percent_complete",
    "primary_constraint",
    "primary_constraint_date",
    "secondary_constraint",
    "secondary_constraint_date",
];

/// One activity field through the shared setters. Returns whether it
/// changed and any follow-up warning.
fn set_activity(
    p: &mut ProjectData,
    i: usize,
    field: &str,
    value: &Value,
) -> std::result::Result<(bool, Option<String>), String> {
    let origin = p.model.origin_serial;
    let choice = |what: &str| -> std::result::Result<String, String> {
        text(value).ok_or_else(|| format!("{what} needs a value."))
    };
    let edit = match field {
        "activity_id" => return Ok((act::set_activity_id(p, i, &choice("Activity ID")?)?, None)),
        "activity_name" => {
            return Ok((
                act::set_activity_name(p, i, &records::clean(&text(value).unwrap_or_default()))?,
                None,
            ))
        }
        "wbs" => {
            let path = choice("WBS")?;
            Edit::Wbs(wbs::find_path(p, &path).ok_or_else(|| format!("No WBS element {path:?}."))?)
        }
        "calendar" => Edit::Calendar(calendar_index(p, i, &choice("Calendar")?)?),
        "activity_type" => {
            let t = choice("Activity Type")?;
            Edit::ActivityType(
                act::parse_task_type(&t)
                    .ok_or_else(|| format!("{t:?} is not an activity type."))?,
            )
        }
        "activity_status" => {
            let t = choice("Activity Status")?;
            Edit::Status(
                act::parse_status(&t).ok_or_else(|| format!("{t:?} is not an activity status."))?,
            )
        }
        "duration_type" => {
            let t = choice("Duration Type")?;
            Edit::DurationType(
                act::duration_type_code(t.trim())
                    .ok_or_else(|| format!("{t:?} is not a duration type."))?,
            )
        }
        "percent_complete_type" => {
            let t = choice("% Complete Type")?;
            Edit::PercentType(
                act::percent_type_code(t.trim())
                    .ok_or_else(|| format!("{t:?} is not a percent complete type."))?,
            )
        }
        "activity_priority" => {
            let t = choice("Activity Leveling Priority")?;
            Edit::Priority(
                act::priority_code(t.trim())
                    .ok_or_else(|| format!("{t:?} is not a leveling priority."))?,
            )
        }
        "primary_resource" => match text(value).filter(|t| !t.trim().is_empty()) {
            None => Edit::PrimaryResource(0),
            Some(name) => Edit::PrimaryResource(find_resource(p, &name)?),
        },
        "original_duration" => Edit::OriginalDuration(hours(value, "Original Duration")?),
        "remaining_duration" => Edit::RemainingDuration(hours(value, "Remaining Duration")?),
        "actual_duration" => Edit::ActualDuration(hours(value, "Actual Duration")?),
        "at_completion_duration" => {
            Edit::AtCompletionDuration(hours(value, "At Completion Duration")?)
        }
        "actual_start" => Edit::ActualStart(date(origin, value)?),
        "actual_finish" => Edit::ActualFinish(date(origin, value)?),
        "start" => Edit::Start(needed(origin, value, "Start")?),
        "start_without_constraint" => Edit::StartUnheld(needed(origin, value, "Start")?),
        "finish" => Edit::Finish(needed(origin, value, "Finish")?),
        "planned_start" => Edit::PlannedStart(needed(origin, value, "Planned Start")?),
        "planned_finish" => Edit::PlannedFinish(needed(origin, value, "Planned Finish")?),
        "expected_finish" => Edit::ExpectedFinish(date(origin, value)?),
        "suspend_date" => Edit::SuspendDate(date(origin, value)?),
        "resume_date" => Edit::ResumeDate(date(origin, value)?),
        "physical_percent_complete" => Edit::PhysicalPercent(number(value, "Physical % Complete")?),
        "duration_percent_complete" => Edit::DurationPercent(number(value, "Duration % Complete")?),
        "activity_percent_complete" => Edit::ActivityPercent(number(value, "Activity % Complete")?),
        "primary_constraint" | "secondary_constraint" => {
            let t = text(value).unwrap_or_default();
            Edit::Constraint {
                secondary: field == "secondary_constraint",
                kind: act::parse_constraint(&t)
                    .ok_or_else(|| format!("{t:?} is not a constraint type."))?,
            }
        }
        "primary_constraint_date" | "secondary_constraint_date" => Edit::ConstraintDate {
            secondary: field == "secondary_constraint_date",
            date: date(origin, value)?,
        },
        other => match figure_field(other) {
            Some((None, measure)) => Edit::ThisPeriod {
                measure,
                value: number(value, other)?,
            },
            Some((Some(phase), measure)) => Edit::Figure {
                measure,
                phase,
                value: number(value, other)?,
            },
            None => {
                return Err(format!(
                    "Unknown activity field {other:?}; fields: {}, and the units and costs                      (budgeted_, actual_, remaining_, at_completion_ or this_period_ with labor_units,                      nonlabor_units, labor_cost, nonlabor_cost or material_cost).",
                    ACTIVITY_FIELDS.join(", ")
                ))
            }
        },
    };
    match act::edit_activity(p, i, edit.clone()) {
        Ok(outcome) => Ok((outcome.changed, outcome.warning)),
        // A value the field already holds is no change, even where the
        // field is locked (Remaining Duration 0 on a finished activity).
        Err(_) if holds(p, i, &edit) => Ok((false, None)),
        Err(e) => Err(e),
    }
}

/// A units or cost field of the Status tab: `budgeted_labor_units`,
/// `actual_labor_cost`, `this_period_nonlabor_units` (phase `None`)...
/// Units are hours for labor and nonlabor resources.
fn figure_field(field: &str) -> Option<(Option<act::Phase>, act::Measure)> {
    let (phase, rest) = if let Some(rest) = field.strip_prefix("this_period_") {
        (None, rest)
    } else {
        let phase = act::Phase::ALL
            .into_iter()
            .find(|ph| field.starts_with(&format!("{}_", ph.key())))?;
        (Some(phase), &field[phase.key().len() + 1..])
    };
    let measure = act::Measure::ALL
        .into_iter()
        .find(|m| m.key() == rest && m.editable())?;
    Some((phase, measure))
}

/// Whether activity `i` already holds the value `edit` would set.
fn holds(p: &ProjectData, i: usize, edit: &Edit) -> bool {
    let a = &p.model.activities[i];
    let d = p.details.get(&a.task_id);
    match edit {
        Edit::RemainingDuration(m) => a.rem == *m,
        Edit::OriginalDuration(m) => d.is_some_and(|d| d.original_duration == *m),
        Edit::ActualStart(v) => a.act_start == *v,
        Edit::ActualFinish(v) => a.act_finish == *v,
        Edit::ExpectedFinish(v) => a.expect_end == *v,
        Edit::ResumeDate(v) => a.resume == *v,
        Edit::PhysicalPercent(v) => d.is_some_and(|d| (d.percent - v).abs() < 1e-9),
        Edit::ConstraintDate {
            secondary: false,
            date,
        } => a.cstr_date == *date,
        Edit::ConstraintDate {
            secondary: true,
            date,
        } => a.cstr_date2 == *date,
        _ => false,
    }
}

/// Whether assignment row `row` already holds `value` in `field`.
fn assignment_holds(p: &ProjectData, row: usize, field: &str, value: &Value) -> bool {
    let Ok(column) = column_for(TASKRSRC_COLUMNS, &p.document, "TASKRSRC", field) else {
        return false;
    };
    let now = records::get(&p.document, "TASKRSRC", row, column);
    if field == "rate_type" {
        let wanted = text(value).and_then(|t| rate_type_code(&t));
        let now = if now.is_empty() {
            "COST_PER_QTY".to_string()
        } else {
            now
        };
        return wanted.as_deref() == Some(now.as_str());
    }
    match (now.trim().parse::<f64>(), number(value, field)) {
        (Ok(a), Ok(b)) => (a - b).abs() < 1e-9,
        (Err(_), Ok(b)) => now.trim().is_empty() && b == 0.,
        _ => false,
    }
}

/// How `period_remaining_units` is written.
const PERIOD_FORM: &str = "period_remaining_units takes DATE=UNITS (one day), \
     START,FINISH=UNITS (those days, the units spread evenly over their working time) \
     or {\"start\", \"finish\", \"units\"}.";

/// The value of `period_remaining_units`: the period (model minutes, from
/// the first day's start to the end of the last) and its units.
fn period_value(p: &ProjectData, value: &Value) -> std::result::Result<((i64, i64), f64), String> {
    const DAY: i64 = 24 * 60;
    let origin = p.model.origin_serial;
    let (start, finish, units) = match value {
        Value::Object(o) => {
            let start = o.get("start").and_then(text).ok_or(PERIOD_FORM)?;
            let finish = o.get("finish").and_then(text);
            let units = number(o.get("units").ok_or(PERIOD_FORM)?, "Units")?;
            (start, finish, units)
        }
        other => {
            let t = text(other).ok_or(PERIOD_FORM)?;
            let (dates, units) = t.rsplit_once('=').ok_or(PERIOD_FORM)?;
            let (start, finish) = match dates.split_once(',') {
                Some((s, f)) => (s.trim().to_string(), Some(f.trim().to_string())),
                None => (dates.trim().to_string(), None),
            };
            (start, finish, number(&Value::from(units.trim()), "Units")?)
        }
    };
    let day = |t: &str| -> std::result::Result<i64, String> {
        let at = parse_relative(origin, t)?.ok_or_else(|| format!("{t:?} is not a date."))?;
        Ok(at.div_euclid(DAY) * DAY)
    };
    let first = day(&start)?;
    let last = match finish {
        Some(f) => day(&f)?,
        None => first,
    };
    if last < first {
        return Err("The period's last day is before its first.".into());
    }
    Ok(((first, last + DAY), units))
}

/// The assignment fields `set_assignment` takes.
pub const ASSIGNMENT_FIELDS: &[&str] = &[
    "lag_hours",
    "planned_lag_hours",
    "rate_type",
    "rate_source",
    "price_per_unit",
    "at_completion_units",
    "at_completion_cost",
    "budgeted_units",
    "remaining_units",
    "budgeted_units_per_hour",
    "remaining_units_per_hour",
    "actual_regular_units",
    "actual_overtime_units",
    "budgeted_cost",
    "remaining_cost",
    "actual_regular_cost",
    "actual_overtime_cost",
    "actual_cost",
    "this_period_units",
    "this_period_cost",
    "calculate_costs_from_units",
    "overtime_factor",
    "original_duration",
    "remaining_duration",
    "drive_activity_dates",
    "resource",
    "role",
    "cost_account",
    "curve",
    "period_remaining_units",
];

/// An id from a dictionary table by name (or short name, or id).
fn dictionary_id(
    p: &ProjectData,
    table: &str,
    key: &str,
    names: &[&str],
    value: &str,
    what: &str,
) -> std::result::Result<i64, String> {
    let t = p
        .document
        .table(table)
        .ok_or_else(|| format!("The file has no {what}s."))?;
    let wanted = value.trim();
    for exact in [true, false] {
        let found: Vec<i64> = t
            .rows
            .iter()
            .filter(|r| {
                names.iter().any(|n| {
                    let v = t.value(r, n).trim();
                    if exact {
                        v == wanted
                    } else {
                        v.eq_ignore_ascii_case(wanted)
                    }
                })
            })
            .map(|r| records::num(t.value(r, key)))
            .collect();
        match found.as_slice() {
            [id] => return Ok(*id),
            [] => {}
            _ => return Err(format!("{what} {wanted:?} is not unique.")),
        }
    }
    t.rows
        .iter()
        .map(|r| records::num(t.value(r, key)))
        .find(|id| id.to_string() == wanted)
        .ok_or_else(|| format!("No {what} {wanted:?}."))
}

/// The assignment edits that name records: `resource` (replace the
/// resource), `role` and `cost_account` (null clears); the rest as
/// [`assignment_edit`].
fn assignment_edit_in(
    p: &ProjectData,
    field: &str,
    value: &Value,
) -> std::result::Result<AE, String> {
    let named = text(value).filter(|t| !t.trim().is_empty());
    Ok(match field {
        "resource" => AE::Resource(find_resource(p, &named.ok_or("A resource is required.")?)?),
        "role" => AE::Role(match named {
            Some(n) => Some(dictionary_id(
                p,
                "ROLES",
                "role_id",
                &["role_short_name", "role_name"],
                &n,
                "role",
            )?),
            None => None,
        }),
        "cost_account" => AE::CostAccount(match named {
            Some(n) => Some(dictionary_id(
                p,
                "ACCOUNT",
                "acct_id",
                &["acct_short_name", "acct_name"],
                &n,
                "cost account",
            )?),
            None => None,
        }),
        _ => assignment_edit(field, value)?,
    })
}

/// A yes or no value: `true`, or the text true, y, yes or 1.
fn flag(value: &Value) -> bool {
    match value {
        Value::Bool(b) => *b,
        other => matches!(
            text(other)
                .unwrap_or_default()
                .to_ascii_lowercase()
                .as_str(),
            "true" | "y" | "yes" | "1"
        ),
    }
}

fn assignment_edit(field: &str, value: &Value) -> std::result::Result<AE, String> {
    let n = |what: &str| number(value, what);
    Ok(match field {
        "lag_hours" => AE::Lag(hours(value, "Lag")?),
        "planned_lag_hours" => AE::PlannedLag(hours(value, "Planned Lag")?),
        "rate_type" => {
            let t = text(value).ok_or("Rate type needs a value.")?;
            AE::RateType(rate_type_code(&t).ok_or_else(|| format!("{t:?} is not a rate type."))?)
        }
        "rate_source" => {
            let t = text(value).ok_or("Rate source needs a value.")?;
            let code = match t.trim().to_ascii_lowercase().as_str() {
                "resource" | "st_rsrc" => "ST_Rsrc",
                "role" | "st_role" => "ST_Role",
                "override" | "st_custom" => "ST_Custom",
                _ => return Err(format!("{t:?} is not a rate source.")),
            };
            AE::RateSource(code.into())
        }
        "price_per_unit" => AE::PricePerUnit(n("Price / Unit")?),
        "at_completion_units" => AE::AtCompletionUnits(n("At Completion Units")?),
        "at_completion_cost" => AE::AtCompletionCost(n("At Completion Cost")?),
        "budgeted_units" => AE::BudgetedUnits(n("Budgeted Units")?),
        "remaining_units" => AE::RemainingUnits(n("Remaining Units")?),
        "budgeted_units_per_hour" => AE::BudgetedUnitsPerTime(n("Budgeted Units / Time")?),
        "remaining_units_per_hour" => AE::RemainingUnitsPerTime(n("Remaining Units / Time")?),
        "actual_regular_units" => AE::ActualRegularUnits(n("Actual Regular Units")?),
        "actual_overtime_units" => AE::ActualOvertimeUnits(n("Actual Overtime Units")?),
        "budgeted_cost" => AE::BudgetedCost(n("Budgeted Cost")?),
        "remaining_cost" => AE::RemainingCost(n("Remaining Cost")?),
        "actual_regular_cost" => AE::ActualRegularCost(n("Actual Regular Cost")?),
        "actual_overtime_cost" => AE::ActualOvertimeCost(n("Actual Overtime Cost")?),
        "this_period_units" => AE::ThisPeriodUnits(n("Actual This Period Units")?),
        "this_period_cost" => AE::ThisPeriodCost(n("Actual This Period Cost")?),
        "actual_cost" => AE::ActualCost(n("Actual Cost")?),
        "overtime_factor" => AE::OvertimeFactor(n("Overtime Factor")?),
        "original_duration" => AE::OriginalDuration(hours(value, "Original Duration")?),
        "remaining_duration" => AE::RemainingDuration(hours(value, "Remaining Duration")?),
        "drive_activity_dates" => AE::DriveActivityDates(flag(value)),
        "calculate_costs_from_units" => AE::CostUnitsLinked(flag(value)),
        other => {
            return Err(format!(
                "Unknown assignment field {other:?}; fields: {}.",
                ASSIGNMENT_FIELDS.join(", ")
            ))
        }
    })
}

fn rate_type_code(text: &str) -> Option<String> {
    let t = text.trim();
    let upper = t.to_ascii_uppercase();
    if let Some(n) = upper.strip_prefix("COST_PER_QTY") {
        return matches!(n, "" | "2" | "3" | "4" | "5").then(|| upper.clone());
    }
    let compact: String = t
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase();
    Some(
        match compact.as_str() {
            "price/unit" => "COST_PER_QTY",
            "price/unit2" => "COST_PER_QTY2",
            "price/unit3" => "COST_PER_QTY3",
            "price/unit4" => "COST_PER_QTY4",
            "price/unit5" => "COST_PER_QTY5",
            _ => return None,
        }
        .into(),
    )
}

// ------------------------------------------------------- exact writes

/// OpenXER activity keys and the TASK column each writes in exact mode.
const TASK_COLUMNS: &[(&str, &str)] = &[
    ("activity_name", "task_name"),
    ("wbs", "wbs_id"),
    ("calendar", "clndr_id"),
    ("activity_type", "task_type"),
    ("activity_status", "status_code"),
    ("status", "status_code"),
    ("planned_start", "target_start_date"),
    ("planned_finish", "target_end_date"),
    ("estimated_weight", "est_wt"),
    ("lock_planned", "lock_plan_flag"),
    ("review_required", "rev_fdbk_flag"),
    ("auto_compute_actuals", "auto_compute_act_flag"),
    ("actual_this_period_labor_units", "act_this_per_work_qty"),
    (
        "actual_this_period_nonlabor_units",
        "act_this_per_equip_qty",
    ),
    ("external_early_start", "external_early_start_date"),
    ("external_late_finish", "external_late_end_date"),
    ("location", "location_id"),
    ("duration_type", "duration_type"),
    ("percent_complete_type", "complete_pct_type"),
    ("activity_priority", "priority_type"),
    ("primary_resource", "rsrc_id"),
    ("original_duration", "target_drtn_hr_cnt"),
    ("remaining_duration", "remain_drtn_hr_cnt"),
    ("actual_start", "act_start_date"),
    ("actual_finish", "act_end_date"),
    ("expected_finish", "expect_end_date"),
    ("suspend_date", "suspend_date"),
    ("resume_date", "resume_date"),
    ("physical_percent_complete", "phys_complete_pct"),
    ("primary_constraint", "cstr_type"),
    ("primary_constraint_date", "cstr_date"),
    ("secondary_constraint", "cstr_type2"),
    ("secondary_constraint_date", "cstr_date2"),
    ("budgeted_labor_units", "target_work_qty"),
    ("actual_labor_units", "act_work_qty"),
    ("remaining_labor_units", "remain_work_qty"),
    ("budgeted_nonlabor_units", "target_equip_qty"),
    ("actual_nonlabor_units", "act_equip_qty"),
    ("remaining_nonlabor_units", "remain_equip_qty"),
];

/// OpenXER assignment keys and the TASKRSRC column each writes in exact mode.
const TASKRSRC_COLUMNS: &[(&str, &str)] = &[
    ("lag_hours", "target_lag_drtn_hr_cnt"),
    ("planned_lag_hours", "target_lag_drtn_hr_cnt"),
    ("remaining_lag_hours", "relag_drtn_hr_cnt"),
    ("rate_type", "rate_type"),
    ("price_per_unit", "cost_per_qty"),
    ("budgeted_units", "target_qty"),
    ("remaining_units", "remain_qty"),
    ("budgeted_units_per_hour", "target_qty_per_hr"),
    ("remaining_units_per_hour", "remain_qty_per_hr"),
    ("actual_regular_units", "act_reg_qty"),
    ("actual_overtime_units", "act_ot_qty"),
    ("budgeted_cost", "target_cost"),
    ("remaining_cost", "remain_cost"),
    ("actual_regular_cost", "act_reg_cost"),
    ("actual_overtime_cost", "act_ot_cost"),
    ("overtime_factor", "ot_factor"),
    ("actual_start", "act_start_date"),
    ("actual_finish", "act_end_date"),
    ("actual_this_period_units", "act_this_per_qty"),
    ("actual_this_period_cost", "act_this_per_cost"),
    ("calculate_costs_from_units", "cost_qty_link_flag"),
    ("skill_level", "skill_level"),
    ("drive_activity_dates", "rollup_dates_flag"),
    ("budgeted_curve", "target_crv"),
    ("remaining_curve", "remain_crv"),
    ("actual_curve", "actual_crv"),
    ("timesheet_pending_finish", "ts_pend_act_end_flag"),
    ("curve", "curv_id"),
    ("resource_type", "rsrc_type"),
    ("rate_source", "cost_per_qty_source_type"),
    ("has_resource_hours", "has_rsrchours"),
];

/// The cell text for an exact write: numbers as given, text as given.
fn exact_text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(s) => records::clean(s),
        other => other.to_string(),
    }
}

fn column_for<'a>(
    map: &'a [(&'a str, &'a str)],
    doc: &Document,
    table: &str,
    field: &'a str,
) -> std::result::Result<&'a str, String> {
    if let Some((_, c)) = map.iter().find(|(k, _)| *k == field) {
        return Ok(c);
    }
    let raw = openxer_xer::standard_fields(table).is_some_and(|f| f.contains(&field))
        || doc.table(table).is_some_and(|t| t.field(field).is_some());
    let protected = ["task_id", "proj_id", "taskrsrc_id", "task_code", "guid"];
    if raw && !protected.contains(&field) {
        Ok(field)
    } else {
        Err(format!(
            "{field:?} is not a {table} field that can be written exactly."
        ))
    }
}

fn set_task_exact(
    p: &mut ProjectData,
    i: usize,
    field: &str,
    value: &Value,
) -> std::result::Result<String, String> {
    let column = column_for(TASK_COLUMNS, &p.document, "TASK", field)?.to_string();
    let t = text(value).unwrap_or_default();
    let task_id = p.model.activities[i].task_id;
    let pid = p.model.project.proj_id;
    // Identifiers and choices become the stored ids and codes.
    let cell = match column.as_str() {
        // Exact writes keep the milestone rule like every other path.
        "task_type" => {
            let to = act::parse_task_type(&t)
                .ok_or_else(|| format!("{t:?} is not an activity type."))?;
            let now = |c: &str| records::task_value(&p.document, task_id, pid, c);
            let from =
                act::parse_task_type(&now("task_type")).unwrap_or(p.model.activities[i].task_type);
            let clear = openxer_xer::assignments::milestone_type_change(
                p,
                task_id,
                &now("task_code"),
                from,
                to,
            )?;
            if clear {
                records::set_task_value(&mut p.document, task_id, pid, "rsrc_id", String::new());
            }
            to.as_str().into()
        }
        "wbs_id" => wbs::find_path(p, &t)
            .ok_or_else(|| format!("No WBS element {t:?}."))?
            .to_string(),
        "clndr_id" => calendar_id(p, &t)?,
        "rsrc_id" if !t.trim().is_empty() => find_resource(p, &t)?.to_string(),
        "duration_type" => act::duration_type_code(t.trim())
            .ok_or_else(|| format!("{t:?} is not a duration type."))?
            .into(),
        "complete_pct_type" => act::percent_type_code(t.trim())
            .ok_or_else(|| format!("{t:?} is not a percent complete type."))?
            .into(),
        "priority_type" => act::priority_code(t.trim())
            .ok_or_else(|| format!("{t:?} is not a leveling priority."))?
            .into(),
        "cstr_type" | "cstr_type2" => openxer_xer::constraint_code(
            act::parse_constraint(&t).ok_or_else(|| format!("{t:?} is not a constraint type."))?,
        )
        .into(),
        "status_code" if !t.starts_with("TK_") => match act::parse_status(&t) {
            Some(openxer_engine::model::StatusType::NotStarted) => "TK_NotStart".into(),
            Some(openxer_engine::model::StatusType::InProgress) => "TK_Active".into(),
            Some(openxer_engine::model::StatusType::Completed) => "TK_Complete".into(),
            None => return Err(format!("{t:?} is not an activity status.")),
        },
        _ => exact_text(value),
    };
    records::set_task_value(&mut p.document, task_id, pid, &column, cell);
    Ok(column)
}

fn set_assignment_exact(
    p: &mut ProjectData,
    row: usize,
    field: &str,
    value: &Value,
) -> std::result::Result<String, String> {
    let column = column_for(TASKRSRC_COLUMNS, &p.document, "TASKRSRC", field)?.to_string();
    let cell = match column.as_str() {
        "rate_type" => {
            let t = text(value).unwrap_or_default();
            rate_type_code(&t).ok_or_else(|| format!("{t:?} is not a rate type."))?
        }
        _ => exact_text(value),
    };
    records::set(&mut p.document, "TASKRSRC", row, &column, cell);
    Ok(column)
}

// ------------------------------------------------------------------ diff

/// The tables the edit operations write, as the diff reports them: table,
/// label and the columns a row is known by (activities by Activity ID and
/// resources by short name). The first seven have their own keys.
const DIFF_TABLES: &[(&str, &str, &[&str])] = &[
    ("TASK", "activities", &[]),
    ("TASKRSRC", "assignments", &[]),
    ("TASKPRED", "relationships", &[]),
    ("PROJWBS", "wbs", &[]),
    ("CALENDAR", "calendars", &[]),
    ("DOCUMENT", "documents", &[]),
    ("TASKDOC", "document_links", &[]),
    ("PROJECT", "project", &["proj_id"]),
    ("SCHEDOPTIONS", "schedule_options", &["schedoptions_id"]),
    ("RSRC", "resources", &["rsrc_id"]),
    (
        "RSRCRATE",
        "resource_limits",
        &["rsrc_id", "start_date", "shift_period_id"],
    ),
    ("ACTVTYPE", "code_types", &["actv_code_type_id"]),
    ("ACTVCODE", "code_values", &["actv_code_id"]),
    (
        "TASKACTV",
        "activity_codes",
        &["task_id", "actv_code_type_id"],
    ),
    (
        "PROJPCAT",
        "project_codes",
        &["proj_id", "proj_catg_type_id"],
    ),
    ("MEMOTYPE", "notebook_topics", &["memo_type_id"]),
    ("TASKMEMO", "notebooks", &["task_id", "memo_type_id"]),
    ("WBSMEMO", "wbs_notebooks", &["wbs_id", "memo_type_id"]),
    ("UDFVALUE", "udf_values", &["udf_type_id", "fk_id"]),
    ("PROJFUND", "funding", &["proj_fund_id"]),
    ("PROJCOST", "expenses", &["cost_item_id"]),
    ("PROJISSU", "issues", &["issue_id"]),
    ("ISSUHIST", "issue_history", &["issue_id"]),
    ("RISK", "risks", &["risk_id"]),
    ("PROJRISK", "risks", &["risk_id"]),
    ("TASKRISK", "risk_assignments", &["risk_id", "task_id"]),
    ("PROJTHRS", "thresholds", &["thresh_id"]),
    ("TASKPROC", "steps", &["proc_id"]),
    ("PROCGROUP", "step_templates", &["proc_group_id"]),
    ("PROCITEM", "step_template_steps", &["proc_item_id"]),
    ("TASKNOTE", "notes_to_resources", &["task_id"]),
    ("TASKFDBK", "feedback", &["task_id"]),
    ("RSRCCURVDATA", "curves", &["curv_id"]),
];

/// Changed fields between two documents for the edited project, keyed the
/// way operations name things.
pub fn diff(before: &Document, after: &Document, pid: i64) -> Value {
    let codes = |doc: &Document| -> BTreeMap<String, String> {
        doc.table("TASK")
            .map(|t| {
                t.rows
                    .iter()
                    .map(|r| {
                        (
                            t.value(r, "task_id").to_string(),
                            t.value(r, "task_code").to_string(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    let resources = |doc: &Document| -> BTreeMap<String, String> {
        doc.table("RSRC")
            .map(|t| {
                t.rows
                    .iter()
                    .map(|r| {
                        (
                            t.value(r, "rsrc_id").to_string(),
                            t.value(r, "rsrc_short_name").to_string(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    let (code_a, code_b) = (codes(before), codes(after));
    let (res_a, res_b) = (resources(before), resources(after));
    let pid = pid.to_string();
    type Keyed = BTreeMap<String, BTreeMap<String, String>>;
    let keyed = |doc: &Document,
                 table: &str,
                 keys: &[&str],
                 code: &BTreeMap<String, String>,
                 res: &BTreeMap<String, String>|
     -> Keyed {
        let Some(t) = doc.table(table) else {
            return Keyed::new();
        };
        let has_project = t.field("proj_id").is_some();
        // Rows of this project, and rows of no project (global calendars,
        // code types, resource values).
        t.rows
            .iter()
            .filter(|r| {
                let owner = t.value(r, "proj_id");
                !has_project || owner == pid || owner.is_empty() || table == "CALENDAR"
            })
            .map(|r| {
                let c = |k: &str| code.get(t.value(r, k)).cloned().unwrap_or_default();
                let key = match table {
                    "TASK" => t.value(r, "task_code").to_string(),
                    "TASKRSRC" => format!(
                        "{}|{}",
                        c("task_id"),
                        res.get(t.value(r, "rsrc_id")).cloned().unwrap_or_default()
                    ),
                    "TASKPRED" => format!(
                        "{}|{}|{}",
                        c("pred_task_id"),
                        c("task_id"),
                        t.value(r, "pred_type")
                    ),
                    "CALENDAR" => t.value(r, "clndr_name").to_string(),
                    "DOCUMENT" => t.value(r, "doc_id").to_string(),
                    "TASKDOC" => match c("task_id") {
                        a if !a.is_empty() => format!("{}|{a}", t.value(r, "doc_id")),
                        _ => format!("{}|wbs {}", t.value(r, "doc_id"), t.value(r, "wbs_id")),
                    },
                    "PROJWBS" => t.value(r, "wbs_id").to_string(),
                    // Activities by Activity ID, resources by short name,
                    // anything else by its stored id.
                    _ => keys
                        .iter()
                        .map(|k| match *k {
                            "task_id" => c(k),
                            "rsrc_id" => res.get(t.value(r, k)).cloned().unwrap_or_default(),
                            _ => t.value(r, k).to_string(),
                        })
                        .collect::<Vec<_>>()
                        .join("|"),
                };
                let fields = t
                    .fields
                    .iter()
                    .zip(r.iter())
                    .filter(|(f, _)| !matches!(f.as_str(), "update_date" | "update_user"))
                    .map(|(f, v)| (f.clone(), v.clone()))
                    .collect();
                (key, fields)
            })
            .collect()
    };
    let mut out = Map::new();
    let mut total = 0usize;
    for &(table, label, keys) in DIFF_TABLES {
        let a = keyed(before, table, keys, &code_a, &res_a);
        let b = keyed(after, table, keys, &code_b, &res_b);
        let keys: BTreeSet<&String> = a.keys().chain(b.keys()).collect();
        let mut added = vec![];
        let mut removed = vec![];
        let mut changed = Map::new();
        for k in keys {
            match (a.get(k), b.get(k)) {
                (None, Some(_)) => added.push(json!(k)),
                (Some(_), None) => removed.push(json!(k)),
                (Some(x), Some(y)) if x != y => {
                    let mut fields = Map::new();
                    let names: BTreeSet<&String> = x.keys().chain(y.keys()).collect();
                    for f in names {
                        let (old, new) = (x.get(f), y.get(f));
                        if old.map_or("", String::as_str) != new.map_or("", String::as_str) {
                            fields.insert(f.clone(), json!({"before":old,"after":new}));
                        }
                    }
                    if !fields.is_empty() {
                        total += fields.len();
                        changed.insert(k.clone(), Value::Object(fields));
                    }
                }
                _ => {}
            }
        }
        total += added.len() + removed.len();
        if !(added.is_empty() && removed.is_empty() && changed.is_empty()) {
            // RISK and PROJRISK share a label (a file holds one or the
            // other); should both change, neither hides the other.
            let label = if out.contains_key(label) {
                format!("{label}_{}", table.to_ascii_lowercase())
            } else {
                label.to_string()
            };
            out.insert(
                label,
                json!({"added":added,"removed":removed,"changed":changed}),
            );
        }
    }
    out.insert("changed_field_count".into(), json!(total));
    Value::Object(out)
}

/// A JSON value as a record cell: text as given, numbers as written, null
/// blank.
fn cell(value: &Value) -> Cell {
    match value {
        Value::Null => Cell::Null,
        Value::String(t) => Cell::Text(t.clone()),
        Value::Bool(b) => Cell::Text(if *b { "Y" } else { "N" }.into()),
        other => Cell::Text(other.to_string()),
    }
}

/// Record edits: written into the document as given (the batch re-reads
/// the project before the next model edit or the schedule).
/// The JSON report of `merge_resources`: what moved where and what was
/// combined, plus the same lines the desktop preview shows.
fn merge_report(r: &resources::MergeReport) -> Value {
    let pair = |moved: usize, dropped: usize| json!({"moved":moved,"dropped":dropped});
    let assignments: Vec<Value> = r
        .assignments
        .iter()
        .map(|(c, n)| json!({"resource":c,"count":n}))
        .collect();
    let combined: Vec<Value> = r
        .combined
        .iter()
        .map(|c| json!({"activity":c.activity,"proj_id":c.proj_id,"assignments":c.rows}))
        .collect();
    let other: Vec<Value> = r
        .other
        .iter()
        .map(|(t, n)| json!({"table":t,"rows":n}))
        .collect();
    json!({
        "op": "merge_resources",
        "into": r.into_code,
        "from": r.from.iter().map(|(_, c)| c.clone()).collect::<Vec<_>>(),
        "assignments_moved": assignments,
        "combined": combined,
        "primary_resource": r.primary,
        "rates": pair(r.rates_moved, r.rates_kept),
        "roles": pair(r.roles_moved, r.roles_kept),
        "codes": pair(r.codes_moved, r.codes_kept),
        "udf_values": pair(r.udfs_moved, r.udfs_kept),
        "child_resources": r.children,
        "calendar_taken": r.calendar,
        "other_rows": other,
        "summary": r.lines(),
    })
}

fn record(p: &mut ProjectData, op: &Operation) -> std::result::Result<Value, String> {
    let task = |p: &ProjectData, code: &str| -> std::result::Result<i64, String> {
        find_activity(p, code).map(|i| p.model.activities[i].task_id)
    };
    match op {
        Operation::SetProject { field, value, .. } => {
            tables::set_project_field(p, field, &cell(value))?
        }
        Operation::SetScheduleOption { field, value } => {
            tables::set_schedule_option(p, field, &cell(value))?
        }
        Operation::SetResourceLimit {
            resource,
            from,
            shift_period,
            max_units_per_hour,
            price_per_unit,
            price_per_unit_2,
            price_per_unit_3,
            price_per_unit_4,
            price_per_unit_5,
        } => {
            let remove = matches!(max_units_per_hour, Some(Value::Null));
            let values: Vec<(&str, Cell)> = [
                ("max_qty_per_hr", max_units_per_hour),
                ("cost_per_qty", price_per_unit),
                ("cost_per_qty2", price_per_unit_2),
                ("cost_per_qty3", price_per_unit_3),
                ("cost_per_qty4", price_per_unit_4),
                ("cost_per_qty5", price_per_unit_5),
            ]
            .into_iter()
            .filter_map(|(c, v)| v.as_ref().map(|v| (c, cell(v))))
            .collect();
            tables::set_resource_limit(p, resource, from, shift_period.as_deref(), &values, remove)?
        }
        Operation::SetActivityCode {
            activity,
            code_type,
            value,
        } => {
            let id = task(p, activity)?;
            let Some(value) = value else {
                return Ok(unchanged(op, "value"));
            };
            tables::set_activity_code(p, id, code_type, text(value).as_deref())?
        }
        Operation::SetProjectCode { code_type, value } => {
            let Some(value) = value else {
                return Ok(unchanged(op, "value"));
            };
            openxer_xer::edit::project_codes::set_project_code(
                p,
                code_type,
                text(value).as_deref(),
            )?
        }
        Operation::AddCodeType {
            code_type,
            scope,
            max_length,
        } => tables::add_code_type(
            p,
            code_type,
            scope.as_deref().unwrap_or("project"),
            *max_length,
        )?,
        Operation::AddCodeValue { code_type, code } => tables::add_code_value(p, code_type, code)?,
        Operation::SetCodeValue {
            code_type,
            code,
            field,
            value,
        } => tables::set_code_value(p, code_type, code, field, &cell(value))?,
        Operation::DeleteCodeValue { code_type, code } => {
            tables::delete_code_value(p, code_type, code)?
        }
        Operation::SetNotebook {
            activity,
            topic,
            text: given,
        } => {
            let id = task(p, activity)?;
            let Some(given) = given else {
                return Ok(unchanged(op, "text"));
            };
            tables::set_notebook(p, id, topic, text(given).as_deref())?
        }
        Operation::SetWbsNotebook {
            wbs,
            topic,
            text: given,
        } => {
            let Some(given) = given else {
                return Ok(unchanged(op, "text"));
            };
            tables::set_wbs_notebook(p, wbs, topic, text(given).as_deref())?
        }
        Operation::SetUdf {
            activity,
            resource,
            step,
            wbs,
            project,
            expense,
            issue,
            risk,
            document,
            udf,
            value,
        } => {
            let named = [
                wbs.is_some(),
                *project == Some(true),
                expense.is_some(),
                issue.is_some(),
                risk.is_some(),
                document.is_some(),
            ];
            let others = named.iter().filter(|r| **r).count();
            let task_owner = activity.is_some() || resource.is_some();
            if others > 1 || (others == 1 && task_owner) {
                return Err(UDF_OWNER.into());
            }
            let (table, id) = match (activity, resource, step) {
                (Some(a), Some(r), None) => {
                    let task_id = task(p, a)?;
                    let rsrc = tables::resource_id(p, r)?;
                    let row = assignment_row(p, task_id, rsrc)?;
                    let id = records::get(&p.document, "TASKRSRC", row, "taskrsrc_id");
                    ("TASKRSRC", records::num(&id))
                }
                (Some(a), None, Some(st)) => {
                    let task_id = task(p, a)?;
                    let row = steps::find_step(p, task_id, &text(st).unwrap_or_default())?;
                    let id = records::get(&p.document, "TASKPROC", row, "proc_id");
                    ("TASKPROC", records::num(&id))
                }
                (Some(a), None, None) => ("TASK", task(p, a)?),
                (None, Some(r), None) => ("RSRC", tables::resource_id(p, r)?),
                (None, None, None) if others == 1 => {
                    if let Some(w) = wbs {
                        ("PROJWBS", earned_value::wbs_id(p, w)?)
                    } else if let Some(e) = expense {
                        ("PROJCOST", project_records::find(p, Kind::Expense, e)?)
                    } else if let Some(i) = issue {
                        ("PROJISSU", project_records::find(p, Kind::Issue, i)?)
                    } else if let Some(r) = risk {
                        ("PROJRISK", project_records::find(p, Kind::Risk, r)?)
                    } else if let Some(d) = document {
                        ("DOCUMENT", documents::find(p, d)?)
                    } else {
                        ("PROJECT", p.model.project.proj_id)
                    }
                }
                // A stored value whose owner is not in the file (an orphan
                // row) has no record to write to; it is reported, not written.
                (None, None, None) => {
                    return Ok(json!({"op":"set_udf","udf":udf,"written":false,
                        "reason":"no record named; orphan values are not written"}))
                }
                _ => return Err(UDF_OWNER.into()),
            };
            tables::set_udf(p, table, id, udf, &cell(value))?
        }
        Operation::AddCalendar { calendar } => tables::add_calendar(p, calendar)?,
        Operation::SetCalendar {
            calendar,
            field,
            value,
        } => {
            let written = tables::set_calendar(p, calendar, field, &cell(value))?;
            return Ok(
                json!({"op":"set_calendar","calendar":calendar,"field":field,"written":written}),
            );
        }
        Operation::DeleteCalendar {
            calendar,
            replacement,
            default_global,
        } => {
            if replacement.is_none() && !*default_global {
                tables::delete_calendar(p, calendar)?
            } else if replacement.is_some() && *default_global {
                return Err("Name a replacement or default_global, not both.".into());
            } else {
                let moved = tables::delete_calendar_replacing(p, calendar, replacement.as_deref())?;
                return Ok(json!({"op":"delete_calendar","calendar":calendar,
                    "replacement":replacement,"activities_moved":moved}));
            }
        }
        Operation::AddResource { resource } => tables::add_resource(p, resource)?,
        Operation::SetResource {
            resource,
            field,
            value,
        } => tables::set_resource(p, resource, field, &cell(value))?,
        Operation::SetWbs { wbs, field, value } => {
            tables::set_wbs_field(p, wbs, field, &cell(value))?
        }
        Operation::AddProjectFunding {
            source,
            amount,
            share,
        } => {
            let text =
                |v: &Option<Value>| v.as_ref().filter(|v| !v.is_null()).map(|v| cell(v).text());
            funding::add_by_source(p, source, text(amount).as_deref(), text(share).as_deref())?
        }
        Operation::SetProjectFunding {
            source,
            field,
            value,
        } => {
            let field = funding::Field::parse(field)?;
            let row = funding::row_by_source(p, source)?;
            funding::set(p, row, field, &cell(value).text())?
        }
        Operation::DeleteProjectFunding { source } => {
            let row = funding::row_by_source(p, source)?;
            funding::delete(p, row)?
        }
        Operation::DeleteResource { resource } => {
            let id = resources::find(&p.document, resource)?;
            resources::delete(&mut p.document, id)?
        }
        Operation::MergeResources { from, into } => {
            // A document edit: the batch re-reads the project afterwards.
            let target = resources::find(&p.document, into)?;
            let sources = from
                .iter()
                .map(|c| resources::find(&p.document, c))
                .collect::<std::result::Result<Vec<i64>, String>>()?;
            let report = resources::merge(&mut p.document, &sources, target)?;
            return Ok(merge_report(&report));
        }
        Operation::SetWbsEarnedValue {
            wbs,
            performance,
            custom_percent,
            estimate_to_complete,
            performance_factor,
        } => {
            let id = earned_value::wbs_id(p, wbs)?;
            let text = |v: &Option<Value>| v.as_ref().map(|v| cell(v).text());
            let changed = earned_value::set(
                p,
                id,
                performance.as_deref(),
                text(custom_percent).as_deref(),
                estimate_to_complete.as_deref(),
                text(performance_factor).as_deref(),
            )?;
            let t = earned_value::get(p, id)?;
            return Ok(
                json!({"op":"set_wbs_earned_value","wbs":wbs,"changed":changed,
                "performance":t.performance,"custom_percent":t.custom_percent,
                "estimate_to_complete":t.to_complete,"performance_factor":t.factor}),
            );
        }
        Operation::AddDocument {
            title,
            parent,
            reference,
            fields,
        } => {
            let parent = match parent.as_deref().filter(|t| !t.trim().is_empty()) {
                Some(name) => Some(documents::find(p, name)?),
                None => None,
            };
            let id = documents::add(p, parent);
            documents::set(p, id, documents::Field::Title, title)?;
            if let Some(r) = reference {
                documents::set(p, id, documents::Field::Reference, r)?;
            }
            for (name, value) in fields.iter().flatten() {
                let field = documents::Field::parse(name)?;
                documents::set(p, id, field, &cell(value).text())?;
            }
            return Ok(json!({"op":"add_document","document_id":id,"title":title}));
        }
        Operation::SetDocument {
            document,
            field,
            value,
        } => {
            let id = documents::find(p, document)?;
            let field = documents::Field::parse(field)?;
            let changed = documents::set(p, id, field, &cell(value).text())?;
            return Ok(json!({"op":"set_document","document_id":id,"changed":changed}));
        }
        Operation::DeleteDocument { document } => {
            let id = documents::find(p, document)?;
            let count = documents::delete(p, id);
            return Ok(json!({"op":"delete_document","document_id":id,"deleted":count}));
        }
        Operation::LinkDocument {
            document,
            activity,
            wbs,
            work_product,
        } => {
            let id = documents::find(p, document)?;
            let (t, w) = document_target(p, activity, wbs)?;
            let added = documents::link(p, id, t, w);
            if let Some(on) = work_product {
                if let Some(row) = documents::link_row(p, id, t, w) {
                    documents::set_work_product(p, row, *on);
                }
            }
            return Ok(json!({"op":"link_document","document_id":id,"added":added}));
        }
        Operation::UnlinkDocument {
            document,
            activity,
            wbs,
        } => {
            let id = documents::find(p, document)?;
            let (t, w) = document_target(p, activity, wbs)?;
            if !documents::unlink(p, id, t, w) {
                return Err(format!("Document {document:?} is not linked there."));
            }
        }
        Operation::AddExpense {
            activity,
            name,
            fields,
        } => {
            let t = task(p, activity)?;
            return add_register(p, op, Kind::Expense, Some(t), name, fields);
        }
        Operation::AddIssue { name, fields } => {
            return add_register(p, op, Kind::Issue, None, name, fields)
        }
        Operation::AddRisk { name, fields } => {
            return add_register(p, op, Kind::Risk, None, name, fields)
        }
        Operation::AddThreshold { parameter, fields } => {
            let mut all = Map::new();
            if let Some(q) = parameter {
                all.insert("parameter".into(), Value::String(q.clone()));
            }
            all.extend(fields.clone().unwrap_or_default());
            return add_register(p, op, Kind::Threshold, None, &None, &Some(all));
        }
        Operation::SetExpense {
            expense: r,
            field,
            value,
        }
        | Operation::SetIssue {
            issue: r,
            field,
            value,
        }
        | Operation::SetRisk {
            risk: r,
            field,
            value,
        }
        | Operation::SetThreshold {
            threshold: r,
            field,
            value,
        } => {
            let kind = register_kind(op);
            let id = project_records::find(p, kind, r)?;
            let text = text(value);
            let changed = project_records::set_field(p, kind, id, field, text.as_deref())?;
            return Ok(json!({"op":op.name(),"id":id,"field":field,"changed":changed}));
        }
        Operation::DeleteExpense { expense: r }
        | Operation::DeleteIssue { issue: r }
        | Operation::DeleteRisk { risk: r }
        | Operation::DeleteThreshold { threshold: r } => {
            let kind = register_kind(op);
            let id = project_records::find(p, kind, r)?;
            project_records::delete(p, kind, id)?;
            return Ok(json!({"op":op.name(),"id":id}));
        }
        Operation::CreateStepTemplate { activity, name } => {
            let t = task(p, activity)?;
            let pid = p.model.project.proj_id;
            let id = steps::template_from_activity(&mut p.document, name, t, pid)?;
            return Ok(json!({"op":"create_step_template","template_id":id,
                "name":steps::templates(&p.document).into_iter().find(|x| x.id == id).map(|x| x.name)}));
        }
        Operation::SetFeedback {
            activity,
            notes,
            review_required,
        } => {
            let t = task(p, activity)?;
            let pid = p.model.project.proj_id;
            if let Some(v) = notes {
                let text = text(v).unwrap_or_default();
                let html = if records::looks_like_html(&text) || text.is_empty() {
                    text
                } else {
                    records::text_to_memo(&text)
                };
                project_records::set_notes_to_resources(&mut p.document, t, pid, html);
            }
            if let Some(on) = review_required {
                project_records::set_review_required(&mut p.document, t, pid, *on);
            }
        }
        Operation::RecalculateCosts {
            active_only,
            sync_overtime,
        } => {
            let pid = p.model.project.proj_id;
            let r = progress::recalculate_assignment_costs(
                &mut p.document,
                pid,
                *active_only,
                *sync_overtime,
            );
            return Ok(json!({"op":"recalculate_costs","updated":r.updated,
                "unchanged":r.unchanged,"skipped":r.skipped}));
        }
        Operation::AssignRisk { risk, activity } | Operation::UnassignRisk { risk, activity } => {
            let id = project_records::find(p, Kind::Risk, risk)?;
            let t = task(p, activity)?;
            let assign = matches!(op, Operation::AssignRisk { .. });
            let changed = project_records::set_risk_activity(p, id, t, assign);
            if !changed && !assign {
                return Err(format!("Risk {risk:?} is not assigned to {activity}."));
            }
            return Ok(json!({"op":op.name(),"id":id,"changed":changed}));
        }
        Operation::DeleteFinancialPeriod { period, calendar } => {
            openxer_xer::edit::financial_periods::delete_by_name(p, period, calendar.as_deref())?
        }
        _ => unreachable!("not a record edit"),
    }
    Ok(json!({"op":op.name(),"subject":op.subject()}))
}

/// The result of an edit given nothing to write: a missing key changes
/// nothing (only an explicit null removes).
fn unchanged(op: &Operation, key: &str) -> Value {
    json!({"op":op.name(),"subject":op.subject(),"changed":false,
        "reason":format!("no {key} given; null removes it")})
}

/// What `set_udf` says when it names no one record.
const UDF_OWNER: &str = "set_udf names one record: activity, resource, activity and resource (an assignment), activity and step, wbs, project, expense, issue, risk or document.";

fn register_kind(op: &Operation) -> Kind {
    match op {
        Operation::AddExpense { .. }
        | Operation::SetExpense { .. }
        | Operation::DeleteExpense { .. } => Kind::Expense,
        Operation::AddIssue { .. } | Operation::SetIssue { .. } | Operation::DeleteIssue { .. } => {
            Kind::Issue
        }
        Operation::AddThreshold { .. }
        | Operation::SetThreshold { .. }
        | Operation::DeleteThreshold { .. } => Kind::Threshold,
        _ => Kind::Risk,
    }
}

/// Add a register record and set its name and fields; the id is reported.
fn add_register(
    p: &mut ProjectData,
    op: &Operation,
    kind: Kind,
    task_id: Option<i64>,
    name: &Option<String>,
    fields: &Option<Map<String, Value>>,
) -> std::result::Result<Value, String> {
    let id = project_records::add(p, kind, task_id, &project_records::today())?;
    if let Some(n) = name {
        project_records::set_field(p, kind, id, "name", Some(n))?;
    }
    for (field, value) in fields.iter().flatten() {
        project_records::set_field(p, kind, id, field, text(value).as_deref())?;
    }
    Ok(json!({"op":op.name(),"id":id,"record":register_json(p, kind, id)}))
}

/// One register record as `records` lists it.
pub fn register_json(p: &ProjectData, kind: Kind, id: i64) -> Value {
    let mut map = Map::new();
    map.insert("id".into(), json!(id));
    for (name, value) in project_records::record_values(p, kind, id) {
        map.insert(name.into(), Value::String(value));
    }
    if kind == Kind::Risk {
        let pid = p.model.project.proj_id;
        let acts: Vec<String> = project_records::risk_activities(&p.document, pid, id)
            .into_iter()
            .map(|t| project_records::activity_label(p, t).0)
            .collect();
        map.insert("activities".into(), json!(acts));
    }
    Value::Object(map)
}
