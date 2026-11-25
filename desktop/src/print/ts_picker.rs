//! The picker behind the `...` buttons of the Page Setup timescale boxes: a
//! small dialog that drops down under the box and writes the expression
//! printing reads (`DD-1W`, `PF+1W`, `SW`, or a date in the user's format).
//! It chooses what the date counts from, an offset in days, weeks, months or
//! years, or a specific date on a month calendar, and shows the expression
//! and the day it comes to in this project, worked out by the same code the
//! printout uses ([`App::resolve_print_expr`]).
use super::dialog::SetupDialog;
use super::setup::{self, Anchor, DateExpr};
use crate::dialog_kit::{self as kit, Entry};
use crate::schedule_dialogs::{modal_at, Canvas};
use crate::*;
use chrono::{Datelike, Duration, NaiveDate};

/// Client area: the working panel and the button strip.
const CLIENT: Vec2 = Vec2::new(440., 322.);
/// The choice that stands for a date picked on the calendar.
const SPECIFIC: usize = 6;
/// What a date can count from, in the order of the drop-down.
const ANCHORS: [Anchor; 6] = [
    Anchor::CurrentDate,
    Anchor::DataDate,
    Anchor::ProjectStart,
    Anchor::ProjectFinish,
    Anchor::StartOfWeek,
    Anchor::StartOfMonth,
];
const UNITS: [(&str, char); 4] = [
    ("Days", 'D'),
    ("Weeks", 'W'),
    ("Months", 'M'),
    ("Years", 'Y'),
];
const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// Which timescale box the picker is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Start,
    Finish,
}

/// What the picker last worked out: the expression text and the minute it
/// resolves to, or a message.
type Evaluated = Result<(String, i64), String>;

/// The choices an [`Evaluated`] was worked out for.
type Choices = (usize, bool, String, usize, NaiveDate);

/// The picker's state: its choices and the month on show.
pub struct TsPicker {
    pub field: Field,
    /// Top-left corner of the window, under the box it was opened from.
    pub(super) at: Pos2,
    /// Index into [`ANCHORS`], or [`SPECIFIC`].
    pub(super) choice: usize,
    pub(super) minus: bool,
    pub(super) count: String,
    pub(super) unit: usize,
    /// The date picked on the calendar; with a relative choice, the day
    /// that choice comes to.
    pub(super) date: NaiveDate,
    /// Year and month the calendar shows.
    pub(super) view: (i32, u32),
    last: Option<(Choices, Evaluated)>,
}

/// A request from the Options tab to open the picker: which box, where it
/// drops down, and the expression the box holds.
pub struct Request {
    pub field: Field,
    pub at: Pos2,
    pub text: String,
}

fn epoch() -> NaiveDate {
    NaiveDate::from_ymd_opt(1899, 12, 30).expect("valid epoch")
}

/// The calendar day of a model minute.
fn date_of(origin: i64, minutes: i64) -> Option<NaiveDate> {
    epoch().checked_add_signed(Duration::days(origin.checked_add(minutes.div_euclid(DAY))?))
}

/// The model minute at midnight starting a calendar day.
fn minutes_of(origin: i64, date: NaiveDate) -> i64 {
    ((date - epoch()).num_days() - origin) * DAY
}

impl TsPicker {
    /// Starts from the expression a box holds. Text that does not read
    /// starts from the data date.
    pub(super) fn open(app: &App, request: Request) -> Self {
        let origin = app.project.model.origin_serial;
        let parse = |t: &str| format::parse_date(origin, t, Some(0)).ok().flatten();
        let expr = setup::parse_expr(&request.text, parse).unwrap_or(DateExpr::Anchored {
            anchor: Anchor::DataDate,
            offset: 0,
            unit: 'D',
        });
        let today = date_of(origin, super::compose::now_minutes(origin)).unwrap_or_default();
        let mut p = TsPicker {
            field: request.field,
            at: request.at,
            choice: 1,
            minus: false,
            count: "0".into(),
            unit: 0,
            date: today,
            view: (today.year(), today.month()),
            last: None,
        };
        match expr {
            DateExpr::Fixed(minutes) => {
                p.choice = SPECIFIC;
                p.date = date_of(origin, minutes).unwrap_or(today);
            }
            DateExpr::Anchored {
                anchor,
                offset,
                unit,
            } => {
                p.choice = ANCHORS.iter().position(|a| *a == anchor).unwrap_or(1);
                p.minus = offset < 0;
                p.count = offset.unsigned_abs().to_string();
                p.unit = UNITS.iter().position(|u| u.1 == unit).unwrap_or(0);
            }
        }
        if let Ok((_, minutes)) = p.evaluate(app) {
            p.follow(origin, minutes);
        }
        p.view = (p.date.year(), p.date.month());
        p
    }

    /// With a relative choice the calendar marks the day it comes to.
    fn follow(&mut self, origin: i64, minutes: i64) {
        if self.choice == SPECIFIC {
            return;
        }
        if let Some(day) = date_of(origin, minutes) {
            if day != self.date {
                self.date = day;
                self.view = (day.year(), day.month());
            }
        }
    }

    /// The expression these choices write and the minute it resolves to.
    pub(super) fn evaluate(&mut self, app: &App) -> Evaluated {
        let key = (
            self.choice,
            self.minus,
            self.count.clone(),
            self.unit,
            self.date,
        );
        if let Some((kept, result)) = &self.last {
            if *kept == key {
                return result.clone();
            }
        }
        let result = self.work_out(app);
        self.last = Some((key, result.clone()));
        result
    }

    fn work_out(&self, app: &App) -> Evaluated {
        let origin = app.project.model.origin_serial;
        if self.choice == SPECIFIC {
            let minutes = minutes_of(origin, self.date);
            return Ok((DateExpr::Fixed(minutes).text(|m| app.day_text(m)), minutes));
        }
        let digits = self.count.trim();
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return Err("The offset must be a whole number.".into());
        }
        let too_big = || "Offsets reach at most 50 years.".to_string();
        let n: i64 = digits.parse().map_err(|_| too_big())?;
        let unit = UNITS[self.unit].1;
        let per = match unit {
            'W' => 7,
            'M' => 30,
            'Y' => 365,
            _ => 1,
        };
        if n.checked_mul(per)
            .is_none_or(|days| days > setup::MAX_OFFSET_DAYS)
        {
            return Err(too_big());
        }
        let expr = DateExpr::Anchored {
            anchor: ANCHORS[self.choice],
            offset: if self.minus { -n } else { n },
            unit,
        };
        let text = expr.text(|m| app.day_text(m));
        let minutes = app.resolve_print_expr(&text)?;
        Ok((text, minutes))
    }
}

/// What the picker's buttons asked for this frame.
enum Done {
    Ok(String),
    Cancel,
}

impl App {
    /// Draws the picker over Page Setup when one is open. OK writes the
    /// expression into its box and unticks Same as screen; Cancel and
    /// Escape change nothing.
    pub(super) fn ts_picker_dialog(&mut self, ctx: &egui::Context, d: &mut SetupDialog) {
        if let Some(request) = d.picker_request.take() {
            d.picker = Some(TsPicker::open(self, request));
        }
        let Some(mut p) = d.picker.take() else {
            return;
        };
        let title = match p.field {
            Field::Start => "Timescale Start",
            Field::Finish => "Timescale Finish",
        };
        let at = p.at;
        let (done, closed, actions) = modal_at(ctx, "ts_picker", title, CLIENT, Some(at), |c| {
            draw(self, c, &mut p)
        });
        self.record_dialog_actions(actions);
        match done {
            Some(Done::Ok(text)) => d.use_expression(p.field, text, self.screen_range_texts()),
            Some(Done::Cancel) => {}
            None if closed => {}
            None => d.picker = Some(p),
        }
    }

    /// The dates Same as screen shows in the two boxes, in the user's date
    /// format.
    pub(super) fn screen_range_texts(&self) -> Option<(String, String)> {
        self.screen_timescale()
            .map(|(a, b)| (self.day_text(a), self.day_text(b)))
    }
}

impl SetupDialog {
    /// Puts `text` in a timescale box. A box edited by hand or by the
    /// picker is no longer the screen's: Same as screen unticks, and the
    /// other box keeps the date it was showing.
    pub(super) fn use_expression(
        &mut self,
        field: Field,
        text: String,
        screen: Option<(String, String)>,
    ) {
        let o = &mut self.draft.options;
        if o.same_as_screen {
            if let Some((start, finish)) = screen {
                o.timescale_start = start;
                o.timescale_finish = finish;
            }
            o.same_as_screen = false;
        }
        match field {
            Field::Start => o.timescale_start = text,
            Field::Finish => o.timescale_finish = text,
        }
    }
}

fn draw(app: &App, c: &mut Canvas, p: &mut TsPicker) -> Option<Done> {
    let origin = app.project.model.origin_serial;
    // Sync the calendar to a relative choice before drawing it.
    if let Ok((_, minutes)) = p.evaluate(app) {
        p.follow(origin, minutes);
    }
    c.fill(5., 5., 320., 310., kit::FACE);
    c.lowered(5., 5., 320., 310.);
    // What the date counts from.
    c.label(18., 17., 21., "Relative to:", true);
    let mut names: Vec<(&str, bool)> = ANCHORS.iter().map(|a| (a.caption(), true)).collect();
    names.push(("Specific Date", true));
    c.combo(
        "ts_picker_anchor",
        104.,
        17.,
        204.,
        21.,
        &names,
        &mut p.choice,
        true,
    );
    // Offset: sign, count, unit.
    let relative = p.choice != SPECIFIC;
    c.label(18., 47., 21., "Offset:", relative);
    let mut sign = usize::from(p.minus);
    if c.combo(
        "ts_picker_sign",
        104.,
        47.,
        44.,
        21.,
        &[("+", true), ("-", true)],
        &mut sign,
        relative,
    ) {
        p.minus = sign == 1;
    }
    c.edit(
        "ts_picker_count",
        154.,
        47.,
        56.,
        21.,
        &mut p.count,
        relative,
        egui::Align::Max,
    );
    let units: Vec<(&str, bool)> = UNITS.iter().map(|u| (u.0, true)).collect();
    c.combo(
        "ts_picker_unit",
        216.,
        47.,
        92.,
        21.,
        &units,
        &mut p.unit,
        relative,
    );
    c.etched(14., 78., 298.);
    calendar(app, c, p);
    c.etched(14., 256., 298.);
    // The expression and what it comes to.
    let result = p.evaluate(app);
    c.label(18., 263., 16., "Expression:", true);
    c.label(18., 283., 16., "Resolves to:", true);
    match &result {
        Ok((text, minutes)) => {
            c.bold(104., 263., 16., text);
            kit::text(c, 104., 284., &app.day_text(*minutes), true);
        }
        Err(message) => {
            kit::text(c, 104., 264., "-", true);
            kit::error_text(c, 104., 284., 218., message);
        }
    }
    kit::strip(c, CLIENT.x, CLIENT.y);
    let column = kit::button_column(
        c,
        CLIENT.x,
        &[
            Entry::Button("ts_picker_ok", "OK", result.is_ok(), false),
            Entry::cancel("ts_picker_cancel"),
        ],
    );
    match column.clicked.as_deref() {
        Some("ts_picker_ok") => result.ok().map(|(text, _)| Done::Ok(text)),
        Some("ts_picker_cancel") => Some(Done::Cancel),
        _ => None,
    }
}

/// The month calendar for a specific date: year and month arrows, the
/// weekday row and six weeks of days. Clicking a day chooses Specific Date.
fn calendar(app: &App, c: &mut Canvas, p: &mut TsPicker) {
    let origin = app.project.model.origin_serial;
    let ink = INK;
    let (mut year, mut month) = p.view;
    for (key, x, text, years, step) in [
        ("ts_picker_prev_year", 18., "<<", true, -1),
        ("ts_picker_prev_month", 50., "<", false, -1),
        ("ts_picker_next_month", 246., ">", false, 1),
        ("ts_picker_next_year", 278., ">>", true, 1),
    ] {
        if kit::button(c, key, x, 86., 30., 21., text, true) {
            if years {
                year += step;
            } else {
                let index = year * 12 + month as i32 - 1 + step;
                year = index.div_euclid(12);
                month = index.rem_euclid(12) as u32 + 1;
            }
        }
    }
    let year = year.clamp(1900, 2200);
    p.view = (year, month);
    let title = format!("{} {year}", MONTHS[month as usize - 1]);
    c.ui.painter().text(
        c.r(163., 96., 0., 0.).min,
        Align2::CENTER_CENTER,
        title,
        kit::body_font(),
        ink,
    );
    let (col_w, row_h) = (41., 20.);
    for (i, name) in ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"]
        .iter()
        .enumerate()
    {
        c.ui.painter().text(
            c.r(19. + col_w * (i as f32 + 0.5), 120., 0., 0.).min,
            Align2::CENTER_CENTER,
            name,
            kit::body_font(),
            ink,
        );
    }
    let Some(first) = NaiveDate::from_ymd_opt(year, month, 1) else {
        return;
    };
    let lead = i64::from(first.weekday().num_days_from_sunday());
    let today = date_of(origin, super::compose::now_minutes(origin));
    for cell in 0..42i64 {
        let Some(day) = first.checked_add_signed(Duration::days(cell - lead)) else {
            continue;
        };
        let (row, col) = (cell / 7, cell % 7);
        let (x, y) = (19. + col_w * col as f32, 130. + row_h * row as f32);
        let inside = day.month() == month;
        let key = if inside {
            format!("ts_picker_day_{}", day.day())
        } else {
            format!("ts_picker_outside_{cell}")
        };
        let rect = c.r(x, y, col_w, row_h);
        let hit = c.hit(&key, rect, true);
        let chosen = day == p.date;
        let painter = c.ui.painter();
        if chosen {
            painter.rect_filled(rect.shrink2(Vec2::new(4., 1.)), 0., kit::SELECTED);
        } else if hit.hovered() {
            painter.rect_filled(rect.shrink2(Vec2::new(4., 1.)), 0., chrome::pal::HOT_FILL);
        }
        if Some(day) == today {
            painter.rect_stroke(
                rect.shrink2(Vec2::new(4., 1.)),
                0.,
                Stroke::new(1_f32, if chosen { Color32::WHITE } else { ink }),
                egui::StrokeKind::Inside,
            );
        }
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            day.day().to_string(),
            kit::body_font(),
            if chosen {
                Color32::WHITE
            } else if inside {
                ink
            } else {
                kit::DISABLED
            },
        );
        if hit.clicked() {
            p.choice = SPECIFIC;
            p.date = day;
            p.view = (day.year(), day.month());
        }
    }
}
