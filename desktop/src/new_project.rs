//! File > New (Ctrl+N): a one-page dialog that makes an empty project the way
//! the Projects view Add does, with its EPS node, Project ID and name, dates
//! and responsible manager chosen first. Finish adds the project to the
//! workspace as one undo step and opens it; Cancel adds nothing. The pages
//! are OpenXER's own.
use super::*;

use openxer_xer::workspace::new_project::{self as shared, FINISH_HOUR, START_HOUR};
pub use openxer_xer::workspace::new_project::{build, Spec, MAX_ID, MAX_NAME};

/// What the New Project dialog collects.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Dialog {
    /// EPS node the project goes under.
    pub eps: i64,
    pub project_id: String,
    pub name: String,
    /// Dates as typed, in the user's date format.
    pub planned_start: String,
    pub must_finish: String,
    /// `obs_id` of the responsible manager; empty for none.
    pub manager: String,
    /// Default rate type of the project's new assignments.
    pub rate_type: String,
    pub error: Option<String>,
    /// The field error is about (its action name), so the message shows
    /// under it; None shows it under all the fields.
    pub error_field: Option<&'static str>,
}

/// The field a refusal from [Dialog::check] is about.
fn field_of(error: &str) -> Option<&'static str> {
    let starts = |prefixes: &[&str]| prefixes.iter().any(|p| error.starts_with(p));
    if starts(&[
        "Enter a Project ID",
        "The Project ID",
        "A project with the ID",
    ]) {
        Some("new_project_id")
    } else if starts(&["The Project Name"]) {
        Some("new_project_name")
    } else if starts(&["Choose an EPS"]) {
        Some("new_project_eps")
    } else if starts(&["Enter a Planned Start"]) {
        Some("new_project_start")
    } else if starts(&["Must Finish By"]) {
        Some("new_project_finish_by")
    } else if starts(&["Unknown rate type"]) {
        Some("new_project_rate_type")
    } else {
        None
    }
}

/// The model origin every new project starts from.
fn origin() -> i64 {
    shared::origin()
}

fn minutes_at(origin: i64, day: chrono::NaiveDate, hour: i64) -> i64 {
    let epoch = chrono::NaiveDate::from_ymd_opt(1899, 12, 30).unwrap();
    (day - epoch).num_days() * 1440 + hour * 60 - origin * DAY
}

impl Dialog {
    /// Starting values: the first free `NEW-n` as the ID, today as the
    /// start, the manager the open project uses, if any, and Price / Unit.
    pub fn new(eps: i64, taken: &[String], manager: String) -> Self {
        let id = (1..)
            .map(|n| format!("NEW-{n}"))
            .find(|c| !taken.iter().any(|t| t.eq_ignore_ascii_case(c)))
            .expect("a free Project ID");
        let today = chrono::Local::now().date_naive();
        let start = minutes_at(origin(), today, START_HOUR);
        Self {
            eps,
            project_id: id,
            name: String::new(),
            planned_start: format::date(origin(), Some(start)),
            must_finish: String::new(),
            manager,
            rate_type: shared::DEFAULT_RATE_TYPE.into(),
            error: None,
            error_field: None,
        }
    }

    /// Checks the entry against the workspace's Project IDs and EPS nodes,
    /// with the rules `openxer-cli eps create-project` uses.
    pub fn check(&self, taken: &[String], eps_nodes: &[i64]) -> Result<Spec, String> {
        self.check_at(taken, eps_nodes).map_err(|(_, e)| e)
    }

    /// [Dialog::check], naming the field a refusal is about.
    fn check_at(
        &self,
        taken: &[String],
        eps_nodes: &[i64],
    ) -> Result<Spec, (Option<&'static str>, String)> {
        let at_field = |e: String| (field_of(&e), e);
        let o = origin();
        // Only the time of day of these is used.
        let at = |hour: i64| hour * 60 - o * DAY;
        let start = format::parse_date(o, &self.planned_start, Some(at(START_HOUR)));
        let finish = format::parse_date(o, &self.must_finish, Some(at(FINISH_HOUR)));
        let entry = shared::Entry {
            eps: self.eps,
            project_id: &self.project_id,
            name: &self.name,
            planned_start: start.clone().ok().flatten(),
            must_finish: finish.clone().ok().flatten(),
            manager: &self.manager,
            rate_type: &self.rate_type,
        };
        // The name and folder checks come first, then the dates as typed.
        let checked = shared::check(
            &shared::Entry {
                planned_start: Some(0),
                must_finish: None,
                ..entry.clone()
            },
            taken,
            eps_nodes,
        );
        checked.map_err(at_field)?;
        start.map_err(|e| (Some("new_project_start"), e))?;
        finish.map_err(|e| (Some("new_project_finish_by"), e))?;
        shared::check(&entry, taken, eps_nodes).map_err(at_field)
    }

    /// Keep a refusal and the field it is about.
    fn refuse(&mut self, error: String, field: Option<&'static str>) {
        self.error_field = field.or_else(|| field_of(&error));
        self.error = Some(error);
    }
}
impl App {
    /// File > New and Ctrl+N: open the New Project dialog.
    pub(crate) fn open_new_project(&mut self) {
        let ws = self.workspace().clone();
        let nodes = ws.eps();
        let chosen = self
            .views
            .projects
            .selected_eps
            .filter(|id| nodes.iter().any(|n| n.id == *id))
            .unwrap_or_else(|| ws.node_of(self.open_set.active));
        self.open_new_project_in(chosen);
    }

    /// The New Project dialog with folder `eps` chosen (the Projects view Add).
    pub(crate) fn open_new_project_in(&mut self, chosen: i64) {
        let manager = views::wbs::project_node(&self.project)
            .map(|n| views::wbs::projwbs_value(&self.project, n, "obs_id"))
            .unwrap_or_default();
        let taken = self.taken_project_ids();
        self.new_project = Some(Dialog::new(chosen, &taken, manager));
    }

    /// Adds the project the New Project dialog describes to the workspace, as one undo
    /// step, without opening it.
    pub(crate) fn create_wizard_project(&mut self, spec: &Spec, time: f64) -> Result<i64, String> {
        let obs = self.project.document.table("OBS").cloned();
        let number = self.free_project_number()?;
        let p = build(spec, obs.as_ref(), number)?;
        self.insert_workspace_project(p, Some(spec.eps), time)
    }

    /// Finish: adds the project and opens it.
    pub(crate) fn finish_new_project(&mut self, time: f64) -> Result<(), String> {
        let Some(d) = self.new_project.clone() else {
            return Ok(());
        };
        let taken = self.taken_project_ids();
        let nodes: Vec<i64> = self.workspace().eps().iter().map(|n| n.id).collect();
        let spec = d.check(&taken, &nodes)?;
        let from = self.open_set.active;
        let id = self.create_wizard_project(&spec, time)?;
        let step = self.history.undo_external();
        let project = self.store.load(id)?;
        self.load(project);
        // The step that added the project also heads its history, so
        // Ctrl+Z here removes it (session_history).
        if let (Some(step), Some(from), Some(true)) =
            (step, from, self.open_set.active.map(|a| a == id))
        {
            self.adopt_create_step(step, from, time);
        }
        self.show_view(views::MainView::Activities);
        self.status = format!("Project {} created", spec.project_id);
        Ok(())
    }

    /// New Project: a kit dialog with the fields on the left and Finish and
    /// Cancel (and Help) in the button column. The Project Name field has the keyboard
    /// when it opens; Enter runs Finish and Escape cancels. A refusal shows
    /// in red under the field it is about.
    pub(crate) fn new_project_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.new_project.take() else {
            return;
        };
        let time = ctx.input(|i| i.time);
        let ws = self.workspace().clone();
        let eps: Vec<(i64, usize)> = ws.eps().iter().map(|n| (n.id, n.depth)).collect();
        let managers = views::wbs::obs_names(&self.project);
        use crate::dialog_kit::{self as kit, Entry};
        const W: f32 = 552.;
        const H: f32 = 252.;
        let window = egui::Id::new(("dialog_window", "New Project"));
        // The first frame the dialog shows.
        let opening = !ctx.memory(|m| {
            m.areas()
                .is_visible(&egui::LayerId::new(egui::Order::Middle, window))
        });
        let eps_names: Vec<String> = eps
            .iter()
            .map(|(id, depth)| {
                format!("{}{}", "    ".repeat(*depth), ws.eps_value(*id, "eps_name"))
            })
            .collect();
        let mut manager_names = vec![String::new()];
        manager_names.extend(managers.iter().map(|(_, n)| n.clone()));
        let rates = openxer_xer::assignments::RATE_TYPES;
        let (clicked, closed, actions) =
            kit::window(ctx, "New Project", "New Project", Vec2::new(W, H), |c| {
                let mut y = 8.;
                // A refusal shows in red under the field it is about.
                let error = |c: &mut kit::Canvas, y: &mut f32, d: &Dialog, key: Option<&str>| {
                    if let Some(e) = d.error.as_deref().filter(|_| d.error_field == key) {
                        kit::error_text(c, 130., *y - 6., 300., e);
                        *y += 16.;
                    }
                };
                let label =
                    |c: &kit::Canvas, y: f32, text: &str| kit::text(c, 8., y + 4., text, true);
                label(c, y, "EPS");
                let items: Vec<(&str, bool)> =
                    eps_names.iter().map(|n| (n.as_str(), true)).collect();
                let mut index = eps.iter().position(|(id, _)| *id == d.eps).unwrap_or(0);
                if c.combo(
                    "new_project_eps",
                    130.,
                    y,
                    300.,
                    21.,
                    &items,
                    &mut index,
                    true,
                ) {
                    if let Some((id, _)) = eps.get(index) {
                        d.eps = *id;
                    }
                }
                y += 30.;
                error(c, &mut y, &d, Some("new_project_eps"));
                for (key, caption, width, limit) in [
                    ("new_project_id", "Project ID", 140., Some(MAX_ID)),
                    ("new_project_name", "Project Name", 300., Some(MAX_NAME)),
                    ("new_project_start", "Planned Start", 140., None),
                    ("new_project_finish_by", "Must Finish By", 140., None),
                ] {
                    label(c, y, caption);
                    let text = match key {
                        "new_project_id" => &mut d.project_id,
                        "new_project_name" => &mut d.name,
                        "new_project_start" => &mut d.planned_start,
                        _ => &mut d.must_finish,
                    };
                    let r = c.edit(key, 130., y, width, 21., text, true, Align::Min);
                    if let Some(limit) = limit {
                        if text.chars().count() > limit {
                            *text = text.chars().take(limit).collect();
                        }
                    }
                    if opening && key == "new_project_name" {
                        r.request_focus();
                    }
                    y += 30.;
                    error(c, &mut y, &d, Some(key));
                }
                label(c, y, "Responsible Manager");
                let items: Vec<(&str, bool)> =
                    manager_names.iter().map(|n| (n.as_str(), true)).collect();
                let mut index = managers
                    .iter()
                    .position(|(k, _)| *k == d.manager)
                    .map_or(0, |i| i + 1);
                if c.combo(
                    "new_project_manager",
                    130.,
                    y,
                    300.,
                    21.,
                    &items,
                    &mut index,
                    true,
                ) {
                    d.manager = index
                        .checked_sub(1)
                        .and_then(|i| managers.get(i))
                        .map(|(k, _)| k.clone())
                        .unwrap_or_default();
                }
                y += 30.;
                label(c, y, "Default Rate Type");
                let items: Vec<(&str, bool)> = rates.iter().map(|(_, l, _)| (*l, true)).collect();
                let mut index = rates
                    .iter()
                    .position(|(code, _, _)| *code == d.rate_type)
                    .unwrap_or(0);
                if c.combo(
                    "new_project_rate_type",
                    130.,
                    y,
                    160.,
                    21.,
                    &items,
                    &mut index,
                    true,
                ) {
                    d.rate_type = rates[index].0.to_string();
                }
                y += 30.;
                error(c, &mut y, &d, Some("new_project_rate_type"));
                error(c, &mut y, &d, None);
                kit::strip(c, W, H);
                kit::button_column(
                    c,
                    W,
                    &[
                        Entry::Button("new_project_finish", "Finish", true, false),
                        Entry::cancel("new_project_cancel"),
                        Entry::help("new_project_help"),
                    ],
                )
                .clicked
            });
        self.record_dialog_actions(actions);
        let open = !closed;
        let clicked = match clicked.as_deref() {
            Some("new_project_finish") => Some(0),
            Some("new_project_cancel") => Some(1),
            Some("new_project_help") => Some(2),
            _ => None,
        };
        if !open || clicked == Some(1) {
            return;
        }
        self.new_project = Some(d);
        if clicked == Some(2) {
            self.open_help_for("New Project");
        }
        if clicked == Some(0) {
            if let Err(e) = self.finish_new_project(time) {
                let taken = self.taken_project_ids();
                let nodes: Vec<i64> = self.workspace().eps().iter().map(|n| n.id).collect();
                if let Some(d) = self.new_project.as_mut() {
                    let field = match d.check_at(&taken, &nodes) {
                        Err((field, same)) if same == e => field,
                        _ => None,
                    };
                    d.refuse(e, field);
                }
            } else {
                self.new_project = None;
            }
        }
    }
}

#[cfg(test)]
mod tests;
