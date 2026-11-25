//! The calendar editor: a month view with work, nonwork and exception days, the
//! selected days' hours (total or detailed half hours), the Workweek dialog and
//! the Time Periods dialog. Nothing is written until OK.
use super::*;
use crate::dialog_kit::{self as kit, Entry};
use crate::schedule_dialogs::Canvas;
use chrono::{Datelike, Months, NaiveDate};
use openxer_xer::calendar_data::{total, weekday, CalendarData, Shift};
use std::collections::BTreeSet;

/// Day cell colors of the month grid.
pub const WORK_FILL: Color32 = Color32::from_rgb(0xF0, 0xF0, 0xF0);
pub const NONWORK_FILL: Color32 = Color32::from_rgb(0xBF, 0xCD, 0xDB);
pub const EXCEPTION_EDGE: Color32 = crate::theme::color(crate::theme::NAVY);
/// Weekday captions of the month grid and the Workweek dialog.
const DAY_CAPTIONS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
/// Month grid cells on a 44 x 35 pitch, room for a day number and its hours.
const CELL: Vec2 = Vec2::new(44., 35.);
/// Default work periods for a day turned into work time: 08:00-12:00 and 13:00-17:00.
const STANDARD_DAY: [Shift; 2] = [(480, 720), (780, 1020)];

pub struct Editor {
    pub id: String,
    pub title: String,
    pub data: CalendarData,
    pub hours: [f64; 4],
    /// Base calendar this one inherits holidays and exceptions from.
    pub base: Option<String>,
    pub month: NaiveDate,
    pub selection: BTreeSet<i64>,
    pub anchor: i64,
    pub workweek: Option<Workweek>,
    pub periods: Option<[f64; 4]>,
    /// Text of the Work hours/day edit and the day it was filled from.
    pub total_text: String,
    pub total_for: Option<i64>,
    /// The hours text was typed in and is not applied yet (it applies on
    /// Enter or when the edit loses focus).
    pub total_edited: bool,
    /// The hours edit had the keyboard last frame (Escape cancels it).
    pub total_focus: bool,
    /// The calendar as opened, to tell whether Cancel throws changes away.
    pub original: (CalendarData, [f64; 4]),
    /// The "discard the changes?" question is showing.
    pub discard: bool,
}
pub struct Workweek {
    pub week: [Vec<Shift>; 7],
    pub days: BTreeSet<usize>,
    /// Text of the seven day edits (total hours mode).
    pub texts: [String; 7],
    /// The first day's edit still has to take the keyboard, text selected.
    pub focus_first: bool,
    /// The day edits typed in and not applied yet, and the one that had the
    /// keyboard last frame (Escape cancels it).
    pub edited: [bool; 7],
    pub focused: Option<usize>,
}

fn epoch() -> NaiveDate {
    NaiveDate::from_ymd_opt(1899, 12, 30).unwrap()
}
pub fn serial(date: NaiveDate) -> i64 {
    (date - epoch()).num_days()
}
pub fn date(serial: i64) -> NaiveDate {
    epoch() + chrono::Duration::days(serial)
}
/// Half-hour slots of a day that hold work. Hours of a period that runs
/// past midnight show at the top of the grid; a slot only partly worked
/// (a quarter-hour start) shows as work.
pub fn slots(shifts: &[Shift]) -> [bool; 48] {
    std::array::from_fn(|k| {
        let (a, b) = (k as i64 * 30, k as i64 * 30 + 30);
        shifts
            .iter()
            .any(|&(s, e)| (s < b && e > a) || (s < b + 1440 && e > a + 1440))
    })
}
/// Half hour `k` of the day turned to work (`work`) or not, every other
/// time kept as stored: only that half hour is added or cut, so periods
/// that do not start on the half hour or run past midnight survive.
pub fn toggle(shifts: &[Shift], k: usize, work: bool) -> Vec<Shift> {
    let (a, b) = (k as i64 * 30, k as i64 * 30 + 30);
    let mut out: Vec<Shift> = vec![];
    if work {
        let mut add = (a, b);
        for &(s, e) in shifts {
            if s <= add.1 && e >= add.0 {
                add = (add.0.min(s), add.1.max(e));
            } else {
                out.push((s, e));
            }
        }
        out.push(add);
    } else {
        for &(s, e) in shifts {
            let mut pieces = vec![(s, e)];
            for (x, y) in [(a, b), (a + 1440, b + 1440)] {
                pieces = pieces
                    .into_iter()
                    .flat_map(|(s, e)| {
                        if y <= s || x >= e {
                            vec![(s, e)]
                        } else {
                            [(s, x), (y, e)]
                                .into_iter()
                                .filter(|(s, e)| e > s)
                                .collect()
                        }
                    })
                    .collect();
            }
            out.extend(pieces);
        }
    }
    out.sort_unstable();
    out
}
#[cfg(test)]
pub fn from_slots(slots: &[bool; 48]) -> Vec<Shift> {
    let mut out: Vec<Shift> = vec![];
    for (k, &work) in slots.iter().enumerate() {
        let (a, b) = (k as i64 * 30, k as i64 * 30 + 30);
        if !work {
            continue;
        }
        match out.last_mut() {
            Some(last) if last.1 == a => last.1 = b,
            _ => out.push((a, b)),
        }
    }
    out
}
/// One period of `hours` that keeps the day's current start (08:00 when idle).
#[cfg(test)]
pub fn total_day(current: &[Shift], hours: f64) -> Vec<Shift> {
    let length = ((hours * 2.).round() as i64 * 30).clamp(0, 1440);
    if length == 0 {
        return vec![];
    }
    let start = current.first().map_or(480, |s| s.0).min(1440 - length);
    vec![(start, start + length)]
}
/// The day's periods with a new total: the last period ends later or
/// earlier, so breaks between periods are kept. Past the end of the day the
/// first period starts earlier, then the breaks fill from the last one. A
/// day with no work gets one period from 08:00. Minutes are kept as typed.
pub fn adjust_total(current: &[Shift], hours: f64) -> Vec<Shift> {
    let want = (hours.clamp(0., 24.) * 60.).round() as i64;
    let mut out: Vec<Shift> = current.to_vec();
    out.sort_unstable();
    if out.is_empty() {
        let start = 480.min(1440 - want);
        return if want > 0 {
            vec![(start, start + want)]
        } else {
            vec![]
        };
    }
    let mut have = total(&out);
    // Less: the last periods give up their end.
    while have > want {
        let Some(last) = out.last_mut() else { break };
        let cut = (have - want).min(last.1 - last.0);
        last.1 -= cut;
        have -= cut;
        if last.1 <= last.0 {
            out.pop();
        }
    }
    if have >= want {
        return out;
    }
    // More: after the last period, then before the first, then the breaks.
    let night = out.last().is_some_and(|l| l.1 > 1440);
    let first = out[0].0;
    let end_cap = if night { first + 1440 } else { 1440 };
    let start_floor = if night {
        (out.last().unwrap().1 - 1440).max(0)
    } else {
        0
    };
    let last = out.last_mut().unwrap();
    let grow = (want - have).min((end_cap - last.1).max(0));
    last.1 += grow;
    have += grow;
    let grow = (want - have).min((out[0].0 - start_floor).max(0));
    out[0].0 -= grow;
    have += grow;
    while have < want && out.len() > 1 {
        let n = out.len();
        let gap = out[n - 1].0 - out[n - 2].1;
        let grow = (want - have).min(gap);
        out[n - 2].1 += grow;
        have += grow;
        if out[n - 2].1 >= out[n - 1].0 {
            out[n - 2].1 = out[n - 1].1;
            out.pop();
        } else {
            break;
        }
    }
    out
}
/// Apply the hours typed for workweek day `d`, if any.
fn commit_day(ww: &mut Workweek, d: usize) {
    if !std::mem::take(&mut ww.edited[d]) {
        return;
    }
    if let Ok(hours) = ww.texts[d].trim().parse::<f64>() {
        if hours.is_finite() {
            ww.week[d] = adjust_total(&ww.week[d], hours.clamp(0., 24.));
        }
    }
    ww.texts[d] = hours_label(total(&ww.week[d]));
}
pub fn hours_label(minutes: i64) -> String {
    crate::records::format_number(minutes as f64 / 60.)
}

impl Editor {
    pub fn open(p: &ProjectData, id: &str) -> Result<Self, String> {
        let doc = &p.document;
        let data = ops::data(doc, id).map_err(|e| format!("This calendar cannot be read: {e}"))?;
        let base_id = ops::value(doc, id, "base_clndr_id");
        let base = (!base_id.is_empty())
            .then(|| ops::name(doc, &base_id))
            .map(|n| if n.is_empty() { base_id.clone() } else { n });
        let today = p
            .model
            .project
            .data_date
            .map(|d| p.model.origin_serial + d.div_euclid(DAY))
            .unwrap_or_else(|| serial(chrono::Local::now().date_naive()));
        let day = date(today);
        let hours = ops::hours(doc, id);
        Ok(Self {
            id: id.into(),
            title: ops::name(doc, id),
            original: (data.clone(), hours),
            data,
            hours,
            base,
            month: day.with_day(1).unwrap(),
            selection: BTreeSet::from([today]),
            anchor: today,
            workweek: None,
            periods: None,
            total_text: String::new(),
            total_for: None,
            total_edited: false,
            total_focus: false,
            discard: false,
        })
    }
    /// The draft differs from the calendar as opened.
    pub fn changed(&self) -> bool {
        (&self.data, self.hours) != (&self.original.0, self.original.1)
    }
    /// The hours box shows the anchor day's hours again on the next frame.
    fn refresh_total(&mut self) {
        self.total_for = None;
        self.total_edited = false;
    }
    pub fn show_total(&self) -> bool {
        self.data.show_total.unwrap_or(true)
    }
    /// The first day shown in the month grid (the Sunday on or before the 1st).
    pub fn grid_start(&self) -> i64 {
        let first = serial(self.month);
        first - weekday(first) as i64
    }
    pub fn click_day(&mut self, day: i64, modifiers: egui::Modifiers) {
        if modifiers.shift {
            let (a, b) = (self.anchor.min(day), self.anchor.max(day));
            if !modifiers.command {
                self.selection.clear();
            }
            self.selection.extend(a..=b);
        } else if modifiers.command {
            if !self.selection.remove(&day) {
                self.selection.insert(day);
            }
            self.anchor = day;
        } else {
            self.selection = BTreeSet::from([day]);
            self.anchor = day;
        }
    }
    pub fn make_standard(&mut self) {
        for &d in &self.selection {
            self.data.set_standard(d);
        }
        self.refresh_total();
    }
    pub fn make_nonwork(&mut self) {
        for &d in &self.selection {
            self.data.set_day(d, vec![]);
        }
        self.refresh_total();
    }
    /// Nonwork days become work days with their workweek hours, or 8 hours.
    pub fn make_work(&mut self) {
        for &d in &self.selection {
            if self.data.shifts_on(d).is_empty() {
                let week = &self.data.week[weekday(d)];
                let hours = if week.is_empty() {
                    STANDARD_DAY.to_vec()
                } else {
                    week.clone()
                };
                self.data.set_day(d, hours);
            }
        }
        self.refresh_total();
    }
    /// Every selected day gets `hours` of work: its last period ends later
    /// or earlier, so a lunch break is kept.
    pub fn set_total(&mut self, hours: f64) {
        for &d in &self.selection {
            let shifts = adjust_total(self.data.shifts_on(d), hours);
            self.data.set_day(d, shifts);
        }
        self.refresh_total();
    }
    /// Apply the typed hours, if they were typed and read as a number.
    pub fn commit_total(&mut self) {
        if !std::mem::take(&mut self.total_edited) {
            return;
        }
        match self.total_text.trim().parse::<f64>() {
            Ok(hours) if hours.is_finite() => self.set_total(hours.clamp(0., 24.)),
            _ => self.refresh_total(),
        }
    }
    /// Toggle one half hour on every selected day, following the anchor day's
    /// state. Other times are kept as stored.
    pub fn toggle_slot(&mut self, k: usize) {
        let work = !slots(self.data.shifts_on(self.anchor))[k];
        for &d in &self.selection {
            let shifts = toggle(self.data.shifts_on(d), k, work);
            self.data.set_day(d, shifts);
        }
        self.refresh_total();
    }
}

impl App {
    pub(super) fn calendar_editor(
        &mut self,
        ctx: &egui::Context,
        st: &mut CalendarsState,
        cmds: &mut Vec<Cmd>,
        escape: &mut bool,
    ) {
        let Some(mut ed) = st.editor.take() else {
            return;
        };
        let mut close = false;
        // Cancel, Escape or the title bar's close: a changed draft asks first.
        let mut cancel = false;
        if ed.workweek.is_some() {
            self.workweek_dialog(ctx, &mut ed, escape);
        }
        if ed.periods.is_some() {
            self.time_periods_dialog(ctx, &mut ed, escape);
        }
        if ed.discard && std::mem::take(escape) {
            ed.discard = false;
        }
        // Escape that only put the hours box back (the window reads the same
        // key as its close).
        let mut field_escape = false;
        if std::mem::take(escape) {
            if ed.total_focus {
                // Escape in the hours box puts the box back, nothing more.
                ed.refresh_total();
                field_escape = true;
            } else {
                cancel = true;
            }
        }
        let busy = ed.workweek.is_some() || ed.periods.is_some() || ed.discard;
        let kind = ops::kind(&self.project.document, &ed.id);
        let title = format!("{} Calendar: {}", kind.caption(), ed.title);
        let mut help = false;
        let ((), closed, actions) =
            kit::window(ctx, "calendar_editor", &title, Vec2::new(581., 461.), |c| {
                kit::strip(c, 581., 461.);
                c.fill(6., 6., 457., 31., kit::FACE);
                c.lowered(6., 6., 457., 31.);
                let mut detailed = !ed.show_total();
                if kit::radio(
                    c,
                    "calendar_total_mode",
                    20.,
                    13.,
                    200.,
                    17.,
                    "Total work hours/day",
                    !detailed,
                    !busy,
                ) {
                    detailed = false;
                }
                if kit::radio(
                    c,
                    "calendar_detailed_mode",
                    240.,
                    13.,
                    200.,
                    17.,
                    "Detailed work hours/day",
                    detailed,
                    !busy,
                ) {
                    detailed = true;
                }
                if detailed == ed.show_total() {
                    ed.data.show_total = Some(!detailed);
                }
                c.fill(6., 49., 455., 406., kit::STRIP);
                c.lowered(6., 49., 455., 406.);
                self.month_view(c, &mut ed, busy);
                self.day_hours(c, &mut ed, busy);
                // Legend.
                for (label, x, swatch, fill) in [
                    ("Standard", 21., 83., Color32::WHITE),
                    ("Nonwork", 118., 183., NONWORK_FILL),
                    ("Exception", 215., 286., Color32::WHITE),
                ] {
                    kit::text(c, x, 346., label, true);
                    let y = if label == "Exception" { 344. } else { 345. };
                    c.sunken(c.r(swatch, y, 15., 15.), fill);
                    if label == "Exception" {
                        c.ui.painter().rect_stroke(
                            c.r(swatch + 2., y + 2., 11., 11.),
                            0.,
                            Stroke::new(1_f32, EXCEPTION_EDGE),
                            egui::StrokeKind::Inside,
                        );
                    }
                }
                c.fill(18., 373., 309., 71., kit::FACE);
                c.raised(18., 373., 309., 71.);
                kit::text(
                    c,
                    26.,
                    385.,
                    "Inherit holidays and exceptions from Global Calendar:",
                    true,
                );
                c.etched(19., 401., 307.);
                let base = ed.base.clone().unwrap_or_else(|| "<None>".into());
                let mut zero = 0;
                c.combo(
                    "calendar_base",
                    27.,
                    413.,
                    197.,
                    22.,
                    &[(base.as_str(), true)],
                    &mut zero,
                    false,
                );
                kit::tooltip(
                    c,
                    "calendar_base_tip",
                    18.,
                    373.,
                    309.,
                    71.,
                    "Changes to the global calendar's exceptions are copied to this calendar.",
                );
                let any = !ed.selection.is_empty() && !busy;
                // The order of the reference's own calendar editor: OK,
                // Cancel, Help, then the day commands and the two editors.
                let out = kit::observed_button_column(
                    c,
                    581.,
                    &[
                        Entry::Button("calendar_ok", "OK", !busy, false),
                        Entry::Button("calendar_cancel", "Cancel", !busy, false),
                        Entry::Button("calendar_help", "Help", !busy, false),
                        Entry::Button("calendar_work", "&Work", any, true),
                        Entry::Button("calendar_nonwork", "&Nonwork", any, false),
                        Entry::Button("calendar_standard", "&Standard", any, false),
                        Entry::Button("calendar_workweek", "Wo&rkweek...", !busy, true),
                        Entry::Button("calendar_time_periods", "Time Periods...", !busy, false),
                    ],
                );
                match out.clicked.as_deref() {
                    Some("calendar_ok") => {
                        ed.commit_total();
                        close = true;
                        cmds.push(Cmd::Commit(
                            ed.id.clone(),
                            Box::new(ed.data.clone()),
                            ed.hours,
                        ));
                    }
                    Some("calendar_cancel") => cancel = true,
                    Some("calendar_work") => ed.make_work(),
                    Some("calendar_nonwork") => ed.make_nonwork(),
                    Some("calendar_standard") => ed.make_standard(),
                    Some("calendar_workweek") => {
                        let week = ed.data.week.clone();
                        ed.workweek = Some(Workweek {
                            texts: std::array::from_fn(|d| hours_label(total(&week[d]))),
                            week,
                            days: BTreeSet::from([1]),
                            focus_first: true,
                            edited: [false; 7],
                            focused: None,
                        });
                    }
                    Some("calendar_time_periods") => ed.periods = Some(ed.hours),
                    Some(_) => help = true,
                    None => {}
                }
            });
        self.record_dialog_actions(actions);
        if help {
            self.open_help_for("calendar_editor");
        }
        if (cancel || (closed && !field_escape)) && !close {
            if ed.changed() {
                ed.discard = true;
            } else {
                close = true;
            }
        }
        if ed.discard && !close {
            let mut rects = vec![];
            let answer = crate::kit_window::message_box(
                ctx,
                "calendar_discard",
                "Calendar",
                crate::kit_window::MessageIcon::Question,
                &format!("Discard the changes to the calendar \"{}\"?", ed.title),
                &["Yes", "No"],
                crate::kit_window::Question::DiscardCalendarChanges.default_in(&["Yes", "No"]),
                &mut rects,
            );
            self.record_dialog_actions(rects);
            match answer {
                Some(0) => close = true,
                Some(_) => ed.discard = false,
                None => {}
            }
        }
        if !close {
            st.editor = Some(ed);
        }
    }

    /// The month control at 19,57: a 30 px header (`<`, caption, `>`), the
    /// weekday row and six rows of 44 x 35 day cells.
    fn month_view(&mut self, c: &mut Canvas, ed: &mut Editor, busy: bool) {
        let (x0, y0) = (19., 57.);
        if kit::button(c, "calendar_prev_month", x0, y0, 51., 30., "<", !busy) {
            ed.month = ed.month - Months::new(1);
        }
        let caption = ed.month.format("%B %Y").to_string();
        c.button(
            "calendar_month_caption",
            x0 + 51.,
            y0,
            206.,
            30.,
            &caption,
            false,
            true,
        );
        if kit::button(
            c,
            "calendar_next_month",
            x0 + 257.,
            y0,
            51.,
            30.,
            ">",
            !busy,
        ) {
            ed.month = ed.month + Months::new(1);
        }
        let top = y0 + 30.;
        for (d, name) in DAY_CAPTIONS.iter().enumerate() {
            let x = x0 + d as f32 * CELL.x;
            let fill = if d == 0 || d == 6 {
                NONWORK_FILL
            } else {
                WORK_FILL
            };
            c.fill(x, top, CELL.x, CELL.y, fill);
            // Two-pixel raised bevel.
            c.hline(x, top, CELL.x, Color32::WHITE);
            c.hline(x, top + 1., CELL.x - 1., Color32::WHITE);
            c.vline(x, top, CELL.y, Color32::WHITE);
            c.vline(x + 1., top, CELL.y - 1., Color32::WHITE);
            c.hline(x + 1., top + CELL.y - 2., CELL.x - 1., kit::SHADOW);
            c.hline(x, top + CELL.y - 1., CELL.x, kit::SHADOW);
            c.vline(x + CELL.x - 2., top + 1., CELL.y - 1., kit::SHADOW);
            c.vline(x + CELL.x - 1., top, CELL.y, kit::SHADOW);
            c.fill(x + CELL.x - 3., top + 2., 1., CELL.y - 4., kit::FACE);
            c.fill(x + 2., top + CELL.y - 3., CELL.x - 4., 1., kit::FACE);
            let center = c.origin + Vec2::new(x + CELL.x / 2. - 1., top + CELL.y / 2. - 1.);
            c.ui.painter().text(
                center,
                Align2::CENTER_CENTER,
                *name,
                crate::schedule_dialogs::dialog_font(),
                INK,
            );
        }
        let start = ed.grid_start();
        let month = ed.month.month();
        let modifiers = c.ui.input(|i| i.modifiers);
        let today = serial(chrono::Local::now().date_naive());
        for k in 0..42 {
            let day = start + k;
            let x = x0 + (k % 7) as f32 * CELL.x;
            let y = top + CELL.y + (k / 7) as f32 * CELL.y;
            let dt = date(day);
            let inside = dt.month() == month;
            let shifts = ed.data.shifts_on(day);
            let selected = inside && ed.selection.contains(&day);
            let fill = if selected {
                kit::SELECTED
            } else if inside && shifts.is_empty() {
                NONWORK_FILL
            } else {
                WORK_FILL
            };
            c.fill(x, y, CELL.x, CELL.y, fill);
            c.hline(x, y, CELL.x - 1., Color32::WHITE);
            c.vline(x, y, CELL.y - 1., Color32::WHITE);
            c.hline(x + 1., y + CELL.y - 2., CELL.x - 2., kit::FACE);
            c.vline(x + CELL.x - 2., y + 1., CELL.y - 2., kit::FACE);
            c.hline(x, y + CELL.y - 1., CELL.x, kit::SHADOW);
            c.vline(x + CELL.x - 1., y, CELL.y, kit::SHADOW);
            if !inside {
                continue;
            }
            if ed.data.exceptions.contains_key(&day) {
                c.ui.painter().rect_stroke(
                    c.r(x + 3., y + 3., CELL.x - 6., CELL.y - 6.),
                    0.,
                    Stroke::new(1_f32, EXCEPTION_EDGE),
                    egui::StrokeKind::Inside,
                );
            }
            if day == today {
                c.ui.painter().rect_stroke(
                    c.r(x, y, CELL.x - 1., CELL.y - 1.),
                    0.,
                    Stroke::new(2_f32, INK),
                    egui::StrokeKind::Inside,
                );
            }
            let color = if selected { Color32::WHITE } else { INK };
            c.ui.painter().text(
                c.origin + Vec2::new(x + 1., y + 2.),
                Align2::LEFT_TOP,
                dt.day().to_string(),
                crate::schedule_dialogs::dialog_font(),
                color,
            );
            let rect = c.r(x, y, CELL.x, CELL.y);
            let response = c.hit(&format!("calendar_day_{dt}"), rect, !busy);
            if response.clicked() {
                ed.click_day(day, modifiers);
            }
            crate::context_menu::ContextMenu::new("calendar_day_menu").show(
                &response,
                ed,
                |ed, menu| {
                    if !ed.selection.contains(&day) {
                        ed.click_day(day, egui::Modifiers::NONE);
                    }
                    if menu.item("nonwork", crate::chrome::Item::new("Nonwork")) {
                        ed.make_nonwork();
                    }
                    if menu.item("normal", crate::chrome::Item::new("Normal")) {
                        ed.make_standard();
                    }
                },
            );
        }
    }

    /// The Work hours/day panel at 338,56: a spin edit in total mode, the
    /// half-hour grid of the anchor day in detailed mode.
    fn day_hours(&mut self, c: &mut Canvas, ed: &mut Editor, busy: bool) {
        c.fill(338., 56., 103., 275., kit::FACE);
        c.lowered(338., 56., 103., 275.);
        c.fill(339., 57., 101., 21., kit::FACE);
        c.raised(339., 57., 101., 21.);
        kit::text(c, 347., 61., "Work hours/day", true);
        let shifts = ed.data.shifts_on(ed.anchor).to_vec();
        if ed.show_total() {
            if ed.total_for != Some(ed.anchor) {
                ed.total_for = Some(ed.anchor);
                ed.total_edited = false;
                ed.total_text = format!("{:.1}", total(&shifts) as f64 / 60.);
            }
            let enabled = !busy && !ed.selection.is_empty();
            let mut text = ed.total_text.clone();
            // Typing changes only the text; Enter, leaving the box or an
            // arrow applies it.
            let committed = c.spin(
                "calendar_total_hours",
                345.,
                88.,
                89.,
                21.,
                &mut text,
                0.5,
                0.0..=24.0,
                enabled,
            );
            if text != ed.total_text {
                ed.total_text = text;
                ed.total_edited = true;
            }
            if committed {
                ed.commit_total();
            }
            let id = c.id("calendar_total_hours");
            ed.total_focus = c.ui.memory(|m| m.has_focus(id));
            // Enter applies the hours and keeps the editor open (it does not
            // press OK).
            crate::dialog_keys::note_keeps_enter_id(c.ui.ctx(), id);
        } else {
            ed.refresh_total();
            ed.total_focus = false;
            kit::text(c, 343., 82., &crate::format::day(date(ed.anchor)), false);
            let work = slots(&shifts);
            let area = c.r(340., 98., 99., 231.);
            let clicked =
                c.ui.scope_builder(egui::UiBuilder::new().max_rect(area), |ui| {
                    self.slot_grid(ui, "calendar_slots", &work, !busy, 30.)
                })
                .inner;
            if let Some(k) = clicked {
                ed.toggle_slot(k);
            }
        }
    }

    /// 24 hour rows with two half-hour cells `slot` wide. Returns the clicked
    /// half hour.
    fn slot_grid(
        &mut self,
        ui: &mut egui::Ui,
        id: &str,
        work: &[bool; 48],
        enabled: bool,
        slot: f32,
    ) -> Option<usize> {
        let mut clicked = None;
        let label = 34.;
        egui::Frame::NONE
            .fill(Color32::WHITE)
            .stroke(Stroke::new(1_f32, kit::SHADOW))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt(id)
                    .max_height(ui.available_height() - 2.)
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing = Vec2::ZERO;
                        for hour in 0..24 {
                            let (row, _) = ui.allocate_exact_size(
                                Vec2::new(label + slot * 2., 17.),
                                Sense::hover(),
                            );
                            let p = ui.painter();
                            p.text(
                                row.left_center() + Vec2::new(3., 0.),
                                Align2::LEFT_CENTER,
                                format!("{hour:02}:00"),
                                crate::schedule_dialogs::dialog_font(),
                                INK,
                            );
                            for half in 0..2 {
                                let k = hour * 2 + half;
                                let r = Rect::from_min_size(
                                    row.min + Vec2::new(label + half as f32 * slot, 0.),
                                    Vec2::new(slot, 17.),
                                );
                                let fill = if work[k] {
                                    Color32::WHITE
                                } else {
                                    NONWORK_FILL
                                };
                                ui.painter().rect_filled(r, 0., fill);
                                ui.painter().rect_stroke(
                                    r,
                                    0.,
                                    Stroke::new(1_f32, kit::SHADOW),
                                    egui::StrokeKind::Inside,
                                );
                                if work[k] && slot >= 30. {
                                    ui.painter().text(
                                        r.center(),
                                        Align2::CENTER_CENTER,
                                        "Work",
                                        crate::schedule_dialogs::dialog_font(),
                                        INK,
                                    );
                                }
                                if enabled {
                                    let response =
                                        ui.interact(r, ui.id().with((id, k)), Sense::click());
                                    self.action(&format!("{id}_{k}"), &response);
                                    if response.clicked() {
                                        clicked = Some(k);
                                    }
                                }
                            }
                        }
                    });
            });
        clicked
    }

    /// Calendar Weekly Hours (539 x 131 px inside the frame): a
    /// `Standard work hours` bar over seven day edits, OK / Cancel / Help
    /// in the button column. In detailed mode the day labels
    /// pick days and a half-hour grid below edits them.
    fn workweek_dialog(&mut self, ctx: &egui::Context, ed: &mut Editor, escape: &mut bool) {
        let Some(mut ww) = ed.workweek.take() else {
            return;
        };
        let mut close = std::mem::take(escape);
        if close {
            if let Some(d) = ww.focused.take() {
                // Escape in a day's hours puts that box back, nothing more.
                ww.texts[d] = hours_label(total(&ww.week[d]));
                ww.edited[d] = false;
                close = false;
            }
        }
        let mut apply = false;
        let mut help = false;
        let detailed = !ed.show_total();
        let height = if detailed { 401. } else { 131. };
        let ((), closed, actions) = kit::window(
            ctx,
            "calendar_workweek",
            "Calendar Weekly Hours",
            Vec2::new(539., height),
            |c| {
                c.fill(0., 0., 402., height, kit::FACE);
                c.lowered(5., 5., 392., height - 10.);
                c.fill(6., 6., 390., 21., kit::FACE);
                c.raised(6., 6., 390., 21.);
                kit::text(c, 13., 10., "Standard work hours", true);
                let xs = [28., 79., 128., 179., 229., 278., 329.];
                let modifiers = c.ui.input(|i| i.modifiers);
                for (d, x) in xs.iter().enumerate() {
                    let picked = ww.days.contains(&d);
                    if detailed {
                        let rect = c.r(*x - 2., 46., 41., 16.);
                        if picked {
                            c.ui.painter().rect_filled(rect, 0., kit::SELECTED);
                        }
                        let color = if picked { Color32::WHITE } else { INK };
                        c.ui.painter().text(
                            rect.min + Vec2::new(2., 1.),
                            Align2::LEFT_TOP,
                            DAY_CAPTIONS[d],
                            crate::schedule_dialogs::dialog_font(),
                            color,
                        );
                        if c.hit(&format!("workweek_day_{d}"), rect, true).clicked() {
                            if modifiers.command {
                                if !ww.days.remove(&d) {
                                    ww.days.insert(d);
                                }
                            } else {
                                ww.days = BTreeSet::from([d]);
                            }
                        }
                        kit::field(
                            c,
                            *x,
                            64.,
                            37.,
                            26.,
                            &hours_label(total(&ww.week[d])),
                            false,
                        );
                    } else {
                        kit::text(c, *x, 49., DAY_CAPTIONS[d], true);
                        let key = format!("workweek_hours_{d}");
                        let mut text = ww.texts[d].clone();
                        let id = c.id(&key);
                        if d == 0 && ww.focus_first {
                            // The dialog opens with the first day selected.
                            let ctx = c.ui.ctx().clone();
                            let mut state =
                                egui::TextEdit::load_state(&ctx, id).unwrap_or_default();
                            state
                                .cursor
                                .set_char_range(Some(egui::text::CCursorRange::two(
                                    egui::text::CCursor::new(0),
                                    egui::text::CCursor::new(text.chars().count()),
                                )));
                            state.store(&ctx, id);
                            ctx.memory_mut(|m| m.request_focus(id));
                        }
                        let response = c.edit(&key, *x, 64., 37., 26., &mut text, true, Align::Min);
                        if d == 0 && response.has_focus() {
                            ww.focus_first = false;
                        }
                        if text != ww.texts[d] {
                            ww.texts[d] = text;
                            ww.edited[d] = true;
                        }
                        // The hours apply on Enter or when the box loses focus.
                        if response.lost_focus() {
                            commit_day(&mut ww, d);
                        }
                        if response.has_focus() {
                            ww.focused = Some(d);
                        } else if ww.focused == Some(d) {
                            ww.focused = None;
                        }
                    }
                }
                if detailed {
                    let any = !ww.days.is_empty();
                    if kit::button(c, "workweek_work", 28., 100., 80., 25., "&Work", any) {
                        for &d in &ww.days {
                            if ww.week[d].is_empty() {
                                ww.week[d] = STANDARD_DAY.to_vec();
                            }
                        }
                    }
                    if kit::button(c, "workweek_nonwork", 114., 100., 80., 25., "&Nonwork", any) {
                        for &d in &ww.days {
                            ww.week[d].clear();
                        }
                    }
                    let anchor = ww.days.iter().next().copied().unwrap_or(1);
                    let work = slots(&ww.week[anchor]);
                    let area = c.r(28., 132., 200., height - 145.);
                    let clicked =
                        c.ui.scope_builder(egui::UiBuilder::new().max_rect(area), |ui| {
                            self.slot_grid(ui, "workweek_slots", &work, any, 54.)
                        })
                        .inner;
                    if let Some(k) = clicked {
                        let on = !work[k];
                        for &d in &ww.days {
                            ww.week[d] = toggle(&ww.week[d], k, on);
                        }
                    }
                }
                kit::strip(c, 539., height);
                let out = kit::button_column(
                    c,
                    539.,
                    &[
                        Entry::ok("workweek_ok"),
                        Entry::cancel("workweek_cancel"),
                        Entry::help("workweek_help"),
                    ],
                );
                match out.clicked.as_deref() {
                    Some("workweek_ok") => {
                        for d in 0..7 {
                            commit_day(&mut ww, d);
                        }
                        apply = true;
                        close = true;
                    }
                    Some("workweek_cancel") => close = true,
                    Some(_) => help = true,
                    None => {}
                }
            },
        );
        self.record_dialog_actions(actions);
        close |= closed;
        if help {
            self.open_help_for("calendar_workweek");
        }
        if apply {
            ed.data.week = ww.week.clone();
            // Dates that now match their workweek day are no longer exceptions.
            let same: Vec<i64> = ed
                .data
                .exceptions
                .iter()
                .filter(|(d, s)| **s == ed.data.week[weekday(**d)])
                .map(|(d, _)| *d)
                .collect();
            for d in same {
                ed.data.exceptions.remove(&d);
            }
            ed.refresh_total();
        }
        if !close {
            ed.workweek = Some(ww);
        }
    }

    /// Time Periods (372 x 142 px inside the frame): the hours in each time
    /// period, OK and Cancel in the button column.
    fn time_periods_dialog(&mut self, ctx: &egui::Context, ed: &mut Editor, escape: &mut bool) {
        let Some(mut hours) = ed.periods else {
            return;
        };
        let mut close = std::mem::take(escape);
        // The four hours as typed, kept while the dialog is open.
        let texts_id = egui::Id::new("calendar_time_periods_texts");
        let mut texts: [String; 4] = ctx
            .data(|d| d.get_temp(texts_id))
            .unwrap_or_else(|| hours.map(kit::format_number));
        const W: f32 = 372.;
        const H: f32 = 142.;
        const PERIODS: [(&str, f64); 4] = [
            ("Hours/Day", 24.),
            ("Hours/Week", 168.),
            ("Hours/Month", 744.),
            ("Hours/Year", 8784.),
        ];
        let ((), closed, actions) = kit::window(
            ctx,
            "calendar_time_periods",
            "Time Periods",
            Vec2::new(W, H),
            |c| {
                kit::strip(c, W, H);
                kit::section(
                    c,
                    6.,
                    6.,
                    W - kit::STRIP_WIDTH - 12.,
                    H - 12.,
                    "Hours per Time Period",
                );
                for (k, (caption, max)) in PERIODS.into_iter().enumerate() {
                    let y = 36. + 24. * k as f32;
                    kit::text(c, 16., y + 4., caption, true);
                    let committed = c.spin(
                        &format!("calendar_period_{k}"),
                        110.,
                        y,
                        120.,
                        21.,
                        &mut texts[k],
                        1.,
                        0.0..=max,
                        true,
                    );
                    if let Ok(v) = texts[k].trim().parse::<f64>() {
                        hours[k] = v.clamp(0., max);
                    }
                    if committed {
                        texts[k] = kit::format_number(hours[k]);
                    }
                }
                let out = kit::button_column(
                    c,
                    W,
                    &[
                        Entry::Button("time_periods_ok", "OK", hours[0] > 0., false),
                        Entry::cancel("time_periods_cancel"),
                    ],
                );
                match out.clicked.as_deref() {
                    Some("time_periods_ok") => {
                        ed.hours = hours;
                        close = true;
                    }
                    Some(_) => close = true,
                    None => {}
                }
            },
        );
        self.record_dialog_actions(actions);
        close |= closed;
        if close {
            ctx.data_mut(|d| d.remove::<[String; 4]>(texts_id));
        } else {
            ctx.data_mut(|d| d.insert_temp(texts_id, texts));
        }
        ed.periods = (!close).then_some(hours);
    }
}
