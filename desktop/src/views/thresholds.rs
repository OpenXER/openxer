//! Thresholds view (Project > Thresholds) and Monitor Thresholds (Tools >
//! Monitor Thresholds, or the view's Monitor button). A threshold (PROJTHRS)
//! watches one parameter (THRSPARM) over the activities or WBS elements of a
//! WBS branch inside a time window; monitoring raises an issue (PROJISSU) for
//! every activity or element whose value falls below the lower or above the
//! upper threshold. A second run updates the open issue it raised before
//! instead of adding another.
//!
//! The thresholds model and the monitor live in
//! `openxer_xer::edit::thresholds`, shared with the command line.
use super::grid::Row;
use super::record_view::{self, ListSpec, ListState, PickTarget, RowKey, PRIORITY};
use super::sheet;
use super::*;
use crate::baselines::data::{BlDates, Compare, Slot};
use crate::grid::columns_ev::{self, Basis};
use crate::records::{self, num};
use crate::schedule_dialogs::Canvas;

pub const TITLES: [&str; 10] = [
    "Threshold Parameter",
    "Lower Threshold",
    "Upper Threshold",
    "Detail to Monitor",
    "WBS",
    "Responsible Manager",
    "Priority",
    "Status",
    "From Date",
    "To Date",
];
const WIDTHS: [f32; 10] = [220., 90., 90., 100., 140., 130., 70., 70., 95., 95.];
const RIGHT: [usize; 2] = [1, 2];
pub const TABS: [&str; 2] = ["General", "Details"];

pub use openxer_xer::edit::thresholds::{
    add_threshold, delete_threshold, monitor, parameters, thresholds, Figures, MonitorResult,
    Threshold, DETAIL, STATUS,
};

/// Figures of every activity of the project (by model index), from the
/// earned value columns' lookups `ix`.
pub fn figures(p: &ProjectData, ix: &crate::grid::columns::Index) -> Vec<Figures> {
    use crate::grid::columns::{self, Field};
    let minutes = |i: usize, f: Field| match columns::field(p, ix, i, f) {
        crate::grid::columns::Value::Duration(m) => m as f64,
        _ => 0.,
    };
    (0..p.model.activities.len())
        .map(|i| Figures {
            parts: columns_ev::parts(p, ix, i, Basis::Cost),
            original: minutes(i, Field::OriginalDuration),
            actual: minutes(i, Field::ActualDuration),
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Filter {
    All,
    Enabled,
    Disabled,
}

pub struct ThresholdsState {
    pub list: ListState,
    pub filter: Filter,
}

impl Default for ThresholdsState {
    fn default() -> Self {
        Self {
            list: ListState::new("thresholds", &TITLES, &WIDTHS),
            filter: Filter::All,
        }
    }
}

impl ThresholdsState {
    /// The display name of the current filter.
    pub fn display(&self) -> &'static str {
        match self.filter {
            Filter::All => "All Thresholds",
            Filter::Enabled => "Enabled Thresholds",
            Filter::Disabled => "Disabled Thresholds",
        }
    }
    pub fn json(&self) -> serde_json::Value {
        let mut v = self.list.json();
        v["filter"] = match self.filter {
            Filter::All => "all",
            Filter::Enabled => "enabled",
            Filter::Disabled => "disabled",
        }
        .into();
        v
    }
    pub fn restore(&mut self, value: &serde_json::Value) {
        self.list.restore(value, TABS.len());
        self.filter = match value["filter"].as_str() {
            Some("enabled") => Filter::Enabled,
            Some("disabled") => Filter::Disabled,
            Some("all") => Filter::All,
            _ => self.filter,
        };
    }
}

/// The Monitor Thresholds dialog: the thresholds' own windows, or one window
/// for this run.
#[derive(Clone, Debug, Default)]
pub struct MonitorDialog {
    pub new_window: bool,
    pub from: String,
    pub to: String,
    pub error: Option<String>,
}

impl App {
    pub(super) fn thresholds_view(&mut self, host: &mut egui::Ui) {
        let pid = self.project.model.project.proj_id;
        let doc = &self.project.document;
        let parms = parameters(doc);
        let obs = record_view::dictionary(doc, "OBS", "obs_id", "obs_name");
        let filter = self.views.records.thresholds.filter;
        let list: Vec<Threshold> = thresholds(doc, pid)
            .into_iter()
            .filter(|t| match filter {
                Filter::All => true,
                Filter::Enabled => t.enabled,
                Filter::Disabled => !t.enabled,
            })
            .collect();
        let origin = self.project.model.origin_serial;
        let mut rows = vec![];
        let mut keys = vec![];
        for t in &list {
            keys.push(RowKey::Record(t.id));
            rows.push(Row {
                icon: Some(chrome::Icon::Thresholds),
                cells: vec![
                    parms
                        .iter()
                        .find(|q| q.id == t.parameter)
                        .map_or(String::new(), |q| q.name.clone()),
                    t.low.map_or(String::new(), records::format_number),
                    t.high.map_or(String::new(), records::format_number),
                    if t.wbs_level { "WBS" } else { "Activities" }.into(),
                    record_view::wbs_path(&self.project, t.wbs_id),
                    record_view::name_of(&obs, &t.obs_id),
                    record_view::label_of(&PRIORITY, &t.priority),
                    if t.enabled { "Enabled" } else { "Disabled" }.into(),
                    record_view::show_date(origin, &t.window_start),
                    record_view::show_date(origin, &t.window_end),
                ],
                ..Default::default()
            });
        }
        record_view::keep_selection(&mut self.views.records.thresholds.list, &keys);
        let selected = self.views.records.thresholds.list.selected;
        let spec = ListSpec {
            id: "thresholds",
            display: self.views.records.thresholds.display().into(),
            titles: &TITLES,
            right: &RIGHT,
            tabs: &TABS,
            commands: vec![
                ("Add", chrome::Icon::Add, true),
                ("Delete", chrome::Icon::Delete, selected.is_some()),
                ("Monitor", chrome::Icon::Thresholds, !list.is_empty()),
            ],
            empty: "This project has no thresholds. Add one with the Add button.",
            details: "Threshold Details",
            customize: "Threshold",
            extras: vec![],
        };
        let command = self.record_list_view(
            host,
            &spec,
            |v| &mut v.records.thresholds.list,
            rows,
            keys,
            Box::new(move |app, c, size, tab| app.threshold_details(c, size, tab, selected)),
            &mut |app, ui| {
                let f = &mut app.views.records.thresholds.filter;
                ui.radio_value(f, Filter::All, "All Thresholds");
                ui.radio_value(f, Filter::Enabled, "Enabled Thresholds");
                ui.radio_value(f, Filter::Disabled, "Disabled Thresholds");
            },
        );
        match command {
            Some("Add") => self.threshold_command_add(),
            Some("Delete") => self.threshold_command_delete(),
            Some("Monitor") => self.open_monitor_thresholds(),
            _ => {}
        }
    }

    pub(super) fn threshold_command_add(&mut self) {
        let id = add_threshold(&mut self.project);
        self.views.records.thresholds.list.selected = Some(id);
        if self.views.records.thresholds.filter == Filter::Disabled {
            self.views.records.thresholds.filter = Filter::All;
        }
        self.touch_records();
        self.status = "Threshold added".into();
    }

    pub(super) fn threshold_command_delete(&mut self) {
        self.record_delete(MainView::Thresholds, false);
    }

    fn set_threshold_field(&mut self, id: i64, column: &str, text: String) {
        if record_view::set_value(
            &mut self.project.document,
            "PROJTHRS",
            "thresh_id",
            id,
            column,
            text,
        ) {
            self.touch_records();
        }
    }

    fn threshold_details(&mut self, c: &mut Canvas, size: Vec2, tab: usize, id: Option<i64>) {
        let (w, h) = (size.x, size.y);
        let Some(id) = id else {
            c.label(12., 12., 13., "Select a threshold.", true);
            return;
        };
        let origin = self.project.model.origin_serial;
        let doc = &self.project.document;
        let v = |k: &str| record_view::value(doc, "PROJTHRS", "thresh_id", id, k);
        let half = ((w - 60.) / 2.).clamp(160., 420.);
        let x2 = 20. + half + 20.;
        match TABS[tab.min(TABS.len() - 1)] {
            "General" => {
                let parms = parameters(doc);
                let options: Vec<(String, String)> = parms
                    .iter()
                    .map(|q| (q.id.to_string(), q.name.clone()))
                    .collect();
                let fields = [
                    v("thresh_parm_id"),
                    v("lo_parm_value"),
                    v("hi_parm_value"),
                    v("thresh_type"),
                    v("status_code"),
                    v("window_start"),
                    v("window_end"),
                ];
                let wbs = num(&v("wbs_id"));
                let wbs_text = if wbs == 0 {
                    String::new()
                } else {
                    format!(
                        "{}  {}",
                        record_view::wbs_path(&self.project, wbs),
                        self.project
                            .wbs
                            .iter()
                            .find(|x| x.id == wbs)
                            .map_or(String::new(), |x| x.name.clone())
                    )
                };
                let opts: Vec<(&str, &str)> = options
                    .iter()
                    .map(|(k, n)| (k.as_str(), n.as_str()))
                    .collect();
                if let Some(code) = record_view::combo_field(
                    c,
                    "threshold_parameter",
                    20.,
                    25.,
                    half,
                    "Threshold Parameter",
                    &opts,
                    &fields[0],
                    true,
                ) {
                    self.set_threshold_field(id, "thresh_parm_id", code);
                }
                let quarter = ((half - 10.) / 2.).max(80.);
                for (k, column, caption) in [
                    (0usize, "lo_parm_value", "Lower Threshold"),
                    (1, "hi_parm_value", "Upper Threshold"),
                ] {
                    if let Some(t) = record_view::number_text(
                        c,
                        &format!("threshold_{column}"),
                        x2 + k as f32 * (quarter + 10.),
                        25.,
                        quarter,
                        caption,
                        &fields[1 + k],
                        true,
                        true,
                    ) {
                        self.set_threshold_field(id, column, t);
                    }
                }
                if let Some(code) = record_view::combo_field(
                    c,
                    "threshold_detail",
                    20.,
                    70.,
                    half.min(200.),
                    "Detail to Monitor",
                    &DETAIL,
                    &fields[3],
                    true,
                ) {
                    self.set_threshold_field(id, "thresh_type", code);
                }
                if let Some(code) = record_view::combo_field(
                    c,
                    "threshold_status",
                    x2,
                    70.,
                    half.min(200.),
                    "Status",
                    &STATUS,
                    &fields[4],
                    true,
                ) {
                    self.set_threshold_field(id, "status_code", code);
                }
                sheet::above(c, 20., 115., "WBS to Monitor", true);
                sheet::value_box(c, "threshold_wbs", 20., 115., half - 100., &wbs_text, false);
                if c.button(
                    "threshold_wbs_pick",
                    20. + half - 92.,
                    115.,
                    92.,
                    21.,
                    "WBS...",
                    false,
                    true,
                ) {
                    self.open_picker(PickTarget::ThresholdWbs(id));
                }
                sheet::section(c, x2 - 10., 100., half + 20., 70., "Monitor Time Window");
                let width = ((half - 30.) / 2.).clamp(100., 217.);
                for (k, column, caption) in [
                    (0usize, "window_start", "From Date"),
                    (1, "window_end", "To Date"),
                ] {
                    if let Some(t) = record_view::date_text(
                        c,
                        &format!("threshold_{column}"),
                        x2 + k as f32 * (width + 10.),
                        142.,
                        width,
                        caption,
                        origin,
                        &fields[5 + k],
                        true,
                    ) {
                        self.set_threshold_field(id, column, t);
                    }
                }
            }
            _ => {
                let obs = record_view::dictionary(doc, "OBS", "obs_id", "obs_name");
                let (manager, priority) = (v("obs_id"), v("priority_type"));
                let raised: Vec<(i64, Vec<String>)> = super::issues::issues(&self.project)
                    .into_iter()
                    .filter(|i| i.threshold_id == id)
                    .map(|i| {
                        (
                            i.id,
                            vec![
                                i.name.clone(),
                                record_view::label_of(&super::issues::STATUS, &i.status_code),
                                i.value.clone(),
                            ],
                        )
                    })
                    .collect();
                if let Some(code) = record_view::dictionary_field(
                    c,
                    "threshold_obs",
                    20.,
                    25.,
                    half,
                    "Responsible Manager",
                    &obs,
                    &manager,
                    true,
                ) {
                    self.set_threshold_field(id, "obs_id", code);
                }
                if let Some(code) = record_view::combo_field(
                    c,
                    "threshold_priority",
                    x2,
                    25.,
                    half.min(200.),
                    "Issue Priority",
                    &PRIORITY,
                    &priority,
                    true,
                ) {
                    self.set_threshold_field(id, "priority_type", code);
                }
                sheet::above(c, 20., 70., "Tracking Layout", false);
                sheet::picker(c, "threshold_tracking", 20., 70., half, None, "", false);
                sheet::above(c, x2, 70., "Project Issue Count", true);
                sheet::value_box(
                    c,
                    "threshold_issue_count",
                    x2,
                    70.,
                    100.,
                    &raised.len().to_string(),
                    true,
                );
                c.bold(20., 100., 18., "Threshold Issues");
                let cols = [
                    ("Issue", 320., false),
                    ("Status", 90., false),
                    ("Actual Value", 90., true),
                ];
                let rows: Vec<Vec<String>> = raised.iter().map(|(_, r)| r.clone()).collect();
                let sub = self
                    .views
                    .records
                    .thresholds
                    .list
                    .sub_row
                    .filter(|&r| r < rows.len());
                if let Some(r) = sheet::table(
                    c,
                    "threshold_issues",
                    20.,
                    120.,
                    w - 140.,
                    (h - 128.).max(40.),
                    &cols,
                    &rows,
                    sub,
                ) {
                    self.views.records.thresholds.list.sub_row = Some(r);
                }
                let issue = sub.and_then(|r| raised.get(r)).map(|(id, _)| *id);
                if c.button(
                    "threshold_issue_goto",
                    w - 110.,
                    120.,
                    90.,
                    21.,
                    "Go To",
                    false,
                    issue.is_some(),
                ) {
                    if let Some(issue) = issue {
                        self.views.records.issues.filter = super::issues::Filter::All;
                        self.views.records.issues.list.selected = Some(issue);
                        self.show_view(MainView::Issues);
                    }
                }
                if c.button(
                    "threshold_issue_delete",
                    w - 110.,
                    147.,
                    90.,
                    21.,
                    "Delete",
                    false,
                    issue.is_some(),
                ) {
                    if let Some(issue) = issue {
                        if super::issues::delete_issue(&mut self.project, issue) {
                            self.views.records.thresholds.list.sub_row = None;
                            self.touch_records();
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn open_monitor_thresholds(&mut self) {
        self.views.records.monitor = Some(MonitorDialog::default());
    }

    /// Run the monitor over the open project and report what it raised.
    pub(super) fn run_monitor(
        &mut self,
        window: Option<(Option<i64>, Option<i64>)>,
    ) -> MonitorResult {
        let compare: std::sync::Arc<Compare> = self.baseline_compare();
        let baseline: Vec<Option<BlDates>> = self
            .project
            .model
            .activities
            .iter()
            .map(|a| compare.dates(&self.project, a, Slot::Project))
            .collect();
        // The earned value columns' own lookups, with the assigned baselines.
        let mut ix = crate::grid::columns::Index::new(self.critical_definition());
        ix.baselines = Some(compare.clone());
        let figures = figures(&self.project, &ix);
        let result = monitor(
            &mut self.project,
            &baseline,
            &figures,
            window,
            &record_view::today(),
            &record_view::wbs_path,
        );
        if result.created + result.updated > 0 {
            self.touch_records();
        }
        self.status = format!(
            "Monitor Thresholds: {} issue(s) raised, {} updated from {} threshold(s){}",
            result.created,
            result.updated,
            result.thresholds,
            if result.skipped > 0 {
                format!(
                    "; {} skipped (parameter missing, or not available at WBS level)",
                    result.skipped
                )
            } else {
                String::new()
            }
        );
        result
    }

    /// Tools > Monitor Thresholds: the thresholds' own windows or one new
    /// window, then Monitor.
    pub(super) fn monitor_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.views.records.monitor.clone() else {
            return;
        };
        let origin = self.project.model.origin_serial;
        let ((monitor, cancel, help), closed, actions) = crate::dialog_kit::window(
            ctx,
            "monitor_thresholds",
            "Monitor Thresholds",
            Vec2::new(400., 170.),
            |c| {
                if c.radio(
                    "monitor_original",
                    12.,
                    12.,
                    270.,
                    17.,
                    "Use original threshold monitor windows",
                    !d.new_window,
                    true,
                ) {
                    d.new_window = false;
                }
                if c.radio(
                    "monitor_new",
                    12.,
                    36.,
                    270.,
                    17.,
                    "Use new threshold monitor window",
                    d.new_window,
                    true,
                ) {
                    d.new_window = true;
                }
                crate::dialog_kit::text(c, 32., 64., "From Date", d.new_window);
                c.edit(
                    "monitor_from",
                    110.,
                    60.,
                    160.,
                    21.,
                    &mut d.from,
                    d.new_window,
                    Align::Min,
                );
                crate::dialog_kit::text(c, 32., 94., "To Date", d.new_window);
                c.edit(
                    "monitor_to",
                    110.,
                    90.,
                    160.,
                    21.,
                    &mut d.to,
                    d.new_window,
                    Align::Min,
                );
                if let Some(e) = &d.error {
                    crate::dialog_kit::error_text(c, 12., 124., 270., e);
                }
                use crate::dialog_kit::Entry;
                crate::dialog_kit::strip(c, 400., 170.);
                let column = crate::dialog_kit::button_column(
                    c,
                    400.,
                    &[
                        Entry::Button("monitor_run", "Monitor", true, false),
                        Entry::cancel("monitor_cancel"),
                        Entry::help("monitor_help"),
                    ],
                );
                let clicked = |key: &str| column.clicked.as_deref() == Some(key);
                (
                    clicked("monitor_run"),
                    clicked("monitor_cancel"),
                    clicked("monitor_help"),
                )
            },
        );
        self.record_dialog_actions(actions);
        if help {
            self.open_help_for("monitor_thresholds");
        }
        if monitor {
            let window = if d.new_window {
                let parse = |t: &str| format::parse_date(origin, t, None);
                match (parse(&d.from), parse(&d.to)) {
                    (Ok(a), Ok(b)) => Some(Some((a, b))),
                    _ => None,
                }
            } else {
                Some(None)
            };
            match window {
                Some(window) => {
                    self.run_monitor(window);
                    self.views.records.monitor = None;
                    self.views.records.issues.filter = super::issues::Filter::All;
                    self.show_view(MainView::Issues);
                    return;
                }
                None => d.error = Some("Enter the From and To dates as dates.".into()),
            }
        }
        if cancel || closed || ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.views.records.monitor = None;
        } else {
            self.views.records.monitor = Some(d);
        }
    }
}
