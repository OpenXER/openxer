//! Writing typed text into a field: the text read into the change the
//! activity's details would make for the same field ([`edit_for`]), then
//! made through the shared setters ([`write`]).
use super::{display, parse_input, value, Editor, Field, FieldDef, Formats, Index, Source};
use super::{strip_date_marker, Value};
use crate::edit::activity::{self as act, Edit, Lock, UdfValue, MAX_DURATION, STATUSES};
use crate::edit::records::{self, num};
use crate::{constraint_label, status_label, type_label, ProjectData};
use openxer_engine::calendar::DAY;
use openxer_engine::model::{Activity, CstrType, TaskType};

/// Does a date field mark the end of work? Finish-type fields take the end
/// of the working day when a date is typed without a time; start-type
/// fields (and user defined dates) take its start. Constraint dates follow
/// their constraint type.
pub fn finish_type(field: Field, a: &Activity) -> bool {
    let constraint = |kind: CstrType| {
        matches!(
            kind,
            CstrType::FinishOn
                | CstrType::FinishOnBefore
                | CstrType::FinishOnAfter
                | CstrType::MandatoryFinish
        )
    };
    match field {
        Field::Finish
        | Field::EarlyFinish
        | Field::LateFinish
        | Field::ActualFinish
        | Field::PlannedFinish
        | Field::RemainingEarlyFinish
        | Field::RemainingLateFinish
        | Field::ExpectedFinish => true,
        Field::PrimaryConstraintDate => constraint(a.cstr),
        Field::SecondaryConstraintDate => constraint(a.cstr2),
        _ => false,
    }
}

/// Minute of day `day` that a date typed without a time takes for
/// activity `i`: the start of its calendar's working day, or the end for
/// a finish-type field. The working day is the shifts that start on it
/// (a shift belongs to the day it starts on, CAL-1), taken in time order
/// whatever order they are stored in: its start is the earliest shift
/// start and its end the latest shift end, which for an overnight shift
/// is the next morning. A day off uses the next working day's hours;
/// 08:00 and 17:00 when the calendar has none.
pub fn day_edge(p: &ProjectData, i: usize, finish: bool, day: i64) -> i64 {
    let m = &p.model;
    let calendar = m.activities.get(i).and_then(|a| m.calendars.get(a.cal));
    if let Some(cal) = calendar {
        for d in day..day + 14 {
            let shifts = cal.cal.shifts_on(d);
            let start = shifts.iter().map(|s| s.0).min();
            let end = shifts.iter().map(|s| s.1).max();
            if let (Some(start), Some(end)) = (start, end) {
                return if finish { end } else { start };
            }
        }
    }
    if finish {
        17 * 60
    } else {
        8 * 60
    }
}

/// Read a date typed into a date field of activity `i` in `f`. With a value
/// already there (`current`) a date without a time keeps that time; an
/// empty field takes the start (or for `finish`, the end) of the activity
/// calendar's working day instead of midnight.
pub fn parse_field_date(
    p: &ProjectData,
    f: &dyn Formats,
    i: usize,
    finish: bool,
    text: &str,
    current: Option<i64>,
) -> Result<Option<i64>, String> {
    let origin = p.model.origin_serial;
    if current.is_some() {
        return f.parse_date(origin, text, current);
    }
    let Some(plain) = f.parse_date(origin, text, None)? else {
        return Ok(None);
    };
    // A typed time wins: the result does not depend on the time kept.
    if f.parse_date(origin, text, Some(12 * 60 + 34))? != Some(plain) {
        let day = plain.div_euclid(DAY);
        return Ok(Some(day * DAY + day_edge(p, i, finish, day)));
    }
    Ok(Some(plain))
}

/// The shared lock a field is checked against.
pub fn lock_of(field: Field) -> Lock {
    match field {
        Field::OriginalDuration => Lock::OriginalDuration,
        Field::RemainingDuration => Lock::RemainingDuration,
        Field::AtCompletionDuration => Lock::AtCompletionDuration,
        Field::ActualDuration => Lock::ActualDuration,
        Field::ExpectedFinish => Lock::ExpectedFinish,
        Field::Finish => Lock::Finish,
        Field::PhysicalPercent => Lock::PhysicalPercent,
        Field::DurationPercent => Lock::DurationPercent,
        Field::ActivityPercent => Lock::ActivityPercent,
        Field::Figure(measure, phase) => Lock::Figure(measure, phase),
        Field::ThisPeriod(measure) => Lock::ThisPeriod(measure),
        Field::SuspendDate => Lock::SuspendDate,
        Field::ResumeDate => Lock::ResumeDate,
        Field::PrimaryConstraintDate => Lock::PrimaryConstraintDate,
        Field::SecondaryConstraintDate => Lock::SecondaryConstraintDate,
        _ => Lock::Open,
    }
}

/// The resources assigned to activity `i`, by name, each once.
pub fn assigned_resources(p: &ProjectData, i: usize) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    let Some(a) = p.model.activities.get(i) else {
        return out;
    };
    let pid = p.model.project.proj_id;
    let doc = &p.document;
    if let Some(t) = doc.table("TASKRSRC") {
        for r in records::rows_for(doc, "TASKRSRC", a.task_id, pid) {
            let id = num(t.value(&t.rows[r], "rsrc_id"));
            let name = p
                .resource_names
                .get(&id)
                .cloned()
                .unwrap_or_else(|| id.to_string());
            if id != 0 && !out.contains(&name) {
                out.push(name);
            }
        }
    }
    out
}

/// Read typed text for field `def` of activity `i` into the change the
/// activity's details would make for the same field. `Ok(None)` means the
/// field is written directly (Activity ID and Activity Name).
pub fn edit_for(
    p: &ProjectData,
    ix: &Index,
    f: &dyn Formats,
    def: &FieldDef,
    i: usize,
    text: &str,
) -> Result<Option<Edit>, String> {
    let typed = || parse_input(p, f, Some(i), def.kind, text);
    let minutes = |what: &str| -> Result<i64, String> {
        match typed() {
            Ok(Value::Duration(m)) if (0..=MAX_DURATION).contains(&m) => Ok(m),
            // The range in the unit the durations are shown in.
            _ => Err(format!(
                "{what} must be from {} to {}.",
                f.duration(p, Some(i), 0),
                f.duration(p, Some(i), MAX_DURATION)
            )),
        }
    };
    // A typed date keeps the field's time of day when the format shows
    // none, as the details date fields do.
    let current = match value(p, ix, def.source, def.kind, i) {
        Value::Date { minute, .. } => Some(minute),
        _ => None,
    };
    let finish = match def.source {
        Source::Field(field) => finish_type(field, &p.model.activities[i]),
        _ => false,
    };
    let date = || -> Result<Option<i64>, String> {
        let bare = strip_date_marker(text);
        if let Ok(Some(v)) = parse_field_date(p, f, i, finish, bare, current) {
            return Ok(Some(v));
        }
        match typed()? {
            Value::Date { minute, .. } => Ok(Some(minute)),
            _ => Ok(None),
        }
    };
    let needed = |what: &str| -> Result<i64, String> {
        date()?.ok_or_else(|| format!("{what} needs a date."))
    };
    let percent = |what: &str| -> Result<f64, String> {
        match typed() {
            Ok(Value::Percent(v)) if (0.0..=100.).contains(&v) => Ok(v),
            _ => Err(format!("{what} must be from 0 to 100.")),
        }
    };
    // Units (in hours) or a cost; cleared means zero.
    let amount = || -> Result<f64, String> {
        match typed() {
            Ok(Value::Units(v) | Value::Cost(v)) if v >= 0. => Ok(v),
            Ok(Value::Empty) => Ok(0.),
            Ok(Value::Units(_) | Value::Cost(_)) => {
                Err(format!("{} must be zero or more.", def.title))
            }
            Err(e) => Err(e),
            Ok(_) => Err(format!("{} must be a number.", def.title)),
        }
    };
    let constraint = |text: &str| -> Result<CstrType, String> {
        if text.is_empty() {
            return Ok(CstrType::None);
        }
        (0..=9)
            .filter_map(CstrType::from_code)
            .find(|c| constraint_label(*c).eq_ignore_ascii_case(text))
            .ok_or_else(|| format!("{text:?} is not a constraint type."))
    };
    let field = match def.source {
        Source::Code(type_id) => {
            let value = if text.is_empty() {
                None
            } else {
                let found = ix
                    .code_values(p)
                    .iter()
                    .find(|(_, v)| v.type_id == type_id && v.short_name.eq_ignore_ascii_case(text))
                    .map(|(id, _)| *id);
                Some(found.ok_or_else(|| format!("{text:?} is not a {} value.", def.title))?)
            };
            return Ok(Some(Edit::Code { type_id, value }));
        }
        Source::Udf(type_id) => {
            let value = match def.editor {
                Editor::Text | Editor::Choice => UdfValue::Text(text.into()),
                Editor::Date => UdfValue::Date(date()?),
                _ if text.is_empty() => UdfValue::Number(None),
                _ => match typed() {
                    Ok(v) => UdfValue::Number(v.number()),
                    Err(_) => return Err(format!("{} must be a number.", def.title)),
                },
            };
            return Ok(Some(Edit::Udf { type_id, value }));
        }
        Source::Field(field) => field,
    };
    Ok(Some(match field {
        Field::ActivityId | Field::ActivityName => return Ok(None),
        Field::ActivityType => Edit::ActivityType(
            [
                TaskType::Task,
                TaskType::Resource,
                TaskType::Loe,
                TaskType::Mile,
                TaskType::FinMile,
                TaskType::Wbs,
            ]
            .into_iter()
            .find(|t| type_label(*t).eq_ignore_ascii_case(text))
            .ok_or_else(|| format!("{text:?} is not an activity type."))?,
        ),
        Field::ActivityStatus => Edit::Status(
            STATUSES
                .into_iter()
                .find(|s| status_label(*s).eq_ignore_ascii_case(text))
                .ok_or_else(|| format!("{text:?} is not an activity status."))?,
        ),
        Field::Wbs => {
            let w = ix.wbs(p);
            let by_full = w.codes.iter().find(|(_, c)| c.eq_ignore_ascii_case(text));
            let id = match by_full {
                Some((id, _)) => *id,
                None => {
                    let short: Vec<i64> = w
                        .short_codes
                        .iter()
                        .filter(|(_, c)| c.eq_ignore_ascii_case(text))
                        .map(|(id, _)| *id)
                        .collect();
                    match short.as_slice() {
                        [one] => *one,
                        [] => return Err(format!("No WBS element {text:?}.")),
                        _ => {
                            return Err(format!(
                                "{text:?} matches several WBS elements; use the full code."
                            ))
                        }
                    }
                }
            };
            Edit::Wbs(id)
        }
        Field::Calendar => {
            let current = p.model.activities[i].cal;
            Edit::Calendar(
                crate::edit::calendars::assignable(p, current)
                    .into_iter()
                    .find(|&c| p.calendar_names[c].eq_ignore_ascii_case(text))
                    .ok_or_else(|| format!("No calendar {text:?} for this project."))?,
            )
        }
        Field::PrimaryResource => {
            if text.is_empty() {
                Edit::PrimaryResource(0)
            } else {
                let name = assigned_resources(p, i)
                    .into_iter()
                    .find(|k| k.eq_ignore_ascii_case(text))
                    .ok_or_else(|| {
                        format!("{text:?} is not a resource assigned to this activity.")
                    })?;
                let id = p
                    .resource_names
                    .iter()
                    .find(|(_, n)| **n == name)
                    .map(|(id, _)| *id)
                    .or_else(|| name.parse().ok())
                    .unwrap_or(0);
                Edit::PrimaryResource(id)
            }
        }
        Field::DurationType => Edit::DurationType(
            act::duration_type_code(text)
                .ok_or_else(|| format!("{text:?} is not a duration type."))?,
        ),
        Field::PercentType => Edit::PercentType(
            act::percent_type_code(text)
                .ok_or_else(|| format!("{text:?} is not a percent complete type."))?,
        ),
        Field::AutoComputeActuals => match typed() {
            Ok(Value::Flag(on)) => Edit::AutoComputeActuals(on),
            _ => return Err("Enter Yes or No.".into()),
        },
        Field::Start => Edit::Start(needed("Start")?),
        Field::Finish => Edit::Finish(needed("Finish")?),
        Field::ActualStart => Edit::ActualStart(date()?),
        Field::ActualFinish => Edit::ActualFinish(date()?),
        Field::PlannedStart => Edit::PlannedStart(needed("Planned Start")?),
        Field::PlannedFinish => Edit::PlannedFinish(needed("Planned Finish")?),
        Field::ExpectedFinish => Edit::ExpectedFinish(date()?),
        Field::SuspendDate => Edit::SuspendDate(date()?),
        Field::ResumeDate => Edit::ResumeDate(date()?),
        Field::OriginalDuration => Edit::OriginalDuration(minutes("Original Duration")?),
        Field::RemainingDuration => Edit::RemainingDuration(minutes("Remaining Duration")?),
        Field::ActualDuration => Edit::ActualDuration(minutes("Actual Duration")?),
        Field::AtCompletionDuration => {
            Edit::AtCompletionDuration(minutes("At Completion Duration")?)
        }
        Field::PhysicalPercent => Edit::PhysicalPercent(percent("Physical % Complete")?),
        Field::ActivityPercent => Edit::ActivityPercent(percent("Activity % Complete")?),
        Field::DurationPercent => Edit::DurationPercent(percent("Duration % Complete")?),
        Field::PrimaryConstraint => Edit::Constraint {
            secondary: false,
            kind: constraint(text)?,
        },
        Field::SecondaryConstraint => Edit::Constraint {
            secondary: true,
            kind: constraint(text)?,
        },
        Field::PrimaryConstraintDate => Edit::ConstraintDate {
            secondary: false,
            date: Some(date()?.ok_or("A constraint needs a date.")?),
        },
        Field::SecondaryConstraintDate => Edit::ConstraintDate {
            secondary: true,
            date: Some(date()?.ok_or("A constraint needs a date.")?),
        },
        Field::Figure(measure, phase) => Edit::Figure {
            measure,
            phase,
            value: amount()?,
        },
        Field::ThisPeriod(measure) => Edit::ThisPeriod {
            measure,
            value: amount()?,
        },
        _ => return Err(format!("{} is not editable.", def.title)),
    }))
}

/// Write typed text into field `def` of activity `i` with the shared
/// setters, as an in-place edit does; durations in messages are written
/// by `duration_text`. Returns whether anything changed; on `Err` nothing
/// did. A unit or cost typed as the field already shows it is no change.
pub fn write(
    p: &mut ProjectData,
    f: &dyn Formats,
    def: &FieldDef,
    i: usize,
    text: &str,
    duration_text: act::DurationText,
) -> Result<bool, String> {
    if p.model.activities.get(i).is_none() {
        return Err("No such activity.".into());
    }
    if def.editor == Editor::ReadOnly {
        return Err(format!("{} is not editable.", def.title));
    }
    if let Source::Field(field) = def.source {
        act::field_lock(p, i, lock_of(field))?;
    }
    let text = text.trim();
    let ix = Index::for_project(p);
    if matches!(
        def.source,
        Source::Field(Field::Figure(..) | Field::ThisPeriod(_))
    ) && display(p, f, Some(i), &value(p, &ix, def.source, def.kind, i)) == text
    {
        return Ok(false);
    }
    match edit_for(p, &ix, f, def, i, text)? {
        Some(edit) => Ok(act::edit_activity_with(p, i, edit, duration_text)?.changed),
        None => match def.source {
            Source::Field(Field::ActivityId) => act::set_activity_id(p, i, text),
            Source::Field(Field::ActivityName) => act::set_activity_name(p, i, text),
            _ => Err(format!("{} is not editable.", def.title)),
        },
    }
}
