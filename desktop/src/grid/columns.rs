//! Column registry for the activity table: every column the table, filters,
//! group and sort, find/replace and exports can use, with its title, default
//! width, alignment, value getter, sort order and editor.
//!
//! Values are typed (`Value`); `display` is the single place that turns a
//! value into table text and `parse_input` the single place that reads typed
//! text back, so the user's time unit and date format preferences change
//! both in one spot.
use super::*;
use crate::details::schedule_tabs::{DURATION_TYPES, PERCENT_TYPES};
use crate::format;
use crate::records::{self, num};
use openxer_xer::edit::fields as shared;
use std::cell::OnceCell;
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}
impl Align {
    pub fn key(self) -> &'static str {
        match self {
            Align::Left => "left",
            Align::Center => "center",
            Align::Right => "right",
        }
    }
    pub fn from_key(key: &str) -> Option<Self> {
        [Align::Left, Align::Center, Align::Right]
            .into_iter()
            .find(|a| a.key() == key)
    }
}

/// The alignment a column of `kind` starts with: text and dates Left,
/// flags Center, numbers Right. Edit Column
/// can set any alignment; a layout stores only an alignment that differs
/// from this default.
pub fn default_align(kind: Kind) -> Align {
    match kind {
        Kind::Text | Kind::Date => Align::Left,
        Kind::Flag => Align::Center,
        _ => Align::Right,
    }
}

/// What a column holds and how a cell is edited in place: the shared
/// field catalog's kinds (`openxer_xer::edit::fields`).
pub use openxer_xer::edit::fields::{Editor, Kind};

pub use openxer_xer::edit::activity::{Measure, Phase};

/// Built-in activity fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Field {
    ActivityId,
    ActivityName,
    ActivityType,
    ActivityStatus,
    Wbs,
    WbsName,
    Calendar,
    PrimaryResource,
    DurationType,
    PercentType,
    /// The activity's Auto Compute Actuals flag, a check box in the cell.
    AutoComputeActuals,
    Critical,
    LongestPath,
    Start,
    Finish,
    EarlyStart,
    EarlyFinish,
    LateStart,
    LateFinish,
    ActualStart,
    ActualFinish,
    PlannedStart,
    PlannedFinish,
    RemainingEarlyStart,
    RemainingEarlyFinish,
    RemainingLateStart,
    RemainingLateFinish,
    ExpectedFinish,
    PrimaryConstraint,
    PrimaryConstraintDate,
    SecondaryConstraint,
    SecondaryConstraintDate,
    SuspendDate,
    ResumeDate,
    OriginalDuration,
    ActualDuration,
    RemainingDuration,
    AtCompletionDuration,
    DurationPercent,
    TotalFloat,
    FreeFloat,
    FloatPath,
    FloatPathOrder,
    ActivityPercent,
    PhysicalPercent,
    Predecessors,
    Successors,
    /// Each predecessor as `<Activity ID>: <type>`, then the lag when it
    /// is not zero, in the user's duration unit without its letter.
    PredecessorDetails,
    /// The same for each successor.
    SuccessorDetails,
    Figure(Measure, Phase),
    /// Actual units or cost this period (`Measure::editable` ones only).
    ThisPeriod(Measure),
    /// A baseline date, duration or variance (`crate::baselines::columns`).
    Baseline(crate::baselines::BlField),
    /// Earned value, percent complete, count, step, feedback and other
    /// columns (`super::columns_ext`).
    Ext(super::columns_ext::Ext),
}

impl Field {
    /// The same field in the shared catalog, which reads and writes it;
    /// `None` for the columns only the table has (relationship details,
    /// baselines and the derived columns of `columns_ext`).
    pub fn shared(self) -> Option<shared::Field> {
        use shared::Field as S;
        Some(match self {
            Field::ActivityId => S::ActivityId,
            Field::ActivityName => S::ActivityName,
            Field::ActivityType => S::ActivityType,
            Field::ActivityStatus => S::ActivityStatus,
            Field::Wbs => S::Wbs,
            Field::WbsName => S::WbsName,
            Field::Calendar => S::Calendar,
            Field::PrimaryResource => S::PrimaryResource,
            Field::DurationType => S::DurationType,
            Field::PercentType => S::PercentType,
            Field::AutoComputeActuals => S::AutoComputeActuals,
            Field::Critical => S::Critical,
            Field::LongestPath => S::LongestPath,
            Field::Start => S::Start,
            Field::Finish => S::Finish,
            Field::EarlyStart => S::EarlyStart,
            Field::EarlyFinish => S::EarlyFinish,
            Field::LateStart => S::LateStart,
            Field::LateFinish => S::LateFinish,
            Field::ActualStart => S::ActualStart,
            Field::ActualFinish => S::ActualFinish,
            Field::PlannedStart => S::PlannedStart,
            Field::PlannedFinish => S::PlannedFinish,
            Field::RemainingEarlyStart => S::RemainingEarlyStart,
            Field::RemainingEarlyFinish => S::RemainingEarlyFinish,
            Field::RemainingLateStart => S::RemainingLateStart,
            Field::RemainingLateFinish => S::RemainingLateFinish,
            Field::ExpectedFinish => S::ExpectedFinish,
            Field::PrimaryConstraint => S::PrimaryConstraint,
            Field::PrimaryConstraintDate => S::PrimaryConstraintDate,
            Field::SecondaryConstraint => S::SecondaryConstraint,
            Field::SecondaryConstraintDate => S::SecondaryConstraintDate,
            Field::SuspendDate => S::SuspendDate,
            Field::ResumeDate => S::ResumeDate,
            Field::OriginalDuration => S::OriginalDuration,
            Field::ActualDuration => S::ActualDuration,
            Field::RemainingDuration => S::RemainingDuration,
            Field::AtCompletionDuration => S::AtCompletionDuration,
            Field::DurationPercent => S::DurationPercent,
            Field::TotalFloat => S::TotalFloat,
            Field::FreeFloat => S::FreeFloat,
            Field::FloatPath => S::FloatPath,
            Field::FloatPathOrder => S::FloatPathOrder,
            Field::ActivityPercent => S::ActivityPercent,
            Field::PhysicalPercent => S::PhysicalPercent,
            Field::Predecessors => S::Predecessors,
            Field::Successors => S::Successors,
            Field::Figure(measure, phase) => S::Figure(measure, phase),
            Field::ThisPeriod(measure) => S::ThisPeriod(measure),
            Field::PredecessorDetails
            | Field::SuccessorDetails
            | Field::Baseline(_)
            | Field::Ext(_) => return None,
        })
    }
}

/// Where a column's value comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Field(Field),
    /// Activity code type (`actv_code_type_id`).
    Code(i64),
    /// Activity user defined field (`udf_type_id`).
    Udf(i64),
    /// Notebook topic (`memo_type_id`).
    Notebook(i64),
}

/// One column the table can show.
#[derive(Clone, Debug, PartialEq)]
pub struct Column {
    /// Stable id stored in layouts (`activity_id`, `code:Area`, `udf:Crew`).
    pub id: String,
    /// The title as the column header shows it.
    pub title: String,
    /// The Available Options group in the Columns dialog.
    pub group: String,
    pub width: f32,
    pub align: Align,
    pub kind: Kind,
    pub editor: Editor,
    pub source: Source,
}

impl Source {
    /// The shared catalog's source of the same field; `None` for the
    /// columns only the table has (notebook topics among them).
    pub fn shared(self) -> Option<shared::Source> {
        match self {
            Source::Field(f) => f.shared().map(shared::Source::Field),
            Source::Code(id) => Some(shared::Source::Code(id)),
            Source::Udf(id) => Some(shared::Source::Udf(id)),
            Source::Notebook(_) => None,
        }
    }
}

impl Column {
    /// This column as a field of the shared catalog, which reads and
    /// writes it; `None` for a column only the table has.
    pub fn shared(&self) -> Option<shared::FieldDef> {
        Some(shared::FieldDef {
            id: self.id.clone(),
            title: self.title.clone(),
            kind: self.kind,
            editor: self.editor,
            source: self.source.shared()?,
        })
    }
}

/// The user's display formats (`crate::format`), as the shared catalog
/// shows and reads values in them.
pub struct UserFormats;

impl shared::Formats for UserFormats {
    fn duration(&self, p: &ProjectData, activity: Option<usize>, minutes: i64) -> String {
        format::duration(minutes, periods(p, activity))
    }
    fn parse_duration(
        &self,
        p: &ProjectData,
        activity: Option<usize>,
        text: &str,
    ) -> Result<i64, String> {
        format::parse_duration(text, periods(p, activity))
    }
    fn date(&self, origin: i64, minute: i64) -> String {
        format::date(origin, Some(minute))
    }
    fn parse_date(
        &self,
        origin: i64,
        text: &str,
        keep: Option<i64>,
    ) -> Result<Option<i64>, String> {
        format::parse_date(origin, text, keep)
    }
    fn units(&self, hours: f64) -> String {
        format::units(hours)
    }
    fn parse_units(&self, text: &str) -> Result<f64, String> {
        parse_units(text)
    }
    fn cost(&self, value: f64) -> String {
        format::cost(value)
    }
}

pub const GENERAL: &str = "General";
pub const DATES: &str = "Dates";
pub const DURATIONS: &str = "Durations";
pub const FLOAT_PATHS: &str = "Multiple Float Paths";
pub const PERCENTS: &str = "Percent Completes";
pub const UNITS: &str = "Units";
pub const COSTS: &str = "Costs";
pub const LISTS: &str = "Lists";
pub const CODES: &str = "Activity Codes";
pub const USER_DEFINED: &str = "User Defined";
pub const NOTEBOOK: &str = "Notebook Topics";

fn spec(
    field: Field,
    id: &str,
    title: &str,
    group: &str,
    width: f32,
    kind: Kind,
    editor: Editor,
) -> Column {
    let align = default_align(kind);
    Column {
        id: id.into(),
        title: title.into(),
        group: group.into(),
        width,
        align,
        kind,
        editor,
        source: Source::Field(field),
    }
}

/// The built-in columns, in Available Options order within each group.
pub fn builtin() -> &'static [Column] {
    static COLUMNS: OnceLock<Vec<Column>> = OnceLock::new();
    COLUMNS.get_or_init(|| {
        use Editor as E;
        use Field as F;
        use Kind as K;
        let mut c = vec![
            spec(
                F::ActivityId,
                "activity_id",
                "Activity ID",
                GENERAL,
                90.,
                K::Text,
                E::Text,
            ),
            spec(
                F::ActivityName,
                "activity_name",
                "Activity Name",
                GENERAL,
                250.,
                K::Text,
                E::Text,
            ),
            spec(
                F::ActivityType,
                "activity_type",
                "Activity Type",
                GENERAL,
                110.,
                K::Text,
                E::Choice,
            ),
            spec(
                F::ActivityStatus,
                "activity_status",
                "Activity Status",
                GENERAL,
                85.,
                K::Text,
                E::Choice,
            ),
            spec(F::Wbs, "wbs", "WBS", GENERAL, 120., K::Text, E::Choice),
            spec(
                F::WbsName,
                "wbs_name",
                "WBS Name",
                GENERAL,
                160.,
                K::Text,
                E::ReadOnly,
            ),
            spec(
                F::Calendar,
                "calendar",
                "Calendar",
                GENERAL,
                140.,
                K::Text,
                E::Choice,
            ),
            spec(
                F::PrimaryResource,
                "primary_resource",
                "Primary Resource",
                GENERAL,
                120.,
                K::Text,
                E::Choice,
            ),
            spec(
                F::DurationType,
                "duration_type",
                "Duration Type",
                GENERAL,
                150.,
                K::Text,
                E::Choice,
            ),
            spec(
                F::PercentType,
                "percent_complete_type",
                "Percent Complete Type",
                GENERAL,
                90.,
                K::Text,
                E::Choice,
            ),
            spec(
                F::AutoComputeActuals,
                "auto_compute_actuals",
                "Auto Compute Actuals",
                GENERAL,
                80.,
                K::Flag,
                E::Choice,
            ),
            spec(
                F::Critical,
                "critical",
                "Critical",
                GENERAL,
                55.,
                K::Flag,
                E::ReadOnly,
            ),
            spec(
                F::LongestPath,
                "longest_path",
                "Longest Path",
                GENERAL,
                60.,
                K::Flag,
                E::ReadOnly,
            ),
            spec(F::Start, "start", "Start", DATES, 100., K::Date, E::Date),
            spec(F::Finish, "finish", "Finish", DATES, 100., K::Date, E::Date),
            spec(
                F::EarlyStart,
                "early_start",
                "Early Start",
                DATES,
                95.,
                K::Date,
                E::ReadOnly,
            ),
            spec(
                F::EarlyFinish,
                "early_finish",
                "Early Finish",
                DATES,
                95.,
                K::Date,
                E::ReadOnly,
            ),
            spec(
                F::LateStart,
                "late_start",
                "Late Start",
                DATES,
                95.,
                K::Date,
                E::ReadOnly,
            ),
            spec(
                F::LateFinish,
                "late_finish",
                "Late Finish",
                DATES,
                95.,
                K::Date,
                E::ReadOnly,
            ),
            spec(
                F::ActualStart,
                "actual_start",
                "Actual Start",
                DATES,
                95.,
                K::Date,
                E::Date,
            ),
            spec(
                F::ActualFinish,
                "actual_finish",
                "Actual Finish",
                DATES,
                95.,
                K::Date,
                E::Date,
            ),
            spec(
                F::PlannedStart,
                "planned_start",
                "Planned Start",
                DATES,
                95.,
                K::Date,
                E::Date,
            ),
            spec(
                F::PlannedFinish,
                "planned_finish",
                "Planned Finish",
                DATES,
                95.,
                K::Date,
                E::Date,
            ),
            spec(
                F::RemainingEarlyStart,
                "remaining_early_start",
                "Remaining Early Start",
                DATES,
                95.,
                K::Date,
                E::ReadOnly,
            ),
            spec(
                F::RemainingEarlyFinish,
                "remaining_early_finish",
                "Remaining Early Finish",
                DATES,
                95.,
                K::Date,
                E::ReadOnly,
            ),
            spec(
                F::RemainingLateStart,
                "remaining_late_start",
                "Remaining Late Start",
                DATES,
                95.,
                K::Date,
                E::ReadOnly,
            ),
            spec(
                F::RemainingLateFinish,
                "remaining_late_finish",
                "Remaining Late Finish",
                DATES,
                95.,
                K::Date,
                E::ReadOnly,
            ),
            spec(
                F::ExpectedFinish,
                "expected_finish",
                "Expected Finish",
                DATES,
                95.,
                K::Date,
                E::Date,
            ),
            spec(
                F::PrimaryConstraint,
                "primary_constraint",
                "Primary Constraint",
                DATES,
                130.,
                K::Text,
                E::Choice,
            ),
            spec(
                F::PrimaryConstraintDate,
                "primary_constraint_date",
                "Primary Constraint Date",
                DATES,
                95.,
                K::Date,
                E::Date,
            ),
            spec(
                F::SecondaryConstraint,
                "secondary_constraint",
                "Secondary Constraint",
                DATES,
                130.,
                K::Text,
                E::Choice,
            ),
            spec(
                F::SecondaryConstraintDate,
                "secondary_constraint_date",
                "Secondary Constraint Date",
                DATES,
                95.,
                K::Date,
                E::Date,
            ),
            spec(
                F::SuspendDate,
                "suspend_date",
                "Suspend Date",
                DATES,
                95.,
                K::Date,
                E::Date,
            ),
            spec(
                F::ResumeDate,
                "resume_date",
                "Resume Date",
                DATES,
                95.,
                K::Date,
                E::Date,
            ),
            spec(
                F::OriginalDuration,
                "original_duration",
                "Original Duration",
                DURATIONS,
                70.,
                K::Duration,
                E::Number,
            ),
            spec(
                F::ActualDuration,
                "actual_duration",
                "Actual Duration",
                DURATIONS,
                70.,
                K::Duration,
                E::Number,
            ),
            spec(
                F::RemainingDuration,
                "remaining_duration",
                "Remaining Duration",
                DURATIONS,
                70.,
                K::Duration,
                E::Number,
            ),
            spec(
                F::AtCompletionDuration,
                "at_completion_duration",
                "At Completion Duration",
                DURATIONS,
                70.,
                K::Duration,
                E::Number,
            ),
            spec(
                F::TotalFloat,
                "total_float",
                "Total Float",
                DURATIONS,
                65.,
                K::Duration,
                E::ReadOnly,
            ),
            spec(
                F::FreeFloat,
                "free_float",
                "Free Float",
                DURATIONS,
                65.,
                K::Duration,
                E::ReadOnly,
            ),
            spec(
                F::FloatPath,
                "float_path",
                "Float Path",
                FLOAT_PATHS,
                55.,
                K::Number,
                E::ReadOnly,
            ),
            spec(
                F::FloatPathOrder,
                "float_path_order",
                "Float Path Order",
                FLOAT_PATHS,
                55.,
                K::Number,
                E::ReadOnly,
            ),
            spec(
                F::ActivityPercent,
                "activity_percent_complete",
                "Activity % Complete",
                PERCENTS,
                70.,
                K::Percent,
                E::Number,
            ),
            spec(
                F::DurationPercent,
                "duration_percent_complete",
                "Duration % Complete",
                PERCENTS,
                70.,
                K::Percent,
                E::Number,
            ),
            spec(
                F::PhysicalPercent,
                "physical_percent_complete",
                "Physical % Complete",
                PERCENTS,
                70.,
                K::Percent,
                E::Number,
            ),
            spec(
                F::Predecessors,
                "predecessors",
                "Predecessors",
                LISTS,
                150.,
                K::Text,
                E::ReadOnly,
            ),
            spec(
                F::Successors,
                "successors",
                "Successors",
                LISTS,
                150.,
                K::Text,
                E::ReadOnly,
            ),
            // Listed under General, as the reference's relationship
            // column lists show them.
            spec(
                F::PredecessorDetails,
                "predecessor_details",
                "Predecessor Details",
                GENERAL,
                150.,
                K::Text,
                E::ReadOnly,
            ),
            spec(
                F::SuccessorDetails,
                "successor_details",
                "Successor Details",
                GENERAL,
                150.,
                K::Text,
                E::ReadOnly,
            ),
        ];
        for measure in Measure::ALL {
            for phase in Phase::ALL {
                let (group, kind) = if measure.is_cost() {
                    (COSTS, K::Cost)
                } else {
                    (UNITS, K::Units)
                };
                // Budgeted, actual and remaining figures are entered on the
                // activity; at completion is their sum.
                let editor = if measure.editable() && phase != Phase::AtCompletion {
                    E::Number
                } else {
                    E::ReadOnly
                };
                c.push(spec(
                    F::Figure(measure, phase),
                    &format!("{}_{}", phase.key(), measure.key()),
                    &format!("{} {}", phase.caption(), measure.caption()),
                    group,
                    80.,
                    kind,
                    editor,
                ));
            }
        }
        for measure in Measure::ALL.into_iter().filter(|m| m.editable()) {
            let (group, kind) = if measure.is_cost() {
                (COSTS, K::Cost)
            } else {
                (UNITS, K::Units)
            };
            c.push(spec(
                F::ThisPeriod(measure),
                &format!("actual_this_period_{}", measure.key()),
                &format!("Actual This Period {}", measure.caption()),
                group,
                100.,
                kind,
                E::Number,
            ));
        }
        c.extend(crate::baselines::columns());
        c.extend(super::columns_ext::columns());
        c
    })
}

/// Code types visible to the project: global, EPS and this project's own
/// (the shared catalog's list).
pub fn code_types(p: &ProjectData) -> Vec<(i64, String)> {
    shared::code_types(p)
}

fn notebook_topics(p: &ProjectData) -> Vec<(i64, String)> {
    let Some(t) = p.document.table("MEMOTYPE") else {
        return vec![];
    };
    let mut topics: Vec<(i64, i64, String)> = t
        .rows
        .iter()
        .filter(|r| t.value(r, "task_flag") != "N")
        .map(|r| {
            (
                num(t.value(r, "seq_num")),
                num(t.value(r, "memo_type_id")),
                t.value(r, "memo_type").to_string(),
            )
        })
        .collect();
    topics.sort();
    topics.into_iter().map(|(_, id, name)| (id, name)).collect()
}

/// Every column available for this project: built-ins, then one per activity
/// code type and activity user defined field as the shared catalog lists
/// them, then one per notebook topic.
pub fn catalog(p: &ProjectData) -> Vec<Column> {
    let mut out = builtin().to_vec();
    let mut seen: BTreeSet<String> = out.iter().map(|c| c.id.clone()).collect();
    let mut push = |c: Column| {
        if seen.insert(c.id.clone()) {
            out.push(c);
        }
    };
    for f in shared::catalog(p) {
        let (group, width, align, source) = match f.source {
            shared::Source::Code(id) => (CODES, 100., Align::Left, Source::Code(id)),
            shared::Source::Udf(id) => (USER_DEFINED, 100., default_align(f.kind), Source::Udf(id)),
            shared::Source::Field(_) => continue,
        };
        push(Column {
            id: f.id,
            title: f.title,
            group: group.into(),
            width,
            align,
            kind: f.kind,
            editor: f.editor,
            source,
        });
    }
    for (id, name) in notebook_topics(p) {
        push(Column {
            id: format!("notebook:{name}"),
            title: name,
            group: NOTEBOOK.into(),
            width: 160.,
            align: Align::Left,
            kind: Kind::Text,
            editor: Editor::ReadOnly,
            source: Source::Notebook(id),
        });
    }
    out
}

/// Look up one column by id in a catalog.
pub fn find<'a>(catalog: &'a [Column], id: &str) -> Option<&'a Column> {
    catalog.iter().find(|c| c.id == id)
}

/// A typed cell value, and the sort order of two (the shared catalog's).
pub use openxer_xer::edit::fields::{compare, Value};

/// Calendar periods for duration text: the activity's calendar, or the
/// project calendar for band totals and filter values without an activity.
fn periods(p: &ProjectData, activity: Option<usize>) -> format::Periods {
    activity.map_or_else(
        || format::project_periods(p),
        |i| format::activity_periods(p, i),
    )
}

/// Table text for a value, in the user's display formats. Every table cell,
/// band total, filter comparison, find/replace match and export of "what you
/// see" goes through here. `activity` picks the calendar for durations.
pub fn display(p: &ProjectData, activity: Option<usize>, value: &Value) -> String {
    shared::display(p, &UserFormats, activity, value)
}

/// Work units typed in the user's units format (`8`, `8h`, `2d`), as hours.
/// Plain numbers and an `h` suffix keep their decimals when the format is
/// hours; other formats convert through the working day.
pub fn parse_units(text: &str) -> Result<f64, String> {
    let t = text.trim();
    let format = format::current().units;
    let plain = t.trim_end_matches(['h', 'H']).replace(',', "");
    if let Ok(v) = plain.trim().parse::<f64>() {
        if (t.len() != plain.len() || format.unit == format::TimeUnit::Hour) && v.is_finite() {
            return Ok(v);
        }
    }
    let minutes = format.parse(t, format::Periods::DEFAULT)?;
    Ok(minutes as f64 / 60.)
}

/// Read typed text for a column kind: the inverse of `display`, also used
/// for filter values. Durations and dates follow the user's formats (dates
/// also accept 2026-11-04 08:00 and 04-Nov-26); percents are 0 to 100.
pub fn parse_input(
    p: &ProjectData,
    activity: Option<usize>,
    kind: Kind,
    text: &str,
) -> Result<Value, String> {
    shared::parse_input(p, &UserFormats, activity, kind, text)
}

pub use openxer_xer::ev::Figures;

/// Predecessor and successor activity indexes per activity.
type Neighbors = (Vec<Vec<usize>>, Vec<Vec<usize>>);

/// Lookups built once per pass over the document, each only when a column
/// that needs it is asked for. The shared catalog's lookups (`core`) serve
/// the fields it reads; the rest are the table's own columns'.
#[derive(Default)]
pub struct Index {
    /// (longest path defines critical, total float threshold in hours)
    pub critical: (bool, f64),
    /// The shared catalog's lookups (codes, user defined fields, figures,
    /// WBS, relationships).
    pub core: shared::Index,
    memos: OnceCell<HashMap<(i64, i64), String>>,
    /// Per activity, the rows of `model.links` into it and out of it.
    link_rows: OnceCell<Neighbors>,
    /// The open project's assigned baselines; baseline columns are blank
    /// without them.
    pub baselines: Option<std::sync::Arc<crate::baselines::Compare>>,
    /// Thresholds of the View > Check Schedule filters.
    pub check_params: crate::schedule_check::CheckParams,
    /// Activities each Check Schedule filter shows, built when first asked.
    check_sets: std::cell::RefCell<
        HashMap<crate::layout_opts::checks::CheckFilter, std::rc::Rc<BTreeSet<usize>>>,
    >,
    /// Lookups of the columns in `super::columns_ext`.
    pub ext: super::columns_ext::Cache,
    /// Band totals already computed with these lookups, per column id and
    /// band member list (see `group::band_total_cached`).
    pub(crate) band_totals: std::cell::RefCell<HashMap<BandTotalKey, BandTotal>>,
    /// View > Activity Critical Path while it shows paths: the table shows
    /// only their activities (see `group::build`).
    pub critical_paths: Option<std::rc::Rc<crate::critical_path::PathView>>,
}

/// A band total's column id and the identity of its member list.
pub(crate) type BandTotalKey = (String, usize, usize, usize);
/// The member list (kept so its identity cannot be reused) and the total.
pub(crate) type BandTotal = (super::group::Members, Option<Value>);

pub use openxer_xer::edit::fields::{CodeValue, WbsIndex};

impl Index {
    pub fn new(critical: (bool, f64)) -> Self {
        Self {
            critical,
            core: shared::Index::new(critical),
            ..Default::default()
        }
    }
    /// Activities the Check Schedule filter `filter` shows.
    pub fn check_set(
        &self,
        p: &ProjectData,
        filter: crate::layout_opts::checks::CheckFilter,
    ) -> std::rc::Rc<BTreeSet<usize>> {
        if let Some(set) = self.check_sets.borrow().get(&filter) {
            return set.clone();
        }
        let set = std::rc::Rc::new(crate::layout_opts::checks::matching(
            p,
            &self.check_params,
            self.baselines.as_deref(),
            filter,
        ));
        self.check_sets.borrow_mut().insert(filter, set.clone());
        set
    }
    pub fn code_of(&self, p: &ProjectData, task_id: i64, type_id: i64) -> Option<CodeValue> {
        self.core.code_of(p, task_id, type_id)
    }
    /// The `actv_code_id` an activity has for a code type.
    pub fn code_id(&self, p: &ProjectData, task_id: i64, type_id: i64) -> Option<i64> {
        self.core.code_id(p, task_id, type_id)
    }
    pub fn code_values(&self, p: &ProjectData) -> &HashMap<i64, CodeValue> {
        self.core.code_values(p)
    }
    fn memo(&self, p: &ProjectData, task_id: i64, topic: i64) -> String {
        self.memos
            .get_or_init(|| {
                let pid = p.model.project.proj_id;
                p.document.table("TASKMEMO").map_or_else(HashMap::new, |t| {
                    let has_project = t.field("proj_id").is_some();
                    t.rows
                        .iter()
                        .filter(|r| !has_project || num(t.value(r, "proj_id")) == pid)
                        .map(|r| {
                            let text = records::memo_to_text(t.value(r, "task_memo"));
                            let line = text.lines().find(|l| !l.trim().is_empty());
                            (
                                (num(t.value(r, "task_id")), num(t.value(r, "memo_type_id"))),
                                line.unwrap_or("").trim().to_string(),
                            )
                        })
                        .collect()
                })
            })
            .get(&(task_id, topic))
            .cloned()
            .unwrap_or_default()
    }
    /// [units or cost] figures per activity, keyed by `Measure` position.
    pub(crate) fn figures(
        &self,
        p: &ProjectData,
        task_id: i64,
        measure: Measure,
    ) -> Option<Figures> {
        self.core.figures(p, task_id, measure)
    }
    /// Actual this period of the activity for an editable measure: the sum
    /// of its assignments of that type, or its own figure without one.
    pub(crate) fn this_period(&self, p: &ProjectData, task_id: i64, measure: Measure) -> f64 {
        self.core.this_period(p, task_id, measure)
    }
    pub fn wbs(&self, p: &ProjectData) -> &WbsIndex {
        self.core.wbs(p)
    }

    /// Per activity, the rows of `model.links` into it (its predecessors)
    /// and out of it (its successors).
    fn link_rows(&self, p: &ProjectData) -> &Neighbors {
        self.link_rows.get_or_init(|| {
            let n = p.model.activities.len();
            let mut into = vec![vec![]; n];
            let mut out = vec![vec![]; n];
            for (r, l) in p.model.links.iter().enumerate() {
                if l.pred < n && l.succ < n {
                    into[l.succ].push(r);
                    out[l.pred].push(r);
                }
            }
            (into, out)
        })
    }
}

/// Predecessor Details (`predecessors` true) or Successor Details text for
/// the relationship rows `rows`: `<Activity ID>: <type>`, a space and the
/// lag when it is not zero (a bare number in the user's duration unit, on
/// the lag's calendar), entries by Activity ID joined with ", ".
pub(crate) fn link_details(p: &ProjectData, rows: &[usize], predecessors: bool) -> String {
    let mut unit = format::current().durations;
    unit.show_label = false;
    unit.sub_unit = false;
    let mut entries: Vec<(&str, String)> = rows
        .iter()
        .filter_map(|&r| {
            let l = p.model.links.get(r)?;
            let other = if predecessors { l.pred } else { l.succ };
            let code = p.model.activities.get(other)?.task_code.as_str();
            // FS, SS, FF or SF, as the XER code names it after "PR_".
            let kind = l.link_type.as_str().trim_start_matches("PR_");
            let text = if l.lag == 0 {
                format!("{code}: {kind}")
            } else {
                let lag = unit.format(l.lag, format::lag_periods(p, l.pred, l.succ));
                format!("{code}: {kind} {lag}")
            };
            Some((code, text))
        })
        .collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));
    entries
        .into_iter()
        .map(|(_, t)| t)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Critical by the project's definition; completed activities never are.
/// The one rule lives in `openxer_xer::critical`, shared with the
/// spreadsheet export.
pub use openxer_xer::is_critical;

/// The value of `column` for activity `i`: the shared catalog reads its
/// fields, codes and user defined fields; the table's own columns are
/// read here.
pub fn value(p: &ProjectData, ix: &Index, column: &Column, i: usize) -> Value {
    let Some(a) = p.model.activities.get(i) else {
        return Value::Empty;
    };
    match column.source {
        Source::Code(type_id) => {
            shared::value(p, &ix.core, shared::Source::Code(type_id), column.kind, i)
        }
        Source::Udf(type_id) => {
            shared::value(p, &ix.core, shared::Source::Udf(type_id), column.kind, i)
        }
        Source::Notebook(topic) => Value::text(ix.memo(p, a.task_id, topic)),
        Source::Field(field) => field_value(p, ix, a, i, field),
    }
}

/// The value of `column` for activity `i` as the table shows it: activity
/// code columns follow the user's Codes preference (code value or
/// description). Sorting, filtering and exports keep the code value.
pub fn shown_value(p: &ProjectData, ix: &Index, column: &Column, i: usize) -> Value {
    if let (Source::Code(type_id), crate::format::CodeDisplay::Description) =
        (column.source, crate::format::code_display())
    {
        let Some(a) = p.model.activities.get(i) else {
            return Value::Empty;
        };
        return ix
            .code_of(p, a.task_id, type_id)
            .map_or(Value::Empty, |c| Value::text(c.name));
    }
    value(p, ix, column, i)
}

/// Activity % of activity `i` by its percent complete type (physical,
/// duration or units): the Activity % column's value as a number.
pub(crate) fn activity_percent(p: &ProjectData, ix: &Index, i: usize) -> f64 {
    match field(p, ix, i, Field::ActivityPercent) {
        Value::Percent(v) => v,
        _ => 0.,
    }
}

/// The value of built-in field `field` for activity `i`.
pub(crate) fn field(p: &ProjectData, ix: &Index, i: usize, field: Field) -> Value {
    p.model
        .activities
        .get(i)
        .map_or(Value::Empty, |a| field_value(p, ix, a, i, field))
}

fn field_value(p: &ProjectData, ix: &Index, a: &Activity, i: usize, field: Field) -> Value {
    if let Some(f) = field.shared() {
        return shared::value(p, &ix.core, shared::Source::Field(f), Kind::Text, i);
    }
    let d = p.details.get(&a.task_id);
    match field {
        Field::PredecessorDetails => Value::text(link_details(p, &ix.link_rows(p).0[i], true)),
        Field::SuccessorDetails => Value::text(link_details(p, &ix.link_rows(p).1[i], false)),
        Field::Baseline(f) => crate::baselines::column_value(p, ix, ix.baselines.as_deref(), a, f),
        Field::Ext(e) => super::columns_ext::value(p, ix, a, d, i, e),
        // Every other field is the shared catalog's.
        _ => Value::Empty,
    }
}

/// Choices offered by a `Choice` editor: (stored key, shown text).
pub fn choices(p: &ProjectData, ix: &Index, column: &Column, i: usize) -> Vec<(String, String)> {
    match column.source {
        Source::Field(Field::ActivityType) => [
            TaskType::Task,
            TaskType::Resource,
            TaskType::Loe,
            TaskType::Mile,
            TaskType::FinMile,
            TaskType::Wbs,
        ]
        .into_iter()
        .map(|t| (type_label(t).to_string(), type_label(t).to_string()))
        .collect(),
        Source::Field(Field::Wbs) => {
            let w = ix.wbs(p);
            let mut ids: Vec<_> = p.wbs.iter().map(|x| x.id).collect();
            ids.sort_by_key(|id| w.order.get(id).copied().unwrap_or(usize::MAX));
            ids.into_iter()
                .map(|id| {
                    let code = w.codes.get(&id).cloned().unwrap_or_default();
                    let name = w.names.get(&id).cloned().unwrap_or_default();
                    (code.clone(), format!("{code}  {name}"))
                })
                .collect()
        }
        Source::Field(Field::ActivityStatus) => crate::details::setters::STATUSES
            .iter()
            .map(|s| (status_label(*s).to_string(), status_label(*s).to_string()))
            .collect(),
        Source::Field(Field::DurationType) => DURATION_TYPES
            .iter()
            .map(|(_, l)| (l.to_string(), l.to_string()))
            .collect(),
        Source::Field(Field::PercentType) => PERCENT_TYPES
            .iter()
            .map(|(_, l)| (l.to_string(), l.to_string()))
            .collect(),
        Source::Field(Field::AutoComputeActuals) => ["Yes", "No"]
            .iter()
            .map(|l| (l.to_string(), l.to_string()))
            .collect(),
        Source::Field(Field::PrimaryResource) => {
            // A blank entry, then the activity's assigned resources.
            std::iter::once((String::new(), String::new()))
                .chain(
                    shared::assigned_resources(p, i)
                        .into_iter()
                        .map(|name| (name.clone(), name)),
                )
                .collect()
        }
        Source::Field(Field::Calendar) => {
            let current = p.model.activities.get(i).map_or(0, |a| a.cal);
            crate::calendars::ops::assignable(p, current)
                .into_iter()
                .map(|c| {
                    let name = p.calendar_names[c].clone();
                    (name.clone(), name)
                })
                .collect()
        }
        Source::Field(Field::PrimaryConstraint) | Source::Field(Field::SecondaryConstraint) => (0
            ..=9)
            .filter_map(CstrType::from_code)
            .map(|c| {
                (
                    constraint_label(c).to_string(),
                    constraint_label(c).to_string(),
                )
            })
            .collect(),
        // A choice editor on a user defined field is an indicator.
        Source::Udf(_) => std::iter::once((String::new(), String::new()))
            .chain(
                crate::details::record_tabs::INDICATORS
                    .iter()
                    .map(|(name, _)| (name.to_string(), name.to_string())),
            )
            .collect(),
        Source::Code(type_id) => {
            let mut values: Vec<_> = ix
                .code_values(p)
                .iter()
                .filter(|(_, v)| v.type_id == type_id)
                .map(|(id, v)| (v.sequence, *id, v.short_name.clone(), v.name.clone()))
                .collect();
            values.sort();
            std::iter::once((String::new(), String::new()))
                .chain(
                    values
                        .into_iter()
                        .map(|(_, _, short, name)| (short.clone(), format!("{short}  {name}"))),
                )
                .collect()
        }
        _ => vec![],
    }
}
