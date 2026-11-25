//! The Additional Activities View (Project menu, Project bar): a second
//! Activities tab over the same project, with its own layout (columns,
//! filters, grouping, Gantt settings, panes) and its own selection, scroll
//! position and collapsed bands. Edits in either view go to the one project
//! and the one undo history, so they show in the other view at once.
//!
//! The Activities code keeps its state on `App`. That state belongs to the
//! Activities view in use; the other view's state waits here and changes
//! places with it (`use_activities_state`) when that view is drawn or gets
//! the focus. The Activities view's state is swapped back in before the
//! workspace preferences are written, so its saved layout never picks up
//! the other view's. The additional view's layout is saved under its own key.
use super::*;
use crate::dialogs::LinkEditor;
use crate::gantt::{BarGeometry, LinkDrag};

/// Workspace preference holding the additional view's layout.
pub const PREF_KEY: &str = "additional_activities_view_v1";

/// Everything one Activities view shows on its own, in `App`'s field order.
pub struct ActivityViewState {
    /// Project the state was last used with: (source, project key).
    project: (String, i64),
    selected: Option<usize>,
    selection: BTreeSet<i64>,
    anchor: Option<i64>,
    col_anchor: Option<String>,
    detail: details::DetailState,
    query: String,
    wbs_filter: Option<i64>,
    collapsed: BTreeSet<String>,
    pixels_per_day: f32,
    timeline_origin: i64,
    date_edits: BTreeMap<String, String>,
    grid_fraction: f32,
    chart_width: f32,
    edit_cell: Option<(usize, String)>,
    grid: crate::grid::GridState,
    edit_text: String,
    edit_focus: bool,
    show_details: bool,
    link_drag: Option<LinkDrag>,
    link_editor: Option<LinkEditor>,
    scroll_to_selected: bool,
    bar_geometry: Vec<BarGeometry>,
    link_geometry: Vec<(usize, Vec<Pos2>)>,
    gantt_left: f32,
    view: view::ViewSettings,
    gantt: gantt::settings::GanttSettings,
    gantt_ui: gantt::GanttUi,
    view_snapshot: String,
    view_save_at: Option<f64>,
    panes: panes::PanesState,
    /// The view's own frozen row order, measured row heights, spotlight end
    /// and hint (Disable Auto-Reorganization itself applies to both views).
    opts: crate::layout_opts::LayoutOpts,
}

/// The parked Activities view state and the save bookkeeping of the
/// additional view.
#[derive(Default)]
pub struct ActivityViews {
    /// Which Activities view's state is on `App` now.
    on_app: Option<MainView>,
    /// The other one's state, once the additional view has been opened.
    parked: Option<Box<ActivityViewState>>,
    /// Project the state just put on `App` was last used with.
    incoming_project: Option<(String, i64)>,
    snapshot: String,
}

macro_rules! swap_fields {
    ($app:expr, $state:expr; $($field:ident),*) => {
        $(std::mem::swap(&mut $app.$field, &mut $state.$field);)*
    };
}

impl ActivityViewState {
    /// A new view showing what `app`'s current Activities view shows.
    fn fork(app: &App) -> Self {
        let mut grid = crate::grid::GridState::default();
        grid.layout = app.grid.layout.clone();
        grid.focus_column = app.grid.focus_column.clone();
        let mut panes = panes::PanesState::default();
        panes.restore(&app.panes.json());
        let mut detail = details::DetailState::default();
        detail.tab = app.detail.tab;
        detail.tabs = app.detail.tabs.clone();
        Self {
            project: project_key(app),
            selected: app.selected,
            selection: BTreeSet::new(),
            anchor: None,
            col_anchor: None,
            detail,
            query: String::new(),
            wbs_filter: app.wbs_filter,
            collapsed: app.collapsed.clone(),
            pixels_per_day: app.pixels_per_day,
            timeline_origin: app.timeline_origin,
            date_edits: BTreeMap::new(),
            grid_fraction: app.grid_fraction,
            chart_width: app.chart_width,
            edit_cell: None,
            grid,
            edit_text: String::new(),
            edit_focus: false,
            show_details: app.show_details,
            link_drag: None,
            link_editor: None,
            scroll_to_selected: true,
            bar_geometry: vec![],
            link_geometry: vec![],
            gantt_left: app.gantt_left,
            view: app.view.clone(),
            gantt: app.gantt.clone(),
            gantt_ui: Default::default(),
            view_snapshot: String::new(),
            view_save_at: None,
            panes,
            opts: Default::default(),
        }
    }
}

fn project_key(app: &App) -> (String, i64) {
    (
        app.project.source_name.clone(),
        app.project.model.project.proj_id,
    )
}

impl App {
    /// Put `view`'s Activities state on `App` (a no-op for other views and
    /// for the view whose state is already there).
    pub(crate) fn use_activities_state(&mut self, view: MainView) {
        if !view.is_activities() {
            return;
        }
        let on_app = self
            .views
            .activity_views
            .on_app
            .unwrap_or(MainView::Activities);
        if on_app == view {
            self.views.activity_views.on_app = Some(view);
            return;
        }
        let mut state = match self.views.activity_views.parked.take() {
            Some(state) => state,
            None => Box::new(self.new_additional_state()),
        };
        self.swap_activities_state(&mut state);
        self.views.activity_views.parked = Some(state);
        self.views.activity_views.on_app = Some(view);
        self.revalidate_activities_state();
    }

    fn swap_activities_state(&mut self, state: &mut ActivityViewState) {
        std::mem::swap(&mut self.editing.selection, &mut state.selection);
        std::mem::swap(&mut self.editing.anchor, &mut state.anchor);
        std::mem::swap(&mut self.editing.col_anchor, &mut state.col_anchor);
        let mut key = project_key(self);
        std::mem::swap(&mut key, &mut state.project);
        swap_fields!(self, state; selected, detail, query, wbs_filter, collapsed,
            pixels_per_day, timeline_origin, date_edits, grid_fraction, chart_width,
            edit_cell, grid, edit_text, edit_focus, show_details, link_drag, link_editor,
            scroll_to_selected, bar_geometry, link_geometry, gantt_left, view, gantt,
            gantt_ui, view_snapshot, view_save_at, panes);
        let (mine, theirs) = (&mut self.layout_opts, &mut state.opts);
        std::mem::swap(&mut mine.frozen, &mut theirs.frozen);
        std::mem::swap(&mut mine.bands_frozen, &mut theirs.bands_frozen);
        std::mem::swap(&mut mine.heights, &mut theirs.heights);
        std::mem::swap(&mut mine.spotlight_end, &mut theirs.spotlight_end);
        std::mem::swap(&mut mine.hint, &mut theirs.hint);
        // Saved layouts and user filters belong to the workspace, not to a view.
        std::mem::swap(&mut self.grid.library, &mut state.grid.library);
        std::mem::swap(&mut self.grid.user_filters, &mut state.grid.user_filters);
        // The state now on `App` remembers the project it was used with.
        self.views.activity_views.incoming_project = Some(key);
    }

    /// The state that came back may predate a deletion, an undo or another
    /// project: keep its selection valid, and start over on another project.
    fn revalidate_activities_state(&mut self) {
        let key = project_key(self);
        let n = self.project.model.activities.len();
        if self.views.activity_views.incoming_project.take() != Some(key) {
            self.selected = (n > 0).then_some(0);
            self.collapsed.clear();
            self.query.clear();
            self.wbs_filter = None;
            self.editing.selection.clear();
            self.edit_cell = None;
            // The table's band selection, row drag and date calendar
            // (swapped with `grid`) belong to the old project's rows.
            self.grid.cells.selected_band = None;
            self.grid.cells.row_drag = None;
            self.grid.cells.calendar = None;
            self.date_edits.clear();
            self.link_drag = None;
            self.link_editor = None;
            self.fit_project();
        }
        if self.selected.is_some_and(|i| i >= n) {
            self.selected = n.checked_sub(1);
        }
        if self.edit_cell.as_ref().is_some_and(|(i, _)| *i >= n) {
            self.edit_cell = None;
        }
    }

    /// The additional view's first state: its saved layout, or a copy of
    /// the Activities view's.
    fn new_additional_state(&mut self) -> ActivityViewState {
        let mut state = ActivityViewState::fork(self);
        let saved = self
            .store
            .preference(PREF_KEY)
            .ok()
            .flatten()
            .and_then(|json| serde_json::from_str::<serde_json::Value>(&json).ok());
        if let Some(value) = saved {
            if let Ok(layout) = crate::grid::layout::Layout::from_json(&value["layout"]) {
                state.grid.layout = layout;
            }
            if let Some(json) = value["gantt"].as_str() {
                if let Ok(settings) = gantt::settings::GanttSettings::from_json(json) {
                    state.gantt = settings;
                }
            }
            state.panes.restore(&value["panes"]);
            if let Some(show) = value["show_details"].as_bool() {
                state.show_details = show;
            }
            view::apply_view_value(
                &value["view"],
                &mut state.view,
                &mut state.pixels_per_day,
                &mut state.grid_fraction,
            );
        }
        state
    }

    /// The additional view's stored form, read from `App` while its state
    /// is there.
    fn additional_json(&self) -> String {
        serde_json::json!({
            "layout": self.grid.layout.to_json(),
            "gantt": self.gantt.to_json(),
            "panes": self.panes.json(),
            "show_details": self.show_details,
            "view": serde_json::from_str::<serde_json::Value>(&self.view_json()).ok(),
        })
        .to_string()
    }

    /// Run `f` with the Activities view's own state on `App` (preferences
    /// that belong to it are written there).
    pub(crate) fn with_first_activities_state(&mut self, f: impl FnOnce(&mut App)) {
        let on_app = self.views.activity_views.on_app;
        self.use_activities_state(MainView::Activities);
        f(self);
        if let Some(view) = on_app {
            self.use_activities_state(view);
        }
    }

    /// Save the additional view's layout when it changed.
    pub(crate) fn persist_additional_activities(&mut self) {
        if self.views.activity_views.parked.is_none() {
            return;
        }
        let on_app = self.views.activity_views.on_app;
        self.use_activities_state(MainView::Activities2);
        let json = self.additional_json();
        if json != self.views.activity_views.snapshot {
            match self.store.set_preference(PREF_KEY, &json) {
                Ok(()) => self.views.activity_views.snapshot = json,
                Err(e) => self.error(format!("Cannot save the additional view: {e}")),
            }
        }
        if let Some(view) = on_app {
            self.use_activities_state(view);
        }
    }

    /// Id of the Activities view's bottom pane; each view keeps its own
    /// pane height.
    pub(crate) fn activity_pane_id(&self) -> egui::Id {
        match self.views.activity_views.on_app {
            Some(MainView::Activities2) => egui::Id::new(("activity_details", "additional")),
            _ => egui::Id::new("activity_details"),
        }
    }

    /// Project > Additional Activities View and the Project bar button.
    pub(crate) fn open_additional_activities(&mut self) {
        self.show_view(MainView::Activities2);
    }
}
