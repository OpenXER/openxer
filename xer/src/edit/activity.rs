//! Activity field setters shared by the desktop (Activity Details, the
//! activity table's in-place editors and the commands built on them) and the
//! command line and MCP edit commands. Each change is an [`Edit`];
//! [`edit_activity`] checks the field's locks, applies the change with its
//! rules and does the data follow-ups: status changes fill or clear actual
//! dates, Original and Remaining Duration stay equal until the activity
//! starts, and resource assignments follow duration changes by the duration
//! type. What only a user interface does after an edit (undo revisions, the
//! out-of-sequence prompt) is left to the caller, which [`Outcome`] tells
//! what happened.
use super::records::{self, format_number, num};
use super::udf;
use crate::assignments::{AssignmentEdit, ResourceSlot, UnitsPhase};
use crate::{constraint_label, date_string, ProjectData};
use openxer_engine::model::{CstrType, StatusType, TaskType};

/// Largest duration a field takes, in minutes.
pub const MAX_DURATION: i64 = 100_000 * 60;

/// Duration types: stored code and label.
pub const DURATION_TYPES: [(&str, &str); 4] = [
    ("DT_FixedDrtn", "Fixed Duration & Units/Time"),
    ("DT_FixedDUR2", "Fixed Duration & Units"),
    ("DT_FixedRate", "Fixed Units/Time"),
    ("DT_FixedQty", "Fixed Units"),
];
/// Percent complete types: stored code and label.
pub const PERCENT_TYPES: [(&str, &str); 3] = [
    ("CP_Phys", "Physical"),
    ("CP_Drtn", "Duration"),
    ("CP_Units", "Units"),
];
/// Activity leveling priorities: stored code and label, highest first.
pub const PRIORITIES: [(&str, &str); 5] = [
    ("PT_Top", "Top"),
    ("PT_High", "High"),
    ("PT_Normal", "Normal"),
    ("PT_Low", "Low"),
    ("PT_Lowest", "Lowest"),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Phase {
    Budgeted,
    Actual,
    Remaining,
    AtCompletion,
}
impl Phase {
    pub const ALL: [Phase; 4] = [
        Phase::Budgeted,
        Phase::Actual,
        Phase::Remaining,
        Phase::AtCompletion,
    ];
    pub fn caption(self) -> &'static str {
        match self {
            Phase::Budgeted => "Budgeted",
            Phase::Actual => "Actual",
            Phase::Remaining => "Remaining",
            Phase::AtCompletion => "At Completion",
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Phase::Budgeted => "budgeted",
            Phase::Actual => "actual",
            Phase::Remaining => "remaining",
            Phase::AtCompletion => "at_completion",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Measure {
    LaborUnits,
    NonlaborUnits,
    MaterialUnits,
    LaborCost,
    NonlaborCost,
    MaterialCost,
    ExpenseCost,
    TotalCost,
}
impl Measure {
    pub const ALL: [Measure; 8] = [
        Measure::LaborUnits,
        Measure::NonlaborUnits,
        Measure::MaterialUnits,
        Measure::LaborCost,
        Measure::NonlaborCost,
        Measure::MaterialCost,
        Measure::ExpenseCost,
        Measure::TotalCost,
    ];
    pub fn caption(self) -> &'static str {
        match self {
            Measure::LaborUnits => "Labor Units",
            Measure::NonlaborUnits => "Nonlabor Units",
            Measure::MaterialUnits => "Material Units",
            Measure::LaborCost => "Labor Cost",
            Measure::NonlaborCost => "Nonlabor Cost",
            Measure::MaterialCost => "Material Cost",
            Measure::ExpenseCost => "Expense Cost",
            Measure::TotalCost => "Total Cost",
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Measure::LaborUnits => "labor_units",
            Measure::NonlaborUnits => "nonlabor_units",
            Measure::MaterialUnits => "material_units",
            Measure::LaborCost => "labor_cost",
            Measure::NonlaborCost => "nonlabor_cost",
            Measure::MaterialCost => "material_cost",
            Measure::ExpenseCost => "expense_cost",
            Measure::TotalCost => "total_cost",
        }
    }
    pub fn is_cost(self) -> bool {
        !matches!(
            self,
            Measure::LaborUnits | Measure::NonlaborUnits | Measure::MaterialUnits
        )
    }
    /// The measures whose budgeted, actual and remaining figures are
    /// entered on the activity.
    pub fn editable(self) -> bool {
        matches!(
            self,
            Measure::LaborUnits
                | Measure::NonlaborUnits
                | Measure::LaborCost
                | Measure::NonlaborCost
                | Measure::MaterialCost
        )
    }
}

pub use super::udf::UdfValue;

/// One change to one activity field. Dates are minutes from the project's
/// origin, durations are working minutes, units are hours.
#[derive(Clone, Debug, PartialEq)]
pub enum Edit {
    ActivityType(TaskType),
    Status(StatusType),
    Wbs(i64),
    Calendar(usize),
    /// `rsrc_id` of one of the activity's assigned resources; 0 clears it.
    PrimaryResource(i64),
    /// Stored code, one of `DURATION_TYPES`.
    DurationType(&'static str),
    /// Stored code, one of `PERCENT_TYPES`.
    PercentType(&'static str),
    /// Stored code, one of `PRIORITIES`.
    Priority(&'static str),
    /// The activity's Auto Compute Actuals flag, which Apply Actuals reads.
    AutoComputeActuals(bool),
    /// A typed Start. Not started: Start and Planned Start move to this
    /// date, the finish follows by the duration, and a Start On or After
    /// constraint on the date holds the activity there (refused when
    /// another constraint type is set). Started: the actual start.
    Start(i64),
    /// [`Edit::Start`] without the constraint: the dates move, nothing
    /// holds them, and the next schedule may move the activity back. The
    /// answer No to the app's question; the same as Start once started.
    StartUnheld(i64),
    /// A typed Finish. Not started: the start stays and the Original (and
    /// so the Remaining) Duration is recalculated to fit; no constraint.
    Finish(i64),
    ActualStart(Option<i64>),
    ActualFinish(Option<i64>),
    PlannedStart(i64),
    PlannedFinish(i64),
    ExpectedFinish(Option<i64>),
    SuspendDate(Option<i64>),
    ResumeDate(Option<i64>),
    OriginalDuration(i64),
    ActualDuration(i64),
    RemainingDuration(i64),
    AtCompletionDuration(i64),
    PhysicalPercent(f64),
    ActivityPercent(f64),
    DurationPercent(f64),
    Constraint {
        secondary: bool,
        kind: CstrType,
    },
    ConstraintDate {
        secondary: bool,
        date: Option<i64>,
    },
    /// An activity code value of a code type; `None` removes it.
    Code {
        type_id: i64,
        value: Option<i64>,
    },
    Udf {
        type_id: i64,
        value: UdfValue,
    },
    /// A not-started activity's bar dragged in the Gantt: its start (and
    /// planned start) moves to this date and the finish follows by the
    /// remaining duration. Like a start typed into the Start cell, it holds
    /// the activity there with a Start On or After constraint, so the next
    /// schedule keeps it (refused when another constraint type is set).
    MoveStart(i64),
    /// Budgeted, actual or remaining units or cost of one measure (hours
    /// for units), through the assignment or the activity's own quantities.
    Figure {
        measure: Measure,
        phase: Phase,
        value: f64,
    },
    /// Actual units or cost this period of one measure.
    ThisPeriod {
        measure: Measure,
        value: f64,
    },
    /// A not-started activity's bar end dragged: the original (and so the
    /// remaining) duration, with the finish redrawn from the start.
    BarDuration(i64),
}

/// The fields whose locks [`field_lock`] knows. Every other field is
/// always open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lock {
    OriginalDuration,
    RemainingDuration,
    AtCompletionDuration,
    ActualDuration,
    ExpectedFinish,
    Finish,
    PhysicalPercent,
    DurationPercent,
    ActivityPercent,
    Figure(Measure, Phase),
    ThisPeriod(Measure),
    SuspendDate,
    ResumeDate,
    PrimaryConstraintDate,
    SecondaryConstraintDate,
    Open,
}

impl Edit {
    /// The lock this edit is checked against.
    pub fn lock(&self) -> Lock {
        match self {
            Edit::Finish(_) => Lock::Finish,
            Edit::ExpectedFinish(_) => Lock::ExpectedFinish,
            Edit::SuspendDate(_) => Lock::SuspendDate,
            Edit::ResumeDate(_) => Lock::ResumeDate,
            Edit::OriginalDuration(_) | Edit::BarDuration(_) => Lock::OriginalDuration,
            Edit::ActualDuration(_) => Lock::ActualDuration,
            Edit::RemainingDuration(_) => Lock::RemainingDuration,
            Edit::AtCompletionDuration(_) => Lock::AtCompletionDuration,
            Edit::PhysicalPercent(_) => Lock::PhysicalPercent,
            Edit::ActivityPercent(_) => Lock::ActivityPercent,
            Edit::DurationPercent(_) => Lock::DurationPercent,
            Edit::ConstraintDate {
                secondary: false,
                date: Some(_),
            } => Lock::PrimaryConstraintDate,
            Edit::ConstraintDate {
                secondary: true,
                date: Some(_),
            } => Lock::SecondaryConstraintDate,
            Edit::Figure { measure, phase, .. } => Lock::Figure(*measure, *phase),
            Edit::ThisPeriod { measure, .. } => Lock::ThisPeriod(*measure),
            _ => Lock::Open,
        }
    }
}

/// Stored duration type code for its label or code.
pub fn duration_type_code(label: &str) -> Option<&'static str> {
    code_of(&DURATION_TYPES, label)
}

/// Stored percent complete type code for its label or code.
pub fn percent_type_code(label: &str) -> Option<&'static str> {
    code_of(&PERCENT_TYPES, label)
}

/// Stored leveling priority code for its label or code.
pub fn priority_code(label: &str) -> Option<&'static str> {
    code_of(&PRIORITIES, label)
}

fn code_of(list: &[(&'static str, &'static str)], label: &str) -> Option<&'static str> {
    list.iter()
        .find(|(code, l)| l.eq_ignore_ascii_case(label) || code.eq_ignore_ascii_case(label))
        .map(|(code, _)| *code)
}

/// The three activity statuses in the order the status choice lists them.
pub const STATUSES: [StatusType; 3] = [
    StatusType::NotStarted,
    StatusType::InProgress,
    StatusType::Completed,
];

/// What an [`edit_activity`] did, for the caller's follow-ups.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Outcome {
    /// Something about the activity changed.
    pub changed: bool,
    /// A project record table (TASK columns, TASKRSRC, codes, UDFs) changed.
    pub records_changed: bool,
    /// Status, actual start or actual finish changed.
    pub progress_changed: bool,
    /// A follow-up that could not be made (the edit itself stands).
    pub warning: Option<String>,
}

/// How a caller writes a duration in a message.
pub type DurationText = fn(&ProjectData, usize, i64) -> String;

/// Durations in hours, as the command line shows them.
pub fn hours_text(_: &ProjectData, _: usize, minutes: i64) -> String {
    format!("{}h", format_number(minutes as f64 / 60.))
}

/// Where an edit of one measure of one activity lands.
#[derive(Clone, Debug, PartialEq)]
pub enum Route {
    /// The `TASKRSRC` row of the activity's only assignment of the type.
    Assignment(usize),
    /// The activity's own `TASK` quantities of the type.
    Own(ResourceSlot),
    /// Not editable, with the reason.
    Locked(String),
}

/// The resource type a measure covers, for the measures that can be edited.
fn slot_of(measure: Measure) -> Option<ResourceSlot> {
    match measure {
        Measure::LaborUnits | Measure::LaborCost => Some(ResourceSlot::Labor),
        Measure::NonlaborUnits | Measure::NonlaborCost => Some(ResourceSlot::Nonlabor),
        Measure::MaterialCost => Some(ResourceSlot::Material),
        _ => None,
    }
}

/// Text of the figure for messages: "Actual Labor Units".
pub fn figure_name(measure: Measure, phase: Option<Phase>) -> String {
    match phase {
        Some(Phase::Budgeted) => format!("Budgeted {}", measure.caption()),
        Some(Phase::Actual) => format!("Actual {}", measure.caption()),
        Some(Phase::Remaining) => format!("Remaining {}", measure.caption()),
        Some(Phase::AtCompletion) => format!("At Completion {}", measure.caption()),
        None => format!("Actual This Period {}", measure.caption()),
    }
}

/// Where the units or cost of `measure` go for activity `i`.
pub fn units_route(p: &ProjectData, i: usize, measure: Measure) -> Route {
    let Some(slot) = slot_of(measure) else {
        return Route::Locked(format!("{} is not editable.", measure.caption()));
    };
    let Some(a) = p.model.activities.get(i) else {
        return Route::Locked("No such activity.".into());
    };
    let rows = p.assignment_rows_of(a.task_id, slot);
    match rows.as_slice() {
        [row] => Route::Assignment(*row),
        [] if measure.is_cost() => Route::Locked(format!(
            "Assign a {} resource to enter its costs.",
            slot.caption()
        )),
        [] if slot == ResourceSlot::Material => {
            Route::Locked("Assign a material resource to enter its quantities.".into())
        }
        [] => Route::Own(slot),
        _ => Route::Locked(format!(
            "The activity has several {} resource assignments; edit them on the Resources tab.",
            slot.caption()
        )),
    }
}

/// Why a unit or cost figure cannot be edited on activity `i`. `phase` is
/// `None` for the Actual This Period figure.
pub fn figure_lock(
    p: &ProjectData,
    i: usize,
    measure: Measure,
    phase: Option<Phase>,
) -> Result<(), String> {
    if !measure.editable() {
        return Err(format!("{} is not editable.", figure_name(measure, phase)));
    }
    // At Completion units and cost are typed and the remaining figure takes
    // the difference (read only once the activity is completed).
    let completed = p.model.activities.get(i).is_some_and(|a| a.is_completed());
    if phase == Some(Phase::AtCompletion) && completed {
        return Err(format!(
            "{} cannot change on a completed activity.",
            figure_name(measure, phase)
        ));
    }
    if phase.is_none() && p.project_option("act_this_per_link_flag") != "Y" {
        return Err(
            "Actual This Period follows Actual only when the project links actual to date \
             and actual this period (Projects, Calculations)."
                .into(),
        );
    }
    match units_route(p, i, measure) {
        Route::Locked(reason) => Err(reason),
        _ => Ok(()),
    }
}

/// Whether physical % complete comes from the activity's weighted steps.
pub fn steps_drive_percent(p: &ProjectData, task_id: i64, pid: i64) -> bool {
    let doc = &p.document;
    let flag = doc
        .table("PROJECT")
        .and_then(|t| {
            t.rows
                .iter()
                .find(|r| num(t.value(r, "proj_id")) == pid)
                .map(|r| t.value(r, "step_complete_flag") == "Y")
        })
        .unwrap_or(false);
    flag && records::task_value(doc, task_id, pid, "complete_pct_type") == "CP_Phys"
}

/// Assign a code value, replacing any value the activity has for the same
/// code type.
pub fn assign_code(p: &mut ProjectData, task_id: i64, type_id: i64, code_id: i64) {
    let pid = p.model.project.proj_id;
    let doc = &mut p.document;
    let mut existing = records::rows_for(doc, "TASKACTV", task_id, pid);
    existing.retain(|&r| num(&records::get(doc, "TASKACTV", r, "actv_code_type_id")) == type_id);
    for r in existing.into_iter().rev() {
        records::remove_row(doc, "TASKACTV", r);
    }
    records::add_row(
        doc,
        "TASKACTV",
        &[
            ("task_id", task_id.to_string()),
            ("actv_code_type_id", type_id.to_string()),
            ("actv_code_id", code_id.to_string()),
            ("proj_id", pid.to_string()),
        ],
    );
}

/// Why field `lock` of activity `i` cannot change now, if it cannot.
pub fn field_lock(p: &ProjectData, i: usize, lock: Lock) -> Result<(), String> {
    let Some(a) = p.model.activities.get(i) else {
        return Err("No such activity.".into());
    };
    let (task_id, pid) = (a.task_id, p.model.project.proj_id);
    let driven = p
        .details
        .get(&task_id)
        .is_some_and(|d| d.resource_driven_duration);
    let done = a.is_completed();
    let percent_type = || records::task_value(&p.document, task_id, pid, "complete_pct_type");
    let duration_percent = |what: &str| -> Result<(), String> {
        if driven {
            Err("Duration is controlled by resource units/rates.".into())
        } else if a.status == StatusType::NotStarted {
            Err(format!("Start the activity before entering its {what}."))
        } else if done {
            Err(format!("{what} cannot change on a completed activity."))
        } else {
            Ok(())
        }
    };
    match lock {
        Lock::OriginalDuration if driven => {
            Err("Duration is controlled by resource units/rates.".into())
        }
        Lock::RemainingDuration | Lock::AtCompletionDuration if driven => {
            Err("Duration is controlled by resource units/rates.".into())
        }
        Lock::RemainingDuration => {
            if done {
                Err("Remaining Duration cannot change on a completed activity.".into())
            } else {
                Ok(())
            }
        }
        Lock::AtCompletionDuration if done => {
            Err("At Completion Duration cannot change on a completed activity.".into())
        }
        Lock::ActualDuration if !done => {
            Err("Actual Duration can change only on a completed activity.".into())
        }
        Lock::ExpectedFinish if done => {
            Err("Expected Finish cannot change on a completed activity.".into())
        }
        Lock::Finish if driven && !done => {
            Err("Duration is controlled by resource units/rates.".into())
        }
        Lock::PhysicalPercent if steps_drive_percent(p, task_id, pid) => {
            Err("Physical % Complete comes from the activity's steps.".into())
        }
        Lock::DurationPercent => duration_percent("Duration % Complete"),
        Lock::ActivityPercent => match percent_type().as_str() {
            "CP_Phys" => field_lock(p, i, Lock::PhysicalPercent),
            "CP_Units" => {
                Err("Activity % Complete comes from the resource assignments' units.".into())
            }
            _ => duration_percent("Activity % Complete"),
        },
        Lock::Figure(measure, phase) => figure_lock(p, i, measure, Some(phase)),
        Lock::ThisPeriod(measure) => figure_lock(p, i, measure, None),
        Lock::SuspendDate | Lock::ResumeDate
            if !(matches!(a.task_type, TaskType::Task | TaskType::Resource)
                && a.status == StatusType::InProgress) =>
        {
            Err(
                "Suspend and Resume apply to in-progress task and resource dependent activities."
                    .into(),
            )
        }
        Lock::PrimaryConstraintDate if matches!(a.cstr, CstrType::None | CstrType::Alap) => {
            Err("Assign a primary constraint that takes a date first.".into())
        }
        Lock::SecondaryConstraintDate if matches!(a.cstr2, CstrType::None | CstrType::Alap) => {
            Err("Assign a secondary constraint that takes a date first.".into())
        }
        _ => Ok(()),
    }
}

/// Apply one change to activity `i` with the shared rules, durations in
/// messages in hours. On `Err` nothing changed.
pub fn edit_activity(p: &mut ProjectData, i: usize, edit: Edit) -> Result<Outcome, String> {
    edit_activity_with(p, i, edit, hours_text)
}

/// [`edit_activity`] with the caller's duration text for messages.
pub fn edit_activity_with(
    p: &mut ProjectData,
    i: usize,
    edit: Edit,
    duration_text: DurationText,
) -> Result<Outcome, String> {
    if i >= p.model.activities.len() {
        return Err("No such activity.".into());
    }
    field_lock(p, i, edit.lock())?;
    let a = &p.model.activities[i];
    let (status, act_start, act_finish, rem) = (a.status, a.act_start, a.act_finish, a.rem);
    let model_before = a.clone();
    let task_id = a.task_id;
    let detail_before = p.details.get(&task_id).cloned();
    let mut ed = Setter {
        p,
        touched: false,
        duration_text,
    };
    let original_before = ed.original_of(i);
    // Unit and cost edits already went through the assignment setters,
    // which settle the activity's durations themselves.
    let units_edit = matches!(edit, Edit::Figure { .. } | Edit::ThisPeriod { .. });
    // A duration percent with the project's "recalculate actual units and
    // cost" option on also sets the assignments' units (see below).
    let pid = ed.p.model.project.proj_id;
    let kind = records::task_value(&ed.p.document, task_id, pid, "complete_pct_type");
    let recalc = match &edit {
        Edit::DurationPercent(v) => Some(*v),
        Edit::ActivityPercent(v) if kind != "CP_Phys" => Some(*v),
        _ => None,
    }
    .filter(|_| ed.p.project_option("act_pct_link_flag") == "Y");
    ed.apply(i, edit)?;
    let mut warning = None;
    let a = &ed.p.model.activities[i];
    let progress_changed =
        status != a.status || act_start != a.act_start || act_finish != a.act_finish;
    // A reopened activity's assignments take its new remaining duration too.
    let rem_changed = rem != a.rem && (a.status == status || status == StatusType::Completed);
    let rem = a.rem;
    let original = ed.original_of(i);
    let original_changed = original != original_before;
    let now = ed.p.model.activities[i].status;
    // Work that starts or finishes, or whose original duration changes,
    // takes its planned finish from its planned start and original duration
    // on its calendar as it is now (a calendar edited since the plan was
    // made moves it).
    let replan = (now != status && now != StatusType::NotStarted) || original_changed;
    let planned = ed.p.model.activities[i].target_start;
    if let (Some(start), true) = (planned, replan && original > 0) {
        let finish = calendar_add(ed.p, i, start, original);
        ed.p.model.activities[i].target_finish = Some(finish);
    }
    // The assignments' actual dates follow the activity's.
    let a = &ed.p.model.activities[i];
    let (new_start, new_finish) = (a.act_start, a.act_finish);
    if new_start != act_start || new_finish != act_finish {
        let origin = ed.p.model.origin_serial;
        for row in ed.p.assignment_rows(task_id) {
            if new_start != act_start {
                records::set(
                    &mut ed.p.document,
                    "TASKRSRC",
                    row,
                    "act_start_date",
                    date_string(origin, new_start),
                );
            }
            if new_finish != act_finish {
                records::set(
                    &mut ed.p.document,
                    "TASKRSRC",
                    row,
                    "act_end_date",
                    date_string(origin, new_finish),
                );
            }
            ed.touched = true;
        }
    }
    if now != status {
        // A finished activity shows no remaining units.
        if !ed.p.assignment_rows(task_id).is_empty() {
            ed.p.sync_task_rollups(task_id);
            ed.touched = true;
        }
    }
    if !units_edit && (original_changed || rem_changed) && !ed.p.assignment_rows(task_id).is_empty()
    {
        // Assignments follow the activity's duration by its duration type.
        match ed.p.activity_duration_changed(
            task_id,
            original_changed.then_some(original),
            rem_changed.then_some(rem),
        ) {
            Ok(()) => ed.touched = true,
            Err(e) => warning = Some(e),
        }
    }
    if let Some(percent) = recalc {
        if ed.p.actuals_from_duration_percent(task_id, percent) {
            ed.touched = true;
        }
    }
    // An activity dated but never scheduled (a new one) keeps the dates of
    // an activity without logic as its duration or calendar changes, so its
    // bar stays right until the next schedule run.
    let a = &ed.p.model.activities[i];
    if (a.rem != model_before.rem || a.cal != model_before.cal)
        && a.early_start.is_some()
        && a.late_start.is_none()
        && a.total_float.is_none()
    {
        ed.p.date_unscheduled(&[i]);
    }
    let changed = ed.p.model.activities[i] != model_before
        || ed.p.details.get(&task_id) != detail_before.as_ref()
        || ed.touched;
    Ok(Outcome {
        changed,
        records_changed: ed.touched,
        progress_changed,
        warning,
    })
}

/// The project being edited, with whether a record table changed.
struct Setter<'a> {
    p: &'a mut ProjectData,
    touched: bool,
    duration_text: DurationText,
}

/// The actual start that checking Started records, and the date its
/// unchecked box shows: the planned start (early start when there is none),
/// but never later than the data date, since an actual cannot be in the
/// future. (A typed actual date is taken as typed.)
pub fn default_actual_start(p: &ProjectData, i: usize) -> i64 {
    let dd = p.model.project.data_date;
    let a = &p.model.activities[i];
    let planned = a.target_start.or(a.early_start).or(dd).unwrap_or(0);
    dd.map_or(planned, |dd| planned.min(dd))
}

/// The actual finish that checking Finished records, and the date its
/// unchecked box shows: the planned finish (early finish when there is
/// none), never later than the data date and never before the actual start.
pub fn default_actual_finish(p: &ProjectData, i: usize) -> i64 {
    let a = &p.model.activities[i];
    let start = a.act_start.unwrap_or_else(|| default_actual_start(p, i));
    let planned = a
        .target_finish
        .or(a.early_finish)
        .unwrap_or_else(|| calendar_add(p, i, start, original_duration(p, i)));
    p.model
        .project
        .data_date
        .map_or(planned, |dd| planned.min(dd))
        .max(start)
}

fn original_duration(p: &ProjectData, i: usize) -> i64 {
    let task_id = p.model.activities[i].task_id;
    p.details.get(&task_id).map_or(0, |d| d.original_duration)
}

/// `start` moved by `minutes` of work on the activity's calendar. A calendar
/// with no recurring work could never place the work (the walk would not
/// end), so elapsed time is used, as for a missing calendar; scheduling
/// refuses such a calendar with a message.
fn calendar_add(p: &ProjectData, i: usize, start: i64, minutes: i64) -> i64 {
    let a = &p.model.activities[i];
    p.model
        .calendars
        .get(a.cal)
        .filter(|cal| openxer_engine::model::has_recurring_work(&cal.cal))
        .map_or(start + minutes, |cal| cal.add(start, minutes))
}

impl Setter<'_> {
    /// Working time from `start` to `finish` (or the data date) on the
    /// activity's calendar.
    fn elapsed(&self, i: usize, start: Option<i64>, finish: Option<i64>) -> i64 {
        let a = &self.p.model.activities[i];
        let finish = finish.or(self.p.model.project.data_date);
        match (start, finish, self.p.model.calendars.get(a.cal)) {
            (Some(s), Some(f), Some(cal)) if f > s => cal.between(s, f).max(0),
            _ => 0,
        }
    }

    fn calendar_add(&self, i: usize, start: i64, minutes: i64) -> i64 {
        calendar_add(self.p, i, start, minutes)
    }

    fn calendar_between(&self, i: usize, start: i64, finish: i64) -> i64 {
        let a = &self.p.model.activities[i];
        self.p
            .model
            .calendars
            .get(a.cal)
            .map_or(finish - start, |cal| cal.between(start, finish))
            .max(0)
    }

    fn original_of(&self, i: usize) -> i64 {
        original_duration(self.p, i)
    }

    /// Move a not-started activity's Start and Planned Start to `v`, the
    /// finish following by the remaining duration and the planned finish by
    /// the original one. With `hold`, a Start On or After constraint on `v`
    /// keeps it there at the next schedule (refused, changing nothing, when
    /// another constraint type is set).
    fn move_start(&mut self, i: usize, v: i64, hold: bool) -> Result<(), String> {
        if hold {
            let a = &mut self.p.model.activities[i];
            match a.cstr {
                CstrType::None | CstrType::StartOnAfter => {
                    a.cstr = CstrType::StartOnAfter;
                    a.cstr_date = Some(v);
                }
                other => {
                    return Err(format!(
                        "The activity has a {} constraint; change its constraint date instead.",
                        constraint_label(other)
                    ))
                }
            }
        }
        let original = self.original_of(i);
        let rem = self.p.model.activities[i].rem;
        let (finish, planned) = (
            self.calendar_add(i, v, rem),
            self.calendar_add(i, v, original),
        );
        let a = &mut self.p.model.activities[i];
        a.early_start = Some(v);
        a.early_finish = Some(finish);
        a.target_start = Some(v);
        a.target_finish = Some(planned);
        Ok(())
    }

    fn ids(&self, i: usize) -> (i64, i64) {
        (
            self.p.model.activities[i].task_id,
            self.p.model.project.proj_id,
        )
    }

    /// Set the status with the Started / Finished rules of the Status tab.
    fn set_status(&mut self, i: usize, status: StatusType) {
        let default_start = default_actual_start(self.p, i);
        let original = self.original_of(i);
        let current = self.p.model.activities[i].status;
        if current == status {
            return;
        }
        match status {
            StatusType::NotStarted => {
                let a = &mut self.p.model.activities[i];
                a.status = StatusType::NotStarted;
                a.act_start = None;
                a.act_finish = None;
                a.rem = original;
            }
            StatusType::InProgress if current == StatusType::Completed => {
                let start = self.p.model.activities[i].act_start;
                let left = original - self.elapsed(i, start, None);
                let a = &mut self.p.model.activities[i];
                a.status = StatusType::InProgress;
                a.act_finish = None;
                a.rem = if left > 0 { left } else { original };
            }
            StatusType::InProgress => {
                let a = &mut self.p.model.activities[i];
                a.status = StatusType::InProgress;
                a.act_start = a.act_start.or(Some(default_start));
            }
            StatusType::Completed => {
                let start = self.p.model.activities[i]
                    .act_start
                    .unwrap_or(default_start);
                let finish = default_actual_finish(self.p, i);
                let a = &mut self.p.model.activities[i];
                a.status = StatusType::Completed;
                a.act_start = Some(start);
                a.act_finish = Some(finish);
                a.rem = 0;
            }
        }
    }

    fn set_original(&mut self, i: usize, minutes: i64) {
        let task_id = self.p.model.activities[i].task_id;
        self.p.details.entry(task_id).or_default().original_duration = minutes;
        // Until the activity starts its remaining duration is its original.
        let a = &mut self.p.model.activities[i];
        if a.status == StatusType::NotStarted {
            a.rem = minutes;
        }
    }

    fn set_remaining(&mut self, i: usize, minutes: i64) {
        let a = &mut self.p.model.activities[i];
        a.rem = minutes;
        if a.status == StatusType::NotStarted {
            let task_id = a.task_id;
            self.p.details.entry(task_id).or_default().original_duration = minutes;
        }
    }

    /// Set a raw TASK column, noting a change.
    fn set_task_column(&mut self, task_id: i64, pid: i64, column: &str, value: String) {
        let doc = &mut self.p.document;
        if records::task_value(doc, task_id, pid, column) != value {
            records::set_task_value(doc, task_id, pid, column, value);
            self.touched = true;
        }
    }

    /// Set one unit or cost figure of activity `i` (`phase` `None` is Actual
    /// This Period). On `Err` nothing changed.
    fn apply_units(
        &mut self,
        i: usize,
        measure: Measure,
        phase: Option<Phase>,
        value: f64,
    ) -> Result<(), String> {
        let name = figure_name(measure, phase);
        if !(0. ..1.0e12).contains(&value) {
            return Err(format!("{name} must be zero or more."));
        }
        match units_route(self.p, i, measure) {
            Route::Locked(reason) => Err(reason),
            Route::Own(slot) => {
                let mut value = value;
                let phase = match phase {
                    Some(Phase::Budgeted) => UnitsPhase::Budgeted,
                    Some(Phase::Actual) => UnitsPhase::Actual,
                    Some(Phase::Remaining) => UnitsPhase::Remaining,
                    // The remaining units take At Completion less Actual.
                    Some(Phase::AtCompletion) => {
                        figure_lock(self.p, i, measure, phase)?;
                        let task_id = self.p.model.activities[i].task_id;
                        let actual = self.p.activity_own_units(task_id, slot)[1];
                        if value + 1e-9 < actual {
                            return Err(format!(
                                "{name} cannot be less than the actual units ({}).",
                                format_number(actual)
                            ));
                        }
                        value = (value - actual).max(0.);
                        UnitsPhase::Remaining
                    }
                    None => UnitsPhase::ThisPeriod,
                };
                if self.p.edit_activity_units(i, slot, phase, value)? {
                    self.touched = true;
                }
                Ok(())
            }
            Route::Assignment(row) => {
                let view = self
                    .p
                    .assignment_view(row)
                    .ok_or("The resource assignment no longer exists.")?;
                let cost = measure.is_cost();
                // Actual figures are regular plus overtime on the assignment.
                let (overtime, label) = if cost {
                    (view.actual_overtime_cost, "cost")
                } else {
                    (view.actual_overtime_units, "units")
                };
                let regular = || -> Result<f64, String> {
                    if value + 1e-9 < overtime {
                        Err(format!(
                            "{name} cannot be less than the overtime {label} ({}).",
                            format_number(overtime)
                        ))
                    } else {
                        Ok((value - overtime).max(0.))
                    }
                };
                let edit = match (cost, phase) {
                    (false, Some(Phase::Budgeted)) => AssignmentEdit::BudgetedUnits(value),
                    (false, Some(Phase::Remaining)) => AssignmentEdit::RemainingUnits(value),
                    (false, Some(Phase::Actual)) => AssignmentEdit::ActualRegularUnits(regular()?),
                    (false, None) => AssignmentEdit::ThisPeriodUnits(value),
                    (true, Some(Phase::Budgeted)) => AssignmentEdit::BudgetedCost(value),
                    (true, Some(Phase::Remaining)) => AssignmentEdit::RemainingCost(value),
                    (true, Some(Phase::Actual)) => AssignmentEdit::ActualRegularCost(regular()?),
                    (true, None) => AssignmentEdit::ThisPeriodCost(value),
                    (false, Some(Phase::AtCompletion)) => {
                        figure_lock(self.p, i, measure, phase)?;
                        AssignmentEdit::AtCompletionUnits(value)
                    }
                    (true, Some(Phase::AtCompletion)) => {
                        figure_lock(self.p, i, measure, phase)?;
                        AssignmentEdit::AtCompletionCost(value)
                    }
                };
                self.p.edit_assignment(row, edit)?;
                self.touched = true;
                Ok(())
            }
        }
    }
    fn apply(&mut self, i: usize, edit: Edit) -> Result<(), String> {
        let (task_id, pid) = self.ids(i);
        let origin = self.p.model.origin_serial;
        // The range in the unit the caller shows durations in.
        let range = (
            (self.duration_text)(self.p, i, 0),
            (self.duration_text)(self.p, i, MAX_DURATION),
        );
        let duration = |m: i64, what: &str| -> Result<i64, String> {
            if (0..=MAX_DURATION).contains(&m) {
                Ok(m)
            } else {
                Err(format!("{what} must be from {} to {}.", range.0, range.1))
            }
        };
        let percent = |p: f64, what: &str| -> Result<f64, String> {
            if p.is_finite() && (0.0..=100.).contains(&p) {
                Ok(p)
            } else {
                Err(format!("{what} must be from 0 to 100."))
            }
        };
        let status = self.p.model.activities[i].status;
        match edit {
            Edit::ActivityType(t) => {
                let a = &self.p.model.activities[i];
                let clear_primary = crate::assignments::milestone_type_change(
                    self.p,
                    task_id,
                    &a.task_code,
                    a.task_type,
                    t,
                )?;
                self.p.model.activities[i].task_type = t;
                if clear_primary {
                    records::set_task_value(
                        &mut self.p.document,
                        task_id,
                        pid,
                        "rsrc_id",
                        "".into(),
                    );
                    self.touched = true;
                }
            }
            Edit::Status(s) => self.set_status(i, s),
            Edit::Wbs(id) => {
                if !self.p.wbs.iter().any(|w| w.id == id) {
                    return Err("No such WBS element in this project.".into());
                }
                self.p.details.entry(task_id).or_default().wbs = id;
            }
            Edit::Calendar(c) => {
                let current = self.p.model.activities[i].cal;
                if !super::calendars::assignable(self.p, current).contains(&c) {
                    return Err("That calendar cannot be assigned in this project.".into());
                }
                self.p.model.activities[i].cal = c;
            }
            Edit::PrimaryResource(id) => {
                let doc = &self.p.document;
                let assigned = doc.table("TASKRSRC").is_some_and(|t| {
                    records::rows_for(doc, "TASKRSRC", task_id, pid)
                        .into_iter()
                        .any(|r| num(t.value(&t.rows[r], "rsrc_id")) == id)
                });
                // A milestone has no assignments; its primary resource is
                // any resource of the file (the activity owner).
                let milestone =
                    crate::assignments::is_milestone(self.p.model.activities[i].task_type);
                let exists = doc
                    .table("RSRC")
                    .is_some_and(|t| t.rows.iter().any(|r| num(t.value(r, "rsrc_id")) == id));
                if id != 0 && milestone && !exists {
                    return Err(format!("Resource {id} is not in this file."));
                }
                if id != 0 && !milestone && !assigned {
                    return Err("The primary resource must be assigned to the activity.".into());
                }
                let value = if id == 0 {
                    String::new()
                } else {
                    id.to_string()
                };
                if records::task_value(doc, task_id, pid, "rsrc_id") != value {
                    records::set_task_value(&mut self.p.document, task_id, pid, "rsrc_id", value);
                    self.touched = true;
                }
            }
            Edit::DurationType(code) => {
                let doc = &mut self.p.document;
                if records::task_value(doc, task_id, pid, "duration_type") != code {
                    records::set_task_value(doc, task_id, pid, "duration_type", code.into());
                    let assigned = doc.table("TASKRSRC").is_some_and(|t| {
                        records::rows_for(doc, "TASKRSRC", task_id, pid)
                            .into_iter()
                            .any(|r| t.value(&t.rows[r], "rollup_dates_flag") == "Y")
                    });
                    if let Some(d) = self.p.details.get_mut(&task_id) {
                        d.resource_driven_duration =
                            matches!(code, "DT_FixedDrtn" | "DT_FixedRate" | "DT_FixedQty")
                                && assigned;
                    }
                    self.touched = true;
                }
            }
            Edit::PercentType(code) => {
                let doc = &mut self.p.document;
                if records::task_value(doc, task_id, pid, "complete_pct_type") != code {
                    records::set_task_value(doc, task_id, pid, "complete_pct_type", code.into());
                    self.touched = true;
                }
            }
            Edit::Priority(code) => {
                self.set_task_column(task_id, pid, "priority_type", code.into())
            }
            Edit::AutoComputeActuals(on) => {
                let flag = if on { "Y" } else { "N" };
                self.set_task_column(task_id, pid, "auto_compute_act_flag", flag.into())
            }
            Edit::Start(v) => match status {
                StatusType::NotStarted => self.move_start(i, v, true)?,
                _ => return self.apply(i, Edit::ActualStart(Some(v))),
            },
            Edit::StartUnheld(v) => match status {
                StatusType::NotStarted => self.move_start(i, v, false)?,
                _ => return self.apply(i, Edit::ActualStart(Some(v))),
            },
            Edit::Finish(v) => match status {
                StatusType::Completed => return self.apply(i, Edit::ActualFinish(Some(v))),
                StatusType::NotStarted => {
                    let a = &self.p.model.activities[i];
                    let start = a
                        .early_start
                        .or(a.target_start)
                        .or(self.p.model.project.data_date)
                        .unwrap_or(v);
                    if v <= start {
                        return Err("Finish must be later than Start.".into());
                    }
                    let m = duration(self.calendar_between(i, start, v), "Original Duration")?;
                    self.set_original(i, m);
                    // The start stays; the finish shows what was typed (on
                    // the calendar) until the next schedule.
                    let finish = self.calendar_add(i, start, m);
                    let a = &mut self.p.model.activities[i];
                    if a.early_start.is_some() {
                        a.early_finish = Some(finish);
                    }
                }
                StatusType::InProgress => {
                    let a = &self.p.model.activities[i];
                    let from = a
                        .restart
                        .or(self.p.model.project.data_date)
                        .or(a.act_start)
                        .unwrap_or(v);
                    if v < from {
                        return Err(
                            "Finish cannot be earlier than the remaining work's start.".into()
                        );
                    }
                    let m = duration(self.calendar_between(i, from, v), "Remaining Duration")?;
                    self.set_remaining(i, m);
                }
            },
            Edit::ActualStart(None) => self.set_status(i, StatusType::NotStarted),
            Edit::ActualStart(Some(v)) => {
                let a = &self.p.model.activities[i];
                if a.act_finish.is_some_and(|f| f < v) {
                    return Err("Actual Start must be on or before Actual Finish.".into());
                }
                let a = &mut self.p.model.activities[i];
                if a.status == StatusType::NotStarted {
                    a.status = StatusType::InProgress;
                }
                a.act_start = Some(v);
            }
            Edit::ActualFinish(None) => {
                if status == StatusType::Completed {
                    self.set_status(i, StatusType::InProgress);
                }
            }
            Edit::ActualFinish(Some(v)) => {
                let start = self.p.model.activities[i]
                    .act_start
                    .unwrap_or_else(|| default_actual_start(self.p, i).min(v));
                if v < start {
                    return Err("Actual Finish must be on or after Actual Start.".into());
                }
                let a = &mut self.p.model.activities[i];
                a.status = StatusType::Completed;
                a.act_start = Some(start);
                a.act_finish = Some(v);
                a.rem = 0;
            }
            Edit::PlannedStart(v) => {
                let a = &self.p.model.activities[i];
                let span = match (a.target_start, a.target_finish) {
                    (Some(s), Some(f)) => self.calendar_between(i, s, f),
                    _ => self.original_of(i),
                };
                let finish = self.calendar_add(i, v, span);
                let a = &mut self.p.model.activities[i];
                a.target_start = Some(v);
                a.target_finish = Some(finish);
            }
            Edit::PlannedFinish(v) => {
                let start = self.p.model.activities[i].target_start;
                if start.is_some_and(|s| v < s) {
                    return Err("Planned Finish must be on or after Planned Start.".into());
                }
                let original = match (start, status) {
                    (Some(s), StatusType::NotStarted) => Some(duration(
                        self.calendar_between(i, s, v),
                        "Original Duration",
                    )?),
                    _ => None,
                };
                self.p.model.activities[i].target_finish = Some(v);
                if let Some(m) = original {
                    self.set_original(i, m);
                }
            }
            Edit::MoveStart(v) => {
                if status != StatusType::NotStarted {
                    return Err("Only an activity that has not started can be moved.".into());
                }
                // The same hold a typed Start adds (Edit::Start).
                self.move_start(i, v, true)?;
            }
            Edit::Figure {
                measure,
                phase,
                value,
            } => self.apply_units(i, measure, Some(phase), value)?,
            Edit::ThisPeriod { measure, value } => self.apply_units(i, measure, None, value)?,
            Edit::BarDuration(m) => {
                if status != StatusType::NotStarted {
                    return Err("Only an activity that has not started can be stretched.".into());
                }
                let m = duration(m, "Original Duration")?;
                self.set_original(i, m);
                let a = &self.p.model.activities[i];
                let start = a
                    .early_start
                    .or(a.target_start)
                    .or(self.p.model.project.data_date);
                if let Some(start) = start {
                    let finish = self.calendar_add(i, start, m);
                    let a = &mut self.p.model.activities[i];
                    a.early_finish = Some(finish);
                    a.target_finish = Some(finish);
                }
            }
            Edit::ExpectedFinish(v) => self.p.model.activities[i].expect_end = v,
            Edit::SuspendDate(v) => {
                if let Some(s) = v {
                    let a = &self.p.model.activities[i];
                    if a.act_start.is_some_and(|start| s < start) {
                        return Err("The suspend date cannot be before the actual start.".into());
                    }
                    if a.resume.is_some_and(|r| r <= s) {
                        return Err("The suspend date must be before the resume date.".into());
                    }
                }
                let doc = &mut self.p.document;
                let text = date_string(origin, v);
                if records::task_value(doc, task_id, pid, "suspend_date") != text {
                    records::set_task_value(doc, task_id, pid, "suspend_date", text);
                    self.touched = true;
                }
            }
            Edit::ResumeDate(v) => {
                if let Some(r) = v {
                    let suspend = crate::parse_relative(
                        origin,
                        &records::task_value(&self.p.document, task_id, pid, "suspend_date"),
                    )
                    .ok()
                    .flatten();
                    if suspend.is_some_and(|s| r <= s) {
                        return Err("The resume date must be after the suspend date.".into());
                    }
                }
                self.p.model.activities[i].resume = v;
            }
            Edit::OriginalDuration(m) => {
                let m = duration(m, "Original Duration")?;
                self.set_original(i, m);
            }
            Edit::RemainingDuration(m) => {
                let m = duration(m, "Remaining Duration")?;
                self.set_remaining(i, m);
            }
            Edit::AtCompletionDuration(m) => {
                let m = duration(m, "At Completion Duration")?;
                if status == StatusType::NotStarted {
                    self.set_original(i, m);
                } else {
                    let start = self.p.model.activities[i].act_start;
                    let actual = self.elapsed(i, start, None);
                    if m < actual {
                        return Err(format!(
                            "At Completion Duration cannot be less than the Actual Duration ({}).",
                            (self.duration_text)(self.p, i, actual)
                        ));
                    }
                    self.set_remaining(i, m - actual);
                }
            }
            Edit::ActualDuration(m) => {
                let m = duration(m, "Actual Duration")?;
                let start = self.p.model.activities[i]
                    .act_start
                    .ok_or("The activity has no actual start.")?;
                let finish = self.calendar_add(i, start, m);
                self.p.model.activities[i].act_finish = Some(finish);
            }
            Edit::PhysicalPercent(p) => {
                let p = percent(p, "Physical % Complete")?;
                self.p.details.entry(task_id).or_default().percent = p;
            }
            Edit::DurationPercent(p) => {
                let p = percent(p, "Duration % Complete")?;
                let original = self.original_of(i);
                let left = (original as f64 * (1. - p / 100.)).round() as i64;
                self.p.model.activities[i].rem = left.clamp(0, original.max(0));
            }
            Edit::ActivityPercent(p) => {
                let kind = records::task_value(&self.p.document, task_id, pid, "complete_pct_type");
                return if kind == "CP_Phys" {
                    self.apply(i, Edit::PhysicalPercent(p))
                } else {
                    let p = percent(p, "Activity % Complete")?;
                    self.apply(i, Edit::DurationPercent(p))
                };
            }
            Edit::Constraint { secondary, kind } => {
                let a = &self.p.model.activities[i];
                if secondary && kind != CstrType::None && a.cstr == CstrType::None {
                    return Err("Assign a primary constraint first.".into());
                }
                let current = if secondary { a.cstr_date2 } else { a.cstr_date };
                let fallback = current.or(a.cstr_date).or(a.early_start).or(self
                    .p
                    .model
                    .project
                    .data_date);
                let date = if matches!(kind, CstrType::None | CstrType::Alap) {
                    None
                } else {
                    fallback
                };
                let a = &mut self.p.model.activities[i];
                if secondary {
                    a.cstr2 = kind;
                    a.cstr_date2 = date;
                } else {
                    a.cstr = kind;
                    a.cstr_date = date;
                    if kind == CstrType::None {
                        a.cstr2 = CstrType::None;
                        a.cstr_date2 = None;
                    }
                }
            }
            Edit::ConstraintDate { secondary, date } => {
                let date = date.ok_or("A constraint needs a date.")?;
                let a = &mut self.p.model.activities[i];
                if secondary {
                    a.cstr_date2 = Some(date);
                } else {
                    a.cstr_date = Some(date);
                }
            }
            Edit::Code { type_id, value } => match value {
                Some(code) => {
                    let current = {
                        let doc = &self.p.document;
                        records::rows_for(doc, "TASKACTV", task_id, pid)
                            .into_iter()
                            .filter(|&r| {
                                num(&records::get(doc, "TASKACTV", r, "actv_code_type_id"))
                                    == type_id
                            })
                            .map(|r| num(&records::get(doc, "TASKACTV", r, "actv_code_id")))
                            .collect::<Vec<_>>()
                    };
                    if current != [code] {
                        assign_code(self.p, task_id, type_id, code);
                        self.touched = true;
                    }
                }
                None => {
                    let doc = &mut self.p.document;
                    let mut rows = records::rows_for(doc, "TASKACTV", task_id, pid);
                    rows.retain(|&r| {
                        num(&records::get(doc, "TASKACTV", r, "actv_code_type_id")) == type_id
                    });
                    if !rows.is_empty() {
                        for r in rows.into_iter().rev() {
                            records::remove_row(doc, "TASKACTV", r);
                        }
                        self.touched = true;
                    }
                }
            },
            Edit::Udf { type_id, value } => {
                if udf::set_value(&mut self.p.document, origin, pid, type_id, task_id, value)? {
                    self.touched = true;
                }
            }
        }
        Ok(())
    }
}

/// Every constraint type, in the order the constraint choice lists them.
pub const CONSTRAINTS: [CstrType; 10] = [
    CstrType::None,
    CstrType::StartOn,
    CstrType::StartOnBefore,
    CstrType::StartOnAfter,
    CstrType::Alap,
    CstrType::FinishOn,
    CstrType::FinishOnBefore,
    CstrType::FinishOnAfter,
    CstrType::MandatoryStart,
    CstrType::MandatoryFinish,
];

/// Activity types a user can choose.
pub const TASK_TYPES: [TaskType; 6] = [
    TaskType::Task,
    TaskType::Resource,
    TaskType::Loe,
    TaskType::Mile,
    TaskType::FinMile,
    TaskType::Wbs,
];

/// An activity type from its XER code (`TT_Task`) or label
/// (`Task Dependent`), ignoring case.
pub fn parse_task_type(text: &str) -> Option<TaskType> {
    let text = text.trim();
    TASK_TYPES.into_iter().find(|t| {
        t.as_str().eq_ignore_ascii_case(text) || crate::type_label(*t).eq_ignore_ascii_case(text)
    })
}

/// A constraint type from its XER code (`CS_MSOA`) or label (`Start On or
/// After`), ignoring case. Empty text is no constraint.
pub fn parse_constraint(text: &str) -> Option<CstrType> {
    let text = text.trim();
    if text.is_empty() {
        return Some(CstrType::None);
    }
    CONSTRAINTS.into_iter().find(|c| {
        crate::constraint_code(*c).eq_ignore_ascii_case(text)
            || constraint_label(*c).eq_ignore_ascii_case(text)
    })
}

/// An activity status from its XER code (`TK_Active`) or label.
pub fn parse_status(text: &str) -> Option<StatusType> {
    let text = text.trim();
    let code = |s: StatusType| match s {
        StatusType::NotStarted => "TK_NotStart",
        StatusType::InProgress => "TK_Active",
        StatusType::Completed => "TK_Complete",
    };
    STATUSES.into_iter().find(|s| {
        code(*s).eq_ignore_ascii_case(text) || crate::status_label(*s).eq_ignore_ascii_case(text)
    })
}

/// Set the Activity ID: nonempty and unique in the project. Returns whether
/// it changed.
pub fn set_activity_id(p: &mut ProjectData, i: usize, text: &str) -> Result<bool, String> {
    let text = text.trim();
    if i >= p.model.activities.len() {
        return Err("No such activity.".into());
    }
    if text.is_empty()
        || p.model
            .activities
            .iter()
            .enumerate()
            .any(|(j, a)| j != i && a.task_code == text)
    {
        return Err("Activity ID must be nonempty and unique.".into());
    }
    let a = &mut p.model.activities[i];
    let changed = a.task_code != text;
    a.task_code = text.into();
    Ok(changed)
}

/// Set the activity name. Returns whether it changed.
pub fn set_activity_name(p: &mut ProjectData, i: usize, text: &str) -> Result<bool, String> {
    let text = text.trim();
    let Some(a) = p.model.activities.get(i) else {
        return Err("No such activity.".into());
    };
    let d = p.details.entry(a.task_id).or_default();
    let changed = d.name != text;
    d.name = text.into();
    Ok(changed)
}
