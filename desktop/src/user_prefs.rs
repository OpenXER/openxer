//! Edit > User Preferences: a vertical-tab dialog (624 x 611 px inside the
//! frame: tab list at the left, page beside it, Help and Close at the bottom
//! right). Time Units, Dates, Currency, Assistance (the New Resource and New
//! Activity wizard switches, see `activity_wizard`, and the out-of-sequence
//! warning),
//! Application and Calculations (assignment options and Retain Lag) are
//! live; the other settings are drawn disabled with a tooltip. Changes apply
//! at once and are saved in the preferences table.
use super::*;
use crate::details::assignments::{AssignmentCalc, PriceSource, Staffing, CALC_KEY};
use crate::dialog_kit::{self as kit, VTab};
use crate::format::{
    CodeDisplay, CurrencyFormat, DateOrder, Formats, StartupWindow, TimeDisplay, TimeUnit,
    UnitFormat, UnitsPerTime, PREFERENCE_KEY, SEPARATORS,
};
use crate::schedule_dialogs::{modal, Canvas};
use crate::views::MainView;

/// Client size of the reference dialog.
const CLIENT: Vec2 = Vec2::new(624., 611.);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrefsTab {
    TimeUnits,
    Dates,
    Currency,
    Assistance,
    Application,
    Calculations,
}
impl PrefsTab {
    pub const ALL: [Self; 6] = [
        Self::TimeUnits,
        Self::Dates,
        Self::Currency,
        Self::Assistance,
        Self::Application,
        Self::Calculations,
    ];
    /// Caption with its mnemonic marker.
    pub fn label(self) -> &'static str {
        match self {
            Self::TimeUnits => "Time &Units",
            Self::Dates => "&Dates",
            Self::Currency => "&Currency",
            Self::Assistance => "A&ssistance",
            Self::Application => "&Application",
            Self::Calculations => "Ca&lculations",
        }
    }
    fn key(self) -> &'static str {
        match self {
            Self::TimeUnits => "time_units",
            Self::Dates => "dates",
            Self::Currency => "currency",
            Self::Assistance => "assistance",
            Self::Application => "application",
            Self::Calculations => "calculations",
        }
    }
}

/// The Application tab's Financial Period Data box: the dates as typed and
/// what the open project's periods make of them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FinRange {
    pub from: String,
    pub to: String,
    /// Periods of the project's financial period calendar.
    pub total: usize,
    /// Of those, the ones that fall in the saved range.
    pub loaded: usize,
    /// A financial period calendar exists for the project.
    pub has_calendar: bool,
    pub error: Option<String>,
}

pub struct PrefsState {
    pub open: bool,
    pub tab: PrefsTab,
    pub fin: FinRange,
    /// Currency definitions of the open project, read when the dialog opens.
    currencies: Vec<CurrencyFormat>,
    /// Admin > Admin Preferences.
    pub admin: crate::admin_prefs::AdminPrefs,
    /// The Application tab's recovery copy setting as shown.
    pub autosave: AutosaveField,
}

/// Recovery copies on or off, the minutes as typed, and why they were not
/// taken.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AutosaveField {
    pub on: bool,
    pub text: String,
    pub error: Option<String>,
}

impl AutosaveField {
    /// The interval this field asks for (0: off), or why it cannot be used.
    pub fn minutes(&self) -> Result<u32, String> {
        use crate::open_set::recovery::MAX_MINUTES;
        if !self.on {
            return Ok(0);
        }
        self.text
            .trim()
            .parse::<u32>()
            .ok()
            .filter(|m| (1..=MAX_MINUTES).contains(m))
            .ok_or(format!("Enter whole minutes from 1 to {MAX_MINUTES}."))
    }
}
impl Default for PrefsState {
    fn default() -> Self {
        Self {
            open: false,
            tab: PrefsTab::TimeUnits,
            fin: FinRange::default(),
            currencies: vec![],
            admin: Default::default(),
            autosave: Default::default(),
        }
    }
}

/// Saved preferences, or an error describing why they were rejected.
pub fn load(store: &Store) -> Result<Option<Formats>, String> {
    match store.preference(PREFERENCE_KEY)? {
        Some(json) => Formats::from_json(&json).map(Some),
        None => Ok(None),
    }
}

impl App {
    /// Read saved formats at startup. Invalid data is ignored and reported.
    pub(crate) fn restore_formats(&mut self) {
        match load(&self.store) {
            Ok(Some(formats)) => self.formats = formats,
            Ok(None) => {}
            Err(error) => {
                self.formats = Formats::default();
                self.status = format!("User preferences were reset: {error}");
            }
        }
        format::install(&self.formats);
        self.views.go(match self.formats.startup {
            StartupWindow::Activities => MainView::Activities,
            StartupWindow::Wbs => MainView::Wbs,
            StartupWindow::Projects => MainView::Projects,
            StartupWindow::Resources => MainView::Resources,
        });
    }
    pub(crate) fn save_formats(&mut self) {
        if let Err(error) = self
            .store
            .set_preference(PREFERENCE_KEY, &self.formats.to_json())
        {
            self.error(format!("Cannot save user preferences: {error}"));
        }
    }
    pub(crate) fn open_user_preferences(&mut self) {
        self.prefs.open = true;
        self.prefs.currencies = CurrencyFormat::from_project(&self.project);
        self.prefs.fin = FinRange {
            from: self.formats.fin_from.map(format::day).unwrap_or_default(),
            to: self.formats.fin_to.map(format::day).unwrap_or_default(),
            ..Default::default()
        };
        self.count_fin_periods();
        let minutes = self.autosave_minutes();
        self.prefs.autosave = AutosaveField {
            on: minutes > 0,
            text: if minutes > 0 {
                minutes
            } else {
                crate::open_set::recovery::DEFAULT_MINUTES
            }
            .to_string(),
            error: None,
        };
    }

    /// The periods of the open project's financial period calendar and how
    /// many of them the saved range loads.
    pub(crate) fn count_fin_periods(&mut self) {
        let doc = &self.project.document;
        let pid = self.project.model.project.proj_id;
        let calendar = enterprise::ops::project_fin_calendar(doc, pid);
        let all = calendar.map_or(vec![], |c| enterprise::ops::periods(doc, c));
        self.prefs.fin.has_calendar = calendar.is_some();
        self.prefs.fin.total = all.len();
        self.prefs.fin.loaded =
            enterprise::ops::in_range(all, self.formats.fin_from, self.formats.fin_to).len();
    }

    /// The Select button of Financial Period Data: reads the two dates and
    /// keeps them as the range of periods to load. Both empty loads every
    /// period. A bad entry changes nothing.
    pub(crate) fn select_fin_range(&mut self) {
        let fin = &self.prefs.fin;
        let parse = |text: &str| format::parse_day(text);
        let range = parse(&fin.from).and_then(|a| parse(&fin.to).map(|b| (a, b)));
        let range = range.and_then(|(a, b)| match (a, b) {
            (Some(_), None) | (None, Some(_)) => {
                Err("Enter both dates, or leave both empty to load every period.".into())
            }
            (Some(a), Some(b)) if a > b => Err("The first date is after the second.".into()),
            ok => Ok(ok),
        });
        match range {
            Ok((from, to)) => {
                self.formats.fin_from = from;
                self.formats.fin_to = to;
                self.prefs.fin.error = None;
                self.count_fin_periods();
            }
            Err(e) => self.prefs.fin.error = Some(e),
        }
    }

    pub(crate) fn user_preferences_dialog(&mut self, ctx: &egui::Context) {
        self.admin_preferences_dialog(ctx);
        if !self.prefs.open {
            return;
        }
        let before = self.formats.clone();
        let mut retain_lag = self.editing.options.retain_lag;
        let mut warn_sequence = self.editing.options.warn_out_of_sequence;
        let mut show_copy = self.editing.options.copy.show_dialog;
        let mut ask_start = self.sched.start_hold_prefs.ask;
        let mut calc = self.detail.calc;
        let mut activity_wizard = self.activity_wizard.assistance.activity_wizard;
        let mut resource_wizard = self.activity_wizard.assistance.resource_wizard;
        let mut select_range = false;
        let tabs: Vec<VTab> = PrefsTab::ALL
            .iter()
            .map(|t| VTab {
                label: t.label(),
                key: t.key(),
                unavailable: None,
            })
            .collect();
        let ((close, help), closed, actions) =
            modal(ctx, "user_preferences", "User Preferences", CLIENT, |c| {
                c.fill(0., 0., CLIENT.x, CLIENT.y, kit::STRIP);
                kit::page(c, 130., 5., 489., 571.);
                let active = PrefsTab::ALL
                    .iter()
                    .position(|t| *t == self.prefs.tab)
                    .unwrap_or(0);
                if let Some(i) = kit::vertical_tabs(c, "prefs_tab_", 5., 5., 126., &tabs, active) {
                    self.prefs.tab = PrefsTab::ALL[i];
                }
                match PrefsTab::ALL[active] {
                    PrefsTab::TimeUnits => time_units_tab(c, &mut self.formats),
                    PrefsTab::Dates => dates_tab(c, &mut self.formats),
                    PrefsTab::Currency => {
                        currency_tab(c, &self.prefs.currencies, &mut self.formats.currency)
                    }
                    PrefsTab::Assistance => assistance_tab(
                        c,
                        &mut resource_wizard,
                        &mut activity_wizard,
                        &mut warn_sequence,
                    ),
                    PrefsTab::Application => {
                        select_range |= application_tab(
                            c,
                            &mut self.formats,
                            &mut self.prefs.fin,
                            &mut show_copy,
                            &mut ask_start,
                            &mut self.prefs.autosave,
                        )
                    }
                    PrefsTab::Calculations => calculations_tab(c, &mut retain_lag, &mut calc),
                }
                // Settings apply as they change: Close, no OK and Cancel.
                let row = kit::button_row(
                    c,
                    CLIENT.x - 5.,
                    581.,
                    &[
                        kit::Entry::close("prefs_close"),
                        kit::Entry::help("prefs_help"),
                    ],
                );
                // Enter closes, not the Select button on the Application tab.
                kit::mark_default(c, "prefs_close");
                let clicked = |key: &str| row.clicked.as_deref() == Some(key);
                (clicked("prefs_close"), clicked("prefs_help"))
            });
        self.record_dialog_actions(actions);
        if help {
            self.open_help_for("user_preferences");
        }
        if select_range {
            self.select_fin_range();
        }
        if calc != self.detail.calc {
            self.detail.calc = calc;
            let text = serde_json::to_string(&calc).unwrap_or_default();
            if let Err(e) = self.store.set_preference(CALC_KEY, &text) {
                self.error(format!("Cannot save the calculation options: {e}"));
            }
        }
        if resource_wizard != self.activity_wizard.assistance.resource_wizard {
            self.set_resource_wizard(resource_wizard);
        }
        if activity_wizard != self.activity_wizard.assistance.activity_wizard {
            self.set_activity_wizard(activity_wizard);
        }
        if retain_lag != self.editing.options.retain_lag {
            self.editing.options.retain_lag = retain_lag;
            self.save_editing_options();
        }
        if warn_sequence != self.editing.options.warn_out_of_sequence {
            self.editing.options.warn_out_of_sequence = warn_sequence;
            self.save_editing_options();
        }
        // The way back from "Skip this dialog next time".
        if show_copy != self.editing.options.copy.show_dialog {
            self.editing.options.copy.show_dialog = show_copy;
            self.save_editing_options();
        }
        // The way back from "Always use this answer" on a typed Start.
        if ask_start != self.sched.start_hold_prefs.ask {
            let hold = self.sched.start_hold_prefs.hold;
            self.set_start_hold(crate::start_hold::Prefs {
                ask: ask_start,
                hold,
            });
        }
        // Recovery copies: a valid entry applies at once; a bad one is
        // explained beside the field and changes nothing.
        match self.prefs.autosave.minutes() {
            Ok(minutes) => {
                self.prefs.autosave.error = None;
                if minutes != self.autosave_minutes() {
                    self.set_autosave_minutes(minutes);
                }
            }
            Err(e) => self.prefs.autosave.error = Some(e),
        }
        if self.formats != before {
            format::install(&self.formats);
            self.date_edits.clear();
            self.detail.dates.clear();
            self.save_formats();
        }
        if close || closed {
            self.prefs.open = false;
        }
    }
}

/// Unit of Time, Sub-unit, Decimals, label and example for one format; `top`
/// is the section's top edge.
fn unit_section(
    c: &mut Canvas,
    top: f32,
    title: &str,
    key: &str,
    label: &str,
    unit: &mut UnitFormat,
) {
    let h = if key == "units" { 116. } else { 112. };
    kit::section(c, 137., top, 475., h, title);
    let y = top + 27.;
    kit::text(c, 149., y, "Unit of Time", true);
    kit::text(c, 269., y, "Sub-unit", true);
    kit::text(c, 424., y, "Decimals", true);
    let items: Vec<(&str, bool)> = TimeUnit::ALL.iter().map(|u| (u.label(), true)).collect();
    let mut index = TimeUnit::ALL
        .iter()
        .position(|u| *u == unit.unit)
        .unwrap_or(0);
    if c.combo(
        &format!("prefs_{key}_unit"),
        149.,
        top + 42.,
        105.,
        22.,
        &items,
        &mut index,
        true,
    ) {
        unit.unit = TimeUnit::ALL[index];
    }
    c.checkbox(
        &format!("prefs_{key}_sub_unit"),
        269.,
        top + 43.,
        149.,
        17.,
        unit.unit.sub_unit_label(),
        &mut unit.sub_unit,
        true,
    );
    let decimals: Vec<String> = (0..=UnitFormat::MAX_DECIMALS)
        .map(|n| n.to_string())
        .collect();
    let items: Vec<(&str, bool)> = decimals.iter().map(|d| (d.as_str(), true)).collect();
    let mut index = unit.decimals as usize;
    if c.combo(
        &format!("prefs_{key}_decimals"),
        424.,
        top + 42.,
        50.,
        22.,
        &items,
        &mut index,
        true,
    ) {
        unit.decimals = index as u8;
    }
    let sub_unit = unit.sub_unit;
    c.checkbox(
        &format!("prefs_{key}_label"),
        149.,
        top + 80.,
        213.,
        17.,
        label,
        &mut unit.show_label,
        !sub_unit,
    );
    kit::text(c, 378., top + 82., "Example", true);
    let example = Formats {
        units: *unit,
        durations: *unit,
        ..Formats::default()
    };
    let sample = example.duration(
        if key == "units" { 41 * 60 } else { 10 * 480 },
        format::Periods::DEFAULT,
    );
    kit::field(c, 424., top + 79., 86., 21., &sample, true);
}

fn time_units_tab(c: &mut Canvas, f: &mut Formats) {
    unit_section(
        c,
        12.,
        "Units Format",
        "units",
        "Show Unit label",
        &mut f.units,
    );
    unit_section(
        c,
        128.,
        "Durations Format",
        "durations",
        "Show Duration label",
        &mut f.durations,
    );
    kit::section(c, 137., 242., 475., 325., "Units/Time Format");
    kit::lines(
        c,
        146.,
        266.,
        14.,
        &[
            "Resource Units/Time can be shown as a percentage or as",
            "units per duration",
        ],
        true,
    );
    for (value, label, key, y) in [
        (
            UnitsPerTime::Percentage,
            "Show as a percentage (50%)",
            "percent",
            300.,
        ),
        (
            UnitsPerTime::UnitsPerDuration,
            "Show as units/duration (4h/d)",
            "duration",
            321.,
        ),
    ] {
        if c.radio(
            &format!("prefs_units_time_{key}"),
            146.,
            y,
            304.,
            17.,
            label,
            f.units_per_time == value,
            true,
        ) {
            f.units_per_time = value;
        }
    }
}

fn dates_tab(c: &mut Canvas, f: &mut Formats) {
    let d = &mut f.dates;
    kit::section(c, 137., 12., 309., 357., "Date Format");
    for (order, label, key, y) in [
        (DateOrder::MonthDayYear, "M&onth, Day, Year", "mdy", 36.),
        (DateOrder::DayMonthYear, "Day, Mont&h, Year", "dmy", 59.),
        (DateOrder::YearMonthDay, "&Year, Month, Day", "ymd", 82.),
    ] {
        if kit::radio(
            c,
            &format!("prefs_date_{key}"),
            154.,
            y,
            139.,
            17.,
            label,
            d.order == order,
            true,
        ) {
            d.order = order;
        }
    }
    kit::section(c, 137., 372., 309., 126., "Time");
    for (time, label, key, y) in [
        (TimeDisplay::Hour12, "&12 hour (1:30 PM)", "12", 396.),
        (TimeDisplay::Hour24, "&24 hour (13:30)", "24", 419.),
        (TimeDisplay::Hidden, "Do not sho&w time", "none", 442.),
    ] {
        if kit::radio(
            c,
            &format!("prefs_time_{key}"),
            153.,
            y,
            137.,
            17.,
            label,
            d.time == time,
            true,
        ) {
            d.time = time;
        }
    }
    let timed = d.time != TimeDisplay::Hidden;
    c.checkbox(
        "prefs_show_minutes",
        153.,
        473.,
        137.,
        17.,
        "Show minutes",
        &mut d.show_minutes,
        timed,
    );
    kit::section(c, 448., 12., 164., 486., "Options");
    kit::check_right(
        c,
        "prefs_four_digit_year",
        460.,
        46.,
        137.,
        17.,
        "&4-digit year",
        &mut d.four_digit_year,
        true,
    );
    kit::check_right(
        c,
        "prefs_month_name",
        460.,
        73.,
        137.,
        17.,
        "Month &name",
        &mut d.month_name,
        true,
    );
    kit::check_right(
        c,
        "prefs_leading_zeros",
        460.,
        100.,
        137.,
        17.,
        "&Leading zeroes",
        &mut d.leading_zeros,
        true,
    );
    kit::mnemonic_text(c, 460., 131., "Se&parator", true);
    let shown: Vec<String> = SEPARATORS
        .iter()
        .map(|&s| {
            if s == ' ' {
                "(space)".to_string()
            } else {
                s.to_string()
            }
        })
        .collect();
    let items: Vec<(&str, bool)> = shown.iter().map(|s| (s.as_str(), true)).collect();
    let mut index = SEPARATORS
        .iter()
        .position(|&s| s == d.separator)
        .unwrap_or(0);
    if c.combo(
        "prefs_separator",
        543.,
        127.,
        54.,
        23.,
        &items,
        &mut index,
        true,
    ) {
        d.separator = SEPARATORS[index];
    }
    kit::section(c, 137., 504., 475., 65., "Sample");
    let sample = f.date_example();
    kit::field(c, 145., 536., 150., 21., &sample, false);
}

fn currency_tab(c: &mut Canvas, currencies: &[CurrencyFormat], current: &mut CurrencyFormat) {
    let mut choices = currencies.to_vec();
    if !choices.iter().any(|x| x.name == current.name) {
        let mut own = current.clone();
        own.show_symbol = true;
        own.show_decimals = true;
        choices.insert(0, own);
    }
    kit::section(c, 137., 12., 475., 188., "Currency Options");
    let names: Vec<String> = choices
        .iter()
        .map(|x| format!("{} ({})", x.name, x.symbol))
        .collect();
    let items: Vec<(&str, bool)> = names.iter().map(|n| (n.as_str(), true)).collect();
    let mut index = choices
        .iter()
        .position(|x| x.name == current.name)
        .unwrap_or(0);
    if c.combo(
        "prefs_currency",
        158.,
        83.,
        220.,
        21.,
        &items,
        &mut index,
        true,
    ) {
        let (symbol, decimals) = (current.show_symbol, current.show_decimals);
        *current = choices[index].clone();
        current.show_symbol = symbol;
        current.show_decimals = decimals;
    }
    c.checkbox(
        "prefs_currency_symbol",
        153.,
        127.,
        227.,
        17.,
        "Show currency symbol",
        &mut current.show_symbol,
        true,
    );
    c.checkbox(
        "prefs_currency_decimals",
        153.,
        159.,
        227.,
        17.,
        "Show decimal digits",
        &mut current.show_decimals,
        true,
    );
    let symbol = if current.show_symbol {
        current.symbol.clone()
    } else {
        String::new()
    };
    kit::text(c, 399., 128., &symbol, true);
    let digits = if current.show_decimals {
        current.decimals as usize
    } else {
        0
    };
    kit::text(c, 398., 160., &format!("{:.*}", digits, 0.0), true);
}

/// Assistance: whether Add opens the New Resource and New Activity wizards,
/// and the out-of-sequence warning (off by default). Each keeps its own storage.
fn assistance_tab(
    c: &mut Canvas,
    resource_wizard: &mut bool,
    activity_wizard: &mut bool,
    warn_sequence: &mut bool,
) {
    kit::section(c, 137., 12., 475., 150., "Wizards");
    kit::lines(
        c,
        146.,
        43.,
        14.,
        &[
            "A wizard can take you through the fields of a new resource or",
            "activity when you add one.",
        ],
        true,
    );
    c.checkbox(
        "prefs_wizard_resource",
        147.,
        96.,
        300.,
        16.,
        "Add resources through the wizard",
        resource_wizard,
        true,
    );
    c.checkbox(
        "prefs_wizard_activity",
        147.,
        122.,
        300.,
        16.,
        "Add activities through the wizard",
        activity_wizard,
        true,
    );
    kit::section(c, 137., 174., 475., 104., "Out of Sequence");
    kit::lines(
        c,
        146.,
        201.,
        14.,
        &[
            "Progress entered on an activity whose predecessors are not",
            "finished leaves it out of sequence. A question can ask first.",
        ],
        true,
    );
    c.checkbox(
        "prefs_out_of_sequence",
        147.,
        246.,
        320.,
        16.,
        "Warn before progress goes out of sequence",
        warn_sequence,
        true,
    );
}

/// Draws the tab; true when Select was pressed.
fn application_tab(
    c: &mut Canvas,
    f: &mut Formats,
    fin: &mut FinRange,
    show_copy: &mut bool,
    ask_start: &mut bool,
    autosave: &mut AutosaveField,
) -> bool {
    kit::section(c, 137., 12., 475., 80., "Startup Window");
    kit::text(c, 146., 43., "Application Startup Window", true);
    let items: Vec<(&str, bool)> = StartupWindow::ALL
        .iter()
        .map(|s| (s.label(), true))
        .collect();
    let mut index = StartupWindow::ALL
        .iter()
        .position(|s| *s == f.startup)
        .unwrap_or(0);
    if c.combo(
        "prefs_startup",
        146.,
        58.,
        215.,
        22.,
        &items,
        &mut index,
        true,
    ) {
        f.startup = StartupWindow::ALL[index];
    }
    kit::section(c, 137., 96., 475., 97., "Group and Sorting");
    kit::text(c, 147., 127., "Labels on grouping bands", true);
    c.checkbox(
        "prefs_band_code",
        148.,
        148.,
        320.,
        18.,
        "Show ID / Code",
        &mut f.band_code,
        true,
    );
    c.checkbox(
        "prefs_band_name",
        148.,
        170.,
        320.,
        18.,
        "Show Name / Description",
        &mut f.band_name,
        true,
    );
    // Codes: how activity code columns show an assigned value.
    kit::section(c, 137., 193., 475., 90., "Codes");
    kit::text(c, 147., 219., "Specify how to display code values", true);
    for (key, y, label, choice) in [
        ("prefs_codes_value", 237., "Code Value", CodeDisplay::Value),
        (
            "prefs_codes_description",
            257.,
            "Description",
            CodeDisplay::Description,
        ),
    ] {
        if c.radio(key, 147., y, 300., 17., label, f.codes == choice, true) {
            f.codes = choice;
        }
    }
    kit::section(c, 137., 283., 475., 95., "Financial Period Data");
    kit::text(
        c,
        148.,
        313.,
        "Select dates range to load Financial Period Data",
        true,
    );
    c.edit(
        "prefs_periods_from",
        147.,
        332.,
        100.,
        21.,
        &mut fin.from,
        true,
        Align::Min,
    );
    kit::text(c, 274., 335., "to", true);
    c.edit(
        "prefs_periods_to",
        305.,
        332.,
        100.,
        21.,
        &mut fin.to,
        true,
        Align::Min,
    );
    let select = kit::button(
        c,
        "prefs_periods_select",
        425.,
        332.,
        80.,
        25.,
        "Select",
        true,
    );
    let note = match (&fin.error, fin.has_calendar) {
        (Some(e), _) => e.clone(),
        (None, false) => "No financial period calendar is defined for the project.".into(),
        (None, true) if f.fin_from.is_none() => {
            format!("All {} financial periods are loaded.", fin.total)
        }
        (None, true) => format!(
            "{} of {} financial periods are loaded.",
            fin.loaded, fin.total
        ),
    };
    if fin.error.is_some() {
        kit::error_text(c, 148., 358., 460., &note);
    } else {
        kit::text(c, 148., 358., &note, true);
    }
    kit::section(c, 137., 382., 475., 77., "Dialogs");
    c.checkbox(
        "prefs_show_copy_options",
        148.,
        410.,
        440.,
        18.,
        "Show Copy Activity Options when pasting activities",
        show_copy,
        true,
    );
    c.checkbox(
        "prefs_ask_start_hold",
        148.,
        432.,
        440.,
        18.,
        "Ask whether a moved Start should hold the activity with a constraint",
        ask_start,
        true,
    );
    kit::section(c, 137., 463., 475., 82., "Recovery");
    c.checkbox(
        "prefs_autosave",
        148.,
        491.,
        292.,
        18.,
        "Keep a recovery copy of unsaved changes every",
        &mut autosave.on,
        true,
    );
    c.edit(
        "prefs_autosave_minutes",
        444.,
        489.,
        40.,
        21.,
        &mut autosave.text,
        autosave.on,
        Align::Max,
    );
    kit::text(c, 490., 492., "minutes", autosave.on);
    if let Some(error) = &autosave.error {
        kit::error_text(c, 148., 518., 460., error);
    } else {
        kit::text(
            c,
            148.,
            518.,
            "If OpenXER closes unexpectedly, the next start offers the copy.",
            true,
        );
    }
    select
}

fn calculations_tab(c: &mut Canvas, retain_lag: &mut bool, calc: &mut AssignmentCalc) {
    c.lowered(139., 14., 471., 149.);
    kit::heading(c, 146., 21., 457., "Resource Assignments");
    kit::lines(
        c,
        155.,
        46.,
        14.,
        &[
            "When adding or removing multiple resource assignments on",
            "activities",
        ],
        true,
    );
    for (key, y, h, label, recalculate) in [
        (
            "prefs_calc_preserve",
            84.,
            21.,
            "Preserve the Units, Duration, and Units/Time for existing assignments",
            false,
        ),
        (
            "prefs_calc_recalculate",
            116.,
            28.,
            "Recalculate the Units, Duration, and Units/Time for existing assignments\nbased on the activity Duration Type",
            true,
        ),
    ] {
        if kit::radio(
            c,
            key,
            158.,
            y,
            400.,
            h,
            label,
            calc.recalculate == recalculate,
            true,
        ) {
            calc.recalculate = recalculate;
        }
    }
    c.lowered(139., 163., 471., 314.);
    kit::heading(c, 140., 164., 469., "Assignment Staffing");
    kit::text(
        c,
        149.,
        192.,
        "When assigning a resource to an existing activity assignment:",
        true,
    );
    for (key, y, label, value) in [
        (
            "prefs_staff_new",
            214.,
            "Always use the new resource's Units per Time and Overtime factor",
            Staffing::New,
        ),
        (
            "prefs_staff_current",
            234.,
            "Always use current assignment's Units per Time and Overtime factor",
            Staffing::Current,
        ),
        (
            "prefs_staff_ask",
            258.,
            "Ask me to select each time I assign",
            Staffing::Ask,
        ),
    ] {
        if kit::radio(
            c,
            key,
            151.,
            y,
            400.,
            19.,
            label,
            calc.staffing == value,
            true,
        ) {
            calc.staffing = value;
        }
    }
    kit::text(
        c,
        149.,
        294.,
        "When a resource and role share an activity assignment:",
        true,
    );
    for (key, y, label, value) in [
        (
            "prefs_price_resource",
            317.,
            "Always use resource's Price per Unit",
            PriceSource::Resource,
        ),
        (
            "prefs_price_role",
            337.,
            "Always use role's Price per Unit",
            PriceSource::Role,
        ),
        (
            "prefs_price_ask",
            359.,
            "Ask me to select each time I assign",
            PriceSource::Ask,
        ),
    ] {
        if kit::radio(c, key, 151., y, 400., 19., label, calc.price == value, true) {
            calc.price = value;
        }
    }
    c.lowered(132., 484., 485., 90.);
    kit::heading(c, 139., 491., 471., "Dissolving Activities");
    c.lowered(139., 512., 471., 55.);
    c.checkbox(
        "prefs_retain_lag",
        148.,
        525.,
        307.,
        15.,
        "Retain Lag",
        retain_lag,
        true,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::headless_render::SoftwarePainter;

    fn temp_db(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "openxer-{name}-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    /// Every text drawn in one frame.
    fn texts(app: &mut App, ctx: &egui::Context, time: &mut f64) -> Vec<String> {
        *time += 0.1;
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1500., 900.))),
                time: Some(*time),
                ..Default::default()
            },
            |ctx| app.update_ui(ctx),
        );
        fn walk(shape: &egui::Shape, out: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(t) => out.push(t.galley.text().to_string()),
                egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
                _ => {}
            }
        }
        let mut out = vec![];
        for clipped in &output.shapes {
            walk(&clipped.shape, &mut out);
        }
        out
    }

    #[test]
    fn dialog_changes_apply_at_once_persist_and_reformat_the_grid() {
        let path = temp_db("prefs-dialog");
        {
            let ctx = egui::Context::default();
            ctx.set_pixels_per_point(1.);
            let store = Store::open(path.clone()).unwrap();
            let mut project = openxer_xer::starter();
            schedule_model(&mut project.model, &project.resources).unwrap();
            let mut app = App::new(&ctx, store, project);
            let mut painter = SoftwarePainter::default();
            let mut time = 0.;
            for _ in 0..3 {
                let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
            }
            let (index, a) = app
                .project
                .model
                .activities
                .iter()
                .enumerate()
                .find(|(_, a)| a.rem > 0 && a.rem % 480 == 0)
                .map(|(i, a)| (i, a.clone()))
                .unwrap();
            let periods = format::activity_periods(&app.project, index);
            assert_eq!(
                periods,
                format::Periods::DEFAULT,
                "starter calendar has no hours, so 8 h/day"
            );
            let days = format!("{}d", a.rem / 480);
            let shown = texts(&mut app, &ctx, &mut time);
            assert!(shown.contains(&days), "grid shows {days}");
            let start = app
                .formats
                .date(app.project.model.origin_serial, displayed_start(&a, false));
            assert!(
                start.len() == "05-Mar-26 13:07".len()
                    && shown.iter().any(|t| t.starts_with(&start)),
                "{start} in {shown:?}"
            );

            app.open_user_preferences();
            for _ in 0..3 {
                let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
            }
            for tab in PrefsTab::ALL {
                assert!(app
                    .action_rects
                    .contains_key(&format!("prefs_tab_{}", tab.key())));
            }
            painter.click_action(&mut app, &ctx, &mut time, "prefs_tab_dates");
            assert_eq!(app.prefs.tab, PrefsTab::Dates);
            painter.click_action(&mut app, &ctx, &mut time, "prefs_four_digit_year");
            painter.click_action(&mut app, &ctx, &mut time, "prefs_time_none");
            assert!(app.formats.dates.four_digit_year);
            assert_eq!(app.formats.dates.time, TimeDisplay::Hidden);
            assert_eq!(format::current(), app.formats, "installed at once");
            let saved = load(&app.store).unwrap().unwrap();
            assert_eq!(saved, app.formats, "saved at once");
            // A disabled tab opens but changes nothing.
            painter.click_action(&mut app, &ctx, &mut time, "prefs_tab_calculations");
            assert_eq!(app.prefs.tab, PrefsTab::Calculations);
            painter.click_action(&mut app, &ctx, &mut time, "prefs_close");
            assert!(!app.prefs.open);

            let shown = texts(&mut app, &ctx, &mut time);
            let start = app
                .formats
                .date(app.project.model.origin_serial, displayed_start(&a, false));
            assert!(
                start.len() == "05-Mar-2026".len() && shown.iter().any(|t| t.starts_with(&start)),
                "{start}"
            );

            // Hours mode, as older builds showed durations.
            app.formats.durations.unit = TimeUnit::Hour;
            let shown = texts(&mut app, &ctx, &mut time);
            assert!(shown.contains(&format!("{}h", a.rem / 60)));
            assert!(app.error.is_none());
        }
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn render_user_preferences_and_grid_in_days_to_png() {
        let artifacts = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("artifacts");
        std::fs::create_dir_all(&artifacts).unwrap();
        let path = temp_db("prefs-render");
        {
            let ctx = egui::Context::default();
            ctx.set_pixels_per_point(1.);
            let store = Store::open(path.clone()).unwrap();
            let mut project = openxer_xer::starter();
            schedule_model(&mut project.model, &project.resources).unwrap();
            let mut app = App::new(&ctx, store, project);
            let mut painter = SoftwarePainter::default();
            let mut time = 0.;
            for _ in 0..3 {
                let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
            }
            let (image, _) = painter
                .frame(&mut app, &ctx, &mut time, vec![], true)
                .unwrap();
            image
                .save(artifacts.join("headless-grid-days.png"))
                .unwrap();
            app.open_user_preferences();
            let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
            for (tab, name) in [
                (PrefsTab::TimeUnits, "headless-user-preferences.png"),
                (PrefsTab::Dates, "headless-user-preferences-dates.png"),
                (PrefsTab::Currency, "headless-user-preferences-currency.png"),
                (
                    PrefsTab::Application,
                    "headless-user-preferences-application.png",
                ),
            ] {
                app.prefs.tab = tab;
                let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
                let (image, triangles) = painter
                    .frame(&mut app, &ctx, &mut time, vec![], true)
                    .unwrap();
                assert!(triangles > 500, "{name} draws the dialog");
                image.save(artifacts.join(name)).unwrap();
                println!("Software-rendered: {}", artifacts.join(name).display());
            }
        }
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn invalid_saved_preferences_are_rejected_and_reset() {
        let path = std::env::temp_dir().join(format!(
            "openxer-prefs-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        {
            let store = Store::open(path.clone()).unwrap();
            assert_eq!(load(&store), Ok(None));
            store
                .set_preference(PREFERENCE_KEY, r#"{"durations":{"unit":"day"}}"#)
                .unwrap();
            assert!(load(&store).is_err());
            let ctx = egui::Context::default();
            let app = App::new(&ctx, store, openxer_xer::starter());
            assert_eq!(app.formats, Formats::default());
            assert!(app.status.starts_with("User preferences were reset"));

            let mut saved = Formats::default();
            saved.durations.unit = TimeUnit::Hour;
            saved.startup = StartupWindow::Wbs;
            app.store
                .set_preference(PREFERENCE_KEY, &saved.to_json())
                .unwrap();
            let store = Store::open(path.clone()).unwrap();
            let app = App::new(&ctx, store, openxer_xer::starter());
            assert_eq!(app.formats, saved);
            assert_eq!(app.views.main, MainView::Wbs);
            assert_eq!(format::current(), saved);
        }
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn codes_preference_switches_code_columns_between_value_and_description() {
        let mut project = openxer_xer::starter();
        schedule_model(&mut project.model, &project.resources).unwrap();
        let doc = &mut project.document;
        doc.table_mut("ACTVTYPE").add(&[
            ("actv_code_type_id", "7".into()),
            ("actv_code_type", "Area".into()),
            ("actv_code_type_scope", "AS_Project".into()),
            ("proj_id", "1".into()),
            ("seq_num", "1".into()),
        ]);
        doc.table_mut("ACTVCODE").add(&[
            ("actv_code_id", "71".into()),
            ("actv_code_type_id", "7".into()),
            ("short_name", "N".into()),
            ("actv_code_name", "North".into()),
            ("seq_num", "1".into()),
        ]);
        let task_id = project.model.activities[0].task_id;
        crate::records::add_row(
            doc,
            "TASKACTV",
            &[
                ("task_id", task_id.to_string()),
                ("actv_code_type_id", "7".into()),
                ("actv_code_id", "71".into()),
                ("proj_id", "1".into()),
            ],
        );
        let mut d = crate::qa_sweep::Driver::new("prefs-codes", project);
        let shown = |d: &crate::qa_sweep::Driver| {
            let catalog = grid::columns::catalog(&d.app.project);
            let column = catalog.iter().find(|c| c.id == "code:Area").unwrap();
            let ix = d.app.grid_index();
            let value = grid::columns::value(&d.app.project, &ix, column, 0);
            let shown = grid::columns::shown_value(&d.app.project, &ix, column, 0);
            (
                grid::columns::display(&d.app.project, Some(0), &value),
                grid::columns::display(&d.app.project, Some(0), &shown),
            )
        };
        assert_eq!(shown(&d), ("N".into(), "N".into()));
        d.click("menu_bar_edit");
        d.click("menu_user_preferences");
        d.click("prefs_tab_application");
        d.click("prefs_codes_description");
        assert_eq!(d.app.formats.codes, CodeDisplay::Description);
        // The table shows descriptions; sorting and filtering keep values.
        assert_eq!(shown(&d), ("N".into(), "North".into()));
        d.click("prefs_close");
        // Kept in the saved preferences; older records without the setting
        // read as Code Value.
        let saved = load(&d.app.store).unwrap().unwrap();
        assert_eq!(saved.codes, CodeDisplay::Description);
        let mut old: serde_json::Value = serde_json::from_str(&saved.to_json()).unwrap();
        old["application"].as_object_mut().unwrap().remove("codes");
        let old = Formats::from_json(&old.to_string()).unwrap();
        assert_eq!(old.codes, CodeDisplay::Value);
        d.click("menu_bar_edit");
        d.click("menu_user_preferences");
        d.click("prefs_tab_application");
        d.click("prefs_codes_value");
        d.click("prefs_close");
        assert_eq!(shown(&d), ("N".into(), "N".into()));
        d.assert_clean("codes preference");
    }

    /// "Skip this dialog next time" on Copy Activity Options can be undone
    /// from User Preferences > Application > Dialogs, and the choice is kept.
    #[test]
    fn copy_activity_options_can_be_shown_again() {
        let path = temp_db("prefs-copy-options");
        let store = crate::store::Store::open(path.clone()).unwrap();
        let ctx = egui::Context::default();
        ctx.set_pixels_per_point(1.);
        let mut app = App::new(&ctx, store, openxer_xer::starter());
        let mut painter = SoftwarePainter::default();
        let mut time = 0.;
        app.editing.options.copy.show_dialog = false;
        app.save_editing_options();
        app.open_user_preferences();
        for _ in 0..2 {
            let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
        }
        painter.click_action(&mut app, &ctx, &mut time, "prefs_tab_application");
        let _ = painter.frame(&mut app, &ctx, &mut time, vec![], false);
        painter.click_action(&mut app, &ctx, &mut time, "prefs_show_copy_options");
        assert!(app.editing.options.copy.show_dialog);
        let stored = app
            .store
            .preference(crate::editing::OPTIONS_KEY)
            .unwrap()
            .unwrap();
        let options: crate::editing::EditingOptions = serde_json::from_str(&stored).unwrap();
        assert!(options.copy.show_dialog, "kept in the workspace");
        drop(app);
        let _ = std::fs::remove_file(path);
    }
}
