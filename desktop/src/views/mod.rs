//! Main-window view switching (side docks, dock tabs, view frame) and the
//! WBS, Projects and Resources views. Each view keeps its own column layout
//! and detail-pane height in the workspace preferences.
use super::*;
pub mod activity_views;
pub mod assignment_codes;
pub mod assignment_columns;
#[cfg(test)]
mod assignment_layout_tests;
pub mod assignment_layouts;
pub mod assignments;
pub mod budget;
#[cfg(test)]
mod budget_tests;
pub mod chart;
#[cfg(test)]
mod columns_tests;
#[cfg(test)]
mod command_bar_tests;
mod docks;
#[cfg(test)]
mod eps_tracking_tests;
pub(crate) use docks::{DockButton, DOCK_WIDTH};
pub(crate) use table_keys::ViewEdit;
pub mod documents;
pub mod expenses;
pub mod funding;
#[cfg(test)]
mod funding_tests;
pub mod grid;
#[cfg(test)]
mod headless;
pub mod issues;
#[cfg(test)]
mod monitor_ev_tests;
#[cfg(test)]
mod multiselect_tests;
pub mod project_ops;
pub mod project_rollups;
pub mod projects;
pub mod record_view;
#[cfg(test)]
mod record_view_tests;
pub mod resource_edit;
pub mod resource_merge;
#[cfg(test)]
pub(crate) mod resource_merge_tests;
#[cfg(test)]
mod resource_table_tests;
#[cfg(test)]
mod resource_tests;
pub mod resources;
#[cfg(test)]
mod resources_bottom_tests;
pub mod risks;
pub mod sheet;
#[cfg(test)]
mod tab_group_tests;
pub mod tab_groups;
pub(crate) mod table_cells;
mod table_keys;
pub mod thresholds;
pub mod udf_columns;
#[cfg(test)]
mod view_menus_tests;
pub mod wbs;
pub mod wbs_columns;
pub mod wbs_copy;

const LAYOUT_KEY: &str = "views_layout_v1";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MainView {
    Activities,
    /// The Additional Activities View.
    Activities2,
    Wbs,
    Projects,
    Resources,
    Reports,
    Assignments,
    Expenses,
    Documents,
    Risks,
    Issues,
    Thresholds,
    /// A report run into its own tab (see `reports::report_tab`).
    Report(crate::reports::report_tab::ReportTabId),
}

impl MainView {
    /// Either Activities view.
    pub fn is_activities(self) -> bool {
        matches!(self, Self::Activities | Self::Activities2)
    }
    pub(crate) fn key(self) -> String {
        match self {
            Self::Activities => "activities".into(),
            Self::Activities2 => "additional_activities".into(),
            Self::Wbs => "wbs".into(),
            Self::Projects => "projects".into(),
            Self::Resources => "resources".into(),
            Self::Reports => "reports".into(),
            Self::Assignments => "assignments".into(),
            Self::Expenses => "expenses".into(),
            Self::Documents => "documents".into(),
            Self::Risks => "risks".into(),
            Self::Issues => "issues".into(),
            Self::Thresholds => "thresholds".into(),
            Self::Report(id) => format!("report_{}", id.0),
        }
    }
}

pub struct Views {
    pub main: MainView,
    pub show_navigation: bool,
    pub show_directory: bool,
    /// Open views as tabs of one or more tab groups.
    pub groups: tab_groups::TabGroups,
    /// Set when a command switches views during the frame, so a click that
    /// also landed in another group does not take the focus back.
    navigated: bool,
    /// Tabs asked to close during a frame; they close when the view area has
    /// been drawn, so a group never disappears under the loop drawing it.
    close_queue: Vec<MainView>,
    /// Toolbars switched off through View > Toolbars.
    pub hidden_toolbars: BTreeSet<String>,
    /// Optional toolbars (hidden by default) switched on.
    pub shown_toolbars: BTreeSet<String>,
    /// Toolbar buttons (by action) hidden through Toolbars > Customize.
    pub hidden_buttons: BTreeSet<String>,
    /// View > Lock All Toolbars: no drag grips.
    pub toolbars_locked: bool,
    /// Where each bar is docked or floating, and the drag in progress.
    pub bars: crate::dockbars::Bars,
    /// The Toolbars > Customize dialog while open.
    pub customize: Option<crate::toolbars::Customize>,
    pub show_status_bar: bool,
    pub wbs: wbs::WbsState,
    pub projects: projects::ProjectsState,
    pub resources: resources::ResourcesState,
    pub assignments: assignments::AssignmentsState,
    /// Budget Log, Spending Plan and WBS Milestones selections.
    pub budget: budget::BudgetState,
    pub reports: crate::reports::reports_view::ReportsViewState,
    /// Expenses, Documents, Risks, Issues and Thresholds.
    pub records: record_view::RecordViews,
    /// The Activities view state that is not on `App` right now.
    pub activity_views: activity_views::ActivityViews,
    project_key: Option<(String, i64)>,
    snapshot: String,
    save_at: Option<f64>,
    /// Values the views and panes derive from the project (see `memo`).
    pub memo: std::cell::RefCell<crate::memo::Memo>,
    /// What the shown view table drew last, for its cell commands.
    pub(crate) cells: table_cells::TableCells,
    /// The in-place cell editor open in a view table, and that table's id.
    pub(crate) cell_edit: (&'static str, Option<grid::CellEdit>),
}

impl Default for Views {
    fn default() -> Self {
        Self {
            main: MainView::Activities,
            show_navigation: true,
            show_directory: true,
            groups: tab_groups::TabGroups::new(
                vec![MainView::Activities, MainView::Projects],
                MainView::Activities,
            ),
            navigated: false,
            close_queue: vec![],
            hidden_toolbars: crate::toolbars::default_hidden_toolbars(),
            shown_toolbars: BTreeSet::new(),
            hidden_buttons: BTreeSet::new(),
            toolbars_locked: false,
            bars: Default::default(),
            customize: None,
            show_status_bar: true,
            wbs: wbs::WbsState::default(),
            projects: projects::ProjectsState::default(),
            resources: resources::ResourcesState::default(),
            assignments: assignments::AssignmentsState::default(),
            budget: Default::default(),
            reports: Default::default(),
            records: record_view::RecordViews::default(),
            activity_views: Default::default(),
            project_key: None,
            snapshot: String::new(),
            save_at: None,
            memo: Default::default(),
            cells: Default::default(),
            cell_edit: ("", None),
        }
    }
}

impl Views {
    /// Switch views, opening a dock tab for a view shown the first time.
    pub fn go(&mut self, view: MainView) {
        self.groups.open(view);
        self.main = view;
        self.navigated = true;
    }
    /// Views with a dock tab, group by group in tab order.
    #[cfg(test)]
    pub fn opened(&self) -> Vec<MainView> {
        self.groups.tabs()
    }
    pub(crate) fn json(&self) -> String {
        serde_json::json!({
            "show_navigation": self.show_navigation,
            "show_directory": self.show_directory,
            "show_status_bar": self.show_status_bar,
            "hidden_toolbars": self.hidden_toolbars,
            "shown_toolbars": self.shown_toolbars,
            "hidden_buttons": self.hidden_buttons,
            "toolbars_locked": self.toolbars_locked,
            "bar_layout": self.bars.layout.to_json(),
            "wbs": self.wbs.json(),
            "projects": self.projects.json(),
            "resources": self.resources.json(),
            "assignments": self.assignments.json(),
            "reports": self.reports.json(),
            "records": self.records.json(),
        })
        .to_string()
    }
    pub(crate) fn restore(&mut self, json: &str) {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
            return;
        };
        for (key, field) in [
            ("show_navigation", &mut self.show_navigation),
            ("show_directory", &mut self.show_directory),
            ("show_status_bar", &mut self.show_status_bar),
            ("toolbars_locked", &mut self.toolbars_locked),
        ] {
            if let Some(saved) = value[key].as_bool() {
                *field = saved;
            }
        }
        if let Some(hidden) = value["hidden_toolbars"].as_array() {
            self.hidden_toolbars = hidden
                .iter()
                .filter_map(|name| name.as_str())
                .filter(|name| menu::BUILT_TOOLBARS.contains(name))
                .map(String::from)
                .collect();
        } else {
            // A stored record without the key predates the reference
            // defaults: every built toolbar was on.
            self.hidden_toolbars.clear();
        }
        if let Some(shown) = value["shown_toolbars"].as_array() {
            self.shown_toolbars = shown
                .iter()
                .filter_map(|name| name.as_str())
                .filter(|name| menu::OPTIONAL_TOOLBARS.contains(name))
                .map(String::from)
                .collect();
        }
        if let Some(hidden) = value["hidden_buttons"].as_array() {
            self.hidden_buttons = hidden
                .iter()
                .filter_map(|name| name.as_str())
                .filter(|name| crate::toolbars::is_button(name))
                .map(String::from)
                .collect();
        }
        if value.get("bar_layout").is_some() {
            self.bars.layout = crate::dockbars::BarLayout::from_json(&value["bar_layout"]);
        }
        self.wbs.restore(&value["wbs"]);
        self.projects.restore(&value["projects"]);
        self.resources.restore(&value["resources"]);
        self.assignments.restore(&value["assignments"]);
        self.reports.restore(&value["reports"]);
        self.records.restore(&value["records"]);
    }
}

/// Clamp a saved detail-pane height to something usable.
pub fn restore_height(value: &serde_json::Value, field: &mut f32) {
    if let Some(saved) = value.as_f64().filter(|v| v.is_finite()) {
        *field = (saved as f32).clamp(90., 700.);
    }
}

/// Clamp a saved table width (the table/chart splitter) to something usable.
pub fn restore_width(value: &serde_json::Value, field: &mut f32) {
    if let Some(saved) = value.as_f64().filter(|v| v.is_finite()) {
        *field = (saved as f32).clamp(150., 3000.);
    }
}

impl App {
    /// The bottom details pane of a view: margin, splitter, the page
    /// control with the shared tab strip, and the sheet, whose content
    /// `content` places with sheet-relative coordinates. Returns the pane
    /// height and the clicked tab.
    pub(super) fn details_pane(
        &mut self,
        host: &mut egui::Ui,
        id: &'static str,
        height: f32,
        names: &[&str],
        tab: usize,
        content: impl FnOnce(&mut App, &mut crate::schedule_dialogs::Canvas, Vec2),
    ) -> (f32, Option<usize>) {
        let mut clicked = None;
        // A short tab group still shows some of the table above.
        let room = (host.available_rect_before_wrap().height() - 150.).max(120.);
        let panel = egui::TopBottomPanel::bottom(egui::Id::new((id, "details")))
            .default_height(height)
            .min_height(120.)
            .max_height(room)
            .resizable(true)
            .show_separator_line(false)
            .frame(egui::Frame::NONE.fill(sheet::MARGIN))
            .show_inside(host, |ui| {
                let area = ui.max_rect();
                // 8 px margin, the 6 px raised splitter, 5 px margin, then the tabs.
                let split = Rect::from_min_size(
                    Pos2::new(area.left(), area.top() + 8.),
                    Vec2::new(area.width(), 6.),
                );
                let p = ui.painter();
                p.rect_filled(split, 0., CHROME);
                for (dy, color) in [
                    (0., sheet::SHADOW),
                    (1., Color32::WHITE),
                    (4., sheet::SHADOW),
                    (5., sheet::SHADOW),
                ] {
                    p.hline(
                        split.x_range(),
                        split.top() + dy + 0.5,
                        Stroke::new(1_f32, color),
                    );
                }
                let page = Rect::from_min_max(
                    Pos2::new(area.left() + 5., area.top() + 19.),
                    Pos2::new(area.right() - 5., area.bottom() - 3.),
                );
                let (tab_clicked, sheet_rect) =
                    sheet::page_control(ui, id, page, names, tab, &mut self.action_rects);
                clicked = tab_clicked;
                self.action_rects.insert(
                    format!("{id}_sheet"),
                    [
                        sheet_rect.left(),
                        sheet_rect.top(),
                        sheet_rect.right(),
                        sheet_rect.bottom(),
                    ],
                );
                let mut child = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(sheet_rect)
                        .layout(Layout::top_down(Align::LEFT)),
                );
                child.set_clip_rect(sheet_rect.intersect(ui.clip_rect()));
                let mut canvas = crate::schedule_dialogs::Canvas {
                    ui: &mut child,
                    origin: sheet_rect.min,
                    salt: id,
                    actions: vec![],
                };
                content(self, &mut canvas, sheet_rect.size());
                for (name, r) in canvas.actions {
                    self.action_rects
                        .insert(name, [r.left(), r.top(), r.right(), r.bottom()]);
                }
                ui.allocate_rect(area, Sense::hover());
            });
        (panel.response.rect.height().round(), clicked)
    }

    pub(crate) fn show_view(&mut self, view: MainView) {
        self.edit_cell = None;
        self.link_drag = None;
        self.views.go(view);
        self.use_activities_state(view);
    }

    /// View > Tab Groups > New Horizontal / Vertical Tab Group.
    pub(crate) fn new_tab_group(&mut self, split: tab_groups::Split) {
        if self.views.groups.split_current(split) {
            self.show_view(self.views.groups.current());
        }
    }

    /// View > Tab Groups > Merge All Tab Groups.
    pub(crate) fn merge_tab_groups(&mut self) {
        if self.views.groups.merge_all() {
            self.show_view(self.views.groups.current());
        }
    }

    /// Ctrl+Tab and Ctrl+Shift+Tab: the next or previous tab of the
    /// focused tab group.
    pub(crate) fn cycle_tabs(&mut self, forward: bool) {
        let view = self.views.groups.cycle(forward);
        self.show_view(view);
    }

    pub(super) fn restore_views(&mut self) {
        if let Ok(Some(json)) = self.store.preference(LAYOUT_KEY) {
            self.views.restore(&json);
        }
        self.views.snapshot = self.views.json();
    }

    pub(crate) fn save_views(&mut self) {
        let json = self.views.json();
        match self.store.set_preference(LAYOUT_KEY, &json) {
            Ok(()) => {
                self.views.snapshot = json;
                self.views.save_at = None;
            }
            Err(error) => self.error(format!("Cannot save view preferences: {error}")),
        }
    }

    fn persist_views_when_idle(&mut self, ctx: &egui::Context) {
        let json = self.views.json();
        let now = ctx.input(|i| i.time);
        if self.views.snapshot != json {
            self.views.snapshot = json;
            self.views.save_at = Some(now + 0.4);
        }
        if let Some(deadline) = self.views.save_at {
            if now >= deadline {
                self.save_views();
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(400));
            }
        }
    }

    /// Side docks, view caption, dock tabs and view frame. Runs every frame
    /// before the view's own panels.
    pub(super) fn navigation_bar(&mut self, ctx: &egui::Context) {
        let key = (
            self.project.source_name.clone(),
            self.project.model.project.proj_id,
        );
        if self.views.project_key.as_ref() != Some(&key) {
            self.views.project_key = Some(key);
            self.views.wbs.reset();
            self.views.resources.reset();
            self.views.assignments.reset();
            self.views.records.reset();
            self.views.projects.invalidate();
            // The reports in tabs were run on the project that is gone.
            self.close_report_tabs();
        }
        self.persist_views_when_idle(ctx);
        self.side_docks(ctx);
    }

    /// The view area between the docks: each tab group's frame and active
    /// view, with a splitter between groups. The focused group is drawn
    /// last and keeps the plain control names; the others' controls are
    /// recorded as `<name>@g<group>`. A click in a group focuses it.
    pub(super) fn view_area(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                let area = ui.max_rect();
                let (rects, splitters) = self.views.groups.layout(area);
                self.splitters(ui, area, &splitters);
                let n = rects.len();
                let focused = self.views.groups.focused;
                let press = ui.input(|i| {
                    i.pointer
                        .any_pressed()
                        .then(|| i.pointer.interact_pos())
                        .flatten()
                });
                let in_view = press.filter(|pos| {
                    ctx.layer_id_at(*pos)
                        .is_none_or(|layer| layer.order == egui::Order::Background)
                });
                self.views.navigated = false;
                let mut clicked = None;
                let order = (0..n).filter(|g| *g != focused).chain([focused]);
                for g in order {
                    let rect = rects[g];
                    let view = self.views.groups.groups[g].view();
                    self.views.main = view;
                    self.use_activities_state(view);
                    if view.is_activities() {
                        self.bar_geometry.clear();
                        self.link_geometry.clear();
                    }
                    let before = (g != focused).then(|| self.action_rects.clone());
                    let mut child = ui.new_child(
                        egui::UiBuilder::new()
                            .id_salt(("view_host", view.key()))
                            .max_rect(rect)
                            .layout(Layout::top_down(Align::Min)),
                    );
                    child.set_clip_rect(rect);
                    self.view_frame(&mut child, g, g == focused);
                    self.view_content(&mut child);
                    if let Some(before) = before {
                        let drawn = std::mem::replace(&mut self.action_rects, before);
                        for (name, r) in drawn {
                            if self.action_rects.get(&name) != Some(&r) {
                                self.action_rects.insert(format!("{name}@g{g}"), r);
                            }
                        }
                    }
                    if in_view.is_some_and(|pos| rect.contains(pos)) {
                        clicked = Some(g);
                    }
                }
                if let Some(g) = clicked.filter(|_| !self.views.navigated) {
                    self.views.groups.focused = g;
                }
                self.views.main = self.views.groups.current();
                self.use_activities_state(self.views.main);
            });
        for view in std::mem::take(&mut self.views.close_queue) {
            self.close_tab(view);
        }
    }

    /// The active view of a tab group, inside its frame.
    fn view_content(&mut self, ui: &mut egui::Ui) {
        if self.views.main.is_activities() {
            self.pin_serial();
            if self.show_details {
                self.bottom_pane(ui);
            }
            self.top_pane(ui);
            self.unpin_serial();
        } else {
            self.other_view(ui);
        }
    }

    /// The 6 px raised splitters between tab groups; dragging one moves the
    /// border between its two groups.
    fn splitters(&mut self, ui: &mut egui::Ui, area: Rect, splitters: &[Rect]) {
        let vertical = self.views.groups.split == tab_groups::Split::Vertical;
        let total = if vertical {
            area.width()
        } else {
            area.height()
        };
        for (i, rect) in splitters.iter().enumerate() {
            crate::activities::paint_splitter(ui.painter(), *rect, vertical);
            let response = ui.interact(*rect, ui.id().with(("tab_splitter", i)), Sense::drag());
            self.action(&format!("tab_splitter_{i}"), &response);
            if response.hovered() || response.dragged() {
                ui.ctx().set_cursor_icon(if vertical {
                    egui::CursorIcon::ResizeHorizontal
                } else {
                    egui::CursorIcon::ResizeVertical
                });
            }
            // The splitter follows the pointer.
            if let Some(pointer) = response
                .interact_pointer_pos()
                .filter(|_| response.dragged())
            {
                let delta = if vertical {
                    pointer.x - rect.center().x
                } else {
                    pointer.y - rect.center().y
                };
                self.views.groups.drag_splitter(i, delta, total);
            }
        }
    }

    /// Draws the selected non-activity view in the central area.
    pub(super) fn other_view(&mut self, host: &mut egui::Ui) {
        match self.views.main {
            MainView::Activities | MainView::Activities2 => {}
            MainView::Wbs => self.wbs_view(host),
            MainView::Projects => self.projects_view(host),
            MainView::Resources => self.resources_view(host),
            MainView::Reports => self.reports_view(host),
            MainView::Assignments => self.assignments_view(host),
            MainView::Expenses => self.expenses_view(host),
            MainView::Documents => self.documents_view(host),
            MainView::Risks => self.risks_view(host),
            MainView::Issues => self.issues_view(host),
            MainView::Thresholds => self.thresholds_view(host),
            MainView::Report(id) => self.report_tab_view(host, id),
        }
    }

    /// The Project menu.
    pub(super) fn project_menu_views(&mut self, ui: &mut egui::Ui) {
        use chrome::{Icon, Item};
        self.view_item(ui, MainView::Activities, "&Activities", Icon::Activities);
        if self.item(
            ui,
            "menu_additional_activities",
            Item::new("Additional A&ctivities View").icon(Icon::Activities),
        ) {
            self.open_additional_activities();
        }
        if self.item(
            ui,
            "menu_resource_assignments",
            Item::new("Re&source Assignments").icon(Icon::AssignResource),
        ) {
            self.show_view(MainView::Assignments);
        }
        self.view_item(ui, MainView::Wbs, "&WBS", Icon::Wbs);
        chrome::menu_separator(ui);
        self.baseline_menu_items(ui);
        self.view_item(ui, MainView::Expenses, "&Expenses", Icon::Expenses);
        self.view_item(ui, MainView::Documents, "&Documents", Icon::WorkProducts);
        chrome::menu_separator(ui);
        self.view_item(ui, MainView::Thresholds, "&Thresholds", Icon::Thresholds);
        self.view_item(ui, MainView::Issues, "&Issues", Icon::Issues);
        self.view_item(ui, MainView::Risks, "&Risks", Icon::Risks);
        chrome::menu_separator(ui);
        if self.item(
            ui,
            "menu_set_default_project",
            Item::new("Set Default &Project...").enabled(self.open_set.multi()),
        ) {
            self.default_project_dialog();
        }
    }

    fn view_item(&mut self, ui: &mut egui::Ui, view: MainView, label: &str, icon: chrome::Icon) {
        let item = chrome::Item::new(label).icon(icon);
        if self.item(ui, &format!("menu_{}", view.key()), item) {
            self.show_view(view);
        }
    }

    /// The Enterprise menu.
    pub(super) fn enterprise_menu(&mut self, ui: &mut egui::Ui) {
        use chrome::Icon;
        self.view_item(ui, MainView::Projects, "&Projects", Icon::Projects);
        self.enterprise_item(ui, crate::enterprise::Dialog::Eps);
        chrome::menu_separator(ui);
        self.view_item(ui, MainView::Resources, "&Resources", Icon::Resources);
        self.enterprise_item(ui, crate::enterprise::Dialog::Roles);
        self.enterprise_item(ui, crate::enterprise::Dialog::Obs);
        chrome::menu_separator(ui);
        self.dictionary_item(ui, crate::dictionaries::Dictionary::RESOURCE_CODES);
        self.dictionary_item(ui, crate::dictionaries::Dictionary::PROJECT_CODES);
        self.dictionary_item(ui, crate::dictionaries::Dictionary::ACTIVITY_CODES);
        self.enterprise_item(ui, crate::enterprise::Dialog::RoleCodes);
        self.dictionary_item(ui, crate::dictionaries::Dictionary::ASSIGNMENT_CODES);
        self.dictionary_item(ui, crate::dictionaries::Dictionary::Udf);
        chrome::menu_separator(ui);
        if self.item(
            ui,
            "menu_calendars",
            chrome::Item::new("&Calendars...").icon(Icon::Calendars),
        ) {
            self.open_calendars();
        }
        self.enterprise_item(ui, crate::enterprise::Dialog::Shifts);
        chrome::menu_separator(ui);
        self.dictionary_item(ui, crate::dictionaries::Dictionary::StepTemplates);
        self.dictionary_item(ui, crate::dictionaries::Dictionary::CostAccounts);
        self.enterprise_item(ui, crate::enterprise::Dialog::Funding);
        self.enterprise_item(ui, crate::enterprise::Dialog::Curves);
        if self.item(
            ui,
            "menu_external_apps",
            chrome::Item::new("Exter&nal Applications..."),
        ) {
            self.open_external_apps();
        }
    }
}

/// Long name of the open project for the window title.
pub fn project_title(p: &ProjectData) -> String {
    wbs::project_name(p)
}

impl App {
    /// The caption of a view: its name, or a report tab's report name.
    pub(crate) fn view_caption(&self, view: MainView) -> String {
        match view {
            MainView::Report(id) => self
                .reports
                .tabs
                .get(id)
                .map_or_else(|| "Report".to_string(), |t| t.caption.clone()),
            other => view_label(other).to_string(),
        }
    }
}

fn view_label(view: MainView) -> &'static str {
    match view {
        MainView::Activities | MainView::Activities2 => "Activities",
        MainView::Wbs => "WBS",
        MainView::Projects => "Projects",
        MainView::Resources => "Resources",
        MainView::Reports => "Reports",
        MainView::Assignments => "Resource Assignments",
        MainView::Expenses => "Project Expenses",
        MainView::Documents => "Documents",
        MainView::Risks => "Risks",
        MainView::Issues => "Project Issues",
        MainView::Thresholds => "Project Thresholds",
        MainView::Report(_) => "Report",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn views_open_dock_tabs_in_first_use_order() {
        let mut v = Views::default();
        assert_eq!(v.opened(), vec![MainView::Activities, MainView::Projects]);
        v.go(MainView::Wbs);
        v.go(MainView::Wbs);
        v.go(MainView::Projects);
        assert_eq!(v.main, MainView::Projects);
        assert_eq!(
            v.opened(),
            vec![MainView::Activities, MainView::Projects, MainView::Wbs]
        );
    }

    #[test]
    fn layout_json_round_trips_and_rejects_bad_columns() {
        let mut v = Views {
            show_directory: false,
            show_status_bar: false,
            ..Default::default()
        };
        v.hidden_toolbars.insert("Dictionaries".into());
        v.shown_toolbars.insert("Find".into());
        v.hidden_buttons.insert("tool_tab_merge".into());
        v.toolbars_locked = true;
        v.wbs.columns.widths[1] = 333.;
        v.wbs.columns.order.swap(0, 1);
        v.resources.details_height = 250.;
        let json = v.json();
        let mut w = Views::default();
        w.restore(&json);
        assert!(!w.show_directory && !w.show_status_bar);
        assert_eq!(w.hidden_toolbars, v.hidden_toolbars);
        assert_eq!(w.hidden_buttons, v.hidden_buttons);
        assert_eq!(w.shown_toolbars, v.shown_toolbars);
        assert!(w.toolbars_locked);
        w.restore(r#"{"shown_toolbars":["Reports","Dictionaries","No Such Bar"]}"#);
        assert_eq!(
            w.shown_toolbars.iter().collect::<Vec<_>>(),
            vec!["Reports"],
            "only optional toolbars can be shown"
        );
        w.restore(r#"{"hidden_buttons":["tool_tab_merge","not_a_button"]}"#);
        assert_eq!(w.hidden_buttons.len(), 1, "unknown buttons are dropped");
        w.restore(r#"{"hidden_toolbars":["Dictionaries","No Such Bar"]}"#);
        assert_eq!(w.hidden_toolbars.len(), 1, "unknown toolbars are dropped");
        assert_eq!(w.wbs.columns, v.wbs.columns);
        assert_eq!(w.resources.details_height, 250.);
        let mut c = grid::Columns::new(&[50., 60.]);
        c.restore(&serde_json::json!({"order":[0,0],"widths":[1.0, f32::MAX]}));
        assert_eq!(c, grid::Columns::new(&[50., 60.]));
    }
}
