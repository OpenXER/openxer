//! Activity column catalog shared by the spreadsheet export and import: titles,
//! Spreadsheet field names (XER), value kinds, editability and value lookup.
use crate::util::num;
use openxer_engine::calendar::DAY;
use openxer_engine::model::{Activity, CstrType, TaskType};
use openxer_xer::{
    constraint_label, is_critical, status_label, type_label, CriticalDefinition, ProjectData,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Column {
    ActivityId,
    ActivityName,
    ActivityStatus,
    ActivityType,
    WbsCode,
    WbsName,
    Calendar,
    OriginalDuration,
    RemainingDuration,
    Start,
    Finish,
    ActualStart,
    ActualFinish,
    EarlyStart,
    EarlyFinish,
    LateStart,
    LateFinish,
    TotalFloat,
    FreeFloat,
    PercentComplete,
    PrimaryConstraint,
    PrimaryConstraintDate,
    SecondaryConstraint,
    SecondaryConstraintDate,
    Critical,
    ExpectedFinish,
    SuspendDate,
    ResumeDate,
    ActualDuration,
    ActivityPercent,
    DurationPercent,
    /// Actual labor units of the activity, in hours.
    ActualLaborUnits,
    /// Remaining labor units of the activity, in hours.
    RemainingLaborUnits,
    /// Activity code type id.
    ActivityCode(i64),
    /// UDF type id (TASK table UDFs).
    Udf(i64),
    /// A column whose title, kind and values the caller computed
    /// (`Lookup::extras`), such as the activity table's earned value and
    /// percent complete columns. Never imported.
    Extra(u16),
}

/// The values of one caller-computed column, per activity index.
#[derive(Clone, Debug, PartialEq)]
pub struct Extra {
    pub title: String,
    pub kind: Kind,
    pub values: HashMap<usize, Value>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Text,
    Date,
    Duration,
    Number,
    Percent,
    Bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DurationUnit {
    #[default]
    Days,
    Hours,
}
impl DurationUnit {
    pub fn suffix(self) -> &'static str {
        match self {
            DurationUnit::Days => "d",
            DurationUnit::Hours => "h",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            DurationUnit::Days => "Days",
            DurationUnit::Hours => "Hours",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "d" | "day" | "days" | "qt_day" => Some(Self::Days),
            "h" | "hr" | "hrs" | "hour" | "hours" | "qt_hour" => Some(Self::Hours),
            _ => None,
        }
    }
    /// Working minutes to a spreadsheet number.
    pub fn from_minutes(self, minutes: i64, hours_per_day: f64) -> f64 {
        match self {
            DurationUnit::Days => minutes as f64 / 60.0 / hours_per_day,
            DurationUnit::Hours => minutes as f64 / 60.0,
        }
    }
    pub fn to_minutes(self, value: f64, hours_per_day: f64) -> i64 {
        match self {
            DurationUnit::Days => (value * hours_per_day * 60.0).round() as i64,
            DurationUnit::Hours => (value * 60.0).round() as i64,
        }
    }
}

/// A cell value in OpenXER terms. Dates are absolute minutes since 1899-12-30,
/// which is also Excel's serial day count times 1440.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Empty,
    Text(String),
    Date(i64),
    Duration(i64),
    Number(f64),
    Bool(bool),
}

impl Column {
    /// Fixed columns in catalog order (codes and UDFs follow from the project).
    pub const FIXED: [Column; 33] = [
        Column::ActivityId,
        Column::ActivityName,
        Column::ActivityStatus,
        Column::ActivityType,
        Column::WbsCode,
        Column::WbsName,
        Column::Calendar,
        Column::OriginalDuration,
        Column::RemainingDuration,
        Column::Start,
        Column::Finish,
        Column::ActualStart,
        Column::ActualFinish,
        Column::EarlyStart,
        Column::EarlyFinish,
        Column::LateStart,
        Column::LateFinish,
        Column::TotalFloat,
        Column::FreeFloat,
        Column::PercentComplete,
        Column::PrimaryConstraint,
        Column::PrimaryConstraintDate,
        Column::SecondaryConstraint,
        Column::SecondaryConstraintDate,
        Column::Critical,
        Column::ExpectedFinish,
        Column::SuspendDate,
        Column::ResumeDate,
        Column::ActualDuration,
        Column::ActivityPercent,
        Column::DurationPercent,
        Column::ActualLaborUnits,
        Column::RemainingLaborUnits,
    ];
    pub fn base_title(self) -> &'static str {
        match self {
            Column::ActivityId => "Activity ID",
            Column::ActivityName => "Activity Name",
            Column::ActivityStatus => "Activity Status",
            Column::ActivityType => "Activity Type",
            Column::WbsCode => "WBS Code",
            Column::WbsName => "WBS Name",
            Column::Calendar => "Calendar",
            Column::OriginalDuration => "Original Duration",
            Column::RemainingDuration => "Remaining Duration",
            Column::Start => "Start",
            Column::Finish => "Finish",
            Column::ActualStart => "Actual Start",
            Column::ActualFinish => "Actual Finish",
            Column::EarlyStart => "Early Start",
            Column::EarlyFinish => "Early Finish",
            Column::LateStart => "Late Start",
            Column::LateFinish => "Late Finish",
            Column::TotalFloat => "Total Float",
            Column::FreeFloat => "Free Float",
            Column::PercentComplete => "Physical % Complete",
            Column::PrimaryConstraint => "Primary Constraint",
            Column::PrimaryConstraintDate => "Primary Constraint Date",
            Column::SecondaryConstraint => "Secondary Constraint",
            Column::SecondaryConstraintDate => "Secondary Constraint Date",
            Column::Critical => "Critical",
            Column::ExpectedFinish => "Expected Finish",
            Column::SuspendDate => "Suspend Date",
            Column::ResumeDate => "Resume Date",
            Column::ActualDuration => "Actual Duration",
            Column::ActivityPercent => "Activity % Complete",
            Column::DurationPercent => "Duration % Complete",
            Column::ActualLaborUnits => "Actual Labor Units",
            Column::RemainingLaborUnits => "Remaining Labor Units",
            Column::ActivityCode(_) => "Activity Code",
            Column::Udf(_) => "User Defined Field",
            Column::Extra(_) => "Column",
        }
    }
    /// XER field name used in row 1 of the tables-layout spreadsheet.
    pub fn field(self) -> String {
        match self {
            Column::ActivityId => "task_code".into(),
            Column::ActivityName => "task_name".into(),
            Column::ActivityStatus => "status_code".into(),
            Column::ActivityType => "task_type".into(),
            Column::WbsCode => "wbs_id".into(),
            Column::WbsName => "wbs_name".into(),
            Column::Calendar => "clndr_id".into(),
            Column::OriginalDuration => "target_drtn_hr_cnt".into(),
            Column::RemainingDuration => "remain_drtn_hr_cnt".into(),
            Column::Start => "start_date".into(),
            Column::Finish => "end_date".into(),
            Column::ActualStart => "act_start_date".into(),
            Column::ActualFinish => "act_end_date".into(),
            Column::EarlyStart => "early_start_date".into(),
            Column::EarlyFinish => "early_end_date".into(),
            Column::LateStart => "late_start_date".into(),
            Column::LateFinish => "late_end_date".into(),
            Column::TotalFloat => "total_float_hr_cnt".into(),
            Column::FreeFloat => "free_float_hr_cnt".into(),
            Column::PercentComplete => "phys_complete_pct".into(),
            Column::PrimaryConstraint => "cstr_type".into(),
            Column::PrimaryConstraintDate => "cstr_date".into(),
            Column::SecondaryConstraint => "cstr_type2".into(),
            Column::SecondaryConstraintDate => "cstr_date2".into(),
            Column::Critical => "driving_path_flag".into(),
            Column::ExpectedFinish => "expect_end_date".into(),
            Column::SuspendDate => "suspend_date".into(),
            Column::ResumeDate => "resume_date".into(),
            Column::ActualDuration => "act_drtn_hr_cnt".into(),
            Column::ActivityPercent => "activity_pct_complete".into(),
            Column::DurationPercent => "duration_pct_complete".into(),
            Column::ActualLaborUnits => "act_work_qty".into(),
            Column::RemainingLaborUnits => "remain_work_qty".into(),
            Column::ActivityCode(id) => format!("actv_code_{id}_id"),
            Column::Udf(id) => format!("user_field_{id}"),
            Column::Extra(n) => format!("extra_{n}"),
        }
    }
    /// Stable key written to the About sheet and used by automation.
    pub fn key(self) -> String {
        match self {
            Column::ActivityCode(id) => format!("activity_code:{id}"),
            Column::Udf(id) => format!("udf:{id}"),
            Column::Extra(n) => format!("extra:{n}"),
            c => c
                .base_title()
                .to_ascii_lowercase()
                .replace("% ", "")
                .replace(' ', "_"),
        }
    }
    pub fn from_key(key: &str) -> Option<Self> {
        if let Some(id) = key.strip_prefix("activity_code:") {
            return id.parse().ok().map(Column::ActivityCode);
        }
        if let Some(id) = key.strip_prefix("udf:") {
            return id.parse().ok().map(Column::Udf);
        }
        Self::FIXED.into_iter().find(|c| c.key() == key)
    }
    pub fn kind(self, lookup: &Lookup) -> Kind {
        match self {
            Column::OriginalDuration
            | Column::RemainingDuration
            | Column::ActualDuration
            | Column::TotalFloat
            | Column::FreeFloat => Kind::Duration,
            Column::Start
            | Column::Finish
            | Column::ActualStart
            | Column::ActualFinish
            | Column::EarlyStart
            | Column::EarlyFinish
            | Column::LateStart
            | Column::LateFinish
            | Column::PrimaryConstraintDate
            | Column::SecondaryConstraintDate
            | Column::ExpectedFinish
            | Column::SuspendDate
            | Column::ResumeDate => Kind::Date,
            Column::PercentComplete | Column::ActivityPercent | Column::DurationPercent => {
                Kind::Percent
            }
            Column::ActualLaborUnits | Column::RemainingLaborUnits => Kind::Number,
            Column::Critical => Kind::Bool,
            Column::Extra(n) => lookup.extras.get(n as usize).map_or(Kind::Text, |e| e.kind),
            Column::Udf(id) => match lookup.udf_types.get(&id).map(|t| t.1.as_str()) {
                Some("FT_START_DATE" | "FT_END_DATE") => Kind::Date,
                Some("FT_FLOAT_2_DECIMALS" | "FT_INT" | "FT_MONEY") => Kind::Number,
                _ => Kind::Text,
            },
            _ => Kind::Text,
        }
    }
    /// Columns the spreadsheet import can change.
    pub fn editable(self) -> bool {
        matches!(
            self,
            Column::ActivityId
                | Column::ActivityName
                | Column::WbsCode
                | Column::Calendar
                | Column::OriginalDuration
                | Column::RemainingDuration
                | Column::ActualDuration
                | Column::ActualStart
                | Column::ActualFinish
                | Column::ExpectedFinish
                | Column::SuspendDate
                | Column::ResumeDate
                | Column::PercentComplete
                | Column::ActivityPercent
                | Column::DurationPercent
                | Column::ActualLaborUnits
                | Column::RemainingLaborUnits
                | Column::PrimaryConstraint
                | Column::PrimaryConstraintDate
                | Column::SecondaryConstraint
                | Column::SecondaryConstraintDate
                | Column::ActivityCode(_)
                | Column::Udf(_)
        )
    }
    pub fn title(self, lookup: &Lookup) -> String {
        match self {
            Column::ActivityCode(id) => lookup
                .code_types
                .get(&id)
                .cloned()
                .unwrap_or_else(|| format!("Activity Code {id}")),
            Column::Udf(id) => lookup
                .udf_types
                .get(&id)
                .map(|t| t.0.clone())
                .unwrap_or_else(|| format!("UDF {id}")),
            Column::Extra(n) => lookup
                .extras
                .get(n as usize)
                .map_or_else(|| format!("Column {n}"), |e| e.title.clone()),
            c => c.base_title().into(),
        }
    }
    /// Header text in the modern export: duration columns carry their unit.
    pub fn header(self, lookup: &Lookup, unit: DurationUnit) -> String {
        let title = self.title(lookup);
        if self.kind(lookup) == Kind::Duration {
            format!("{title} ({})", unit.suffix())
        } else {
            title
        }
    }
    pub fn width(self, lookup: &Lookup) -> f64 {
        match self {
            Column::ActivityName => 42.0,
            Column::WbsName => 30.0,
            Column::ActivityId | Column::WbsCode => 16.0,
            Column::Calendar => 22.0,
            Column::PrimaryConstraint | Column::SecondaryConstraint => 20.0,
            c => match c.kind(lookup) {
                Kind::Date => 15.0,
                Kind::Duration | Kind::Number | Kind::Percent => 12.0,
                Kind::Bool => 9.0,
                Kind::Text => 18.0,
            },
        }
    }
}

/// Lookups built once per export or import.
pub struct Lookup {
    pub code_types: HashMap<i64, String>,
    /// (task_id, code type) to code short name.
    pub codes: HashMap<(i64, i64), String>,
    /// UDF type id to (label, logical data type).
    pub udf_types: HashMap<i64, (String, String)>,
    pub udf_values: HashMap<(i64, i64), Value>,
    pub wbs_path: HashMap<i64, String>,
    pub wbs_depth: HashMap<i64, usize>,
    pub hours_per_day: f64,
    pub origin_minutes: i64,
    /// Caller-computed columns (`Column::Extra`).
    pub extras: Vec<Extra>,
    /// Suspend date of each suspended activity (minutes from the origin).
    pub suspend: HashMap<i64, i64>,
    /// Stored percent complete type of each activity.
    pub percent_types: HashMap<i64, String>,
    /// (actual, remaining) labor units in hours of each activity.
    pub labor_units: HashMap<i64, (f64, f64)>,
    /// The project's critical-activity definition (`openxer_xer::critical`),
    /// the same one the activity table uses.
    pub critical: CriticalDefinition,
}

impl Lookup {
    /// A lookup with nothing in it, for asking a fixed column's kind.
    pub fn empty() -> Self {
        Self {
            code_types: HashMap::new(),
            codes: HashMap::new(),
            udf_types: HashMap::new(),
            udf_values: HashMap::new(),
            wbs_path: HashMap::new(),
            wbs_depth: HashMap::new(),
            hours_per_day: 8.0,
            origin_minutes: 0,
            extras: vec![],
            suspend: HashMap::new(),
            percent_types: HashMap::new(),
            labor_units: HashMap::new(),
            critical: (false, 0.),
        }
    }
    pub fn new(p: &ProjectData) -> Self {
        let doc = &p.document;
        let pid = p.model.project.proj_id;
        let mut code_types = HashMap::new();
        if let Some(t) = doc.table("ACTVTYPE") {
            for r in &t.rows {
                let scope = t.value(r, "proj_id").trim();
                if scope.is_empty() || num(scope) == pid {
                    code_types.insert(
                        num(t.value(r, "actv_code_type_id")),
                        t.value(r, "actv_code_type").to_string(),
                    );
                }
            }
        }
        let code_names: HashMap<i64, String> = doc.table("ACTVCODE").map_or(HashMap::new(), |t| {
            t.rows
                .iter()
                .map(|r| {
                    (
                        num(t.value(r, "actv_code_id")),
                        t.value(r, "short_name").to_string(),
                    )
                })
                .collect()
        });
        let mut codes = HashMap::new();
        if let Some(t) = doc.table("TASKACTV") {
            for r in &t.rows {
                let project = t.value(r, "proj_id").trim();
                if !project.is_empty() && num(project) != pid {
                    continue;
                }
                if let Some(name) = code_names.get(&num(t.value(r, "actv_code_id"))) {
                    codes.insert(
                        (
                            num(t.value(r, "task_id")),
                            num(t.value(r, "actv_code_type_id")),
                        ),
                        name.clone(),
                    );
                }
            }
        }
        let mut udf_types = HashMap::new();
        if let Some(t) = doc.table("UDFTYPE") {
            for r in &t.rows {
                if t.value(r, "table_name").eq_ignore_ascii_case("TASK") {
                    udf_types.insert(
                        num(t.value(r, "udf_type_id")),
                        (
                            t.value(r, "udf_type_label").to_string(),
                            t.value(r, "logical_data_type").to_string(),
                        ),
                    );
                }
            }
        }
        let mut udf_values = HashMap::new();
        if let Some(t) = doc.table("UDFVALUE") {
            for r in &t.rows {
                let kind = num(t.value(r, "udf_type_id"));
                let Some((_, logical)) = udf_types.get(&kind) else {
                    continue;
                };
                let value = match logical.as_str() {
                    "FT_START_DATE" | "FT_END_DATE" => {
                        openxer_xer::parse_absolute(t.value(r, "udf_date"))
                            .map_or(Value::Empty, Value::Date)
                    }
                    "FT_FLOAT_2_DECIMALS" | "FT_INT" | "FT_MONEY" => t
                        .value(r, "udf_number")
                        .trim()
                        .parse::<f64>()
                        .map_or(Value::Empty, Value::Number),
                    _ => {
                        let s = t.value(r, "udf_text");
                        if s.is_empty() {
                            Value::Empty
                        } else {
                            Value::Text(s.to_string())
                        }
                    }
                };
                udf_values.insert((num(t.value(r, "fk_id")), kind), value);
            }
        }
        // WBS paths skip the project node, the way codes are typed by schedulers.
        let roots: Vec<i64> = doc.table("PROJWBS").map_or(vec![], |t| {
            t.rows
                .iter()
                .filter(|r| {
                    num(t.value(r, "proj_id")) == pid && t.value(r, "proj_node_flag") == "Y"
                })
                .map(|r| num(t.value(r, "wbs_id")))
                .collect()
        });
        let mut wbs_path = HashMap::new();
        let mut wbs_depth = HashMap::new();
        for w in &p.wbs {
            let mut parts = vec![];
            let mut current = Some(w.id);
            for _ in 0..256 {
                let Some(id) = current else { break };
                if roots.contains(&id) {
                    break;
                }
                let Some(node) = p.wbs.iter().find(|n| n.id == id) else {
                    break;
                };
                parts.push(node.code.clone());
                current = node.parent;
            }
            parts.reverse();
            wbs_depth.insert(w.id, parts.len().saturating_sub(1));
            wbs_path.insert(w.id, parts.join("."));
        }
        let hours_per_day = doc
            .table("CALENDAR")
            .and_then(|t| {
                let id = &p.model.calendars[p.model.project_calendar].id;
                t.rows
                    .iter()
                    .find(|r| t.value(r, "clndr_id") == id)
                    .and_then(|r| t.value(r, "day_hr_cnt").trim().parse::<f64>().ok())
            })
            .filter(|h| *h > 0.0 && *h <= 24.0)
            .unwrap_or(8.0);
        let (suspend, percent_types, labor_units) = progress_lookups(p);
        Self {
            code_types,
            codes,
            udf_types,
            udf_values,
            wbs_path,
            wbs_depth,
            hours_per_day,
            origin_minutes: p.model.origin_serial * DAY,
            extras: vec![],
            suspend,
            percent_types,
            labor_units,
            critical: openxer_xer::critical_definition(p),
        }
    }
    /// Columns this project offers: fixed ones, then its activity codes and UDFs.
    pub fn catalog(&self) -> Vec<Column> {
        let mut out = Column::FIXED.to_vec();
        let mut codes: Vec<_> = self.code_types.keys().copied().collect();
        codes.sort_unstable();
        out.extend(codes.into_iter().map(Column::ActivityCode));
        let mut udfs: Vec<_> = self.udf_types.keys().copied().collect();
        udfs.sort_unstable();
        out.extend(udfs.into_iter().map(Column::Udf));
        out
    }
    fn date(&self, v: Option<i64>) -> Value {
        v.map_or(Value::Empty, |m| Value::Date(m + self.origin_minutes))
    }
}

type ProgressLookups = (
    HashMap<i64, i64>,
    HashMap<i64, String>,
    HashMap<i64, (f64, f64)>,
);

/// Suspend dates, percent complete types and labor units per activity.
fn progress_lookups(p: &ProjectData) -> ProgressLookups {
    let doc = &p.document;
    let pid = p.model.project.proj_id;
    let float = |s: &str| s.trim().parse::<f64>().unwrap_or(0.0);
    let mut suspend = HashMap::new();
    let mut types = HashMap::new();
    let mut labor: HashMap<i64, (f64, f64)> = HashMap::new();
    let mut has_labor = std::collections::HashSet::new();
    if let Some(t) = doc.table("TASKRSRC") {
        let kinds: HashMap<i64, String> = doc.table("RSRC").map_or_else(HashMap::new, |r| {
            r.rows
                .iter()
                .map(|row| {
                    (
                        num(r.value(row, "rsrc_id")),
                        r.value(row, "rsrc_type").to_string(),
                    )
                })
                .collect()
        });
        let has_project = t.field("proj_id").is_some();
        for r in &t.rows {
            if has_project && num(t.value(r, "proj_id")) != pid {
                continue;
            }
            let mut kind = t.value(r, "rsrc_type").to_string();
            if kind.is_empty() {
                kind = kinds
                    .get(&num(t.value(r, "rsrc_id")))
                    .cloned()
                    .unwrap_or_default();
            }
            if matches!(kind.as_str(), "RT_Equip" | "RT_Mat") {
                continue;
            }
            let task = num(t.value(r, "task_id"));
            has_labor.insert(task);
            let e = labor.entry(task).or_default();
            e.0 += float(t.value(r, "act_reg_qty")) + float(t.value(r, "act_ot_qty"));
            e.1 += float(t.value(r, "remain_qty"));
        }
    }
    if let Some(t) = doc.table("TASK") {
        for r in &t.rows {
            if num(t.value(r, "proj_id")) != pid {
                continue;
            }
            let task = num(t.value(r, "task_id"));
            if let Ok(Some(v)) =
                openxer_xer::parse_relative(p.model.origin_serial, t.value(r, "suspend_date"))
            {
                suspend.insert(task, v);
            }
            types.insert(task, t.value(r, "complete_pct_type").to_string());
            // An activity without labor assignments carries its own quantities.
            let (act, rem) = (
                float(t.value(r, "act_work_qty")),
                float(t.value(r, "remain_work_qty")),
            );
            if !has_labor.contains(&task) && (act != 0.0 || rem != 0.0) {
                labor.insert(task, (act, rem));
            }
        }
    }
    (suspend, types, labor)
}

fn elapsed(p: &ProjectData, a: &Activity) -> i64 {
    let finish = a.act_finish.or(p.model.project.data_date);
    match (a.act_start, finish, p.model.calendars.get(a.cal)) {
        (Some(s), Some(f), Some(cal)) if f > s => cal.between(s, f).max(0),
        _ => 0,
    }
}

fn duration_percent(original: i64, a: &Activity) -> f64 {
    if original > 0 {
        ((original - a.rem) as f64 / original as f64 * 100.).clamp(0., 100.)
    } else if a.is_completed() {
        100.
    } else {
        0.
    }
}

pub fn displayed_start(a: &Activity, leveled: bool) -> Option<i64> {
    a.act_start
        .or(if leveled { a.restart } else { a.early_start })
        .or(a.target_start)
}
pub fn displayed_finish(a: &Activity, leveled: bool) -> Option<i64> {
    a.act_finish
        .or(if leveled { a.reend } else { a.early_finish })
        .or(a.target_finish)
}

/// Value of `column` for activity `index`.
pub fn value(p: &ProjectData, lookup: &Lookup, index: usize, column: Column) -> Value {
    let a = &p.model.activities[index];
    let d = p.details.get(&a.task_id);
    let leveled = p.model.options.do_level;
    let text = |s: &str| {
        if s.is_empty() {
            Value::Empty
        } else {
            Value::Text(s.to_string())
        }
    };
    match column {
        Column::ActivityId => text(&a.task_code),
        Column::ActivityName => text(d.map_or("", |d| d.name.as_str())),
        Column::ActivityStatus => text(status_label(a.status)),
        Column::ActivityType => text(type_label(a.task_type)),
        Column::WbsCode => d
            .and_then(|d| lookup.wbs_path.get(&d.wbs))
            .map_or(Value::Empty, |s| text(s)),
        Column::WbsName => d
            .and_then(|d| p.wbs.iter().find(|w| w.id == d.wbs))
            .map_or(Value::Empty, |w| text(&w.name)),
        Column::Calendar => text(p.calendar_names.get(a.cal).map_or("", String::as_str)),
        Column::OriginalDuration => Value::Duration(d.map_or(0, |d| d.original_duration)),
        Column::RemainingDuration => Value::Duration(a.rem),
        Column::Start => lookup.date(displayed_start(a, leveled).map(|s| {
            if a.task_type == TaskType::FinMile {
                displayed_finish(a, leveled).unwrap_or(s)
            } else {
                s
            }
        })),
        Column::Finish => lookup.date(displayed_finish(a, leveled).map(|f| {
            if a.task_type == TaskType::Mile {
                displayed_start(a, leveled).unwrap_or(f)
            } else {
                f
            }
        })),
        Column::ActualStart => lookup.date(a.act_start),
        Column::ActualFinish => lookup.date(a.act_finish),
        Column::EarlyStart => lookup.date(a.early_start),
        Column::EarlyFinish => lookup.date(a.early_finish),
        Column::LateStart => lookup.date(a.late_start),
        Column::LateFinish => lookup.date(a.late_finish),
        Column::TotalFloat => a.total_float.map_or(Value::Empty, Value::Duration),
        Column::FreeFloat => a.free_float.map_or(Value::Empty, Value::Duration),
        Column::PercentComplete => Value::Number(d.map_or(0.0, |d| d.percent)),
        Column::PrimaryConstraint => constraint_value(a.cstr),
        Column::PrimaryConstraintDate => lookup.date(a.cstr_date),
        Column::SecondaryConstraint => constraint_value(a.cstr2),
        Column::SecondaryConstraintDate => lookup.date(a.cstr_date2),
        Column::Critical => Value::Bool(is_critical(a, lookup.critical)),
        Column::ExpectedFinish => lookup.date(a.expect_end),
        Column::SuspendDate => lookup.date(lookup.suspend.get(&a.task_id).copied()),
        Column::ResumeDate => lookup.date(a.resume),
        Column::ActualDuration => Value::Duration(elapsed(p, a)),
        Column::DurationPercent => {
            Value::Number(duration_percent(d.map_or(0, |d| d.original_duration), a))
        }
        Column::ActivityPercent => {
            let physical = d.map_or(0.0, |d| d.percent);
            Value::Number(
                match lookup.percent_types.get(&a.task_id).map(String::as_str) {
                    Some("CP_Phys") => physical,
                    Some("CP_Units") => {
                        let (actual, remaining) = lookup
                            .labor_units
                            .get(&a.task_id)
                            .copied()
                            .unwrap_or_default();
                        let total = actual + remaining;
                        if total > 0.0 {
                            actual / total * 100.0
                        } else if a.is_completed() {
                            100.0
                        } else {
                            0.0
                        }
                    }
                    _ => duration_percent(d.map_or(0, |d| d.original_duration), a),
                },
            )
        }
        Column::ActualLaborUnits => Value::Number(
            lookup
                .labor_units
                .get(&a.task_id)
                .map_or(0.0, |units| units.0),
        ),
        Column::RemainingLaborUnits => Value::Number(
            lookup
                .labor_units
                .get(&a.task_id)
                .map_or(0.0, |units| units.1),
        ),
        Column::ActivityCode(id) => lookup
            .codes
            .get(&(a.task_id, id))
            .map_or(Value::Empty, |s| text(s)),
        Column::Udf(id) => lookup
            .udf_values
            .get(&(a.task_id, id))
            .cloned()
            .unwrap_or(Value::Empty),
        Column::Extra(n) => lookup
            .extras
            .get(n as usize)
            .and_then(|e| e.values.get(&index))
            .cloned()
            .unwrap_or(Value::Empty),
    }
}

fn constraint_value(c: CstrType) -> Value {
    if c == CstrType::None {
        Value::Empty
    } else {
        Value::Text(constraint_label(c).into())
    }
}

/// Parse a constraint by its label, its XER code, or a common abbreviation.
pub fn parse_constraint(s: &str) -> Option<CstrType> {
    let t = s.trim().to_ascii_lowercase();
    if t.is_empty() || t == "none" || t == "<none>" {
        return Some(CstrType::None);
    }
    let all = [
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
    if let Some(c) = all
        .into_iter()
        .find(|c| constraint_label(*c).to_ascii_lowercase() == t)
    {
        return Some(c);
    }
    Some(match t.as_str() {
        "cs_mso" | "so" => CstrType::StartOn,
        "cs_msob" | "snlt" | "sob" => CstrType::StartOnBefore,
        "cs_msoa" | "snet" | "soa" => CstrType::StartOnAfter,
        "cs_alap" | "alap" => CstrType::Alap,
        "cs_meo" | "fo" => CstrType::FinishOn,
        "cs_meob" | "fnlt" | "fob" => CstrType::FinishOnBefore,
        "cs_meoa" | "fnet" | "foa" => CstrType::FinishOnAfter,
        "cs_mandstart" | "ms" => CstrType::MandatoryStart,
        "cs_mandfin" | "mf" => CstrType::MandatoryFinish,
        _ => return None,
    })
}

/// Normalize a header for matching: lower case, no `(*)` marker, no unit suffix.
pub fn normalize_header(s: &str) -> String {
    let mut t = s.trim().to_ascii_lowercase();
    for marker in ["(*)", "*"] {
        if let Some(rest) = t.strip_prefix(marker) {
            t = rest.trim().to_string();
        }
    }
    for suffix in [
        "(d)", "(h)", "(days)", "(hours)", "(day)", "(hr)", "(hrs)", "(hour)", "(%)",
    ] {
        if let Some(rest) = t.strip_suffix(suffix) {
            t = rest.trim().to_string();
        }
    }
    t.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Unit stated in a header such as `Remaining Duration (h)`.
pub fn header_unit(s: &str) -> Option<DurationUnit> {
    let t = s.trim().to_ascii_lowercase();
    let inner = t.strip_suffix(')')?.rsplit_once('(')?.1.trim().to_string();
    DurationUnit::parse(&inner)
}

/// Match a header (display title or field name) to a catalog column.
pub fn match_header(header: &str, lookup: &Lookup) -> Option<Column> {
    let h = normalize_header(header);
    if h.is_empty() {
        return None;
    }
    let aliases: &[(&str, Column)] = &[
        ("activity id", Column::ActivityId),
        ("id", Column::ActivityId),
        ("activity name", Column::ActivityName),
        ("name", Column::ActivityName),
        ("description", Column::ActivityName),
        ("status", Column::ActivityStatus),
        ("wbs", Column::WbsCode),
        ("original duration", Column::OriginalDuration),
        ("planned duration", Column::OriginalDuration),
        ("od", Column::OriginalDuration),
        ("remaining duration", Column::RemainingDuration),
        ("rem. dur.", Column::RemainingDuration),
        ("rd", Column::RemainingDuration),
        ("% complete", Column::PercentComplete),
        ("percent complete", Column::PercentComplete),
        ("complete_pct", Column::PercentComplete),
        ("physical % complete", Column::PercentComplete),
        ("total float", Column::TotalFloat),
        ("tf", Column::TotalFloat),
        ("free float", Column::FreeFloat),
        ("constraint", Column::PrimaryConstraint),
        ("constraint date", Column::PrimaryConstraintDate),
    ];
    if let Some((_, c)) = aliases.iter().find(|(a, _)| *a == h) {
        return Some(*c);
    }
    let catalog = lookup.catalog();
    catalog
        .iter()
        .copied()
        .find(|c| normalize_header(&c.title(lookup)) == h)
        .or_else(|| catalog.iter().copied().find(|c| c.field() == h))
        .or_else(|| catalog.iter().copied().find(|c| c.key() == h))
}
