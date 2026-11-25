//! Edit > Add with the New Activity wizard, page for page as observed:
//! Activity ID and name, WBS, activity type, resources, duration type,
//! units and duration, Dependent Activities (Yes adds a Predecessor and a
//! Successor page), More Details (Yes adds Expenses, Activity Codes and
//! Work Products and Documents pages) and a last page. Every page has
//! Cancel at the bottom left, Prev, Next and Finish at the right, and a box
//! that turns the wizard off. Only the Activity ID is required: Next from
//! the first page and Finish refuse a blank or used one with a message.
//! Finish on any page adds the activity with what has been chosen so far,
//! through the same code Add uses without the wizard
//! (`openxer_xer::edit::new_activity`), and the whole result (the activity,
//! its assignments, relationships, expenses, codes and documents) is one
//! undo step. Cancel adds nothing. The words are OpenXER's own.
//!
//! The switch is User Preferences, Assistance, "Add activities through the
//! wizard" (on unless turned off), kept in the workspace as
//! `openxer_automation::user_prefs::Assistance` so `openxer-cli workspace
//! preference` reads and sets the same row. With it off, Add inserts the
//! row at once and opens its name for typing.
use super::*;
use crate::details::setters::Edit;
use crate::dialog_kit::{self as kit, WizardNav};
use crate::grid::columns::{Measure, Phase};
use openxer_automation::user_prefs::{Assistance, ASSISTANCE_KEY};
use openxer_engine::model::{LinkType, TaskType};
use openxer_xer::edit::activity::{DURATION_TYPES, TASK_TYPES};
use openxer_xer::edit::new_activity::{self as shared, NEW_ACTIVITY_MINUTES};

/// Where Add puts a new activity (see `App::new_activity_place`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NewActivityPlace {
    pub wbs: i64,
    /// The selected activity, which numbers the new one.
    pub selected: Option<usize>,
    /// Code values from a selected band row: (code type, value).
    pub codes: Vec<(i64, i64)>,
}

/// One page of the wizard, in the order they come.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Name,
    Wbs,
    Type,
    Resources,
    DurationType,
    Units,
    Dependent,
    /// Only after "Yes" on [`Page::Dependent`].
    Predecessors,
    /// Only after "Yes" on [`Page::Dependent`].
    Successors,
    MoreDetails,
    /// Only after "Yes" on [`Page::MoreDetails`].
    Expenses,
    /// Only after "Yes" on [`Page::MoreDetails`].
    Codes,
    /// Only after "Yes" on [`Page::MoreDetails`].
    Documents,
    Done,
}

impl Page {
    pub fn title(self) -> &'static str {
        match self {
            Page::Name => "Activity Name",
            Page::Wbs => "Work Breakdown Structure",
            Page::Type => "Activity Type",
            Page::Resources => "Assign Resources",
            Page::DurationType => "Duration Type",
            Page::Units => "Units and Duration",
            Page::Dependent => "Dependent Activities",
            Page::Predecessors => "Predecessor Activities",
            Page::Successors => "Successor Activities",
            Page::MoreDetails => "More Details",
            Page::Expenses => "Expenses",
            Page::Codes => "Activity Codes",
            Page::Documents => "Work Products and Documents",
            Page::Done => "Congratulations",
        }
    }
}

/// Which existing picker the wizard has open for itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    /// Assign Resources.
    Resource,
    /// Assign Predecessors or Assign Successors.
    Relationship,
    /// Assign Activity Codes or Assign Work Products and Documents.
    Record,
}

/// A relationship the wizard will add.
#[derive(Clone, Debug, PartialEq)]
pub struct WizardLink {
    /// `task_id` of the other activity.
    pub task_id: i64,
    /// The other activity is the new one's predecessor.
    pub predecessor: bool,
    pub kind: LinkType,
    /// Minutes.
    pub lag: i64,
}

/// The wizard's choices while it is open.
#[derive(Clone, Debug, PartialEq)]
pub struct Wizard {
    pub page: Page,
    pub place: NewActivityPlace,
    pub id: String,
    pub name: String,
    pub wbs: i64,
    pub task_type: TaskType,
    /// Stored code, one of `DURATION_TYPES`.
    pub duration_type: &'static str,
    /// `rsrc_id` of each resource to assign, in the order picked.
    pub resources: Vec<i64>,
    /// As typed, in the user's units and durations formats.
    pub labor: String,
    pub nonlabor: String,
    pub duration: String,
    /// "Yes" on Dependent Activities.
    pub relationships: bool,
    pub links: Vec<WizardLink>,
    /// "Yes" on More Details.
    pub more_details: bool,
    /// Expense item name and budgeted cost, as typed.
    pub expenses: Vec<(String, String)>,
    /// (code type, code value).
    pub codes: Vec<(i64, i64)>,
    /// `doc_id` of each work product or document.
    pub documents: Vec<i64>,
    /// The selected row of the page's list.
    pub row: Option<usize>,
    /// "Skip this wizard next time" is ticked.
    pub skip: bool,
    /// A refusal waiting for OK, and the page it is about.
    pub message: Option<(Page, String)>,
    pub picking: Option<Pick>,
    /// The page's first field takes the keyboard on its next frame.
    pub focus: bool,
}

impl Wizard {
    /// The pages in order, with the ones the two "Yes" answers add.
    pub fn pages(&self) -> Vec<Page> {
        let mut pages = vec![
            Page::Name,
            Page::Wbs,
            Page::Type,
            Page::Resources,
            Page::DurationType,
            Page::Units,
            Page::Dependent,
        ];
        if self.relationships {
            pages.extend([Page::Predecessors, Page::Successors]);
        }
        pages.push(Page::MoreDetails);
        if self.more_details {
            pages.extend([Page::Expenses, Page::Codes, Page::Documents]);
        }
        pages.push(Page::Done);
        pages
    }
    fn step(&mut self, by: isize) {
        let pages = self.pages();
        let at = pages.iter().position(|p| *p == self.page).unwrap_or(0) as isize;
        let to = (at + by).clamp(0, pages.len() as isize - 1) as usize;
        if pages[to] != self.page {
            self.go(pages[to]);
        }
    }
    fn go(&mut self, page: Page) {
        self.page = page;
        self.row = None;
        self.focus = true;
    }
    fn milestone(&self) -> bool {
        openxer_xer::assignments::is_milestone(self.task_type)
    }
}

/// The Assistance switches and the wizard while it is open.
#[derive(Default)]
pub struct WizardState {
    pub assistance: Assistance,
    pub open: Option<Wizard>,
    /// The New Resource wizard while it is open (`resource_wizard`).
    pub resource: Option<crate::resource_wizard::ResourceWizard>,
}

/// What Finish would do with the wizard's choices, read before anything
/// changes.
struct Checked {
    code: String,
    name: String,
    /// Original Duration in minutes; `None` for a milestone.
    minutes: Option<i64>,
    labor: f64,
    nonlabor: f64,
    /// Expense item names and budgeted costs.
    expenses: Vec<(String, f64)>,
}

/// The rows the list pages show, read from the project each frame.
#[derive(Default)]
struct Lists {
    resources: Vec<Vec<String>>,
    predecessors: Vec<Vec<String>>,
    successors: Vec<Vec<String>>,
    expenses: Vec<Vec<String>>,
    codes: Vec<Vec<String>>,
    documents: Vec<Vec<String>>,
}

/// What a page asked for beyond its own fields.
#[derive(Default)]
struct Asked {
    /// Open this picker (with predecessors for a relationship picker, the
    /// code picker rather than the document picker for a record picker).
    picker: Option<(Pick, bool)>,
    nav: Option<WizardNav>,
}

impl App {
    /// Read the Assistance switches at startup. A test harness starts with
    /// the wizard off unless the workspace says otherwise, so scenarios that
    /// add activities or resources keep the plain Add; the wizards' own
    /// tests turn them on.
    pub(crate) fn restore_assistance(&mut self) {
        let stored = self.store.preference(ASSISTANCE_KEY).ok().flatten();
        self.activity_wizard.assistance = match stored {
            Some(text) => Assistance::from_json(&text),
            None if cfg!(test) => Assistance {
                activity_wizard: false,
                resource_wizard: false,
            },
            None => Assistance::default(),
        };
    }

    /// Turn the New Activity wizard on or off and keep the choice.
    pub(crate) fn set_activity_wizard(&mut self, on: bool) {
        self.activity_wizard.assistance.activity_wizard = on;
        let text = self.activity_wizard.assistance.to_json();
        if let Err(e) = self.store.set_preference(ASSISTANCE_KEY, &text) {
            self.error(format!("Cannot save the wizard setting: {e}"));
        }
    }

    /// Edit > Add, the Add button, Insert and the row menu's Add: the
    /// wizard when it is on, otherwise the row at once.
    pub(crate) fn add_activity_command(&mut self) {
        if self.activity_wizard.open.is_some() {
            return;
        }
        if self.activity_wizard.assistance.activity_wizard {
            self.open_activity_wizard();
        } else {
            self.new_activity();
        }
    }

    /// Open the wizard with the values Add would give: the next Activity ID
    /// from the project's numbering, "New Activity", the WBS Add would use
    /// and the project's Defaults tab types.
    pub(crate) fn open_activity_wizard(&mut self) {
        let place = self.new_activity_place();
        let defaults = shared::defaults(&self.project);
        let periods = format::calendar_periods(&self.project, defaults.calendar);
        let milestone = openxer_xer::assignments::is_milestone(defaults.task_type);
        self.edit_cell = None;
        self.activity_wizard.open = Some(Wizard {
            page: Page::Name,
            id: shared::new_activity_id(&self.project, place.selected),
            name: shared::NEW_ACTIVITY_NAME.into(),
            wbs: place.wbs,
            place,
            task_type: defaults.task_type,
            duration_type: defaults.duration_type,
            resources: vec![],
            labor: format::units(0.),
            nonlabor: format::units(0.),
            duration: format::duration(if milestone { 0 } else { NEW_ACTIVITY_MINUTES }, periods),
            relationships: false,
            links: vec![],
            more_details: false,
            expenses: vec![],
            codes: vec![],
            documents: vec![],
            row: None,
            skip: false,
            message: None,
            picking: None,
            focus: true,
        });
    }

    /// The wizard has `pick`'s picker open for itself: what is picked goes
    /// to the wizard, not to the selected activity.
    pub(crate) fn wizard_picks(&self, pick: Pick) -> bool {
        self.activity_wizard
            .open
            .as_ref()
            .is_some_and(|w| w.picking == Some(pick))
    }

    /// A resource picked in Assign Resources for the wizard.
    pub(crate) fn wizard_take_resource(&mut self, rsrc: i64) {
        let Some(w) = self.activity_wizard.open.as_mut() else {
            return;
        };
        if w.resources.contains(&rsrc) {
            self.status = "That resource is already on the list.".into();
        } else {
            w.resources.push(rsrc);
            w.row = Some(w.resources.len() - 1);
        }
    }

    /// An activity picked in Assign Predecessors or Successors for the
    /// wizard (`other` indexes the open project's activities).
    pub(crate) fn wizard_take_link(
        &mut self,
        other: usize,
        predecessor: bool,
        kind: LinkType,
        lag: i64,
    ) {
        let Some(task_id) = self
            .project
            .model
            .activities
            .get(other)
            .filter(|_| self.active_rows().is_none_or(|n| other < n))
            .map(|a| a.task_id)
        else {
            self.status = "The new activity can be tied to activities of its own project here; add relationships to other projects in Activity Details.".into();
            return;
        };
        let Some(w) = self.activity_wizard.open.as_mut() else {
            return;
        };
        if w.links
            .iter()
            .any(|l| l.task_id == task_id && l.predecessor == predecessor)
        {
            self.status = "That relationship is already on the list.".into();
            return;
        }
        w.links.push(WizardLink {
            task_id,
            predecessor,
            kind,
            lag,
        });
        w.row = Some(
            w.links
                .iter()
                .filter(|l| l.predecessor == predecessor)
                .count()
                - 1,
        );
    }

    /// A code value picked in Assign Activity Codes for the wizard; it
    /// replaces a value of the same code type.
    pub(crate) fn wizard_take_code(&mut self, type_id: i64, value: i64) {
        let Some(w) = self.activity_wizard.open.as_mut() else {
            return;
        };
        match w.codes.iter().position(|(t, _)| *t == type_id) {
            Some(k) => {
                w.codes[k].1 = value;
                w.row = Some(k);
            }
            None => {
                w.codes.push((type_id, value));
                w.row = Some(w.codes.len() - 1);
            }
        }
    }

    /// A document picked in Assign Work Products and Documents for the
    /// wizard.
    pub(crate) fn wizard_take_document(&mut self, doc_id: i64) {
        let Some(w) = self.activity_wizard.open.as_mut() else {
            return;
        };
        if w.documents.contains(&doc_id) {
            self.status = "That document is already on the list.".into();
        } else {
            w.documents.push(doc_id);
            w.row = Some(w.documents.len() - 1);
        }
    }

    /// The Activity ID check that Next on the first page and Finish make.
    fn check_activity_id(&self, w: &Wizard) -> Result<String, (Page, String)> {
        let code = w.id.trim().to_string();
        if code.is_empty() {
            return Err((
                Page::Name,
                "Type an Activity ID for the new activity.".into(),
            ));
        }
        if self
            .project
            .model
            .activities
            .iter()
            .any(|a| a.task_code == code)
        {
            return Err((
                Page::Name,
                format!("Another activity already uses Activity ID {code}. Type a different one."),
            ));
        }
        Ok(code)
    }

    fn check_wizard(&self, w: &Wizard) -> Result<Checked, (Page, String)> {
        let code = self.check_activity_id(w)?;
        let units = |text: &str, what: &str| -> Result<f64, (Page, String)> {
            match crate::grid::columns::parse_units(text) {
                Ok(v) if v >= 0. => Ok(v),
                _ => Err((
                    Page::Units,
                    format!("{what} must be a number of hours, 0 or more."),
                )),
            }
        };
        let labor = units(&w.labor, "High Labor Units")?;
        let nonlabor = units(&w.nonlabor, "High Nonlabor Units")?;
        let minutes = if w.milestone() {
            None
        } else {
            let calendar = shared::defaults(&self.project).calendar;
            let periods = format::calendar_periods(&self.project, calendar);
            match format::parse_duration(&w.duration, periods) {
                Ok(m) if m >= 0 => Some(m),
                Ok(_) => return Err((Page::Units, "High Duration cannot be negative.".into())),
                Err(e) => return Err((Page::Units, e)),
            }
        };
        let mut expenses = vec![];
        for (name, cost) in w.expenses.iter().filter(|_| w.more_details) {
            match format::parse_cost(cost).filter(|v| *v >= 0.) {
                Some(v) => expenses.push((name.trim().to_string(), v)),
                None => {
                    return Err((
                        Page::Expenses,
                        format!("The budgeted cost of {} is not an amount.", name.trim()),
                    ))
                }
            }
        }
        Ok(Checked {
            code,
            name: w.name.trim().to_string(),
            minutes,
            labor,
            nonlabor,
            expenses,
        })
    }

    /// Finish: add the activity with the wizard's choices as one undo step.
    /// A refusal changes nothing and names the page it is about.
    fn finish_activity_wizard(&mut self, time: f64) -> Result<usize, (Page, String)> {
        let Some(w) = self.activity_wizard.open.clone() else {
            return Err((Page::Name, "The wizard is not open.".into()));
        };
        let checked = self.check_wizard(&w)?;
        let before = self.project.clone();
        let before_revision = self.records_revision;
        let selected = self.selected;
        match self.build_wizard_activity(&w, &checked) {
            Ok(i) => {
                self.edit_step(before, before_revision, time);
                Ok(i)
            }
            Err(e) => {
                self.project = before;
                self.records_revision = before_revision;
                self.selected = selected;
                Err((w.page, e))
            }
        }
    }

    /// The activity Add makes, then each choice the wizard holds, through
    /// the setters Activity Details uses.
    fn build_wizard_activity(&mut self, w: &Wizard, c: &Checked) -> Result<usize, String> {
        let place = NewActivityPlace {
            wbs: w.wbs,
            ..w.place.clone()
        };
        let i = self.insert_new_activity(&place);
        openxer_xer::edit::activity::set_activity_id(&mut self.project, i, &c.code)?;
        openxer_xer::edit::activity::set_activity_name(&mut self.project, i, &c.name)?;
        if self.project.model.activities[i].task_type != w.task_type {
            self.edit_activity(i, Edit::ActivityType(w.task_type))?;
        }
        // The duration goes in before the resources: once assignments
        // drive it, some duration types refuse a typed duration.
        if let Some(minutes) = c.minutes {
            self.edit_activity(i, Edit::OriginalDuration(minutes))?;
        }
        self.edit_activity(i, Edit::DurationType(w.duration_type))?;
        let task_id = self.project.model.activities[i].task_id;
        if !w.milestone() {
            for &rsrc in &w.resources {
                self.project.add_assignment_with(
                    task_id,
                    Some(rsrc),
                    None,
                    self.detail.calc.recalculate,
                )?;
                self.touch_records();
            }
        }
        for (measure, value) in [
            (Measure::LaborUnits, c.labor),
            (Measure::NonlaborUnits, c.nonlabor),
        ] {
            if value > 0. {
                self.edit_activity(
                    i,
                    Edit::Figure {
                        measure,
                        phase: Phase::Budgeted,
                        value,
                    },
                )?;
            }
        }
        for l in w.links.iter().filter(|_| w.relationships) {
            let Some(other) = self
                .project
                .model
                .activities
                .iter()
                .position(|a| a.task_id == l.task_id)
            else {
                continue;
            };
            let (pred, succ) = if l.predecessor {
                (other, i)
            } else {
                (i, other)
            };
            crate::relationships::add(&mut self.project.model, pred, succ, l.kind, l.lag)?;
        }
        if w.more_details {
            use openxer_xer::edit::project_records as records;
            for (name, cost) in &c.expenses {
                let id = records::add_expense(&mut self.project, task_id);
                if !name.is_empty() {
                    records::set_expense(&mut self.project, id, "cost_name", name.clone());
                }
                records::set_expense(
                    &mut self.project,
                    id,
                    "target_cost",
                    kit::format_number(*cost),
                );
                self.touch_records();
            }
            for &(type_id, value) in &w.codes {
                self.edit_activity(
                    i,
                    Edit::Code {
                        type_id,
                        value: Some(value),
                    },
                )?;
            }
            for &doc_id in &w.documents {
                openxer_xer::edit::documents::link(&mut self.project, doc_id, Some(task_id), None);
                self.touch_records();
            }
        }
        Ok(i)
    }

    /// Close the wizard; "Skip this wizard next time" turns it off.
    fn close_activity_wizard(&mut self) {
        if let Some(w) = self.activity_wizard.open.take() {
            if w.skip {
                self.set_activity_wizard(false);
            }
        }
    }

    /// The rows of the wizard's list pages.
    fn wizard_lists(&self, w: &Wizard) -> Lists {
        let p = &self.project;
        let activity = |task_id: i64| {
            p.model
                .activities
                .iter()
                .position(|a| a.task_id == task_id)
                .map(|k| {
                    (
                        p.model.activities[k].task_code.clone(),
                        p.name(k).to_string(),
                    )
                })
                .unwrap_or_default()
        };
        let links = |predecessor: bool| {
            w.links
                .iter()
                .filter(|l| l.predecessor == predecessor)
                .map(|l| {
                    let (code, name) = activity(l.task_id);
                    vec![code, name, l.kind.as_str()[3..].to_string()]
                })
                .collect()
        };
        let resources = resource_names(p);
        let pid = p.model.project.proj_id;
        let types = self.code_types(pid);
        let values = p.document.table("ACTVCODE");
        let documents = self.unassigned_documents(0, pid);
        Lists {
            resources: w
                .resources
                .iter()
                .map(|id| {
                    resources
                        .iter()
                        .find(|(r, ..)| r == id)
                        .map_or(vec![], |(_, code, name)| vec![code.clone(), name.clone()])
                })
                .collect(),
            predecessors: links(true),
            successors: links(false),
            expenses: w
                .expenses
                .iter()
                .map(|(name, cost)| vec![name.clone(), cost.clone()])
                .collect(),
            codes: w
                .codes
                .iter()
                .map(|(type_id, value)| {
                    let type_name = types
                        .iter()
                        .find(|(t, _)| t == type_id)
                        .map_or(String::new(), |(_, n)| n.clone());
                    let row = values.and_then(|t| {
                        t.rows
                            .iter()
                            .find(|r| crate::records::num(t.value(r, "actv_code_id")) == *value)
                            .map(|r| {
                                (
                                    t.value(r, "short_name").to_string(),
                                    t.value(r, "actv_code_name").to_string(),
                                )
                            })
                    });
                    let (short, description) = row.unwrap_or_default();
                    vec![type_name, short, description]
                })
                .collect(),
            documents: w
                .documents
                .iter()
                .map(|id| {
                    let title = documents
                        .iter()
                        .find(|(d, _)| d == id)
                        .map_or(String::new(), |(_, c)| c[0].clone());
                    vec![title, "No".into()]
                })
                .collect(),
        }
    }

    /// The New Activity wizard, while open.
    pub(crate) fn activity_wizard_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut w) = self.activity_wizard.open.take() else {
            return;
        };
        // A picker the wizard opened has closed.
        if self.detail.assign.is_none() && self.detail.picker.is_none() {
            w.picking = None;
        }
        // Escape and the close box belong to a picker or a message on top.
        let on_top = w.picking.is_some() || w.message.is_some();
        let time = ctx.input(|i| i.time);
        let lists = self.wizard_lists(&w);
        let wbs_names: Vec<String> = self
            .project
            .wbs
            .iter()
            .map(|wbs| format!("{}  {}", wbs.code, wbs.name))
            .collect();
        let wbs_ids: Vec<i64> = self.project.wbs.iter().map(|wbs| wbs.id).collect();
        let (asked, closed, actions) = kit::window(ctx, SALT, "New Activity", CLIENT, |c| {
            page(c, &mut w, &lists, &wbs_names, &wbs_ids)
        });
        self.record_dialog_actions(actions);
        // A refusal shows in a message box; OK goes to its page.
        if let Some((at, text)) = w.message.clone() {
            let mut rects = vec![];
            let ok = crate::kit_window::message_box(
                ctx,
                "new_activity_message",
                "New Activity",
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
            self.activity_wizard.open = Some(w);
            return;
        }
        if let Some((pick, flag)) = asked.picker {
            w.picking = Some(pick);
            match pick {
                Pick::Resource => {
                    self.detail.new_resource = None;
                    self.detail.assign = Some(details::assignments::AssignPicker::Resource {
                        query: String::new(),
                        selected: None,
                    });
                }
                Pick::Relationship => {
                    self.detail.picker = Some(details::Picker::Relationship {
                        predecessor: flag,
                        query: String::new(),
                        selected: None,
                        kind: LinkType::FS,
                        lag: 0.,
                    });
                }
                Pick::Record if flag => {
                    self.detail.picker = Some(details::Picker::Code {
                        type_id: None,
                        selected: None,
                    });
                }
                Pick::Record => {
                    self.detail.picker = Some(details::Picker::Document { selected: None });
                }
            }
        }
        let nav = match asked.nav {
            None if closed && !on_top => Some(WizardNav::Cancel),
            nav => nav,
        };
        match nav {
            Some(WizardNav::Back) => w.step(-1),
            // Only the Activity ID is checked on the way: the rest waits
            // for Finish.
            Some(WizardNav::Next) => match (w.page, self.check_activity_id(&w)) {
                (Page::Name, Err(refusal)) => w.message = Some(refusal),
                _ => w.step(1),
            },
            Some(WizardNav::Cancel) => {
                self.activity_wizard.open = Some(w);
                self.close_wizard_pickers();
                self.close_activity_wizard();
                self.status = "No activity was added.".into();
                return;
            }
            Some(WizardNav::Finish) => {
                self.activity_wizard.open = Some(w.clone());
                match self.finish_activity_wizard(time) {
                    Ok(i) => {
                        self.close_wizard_pickers();
                        self.close_activity_wizard();
                        self.selected = Some(i);
                        self.reveal_activity(i);
                        self.status = format!(
                            "Activity {} added",
                            self.project.model.activities[i].task_code
                        );
                        return;
                    }
                    Err(refusal) => w.message = Some(refusal),
                }
            }
            None => {}
        }
        self.activity_wizard.open = Some(w);
    }

    /// Close a picker the wizard opened for itself.
    fn close_wizard_pickers(&mut self) {
        match self.activity_wizard.open.as_ref().and_then(|w| w.picking) {
            Some(Pick::Resource) => self.detail.assign = None,
            Some(Pick::Relationship | Pick::Record) => self.detail.picker = None,
            None => {}
        }
    }
}

/// The dialog's name (help, tests and renders) and size.
pub(crate) const SALT: &str = "new_activity_wizard";
const CLIENT: Vec2 = Vec2::new(520., 372.);
/// Where the page panel ends; the skip box and the buttons are under it.
const PANEL_BOTTOM: f32 = CLIENT.y - 62.;
/// Left edge and top of a page's fields.
const FIELD_X: f32 = 30.;
const FIELD_Y: f32 = 196.;

/// (`rsrc_id`, Resource ID, Resource Name) of every resource.
fn resource_names(p: &ProjectData) -> Vec<(i64, String, String)> {
    p.document.table("RSRC").map_or(vec![], |t| {
        t.rows
            .iter()
            .map(|r| {
                (
                    crate::records::num(t.value(r, "rsrc_id")),
                    t.value(r, "rsrc_short_name").to_string(),
                    t.value(r, "rsrc_name").to_string(),
                )
            })
            .filter(|(id, ..)| *id != 0)
            .collect()
    })
}

/// The page's explanation, one line per entry.
fn explanation(page: Page, milestone: bool) -> &'static [&'static str] {
    match page {
        Page::Name => &[
            "Type the new activity's Activity ID and its name.",
            "The Activity ID tells this activity apart from every other one.",
        ],
        Page::Wbs => &[
            "Choose the WBS element the activity belongs to.",
            "",
            "The WBS splits the project into the parts it delivers. Each element",
            "has a responsible manager, who answers for the activities under it.",
        ],
        Page::Type => &[
            "Choose how the activity is scheduled.",
            "",
            "Task Dependent suits most work. Resource Dependent lets the resources'",
            "calendars decide when the work is done. A milestone has no duration and",
            "marks an event. Level of Effort takes its dates from the activities it is",
            "tied to, for support work such as supervision.",
        ],
        Page::Resources if milestone => &[
            "A milestone has no resource assignments.",
            "Go on to the next page, or go back and choose another activity type.",
        ],
        Page::Resources => &["Pick the resources who will do this activity's work."],
        Page::DurationType => &[
            "Choose what stays the same when resources or units change.",
            "",
            "Fixed Duration & Units/Time and Fixed Duration & Units keep the duration",
            "whatever resources are added; with Units the units hold as well. Fixed",
            "Units finishes sooner with more resources, as the total work stays the",
            "same. Fixed Units/Time keeps each resource's output per period.",
        ],
        Page::Units => &[
            "Give the high estimate of the activity's labor and nonlabor units, the",
            "total work in hours, and of its duration, counted in working time on",
            "the activity's calendar.",
        ],
        Page::Dependent => &["Do you want to add the activities this one is tied to now?"],
        Page::Predecessors => &[
            "Pick the activities that come before this one.",
            "",
            "A predecessor comes first in the logic: as a rule this activity waits",
            "for its predecessors to finish before it can start.",
        ],
        Page::Successors => &[
            "Pick the activities that come after this one.",
            "",
            "A successor follows in the logic: as a rule it waits for this activity",
            "to finish before it can start.",
        ],
        Page::MoreDetails => &[
            "Do you want to give the activity more details now?",
            "",
            "The next pages take:",
            "- Expenses",
            "- Activity Codes",
            "- Work Products and Documents",
        ],
        Page::Expenses => &[
            "Add the activity's expenses.",
            "",
            "An expense is a one-off cost that no resource carries, such as",
            "materials, site facilities, travel, overheads or training.",
        ],
        Page::Codes => &[
            "Give the activity its activity code values.",
            "",
            "Activity codes sort and group activities the way your organization and",
            "your project need.",
        ],
        Page::Documents => &[
            "Attach work products and documents to the activity.",
            "",
            "These are the standards and guidance the work follows, or what the",
            "activity delivers, such as design documents and test plans.",
        ],
        Page::Done => &[
            "The new activity is ready. Click Finish to add it; you can change any of",
            "its values later in the activity table.",
        ],
    }
}

/// A list page: the list, then two buttons under it (the first adds, the
/// second takes the selected row off). Returns which button was clicked.
fn list_page(
    c: &mut kit::Canvas,
    key: &str,
    cols: &[(&str, f32)],
    rows: &[Vec<String>],
    row: &mut Option<usize>,
    (add, add_enabled): (&str, bool),
    remove: &str,
) -> (bool, bool) {
    let cols: Vec<kit::Col> = cols
        .iter()
        .map(|(title, width)| kit::Col {
            title,
            width: *width,
        })
        .collect();
    let out = kit::list(
        c,
        key,
        (FIELD_X - 4., 150., 472., 98.),
        None,
        &cols,
        rows.len(),
        *row,
        None,
        &mut |c, r, col, rect, selected| {
            let text = rows[r].get(col).map_or("", String::as_str);
            kit::cell_text(c, rect, text, selected, 3.);
        },
    );
    if let Some(r) = out.clicked {
        *row = Some(r);
    }
    let added = kit::button(
        c,
        &format!("{key}_add"),
        FIELD_X - 4.,
        254.,
        84.,
        23.,
        add,
        add_enabled,
    );
    let selected = row.filter(|&r| r < rows.len());
    let removed = kit::button(
        c,
        &format!("{key}_remove"),
        FIELD_X + 86.,
        254.,
        84.,
        23.,
        remove,
        selected.is_some(),
    );
    (added, removed && selected.is_some())
}

/// Draw the wizard's current page with its buttons.
fn page(
    c: &mut kit::Canvas,
    w: &mut Wizard,
    lists: &Lists,
    wbs_names: &[String],
    wbs_ids: &[i64],
) -> Asked {
    let mut asked = Asked::default();
    let (width, height) = (CLIENT.x, CLIENT.y);
    kit::wizard_page(c, width, height, PANEL_BOTTOM, w.page.title());
    kit::lines(c, 16., 50., 15., explanation(w.page, w.milestone()), true);
    let focus = std::mem::take(&mut w.focus);
    let label = |c: &kit::Canvas, y: f32, text: &str| kit::text(c, FIELD_X, y, text, true);
    match w.page {
        Page::Name => {
            label(c, FIELD_Y, "Activity ID");
            let id = c.edit(
                "new_activity_id",
                FIELD_X,
                FIELD_Y + 16.,
                110.,
                21.,
                &mut w.id,
                true,
                Align::Min,
            );
            if focus {
                select_all(c, &id, &w.id);
            }
            label(c, FIELD_Y + 46., "Activity Name");
            c.edit(
                "new_activity_name",
                FIELD_X,
                FIELD_Y + 62.,
                286.,
                21.,
                &mut w.name,
                true,
                Align::Min,
            );
        }
        Page::Wbs => {
            label(c, FIELD_Y + 46., "WBS");
            let rect = c.r(FIELD_X, FIELD_Y + 62., 250., 21.);
            let id = c.id("new_activity_wbs");
            let (response, picked) = crate::details::form::picker(
                c.ui,
                id,
                rect,
                chrome::Icon::Wbs,
                wbs_names,
                wbs_ids.iter().position(|id| *id == w.wbs),
                true,
            );
            c.act("new_activity_wbs", response.rect);
            if let Some(n) = picked {
                w.wbs = wbs_ids[n];
            }
        }
        Page::Type => {
            label(c, FIELD_Y + 46., "Activity Type");
            let items: Vec<(&str, bool)> = TASK_TYPES
                .iter()
                .map(|t| (openxer_xer::type_label(*t), true))
                .collect();
            let mut index = TASK_TYPES
                .iter()
                .position(|t| *t == w.task_type)
                .unwrap_or(0);
            if c.combo(
                "new_activity_type",
                FIELD_X,
                FIELD_Y + 62.,
                142.,
                21.,
                &items,
                &mut index,
                true,
            ) {
                w.task_type = TASK_TYPES[index];
            }
        }
        Page::Resources => {
            let enabled = !w.milestone();
            let (add, remove) = list_page(
                c,
                "new_activity_resources",
                &[("Resource ID", 100.), ("Resource Name", 300.)],
                &lists.resources,
                &mut w.row,
                ("Assign", enabled),
                "Remove",
            );
            if add {
                asked.picker = Some((Pick::Resource, false));
            }
            if let (true, Some(r)) = (remove, w.row) {
                w.resources.remove(r);
                w.row = None;
            }
        }
        Page::DurationType => {
            label(c, FIELD_Y + 46., "Duration Type");
            let items: Vec<(&str, bool)> = DURATION_TYPES.iter().map(|(_, l)| (*l, true)).collect();
            let mut index = DURATION_TYPES
                .iter()
                .position(|(code, _)| *code == w.duration_type)
                .unwrap_or(0);
            if c.combo(
                "new_activity_duration_type",
                FIELD_X,
                FIELD_Y + 62.,
                172.,
                21.,
                &items,
                &mut index,
                true,
            ) {
                w.duration_type = DURATION_TYPES[index].0;
            }
        }
        Page::Units => {
            let milestone = w.milestone();
            for (k, (key, caption, text, enabled)) in [
                ("new_activity_labor", "High Labor Units", &mut w.labor, true),
                (
                    "new_activity_nonlabor",
                    "High Nonlabor Units",
                    &mut w.nonlabor,
                    true,
                ),
                (
                    "new_activity_duration",
                    "High Duration",
                    &mut w.duration,
                    !milestone,
                ),
            ]
            .into_iter()
            .enumerate()
            {
                let x = FIELD_X + 126. * k as f32;
                kit::text(c, x, FIELD_Y + 46., caption, true);
                let response = c.edit(key, x, FIELD_Y + 62., 66., 21., text, enabled, Align::Max);
                if focus && k == 0 {
                    select_all(c, &response, text);
                }
            }
        }
        Page::Dependent | Page::MoreDetails => {
            let (key, yes_text, no_text, value) = if w.page == Page::Dependent {
                (
                    "new_activity_relationships",
                    "Yes, add predecessors and successors now",
                    "No, go on",
                    &mut w.relationships,
                )
            } else {
                (
                    "new_activity_more",
                    "Yes, go on to these details",
                    "No, not now",
                    &mut w.more_details,
                )
            };
            if c.radio(
                &format!("{key}_on"),
                FIELD_X + 4.,
                FIELD_Y + 32.,
                360.,
                17.,
                yes_text,
                *value,
                true,
            ) {
                *value = true;
            }
            if c.radio(
                &format!("{key}_off"),
                FIELD_X + 4.,
                FIELD_Y + 62.,
                360.,
                17.,
                no_text,
                !*value,
                true,
            ) {
                *value = false;
            }
        }
        Page::Predecessors | Page::Successors => {
            let predecessor = w.page == Page::Predecessors;
            let (key, rows) = if predecessor {
                ("new_activity_predecessors", &lists.predecessors)
            } else {
                ("new_activity_successors", &lists.successors)
            };
            let (add, remove) = list_page(
                c,
                key,
                &[
                    ("Activity ID", 100.),
                    ("Activity Name", 220.),
                    ("Relationship Type", 110.),
                ],
                rows,
                &mut w.row,
                ("Assign", true),
                "Remove",
            );
            if add {
                asked.picker = Some((Pick::Relationship, predecessor));
            }
            if let (true, Some(r)) = (remove, w.row) {
                let at = w
                    .links
                    .iter()
                    .enumerate()
                    .filter(|(_, l)| l.predecessor == predecessor)
                    .nth(r)
                    .map(|(k, _)| k);
                if let Some(k) = at {
                    w.links.remove(k);
                }
                w.row = None;
            }
        }
        Page::Expenses => {
            let (add, remove) = list_page(
                c,
                "new_activity_expenses",
                &[("Expense Item", 260.), ("Budgeted Cost", 120.)],
                &lists.expenses,
                &mut w.row,
                ("Add", true),
                "Delete",
            );
            if add {
                w.expenses
                    .push(("(New Expense Item)".into(), format::cost(0.)));
                w.row = Some(w.expenses.len() - 1);
            }
            if let (true, Some(r)) = (remove, w.row) {
                w.expenses.remove(r);
                w.row = None;
            }
            // The selected expense's name and cost are typed beside the
            // buttons.
            if let Some((name, cost)) = w.row.and_then(|r| w.expenses.get_mut(r)) {
                c.edit(
                    "new_activity_expense_name",
                    FIELD_X + 176.,
                    255.,
                    180.,
                    21.,
                    name,
                    true,
                    Align::Min,
                );
                c.edit(
                    "new_activity_expense_cost",
                    FIELD_X + 362.,
                    255.,
                    84.,
                    21.,
                    cost,
                    true,
                    Align::Max,
                );
            }
        }
        Page::Codes => {
            let (add, remove) = list_page(
                c,
                "new_activity_codes",
                &[
                    ("Activity Code", 120.),
                    ("Activity Code Value", 120.),
                    ("Description", 200.),
                ],
                &lists.codes,
                &mut w.row,
                ("Assign", true),
                "Remove",
            );
            if add {
                asked.picker = Some((Pick::Record, true));
            }
            if let (true, Some(r)) = (remove, w.row) {
                w.codes.remove(r);
                w.row = None;
            }
        }
        Page::Documents => {
            let (add, remove) = list_page(
                c,
                "new_activity_documents",
                &[("Title", 300.), ("Work Product", 100.)],
                &lists.documents,
                &mut w.row,
                ("Assign", true),
                "Remove",
            );
            if add {
                asked.picker = Some((Pick::Record, false));
            }
            if let (true, Some(r)) = (remove, w.row) {
                w.documents.remove(r);
                w.row = None;
            }
        }
        Page::Done => {}
    }
    c.checkbox(
        "new_activity_skip",
        10.,
        PANEL_BOTTOM + 7.,
        260.,
        17.,
        kit::SKIP_WIZARD,
        &mut w.skip,
        true,
    );
    let pages = w.pages();
    let at = pages.iter().position(|p| *p == w.page).unwrap_or(0);
    let last = at + 1 == pages.len();
    asked.nav = kit::wizard_buttons(c, SALT, width - 5., height - 30., (at > 0, !last, true));
    // Enter goes on to the next page, and finishes on the last.
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
