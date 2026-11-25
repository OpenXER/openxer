//! Enterprise structures: the EPS, OBS, Roles and Role Codes, Resource
//! Curves, Resource Shifts, Currencies, Funding Sources, Financial Period
//! Calendars (with Save Period Actuals).
//!
//! The dictionaries live in the project document's XER tables (`ops`), so
//! save, reopen and export carry them and every edit is one undo step through
//! the detail tables. The EPS spans projects and XER files do not carry it,
//! so it lives in the workspace database (`workspace`); each of its edits is
//! one undo step recorded as a workspace change.
use super::*;
use crate::schedule_dialogs::Canvas;
use openxer_xer::Document;

mod dialogs;
mod lists;
pub mod ops;
#[cfg(test)]
mod tests;
pub mod tree;
pub mod ui;
pub mod workspace;

use tree::Result;
use workspace::Workspace;

/// An enterprise dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dialog {
    Eps,
    Roles,
    Obs,
    RoleCodes,
    Shifts,
    Funding,
    Curves,
    Currencies,
    FinCalendars,
}

impl Dialog {
    #[cfg(test)]
    pub const ALL: [Dialog; 9] = [
        Dialog::Eps,
        Dialog::Roles,
        Dialog::Obs,
        Dialog::RoleCodes,
        Dialog::Shifts,
        Dialog::Funding,
        Dialog::Curves,
        Dialog::Currencies,
        Dialog::FinCalendars,
    ];
    /// Menu caption, with the menu's mnemonic.
    pub fn menu_label(self) -> &'static str {
        match self {
            Dialog::Eps => "Project Fol&ders (EPS)...",
            Dialog::Roles => "R&oles...",
            Dialog::Obs => "Responsible &Managers (OBS)...",
            Dialog::RoleCodes => "R&ole Codes...",
            Dialog::Shifts => "R&esource Shifts...",
            Dialog::Funding => "&Funding Sources...",
            Dialog::Curves => "Resource C&urves...",
            Dialog::Currencies => "&Currencies...",
            Dialog::FinCalendars => "&Financial Period Calendars...",
        }
    }
    pub fn icon(self) -> Option<chrome::Icon> {
        use chrome::Icon;
        match self {
            Dialog::Eps => Some(Icon::Eps),
            Dialog::Roles => Some(Icon::Roles),
            Dialog::Obs => Some(Icon::Obs),
            Dialog::RoleCodes => Some(Icon::RoleCodes),
            Dialog::Funding => Some(Icon::FundingSources),
            // The Administrator toolbar's own pictures for these two.
            Dialog::Currencies => Some(Icon::Currencies),
            Dialog::FinCalendars => Some(Icon::Calendars),
            // A shift is a working-time pattern: the calendar picture. A
            // curve spreads units over time: the resource profile picture.
            Dialog::Shifts => Some(Icon::Calendars),
            Dialog::Curves => Some(Icon::ResourceUsageProfile),
        }
    }
    /// Action name of the menu item.
    pub fn action(self) -> &'static str {
        match self {
            Dialog::Eps => "menu_eps",
            Dialog::Roles => "menu_roles",
            Dialog::Obs => "menu_obs",
            Dialog::RoleCodes => "menu_role_codes",
            Dialog::Shifts => "menu_resource_shifts",
            Dialog::Funding => "menu_funding_sources",
            Dialog::Curves => "menu_resource_curves",
            Dialog::Currencies => "menu_currencies",
            Dialog::FinCalendars => "menu_fin_calendars",
        }
    }
    /// The main menu that holds the item.
    #[cfg(test)]
    pub fn menu(self) -> &'static str {
        match self {
            Dialog::Currencies | Dialog::FinCalendars => "Admin",
            _ => "Enterprise",
        }
    }
    /// Window title.
    pub fn title(self) -> &'static str {
        match self {
            Dialog::Eps => "Project Folders (EPS)",
            Dialog::Roles => "Roles",
            Dialog::Obs => "Responsible Managers (OBS)",
            Dialog::RoleCodes => "Role Codes",
            Dialog::Shifts => "Resource Shifts",
            Dialog::Funding => "Funding Sources",
            Dialog::Curves => "Resource Curves",
            Dialog::Currencies => "Currencies",
            Dialog::FinCalendars => "Financial Period Calendars",
        }
    }
    /// Window salt (also the prefix of the dialog's action names).
    #[cfg(test)]
    pub fn salt(self) -> &'static str {
        match self {
            Dialog::Eps => "eps",
            Dialog::Roles => "roles",
            Dialog::Obs => "obs",
            Dialog::RoleCodes => "role_codes",
            Dialog::Shifts => "shifts",
            Dialog::Funding => "funding",
            Dialog::Curves => "curves",
            Dialog::Currencies => "currencies",
            Dialog::FinCalendars => "fin_calendars",
        }
    }
    /// The dialog a Dictionaries toolbar button opens.
    pub fn from_tool(action: &str) -> Option<Self> {
        match action {
            "tool_role_codes" => Some(Dialog::RoleCodes),
            "tool_funding_sources" => Some(Dialog::Funding),
            _ => None,
        }
    }
}

/// Which row a finished edit selects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sel {
    Main(i64),
    Sub(i64),
}

/// The result of an edit: nothing changed, or changed (one undo step).
pub enum Done {
    Nothing,
    Changed(Option<Sel>),
}

pub type DocJob = Box<dyn FnOnce(&mut Document) -> Result<Done>>;
pub type WsJob = Box<dyn FnOnce(&mut Workspace) -> Result<Done>>;

/// An edit waiting to run after the dialogs are drawn.
pub enum Job {
    /// Edits the open project's document.
    Doc(DocJob),
    /// Edits the workspace EPS.
    Ws(WsJob),
    /// Deletes an EPS node with the nodes and projects under it
    /// (`App::delete_folder`).
    DeleteFolder(i64),
    /// Ask first.
    Ask(Confirm),
}

/// An edit of a document tree, run on the project or the workspace.
pub fn tree_job(workspace: bool, f: impl FnOnce(&mut Document) -> Result<Done> + 'static) -> Job {
    if workspace {
        Job::Ws(Box::new(move |ws| f(&mut ws.doc)))
    } else {
        Job::Doc(Box::new(f))
    }
}

/// A question asked before an edit runs.
pub struct Confirm {
    pub title: String,
    pub text: String,
    pub buttons: Vec<&'static str>,
    /// The job each button runs (`None` only closes the box).
    pub then: Vec<Option<Job>>,
}

impl Confirm {
    /// Yes runs the job, No does nothing.
    pub fn yes_no(title: &str, text: String, job: Job) -> Self {
        Self {
            title: title.into(),
            text,
            buttons: vec!["Yes", "No"],
            then: vec![Some(job), None],
        }
    }
}

/// The Edit Resource Curve dialog.
#[derive(Clone, Debug, PartialEq)]
pub struct CurveEditor {
    pub id: i64,
    pub name: String,
    pub points: Vec<String>,
    pub read_only: bool,
}

/// Batch Create settings of the financial period dialog.
#[derive(Clone, Debug, PartialEq)]
pub struct Batch {
    pub from: String,
    pub to: String,
    pub monthly: bool,
    pub weekday: usize,
    pub month_day: usize,
}

impl Default for Batch {
    fn default() -> Self {
        Self {
            from: String::new(),
            to: String::new(),
            monthly: true,
            weekday: 6,
            month_day: 30,
        }
    }
}

/// What the open dialog shows; reset each time one opens.
#[derive(Default)]
pub struct DialogState {
    pub open: Option<Dialog>,
    pub selected: Option<i64>,
    /// Selected row of the second list (periods, shift periods, projects).
    pub sub: Option<i64>,
    /// The code type (Role Codes) shown.
    pub owner: Option<i64>,
    pub tab: usize,
    pub edit: Option<ui::CellEdit>,
    pub sub_edit: Option<ui::CellEdit>,
    pub confirm: Option<Confirm>,
    pub notice: Option<String>,
    pub curve: Option<CurveEditor>,
    /// Role Codes: the code definitions dialog (Modify...).
    pub types_open: bool,
    pub type_selected: Option<i64>,
    pub type_edit: Option<ui::CellEdit>,
    pub batch: Batch,
    /// Select mode (Roles, Currencies): the picker that opened the dialog and
    /// the resource it sets. Select puts the chosen row there and closes.
    pub select: Option<(crate::views::resources::Pick, i64)>,
    /// The row Select chose (its ID).
    pub chosen: Option<i64>,
}

impl DialogState {
    fn busy(&self) -> bool {
        self.confirm.is_some() || self.notice.is_some()
    }
    fn child_open(&self) -> bool {
        self.curve.is_some() || self.types_open
    }
    fn apply(&mut self, sel: Option<Sel>) {
        match sel {
            Some(Sel::Main(id)) => {
                self.selected = Some(id);
                self.edit = None;
            }
            Some(Sel::Sub(id)) => self.sub = Some(id),
            None => {}
        }
    }
}

#[derive(Default)]
pub struct EnterpriseState {
    pub ui: DialogState,
    /// The last Cut or Copy in a tree dialog.
    pub clip: Option<tree::Clip>,
    /// The workspace tables as last read or written.
    pub(crate) cache: Option<Workspace>,
    /// Before and after texts of each workspace change, by undo ID.
    ops: BTreeMap<u64, (String, String)>,
    next_op: u64,
    /// Store Period Performance: the chosen period.
    pub store_period: Option<i64>,
}

/// Undo IDs of workspace changes start here, clear of the baseline ops.
const OP_BASE: u64 = 1 << 40;

impl App {
    /// A menu item that opens an enterprise dialog.
    pub(crate) fn enterprise_item(&mut self, ui: &mut egui::Ui, which: Dialog) {
        let mut item = chrome::Item::new(which.menu_label());
        item.icon = which.icon();
        if self.item(ui, which.action(), item) {
            self.open_enterprise(which);
        }
    }

    /// A Dictionaries toolbar button: enabled when it opens one of these
    /// dialogs.
    pub(crate) fn enterprise_tool(
        &mut self,
        ui: &mut egui::Ui,
        action: &str,
        icon: chrome::Icon,
        tip: &str,
    ) {
        let which = Dialog::from_tool(action);
        if self.tool(ui, action, icon, tip, which.is_some()) {
            if let Some(d) = which {
                self.open_enterprise(d);
            }
        }
    }

    pub(crate) fn open_enterprise(&mut self, which: Dialog) {
        let mut st = DialogState {
            open: Some(which),
            ..Default::default()
        };
        match which {
            Dialog::RoleCodes => {
                st.owner = ops::role_code_types(&self.project.document)
                    .first()
                    .copied();
            }
            Dialog::FinCalendars => {
                st.selected = ops::default_fin_calendar(&self.project.document);
            }
            _ => {}
        }
        self.enterprise.ui = st;
    }

    pub(crate) fn enterprise_open(&self) -> bool {
        self.enterprise.ui.open.is_some()
    }

    // -------------------------------------------------------- workspace

    /// The workspace EPS.
    pub(crate) fn workspace(&mut self) -> &Workspace {
        if self.enterprise.cache.is_none() {
            let text = self
                .store
                .preference(workspace::KEY)
                .ok()
                .flatten()
                .unwrap_or_default();
            self.enterprise.cache = Some(Workspace::from_text(&text));
        }
        self.enterprise.cache.as_ref().unwrap()
    }

    /// Workspace IDs of the stored projects.
    pub(crate) fn workspace_ids(&self) -> Vec<i64> {
        self.store
            .list()
            .unwrap_or_default()
            .into_iter()
            .map(|(id, _)| id)
            .collect()
    }

    /// Save a workspace change and record it as one undo step.
    fn commit_workspace(&mut self, before: String, ws: Workspace, time: f64) {
        let after = ws.to_text();
        if after == before {
            return;
        }
        if let Err(e) = self.store.set_preference(workspace::KEY, &after) {
            self.enterprise.ui.notice = Some(e);
            return;
        }
        self.enterprise.cache = Some(ws);
        self.history
            .record(&self.project, self.records_revision, None, time);
        self.enterprise.next_op += 1;
        let id = OP_BASE + self.enterprise.next_op;
        self.enterprise.ops.insert(id, (before, after));
        self.history
            .record_external(&self.project, self.records_revision, id, time);
    }

    /// Undo (`redo` false) or redo a workspace change; false when the ID is
    /// not one of ours.
    pub(crate) fn enterprise_history(&mut self, id: u64, redo: bool) -> bool {
        let Some((before, after)) = self.enterprise.ops.get(&id).cloned() else {
            return false;
        };
        let text = if redo { after } else { before };
        if let Err(e) = self.store.set_preference(workspace::KEY, &text) {
            self.error(e);
        }
        self.enterprise.cache = Some(Workspace::from_text(&text));
        true
    }

    /// Run a workspace edit as one undo step (used by the Projects view).
    pub(crate) fn run_workspace(&mut self, job: WsJob, time: f64) -> Result<bool> {
        let mut ws = self.workspace().clone();
        let before = ws.to_text();
        match job(&mut ws)? {
            Done::Changed(_) => {
                self.commit_workspace(before, ws, time);
                Ok(true)
            }
            Done::Nothing => Ok(false),
        }
    }

    // ------------------------------------------------------------ jobs

    fn run_jobs(&mut self, jobs: Vec<Job>, time: f64) {
        for job in jobs {
            match job {
                Job::Doc(f) => match f(&mut self.project.document) {
                    Ok(Done::Changed(sel)) => {
                        self.touch_records();
                        self.enterprise.ui.apply(sel);
                    }
                    Ok(Done::Nothing) => {}
                    Err(e) => self.enterprise.ui.notice = Some(e),
                },
                Job::Ws(f) => {
                    let mut ws = self.workspace().clone();
                    let before = ws.to_text();
                    match f(&mut ws) {
                        Ok(Done::Changed(sel)) => {
                            self.commit_workspace(before, ws, time);
                            self.enterprise.ui.apply(sel);
                        }
                        Ok(Done::Nothing) => {}
                        Err(e) => self.enterprise.ui.notice = Some(e),
                    }
                }
                Job::DeleteFolder(id) => {
                    if let Err(e) = self.delete_folder(id, time) {
                        self.enterprise.ui.notice = Some(e);
                    } else if self.enterprise.ui.open.is_some() {
                        self.enterprise.ui.selected = None;
                    }
                }
                Job::Ask(confirm) => self.enterprise.ui.confirm = Some(confirm),
            }
        }
    }

    /// Save Period Actuals (store period performance) into every open
    /// project as one undo step. The active project uses `calendar`; each
    /// other open project uses its own financial period calendar and the
    /// period there with the same start and end. A project without that
    /// period is left alone and named in the status bar.
    pub(crate) fn store_period_now(&mut self, calendar: i64, period: i64, time: f64) {
        let Some(target) = ops::periods(&self.project.document, calendar)
            .into_iter()
            .find(|p| p.id == period)
        else {
            self.error("Choose a financial period.".into());
            return;
        };
        let mut first = true;
        let results = self.edit_every_open_project(
            time,
            |p| {
                let pid = p.model.project.proj_id;
                let name = p.model.project.name.clone();
                let own = if std::mem::take(&mut first) {
                    Some((calendar, period))
                } else {
                    ops::project_fin_calendar(&p.document, pid).and_then(|cal| {
                        ops::periods(&p.document, cal)
                            .into_iter()
                            .find(|q| q.start == target.start && q.end == target.end)
                            .map(|q| (cal, q.id))
                    })
                };
                let Some((cal, id)) = own else {
                    return (name, Err("no matching financial period".to_string()));
                };
                let current = p.updated_document();
                (
                    name,
                    ops::store_period(&mut p.document, &current, pid, cal, id),
                )
            },
            |(_, r)| r.is_ok(),
        );
        let multi = results.len() > 1;
        let stored: usize = results.iter().filter_map(|(_, r)| r.as_ref().ok()).sum();
        let failed: Vec<String> = results
            .iter()
            .filter_map(|(name, r)| r.as_ref().err().map(|e| format!("{name}: {e}")))
            .collect();
        if !multi {
            if let Some(e) = failed.first() {
                let e = e.split_once(": ").map_or(e.as_str(), |(_, e)| e);
                self.error(e.to_string());
                return;
            }
        }
        let projects = results.len() - failed.len();
        self.status = format!(
            "Period performance stored for {stored} activities{}{}",
            if multi {
                format!(" in {projects} projects")
            } else {
                String::new()
            },
            if failed.is_empty() {
                String::new()
            } else {
                format!("; not stored: {}", failed.join("; "))
            }
        );
    }

    // --------------------------------------------------------- drawing

    /// Draw the open enterprise dialog with its message boxes, then run the
    /// edits it asked for.
    pub(crate) fn enterprise_dialogs(&mut self, ctx: &egui::Context) {
        let Some(which) = self.enterprise.ui.open else {
            return;
        };
        let time = ctx.input(|i| i.time);
        let escape = ctx.input(|i| i.key_pressed(Key::Escape));
        let typing = ctx.wants_keyboard_input();
        let mut st = std::mem::take(&mut self.enterprise.ui);
        let mut jobs = vec![];
        let busy = st.busy();
        self.enterprise_messages(ctx, &mut st, &mut jobs, escape);
        let editing = st.edit.is_some() || st.sub_edit.is_some() || st.type_edit.is_some();
        let mut escape = escape && !busy && !typing && !editing;
        if escape && st.child_open() {
            st.curve = None;
            st.types_open = false;
            escape = false;
        }
        let close = match which {
            Dialog::Eps => self.eps_dialog(ctx, &mut st, &mut jobs, busy),
            Dialog::Obs => self.obs_dialog(ctx, &mut st, &mut jobs, busy),
            Dialog::Roles => self.roles_dialog(ctx, &mut st, &mut jobs, busy),
            Dialog::RoleCodes => self.role_codes_dialog(ctx, &mut st, &mut jobs, busy),
            Dialog::Funding => self.funding_dialog(ctx, &mut st, &mut jobs, busy),
            Dialog::Curves => self.curves_dialog(ctx, &mut st, &mut jobs, busy),
            Dialog::Shifts => self.shifts_dialog(ctx, &mut st, &mut jobs, busy),
            Dialog::Currencies => self.currencies_dialog(ctx, &mut st, &mut jobs, busy),
            Dialog::FinCalendars => self.fin_calendars_dialog(ctx, &mut st, &mut jobs, busy),
        };
        // Select mode: the chosen row goes to the resource once the dialog is gone.
        let picked = st.chosen.take().zip(st.select);
        if close || escape || picked.is_some() {
            st.open = None;
        }
        if st.open.is_some() {
            self.enterprise.ui = st;
        } else {
            self.enterprise.ui = DialogState::default();
        }
        self.run_jobs(jobs, time);
        if let Some((id, (pick, rsrc))) = picked {
            self.apply_resource_pick(pick, rsrc, id.to_string());
        }
    }

    fn enterprise_messages(
        &mut self,
        ctx: &egui::Context,
        st: &mut DialogState,
        jobs: &mut Vec<Job>,
        escape: bool,
    ) {
        use crate::dialog_kit::{message_box, MessageIcon};
        let title = st.open.map_or("", |d| d.title());
        let mut actions = vec![];
        if let Some(text) = st.notice.clone() {
            let answer = message_box(
                ctx,
                "enterprise_msg",
                title,
                MessageIcon::Info,
                &text,
                &["OK"],
                Some(0),
                &mut actions,
            );
            self.record_dialog_actions(actions);
            if answer.is_some() || escape {
                st.notice = None;
            }
            return;
        }
        let Some(confirm) = &st.confirm else {
            return;
        };
        let answer = message_box(
            ctx,
            "enterprise_msg",
            &confirm.title,
            MessageIcon::Question,
            &confirm.text,
            &confirm.buttons,
            crate::kit_window::Question::EnterpriseDelete.default_in(&confirm.buttons),
            &mut actions,
        );
        self.record_dialog_actions(actions);
        let answer = match answer {
            Some(a) => a,
            None if escape => confirm.buttons.len() - 1,
            None => return,
        };
        let mut confirm = st.confirm.take().unwrap();
        if let Some(Some(job)) = confirm.then.get_mut(answer).map(Option::take) {
            jobs.push(job);
        }
    }
}
