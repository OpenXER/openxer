//! CALENDAR table edits. Every change goes through the project document and then
//! `ProjectData::rebuild_calendars`, so scheduling, save and export all see it.
use super::records::{format_number, num};
use crate::calendar_data::{propagate_exceptions, CalendarData};
use crate::{Document, ProjectData, Table};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Global,
    Resource,
    Project,
}
impl Kind {
    pub fn code(self) -> &'static str {
        match self {
            Kind::Global => "CA_Base",
            Kind::Resource => "CA_Rsrc",
            Kind::Project => "CA_Project",
        }
    }
    pub fn caption(self) -> &'static str {
        match self {
            Kind::Global => "Global",
            Kind::Resource => "Resource",
            Kind::Project => "Project",
        }
    }
    /// Calendars without a type (hand-made files) behave as global calendars.
    pub fn of(code: &str) -> Kind {
        match code {
            "CA_Rsrc" => Kind::Resource,
            "CA_Project" => Kind::Project,
            _ => Kind::Global,
        }
    }
}

/// Column order of exported CALENDAR tables.
pub const FIELDS: [&str; 13] = [
    "clndr_id",
    "default_flag",
    "clndr_name",
    "proj_id",
    "base_clndr_id",
    "last_chng_date",
    "clndr_type",
    "day_hr_cnt",
    "week_hr_cnt",
    "month_hr_cnt",
    "year_hr_cnt",
    "rsrc_private",
    "clndr_data",
];
pub const HOUR_FIELDS: [&str; 4] = ["day_hr_cnt", "week_hr_cnt", "month_hr_cnt", "year_hr_cnt"];

/// One row of the Calendars list.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub id: String,
    pub name: String,
    pub default: bool,
    pub personal: bool,
}

fn table(doc: &Document) -> Option<&Table> {
    doc.table("CALENDAR")
}
pub fn row_of(doc: &Document, id: &str) -> Option<usize> {
    let t = table(doc)?;
    t.rows.iter().position(|r| t.value(r, "clndr_id") == id)
}
pub fn value(doc: &Document, id: &str, column: &str) -> String {
    row_of(doc, id)
        .and_then(|i| table(doc).map(|t| t.value(&t.rows[i], column).to_string()))
        .unwrap_or_default()
}
fn set(doc: &mut Document, id: &str, column: &str, text: String) {
    if let Some(i) = row_of(doc, id) {
        doc.table_mut("CALENDAR").set(i, column, text);
    }
}
pub fn kind(doc: &Document, id: &str) -> Kind {
    Kind::of(&value(doc, id, "clndr_type"))
}
pub fn name(doc: &Document, id: &str) -> String {
    value(doc, id, "clndr_name")
}
pub fn data(doc: &Document, id: &str) -> Result<CalendarData, String> {
    CalendarData::parse(&value(doc, id, "clndr_data"))
}
pub fn hours(doc: &Document, id: &str) -> [f64; 4] {
    HOUR_FIELDS.map(|c| value(doc, id, c).trim().parse().unwrap_or(0.))
}

/// Calendars of one kind in table order. Project calendars are the current project's.
pub fn list(doc: &Document, kind: Kind, pid: i64) -> Vec<Entry> {
    let Some(t) = table(doc) else {
        return vec![];
    };
    t.rows
        .iter()
        .filter(|r| {
            Kind::of(t.value(r, "clndr_type")) == kind
                && (kind != Kind::Project || num(t.value(r, "proj_id")) == pid)
        })
        .map(|r| Entry {
            id: t.value(r, "clndr_id").into(),
            name: t.value(r, "clndr_name").into(),
            default: t.value(r, "default_flag") == "Y",
            personal: t.value(r, "rsrc_private") == "Y",
        })
        .collect()
}

/// Calendars an activity can use: global calendars and this project's calendars,
/// plus `keep` (the current choice) so an imported assignment stays visible.
pub fn assignable(p: &ProjectData, keep: usize) -> Vec<usize> {
    let pid = p.model.project.proj_id;
    let doc = &p.document;
    (0..p.model.calendars.len())
        .filter(|&i| {
            let id = &p.model.calendars[i].id;
            i == keep
                || match kind(doc, id) {
                    Kind::Global => true,
                    Kind::Project => num(&value(doc, id, "proj_id")) == pid,
                    Kind::Resource => false,
                }
        })
        .collect()
}

/// Where a calendar is referenced.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Usage {
    pub activities: usize,
    pub projects: Vec<String>,
    pub resources: Vec<String>,
    pub derived: Vec<String>,
    pub default_global: bool,
}
impl Usage {
    pub fn is_empty(&self) -> bool {
        self.activities == 0
            && self.projects.is_empty()
            && self.resources.is_empty()
            && self.derived.is_empty()
            && !self.default_global
    }
    pub fn describe(&self) -> String {
        let mut parts = vec![];
        if self.default_global {
            parts.push("it is the default global calendar".to_string());
        }
        if self.activities > 0 {
            parts.push(format!("{} activities use it", self.activities));
        }
        if !self.projects.is_empty() {
            parts.push(format!("projects: {}", self.projects.join(", ")));
        }
        if !self.resources.is_empty() {
            parts.push(format!("resources: {}", self.resources.join(", ")));
        }
        if !self.derived.is_empty() {
            parts.push(format!(
                "calendars derived from it: {}",
                self.derived.join(", ")
            ));
        }
        parts.join("; ")
    }
}

pub fn usage(p: &ProjectData, id: &str) -> Usage {
    let doc = &p.document;
    let pid = p.model.project.proj_id;
    let mut u = Usage {
        default_global: kind(doc, id) == Kind::Global && value(doc, id, "default_flag") == "Y",
        ..Usage::default()
    };
    u.activities = p
        .model
        .activities
        .iter()
        .filter(|a| p.model.calendars.get(a.cal).is_some_and(|c| c.id == id))
        .count();
    let mut project_ids = vec![];
    let project_default = p
        .model
        .calendars
        .get(p.model.project_calendar)
        .map(|c| c.id.as_str())
        == Some(id);
    if project_default || u.activities > 0 {
        project_ids.push(pid);
    }
    if let Some(t) = doc.table("TASK") {
        for r in &t.rows {
            let owner = num(t.value(r, "proj_id"));
            if owner != pid && t.value(r, "clndr_id") == id {
                u.activities += 1;
                if !project_ids.contains(&owner) {
                    project_ids.push(owner);
                }
            }
        }
    }
    if let Some(t) = doc.table("PROJECT") {
        for r in &t.rows {
            let owner = num(t.value(r, "proj_id"));
            if owner != pid && t.value(r, "clndr_id") == id && !project_ids.contains(&owner) {
                project_ids.push(owner);
            }
        }
        for owner in project_ids {
            let name = t
                .rows
                .iter()
                .find(|r| num(t.value(r, "proj_id")) == owner)
                .map_or(owner.to_string(), |r| t.value(r, "proj_short_name").into());
            u.projects.push(name);
        }
    }
    if let Some(t) = doc.table("RSRC") {
        for r in &t.rows {
            if t.value(r, "clndr_id") == id {
                let short = t.value(r, "rsrc_short_name");
                u.resources.push(if short.is_empty() {
                    t.value(r, "rsrc_name").into()
                } else {
                    short.into()
                });
            }
        }
    }
    if let Some(t) = table(doc) {
        for r in &t.rows {
            if t.value(r, "base_clndr_id") == id {
                u.derived.push(t.value(r, "clndr_name").into());
            }
        }
    }
    u
}

/// Run a table edit and rebuild the scheduling calendars; on error the table is restored.
fn edit(
    p: &mut ProjectData,
    change: impl FnOnce(&mut Document) -> Result<(), String>,
) -> Result<(), String> {
    let before = table(&p.document).cloned();
    let result = change(&mut p.document).and_then(|_| p.rebuild_calendars());
    if result.is_err() {
        match before {
            Some(t) => *p.document.table_mut("CALENDAR") = t,
            None => p
                .document
                .tables
                .retain(|t| !t.name.eq_ignore_ascii_case("CALENDAR")),
        }
    }
    result
}

fn ensure_table(doc: &mut Document) -> &mut Table {
    let t = doc.table_mut("CALENDAR");
    if t.fields.is_empty() {
        t.fields = FIELDS.iter().map(|s| s.to_string()).collect();
    }
    t
}

/// A standard 5-day, 8-hour week for calendars created without a source.
pub fn standard_week() -> CalendarData {
    let mut data = CalendarData::default();
    for shifts in &mut data.week[1..6] {
        *shifts = vec![(480, 720), (780, 1020)];
    }
    data
}

/// Add a calendar of `kind`, copied from `source` when given. Project and resource
/// calendars copied from a global calendar inherit its holidays and exceptions.
pub fn add(
    p: &mut ProjectData,
    kind: Kind,
    source: Option<&str>,
    stamp: &str,
) -> Result<String, String> {
    let pid = p.model.project.proj_id;
    let id = crate::edit::records::next_id(&p.document, "CALENDAR", "clndr_id").to_string();
    let (data, hours, base) = match source {
        Some(src) => (
            value(&p.document, src, "clndr_data"),
            hours(&p.document, src),
            (kind != Kind::Global && self::kind(&p.document, src) == Kind::Global)
                .then(|| src.to_string()),
        ),
        None => (standard_week().serialize(), [8., 40., 172., 2000.], None),
    };
    let new_id = id.clone();
    edit(p, move |doc| {
        let mut values = vec![
            ("clndr_id", new_id),
            ("default_flag", "N".to_string()),
            ("clndr_name", "New Calendar".to_string()),
            (
                "proj_id",
                if kind == Kind::Project {
                    pid.to_string()
                } else {
                    String::new()
                },
            ),
            ("base_clndr_id", base.unwrap_or_default()),
            ("last_chng_date", stamp.to_string()),
            ("clndr_type", kind.code().to_string()),
            ("rsrc_private", "N".to_string()),
            ("clndr_data", data),
        ];
        for (field, h) in HOUR_FIELDS.iter().zip(hours) {
            values.push((field, format_number(h)));
        }
        ensure_table(doc).add(&values);
        Ok(())
    })?;
    Ok(id)
}

pub fn delete(p: &mut ProjectData, id: &str) -> Result<(), String> {
    let u = usage(p, id);
    if !u.is_empty() {
        return Err(format!(
            "\"{}\" cannot be deleted because {}. Assign another calendar first, or delete it \
             with a replacement calendar.",
            name(&p.document, id),
            u.describe()
        ));
    }
    let id = id.to_string();
    edit(p, move |doc| {
        let i = row_of(doc, &id).ok_or("The calendar no longer exists")?;
        doc.table_mut("CALENDAR").rows.remove(i);
        Ok(())
    })
}

/// The default global calendar, if one is marked.
pub fn default_global(doc: &Document) -> Option<String> {
    list(doc, Kind::Global, 0)
        .into_iter()
        .find(|e| e.default)
        .map(|e| e.id)
}

/// What deleting a calendar needs: `Ok(false)` when nothing uses it (a
/// plain delete), `Ok(true)` when something does and a replacement must be
/// chosen first, `Err` with the reason when it cannot be deleted at all.
pub fn delete_check(p: &ProjectData, id: &str) -> Result<bool, String> {
    if row_of(&p.document, id).is_none() {
        return Err("The calendar no longer exists.".into());
    }
    let u = usage(p, id);
    if u.default_global {
        return Err(format!(
            "\"{}\" is the default global calendar, so it cannot be deleted. Make another \
             global calendar the default first.",
            name(&p.document, id)
        ));
    }
    Ok(!u.is_empty())
}

/// The global calendars that can take the place of `id` when it is deleted.
pub fn replacements(doc: &Document, id: &str) -> Vec<Entry> {
    list(doc, Kind::Global, 0)
        .into_iter()
        .filter(|e| e.id != id)
        .collect()
}

/// Delete calendar `id` and give everything that used it the global
/// calendar `replacement` instead: activities in every project, projects
/// (their default calendar), resources, and calendars that inherited from
/// it. All of it or nothing changes. Returns how many activities moved.
pub fn delete_replacing(p: &mut ProjectData, id: &str, replacement: &str) -> Result<usize, String> {
    delete_check(p, id)?;
    if replacement == id {
        return Err("A calendar cannot replace itself.".into());
    }
    if row_of(&p.document, replacement).is_none() {
        return Err("The replacement calendar no longer exists.".into());
    }
    if kind(&p.document, replacement) != Kind::Global {
        return Err("The replacement must be a global calendar.".into());
    }
    let moved = usage(p, id).activities;
    let before = p.clone();
    let result = replace_and_remove(p, id, replacement);
    if result.is_err() {
        *p = before;
    }
    result.map(|_| moved)
}

fn replace_and_remove(p: &mut ProjectData, id: &str, replacement: &str) -> Result<(), String> {
    let old = p.model.calendars.iter().position(|c| c.id == id);
    let new = p
        .model
        .calendars
        .iter()
        .position(|c| c.id == replacement)
        .ok_or("The replacement calendar cannot be read.")?;
    if let Some(old) = old {
        for a in p.model.activities.iter_mut().filter(|a| a.cal == old) {
            a.cal = new;
        }
        if p.model.project_calendar == old {
            p.model.project_calendar = new;
        }
    }
    for (name, column) in [
        ("TASK", "clndr_id"),
        ("PROJECT", "clndr_id"),
        ("RSRC", "clndr_id"),
        ("CALENDAR", "base_clndr_id"),
    ] {
        let Some(t) = p
            .document
            .tables
            .iter_mut()
            .find(|t| t.name.eq_ignore_ascii_case(name))
        else {
            continue;
        };
        let Some(c) = t.field(column) else {
            continue;
        };
        for r in t.rows.iter_mut() {
            if let Some(cell) = r.get_mut(c).filter(|v| v.as_str() == id) {
                *cell = replacement.to_string();
            }
        }
    }
    let i = row_of(&p.document, id).ok_or("The calendar no longer exists.")?;
    p.document.table_mut("CALENDAR").rows.remove(i);
    p.rebuild_calendars()?;
    p.refresh_resource_inputs();
    Ok(())
}

pub fn rename(p: &mut ProjectData, id: &str, text: &str) -> Result<(), String> {
    let text = super::records::clean(text).trim().to_string();
    if text.is_empty() {
        return Err("Calendar Name cannot be blank.".into());
    }
    let pid = p.model.project.proj_id;
    let k = kind(&p.document, id);
    if list(&p.document, k, pid)
        .iter()
        .any(|e| e.id != id && e.name.eq_ignore_ascii_case(&text))
    {
        return Err(format!(
            "A {} calendar named \"{text}\" already exists.",
            k.caption().to_lowercase()
        ));
    }
    let id = id.to_string();
    edit(p, move |doc| {
        set(doc, &id, "clndr_name", text);
        Ok(())
    })
}

/// Make a global calendar the default global calendar.
pub fn set_default(p: &mut ProjectData, id: &str) -> Result<(), String> {
    if kind(&p.document, id) != Kind::Global {
        return Err("Only a global calendar can be the default.".into());
    }
    let id = id.to_string();
    edit(p, move |doc| {
        let t = doc.table_mut("CALENDAR");
        for i in 0..t.rows.len() {
            if Kind::of(t.value(&t.rows[i], "clndr_type")) == Kind::Global {
                let flag = if t.value(&t.rows[i], "clndr_id") == id {
                    "Y"
                } else {
                    "N"
                };
                t.set(i, "default_flag", flag.into());
            }
        }
        Ok(())
    })
}

/// Project calendar to global, or resource calendar between shared and personal.
pub fn convert(p: &mut ProjectData, id: &str, to: Convert, stamp: &str) -> Result<(), String> {
    let k = kind(&p.document, id);
    match (to, k) {
        (Convert::ToGlobal, Kind::Project)
        | (Convert::Shared | Convert::Personal, Kind::Resource) => {}
        _ => return Err("This conversion does not apply to the selected calendar.".into()),
    }
    let id = id.to_string();
    let stamp = stamp.to_string();
    edit(p, move |doc| {
        match to {
            Convert::ToGlobal => {
                set(doc, &id, "clndr_type", Kind::Global.code().into());
                set(doc, &id, "proj_id", String::new());
                set(doc, &id, "base_clndr_id", String::new());
                set(doc, &id, "default_flag", "N".into());
            }
            Convert::Shared => set(doc, &id, "rsrc_private", "N".into()),
            Convert::Personal => set(doc, &id, "rsrc_private", "Y".into()),
        }
        set(doc, &id, "last_chng_date", stamp);
        Ok(())
    })
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Convert {
    ToGlobal,
    Shared,
    Personal,
}

/// Write an edited calendar. Exception changes on a calendar flow into the
/// calendars derived from it wherever those still matched it.
pub fn commit(
    p: &mut ProjectData,
    id: &str,
    new: &CalendarData,
    new_hours: [f64; 4],
    stamp: &str,
) -> Result<bool, String> {
    let old = data(&p.document, id)?;
    let old_hours = hours(&p.document, id);
    if old == *new && old_hours == new_hours {
        return Ok(false);
    }
    let derived: Vec<String> = table(&p.document)
        .map(|t| {
            t.rows
                .iter()
                .filter(|r| t.value(r, "base_clndr_id") == id && t.value(r, "clndr_id") != id)
                .map(|r| t.value(r, "clndr_id").to_string())
                .collect()
        })
        .unwrap_or_default();
    let id = id.to_string();
    let new = new.clone();
    let stamp = stamp.to_string();
    edit(p, move |doc| {
        if old != new {
            set(doc, &id, "clndr_data", new.serialize());
            for child in derived {
                let mut d = data(doc, &child)?;
                if propagate_exceptions(&old, &new, &mut d) {
                    set(doc, &child, "clndr_data", d.serialize());
                    set(doc, &child, "last_chng_date", stamp.clone());
                }
            }
        }
        for ((field, before), after) in HOUR_FIELDS.iter().zip(old_hours).zip(new_hours) {
            if before != after {
                set(doc, &id, field, format_number(after));
            }
        }
        set(doc, &id, "last_chng_date", stamp);
        Ok(())
    })?;
    Ok(true)
}

/// Timestamp for `last_chng_date`.
pub fn now_stamp() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M").to_string()
}
