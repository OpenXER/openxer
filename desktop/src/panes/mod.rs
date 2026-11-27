//! What the Activities view shows in its two panes: the top pane holds the
//! table and Gantt Chart or the Activity Network; the bottom pane holds
//! Activity Details, a second activity table or table and Gantt Chart,
//! Trace Logic, a usage spreadsheet or profile, or nothing (View > Show on
//! Top / Show on Bottom, the Top Layout and Bottom Layout toolbars). The
//! choice is kept in the workspace and in saved layouts. These are views
//! only: nothing here edits the project, so undo is not involved (edits
//! made in the bottom table go through the same commands as the top's).
use super::*;
#[cfg(test)]
mod actual_remaining_tests;
pub mod boxes;
#[cfg(test)]
mod ctx_tests;
#[cfg(test)]
mod curve_spread_tests;
pub mod histogram;
pub mod network;
pub mod profile_options;
pub(crate) mod sheet_pane;
pub mod stacked;
#[cfg(test)]
pub(crate) mod stacked_tests;
#[cfg(test)]
mod tests;
pub mod time_fields;
pub mod trace;
pub mod usage;
pub mod usage_cells;
pub mod usage_fields;
pub mod usage_list;
#[cfg(test)]
mod usage_list_tests;
mod usage_menu;
#[cfg(test)]
mod usage_totals_tests;

use crate::view::ViewSettings;
use chrome::{Icon, Item};

/// Workspace preference holding the pane choices and their options.
pub const PREF_KEY: &str = "activity_panes_v1";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BottomView {
    Details,
    ActivityTable,
    Gantt,
    ActivityUsageSheet,
    ResourceUsageSheet,
    ActivityUsageProfile,
    ResourceUsageProfile,
    TraceLogic,
}

impl BottomView {
    /// Show on Bottom order, after No Bottom Layout.
    pub const MENU: [BottomView; 8] = [
        BottomView::Details,
        BottomView::ActivityTable,
        BottomView::Gantt,
        BottomView::ActivityUsageSheet,
        BottomView::ResourceUsageSheet,
        BottomView::ActivityUsageProfile,
        BottomView::ResourceUsageProfile,
        BottomView::TraceLogic,
    ];
    pub fn key(self) -> &'static str {
        match self {
            BottomView::Details => "details",
            BottomView::ActivityTable => "activity_table",
            BottomView::Gantt => "gantt",
            BottomView::ActivityUsageSheet => "activity_usage_spreadsheet",
            BottomView::ResourceUsageSheet => "resource_usage_spreadsheet",
            BottomView::ActivityUsageProfile => "activity_usage_profile",
            BottomView::ResourceUsageProfile => "resource_usage_profile",
            BottomView::TraceLogic => "trace_logic",
        }
    }
    pub fn from_key(key: &str) -> Option<Self> {
        Self::MENU.into_iter().find(|v| v.key() == key)
    }
    pub fn caption(self) -> &'static str {
        match self {
            BottomView::Details => "Activity Details",
            BottomView::ActivityTable => "Activity Table",
            BottomView::Gantt => "Gantt Chart",
            BottomView::ActivityUsageSheet => "Activity Usage Spreadsheet",
            BottomView::ResourceUsageSheet => "Resource Usage Spreadsheet",
            BottomView::ActivityUsageProfile => "Activity Usage Profile",
            BottomView::ResourceUsageProfile => "Resource Usage Profile",
            BottomView::TraceLogic => "Trace Logic",
        }
    }
    pub fn icon(self) -> Icon {
        match self {
            BottomView::Details => Icon::Details,
            BottomView::ActivityTable => Icon::Table,
            BottomView::Gantt => Icon::Gantt,
            BottomView::ActivityUsageSheet => Icon::ActivityUsageSheet,
            BottomView::ResourceUsageSheet => Icon::ResourceUsageSheet,
            BottomView::ActivityUsageProfile => Icon::ActivityUsageProfile,
            BottomView::ResourceUsageProfile => Icon::ResourceUsageProfile,
            BottomView::TraceLogic => Icon::Trace,
        }
    }
    /// The bottom Activity Table and Gantt Chart: a second activity table
    /// (with the Gantt for `Gantt`) in the bottom pane.
    pub fn is_table(self) -> bool {
        matches!(self, BottomView::ActivityTable | BottomView::Gantt)
    }
}

/// The bottom Gantt Chart's own table/chart split and horizontal timescale:
/// its place along the dates, zoom and bands, kept apart from the top
/// pane's. Until it is first shown it starts from the top pane's.
#[derive(Clone, Debug)]
pub struct BottomScale {
    pub grid_fraction: f32,
    /// None until the bottom Gantt first draws (or a saved layout without
    /// its scroll position loads); it then starts at the top pane's.
    pub timeline_origin: Option<i64>,
    pub pixels_per_day: f32,
    pub chart_width: f32,
    pub upper: timescale::Unit,
    pub lower: timescale::Unit,
    pub automatic_units: bool,
    /// Set once the bottom Gantt has its own values (drawn once, or loaded).
    pub own: bool,
}

impl Default for BottomScale {
    fn default() -> Self {
        let view = ViewSettings::default();
        Self {
            grid_fraction: 0.6,
            timeline_origin: None,
            pixels_per_day: 16.,
            chart_width: 520.,
            upper: view.upper,
            lower: view.lower,
            automatic_units: view.automatic_units,
            own: false,
        }
    }
}

impl BottomScale {
    /// The stored form; `scroll` adds the position along the dates.
    fn json(&self, scroll: bool) -> serde_json::Value {
        let mut v = serde_json::json!({
            "upper": self.upper.label(),
            "lower": self.lower.label(),
            "automatic_units": self.automatic_units,
            "pixels_per_day": self.pixels_per_day,
            "grid_fraction": self.grid_fraction,
        });
        if let Some(origin) = self.timeline_origin.filter(|_| scroll) {
            v["timeline_origin"] = origin.into();
        }
        v
    }

    /// Restore from the stored form; invalid parts keep the current values.
    fn restore(&mut self, v: &serde_json::Value) {
        let mut view = ViewSettings {
            upper: self.upper,
            lower: self.lower,
            automatic_units: self.automatic_units,
            ..Default::default()
        };
        view::apply_view_value(
            v,
            &mut view,
            &mut self.pixels_per_day,
            &mut self.grid_fraction,
        );
        (self.upper, self.lower, self.automatic_units) =
            (view.upper, view.lower, view.automatic_units);
        // About 1,000 years of minutes either side of the model's origin.
        if let Some(origin) = v["timeline_origin"]
            .as_i64()
            .filter(|o| o.unsigned_abs() < 600_000_000_000)
        {
            self.timeline_origin = Some(origin);
        }
        self.own = true;
    }
}

/// Most columns a saved bottom table may list.
pub const MAX_BOTTOM_COLUMNS: usize = 400;

/// The bottom Activity Table and Gantt Chart's own state. They show the top
/// pane's rows (filter, grouping, sorting and collapsed bands), selection
/// and timescale, with their own columns. The table and Gantt painters keep
/// caches and hit geometry on `App`; those of the bottom pane wait here and
/// change places with the top pane's while the bottom pane is drawn.
#[derive(Default)]
pub struct BottomGrid {
    /// The bottom table's columns; empty until first shown, when it starts
    /// from the top table's columns.
    pub columns: Vec<grid::layout::ColumnSetting>,
    /// Row cache, frame cache, band list, horizontal scroll, row offsets.
    cache: Option<Box<grid::GridState>>,
    bar_geometry: Vec<BarGeometry>,
    row_geometry: Vec<(RowAnchor, Rect)>,
    link_geometry: Vec<(usize, Vec<Pos2>)>,
    collapsed_geometry: Vec<BarGeometry>,
    pub(crate) gantt_left: f32,
    edit_cell: Option<(usize, String)>,
    edit_text: String,
    edit_focus: bool,
    link_drag: Option<LinkDrag>,
    heights: std::cell::RefCell<Option<crate::layout_opts::MeasuredHeights>>,
    /// True while the bottom table is drawn (its state is on `App`).
    pub drawing: bool,
    /// Row tops of the bottom table in the last frame and the screen y of
    /// its first row, for tests and renders.
    pub row_offsets: std::rc::Rc<Vec<f32>>,
    pub body_top: f32,
    /// The bottom table's header top and height in the last frame.
    pub header: (f32, f32),
    /// The bottom Gantt's own split and timescale.
    pub scale: BottomScale,
    /// A zoom command for the bottom Gantt, applied when it next draws (its
    /// timescale is on `App` only then): one rung in or out, or None to fit.
    pub pending_zoom: Option<Option<bool>>,
}

impl BottomGrid {
    /// The bottom table's horizontal scroll (its state is parked here
    /// except while the bottom table is drawn).
    #[cfg(test)]
    pub(crate) fn scroll_x(&self) -> f32 {
        self.cache.as_ref().map_or(0., |g| g.scroll_x)
    }

    #[cfg(test)]
    pub(crate) fn set_scroll_x(&mut self, x: f32) {
        if let Some(g) = self.cache.as_mut() {
            g.scroll_x = x;
        }
    }
}

/// Bottom table columns in the stored form.
fn columns_json(columns: &[grid::layout::ColumnSetting]) -> serde_json::Value {
    serde_json::Value::Array(
        columns
            .iter()
            .map(|c| {
                serde_json::json!({
                    "id": c.id, "width": c.width, "title": c.title,
                    "align": c.align.map(grid::columns::Align::key),
                })
            })
            .collect(),
    )
}

/// Parse stored bottom table columns; any invalid entry rejects the list.
pub fn columns_from_json(v: &serde_json::Value) -> Option<Vec<grid::layout::ColumnSetting>> {
    let list = v.as_array()?;
    if list.len() > MAX_BOTTOM_COLUMNS {
        return None;
    }
    let mut out: Vec<grid::layout::ColumnSetting> = vec![];
    for c in list {
        let id = c["id"].as_str()?;
        if id.trim().is_empty() || id.len() > 200 || out.iter().any(|x| x.id == id) {
            return None;
        }
        let width = c["width"].as_f64().filter(|w| (8. ..=2000.).contains(w))? as f32;
        let title = match &c["title"] {
            serde_json::Value::Null => None,
            t => Some(t.as_str().filter(|t| t.chars().count() <= 200)?.to_string()),
        };
        let align = match &c["align"] {
            serde_json::Value::Null => None,
            a => Some(grid::columns::Align::from_key(a.as_str()?)?),
        };
        out.push(grid::layout::ColumnSetting {
            id: id.to_string(),
            width,
            title,
            align,
        });
    }
    Some(out)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TopView {
    /// The activity table beside the Gantt chart.
    Gantt,
    Network,
    /// The activity table alone.
    Table,
    /// The Activity Usage Spreadsheet (activity rows and time-phased units).
    ActivityUsageSheet,
    /// The Resource Usage Spreadsheet (resources, their assignments and
    /// time-phased units).
    ResourceUsageSheet,
}

impl TopView {
    fn key(self) -> &'static str {
        match self {
            TopView::Gantt => "gantt",
            TopView::Network => "network",
            TopView::Table => "table",
            TopView::ActivityUsageSheet => "activity_usage_spreadsheet",
            TopView::ResourceUsageSheet => "resource_usage_spreadsheet",
        }
    }
}

pub struct PanesState {
    pub bottom: BottomView,
    pub top: TopView,
    pub trace: trace::TraceState,
    pub network: network::NetworkState,
    pub usage: usage::UsageState,
    /// Spreadsheet Fields and Profile Options dialogs, while open.
    pub usage_fields: Option<usage_fields::FieldsDraft>,
    /// Spreadsheet Fields > Customize... while open: the time-phased fields.
    pub time_fields: Option<usage_fields::TimeFieldsDraft>,
    /// Profile Options while open: the working copy.
    pub profile_dialog: Option<profile_options::Draft>,
    /// The resource list's Filter By > Customize... and Group and Sort By >
    /// Customize... dialogs while open: which list and the working copy.
    pub usage_list_filter: Option<(usage_list::ListOwner, usage_list::CustomFilter)>,
    pub usage_list_group: Option<(usage_list::ListOwner, Vec<usage_list::GroupLevel>)>,
    /// The periods and kind of the usage spreadsheet drawn last, for Copy to
    /// Excel.
    pub sheet_periods: Vec<(i64, i64)>,
    /// The Resource Usage Spreadsheet's selected cells and cell editor.
    pub sheet_cells: usage_cells::SheetCells,
    /// The spreadsheet the cell selection belongs to: the Resource Usage
    /// Spreadsheet or the Resource Assignments window's.
    pub sheet_host: usage_cells::SheetHost,
    pub trace_dialog: Option<trace::TraceOptions>,
    pub network_dialog: Option<network::NetworkDialog>,
    /// Activity Network Font & Colors, and its result for an open Activity
    /// Network Options dialog.
    pub network_font: Option<network::FontDraft>,
    pub network_font_done: Option<network::NetworkOptions>,
    /// Activity Network Copy From, and its result for an open Activity
    /// Network Options dialog.
    pub network_copy: Option<network::CopyDraft>,
    pub network_copy_done: Option<network::Copied>,
    /// The bottom Activity Table and Gantt Chart.
    pub grid: BottomGrid,
    /// The bottom view's body in the last frame, for tests and renders.
    pub body: Rect,
    /// True while a bottom view paints into a printout (no controls).
    pub printing: bool,
    /// The timeline the zoom commands act on: the top Gantt (and the usage
    /// views that share its timescale) or the bottom Gantt.
    pub timeline_focus: crate::timeline::Focus,
    /// The scroll bar strip of the usage profile being drawn, handed to its
    /// histogram painter.
    pub usage_strip: Option<Rect>,
    snapshot: String,
}

impl Default for PanesState {
    fn default() -> Self {
        Self {
            bottom: BottomView::Details,
            top: TopView::Gantt,
            trace: Default::default(),
            network: Default::default(),
            usage: Default::default(),
            usage_fields: None,
            time_fields: None,
            profile_dialog: None,
            usage_list_filter: None,
            usage_list_group: None,
            sheet_periods: vec![],
            sheet_cells: Default::default(),
            sheet_host: Default::default(),
            trace_dialog: None,
            network_dialog: None,
            network_font: None,
            network_font_done: None,
            network_copy: None,
            network_copy_done: None,
            grid: BottomGrid::default(),
            body: Rect::NOTHING,
            printing: false,
            timeline_focus: crate::timeline::Focus::Activities,
            usage_strip: None,
            snapshot: String::new(),
        }
    }
}

impl PanesState {
    /// Forget the bottom table's kept rows and lookups and the network
    /// layout (see `App::drop_kept_values`).
    #[cfg(test)]
    pub(crate) fn drop_kept_values(&self) {
        if let Some(g) = &self.grid.cache {
            g.drop_rows_cache();
        }
        self.network.forget_layout();
    }
}

impl PanesState {
    /// The stored form: pane choices and the options of each pane.
    pub fn json(&self) -> serde_json::Value {
        self.json_with(true)
    }

    /// The stored form; `scroll` includes the bottom Gantt's position along
    /// the dates, which layouts keep and the workspace preference leaves
    /// out (it would be written at every scroll step).
    pub fn json_with(&self, scroll: bool) -> serde_json::Value {
        let mut v = serde_json::json!({
            "top": self.top.key(),
            "bottom": self.bottom.key(),
            "trace": self.trace.options.json(),
            "network": self.network.json(),
            "usage": self.usage.json(),
            "bottom_columns": columns_json(&self.grid.columns),
        });
        if self.grid.scale.own {
            v["bottom_gantt"] = self.grid.scale.json(scroll);
        }
        v
    }
    /// Restore from the stored form; unknown or invalid parts keep their
    /// current values.
    pub fn restore(&mut self, v: &serde_json::Value) {
        match v["top"].as_str() {
            Some("network") => self.top = TopView::Network,
            Some("gantt") => self.top = TopView::Gantt,
            Some("table") => self.top = TopView::Table,
            Some("activity_usage_spreadsheet") => self.top = TopView::ActivityUsageSheet,
            Some("resource_usage_spreadsheet") => self.top = TopView::ResourceUsageSheet,
            _ => {}
        }
        if let Some(view) = v["bottom"].as_str().and_then(BottomView::from_key) {
            self.bottom = view;
        }
        self.trace.options.restore(&v["trace"]);
        self.network.restore(&v["network"]);
        self.usage.restore(&v["usage"]);
        if v["bottom_gantt"].is_object() {
            self.grid.scale.restore(&v["bottom_gantt"]);
        }
        if let Some(columns) = columns_from_json(&v["bottom_columns"]) {
            self.grid.columns = columns;
        }
    }
}

/// Fill a rectangle on whole pixels.
fn fill(p: &egui::Painter, rect: Rect, color: Color32) {
    chrome::fill(p, rect, color);
}

impl App {
    pub(crate) fn restore_panes(&mut self) {
        if let Ok(Some(json)) = self.store.preference(PREF_KEY) {
            if let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&json) {
                crate::theme::legacy::upgrade_panes(&mut value);
                self.panes.restore(&value);
            }
        }
        self.panes.snapshot = self.panes.json_with(false).to_string();
    }

    /// Save the pane choices when they change.
    pub(crate) fn persist_panes(&mut self) {
        let json = self.panes.json_with(false).to_string();
        if json != self.panes.snapshot {
            match self.store.set_preference(PREF_KEY, &json) {
                Ok(()) => self.panes.snapshot = json,
                Err(e) => self.error(format!("Cannot save the pane layout: {e}")),
            }
        }
    }

    /// Apply the panes part of an opened layout.
    pub(crate) fn apply_layout_panes(
        &mut self,
        bottom: Option<&str>,
        panes: Option<&serde_json::Value>,
    ) {
        if let Some(v) = panes {
            self.panes.restore(v);
        }
        if let Some(key) = bottom {
            self.panes.bottom = BottomView::from_key(key).unwrap_or(BottomView::Details);
        }
    }

    /// Show `view` in the bottom pane, or hide the pane for `None`.
    pub(crate) fn set_bottom(&mut self, view: Option<BottomView>) {
        match view {
            None => self.show_details = false,
            Some(v) => {
                self.panes.bottom = v;
                self.show_details = true;
                if v == BottomView::TraceLogic {
                    self.panes.trace.synced = None;
                }
                if v.is_table() && self.panes.grid.columns.is_empty() {
                    self.panes.grid.columns = self.grid.layout.columns.clone();
                }
            }
        }
    }

    pub(crate) fn bottom_showing(&self, view: BottomView) -> bool {
        self.show_details && self.panes.bottom == view
    }

    pub(crate) fn set_top(&mut self, view: TopView) {
        if self.panes.top != view {
            self.panes.top = view;
            if view == TopView::Network {
                self.panes.network.synced = None;
                if self.panes.network.options.zoom_to_fit {
                    self.network_fit();
                }
            }
        }
    }

    /// Show on Bottom: No Bottom Layout, then every bottom view. `details`
    /// is the first view's caption (the View menu says `Details`).
    pub(crate) fn bottom_menu_items(&mut self, ui: &mut egui::Ui, details: &str) {
        if self.item(
            ui,
            "menu_no_bottom",
            Item::new("No Bottom Layout").checked(!self.show_details),
        ) {
            self.set_bottom(None);
        }
        for view in BottomView::MENU {
            let caption = if view == BottomView::Details {
                details
            } else {
                view.caption()
            };
            let action = match view {
                BottomView::Details => "menu_bottom_details".to_string(),
                BottomView::TraceLogic => "menu_bottom_trace".to_string(),
                v => format!("menu_bottom_{}", v.key()),
            };
            let item = Item::new(caption)
                .icon(view.icon())
                .checked(self.bottom_showing(view));
            if self.item(ui, &action, item) {
                self.set_bottom(Some(view));
            }
        }
    }

    /// Show on Top: Table (the table alone), Gantt Chart (the table with
    /// the chart), the two spreadsheets and Activity Network. The Resource
    /// Usage Spreadsheet is disabled in the Activities view, as in the
    /// reference.
    pub(crate) fn top_menu_items(&mut self, ui: &mut egui::Ui) {
        let table = Item::new("Table")
            .icon(Icon::Table)
            .checked(self.panes.top == TopView::Table);
        if self.item(ui, "menu_top_table", table) {
            self.set_top(TopView::Table);
        }
        let gantt = Item::new("Gantt Chart")
            .icon(Icon::Gantt)
            .checked(self.panes.top == TopView::Gantt);
        if self.item(ui, "menu_top_gantt", gantt) {
            self.set_top(TopView::Gantt);
        }
        let usage = Item::new("Activity Usage Spreadsheet")
            .icon(Icon::ActivityUsageSheet)
            .checked(self.panes.top == TopView::ActivityUsageSheet);
        if self.item(ui, "menu_top_activity_usage", usage) {
            self.set_top(TopView::ActivityUsageSheet);
        }
        let resources = Item::new("Resource Usage Spreadsheet")
            .icon(Icon::ResourceUsageSheet)
            .checked(self.panes.top == TopView::ResourceUsageSheet);
        if self.item(ui, "menu_top_resource_usage", resources) {
            self.set_top(TopView::ResourceUsageSheet);
        }
        let network = Item::new("Activity Network")
            .icon(Icon::Network)
            .checked(self.panes.top == TopView::Network);
        if self.item(ui, "menu_top_network", network) {
            self.set_top(TopView::Network);
        }
    }

    /// The Activity Network submenu of View and the layout options menu.
    pub(crate) fn network_menu_items(&mut self, ui: &mut egui::Ui) {
        if self.item(
            ui,
            "menu_network_options",
            Item::new("Activity Network Options..."),
        ) {
            self.open_network_options();
        }
        let network = self.panes.top == TopView::Network;
        if self.item(
            ui,
            "menu_network_reorganize",
            Item::new("Reorganize").enabled(network),
        ) {
            self.network_reorganize();
        }
        let tree = Item::new("WBS Tree")
            .checked(self.panes.network.show_tree)
            .enabled(network);
        if self.item(ui, "menu_network_tree", tree) {
            self.panes.network.show_tree = !self.panes.network.show_tree;
        }
    }

    /// Bottom Layout Options: the options of the view in the bottom pane.
    pub(crate) fn open_bottom_layout_options(&mut self) {
        use usage_fields::FieldsView;
        match self.panes.bottom {
            BottomView::TraceLogic => self.open_trace_options(),
            BottomView::ActivityTable | BottomView::Gantt => self.open_bottom_columns(),
            // The usage views open the options their own right-click menu
            // offers.
            BottomView::ActivityUsageSheet => {
                self.open_spreadsheet_fields(FieldsView::ActivitySheet)
            }
            BottomView::ResourceUsageSheet => {
                self.open_spreadsheet_fields(FieldsView::ResourceSheet)
            }
            BottomView::ActivityUsageProfile | BottomView::ResourceUsageProfile => {
                self.open_profile_options()
            }
            BottomView::Details => self.open_customize_details(),
        }
    }

    /// The Bottom Layout toolbar: Details and the five built views; a
    /// checked button hides the pane again.
    pub(crate) fn bottom_tool_buttons(&mut self, ui: &mut egui::Ui) {
        let activities = self.views.main.is_activities();
        for (view, name) in [
            (BottomView::Details, "details"),
            (BottomView::ActivityUsageSheet, "tool_activity_usage_sheet"),
            (
                BottomView::ActivityUsageProfile,
                "tool_activity_usage_profile",
            ),
            (BottomView::ResourceUsageSheet, "tool_resource_usage_sheet"),
            (
                BottomView::ResourceUsageProfile,
                "tool_resource_usage_profile",
            ),
            (BottomView::TraceLogic, "trace"),
        ] {
            let on = activities && self.bottom_showing(view);
            if self.tool_ex(ui, name, view.icon(), view.caption(), activities, on, false) {
                self.set_bottom(if on { None } else { Some(view) });
            }
        }
    }

    /// The bottom pane of the Activities view.
    pub(crate) fn bottom_pane(&mut self, host: &mut egui::Ui) {
        if self.panes.bottom == BottomView::Details {
            self.activity_details(host);
            return;
        }
        // Same panel id as Activity Details so the pane keeps its height
        // when the view changes.
        egui::TopBottomPanel::bottom(self.activity_pane_id())
            .default_height(230.)
            .min_height(155.)
            .max_height((host.available_rect_before_wrap().height() - 150.).max(155.))
            .resizable(true)
            .show_separator_line(false)
            .frame(egui::Frame::NONE.fill(chrome::pal::VIEW_MARGIN))
            .show_inside(host, |ui| {
                let outer = ui.max_rect();
                let p = ui.painter();
                // 1 px #A0A0A0 frame around a white body, under the 6 px
                // raised splitter that resizes the pane.
                fill(p, outer, chrome::pal::SHADOW);
                let body = Rect::from_min_max(
                    outer.min + Vec2::new(1., crate::activities::SPLITTER),
                    outer.max - Vec2::splat(1.),
                );
                fill(p, body, Color32::WHITE);
                crate::activities::paint_splitter(
                    p,
                    Rect::from_min_size(
                        outer.min,
                        Vec2::new(outer.width(), crate::activities::SPLITTER),
                    ),
                    false,
                );
                self.panes.body = body;
                let width = ui.available_width();
                let table_w = crate::activities::table_width(width, self.grid_fraction);
                match self.panes.bottom {
                    BottomView::ActivityTable => self.bottom_table(ui, body, false),
                    BottomView::Gantt => self.bottom_table(ui, body, true),
                    BottomView::TraceLogic => self.trace_logic_pane(ui, body),
                    BottomView::ActivityUsageSheet => self.activity_usage_sheet(ui, body, table_w),
                    BottomView::ActivityUsageProfile => {
                        self.activity_usage_profile(ui, body, table_w)
                    }
                    BottomView::ResourceUsageSheet => self.resource_usage_sheet(ui, body, table_w),
                    BottomView::ResourceUsageProfile => {
                        self.resource_usage_profile(ui, body, table_w)
                    }
                    BottomView::Details => {}
                }
                ui.allocate_rect(outer, Sense::hover());
            });
    }

    /// Put the bottom table's state on `App` (and the top table's here), or
    /// back. Rows, filter, grouping, sorting and band styles stay shared:
    /// only the columns, caches and hit geometry change places.
    fn swap_bottom_grid(&mut self) {
        let mut cache = self.panes.grid.cache.take().unwrap_or_default();
        std::mem::swap(&mut self.grid, &mut cache);
        // `self.grid` now holds the other pane's caches, `cache` this one's
        // layout and workspace lists: move those across.
        std::mem::swap(&mut self.grid.layout, &mut cache.layout);
        std::mem::swap(&mut self.grid.library, &mut cache.library);
        std::mem::swap(&mut self.grid.user_filters, &mut cache.user_filters);
        std::mem::swap(&mut self.grid.dialogs, &mut cache.dialogs);
        std::mem::swap(&mut self.grid.snapshot, &mut cache.snapshot);
        std::mem::swap(&mut self.grid.save_at, &mut cache.save_at);
        std::mem::swap(&mut self.grid.pending_edit, &mut cache.pending_edit);
        std::mem::swap(&mut self.grid.layout.columns, &mut self.panes.grid.columns);
        self.panes.grid.cache = Some(cache);
        let b = &mut self.panes.grid;
        std::mem::swap(&mut self.bar_geometry, &mut b.bar_geometry);
        std::mem::swap(&mut self.row_geometry, &mut b.row_geometry);
        std::mem::swap(&mut self.link_geometry, &mut b.link_geometry);
        std::mem::swap(
            &mut self.gantt_ui.collapsed_geometry,
            &mut b.collapsed_geometry,
        );
        std::mem::swap(&mut self.gantt_left, &mut b.gantt_left);
        std::mem::swap(&mut self.edit_cell, &mut b.edit_cell);
        std::mem::swap(&mut self.edit_text, &mut b.edit_text);
        std::mem::swap(&mut self.edit_focus, &mut b.edit_focus);
        std::mem::swap(&mut self.link_drag, &mut b.link_drag);
        std::mem::swap(&mut self.layout_opts.heights, &mut b.heights);
        // The bottom Gantt's own split and timescale.
        let s = &mut b.scale;
        let mut origin = s.timeline_origin.unwrap_or(self.timeline_origin);
        std::mem::swap(&mut self.grid_fraction, &mut s.grid_fraction);
        std::mem::swap(&mut self.timeline_origin, &mut origin);
        s.timeline_origin = Some(origin);
        std::mem::swap(&mut self.pixels_per_day, &mut s.pixels_per_day);
        std::mem::swap(&mut self.chart_width, &mut s.chart_width);
        std::mem::swap(&mut self.view.upper, &mut s.upper);
        std::mem::swap(&mut self.view.lower, &mut s.lower);
        std::mem::swap(&mut self.view.automatic_units, &mut s.automatic_units);
        b.drawing = !b.drawing;
    }

    /// Show on Bottom > Activity Table (`chart` false) and Gantt Chart: the
    /// top pane's rows with the bottom pane's own columns, on the shared
    /// selection and timescale.
    fn bottom_table(&mut self, ui: &mut egui::Ui, body: Rect, chart: bool) {
        if self.panes.grid.columns.is_empty() {
            self.panes.grid.columns = self.grid.layout.columns.clone();
        }
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(body)
                .layout(Layout::top_down(Align::LEFT)),
        );
        child.set_clip_rect(body.intersect(ui.clip_rect()));
        let pending = self.editing.pending;
        if !self.panes.grid.scale.own {
            // First shown: start from the top pane's split and timescale.
            let s = &mut self.panes.grid.scale;
            s.grid_fraction = self.grid_fraction;
            s.timeline_origin = Some(self.timeline_origin);
            s.pixels_per_day = self.pixels_per_day;
            s.chart_width = self.chart_width;
            (s.upper, s.lower) = (self.view.upper, self.view.lower);
            s.automatic_units = self.view.automatic_units;
            s.own = true;
        }
        self.swap_bottom_grid();
        // View > Zoom on the bottom Gantt, now that its timescale is here.
        if let Some(request) = self.panes.grid.pending_zoom.take().filter(|_| chart) {
            match request {
                Some(zoom_in) => self.zoom_ladder(zoom_in),
                None => self.fit(),
            }
        }
        // This frame's hit geometry replaces the last frame's.
        self.bar_geometry.clear();
        self.row_geometry.clear();
        self.link_geometry.clear();
        self.table_and_gantt(&mut child, chart, true);
        let (offsets, top) = (self.grid.row_offsets.clone(), self.grid.body_top);
        self.swap_bottom_grid();
        self.panes.grid.row_offsets = offsets;
        self.panes.grid.body_top = top;
        self.panes.grid.header = (body.top(), self.gantt_header_height());
        // Columns... from the bottom table's menu edits its own columns.
        if pending != self.editing.pending
            && self.editing.pending == Some(editing::Command::Columns)
        {
            self.editing.pending = pending;
            self.open_bottom_columns();
        }
    }

    /// The Columns dialog for the bottom table.
    pub(crate) fn open_bottom_columns(&mut self) {
        if self.panes.grid.columns.is_empty() {
            self.panes.grid.columns = self.grid.layout.columns.clone();
        }
        self.open_columns_dialog();
        if let Some(d) = self.grid.dialogs.columns.as_mut() {
            d.columns = self.panes.grid.columns.clone();
            d.bottom = true;
        }
    }

    /// The top pane of the Activities view.
    pub(crate) fn top_pane(&mut self, host: &mut egui::Ui) {
        match self.panes.top {
            TopView::Gantt | TopView::Table => self.activity_view(host),
            TopView::Network => self.network_view(host),
            TopView::ActivityUsageSheet => self.top_usage_sheet(host, false),
            TopView::ResourceUsageSheet => self.top_usage_sheet(host, true),
        }
    }

    /// Show on Top > Activity Usage Spreadsheet or Resource Usage
    /// Spreadsheet (`resources`): the bottom pane's spreadsheet filling the
    /// top pane, on the shared timeline.
    fn top_usage_sheet(&mut self, host: &mut egui::Ui, resources: bool) {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(chrome::pal::VIEW_MARGIN))
            .show_inside(host, |ui| {
                let outer = ui.max_rect();
                let p = ui.painter();
                fill(p, outer, chrome::pal::SHADOW);
                let body = outer.shrink(1.);
                fill(p, body, Color32::WHITE);
                let width = ui.available_width();
                let table_w = crate::activities::table_width(width, self.grid_fraction);
                if resources {
                    self.resource_usage_sheet(ui, body, table_w);
                } else {
                    self.activity_usage_sheet(ui, body, table_w);
                }
                ui.allocate_rect(outer, Sense::hover());
            });
    }

    pub(crate) fn pane_dialogs(&mut self, ctx: &egui::Context) {
        // Escape closes the topmost pane dialog without applying it.
        if ctx.input(|i| i.key_pressed(egui::Key::Escape))
            && self.panes.network_copy.take().is_none()
            && self.panes.network_font.take().is_none()
            && self.panes.network_dialog.take().is_none()
        {
            self.panes.trace_dialog = None;
        }
        self.trace_options_dialog(ctx);
        self.network_options_dialog(ctx);
        self.network_font_dialog(ctx);
        self.network_copy_dialog(ctx);
        self.usage_dialogs_escape(ctx);
        self.usage_dialogs(ctx);
    }

    /// Zoom To Best Fit for the pane on top.
    pub(crate) fn zoom_to_best_fit(&mut self) {
        self.focused_fit();
    }
}
