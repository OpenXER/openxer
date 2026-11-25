//! Resources view Add with the New Resource wizard, page for page as
//! observed (private observation notes): Resource ID and name, resource
//! type (with the unit of measure of a material), price and units per
//! time, phone and e-mail, roles, the calendar (an existing one, or a new
//! personal calendar), whether actuals are computed, and a last page. Every
//! page has Cancel at the bottom left, Prev, Next and Finish at the right,
//! and a box that turns the wizard off. Only the Resource ID is required:
//! Next from the first page and Finish refuse a blank or used one with a
//! message. Finish on any page adds the resource with what has been chosen
//! so far: the same new resource the plain Add makes (`App::new_resource`,
//! over `views::resource_edit`), then each choice through the same setters
//! the Resources view uses, as one undo step. Cancel adds nothing. The
//! words are OpenXER's own.
//!
//! The switch is User Preferences, Assistance, "Add resources through the
//! wizard" (on unless turned off), kept with the New Activity switch in
//! `openxer_automation::user_prefs::Assistance`. With it off, Add inserts
//! the resource row at once.
use super::*;
use crate::dialog_kit::{self as kit, WizardNav};
use crate::panes::usage_list::{ListFilter, ListKey};
use crate::views::resource_edit as edit;
use crate::views::resources::Pick as ResourcePick;
use openxer_xer::edit::calendars::{self as calendars, Kind};

/// The resource id a dictionary dialog in select mode is given while it
/// chooses for the wizard; no resource has it.
pub(crate) const WIZARD_RSRC: i64 = -1;

/// The dialog's name (help, tests and renders) and size.
pub(crate) const SALT: &str = "new_resource_wizard";
const CLIENT: Vec2 = Vec2::new(520., 372.);
/// Where the page panel ends; the skip box and the buttons are under it.
const PANEL_BOTTOM: f32 = CLIENT.y - 62.;
const FIELD_X: f32 = 30.;
const FIELD_Y: f32 = 196.;

/// Resource ID and name a new resource starts with, in the wizard and with
/// a plain Add (one definition, `views::resource_edit`).
pub use crate::views::resource_edit::{NEW_RESOURCE_ID, NEW_RESOURCE_NAME};

/// One page of the wizard, in the order they come.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Name,
    Type,
    Units,
    Contact,
    Roles,
    Calendar,
    /// The chosen existing calendar, or the personal one to be made.
    CalendarName,
    AutoCompute,
    Done,
}

impl Page {
    pub const ALL: [Page; 9] = [
        Page::Name,
        Page::Type,
        Page::Units,
        Page::Contact,
        Page::Roles,
        Page::Calendar,
        Page::CalendarName,
        Page::AutoCompute,
        Page::Done,
    ];
    fn title(self, personal: bool) -> &'static str {
        match self {
            Page::Name => "ID and Name",
            Page::Type => "Resource Type",
            Page::Units => "Units / Time & Prices",
            Page::Contact => "Phone & E-Mail",
            Page::Roles => "Roles",
            Page::Calendar => "Resource Calendar",
            Page::CalendarName if personal => "Personal Calendar",
            Page::CalendarName => "Select Existing Calendar",
            Page::AutoCompute => "Auto-Compute Actuals",
            Page::Done => "Congratulations",
        }
    }
}

/// The wizard's choices while it is open.
#[derive(Clone, Debug, PartialEq)]
pub struct ResourceWizard {
    pub page: Page,
    /// The parent Add would give the resource.
    pub parent: Option<i64>,
    pub id: String,
    pub name: String,
    /// Stored code, one of `resource_edit::TYPES`.
    pub kind: &'static str,
    /// `unit_id` of a material's unit of measure.
    pub unit: Option<String>,
    /// As typed, in the user's currency and units per time formats.
    pub price: String,
    pub default_units: String,
    pub max_units: String,
    pub phone: String,
    pub email: String,
    /// `role_id` of each role, in the order picked; the first is primary.
    pub roles: Vec<i64>,
    pub row: Option<usize>,
    /// "Create a new personal calendar" is chosen.
    pub personal: bool,
    /// `clndr_id` of the existing calendar.
    pub calendar: String,
    pub auto_compute: bool,
    pub skip: bool,
    /// A refusal waiting for OK, and the page it is about.
    pub message: Option<(Page, String)>,
    pub focus: bool,
}

impl ResourceWizard {
    fn step(&mut self, by: isize) {
        let at = Page::ALL.iter().position(|p| *p == self.page).unwrap_or(0) as isize;
        let to = (at + by).clamp(0, Page::ALL.len() as isize - 1) as usize;
        if Page::ALL[to] != self.page {
            self.go(Page::ALL[to]);
        }
    }
    fn go(&mut self, page: Page) {
        self.page = page;
        self.row = None;
        self.focus = true;
    }
}

/// What Finish would write, read before anything changes.
struct Checked {
    code: String,
    name: String,
    /// Per hour.
    default_units: f64,
    max_units: f64,
    price: f64,
}

/// What the page asked for beyond its own fields.
#[derive(Default)]
struct Asked {
    nav: Option<WizardNav>,
    /// Open a dictionary in select mode for the wizard.
    pick: Option<ResourcePick>,
    view_calendar: bool,
}

impl App {
    /// Turn the New Resource wizard on or off and keep the choice.
    pub(crate) fn set_resource_wizard(&mut self, on: bool) {
        self.activity_wizard.assistance.resource_wizard = on;
        let text = self.activity_wizard.assistance.to_json();
        let key = openxer_automation::user_prefs::ASSISTANCE_KEY;
        if let Err(e) = self.store.set_preference(key, &text) {
            self.error(format!("Cannot save the wizard setting: {e}"));
        }
    }

    /// Open the wizard with the values Add would give: "R", "(New
    /// Resource)", labor, no price, 8 hours a day default and most, the
    /// project calendar, actuals computed.
    pub(crate) fn open_resource_wizard(&mut self) {
        if self.activity_wizard.resource.is_some() {
            return;
        }
        let parent = self.new_resource_parent();
        let pid = self.project.model.project.proj_id;
        let calendar = crate::records::lookup(
            &self.project.document,
            "PROJECT",
            "proj_id",
            pid,
            "clndr_id",
        );
        self.activity_wizard.resource = Some(ResourceWizard {
            page: Page::Name,
            parent,
            // The ID a plain Add would give: "R", or the next free one.
            id: edit::unique_code(&self.project.document, NEW_RESOURCE_ID),
            name: NEW_RESOURCE_NAME.into(),
            kind: "RT_Labor",
            unit: None,
            price: format::cost(0.),
            default_units: format::units_per_time(1.),
            max_units: format::units_per_time(1.),
            phone: String::new(),
            email: String::new(),
            roles: vec![],
            row: None,
            personal: false,
            calendar,
            auto_compute: true,
            skip: false,
            message: None,
            focus: true,
        });
    }

    /// A unit of measure, role or calendar chosen in a dictionary dialog
    /// for the wizard.
    pub(crate) fn resource_wizard_pick(&mut self, pick: ResourcePick, key: String) {
        let Some(w) = self.activity_wizard.resource.as_mut() else {
            return;
        };
        match pick {
            ResourcePick::Unit => w.unit = Some(key),
            ResourcePick::Calendar => w.calendar = key,
            ResourcePick::Role => {
                let role = crate::records::num(&key);
                if w.roles.contains(&role) {
                    self.status = "The resource already has that role.".into();
                } else {
                    w.roles.push(role);
                    w.row = Some(w.roles.len() - 1);
                }
            }
            ResourcePick::Currency | ResourcePick::Code => {}
        }
    }

    /// The Resource ID check that Next on the first page and Finish make.
    fn check_resource_id(&self, w: &ResourceWizard) -> Result<String, (Page, String)> {
        let code = w.id.trim().to_string();
        if code.is_empty() {
            return Err((
                Page::Name,
                "Type a Resource ID for the new resource.".into(),
            ));
        }
        if edit::unique_code(&self.project.document, &code) != code {
            return Err((
                Page::Name,
                format!("Another resource already uses Resource ID {code}. Type a different one."),
            ));
        }
        Ok(code)
    }

    fn check_resource_wizard(&self, w: &ResourceWizard) -> Result<Checked, (Page, String)> {
        let code = self.check_resource_id(w)?;
        let per_time = |text: &str, what: &str| {
            crate::details::assignments::parse_units_per_time(text)
                .filter(|v| *v >= 0.)
                .ok_or((
                    Page::Units,
                    format!("{what} must be units per time, such as 8/d or 100%."),
                ))
        };
        let default_units = per_time(&w.default_units, "Default Units/Time")?;
        let max_units = per_time(&w.max_units, "Max Units/Time")?;
        let price = format::parse_cost(w.price.trim_end_matches("/h"))
            .filter(|v| *v >= 0.)
            .ok_or((Page::Units, "Price/Unit must be an amount.".to_string()))?;
        Ok(Checked {
            code,
            name: w.name.trim().to_string(),
            default_units,
            max_units,
            price,
        })
    }

    /// Finish: add the resource with the wizard's choices as one undo step.
    fn finish_resource_wizard(&mut self, time: f64) -> Result<i64, (Page, String)> {
        let Some(w) = self.activity_wizard.resource.clone() else {
            return Err((Page::Name, "The wizard is not open.".into()));
        };
        let c = self.check_resource_wizard(&w)?;
        let before = self.project.clone();
        let revision = self.records_revision;
        match self.build_wizard_resource(&w, &c) {
            Ok(id) => {
                self.project.refresh_resource_inputs();
                self.edit_step(before, revision, time);
                Ok(id)
            }
            Err(e) => {
                self.project = before;
                self.records_revision = revision;
                Err((w.page, e))
            }
        }
    }

    /// The resource Add makes, then each choice the wizard holds.
    fn build_wizard_resource(&mut self, w: &ResourceWizard, c: &Checked) -> Result<i64, String> {
        let id = self.new_resource(w.parent);
        let number = crate::dialog_kit::format_number;
        let doc = &mut self.project.document;
        edit::set_value(doc, id, "rsrc_short_name", c.code.clone());
        edit::set_value(doc, id, "rsrc_name", c.name.clone());
        edit::set_value(doc, id, "rsrc_type", w.kind.to_string());
        if w.kind == "RT_Mat" {
            if let Some(unit) = &w.unit {
                edit::set_value(doc, id, "unit_id", unit.clone());
            }
        }
        edit::set_value(doc, id, "def_qty_per_hr", number(c.default_units));
        if let Some(&row) = edit::rate_rows(doc, id).first() {
            edit::set_rate_number(doc, row, "max_qty_per_hr", c.max_units)?;
            edit::set_rate_number(doc, row, "cost_per_qty", c.price)?;
        }
        edit::set_value(doc, id, "office_phone", w.phone.trim().to_string());
        edit::set_value(doc, id, "email_addr", w.email.trim().to_string());
        for &role in &w.roles {
            edit::assign_role(doc, id, role)?;
        }
        edit::set_value(
            doc,
            id,
            "auto_compute_act_flag",
            if w.auto_compute { "Y" } else { "N" }.into(),
        );
        let calendar = if w.personal {
            // A personal calendar copied from the chosen one, named after
            // the resource.
            let source = Some(w.calendar.as_str()).filter(|c| !c.is_empty());
            let made = calendars::add(
                &mut self.project,
                Kind::Resource,
                source,
                &calendars::now_stamp(),
            )?;
            calendars::rename(
                &mut self.project,
                &made,
                &format!("{} - {}", c.code, c.name),
            )?;
            made
        } else {
            w.calendar.clone()
        };
        if !calendar.is_empty() {
            edit::set_value(&mut self.project.document, id, "clndr_id", calendar);
        }
        self.project.resource_names.insert(id, c.name.clone());
        self.touch_records();
        Ok(id)
    }

    fn close_resource_wizard(&mut self) {
        if let Some(w) = self.activity_wizard.resource.take() {
            if w.skip {
                self.set_resource_wizard(false);
            }
        }
    }

    /// The New Resource wizard, while open.
    pub(crate) fn resource_wizard_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut w) = self.activity_wizard.resource.take() else {
            return;
        };
        let time = ctx.input(|i| i.time);
        let doc = &self.project.document;
        let roles: Vec<Vec<String>> = w
            .roles
            .iter()
            .enumerate()
            .map(|(k, role)| {
                let name = doc.table("ROLES").and_then(|t| {
                    t.rows
                        .iter()
                        .find(|r| crate::records::num(t.value(r, "role_id")) == *role)
                        .map(|r| t.value(r, "role_name").to_string())
                });
                let primary = if k == 0 { "Yes" } else { "No" };
                vec![name.unwrap_or_default(), "3".into(), primary.into()]
            })
            .collect();
        let unit = w
            .unit
            .as_ref()
            .and_then(|u| {
                doc.table("UMEASURE").and_then(|t| {
                    t.rows
                        .iter()
                        .find(|r| t.value(r, "unit_id") == u)
                        .map(|r| t.value(r, "unit_name").to_string())
                })
            })
            .unwrap_or_default();
        let units_exist = doc.table("UMEASURE").is_some_and(|t| !t.rows.is_empty());
        let calendar_name = if w.personal {
            format!("{} - {}", w.id.trim(), w.name.trim())
        } else {
            calendars::name(doc, &w.calendar)
        };
        let (asked, closed, actions) = kit::window(ctx, SALT, "New Resource", CLIENT, |c| {
            page(c, &mut w, &roles, &unit, units_exist, &calendar_name)
        });
        self.record_dialog_actions(actions);
        if let Some((at, text)) = w.message.clone() {
            let mut rects = vec![];
            let ok = crate::kit_window::message_box(
                ctx,
                "new_resource_message",
                "New Resource",
                crate::kit_window::MessageIcon::Error,
                &text,
                &["OK"],
                Some(0),
                &mut rects,
            );
            self.record_dialog_actions(rects);
            if ok.is_some() || ctx.input(|i| i.key_pressed(Key::Escape)) {
                w.message = None;
                if w.page != at {
                    w.go(at);
                }
            }
            self.activity_wizard.resource = Some(w);
            return;
        }
        match asked.pick {
            Some(ResourcePick::Calendar) => {
                self.open_calendars_to_select(ResourcePick::Calendar, WIZARD_RSRC, &w.calendar)
            }
            Some(pick) => self.views.resources.pick = Some((pick, WIZARD_RSRC)),
            None => {}
        }
        if asked.view_calendar {
            self.open_calendars();
        }
        let nav = match asked.nav {
            None if closed => Some(WizardNav::Cancel),
            nav => nav,
        };
        match nav {
            Some(WizardNav::Back) => w.step(-1),
            // Only the Resource ID is checked on the way.
            Some(WizardNav::Next) => match (w.page, self.check_resource_id(&w)) {
                (Page::Name, Err(refusal)) => w.message = Some(refusal),
                _ => w.step(1),
            },
            Some(WizardNav::Cancel) => {
                self.activity_wizard.resource = Some(w);
                self.close_resource_wizard();
                self.status = "No resource was added.".into();
                return;
            }
            Some(WizardNav::Finish) => {
                self.activity_wizard.resource = Some(w.clone());
                match self.finish_resource_wizard(time) {
                    Ok(id) => {
                        self.close_resource_wizard();
                        let state = &mut self.views.resources;
                        state.set_filter(ListFilter::All);
                        if let Some(p) = w.parent {
                            state.list.collapsed.remove(&ListKey::Resource(p));
                        }
                        state.selected = state.rows.select(&[id]);
                        self.status = format!("Resource {} added", w.id.trim());
                        return;
                    }
                    Err(refusal) => w.message = Some(refusal),
                }
            }
            None => {}
        }
        self.activity_wizard.resource = Some(w);
    }
}

/// The page's explanation, one line per entry.
fn explanation(page: Page, personal: bool) -> &'static [&'static str] {
    match page {
        Page::Name => &[
            "Type the new resource's Resource ID and its name. The Resource ID tells",
            "this resource apart from every other one.",
        ],
        Page::Type => &[
            "Say whether the resource is labor, nonlabor or material.",
            "",
            "Labor and nonlabor resources measure their work in time. A material",
            "measures it in its own unit, chosen under Unit of Measure.",
        ],
        Page::Units => &[
            "Set the resource's price and how much it can work per period.",
            "",
            "Default Units/Time is what a new assignment of the resource starts with,",
            "shown as a percentage or as units per duration. Max Units/Time is the",
            "most it can do per period on all its activities, such as 8 hours a day.",
        ],
        Page::Contact => &["Give a phone number and an e-mail address for the resource."],
        Page::Roles => &[
            "Give the resource its roles.",
            "",
            "A role describes a skill. Activities that ask for a role can then be",
            "staffed with the resources that have it.",
        ],
        Page::Calendar => &[
            "A calendar sets the resource's working hours, weekends, holidays and",
            "time off. Use a calendar that already exists, or make the resource its",
            "own personal calendar.",
        ],
        Page::CalendarName if personal => &[
            "Finish makes a personal calendar for the resource, copied from the",
            "calendar chosen before, with the name below.",
            "",
            "Edit it afterwards in the Calendars dialog.",
        ],
        Page::CalendarName => &[
            "Choose the calendar the resource works to.",
            "",
            "View/Edit opens the Calendars dialog to look at or change calendars.",
        ],
        Page::AutoCompute => &[
            "OpenXER can fill in the resource's actual units for you.",
            "",
            "With this on, the actual units follow the plan: they come from the",
            "activity's percent complete, which saves entering actuals by hand.",
        ],
        Page::Done => &[
            "The new resource is ready. Click Finish to add it and go back to the",
            "Resources view.",
        ],
    }
}

/// Draw the wizard's current page with its buttons.
fn page(
    c: &mut kit::Canvas,
    w: &mut ResourceWizard,
    roles: &[Vec<String>],
    unit: &str,
    units_exist: bool,
    calendar_name: &str,
) -> Asked {
    let mut asked = Asked::default();
    let (width, height) = (CLIENT.x, CLIENT.y);
    kit::wizard_page(c, width, height, PANEL_BOTTOM, w.page.title(w.personal));
    kit::lines(c, 16., 50., 15., explanation(w.page, w.personal), true);
    let focus = std::mem::take(&mut w.focus);
    let label = |c: &kit::Canvas, y: f32, text: &str| kit::text(c, FIELD_X, y, text, true);
    match w.page {
        Page::Name => {
            label(c, FIELD_Y - 12., "Resource ID");
            let id = c.edit(
                "new_resource_id",
                FIELD_X,
                FIELD_Y + 4.,
                110.,
                21.,
                &mut w.id,
                true,
                Align::Min,
            );
            if focus {
                select_all(c, &id, &w.id);
            }
            label(c, FIELD_Y + 34., "Resource Name");
            c.edit(
                "new_resource_name",
                FIELD_X,
                FIELD_Y + 50.,
                286.,
                21.,
                &mut w.name,
                true,
                Align::Min,
            );
        }
        Page::Type => {
            kit::section(c, 16., 126., 482., 178., "Resource Type");
            for (k, (key, text, code)) in [
                ("new_resource_labor", "Labor (people)", "RT_Labor"),
                (
                    "new_resource_nonlabor",
                    "Nonlabor (machines, equipment)",
                    "RT_Equip",
                ),
                ("new_resource_material", "Material", "RT_Mat"),
            ]
            .into_iter()
            .enumerate()
            {
                if c.radio(
                    key,
                    FIELD_X,
                    156. + 22. * k as f32,
                    300.,
                    17.,
                    text,
                    w.kind == code,
                    true,
                ) {
                    w.kind = code;
                }
            }
            label(c, 226., "Unit of Measure");
            let material = w.kind == "RT_Mat";
            if crate::views::sheet::picker_button(
                c,
                "new_resource_unit",
                FIELD_X,
                242.,
                160.,
                Some(chrome::Icon::Resources),
                if material { unit } else { "" },
                material && units_exist,
            ) {
                asked.pick = Some(ResourcePick::Unit);
            }
        }
        Page::Units => {
            label(c, FIELD_Y - 12., "Price/Unit");
            let price = c.edit(
                "new_resource_price",
                FIELD_X,
                FIELD_Y + 4.,
                110.,
                21.,
                &mut w.price,
                true,
                Align::Max,
            );
            if focus {
                select_all(c, &price, &w.price);
            }
            label(c, FIELD_Y + 34., "Default Units/Time");
            c.edit(
                "new_resource_default_units",
                FIELD_X,
                FIELD_Y + 50.,
                110.,
                21.,
                &mut w.default_units,
                true,
                Align::Max,
            );
            kit::text(c, FIELD_X + 136., FIELD_Y + 34., "Max Units/Time", true);
            c.edit(
                "new_resource_max_units",
                FIELD_X + 136.,
                FIELD_Y + 50.,
                110.,
                21.,
                &mut w.max_units,
                true,
                Align::Max,
            );
        }
        Page::Contact => {
            label(c, FIELD_Y, "Office Phone");
            let phone = c.edit(
                "new_resource_phone",
                FIELD_X,
                FIELD_Y + 16.,
                206.,
                21.,
                &mut w.phone,
                true,
                Align::Min,
            );
            if focus {
                select_all(c, &phone, &w.phone);
            }
            label(c, FIELD_Y + 46., "E-Mail Address");
            c.edit(
                "new_resource_email",
                FIELD_X,
                FIELD_Y + 62.,
                206.,
                21.,
                &mut w.email,
                true,
                Align::Min,
            );
        }
        Page::Roles => {
            let cols = [
                kit::Col {
                    title: "Role Name",
                    width: 150.,
                },
                kit::Col {
                    title: "Proficiency",
                    width: 80.,
                },
                kit::Col {
                    title: "Primary Role",
                    width: 80.,
                },
            ];
            let out = kit::list(
                c,
                "new_resource_roles",
                (FIELD_X - 4., 150., 472., 98.),
                None,
                &cols,
                roles.len(),
                w.row,
                None,
                &mut |c, r, col, rect, selected| {
                    let text = roles[r].get(col).map_or("", String::as_str);
                    kit::cell_text(c, rect, text, selected, 3.);
                },
            );
            if let Some(r) = out.clicked {
                w.row = Some(r);
            }
            if kit::button(
                c,
                "new_resource_roles_add",
                FIELD_X - 4.,
                254.,
                84.,
                23.,
                "Assign",
                true,
            ) {
                asked.pick = Some(ResourcePick::Role);
            }
            let selected = w.row.filter(|&r| r < w.roles.len());
            if kit::button(
                c,
                "new_resource_roles_remove",
                FIELD_X + 86.,
                254.,
                84.,
                23.,
                "Remove",
                selected.is_some(),
            ) {
                if let Some(r) = selected {
                    w.roles.remove(r);
                    w.row = None;
                }
            }
        }
        Page::Calendar => {
            for (key, text, personal, y) in [
                (
                    "new_resource_calendar_existing",
                    "Use a calendar that already exists",
                    false,
                    FIELD_Y + 32.,
                ),
                (
                    "new_resource_calendar_personal",
                    "Make a personal calendar for this resource",
                    true,
                    FIELD_Y + 54.,
                ),
            ] {
                if c.radio(
                    key,
                    FIELD_X + 4.,
                    y,
                    360.,
                    17.,
                    text,
                    w.personal == personal,
                    true,
                ) {
                    w.personal = personal;
                }
            }
        }
        Page::CalendarName => {
            label(c, FIELD_Y + 46., "Calendar Name");
            if crate::views::sheet::picker_button(
                c,
                "new_resource_calendar",
                FIELD_X,
                FIELD_Y + 62.,
                180.,
                Some(chrome::Icon::Calendars),
                calendar_name,
                !w.personal,
            ) {
                asked.pick = Some(ResourcePick::Calendar);
            }
            if kit::button(
                c,
                "new_resource_view_calendar",
                FIELD_X + 190.,
                FIELD_Y + 60.,
                108.,
                25.,
                "View/Edit",
                !w.personal,
            ) {
                asked.view_calendar = true;
            }
        }
        Page::AutoCompute => {
            for (key, text, on, y) in [
                (
                    "new_resource_auto_on",
                    "Compute this resource's actuals for me",
                    true,
                    FIELD_Y + 32.,
                ),
                (
                    "new_resource_auto_off",
                    "Leave the actuals to me",
                    false,
                    FIELD_Y + 62.,
                ),
            ] {
                if c.radio(
                    key,
                    FIELD_X + 4.,
                    y,
                    360.,
                    17.,
                    text,
                    w.auto_compute == on,
                    true,
                ) {
                    w.auto_compute = on;
                }
            }
        }
        Page::Done => {}
    }
    c.checkbox(
        "new_resource_skip",
        10.,
        PANEL_BOTTOM + 7.,
        260.,
        17.,
        kit::SKIP_WIZARD,
        &mut w.skip,
        true,
    );
    let at = Page::ALL.iter().position(|p| *p == w.page).unwrap_or(0);
    let last = at + 1 == Page::ALL.len();
    asked.nav = kit::wizard_buttons(c, SALT, width - 5., height - 30., (at > 0, !last, true));
    kit::mark_default(
        c,
        &format!("{SALT}_{}", if last { "finish" } else { "next" }),
    );
    asked
}

/// Give `response`'s edit the keyboard with all of `text` selected.
fn select_all(c: &kit::Canvas, response: &egui::Response, text: &str) {
    response.request_focus();
    let ctx = c.ui.ctx().clone();
    let mut state = egui::TextEdit::load_state(&ctx, response.id).unwrap_or_default();
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::two(
            egui::text::CCursor::new(0),
            egui::text::CCursor::new(text.chars().count()),
        )));
    state.store(&ctx, response.id);
}
