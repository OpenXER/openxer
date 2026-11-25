//! New activities: the project's Defaults (Projects view, Defaults tab) that
//! a new activity takes, and the Activity ID auto-numbering that Add, Paste
//! and Renumber share. The desktop's Edit > Add, the command line and the MCP
//! `add_activity` all call [`add_activity`], so a new activity is the same
//! wherever it is made.
//!
//! The defaults live in the PROJECT row: `def_duration_type`,
//! `def_complete_pct_type`, `def_task_type`, `clndr_id` (the project's
//! default calendar) and `acct_id` (the cost account, which new resource
//! assignments take), with the numbering in `task_code_prefix`,
//! `task_code_base`, `task_code_step` and `task_code_prefix_flag`.
use super::activity::{duration_type_code, parse_task_type, percent_type_code};
use super::records::{self, num};
use crate::ProjectData;
use openxer_engine::model::TaskType;
use std::collections::HashSet;

/// Name a new activity starts with.
pub const NEW_ACTIVITY_NAME: &str = "New Activity";

/// Original and Remaining Duration of a new activity, in minutes: five days
/// of eight hours, the default duration Admin Preferences shows.
pub const NEW_ACTIVITY_MINUTES: i64 = 5 * 8 * 60;

/// The Defaults tab values a project made by New Project starts with
/// (PROJECT column, value), and the observed Calculations tab switch that
/// recalculates actual units and cost from a duration percent.
/// [`new_project_defaults`] writes them where the project has none.
pub const NEW_PROJECT_DEFAULTS: [(&str, &str); 8] = [
    ("def_duration_type", "DT_FixedDUR2"),
    ("def_complete_pct_type", "CP_Drtn"),
    ("def_task_type", "TT_Task"),
    ("task_code_prefix", "A"),
    ("task_code_base", "1000"),
    ("task_code_step", "10"),
    ("task_code_prefix_flag", "Y"),
    ("act_pct_link_flag", "Y"),
];

/// A PROJECT column of the open project; empty when it has none.
fn project_value(p: &ProjectData, column: &str) -> Option<String> {
    let pid = p.model.project.proj_id;
    let t = p.document.table("PROJECT")?;
    t.field(column)?;
    t.rows
        .iter()
        .find(|r| num(t.value(r, "proj_id")) == pid)
        .map(|r| t.value(r, column).trim().to_string())
}

/// Fill the Defaults tab values a project made by New Project starts with,
/// leaving every value the project already holds.
pub fn new_project_defaults(p: &mut ProjectData) {
    let pid = p.model.project.proj_id;
    let t = p.document.table_mut("PROJECT");
    let Some(row) = t
        .rows
        .iter()
        .position(|r| num(t.value(r, "proj_id")) == pid)
    else {
        return;
    };
    for (column, value) in NEW_PROJECT_DEFAULTS {
        if t.value(&t.rows[row], column).trim().is_empty() {
            t.set(row, column, value.into());
        }
    }
}

// ------------------------------------------------------------- defaults

/// What a new activity takes from its project.
#[derive(Clone, Debug, PartialEq)]
pub struct Defaults {
    /// Stored code, one of `DURATION_TYPES`.
    pub duration_type: &'static str,
    /// Stored code, one of `PERCENT_TYPES`.
    pub percent_type: &'static str,
    pub task_type: TaskType,
    /// Index in the model's calendars.
    pub calendar: usize,
}

/// The open project's defaults for new activities. A missing or unknown
/// value falls back to Fixed Duration and Units, Duration percent complete,
/// Task Dependent and the project calendar.
pub fn defaults(p: &ProjectData) -> Defaults {
    let read = |c: &str| project_value(p, c);
    let value = |c: &str| effective_default(c, &read).unwrap_or_default();
    let calendar = project_value(p, "clndr_id").unwrap_or_default();
    Defaults {
        duration_type: duration_type_code(&value("def_duration_type")).unwrap_or("DT_FixedDUR2"),
        percent_type: percent_type_code(&value("def_complete_pct_type")).unwrap_or("CP_Drtn"),
        task_type: parse_task_type(&value("def_task_type")).unwrap_or(TaskType::Task),
        calendar: p
            .model
            .calendars
            .iter()
            .position(|c| !calendar.is_empty() && c.id == calendar)
            .unwrap_or(p.model.project_calendar),
    }
}

/// The value a new activity takes for Defaults tab column `column` (the
/// three types and the four numbering values), the fallbacks of
/// [`defaults`] and [`Numbering::default`] filled in where the project holds
/// no usable value. `value` reads a PROJECT column, `None` when the file
/// lacks it. `None` for any other column. The Defaults tab shows these, so
/// it never shows a blank where a new activity takes a value.
pub fn effective_default(column: &str, value: &dyn Fn(&str) -> Option<String>) -> Option<String> {
    let stored = value(column).unwrap_or_default();
    let digits = !stored.is_empty() && stored.len() <= 18;
    let fallback = Numbering::default();
    Some(match column {
        "def_duration_type" => {
            let code = duration_type_code(&stored);
            code.unwrap_or("DT_FixedDUR2").into()
        }
        "def_complete_pct_type" => {
            let code = percent_type_code(&stored);
            code.unwrap_or("CP_Drtn").into()
        }
        "def_task_type" => {
            let kind = parse_task_type(&stored).unwrap_or(TaskType::Task);
            kind.as_str().into()
        }
        "task_code_prefix" => value(column).unwrap_or(fallback.prefix),
        "task_code_base" if digits && stored.bytes().all(|b| b.is_ascii_digit()) => stored,
        "task_code_base" => fallback.suffix,
        "task_code_step" if num(&stored) > 0 => num(&stored).to_string(),
        "task_code_step" => fallback.step.to_string(),
        "task_code_prefix_flag" if stored == "Y" => stored,
        "task_code_prefix_flag" => "N".into(),
        _ => return None,
    })
}

// ------------------------------------------------------------- numbering

/// The project's auto-numbering defaults.
#[derive(Clone, Debug, PartialEq)]
pub struct Numbering {
    pub prefix: String,
    /// The first number, as text so leading zeros set the digit count.
    pub suffix: String,
    pub step: i64,
    /// "Increment Activity ID based on selected activity".
    pub from_selected: bool,
}

impl Default for Numbering {
    /// What a project without numbering values gets (a file that does not
    /// carry them). New Project writes the switch on
    /// ([`NEW_PROJECT_DEFAULTS`]).
    fn default() -> Self {
        Self {
            prefix: "A".into(),
            suffix: "1000".into(),
            step: 10,
            from_selected: false,
        }
    }
}

/// The open project's numbering defaults; missing or invalid values fall
/// back to [`Numbering::default`].
pub fn numbering(p: &ProjectData) -> Numbering {
    let read = |c: &str| project_value(p, c);
    let value = |c: &str| effective_default(c, &read).unwrap_or_default();
    Numbering {
        prefix: value("task_code_prefix"),
        suffix: value("task_code_base"),
        step: num(&value("task_code_step")),
        from_selected: value("task_code_prefix_flag") == "Y",
    }
}

/// An ID split into the text before its trailing digits, the number those
/// digits form and how many there are (at most 18 count as a number).
pub fn split_id(code: &str) -> (&str, Option<i64>, usize) {
    let digits = code
        .bytes()
        .rev()
        .take_while(u8::is_ascii_digit)
        .count()
        .min(18);
    let cut = code.len() - digits;
    let number = if digits > 0 {
        code[cut..].parse().ok()
    } else {
        None
    };
    (&code[..cut], number, digits)
}

/// `prefix` and `number` padded with zeros to `width` digits.
pub fn compose(prefix: &str, number: i64, width: usize) -> String {
    format!("{prefix}{number:0width$}")
}

/// Activity IDs in use in a project, handing out new ones.
pub struct Ids {
    taken: HashSet<String>,
}

impl Ids {
    pub fn new(p: &ProjectData) -> Self {
        Self {
            taken: p
                .model
                .activities
                .iter()
                .map(|a| a.task_code.clone())
                .collect(),
        }
    }
    /// Claim `code`; false when it is already in use.
    pub fn take(&mut self, code: &str) -> bool {
        self.taken.insert(code.to_string())
    }
    /// Auto-numbering: prefix plus the next number after the highest one in
    /// use with that prefix (never below the suffix), stepping past IDs in
    /// use. IDs with another prefix, or not ending in digits, do not count.
    pub fn auto(&mut self, n: &Numbering) -> String {
        let width = n.suffix.len();
        let base = n.suffix.parse::<i64>().unwrap_or(0);
        let step = n.step.max(1);
        let highest = self
            .taken
            .iter()
            .filter_map(|c| c.strip_prefix(n.prefix.as_str()))
            .filter(|rest| {
                !rest.is_empty() && rest.len() <= 18 && rest.bytes().all(|b| b.is_ascii_digit())
            })
            .filter_map(|rest| rest.parse::<i64>().ok())
            .max();
        let mut next = highest.map_or(base, |h| h.saturating_add(step).max(base));
        loop {
            let code = compose(&n.prefix, next, width);
            if self.take(&code) {
                return code;
            }
            next = next.saturating_add(step);
        }
    }
    /// Increment from an existing ID: same text and digit count, its number
    /// plus the step, stepping past IDs in use (so a gap after `code` is
    /// filled before anything higher).
    pub fn after(&mut self, code: &str, step: i64) -> String {
        let (prefix, number, width) = split_id(code);
        let step = step.max(1);
        let mut next = number.unwrap_or(0).saturating_add(step);
        loop {
            let code = compose(prefix, next, width);
            if self.take(&code) {
                return code;
            }
            next = next.saturating_add(step);
        }
    }
    /// The ID of a new activity: with "Increment based on selected" on and
    /// a selected activity whose ID ends in digits, that ID plus the step;
    /// otherwise (switch off, nothing selected, or a selected ID without a
    /// trailing number) auto-numbering from the highest ID with the prefix.
    pub fn next(&mut self, n: &Numbering, selected: Option<&str>) -> String {
        match selected {
            Some(code) if n.from_selected && split_id(code).1.is_some() => self.after(code, n.step),
            _ => self.auto(n),
        }
    }
}

/// The Activity ID Add gives a new activity, `selected` being the index of
/// the selected activity.
pub fn new_activity_id(p: &ProjectData, selected: Option<usize>) -> String {
    let selected = selected
        .and_then(|i| p.model.activities.get(i))
        .map(|a| a.task_code.clone());
    Ids::new(p).next(&numbering(p), selected.as_deref())
}

/// Add a new activity under WBS element `wbs` with the project's defaults:
/// its ID from the auto-numbering (`selected` is the index of the selected
/// activity), named [`NEW_ACTIVITY_NAME`], lasting [`NEW_ACTIVITY_MINUTES`]
/// (none for a milestone), with the default activity type, duration type,
/// percent complete type and calendar. Returns its index.
pub fn add_activity(p: &mut ProjectData, wbs: i64, selected: Option<usize>) -> usize {
    let code = new_activity_id(p, selected);
    let i = p.add_activity(wbs);
    let d = defaults(p);
    let minutes = if crate::assignments::is_milestone(d.task_type) {
        0
    } else {
        NEW_ACTIVITY_MINUTES
    };
    let pid = p.model.project.proj_id;
    let a = &mut p.model.activities[i];
    a.task_code = code;
    a.task_type = d.task_type;
    a.rem = minutes;
    if d.calendar < p.model.calendars.len() {
        a.cal = d.calendar;
    }
    let task_id = a.task_id;
    if let Some(detail) = p.details.get_mut(&task_id) {
        detail.name = NEW_ACTIVITY_NAME.into();
        detail.original_duration = minutes;
    }
    records::set_task_value(
        &mut p.document,
        task_id,
        pid,
        "duration_type",
        d.duration_type.into(),
    );
    records::set_task_value(
        &mut p.document,
        task_id,
        pid,
        "complete_pct_type",
        d.percent_type.into(),
    );
    p.date_unscheduled(&[i]);
    i
}
