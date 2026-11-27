//! Issues view (Project > Issues) and the Issue Navigator (Tools > Issue
//! Navigator): the project issues (PROJISSU) raised by hand or by Monitor
//! Thresholds, with priority, status, responsible manager, the activity or
//! WBS element they apply to, the threshold values and notes. Issue history
//! (ISSUHIST) is shown read-only.
use super::grid::Row;
use super::record_view::{self, ListSpec, ListState, PickTarget, RowKey, PRIORITY};
use super::sheet;
use super::*;
use crate::records::{self, num};
use crate::schedule_dialogs::Canvas;
pub use openxer_xer::edit::project_records::{add_issue, delete_issue, ISSUE_STATUS as STATUS};

pub const TITLES: [&str; 14] = [
    "Issue",
    "Priority",
    "Status",
    "Responsible Manager",
    "Date Identified",
    "Identified By",
    "Resolution Date",
    "Activity ID",
    "WBS",
    "Threshold Parameter",
    "Actual Value",
    "Lower Threshold",
    "Upper Threshold",
    "Activity Name",
];
const WIDTHS: [f32; 14] = [
    220., 70., 70., 130., 95., 120., 95., 90., 110., 170., 80., 80., 80., 180.,
];
const RIGHT: [usize; 3] = [10, 11, 12];
pub const TABS: [&str; 4] = ["General", "Details", "Notes", "User Defined Fields"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Filter {
    All,
    Open,
    Closed,
}

pub struct IssuesState {
    pub list: ListState,
    pub filter: Filter,
}

impl Default for IssuesState {
    fn default() -> Self {
        Self {
            list: ListState::new("issues", &TITLES, &WIDTHS),
            filter: Filter::All,
        }
    }
}

impl IssuesState {
    /// The display name of the current filter.
    pub fn display(&self) -> &'static str {
        match self.filter {
            Filter::All => "All Issues",
            Filter::Open => "Open Issues",
            Filter::Closed => "Closed Issues",
        }
    }
    pub fn json(&self) -> serde_json::Value {
        let mut v = self.list.json();
        v["filter"] = match self.filter {
            Filter::All => "all",
            Filter::Open => "open",
            Filter::Closed => "closed",
        }
        .into();
        v
    }
    pub fn restore(&mut self, value: &serde_json::Value) {
        self.list.restore(value, TABS.len());
        self.filter = match value["filter"].as_str() {
            Some("open") => Filter::Open,
            Some("closed") => Filter::Closed,
            Some("all") => Filter::All,
            _ => self.filter,
        };
    }
}

/// One issue as the view lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct Issue {
    pub id: i64,
    pub name: String,
    pub priority: String,
    pub status_code: String,
    pub manager: String,
    pub identified_on: String,
    pub identified_by: String,
    pub resolved_on: String,
    pub task_id: i64,
    pub wbs_id: i64,
    pub threshold_id: i64,
    pub parameter: String,
    pub value: String,
    pub low: String,
    pub high: String,
}

impl Issue {
    pub fn closed(&self) -> bool {
        self.status_code == "IS_Closed"
    }
}

/// The open project's issues, by name.
pub fn issues(p: &ProjectData) -> Vec<Issue> {
    let doc = &p.document;
    let pid = p.model.project.proj_id;
    let obs = record_view::dictionary(doc, "OBS", "obs_id", "obs_name");
    let parms = thresholds::parameters(doc);
    let mut list: Vec<Issue> = record_view::project_rows(doc, "PROJISSU", pid)
        .into_iter()
        .map(|r| {
            let v = |k: &str| records::get(doc, "PROJISSU", r, k);
            let parm = num(&v("thresh_parm_id"));
            Issue {
                id: num(&v("issue_id")),
                name: v("issue_name"),
                priority: record_view::label_of(&PRIORITY, &v("priority_type")),
                status_code: v("status_code"),
                manager: record_view::name_of(&obs, &v("obs_id")),
                identified_on: v("add_date"),
                identified_by: v("add_by_name"),
                resolved_on: v("resolv_date"),
                task_id: num(&v("task_id")),
                wbs_id: num(&v("wbs_id")),
                threshold_id: num(&v("thresh_id")),
                parameter: parms
                    .iter()
                    .find(|q| q.id == parm)
                    .map_or(String::new(), |q| q.name.clone()),
                value: record_view::show_number(&v("issue_value")),
                low: record_view::show_number(&v("lo_parm_value")),
                high: record_view::show_number(&v("hi_parm_value")),
            }
        })
        .collect();
    list.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    list
}

/// The text shown for what an issue applies to: Activity ID and name, or
/// WBS code.
fn applies_to(p: &ProjectData, i: &Issue) -> ((String, String), String) {
    let activity = if i.task_id != 0 {
        record_view::activity_label(p, i.task_id)
    } else {
        Default::default()
    };
    let wbs = if i.wbs_id != 0 {
        record_view::wbs_path(p, i.wbs_id)
    } else {
        String::new()
    };
    (activity, wbs)
}

/// The Issue Navigator window's selection.
#[derive(Clone, Debug, Default)]
pub struct Navigator {
    pub selected: Option<usize>,
}

impl App {
    pub(super) fn issues_view(&mut self, host: &mut egui::Ui) {
        let all = issues(&self.project);
        let filter = self.views.records.issues.filter;
        let list: Vec<Issue> = all
            .into_iter()
            .filter(|i| match filter {
                Filter::All => true,
                Filter::Open => !i.closed(),
                Filter::Closed => i.closed(),
            })
            .collect();
        let origin = self.project.model.origin_serial;
        let mut rows = vec![];
        let mut keys = vec![];
        for i in &list {
            let ((activity, activity_name), wbs) = applies_to(&self.project, i);
            keys.push(RowKey::Record(i.id));
            rows.push(Row {
                icon: Some(chrome::Icon::Issues),
                cells: vec![
                    i.name.clone(),
                    i.priority.clone(),
                    record_view::label_of(&STATUS, &i.status_code),
                    i.manager.clone(),
                    record_view::show_date(origin, &i.identified_on),
                    i.identified_by.clone(),
                    record_view::show_date(origin, &i.resolved_on),
                    activity,
                    wbs,
                    i.parameter.clone(),
                    i.value.clone(),
                    i.low.clone(),
                    i.high.clone(),
                    activity_name,
                ],
                ..Default::default()
            });
        }
        record_view::keep_selection(&mut self.views.records.issues.list, &keys);
        let selected = self.views.records.issues.list.selected;
        let spec = ListSpec {
            id: "issues",
            display: self.views.records.issues.display().into(),
            titles: &TITLES,
            right: &RIGHT,
            tabs: &TABS,
            commands: vec![
                ("Add", chrome::Icon::Add, true),
                ("Delete", chrome::Icon::Delete, selected.is_some()),
            ],
            empty: "No issues. Add one with the Add button, or run Tools > Monitor Thresholds.",
            details: "Issue Details",
            customize: "Issue",
            extras: vec![],
        };
        let command = self.record_list_view(
            host,
            &spec,
            |v| &mut v.records.issues.list,
            rows,
            keys,
            Box::new(move |app, c, size, tab| app.issue_details(c, size, tab, selected)),
            &mut |app, ui| {
                let f = &mut app.views.records.issues.filter;
                for (choice, label) in [
                    (Filter::All, "All Issues"),
                    (Filter::Open, "Open Issues"),
                    (Filter::Closed, "Closed Issues"),
                ] {
                    if chrome::menu_item(ui, chrome::Item::new(label).checked(*f == choice))
                        .clicked()
                    {
                        *f = choice;
                        ui.close();
                    }
                }
            },
        );
        match command {
            Some("Add") => self.issue_command_add(),
            Some("Delete") => self.issue_command_delete(),
            _ => {}
        }
    }

    pub(super) fn issue_command_add(&mut self) {
        let id = add_issue(&mut self.project, &record_view::today());
        self.views.records.issues.list.selected = Some(id);
        if self.views.records.issues.filter == Filter::Closed {
            self.views.records.issues.filter = Filter::All;
        }
        self.touch_records();
        self.status = "Issue added".into();
    }

    pub(super) fn issue_command_delete(&mut self) {
        self.record_delete(MainView::Issues, false);
    }

    fn set_issue_field(&mut self, id: i64, column: &str, text: String) {
        if record_view::set_value(
            &mut self.project.document,
            "PROJISSU",
            "issue_id",
            id,
            column,
            text,
        ) {
            self.touch_records();
        }
    }

    fn issue_details(&mut self, c: &mut Canvas, size: Vec2, tab: usize, id: Option<i64>) {
        let (w, h) = (size.x, size.y);
        let Some(id) = id else {
            c.label(12., 12., 13., "Select an issue.", true);
            return;
        };
        let origin = self.project.model.origin_serial;
        let doc = &self.project.document;
        let v = |k: &str| record_view::value(doc, "PROJISSU", "issue_id", id, k);
        let half = ((w - 60.) / 2.).clamp(160., 420.);
        let x2 = 20. + half + 20.;
        match TABS[tab.min(TABS.len() - 1)] {
            "General" => {
                let obs = record_view::dictionary(doc, "OBS", "obs_id", "obs_name");
                let fields = [
                    v("issue_name"),
                    v("obs_id"),
                    v("priority_type"),
                    v("status_code"),
                    v("add_date"),
                    v("add_by_name"),
                    v("resolv_date"),
                ];
                if let Some(t) = sheet::text_field(
                    c,
                    "issue_name",
                    20.,
                    25.,
                    w - 40.,
                    "Issue Name",
                    &fields[0],
                    true,
                ) {
                    self.set_issue_field(id, "issue_name", records::clean(&t));
                }
                if let Some(code) = record_view::dictionary_field(
                    c,
                    "issue_obs",
                    20.,
                    70.,
                    half,
                    "Responsible OBS",
                    &obs,
                    &fields[1],
                    true,
                ) {
                    self.set_issue_field(id, "obs_id", code);
                }
                if let Some(code) = record_view::combo_field(
                    c,
                    "issue_priority",
                    x2,
                    70.,
                    half.min(200.),
                    "Priority",
                    &PRIORITY,
                    &fields[2],
                    true,
                ) {
                    self.set_issue_field(id, "priority_type", code);
                }
                if let Some(code) = record_view::combo_field(
                    c,
                    "issue_status",
                    20.,
                    115.,
                    half.min(200.),
                    "Status",
                    &STATUS,
                    &fields[3],
                    true,
                ) {
                    self.set_issue_field(id, "status_code", code);
                }
                if let Some(t) = sheet::text_field(
                    c,
                    "issue_identified_by",
                    x2,
                    115.,
                    half,
                    "Identified By",
                    &fields[5],
                    true,
                ) {
                    self.set_issue_field(id, "add_by_name", records::clean(&t));
                }
                if let Some(t) = record_view::date_text(
                    c,
                    "issue_identified_on",
                    20.,
                    160.,
                    half.min(217.),
                    "Date Identified",
                    origin,
                    &fields[4],
                    true,
                ) {
                    self.set_issue_field(id, "add_date", t);
                }
                if let Some(t) = record_view::date_text(
                    c,
                    "issue_resolved_on",
                    x2,
                    160.,
                    half.min(217.),
                    "Resolution Date",
                    origin,
                    &fields[6],
                    true,
                ) {
                    self.set_issue_field(id, "resolv_date", t);
                }
            }
            "Details" => {
                let task_id = num(&v("task_id"));
                let wbs_id = num(&v("wbs_id"));
                let rsrc = v("rsrc_id");
                let parms = thresholds::parameters(doc);
                let parm = num(&v("thresh_parm_id"));
                let parameter = parms
                    .iter()
                    .find(|q| q.id == parm)
                    .map_or(String::new(), |q| q.name.clone());
                let (low, high, actual) = (
                    record_view::show_number(&v("lo_parm_value")),
                    record_view::show_number(&v("hi_parm_value")),
                    record_view::show_number(&v("issue_value")),
                );
                let activity = if task_id != 0 {
                    let (code, name) = record_view::activity_label(&self.project, task_id);
                    format!("{code}  {name}")
                } else {
                    String::new()
                };
                let wbs = if wbs_id != 0 {
                    record_view::wbs_path(&self.project, wbs_id)
                } else {
                    String::new()
                };
                let mut people: Vec<(String, String)> = self
                    .project
                    .resource_names
                    .iter()
                    .map(|(k, n)| (k.to_string(), n.clone()))
                    .collect();
                people.sort_by(|a, b| a.1.cmp(&b.1));
                sheet::above(c, 20., 25., "Applies to Activity", true);
                sheet::value_box(c, "issue_activity", 20., 25., half - 100., &activity, false);
                if c.button(
                    "issue_activity_pick",
                    20. + half - 92.,
                    25.,
                    92.,
                    21.,
                    "Activity...",
                    false,
                    true,
                ) {
                    self.open_picker(PickTarget::IssueActivity(id));
                }
                sheet::above(c, x2, 25., "Applies to WBS", true);
                sheet::value_box(c, "issue_wbs", x2, 25., half - 100., &wbs, false);
                if c.button(
                    "issue_wbs_pick",
                    x2 + half - 92.,
                    25.,
                    92.,
                    21.,
                    "WBS...",
                    false,
                    true,
                ) {
                    self.open_picker(PickTarget::IssueWbs(id));
                }
                if let Some(code) = record_view::dictionary_field(
                    c,
                    "issue_resource",
                    20.,
                    70.,
                    half,
                    "Applies to Resource",
                    &people,
                    &rsrc,
                    true,
                ) {
                    self.set_issue_field(id, "rsrc_id", code);
                }
                sheet::above(c, x2, 70., "Tracking Layout", false);
                sheet::picker(c, "issue_tracking_layout", x2, 70., half, None, "", false);
                sheet::above(c, 20., 115., "Threshold Parameter", true);
                sheet::value_box(c, "issue_parameter", 20., 115., half, &parameter, false);
                let third = ((half - 20.) / 3.).max(70.);
                for (k, caption, text) in [
                    (0, "Lower Threshold", &low),
                    (1, "Upper Threshold", &high),
                    (2, "Actual Value", &actual),
                ] {
                    let x = x2 + k as f32 * (third + 10.);
                    sheet::above(c, x, 115., caption, true);
                    sheet::value_box(c, &format!("issue_value_{k}"), x, 115., third, text, true);
                }
            }
            "Notes" => {
                let stored = v("issue_notes");
                let history: String = doc
                    .table("ISSUHIST")
                    .map(|t| {
                        t.rows
                            .iter()
                            .filter(|r| num(t.value(r, "issue_id")) == id)
                            .map(|r| records::memo_to_text(t.value(r, "issue_history")))
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .unwrap_or_default();
                let memo_h = if history.is_empty() {
                    h - 12.
                } else {
                    (h - 12.) * 0.6
                };
                if let Some(t) =
                    record_view::memo(c, "issue_notes", 6., 6., w - 12., memo_h, &stored, true)
                {
                    self.set_issue_field(id, "issue_notes", t);
                }
                if !history.is_empty() {
                    let top = 6. + memo_h + 6.;
                    c.bold(8., top, 18., "Issue History");
                    let rect = c.r(6., top + 20., w - 12., h - top - 26.);
                    c.sunken(rect, CHROME);
                    let galley = c.ui.painter().layout(
                        history,
                        crate::schedule_dialogs::dialog_font(),
                        INK,
                        rect.width() - 8.,
                    );
                    c.ui.painter()
                        .with_clip_rect(rect.intersect(c.ui.clip_rect()))
                        .galley(rect.min + Vec2::new(4., 3.), galley, INK);
                }
            }
            _ => {
                let rows = record_view::udf_rows(doc, "PROJISSU", id);
                record_view::udf_tab(c, "issue_udf", w, h, &rows);
            }
        }
    }

    pub(crate) fn open_issue_navigator(&mut self) {
        self.views.records.navigator = Some(Navigator::default());
    }

    /// Tools > Issue Navigator: the project issues with buttons that go to
    /// the issue's activity, its WBS element or its details.
    pub(super) fn issue_navigator(&mut self, ctx: &egui::Context) {
        let Some(nav) = self.views.records.navigator.clone() else {
            return;
        };
        let list = issues(&self.project);
        let applies: Vec<(String, String)> = list
            .iter()
            .map(|i| {
                let ((activity, _), wbs) = applies_to(&self.project, i);
                (activity, wbs)
            })
            .collect();
        let mut selected = nav.selected.filter(|&i| i < list.len());
        let (clicked, closed, actions) = crate::dialog_kit::window(
            ctx,
            "issue_navigator",
            "Issue Navigator",
            Vec2::new(640., 380.),
            |c| {
                crate::dialog_kit::text(c, 10., 8., "Project Issues", true);
                let cols = [
                    crate::dialog_kit::Col {
                        title: "Issue",
                        width: 200.,
                    },
                    crate::dialog_kit::Col {
                        title: "Priority",
                        width: 60.,
                    },
                    crate::dialog_kit::Col {
                        title: "Status",
                        width: 60.,
                    },
                    crate::dialog_kit::Col {
                        title: "Activity ID",
                        width: 90.,
                    },
                    crate::dialog_kit::Col {
                        title: "WBS",
                        width: 90.,
                    },
                ];
                let out = crate::dialog_kit::list(
                    c,
                    "navigator",
                    (10., 26., 515., 344.),
                    None,
                    &cols,
                    list.len(),
                    selected,
                    Some(0),
                    &mut |c, r, k, rect, sel| {
                        let i = &list[r];
                        let text = match k {
                            0 => i.name.clone(),
                            1 => i.priority.clone(),
                            2 => record_view::label_of(&STATUS, &i.status_code),
                            3 => applies[r].0.clone(),
                            _ => applies[r].1.clone(),
                        };
                        crate::dialog_kit::cell_text(c, rect, &text, sel, 3.);
                    },
                );
                if let Some(r) = out.clicked {
                    selected = Some(r);
                }
                let issue = selected.and_then(|r| list.get(r));
                use crate::dialog_kit::Entry;
                crate::dialog_kit::strip(c, 640., 380.);
                let column = crate::dialog_kit::button_column(
                    c,
                    640.,
                    &[
                        Entry::close("navigator_close"),
                        Entry::Button(
                            "navigator_activities",
                            "Activities",
                            issue.is_some_and(|i| i.task_id != 0),
                            true,
                        ),
                        Entry::Button(
                            "navigator_wbs",
                            "WBS",
                            issue.is_some_and(|i| i.wbs_id != 0 || i.task_id != 0),
                            false,
                        ),
                        Entry::Button("navigator_resources", "Resources", false, false),
                        Entry::Button("navigator_tracking", "Tracking", false, false),
                        Entry::Button("navigator_details", "Issue Details", issue.is_some(), false),
                        Entry::Button("navigator_notify", "Notify", false, false),
                        Entry::help("navigator_help"),
                    ],
                );
                let mut clicked = column
                    .clicked
                    .as_deref()
                    .and_then(|key| key.strip_prefix("navigator_"))
                    .and_then(|key| {
                        ["close", "help", "activities", "wbs", "details"]
                            .into_iter()
                            .find(|k| *k == key)
                    });
                if out.double.is_some() {
                    selected = out.double;
                    clicked = Some("details");
                }
                clicked
            },
        );
        self.record_dialog_actions(actions);
        if let Some(n) = &mut self.views.records.navigator {
            n.selected = selected;
        }
        let issue = selected.and_then(|r| list.get(r)).cloned();
        match clicked {
            Some("close") => self.views.records.navigator = None,
            Some("help") => self.open_help_for("issue_navigator"),
            Some("activities") => {
                if let Some(i) = issue {
                    if let Some(index) = self
                        .project
                        .model
                        .activities
                        .iter()
                        .position(|a| a.task_id == i.task_id)
                    {
                        self.selected = Some(index);
                        self.scroll_to_selected = true;
                        self.show_view(MainView::Activities);
                    }
                }
            }
            Some("wbs") => {
                if let Some(i) = issue {
                    let wbs = if i.wbs_id != 0 {
                        Some(i.wbs_id)
                    } else {
                        self.project.details.get(&i.task_id).map(|d| d.wbs)
                    };
                    if let Some(wbs) = wbs {
                        self.views.wbs.selected = Some(wbs);
                        self.show_view(MainView::Wbs);
                    }
                }
            }
            Some("details") => {
                if let Some(i) = issue {
                    self.views.records.issues.filter = Filter::All;
                    self.views.records.issues.list.selected = Some(i.id);
                    self.views.records.issues.list.show_details = true;
                    self.show_view(MainView::Issues);
                }
            }
            _ => {}
        }
        if closed || ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.views.records.navigator = None;
        }
    }
}
