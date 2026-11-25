//! The activity table: column registry, filters, grouping and sorting,
//! named layouts, find/replace and in-place editing. The Gantt half of the
//! Activities view stays in `activities.rs`; both share one row list.
use super::*;
#[cfg(test)]
mod align_render;
pub mod band;
pub(crate) mod cell_combo;
pub mod columns;
pub mod columns_ev;
pub mod columns_ext;
pub(crate) mod dialogs;
pub(crate) mod edit;
pub mod filters;
pub mod group;
pub mod interval;
pub mod layout;
pub mod marks;
#[cfg(test)]
mod marks_render;
pub mod presets;
pub mod relative;
#[cfg(test)]
mod review_renders;
pub(crate) mod sort_keys;
pub mod starters;
#[cfg(test)]
mod starters_tests;
mod table;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_cache;
#[cfg(test)]
mod tests_ev_techniques;
#[cfg(test)]
pub(crate) mod tests_h3;

pub use columns::Column;
#[cfg(test)]
pub use columns::Value;
use group::{Band, Scope};
use std::cell::RefCell;

/// Preference keys.
pub const CURRENT_KEY: &str = "activity_layout_current_v1";
pub const LIBRARY_KEY: &str = "activity_layouts_v1";
pub const FILTERS_KEY: &str = "activity_filters_v1";
/// The view preference whose old column order and widths are migrated.
const OLD_VIEW_KEY: &str = "activities_view_v1";

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Row {
    /// Index into the bands of the last `rows()` call.
    Group(usize),
    Activity(usize),
}

/// One line of an activity picker that follows the table: a grouping band
/// or an activity (an index into `App::shown()`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PickLine {
    /// `key` names the band among the picker's collapsed bands.
    Band {
        level: usize,
        label: String,
        key: String,
        collapsed: bool,
    },
    Activity(usize),
}

/// The band expand box of the activity table, for pickers that show bands.
pub(crate) use table::expand_box;

/// Table state kept on the app.
#[derive(Default)]
pub struct GridState {
    /// The working layout (what the table shows now).
    pub layout: layout::Layout,
    /// Saved named layouts.
    pub library: Vec<layout::Layout>,
    /// User-defined filters.
    pub user_filters: Vec<filters::UserFilter>,
    /// Bands of the last row build; `Row::Group` indexes this.
    pub bands: RefCell<Vec<Band>>,
    pub highlighted: RefCell<BTreeSet<usize>>,
    pub column_drag: Option<usize>,
    /// The focused column of the selected row (column id).
    pub focus_column: Option<String>,
    /// Horizontal scroll of the table in pixels.
    pub scroll_x: f32,
    /// An in-place edit finished during the row pass: (activity, column, text).
    pub pending_edit: Option<(usize, String, String)>,
    pub snapshot: String,
    pub save_at: Option<f64>,
    pub dialogs: dialogs::Dialogs,
    frame: RefCell<Option<table::FrameCache>>,
    /// The last row build and what it depended on, reused while none of it
    /// changes (see `rows`).
    rows_cache: RefCell<Option<RowsCache>>,
    /// The column lookups of the last frames and what they were built for,
    /// reused while none of it changes (see `shared_index`).
    index_cache: RefCell<Option<IndexCache>>,
    /// The project's undo snapshot serial, worked out once while the table
    /// and Gantt rows are painted (nothing changes the project there), so
    /// the many row and column lookups of one pass do not each compare the
    /// whole project with the snapshot. Empty outside that pass.
    serial_pin: std::cell::Cell<Option<Option<u64>>>,
    /// Row tops from the first row's top in the last painted frame, plus the
    /// total height; the table and the Gantt share these rows.
    pub row_offsets: Vec<f32>,
    /// Screen y of the first row's top in the last painted frame.
    pub body_top: f32,
    /// Rows wholly in view in the last painted frame: PageUp and PageDown
    /// move by this many.
    pub page_rows: usize,
    /// In-place editing and band selection (`edit.rs`).
    pub cells: edit::CellState,
}

impl GridState {
    /// Forget the last row build, so the next one starts afresh.
    pub(crate) fn drop_rows_cache(&self) {
        self.rows_cache.borrow_mut().take();
        self.index_cache.borrow_mut().take();
    }
}

/// Column lookups (`columns::Index`) built for one undo snapshot of the
/// project and one critical definition, baseline set, set of Check Schedule
/// thresholds and shown critical paths. The lookups fill in lazily as columns ask for them
/// and stay valid while all of these are unchanged.
struct IndexCache {
    serial: u64,
    critical: (bool, f64),
    baselines: Option<std::sync::Arc<crate::baselines::Compare>>,
    checks: crate::schedule_check::CheckParams,
    /// The shown Activity Critical Path paths (their serial).
    critical_paths: Option<u64>,
    ix: std::rc::Rc<columns::Index>,
}

fn same_baselines(
    a: &Option<std::sync::Arc<crate::baselines::Compare>>,
    b: &Option<std::sync::Arc<crate::baselines::Compare>>,
) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => std::sync::Arc::ptr_eq(a, b),
        (None, None) => true,
        _ => false,
    }
}

/// Inputs of a row build other than the project state, which the history
/// snapshot serial stands for.
struct RowsCache {
    serial: u64,
    filter: filters::FilterSelection,
    group_sort: layout::GroupSort,
    columns: Vec<layout::ColumnSetting>,
    users: Vec<filters::UserFilter>,
    query: String,
    wbs_filter: Option<i64>,
    collapsed: BTreeSet<String>,
    critical: (bool, f64),
    baselines: Option<std::sync::Arc<crate::baselines::Compare>>,
    formats: format::Formats,
    checks: crate::schedule_check::CheckParams,
    spotlight: Option<(i64, i64)>,
    critical_paths: Option<u64>,
    rows: std::rc::Rc<Vec<Row>>,
}

/// One column of the current layout as shown.
#[cfg(test)]
#[derive(Clone, Debug, PartialEq)]
pub struct TableColumn {
    pub id: String,
    pub title: String,
    pub width: f32,
    pub align: columns::Align,
    pub kind: columns::Kind,
}

/// One row of the current layout as shown.
#[cfg(test)]
#[derive(Clone, Debug, PartialEq)]
pub enum TableRow {
    /// A grouping band: nesting level from 0, caption, and the band totals
    /// per column (empty text where a column has no total).
    Band {
        level: usize,
        label: String,
        totals: Vec<String>,
    },
    /// An activity: model index, `task_id`, typed values and the table text.
    Activity {
        index: usize,
        task_id: i64,
        values: Vec<Value>,
        texts: Vec<String>,
    },
}

/// Exactly what the activity table shows: the current layout's visible
/// columns in order and its rows after filter, group, sort and collapse.
#[cfg(test)]
#[derive(Clone, Debug, PartialEq)]
pub struct TableSnapshot {
    pub layout_name: String,
    pub columns: Vec<TableColumn>,
    pub rows: Vec<TableRow>,
}

#[cfg(test)]
thread_local! {
    /// Row builds (filter, group and sort runs) on this thread, so a test
    /// can check how many a frame costs.
    pub(crate) static ROW_BUILDS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

impl App {
    pub(crate) fn grid_index(&self) -> columns::Index {
        let mut ix = columns::Index::new(self.critical_definition());
        ix.baselines = Some(self.baseline_compare());
        ix.check_params = self.sched.check_params.clone();
        ix.critical_paths = self.critical_path_view();
        ix
    }

    /// The undo snapshot serial the project is exactly at, when only one
    /// project is open (with others open, keys built on it would not see
    /// their changes).
    pub(crate) fn shown_serial(&self) -> Option<u64> {
        if let Some(pinned) = self.grid.serial_pin.get() {
            return pinned;
        }
        if self.active_rows().is_none() {
            self.history
                .state_serial(&self.project, self.records_revision)
        } else {
            None
        }
    }

    /// Work out the snapshot serial once for a pass that does not change the
    /// project (painting the table and Gantt rows); `unpin_serial` ends it.
    pub(crate) fn pin_serial(&self) {
        self.grid.serial_pin.set(None);
        let serial = self.shown_serial();
        self.grid.serial_pin.set(Some(serial));
    }

    /// The serial worked out for the current painting pass, if one is on.
    pub(crate) fn pinned_serial(&self) -> Option<Option<u64>> {
        self.grid.serial_pin.get()
    }

    /// End the pass started by `pin_serial`: from here on the project may
    /// change, so every lookup compares it again.
    pub(crate) fn unpin_serial(&self) {
        self.grid.serial_pin.set(None);
    }

    /// The column lookups for the project as it is now: the ones of earlier
    /// frames while the project is the same undo snapshot and the critical
    /// definition, baselines and check thresholds are unchanged, so the
    /// table does not rescan the XER tables and recompute earned value and
    /// band totals each frame.
    pub(crate) fn shared_index(&self) -> std::rc::Rc<columns::Index> {
        self.shared_index_at(self.shown_serial())
    }

    /// `shared_index` with the project's snapshot serial already known.
    fn shared_index_at(&self, serial: Option<u64>) -> std::rc::Rc<columns::Index> {
        let ix = self.grid_index();
        if let (Some(serial), Some(c)) = (serial, self.grid.index_cache.borrow().as_ref()) {
            if c.serial == serial
                && c.critical == ix.critical
                && same_baselines(&c.baselines, &ix.baselines)
                && c.checks == ix.check_params
                && c.critical_paths == ix.critical_paths.as_ref().map(|v| v.serial)
            {
                return c.ix.clone();
            }
        }
        let ix = std::rc::Rc::new(ix);
        *self.grid.index_cache.borrow_mut() = serial.map(|serial| IndexCache {
            serial,
            critical: ix.critical,
            baselines: ix.baselines.clone(),
            checks: ix.check_params.clone(),
            critical_paths: ix.critical_paths.as_ref().map(|v| v.serial),
            ix: ix.clone(),
        });
        ix
    }

    /// Columns of the current layout that exist for this project, with the
    /// layout's title, width and alignment applied.
    pub(crate) fn visible_columns(&self, catalog: &[Column]) -> Vec<Column> {
        self.grid
            .layout
            .columns
            .iter()
            .filter_map(|s| {
                columns::find(catalog, &s.id).map(|c| {
                    let mut c = c.clone();
                    c.width = s.width;
                    if let Some(t) = &s.title {
                        c.title = t.clone();
                    }
                    if let Some(a) = s.align {
                        c.align = a;
                    }
                    c
                })
            })
            .collect()
    }

    pub(crate) fn build_rows(&self, ix: &columns::Index, catalog: &[Column]) -> group::Built {
        group::build(
            self.shown(),
            ix,
            catalog,
            &self.grid.layout,
            &Scope {
                query: &self.query,
                wbs_filter: self.wbs_filter,
                collapsed: &self.collapsed,
                users: &self.grid.user_filters,
            },
        )
    }

    /// The lines of an activity picker: every activity whose ID or name
    /// contains `query`, under the bands and in the order the table's Group
    /// and Sort put them now. A picker offers every activity, so the
    /// layout's filter, the WBS filter and the table's collapsed bands do
    /// not apply; `collapsed` holds the bands folded in the picker itself.
    /// Activities `skip` names are left out, and so is a band left empty.
    pub(crate) fn picker_lines(
        &self,
        query: &str,
        collapsed: &BTreeSet<String>,
        skip: &dyn Fn(usize) -> bool,
    ) -> Vec<PickLine> {
        let shown = self.shown();
        let catalog = columns::catalog(shown);
        let ix = self.shared_index_at(self.shown_serial());
        let layout = layout::Layout {
            filter: filters::FilterSelection::default(),
            ..self.grid.layout.clone()
        };
        let built = group::build(
            shown,
            &ix,
            &catalog,
            &layout,
            &Scope {
                query,
                wbs_filter: None,
                collapsed,
                users: &self.grid.user_filters,
            },
        );
        built
            .rows
            .iter()
            .filter_map(|row| match *row {
                Row::Activity(k) => (!skip(k)).then_some(PickLine::Activity(k)),
                Row::Group(b) => {
                    let band = &built.bands[b];
                    band.members
                        .iter()
                        .any(|&m| !skip(m))
                        .then(|| PickLine::Band {
                            level: band.level,
                            label: band.label.clone(),
                            key: band.key.clone(),
                            collapsed: collapsed.contains(&band.key),
                        })
                }
            })
            .collect()
    }

    /// Table rows of the active project in display order (rows of other open
    /// projects are left out, so every index addresses `App.project`). Also
    /// refreshes the band list.
    pub(crate) fn rows(&self) -> Vec<Row> {
        let rows = self.shown_rows_shared();
        match self.active_rows() {
            Some(n) => rows
                .iter()
                .copied()
                .filter(|r| !matches!(r, Row::Activity(i) if *i >= n))
                .collect(),
            None => std::rc::Rc::unwrap_or_clone(rows),
        }
    }

    /// Every table row in display order, other open projects included
    /// (indices into `App::shown()`). Also refreshes the band list. The app
    /// itself uses `shown_rows_shared`.
    #[cfg(test)]
    pub(crate) fn shown_rows(&self) -> Vec<Row> {
        std::rc::Rc::unwrap_or_clone(self.shown_rows_shared())
    }

    /// `shown_rows` without a copy: while the layout keeps its regular order
    /// and no usage or loop view narrows the rows, this is the cached build
    /// itself.
    pub(crate) fn shown_rows_shared(&self) -> std::rc::Rc<Vec<Row>> {
        let rows = self.reorganized_shared(self.built_rows_shared());
        let rows = self.time_period_rows_shared(rows);
        if self.loops.view.is_none() || self.open_set.multi() {
            return rows;
        }
        std::rc::Rc::new(self.loop_view_rows(std::rc::Rc::unwrap_or_clone(rows)))
    }

    /// `time_period_rows` that hands `rows` back untouched when no usage
    /// period or resource filter applies.
    fn time_period_rows_shared(&self, rows: std::rc::Rc<Vec<Row>>) -> std::rc::Rc<Vec<Row>> {
        let (period, members) = self.usage_table_filter();
        if period.is_none() && members.is_none() {
            return rows;
        }
        std::rc::Rc::new(self.time_period_rows(std::rc::Rc::unwrap_or_clone(rows)))
    }

    /// The rows as filter, group and sort place them now.
    pub(crate) fn built_rows(&self) -> Vec<Row> {
        std::rc::Rc::unwrap_or_clone(self.built_rows_shared())
    }

    /// `built_rows` sharing the cached list instead of copying it.
    pub(crate) fn built_rows_shared(&self) -> std::rc::Rc<Vec<Row>> {
        // The build is reused while only one project is open and that project
        // is exactly the current undo snapshot (see `RowsCache`); with other
        // projects open the key below would not see their changes.
        let serial = self.shown_serial();
        let ix = self.shared_index_at(serial);
        let layout = &self.grid.layout;
        if let (Some(serial), Some(c)) = (serial, self.grid.rows_cache.borrow().as_ref()) {
            if c.serial == serial
                && same_baselines(&c.baselines, &ix.baselines)
                && c.critical == ix.critical
                && c.filter == layout.filter
                && c.group_sort == layout.group_sort
                && c.columns == layout.columns
                && c.users == self.grid.user_filters
                && c.query == self.query
                && c.wbs_filter == self.wbs_filter
                && c.collapsed == self.collapsed
                && c.formats == self.formats
                && c.checks == ix.check_params
                && c.spotlight == self.spotlight_window()
                && c.critical_paths == ix.critical_paths.as_ref().map(|v| v.serial)
            {
                return c.rows.clone();
            }
        }
        #[cfg(test)]
        ROW_BUILDS.with(|n| n.set(n.get() + 1));
        let catalog = columns::catalog(self.shown());
        let built = self.build_rows(&ix, &catalog);
        let rows = std::rc::Rc::new(built.rows);
        *self.grid.bands.borrow_mut() = built.bands;
        let mut highlighted = built.highlighted;
        highlighted.extend(self.spotlight_activities());
        *self.grid.highlighted.borrow_mut() = highlighted;
        *self.grid.rows_cache.borrow_mut() = serial.map(|serial| RowsCache {
            serial,
            filter: layout.filter.clone(),
            group_sort: layout.group_sort.clone(),
            columns: layout.columns.clone(),
            users: self.grid.user_filters.clone(),
            query: self.query.clone(),
            wbs_filter: self.wbs_filter,
            collapsed: self.collapsed.clone(),
            critical: ix.critical,
            baselines: ix.baselines.clone(),
            formats: self.formats.clone(),
            checks: ix.check_params.clone(),
            spotlight: self.spotlight_window(),
            critical_paths: ix.critical_paths.as_ref().map(|v| v.serial),
            rows: rows.clone(),
        });
        rows
    }

    /// Activities of the active project under band `index` of the last row build.
    pub(crate) fn band_members(&self, index: usize) -> Vec<usize> {
        let mut members = self.band_members_shown(index);
        if let Some(n) = self.active_rows() {
            members.retain(|&i| i < n);
        }
        members
    }

    /// Activities of every open project under band `index` (indices into
    /// `App::shown()`).
    pub(crate) fn band_members_shown(&self, index: usize) -> Vec<usize> {
        self.grid
            .bands
            .borrow()
            .get(index)
            .map(|b| b.members.to_vec())
            .unwrap_or_default()
    }

    /// Caption of band `index` of the last row build.
    pub(crate) fn band_label(&self, index: usize) -> String {
        self.grid
            .bands
            .borrow()
            .get(index)
            .map(|b| b.label.clone())
            .unwrap_or_default()
    }

    /// Is band `index` of the last row build collapsed?
    pub(crate) fn band_collapsed(&self, index: usize) -> bool {
        self.grid
            .bands
            .borrow()
            .get(index)
            .is_some_and(|b| self.collapsed.contains(&b.key))
    }

    /// The current layout as shown, as the tests read it.
    #[cfg(test)]
    pub(crate) fn current_layout_table(&self) -> TableSnapshot {
        let shown = self.shown();
        let catalog = columns::catalog(shown);
        let ix = self.grid_index();
        let cols = self.visible_columns(&catalog);
        let built = self.build_rows(&ix, &catalog);
        let rows = built
            .rows
            .iter()
            .map(|row| match *row {
                Row::Group(b) => {
                    let band = &built.bands[b];
                    TableRow::Band {
                        level: band.level,
                        label: band.label.clone(),
                        totals: cols
                            .iter()
                            .map(|c| {
                                if self.grid.layout.group_sort.show_group_totals {
                                    group::band_total(shown, &ix, c, &band.members)
                                        .map(|v| columns::display(shown, None, &v))
                                        .unwrap_or_default()
                                } else {
                                    String::new()
                                }
                            })
                            .collect(),
                    }
                }
                Row::Activity(i) => {
                    let values: Vec<Value> = cols
                        .iter()
                        .map(|c| columns::value(shown, &ix, c, i))
                        .collect();
                    TableRow::Activity {
                        index: i,
                        task_id: shown.model.activities[i].task_id,
                        texts: cols
                            .iter()
                            .map(|c| {
                                let v = columns::shown_value(shown, &ix, c, i);
                                columns::display(shown, Some(i), &v)
                            })
                            .collect(),
                        values,
                    }
                }
            })
            .collect();
        TableSnapshot {
            layout_name: self.grid.layout.name.clone(),
            columns: cols
                .into_iter()
                .map(|c| TableColumn {
                    id: c.id,
                    title: c.title,
                    width: c.width,
                    align: c.align,
                    kind: c.kind,
                })
                .collect(),
            rows,
        }
    }

    pub(crate) fn jump_to(&mut self, activity: usize) {
        if activity >= self.project.model.activities.len() {
            return;
        }
        self.selected = Some(activity);
        // Say what had to change to show it, so a filter does not vanish
        // unnoticed.
        let mut changed = vec![];
        if !self.query.is_empty() {
            self.query.clear();
            changed.push("the find text cleared");
        }
        if self.wbs_filter.take().is_some() {
            changed.push("the WBS filter cleared");
        }
        if self.reveal_activity(activity) {
            changed.push("the filter set to All Activities");
        }
        if !changed.is_empty() {
            self.status = format!(
                "To show {}: {}.",
                self.project.model.activities[activity].task_code,
                changed.join(", ")
            );
        }
        self.scroll_to_selected = true;
        self.date_edits.clear();
        if let Some(start) = displayed_start(
            &self.project.model.activities[activity],
            self.project.model.options.do_level,
        ) {
            let end =
                self.timeline_origin + (self.chart_width / self.pixels_per_day * DAY as f32) as i64;
            if start < self.timeline_origin || start > end {
                self.timeline_origin = start.div_euclid(DAY) * DAY - 2 * DAY;
            }
        }
    }

    /// Make an activity visible: clear a hiding filter and expand its bands.
    /// Returns true when the filter had to change.
    pub(crate) fn reveal_activity(&mut self, activity: usize) -> bool {
        let mut filter_changed = false;
        let visible = |app: &App| {
            app.rows()
                .iter()
                .any(|r| matches!(r, Row::Activity(i) if *i == activity))
        };
        if !visible(self) && !self.grid.layout.filter.highlight {
            let ix = self.grid_index();
            let catalog = columns::catalog(&self.project);
            if !filters::selection_matches(
                &self.project,
                &ix,
                &catalog,
                &self.grid.layout.filter,
                &self.grid.user_filters,
                activity,
            ) {
                filter_changed = !self.grid.layout.filter.all_activities;
                self.grid.layout.filter.all_activities = true;
            }
        }
        if !visible(self) {
            // Expand every band holding the activity.
            let keys: Vec<String> = self
                .grid
                .bands
                .borrow()
                .iter()
                .filter(|b| b.members.contains(&activity))
                .map(|b| b.key.clone())
                .collect();
            for key in keys {
                self.collapsed.remove(&key);
            }
        }
        filter_changed
    }

    /// Expand All: every band of the table, or of the Activity Network
    /// while that is on top.
    pub(crate) fn expand_all_bands(&mut self) {
        if self.panes.top == crate::panes::TopView::Network {
            self.network_expand_all();
            return;
        }
        self.collapsed.clear();
    }

    /// Collapse All: every band of the table, or of the Activity Network
    /// while that is on top.
    pub(crate) fn collapse_all_bands(&mut self) {
        if self.panes.top == crate::panes::TopView::Network {
            self.network_collapse_all();
            return;
        }
        self.collapsed.clear();
        let _ = self.rows();
        self.collapsed = self
            .grid
            .bands
            .borrow()
            .iter()
            .map(|b| b.key.clone())
            .collect();
    }

    /// Load the working layout, saved layouts and user filters; migrate the
    /// column order and widths of the older view preference once.
    pub(crate) fn restore_layout(&mut self) {
        if let Ok(Some(text)) = self.store.preference(FILTERS_KEY) {
            match layout::filters_from_json(&text) {
                Ok(f) => self.grid.user_filters = f,
                Err(e) => self.status = format!("Saved filters ignored: {e}"),
            }
        }
        if let Ok(Some(text)) = self.store.preference(LIBRARY_KEY) {
            match layout::library_from_json(&text) {
                Ok(l) => self.grid.library = l,
                Err(e) => self.status = format!("Saved layouts ignored: {e}"),
            }
        }
        self.install_starter_content();
        let current = self
            .store
            .preference(CURRENT_KEY)
            .ok()
            .flatten()
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            .and_then(|v| layout::Layout::from_json(&v).ok());
        match current {
            Some(mut l) => {
                crate::theme::legacy::upgrade_bands(&mut l.group_sort);
                self.grid.layout = l;
            }
            None => {
                self.grid.layout = layout::Layout::default();
                self.migrate_old_columns();
            }
        }
        self.grid.snapshot = self.grid.layout.to_json().to_string();
        let _ = self
            .store
            .set_preference(CURRENT_KEY, &self.grid.snapshot.clone());
    }

    fn migrate_old_columns(&mut self) {
        let Ok(Some(text)) = self.store.preference(OLD_VIEW_KEY) else {
            return;
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
            return;
        };
        let old_ids = [
            "activity_id",
            "activity_name",
            "remaining_duration",
            "start",
            "finish",
            "total_float",
            "activity_status",
            "free_float",
        ];
        let order = serde_json::from_value::<[usize; 8]>(v["column_order"].clone()).ok();
        let widths = serde_json::from_value::<[f32; 8]>(v["column_widths"].clone()).ok();
        let Some(order) = order.filter(|o| {
            let mut s = *o;
            s.sort();
            s == [0, 1, 2, 3, 4, 5, 6, 7]
        }) else {
            return;
        };
        // Old widths were fractions of the table pane (about 900 px wide).
        let table = 1500. * v["grid_fraction"].as_f64().unwrap_or(0.6).clamp(0.3, 0.8) as f32;
        let defaults = layout::earlier_columns();
        self.grid.layout.columns = order
            .iter()
            .map(|&field| {
                let id = old_ids[field];
                let width = widths
                    .filter(|w| w.iter().all(|x| x.is_finite() && *x > 0.))
                    .map(|w| (w[field] * table).clamp(30., 800.))
                    .or_else(|| defaults.iter().find(|c| c.id == id).map(|c| c.width))
                    .unwrap_or(80.);
                layout::ColumnSetting::new(id, width.round())
            })
            .collect();
    }

    pub(crate) fn save_layout(&mut self) {
        let json = self.grid.layout.to_json().to_string();
        match self.store.set_preference(CURRENT_KEY, &json) {
            Ok(()) => {
                self.grid.snapshot = json;
                self.grid.save_at = None;
            }
            Err(e) => self.error(format!("Cannot save the layout: {e}")),
        }
    }

    pub(crate) fn save_library(&mut self) {
        let text = layout::library_to_json(&self.grid.library);
        if let Err(e) = self.store.set_preference(LIBRARY_KEY, &text) {
            self.error(format!("Cannot save layouts: {e}"));
        }
    }

    pub(crate) fn save_user_filters(&mut self) {
        let text = layout::filters_to_json(&self.grid.user_filters);
        if let Err(e) = self.store.set_preference(FILTERS_KEY, &text) {
            self.error(format!("Cannot save filters: {e}"));
        }
    }

    /// Persist the working layout shortly after it changes.
    pub(crate) fn persist_layout_when_idle(&mut self, ctx: &egui::Context) {
        let json = self.grid.layout.to_json().to_string();
        let now = ctx.input(|i| i.time);
        if self.grid.snapshot != json {
            self.grid.snapshot = json;
            self.grid.save_at = Some(now + 0.4);
        }
        if let Some(deadline) = self.grid.save_at {
            if now >= deadline {
                self.save_layout();
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(400));
            }
        }
    }

    /// The working layout with the current Gantt view and bottom pane.
    pub(crate) fn capture_layout(&self, name: &str) -> layout::Layout {
        let mut l = self.grid.layout.clone();
        l.name = name.into();
        // The Gantt part: the view (zoom, bands, row height, splitter) and the
        // Bars, Bar Chart Options and Timescale settings.
        l.gantt = Some(serde_json::json!({
            "view": serde_json::from_str::<serde_json::Value>(&self.view_json()).ok(),
            "settings": serde_json::from_str::<serde_json::Value>(&self.gantt.to_json()).ok(),
        }));
        l.bottom = Some(layout::Bottom {
            show_details: self.show_details,
            tab: self.detail.tab.key().into(),
            view: self.panes.bottom.key().into(),
        });
        l.panes = Some(self.panes.json());
        l
    }

    /// Is the working layout different from its saved copy? The columns,
    /// filters, grouping and sorting count, and so do the Bars, Bar Chart
    /// Options and Timescale settings; the view's zoom, scroll and
    /// splitter positions do not.
    pub(crate) fn layout_changed(&self) -> bool {
        let saved = self
            .grid
            .library
            .iter()
            .find(|l| l.name == self.grid.layout.name)
            .cloned()
            .or_else(|| {
                (self.grid.layout.name == layout::STANDARD_NAME).then(layout::Layout::default)
            });
        let mut current = self.grid.layout.clone();
        current.gantt = None;
        current.bottom = None;
        current.panes = None;
        saved.is_none_or(|mut s| {
            let gantt = s.gantt.take();
            s.bottom = None;
            s.panes = None;
            s.content_key() != current.content_key() || self.gantt_settings_changed(gantt.as_ref())
        })
    }

    /// Do the Gantt settings differ from those a saved layout's Gantt part
    /// holds? A layout without stored settings (the standard layout before
    /// it is saved) compares with the default settings; one saved before
    /// settings were kept with layouts holds nothing to compare with.
    fn gantt_settings_changed(&self, saved: Option<&serde_json::Value>) -> bool {
        let stored = match saved {
            Some(g) if g.get("view").is_none() => return false,
            Some(g) => match g.get("settings").filter(|v| v.is_object()) {
                Some(v) => gantt::settings::GanttSettings::from_json(&v.to_string()).ok(),
                None => return false,
            },
            None => Some(gantt::settings::GanttSettings::default()),
        };
        let normal = |g: &gantt::settings::GanttSettings| {
            serde_json::from_str::<serde_json::Value>(&g.to_json()).ok()
        };
        stored.is_some_and(|stored| normal(&stored) != normal(&self.gantt))
    }

    /// Save the working layout under `name`, replacing a layout of that name.
    pub(crate) fn save_layout_as(&mut self, name: &str) -> Result<(), String> {
        let name = layout::check_name(name)?;
        let captured = self.capture_layout(&name);
        // Round trip through the stored form so only valid layouts are kept.
        let checked = layout::Layout::from_json(&captured.to_json())?;
        match self
            .grid
            .library
            .iter_mut()
            .find(|l| l.name.eq_ignore_ascii_case(&name))
        {
            Some(slot) => *slot = checked,
            None => self.grid.library.push(checked),
        }
        self.grid.layout.name = name.clone();
        self.save_library();
        self.save_layout();
        self.status = format!("Layout {name} saved");
        Ok(())
    }

    /// Open a saved layout (or the standard layout) by name.
    pub(crate) fn open_layout(&mut self, name: &str, use_filter: bool) -> Result<(), String> {
        let layout = self
            .grid
            .library
            .iter()
            .find(|l| l.name == name)
            .cloned()
            .or_else(|| (name == layout::STANDARD_NAME).then(layout::Layout::default))
            .ok_or_else(|| format!("No layout named {name}."))?;
        // Check the Gantt part before anything changes.
        let gantt_settings = match layout.gantt.as_ref().map(|g| &g["settings"]) {
            Some(v) if v.is_object() => Some(
                gantt::settings::GanttSettings::from_json(&v.to_string())
                    .map_err(|e| format!("The layout's Gantt settings were rejected: {e}"))?,
            ),
            _ => None,
        };
        let filter = self.grid.layout.filter.clone();
        self.grid.layout = layout.clone();
        if !use_filter {
            self.grid.layout.filter = filter;
        }
        if let Some(g) = &layout.gantt {
            // Layouts saved before the Gantt settings existed hold the view alone.
            let view = if g.get("view").is_some() {
                &g["view"]
            } else {
                g
            };
            if view.is_object()
                && self
                    .store
                    .set_preference(OLD_VIEW_KEY, &view.to_string())
                    .is_ok()
            {
                self.restore_view();
                self.update_timescale_units();
            }
            if let Some(settings) = gantt_settings {
                self.apply_gantt(settings)?;
            }
        }
        self.apply_layout_panes(
            layout.bottom.as_ref().map(|b| b.view.as_str()),
            layout.panes.as_ref(),
        );
        if let Some(b) = &layout.bottom {
            self.show_details = b.show_details;
            if let Some(tab) = DetailTab::ALL
                .into_iter()
                .find(|t| t.key() == b.tab && self.detail.tabs.contains(t))
            {
                self.detail.tab = tab;
            }
        }
        self.collapsed.clear();
        self.grid.scroll_x = 0.;
        self.save_layout();
        self.status = format!("Layout {name} opened");
        Ok(())
    }

    pub(crate) fn delete_layout(&mut self, name: &str) -> Result<(), String> {
        let before = self.grid.library.len();
        self.grid.library.retain(|l| l.name != name);
        if self.grid.library.len() == before {
            return Err(format!("No saved layout named {name}."));
        }
        self.save_library();
        Ok(())
    }

    /// A saved layout (or the standard layout) by name.
    pub(crate) fn saved_layout(&self, name: &str) -> Option<layout::Layout> {
        self.grid
            .library
            .iter()
            .find(|l| l.name == name)
            .cloned()
            .or_else(|| (name == layout::STANDARD_NAME).then(layout::Layout::default))
    }

    /// Open Layout > Export: write layout `name` to a layout file, whole or
    /// not at all.
    pub(crate) fn export_layout_file(&mut self, name: &str, path: &Path) -> Result<(), String> {
        let layout = self
            .saved_layout(name)
            .ok_or_else(|| format!("No layout named {name}."))?;
        openxer_xer::write_file_atomic(path, layout::file_text(&layout).as_bytes())
            .map_err(|e| format!("Cannot write {}: {e}", path.display()))?;
        self.status = format!("Layout {name} exported to {}", path.display());
        Ok(())
    }

    /// Open Layout > Import: add the layout in a layout file to the saved
    /// layouts. A name already in use gets a number. Returns the name.
    pub(crate) fn import_layout_file(&mut self, path: &Path) -> Result<String, String> {
        use std::io::Read;
        let file = std::fs::File::open(path)
            .map_err(|e| format!("Cannot open {}: {e}", path.display()))?;
        let mut text = String::new();
        file.take(layout::MAX_FILE_BYTES + 1)
            .read_to_string(&mut text)
            .map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
        let mut imported = layout::from_file_text(&text)?;
        // The Gantt part is checked now, not when the layout is opened.
        if let Some(v) = imported.gantt.as_ref().map(|g| &g["settings"]) {
            if v.is_object() {
                gantt::settings::GanttSettings::from_json(&v.to_string()).map_err(|e| {
                    format!("Invalid layout: the Gantt settings were rejected: {e}")
                })?;
            }
        }
        let taken = |n: &str| {
            n.eq_ignore_ascii_case(layout::STANDARD_NAME)
                || self
                    .grid
                    .library
                    .iter()
                    .any(|l| l.name.eq_ignore_ascii_case(n))
        };
        let base = imported.name.clone();
        let mut name = base.clone();
        let mut n = 2;
        while taken(&name) {
            let suffix = format!(" ({n})");
            let keep = 100 - suffix.chars().count();
            name = format!("{}{suffix}", base.chars().take(keep).collect::<String>());
            n += 1;
        }
        imported.name = name.clone();
        self.grid.library.push(imported);
        self.save_library();
        self.status = format!("Layout {name} imported");
        Ok(name)
    }

    /// Names in the Open layout::Layout list: the standard layout, then saved ones.
    pub(crate) fn layout_names(&self) -> Vec<String> {
        let mut names = vec![];
        if !self
            .grid
            .library
            .iter()
            .any(|l| l.name == layout::STANDARD_NAME)
        {
            names.push(layout::STANDARD_NAME.to_string());
        }
        let mut saved: Vec<String> = self.grid.library.iter().map(|l| l.name.clone()).collect();
        saved.sort_by_key(|n| n.to_lowercase());
        names.extend(saved);
        names
    }
}
