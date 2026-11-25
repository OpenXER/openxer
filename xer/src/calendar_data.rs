//! The CALENDAR `clndr_data` text: a nested `(0||name(attrs)(children))` tree that
//! holds the standard workweek (`DaysOfWeek`, day 1 = Sunday), the optional
//! `VIEW(ShowTotal|Y)` display flag and dated `Exceptions` keyed by Excel serial day.
//!
//! Untouched calendars keep their original text in the document. Edited calendars
//! are written in the indented form current exports use: each child on its own
//! line (`\x7f\x7f` line break plus two spaces per level) and the closing
//! parentheses run together on the last child.
use super::{calendar, node, Node, ProjectData};
use std::collections::{BTreeMap, HashMap};

/// A work period in minutes from midnight. The finish is normalized to
/// `start < finish <= start + 1440`, so `00:00` as a finish means midnight.
pub type Shift = (i64, i64);
/// Line break inside XER text values.
pub const LINE: &str = "\u{7f}\u{7f}";
pub const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tues", "Wed", "Thu", "Fri", "Sat"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CalendarData {
    /// Name of the outer node, normally `CalendarData` (some files leave it empty).
    pub root: String,
    /// Standard workweek, index 0 = Sunday.
    pub week: [Vec<Shift>; 7],
    /// `VIEW(ShowTotal|...)`: show total hours per day instead of detailed hours.
    pub show_total: Option<bool>,
    /// Excel serial day (1899-12-30 = 0) to that day's periods; empty = nonwork.
    pub exceptions: BTreeMap<i64, Vec<Shift>>,
    /// Unrecognized top-level nodes, written back after the known ones.
    other: Vec<Node>,
}

impl Default for CalendarData {
    fn default() -> Self {
        Self {
            root: "CalendarData".into(),
            week: std::array::from_fn(|_| vec![]),
            show_total: None,
            exceptions: BTreeMap::new(),
            other: vec![],
        }
    }
}

/// A shift time `HH:MM` in minutes from midnight, within 00:00 to 24:59.
pub(crate) fn shift_minutes(text: &str) -> Result<i64, String> {
    minutes(text)
}
fn minutes(text: &str) -> Result<i64, String> {
    let (h, m) = text.split_once(':').ok_or("Invalid shift time")?;
    let h = h.trim().parse::<i64>().map_err(|_| "Invalid shift hour")?;
    let m = m
        .trim()
        .parse::<i64>()
        .map_err(|_| "Invalid shift minute")?;
    if !(0..=24).contains(&h) || !(0..60).contains(&m) {
        return Err("Shift time outside the day".into());
    }
    Ok(h * 60 + m)
}
fn clock(minutes: i64) -> String {
    let m = minutes.rem_euclid(1440);
    format!("{:02}:{:02}", m / 60, m % 60)
}
fn shifts(n: &Node) -> Result<Vec<Shift>, String> {
    n.children
        .iter()
        .map(|c| {
            let s = minutes(c.attrs.get("s").ok_or("Shift start missing")?)? % 1440;
            let f = minutes(c.attrs.get("f").ok_or("Shift finish missing")?)?;
            Ok((s, if f <= s { f + 1440 } else { f }))
        })
        .collect()
}

/// Weekday index (0 = Sunday) of an Excel serial day.
pub fn weekday(serial: i64) -> usize {
    (serial + 6).rem_euclid(7) as usize
}
/// Total minutes of a list of periods.
pub fn total(shifts: &[Shift]) -> i64 {
    shifts.iter().map(|(s, f)| f - s).sum()
}

impl CalendarData {
    pub fn parse(text: &str) -> Result<Self, String> {
        let normalized = text.replace(LINE, "\r\n").replace('\u{7f}', "\n");
        let root = node(normalized.trim(), &mut 0)?;
        let mut data = Self {
            root: root.name.clone(),
            ..Self::default()
        };
        for n in root.children {
            match n.name.as_str() {
                "DaysOfWeek" => {
                    for day in &n.children {
                        let index = day.name.parse::<usize>().map_err(|_| "Invalid weekday")?;
                        if !(1..=7).contains(&index) {
                            return Err("Weekday outside 1..7".into());
                        }
                        data.week[index - 1] = shifts(day)?;
                    }
                }
                "Exceptions" => {
                    for day in &n.children {
                        let serial = day
                            .attrs
                            .get("d")
                            .ok_or("Exception date missing")?
                            .trim()
                            .parse::<i64>()
                            .map_err(|_| "Invalid exception date")?;
                        data.exceptions.insert(serial, shifts(day)?);
                    }
                }
                "VIEW" if n.attrs.contains_key("ShowTotal") => {
                    data.show_total = Some(n.attrs["ShowTotal"] == "Y");
                }
                _ => data.other.push(n),
            }
        }
        Ok(data)
    }

    /// Indented text as written by current exports.
    pub fn serialize(&self) -> String {
        let shift_nodes = |list: &[Shift]| -> Vec<Node> {
            list.iter()
                .enumerate()
                .map(|(i, &(s, f))| {
                    leaf(&i.to_string(), &[("s", clock(s)), ("f", clock(f))], vec![])
                })
                .collect()
        };
        let days = (0..7)
            .map(|d| leaf(&(d + 1).to_string(), &[], shift_nodes(&self.week[d])))
            .collect();
        let mut children = vec![leaf("DaysOfWeek", &[], days)];
        if let Some(total) = self.show_total {
            children.push(leaf(
                "VIEW",
                &[("ShowTotal", if total { "Y" } else { "N" }.into())],
                vec![],
            ));
        }
        let exceptions = self
            .exceptions
            .iter()
            .enumerate()
            .map(|(i, (serial, list))| {
                leaf(
                    &i.to_string(),
                    &[("d", serial.to_string())],
                    shift_nodes(list),
                )
            })
            .collect();
        children.push(leaf("Exceptions", &[], exceptions));
        children.extend(self.other.iter().cloned());
        let mut out = String::new();
        write_node(&mut out, &self.root, &[], &children, 0);
        out
    }

    /// The periods that apply on an Excel serial day.
    pub fn shifts_on(&self, serial: i64) -> &[Shift] {
        self.exceptions
            .get(&serial)
            .unwrap_or(&self.week[weekday(serial)])
    }
    /// Make a date follow the standard workweek again.
    pub fn set_standard(&mut self, serial: i64) {
        self.exceptions.remove(&serial);
    }
    /// Set a date's periods. A date that matches its workweek day stops being an exception.
    pub fn set_day(&mut self, serial: i64, shifts: Vec<Shift>) {
        if shifts == self.week[weekday(serial)] {
            self.exceptions.remove(&serial);
        } else {
            self.exceptions.insert(serial, shifts);
        }
    }
    pub fn week_minutes(&self) -> i64 {
        self.week.iter().map(|d| total(d)).sum()
    }
}

/// Copy a base calendar's exception changes into a calendar derived from it. Dates
/// where the derived calendar already differed from the old base keep their own
/// value. Returns whether the derived calendar changed.
pub fn propagate_exceptions(
    old_base: &CalendarData,
    new_base: &CalendarData,
    derived: &mut CalendarData,
) -> bool {
    let mut changed = false;
    let dates: Vec<i64> = old_base
        .exceptions
        .keys()
        .chain(new_base.exceptions.keys())
        .copied()
        .collect();
    for date in dates {
        let (before, after) = (
            old_base.exceptions.get(&date),
            new_base.exceptions.get(&date),
        );
        if before == after || derived.exceptions.get(&date) != before {
            continue;
        }
        match after {
            Some(list) => derived.exceptions.insert(date, list.clone()),
            None => derived.exceptions.remove(&date),
        };
        changed = true;
    }
    changed
}

fn leaf(name: &str, attrs: &[(&str, String)], children: Vec<Node>) -> Node {
    Node {
        name: name.into(),
        attrs: attrs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
        order: attrs.iter().map(|(k, _)| k.to_string()).collect(),
        children,
    }
}
fn write_node(out: &mut String, name: &str, attrs: &[String], children: &[Node], depth: usize) {
    out.push_str("(0||");
    out.push_str(name);
    out.push('(');
    out.push_str(&attrs.join("|"));
    out.push_str(")(");
    for child in children {
        out.push_str(LINE);
        out.push_str(&"  ".repeat(depth + 1));
        let pairs: Vec<String> = child
            .order
            .iter()
            .flat_map(|k| [k.clone(), child.attrs[k].clone()])
            .collect();
        write_node(out, &child.name, &pairs, &child.children, depth + 1);
    }
    out.push_str("))");
}

impl ProjectData {
    /// Rebuild the scheduling calendars and their names from the CALENDAR table after
    /// an edit. Activities, the project default and resource demands keep pointing at
    /// the same `clndr_id`. On error nothing changes.
    pub fn rebuild_calendars(&mut self) -> Result<(), String> {
        let origin = self.model.origin_serial;
        let mut calendars = vec![];
        let mut names = vec![];
        let mut index = HashMap::new();
        // The import's rules: the first readable row of a calendar ID is
        // used, and a calendar the project did not have (one that could not
        // be read on import) is skipped when it still cannot be read.
        let known: std::collections::HashSet<&str> =
            self.model.calendars.iter().map(|c| c.id.as_str()).collect();
        if let Some(t) = self.document.table("CALENDAR") {
            for r in &t.rows {
                let id = t.value(r, "clndr_id");
                if index.contains_key(id) {
                    continue;
                }
                let c = match calendar(id, t.value(r, "clndr_data"), origin) {
                    Ok(c) => c,
                    Err(_) if !known.contains(id) => continue,
                    Err(e) => return Err(format!("Calendar {}: {e}", t.value(r, "clndr_name"))),
                };
                index.insert(id.to_string(), calendars.len());
                calendars.push(c);
                names.push(t.value(r, "clndr_name").to_string());
            }
        }
        let old = &self.model.calendars;
        let remap = |i: usize| -> Result<usize, String> {
            // A project that opened without calendars points at none yet:
            // the first calendar becomes its calendar.
            if old.is_empty() && !calendars.is_empty() {
                return Ok(0);
            }
            let id = old.get(i).map_or("", |c| c.id.as_str());
            index
                .get(id)
                .copied()
                .ok_or_else(|| format!("Calendar {id} is in use and cannot be removed"))
        };
        let activity_cals = self
            .model
            .activities
            .iter()
            .map(|a| remap(a.cal))
            .collect::<Result<Vec<_>, _>>()?;
        let project_calendar = remap(self.model.project_calendar)?;
        for demands in self.resources.demands.values_mut() {
            for d in demands {
                d.calendar = d.calendar.and_then(|c| remap(c).ok());
            }
        }
        for (a, cal) in self.model.activities.iter_mut().zip(activity_cals) {
            a.cal = cal;
        }
        // The first calendar of a project that had none is its calendar in
        // the file too, as when an activity is added first.
        let first_calendar = old.is_empty().then(|| calendars.first()).flatten();
        if let Some(cal) = first_calendar {
            let pid = self.model.project.proj_id;
            let project = self.document.table_mut("PROJECT");
            if let Some(row) = project
                .rows
                .iter()
                .position(|r| crate::edit::records::num(project.value(r, "proj_id")) == pid)
            {
                project.set(row, "clndr_id", cal.id.clone());
            }
        }
        self.model.project_calendar = project_calendar;
        self.model.calendars = calendars;
        self.calendar_names = names;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{starter, Document};

    const CANONICAL: &str = "(0||CalendarData()(\u{7f}\u{7f}  (0||DaysOfWeek()(\u{7f}\u{7f}    (0||1()())\u{7f}\u{7f}    (0||2()(\u{7f}\u{7f}      (0||0(s|07:00|f|17:00)())))\u{7f}\u{7f}    (0||3()(\u{7f}\u{7f}      (0||0(s|07:00|f|17:00)())))\u{7f}\u{7f}    (0||4()(\u{7f}\u{7f}      (0||0(s|07:00|f|17:00)())))\u{7f}\u{7f}    (0||5()(\u{7f}\u{7f}      (0||0(s|07:00|f|17:00)())))\u{7f}\u{7f}    (0||6()(\u{7f}\u{7f}      (0||0(s|07:00|f|17:00)())))\u{7f}\u{7f}    (0||7()())))\u{7f}\u{7f}  (0||VIEW(ShowTotal|Y)())\u{7f}\u{7f}  (0||Exceptions()(\u{7f}\u{7f}    (0||0(d|37865)())\u{7f}\u{7f}    (0||1(d|40468)(\u{7f}\u{7f}      (0||0(s|08:00|f|16:00)())))))))";

    #[test]
    fn canonical_text_round_trips_byte_for_byte() {
        let data = CalendarData::parse(CANONICAL).unwrap();
        assert_eq!(data.week[1], vec![(420, 1020)]);
        assert!(data.week[0].is_empty());
        assert_eq!(data.show_total, Some(true));
        assert_eq!(data.exceptions[&37865], vec![]);
        assert_eq!(data.exceptions[&40468], vec![(480, 960)]);
        assert_eq!(data.serialize(), CANONICAL);
    }
    #[test]
    fn compact_and_reordered_attributes_parse() {
        let text = "(0||()((0||DaysOfWeek()((0||2()((0||0(f|00:00|s|12:30)())))))(0||Exceptions()((0||0(d|46298)())))))";
        let data = CalendarData::parse(text).unwrap();
        assert_eq!(data.root, "");
        assert_eq!(data.week[1], vec![(750, 1440)]);
        assert_eq!(data.show_total, None);
        let again = CalendarData::parse(&data.serialize()).unwrap();
        assert_eq!(again, data);
        assert!(data.serialize().contains("(s|12:30|f|00:00)"));
    }
    #[test]
    fn day_edits_create_and_clear_exceptions() {
        let mut data = CalendarData::parse(CANONICAL).unwrap();
        // 46328 is Monday 2026-11-02.
        assert_eq!(weekday(46328), 1);
        data.set_day(46328, vec![]);
        assert_eq!(data.shifts_on(46328), &[] as &[Shift]);
        data.set_day(46328, vec![(420, 1020)]);
        assert!(!data.exceptions.contains_key(&46328));
        data.set_day(46329, vec![(420, 720)]);
        assert_eq!(total(data.shifts_on(46329)), 300);
        data.set_standard(46329);
        assert_eq!(total(data.shifts_on(46329)), 600);
        assert_eq!(data.week_minutes(), 3000);
    }
    #[test]
    fn base_exception_changes_reach_inheriting_dates_only() {
        let old = CalendarData::parse(CANONICAL).unwrap();
        let mut new = old.clone();
        new.exceptions.insert(46328, vec![]);
        new.exceptions.remove(&37865);
        new.exceptions.insert(40468, vec![(480, 600)]);
        let mut derived = old.clone();
        derived.exceptions.insert(40468, vec![(0, 1440)]);
        assert!(propagate_exceptions(&old, &new, &mut derived));
        assert_eq!(derived.exceptions.get(&46328), Some(&vec![]));
        assert!(!derived.exceptions.contains_key(&37865));
        assert_eq!(derived.exceptions[&40468], vec![(0, 1440)]);
        assert!(!propagate_exceptions(&new, &new, &mut derived));
    }
    /// Every corpus calendar: parse, write, parse again gives the same data and the
    /// same scheduling calendar; text already in the indented form is reproduced
    /// byte for byte, and untouched calendars export unchanged.
    #[test]
    fn corpus_calendars_round_trip() {
        // The corpus is client data outside the repository (OPENXER_CORPUS).
        let Some(corpus) = std::env::var_os("OPENXER_CORPUS") else {
            return;
        };
        let Ok(entries) = std::fs::read_dir(&corpus) else {
            return;
        };
        let (mut total, mut exact) = (0, 0);
        for entry in entries {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|x| x != "xer") {
                continue;
            }
            let doc = Document::read(&path).unwrap();
            let reread = Document::parse(&doc.text()).unwrap();
            let t = doc.table("CALENDAR").unwrap();
            assert_eq!(
                reread.table("CALENDAR").unwrap().rows,
                t.rows,
                "{}",
                path.display()
            );
            for r in &t.rows {
                let raw = t.value(r, "clndr_data");
                let id = t.value(r, "clndr_id");
                let Ok(engine) = calendar(id, raw, 46000) else {
                    continue; // The reference parser rejects this source calendar too.
                };
                total += 1;
                let data = CalendarData::parse(raw)
                    .unwrap_or_else(|e| panic!("{} {id}: {e}", path.display()));
                let text = data.serialize();
                assert_eq!(CalendarData::parse(&text).unwrap(), data);
                assert_eq!(
                    calendar(id, &text, 46000).unwrap(),
                    engine,
                    "{} calendar {id}",
                    path.display()
                );
                if text == raw {
                    exact += 1;
                } else if raw.starts_with("(0||CalendarData()(\u{7f}\u{7f}  (0||DaysOfWeek")
                    && !raw.ends_with(LINE)
                    && !raw.contains(")\u{7f}\u{7f}))")
                    && !raw.contains("(f|")
                {
                    // Indented text must match exactly unless the exceptions are unsorted.
                    let dates: Vec<i64> = raw
                        .split("(d|")
                        .skip(1)
                        .filter_map(|s| s.split(')').next()?.parse().ok())
                        .collect();
                    assert!(
                        dates.windows(2).any(|w| w[0] >= w[1]),
                        "{} calendar {id} is not reproduced",
                        path.display()
                    );
                }
            }
        }
        assert!(total > 50, "{total} corpus calendars");
        assert!(exact * 2 > total, "{exact} of {total} reproduced exactly");
        println!("{exact} of {total} corpus calendars reproduced byte for byte");
    }
    #[test]
    fn rebuild_keeps_calendar_references_by_id() {
        let mut p = starter();
        let base = p.document.table("CALENDAR").unwrap().rows[0].clone();
        let cals = p.document.table_mut("CALENDAR");
        let mut extra = base.clone();
        extra[cals.field("clndr_id").unwrap()] = "7".into();
        cals.rows.insert(0, extra);
        let name = cals.field("clndr_name").unwrap();
        cals.rows[0][name] = "Seven".into();
        p.rebuild_calendars().unwrap();
        assert_eq!(p.calendar_names, ["Seven", "Standard 5-day / 8-hour"]);
        assert_eq!(p.model.project_calendar, 1);
        assert!(p.model.activities.iter().all(|a| a.cal == 1));
        // Removing a calendar that is still referenced is refused without changes.
        let before = p.model.clone();
        p.document.table_mut("CALENDAR").rows.remove(1);
        assert!(p.rebuild_calendars().is_err());
        assert_eq!(p.model, before);
        let doc = Document::parse(&p.updated_document().text()).unwrap();
        assert_eq!(doc.table("CALENDAR").unwrap().rows.len(), 1);
    }
}
