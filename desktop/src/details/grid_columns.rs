//! Column customization for the Activity Details grids, the project record
//! views (Expenses, WPs & Docs, Risks, Issues, Thresholds) and the Resources
//! view's rate grid: each list's context menu opens Customize ... Columns,
//! where its Available Options move to and from its Selected Options and are
//! put in order; Default puts the list's own column set back. A details
//! grid's choice is kept per grid in the workspace preferences; a view's is
//! kept with that view's layout (its column order). Nothing here touches the
//! project.
use super::*;
use crate::available_options;
use crate::context_menu::Menu;

/// Preference row holding every grid's chosen columns.
const KEY: &str = "detail_grid_columns_v1";

/// One details grid: (title, width, cell index) for every column it can
/// show, and the titles it shows by default.
pub struct GridColumns {
    pub key: &'static str,
    /// The menu caption names the grid: `Customize Steps Columns...`.
    pub caption: &'static str,
    pub available: &'static [(&'static str, f32, usize)],
    /// The titles shown by default (the earlier defaults, see
    /// `crate::defaults`).
    pub default: &'static [&'static str],
    /// The out-of-the-box titles where they differ from `default`.
    pub reference: Option<&'static [&'static str]>,
    /// A grid whose stored choice this grid reads while it has none of its
    /// own (the Relationships tab halves read what was chosen when they
    /// shared the Predecessors and Successors tabs' choice).
    pub inherits: Option<&'static str>,
}

pub const STEPS: GridColumns = GridColumns {
    key: "steps",
    caption: "Steps",
    available: &[
        ("Step Name", 200., 0),
        ("% Complete", 50., 2),
        ("Completed", 62., 1),
        ("Step Weight", 70., 3),
        ("Step Weight Percent", 110., 4),
    ],
    default: &["Step Name", "% Complete"],
    reference: Some(crate::defaults::STEP_COLUMNS),
    inherits: None,
};

pub const CODES: GridColumns = GridColumns {
    key: "codes",
    caption: "Activity Code",
    available: &[
        ("Activity Code", 100., 0),
        ("Code Value", 100., 1),
        ("Description", 200., 2),
    ],
    default: &["Activity Code", "Code Value", "Description"],
    reference: None,
    inherits: None,
};

pub const EXPENSES: GridColumns = GridColumns {
    key: "expenses",
    caption: "Expense",
    available: &[
        ("Expense Item", 150., 0),
        ("Cost Account", 100., 11),
        ("Expense Category", 100., 1),
        ("Accrual Type", 100., 10),
        ("Budgeted Cost", 80., 2),
        ("Actual Cost", 80., 3),
        ("Remaining Cost", 80., 4),
        ("At Completion Cost", 80., 5),
        ("Vendor", 150., 9),
        ("Auto Compute Actuals", 80., 12),
        ("Budgeted Units", 80., 6),
        ("Price / Unit", 75., 7),
        ("Unit of Measure", 85., 8),
    ],
    default: &[
        "Expense Item",
        "Cost Account",
        "Expense Category",
        "Accrual Type",
        "Budgeted Cost",
        "Actual Cost",
        "Remaining Cost",
        "At Completion Cost",
        "Vendor",
        "Auto Compute Actuals",
    ],
    reference: Some(crate::defaults::EXPENSE_COLUMNS),
    inherits: None,
};

pub const DOCUMENTS: GridColumns = GridColumns {
    key: "documents",
    caption: "Document Assignment",
    available: &[
        ("Title", 550., 0),
        ("Reference No.", 120., 1),
        ("Status Code", 100., 2),
        ("Work Product", 100., 3),
    ],
    default: &["Title", "Status Code", "Work Product"],
    reference: None,
    inherits: None,
};

pub const RISKS: GridColumns = GridColumns {
    key: "risks",
    caption: "Risk",
    available: &[
        ("Risk ID", 150., 0),
        ("Risk Name", 150., 1),
        ("Risk Category", 150., 2),
        ("Risk Type", 150., 3),
        ("Risk Status", 150., 4),
        ("Risk Owner", 150., 5),
    ],
    default: &[
        "Risk ID",
        "Risk Name",
        "Risk Category",
        "Risk Type",
        "Risk Status",
        "Risk Owner",
    ],
    reference: None,
    inherits: None,
};

const RELATIONSHIP_COLUMNS: &[(&str, f32, usize)] = &[
    ("Project ID", 80., 9),
    ("WBS", 80., 10),
    ("Activity ID", 80., 0),
    ("Activity Name", 200., 1),
    ("Relationship Type", 50., 2),
    ("Lag", 50., 3),
    ("Activity Status", 80., 4),
    ("Primary Resource", 120., 11),
    ("Comments", 120., 12),
    ("Start", 76., 5),
    ("Finish", 76., 6),
    ("Driving", 46., 7),
    ("Critical", 46., 8),
];
const RELATIONSHIP_DEFAULT: &[&str] = &[
    "Project ID",
    "WBS",
    "Activity ID",
    "Activity Name",
    "Relationship Type",
    "Lag",
    "Activity Status",
    "Primary Resource",
    "Comments",
];

/// The Predecessors tab.
pub const PREDECESSORS: GridColumns = GridColumns {
    key: "predecessors",
    caption: "Predecessor",
    available: RELATIONSHIP_COLUMNS,
    default: RELATIONSHIP_DEFAULT,
    reference: Some(crate::defaults::LINK_TAB_COLUMNS),
    inherits: None,
};

/// The Successors tab.
pub const SUCCESSORS: GridColumns = GridColumns {
    key: "successors",
    caption: "Successor",
    available: RELATIONSHIP_COLUMNS,
    default: RELATIONSHIP_DEFAULT,
    reference: Some(crate::defaults::LINK_TAB_COLUMNS),
    inherits: None,
};

/// The Predecessors half of the Relationships tab.
pub const RELATIONSHIP_PREDECESSORS: GridColumns = GridColumns {
    key: "relationship_predecessors",
    caption: "Predecessor",
    available: RELATIONSHIP_COLUMNS,
    default: RELATIONSHIP_DEFAULT,
    reference: None,
    inherits: Some("predecessors"),
};

/// The Successors half of the Relationships tab.
pub const RELATIONSHIP_SUCCESSORS: GridColumns = GridColumns {
    key: "relationship_successors",
    caption: "Successor",
    available: RELATIONSHIP_COLUMNS,
    default: RELATIONSHIP_DEFAULT,
    reference: None,
    inherits: Some("successors"),
};

/// The Resources tab: the reference's default columns and widths first (the
/// numbers are the data column indexes of the tab's cells), then the columns
/// OpenXER adds, as the shared assignment column catalog lists them
/// (`views::assignment_columns`, which the Resource Assignments window uses
/// too). Remaining Start and Remaining Finish follow the assignment's lag
/// and duration (a started activity's remaining start cannot move).
pub const RESOURCES: GridColumns = GridColumns {
    key: "resources",
    caption: "Resource",
    // The shared assignment column catalog's Resources tab fields.
    available: &crate::views::assignment_columns::TAB_COLUMNS,
    default: &[
        "Resource ID Name",
        "Price / Unit",
        "Rate Type",
        "Rate Source",
        "Primary Resource",
        "Budgeted Units",
        "Actual Units",
        "Remaining Units",
        "Remaining Units / Time",
        "Role",
        "Cost Account",
    ],
    reference: None,
    inherits: None,
};

const ALL: [&GridColumns; 10] = [
    &RESOURCES,
    &STEPS,
    &CODES,
    &EXPENSES,
    &DOCUMENTS,
    &RISKS,
    &PREDECESSORS,
    &SUCCESSORS,
    &RELATIONSHIP_PREDECESSORS,
    &RELATIONSHIP_SUCCESSORS,
];

impl GridColumns {
    fn column(&self, title: &str) -> Option<(&'static str, f32, usize)> {
        self.available.iter().copied().find(|c| c.0 == title)
    }
    /// The titles the grid shows when nothing was chosen.
    pub fn defaults(&self) -> &'static [&'static str] {
        match self.reference {
            Some(titles) if crate::defaults::reference() => titles,
            _ => self.default,
        }
    }
    pub fn menu_caption(&self) -> String {
        format!("Customize {} Columns...", self.caption)
    }
}

/// The Available Options header: bold title and a `v` chevron, 18 px high.
fn options_header(ui: &mut egui::Ui, title: &str) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(170., 18.), Sense::click());
    let painter = ui.painter();
    if response.hovered() {
        painter.rect_filled(rect, 0., Color32::from_gray(0xE4));
    }
    painter.text(
        rect.left_center() + Vec2::new(2., 0.),
        Align2::LEFT_CENTER,
        title,
        chrome::bold_font(),
        INK,
    );
    chrome::chevron(
        painter,
        Pos2::new(rect.right() - 12., rect.center().y - 2.),
        true,
        INK,
    );
    response
}

/// The group heading the Category view shows before item `n` of the
/// Available Options (`{key}_group`).
fn group_head(
    ui: &mut egui::Ui,
    heads: &[(usize, &str)],
    n: usize,
    key: &str,
    rects: &mut Vec<(String, egui::Response)>,
) {
    for (_, head) in heads.iter().filter(|h| h.0 == n) {
        let h = ui.label(RichText::new(*head).strong());
        rects.push((format!("{key}_group"), h));
    }
}

/// Whose columns the dialog edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColumnTarget {
    /// An Activity Details grid, by its key.
    Detail(&'static str),
    /// A project record view, by its view id (`expenses`, `risks`...).
    Record(&'static str),
    /// The Resources view's rate grid.
    ResourceRates,
    /// The Projects view table.
    Projects,
    /// The WBS view table.
    Wbs,
    /// The Resources view table.
    Resources,
    /// The Resource Assignments window's table.
    Assignments,
}

/// The Customize Columns dialog while open.
#[derive(Clone, Debug, PartialEq)]
pub struct ColumnDialog {
    pub target: ColumnTarget,
    /// The window title names the list: `Customize <caption> Columns`.
    pub caption: String,
    /// Every column the list can show, in its natural order: the list's
    /// own, then its User Defined columns.
    pub available: Vec<String>,
    /// Ids of the User Defined columns, the last of `available`
    /// (`views::udf_columns`); the Category view lists them under a
    /// "User Defined" heading.
    pub udfs: Vec<String>,
    /// The Category view's group of the columns, in `available` order
    /// (empty: the own columns stand without groups; a column past the
    /// end of the list is a User Defined one).
    pub groups: Vec<String>,
    /// What Default puts back.
    pub default: Vec<String>,
    /// Selected Options, in order.
    pub selected: Vec<String>,
    pub pick_available: Option<usize>,
    pub pick_selected: Option<usize>,
    /// Start Available Options at the top when newly opened or regrouped.
    pub scroll_to_top: bool,
}

impl ColumnDialog {
    /// The Selected Options as a view table's fields: its own by index,
    /// its User Defined ones by id (`views::grid::Columns::show_fields`).
    pub fn shown_fields(&self) -> Vec<Result<usize, String>> {
        let own = self.available.len() - self.udfs.len();
        self.selected
            .iter()
            .filter_map(|t| self.available.iter().position(|a| a == t))
            .map(|i| match i.checked_sub(own) {
                Some(k) => Err(self.udfs[k].clone()),
                None => Ok(i),
            })
            .collect()
    }

    /// Whether `title` is one of the User Defined columns.
    fn is_udf(&self, title: &str) -> bool {
        let own = self.available.len() - self.udfs.len();
        self.available[own..].iter().any(|a| a == title)
    }
}

/// The grid's context menu item; true when clicked (the caller opens the dialog).
pub fn menu_item(ui: &mut egui::Ui, grid: &GridColumns) -> bool {
    let caption = grid.menu_caption();
    Menu::on(ui, &format!("{}_grid_menu", grid.key)).item("customize", chrome::Item::new(&caption))
}

impl App {
    /// The columns `grid` shows: every available column with the grid's
    /// Show All option, else the chosen set, else its default set.
    pub(crate) fn detail_columns(
        &self,
        grid: &GridColumns,
        all: bool,
    ) -> Vec<(&'static str, f32, usize)> {
        if all {
            return grid.available.to_vec();
        }
        let chosen = &self.detail.grid_columns;
        let own = chosen.get(grid.key);
        let stored = own.or_else(|| chosen.get(grid.inherits?));
        let defaults = grid.defaults();
        match stored {
            Some(titles) => titles.iter().filter_map(|t| grid.column(t)).collect(),
            None => defaults.iter().filter_map(|t| grid.column(t)).collect(),
        }
    }

    pub(crate) fn open_column_customize(&mut self, grid: &GridColumns) {
        let selected = self
            .detail_columns(grid, false)
            .into_iter()
            .map(|c| c.0.to_string())
            .collect();
        self.detail.column_dialog = Some(ColumnDialog {
            target: ColumnTarget::Detail(grid.key),
            caption: grid.caption.to_string(),
            available: grid.available.iter().map(|c| c.0.to_string()).collect(),
            udfs: vec![],
            groups: vec![],
            default: grid.defaults().iter().map(|t| t.to_string()).collect(),
            selected,
            pick_available: None,
            pick_selected: None,
            scroll_to_top: true,
        });
    }

    /// Customize Columns for a view list: `available` in the list's own
    /// order, `shown` the titles it shows now, in order. Default shows them
    /// all.
    pub(crate) fn open_view_columns(
        &mut self,
        target: ColumnTarget,
        caption: &str,
        available: &[&str],
        default: &[&str],
        shown: Vec<String>,
    ) {
        self.open_view_columns_with(target, caption, available, &[], default, shown);
    }

    /// [`Self::open_view_columns`] with the list's User Defined columns
    /// (`udfs`: id and title), offered after its own under "User Defined".
    pub(crate) fn open_view_columns_with(
        &mut self,
        target: ColumnTarget,
        caption: &str,
        available: &[&str],
        udfs: &[(String, String)],
        default: &[&str],
        shown: Vec<String>,
    ) {
        self.detail.column_dialog = Some(ColumnDialog {
            target,
            caption: caption.to_string(),
            available: available
                .iter()
                .map(|t| t.to_string())
                .chain(udfs.iter().map(|u| u.1.clone()))
                .collect(),
            udfs: udfs.iter().map(|u| u.0.clone()).collect(),
            groups: vec![],
            default: default.iter().map(|t| t.to_string()).collect(),
            selected: shown,
            pick_available: None,
            pick_selected: None,
            scroll_to_top: true,
        });
    }

    /// OK in the dialog: the chosen columns go to their list.
    fn apply_columns(&mut self, d: ColumnDialog) {
        match d.target {
            ColumnTarget::Detail(key) => {
                // A grid that reads another grid's choice keeps its own
                // choice even when it is the default.
                let inherits = ALL
                    .iter()
                    .find(|g| g.key == key)
                    .and_then(|g| g.inherits)
                    .is_some_and(|k| self.detail.grid_columns.contains_key(k));
                if d.selected == d.default && !inherits {
                    self.detail.grid_columns.remove(key);
                } else {
                    self.detail.grid_columns.insert(key.into(), d.selected);
                }
                self.save_grid_columns();
            }
            ColumnTarget::Record(id) => {
                let shown = d.shown_fields();
                if let Some(state) = crate::views::record_view::list_state(&mut self.views, id) {
                    state
                        .columns
                        .show_fields(&shown, crate::views::udf_columns::WIDTH);
                }
            }
            ColumnTarget::Projects => {
                let shown = d.shown_fields();
                self.views
                    .projects
                    .columns
                    .show_fields(&shown, crate::views::udf_columns::WIDTH);
            }
            ColumnTarget::Wbs => {
                let shown = d.shown_fields();
                self.views
                    .wbs
                    .columns
                    .show_fields(&shown, crate::views::udf_columns::WIDTH);
            }
            ColumnTarget::Resources => self.apply_resource_columns(&d.selected),
            ColumnTarget::Assignments => {
                let shown = d.shown_fields();
                self.views
                    .assignments
                    .columns
                    .show_fields(&shown, crate::views::udf_columns::WIDTH);
            }
            ColumnTarget::ResourceRates => {
                self.views.resources.rate_columns = if d.selected == d.default {
                    vec![]
                } else {
                    d.selected
                };
            }
        }
    }

    /// Read the chosen columns from the workspace preferences.
    pub(crate) fn restore_grid_columns(&mut self) {
        let Ok(Some(json)) = self.store.preference(KEY) else {
            return;
        };
        let Ok(map) = serde_json::from_str::<BTreeMap<String, Vec<String>>>(&json) else {
            return;
        };
        self.detail.grid_columns = map
            .into_iter()
            .filter_map(|(key, titles)| {
                let grid = ALL.iter().find(|g| g.key == key)?;
                let titles: Vec<String> = titles
                    .into_iter()
                    .filter(|t| grid.column(t).is_some())
                    .collect();
                (!titles.is_empty()).then_some((key, titles))
            })
            .collect();
    }

    fn save_grid_columns(&mut self) {
        let json = serde_json::to_string(&self.detail.grid_columns).unwrap_or_default();
        if let Err(e) = self.store.set_preference(KEY, &json) {
            self.error(format!("Cannot save the grid columns: {e}"));
        }
    }

    /// Customize ... Columns: Available Options and Selected Options with
    /// the arrows between them, Default, OK and Cancel. Escape cancels.
    pub(crate) fn column_customize_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.detail.column_dialog.take() else {
            return;
        };
        // Escape closes an open Available Options menu before the dialog.
        let menu_was_open = available_options::menu_open(ctx);
        let view = self.available_view;
        let flat = view.is_list();
        let mut available: Vec<&str> = d
            .available
            .iter()
            .map(String::as_str)
            .filter(|t| !d.selected.iter().any(|s| s == t))
            .collect();
        // Category: the list's own columns, then its User Defined ones under
        // their heading; List: one alphabetical list.
        let mut heads: Vec<(usize, &str)> = vec![];
        if flat {
            available_options::alphabetical(&mut available, |t| t);
        } else if d.groups.is_empty() {
            available.sort_by_key(|t| d.is_udf(t));
            if let Some(at) = available.iter().position(|t| d.is_udf(t)) {
                heads.push((at, crate::views::udf_columns::GROUP));
            }
        } else {
            // Grouped columns: groups and the columns in each in
            // alphabetical order; a column without a group of its own is a
            // User Defined one.
            let by_title: std::collections::HashMap<&str, &str> = d
                .available
                .iter()
                .zip(&d.groups)
                .map(|(t, g)| (t.as_str(), g.as_str()))
                .collect();
            let group_of = |title: &str| {
                by_title
                    .get(title)
                    .copied()
                    .unwrap_or(crate::views::udf_columns::GROUP)
            };
            available.sort_by_cached_key(|t| (group_of(t).to_lowercase(), t.to_lowercase()));
            let mut last = None;
            for (n, t) in available.iter().enumerate() {
                let group = group_of(t);
                if last != Some(group) {
                    heads.push((n, group));
                    last = Some(group);
                }
            }
        }
        let mut answer = None;
        let mut chosen_view = None;
        let mut menu_actions = vec![];
        let to_top = std::mem::take(&mut d.scroll_to_top);
        crate::chrome::plain_window(format!("Customize {} Columns", d.caption))
            .id(egui::Id::new("customize_detail_columns"))
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.horizontal_top(|ui| {
                    let mut list =
                        |ui: &mut egui::Ui,
                         title: &str,
                         items: &[&str],
                         heads: &[(usize, &str)],
                         pick: &mut Option<usize>,
                         key: &str,
                         menu: bool,
                         reset_scroll: bool,
                         rects: &mut Vec<(String, egui::Response)>| {
                            ui.vertical(|ui| {
                                if menu {
                                    // The header opens the Group and Sort By menu.
                                    let header = options_header(ui, title);
                                    rects.push((format!("{key}_header"), header.clone()));
                                    chosen_view = available_options::header_menu(
                                        &header,
                                        view,
                                        &mut menu_actions,
                                    )
                                    .or(chosen_view);
                                } else {
                                    ui.label(RichText::new(title).strong());
                                }
                                egui::Frame::NONE
                                    .stroke(Stroke::new(1_f32, Color32::from_gray(160)))
                                    .fill(Color32::WHITE)
                                    .show(ui, |ui| {
                                        ui.set_min_size(Vec2::new(170., 220.));
                                        let mut area = egui::ScrollArea::vertical()
                                            .id_salt(key)
                                            .max_height(220.);
                                        if reset_scroll {
                                            area = area.vertical_scroll_offset(0.);
                                        }
                                        area.show(ui, |ui| {
                                            for (n, item) in items.iter().enumerate() {
                                                group_head(ui, heads, n, key, rects);
                                                let r =
                                                    ui.selectable_label(*pick == Some(n), *item);
                                                if r.clicked() {
                                                    *pick = Some(n);
                                                }
                                                rects.push((format!("{key}_{n}"), r));
                                            }
                                        });
                                    });
                            });
                        };
                    let mut rects = vec![];
                    list(
                        ui,
                        "Available Options",
                        &available,
                        &heads,
                        &mut d.pick_available,
                        "detail_columns_available",
                        true,
                        to_top,
                        &mut rects,
                    );
                    let mut moves = vec![];
                    ui.vertical(|ui| {
                        ui.add_space(60.);
                        for (key, label) in [
                            ("detail_columns_add", ">"),
                            ("detail_columns_remove", "<"),
                            ("detail_columns_up", "Up"),
                            ("detail_columns_down", "Down"),
                        ] {
                            let r = chrome::command_button(ui, label, Vec2::new(48., 22.), true);
                            if r.clicked() {
                                moves.push(key);
                            }
                            rects.push((key.to_string(), r));
                        }
                    });
                    let shown: Vec<&str> = d.selected.iter().map(String::as_str).collect();
                    list(
                        ui,
                        "Selected Options",
                        &shown,
                        &[],
                        &mut d.pick_selected,
                        "detail_columns_selected",
                        false,
                        false,
                        &mut rects,
                    );
                    for (name, r) in &rects {
                        self.action(name, r);
                    }
                    self.record_dialog_actions(std::mem::take(&mut menu_actions));
                    for m in moves {
                        match m {
                            "detail_columns_add" => {
                                if let Some(t) = d.pick_available.and_then(|i| available.get(i)) {
                                    let at = d.pick_selected.map_or(d.selected.len(), |i| i + 1);
                                    d.selected.insert(at.min(d.selected.len()), t.to_string());
                                    d.pick_selected = Some(at.min(d.selected.len() - 1));
                                    d.pick_available = None;
                                }
                            }
                            "detail_columns_remove" => {
                                if let Some(i) = d.pick_selected.filter(|&i| i < d.selected.len()) {
                                    if d.selected.len() > 1 {
                                        d.selected.remove(i);
                                        d.pick_selected = None;
                                    }
                                }
                            }
                            "detail_columns_up" => {
                                if let Some(i) =
                                    d.pick_selected.filter(|&i| i > 0 && i < d.selected.len())
                                {
                                    d.selected.swap(i, i - 1);
                                    d.pick_selected = Some(i - 1);
                                }
                            }
                            _ => {
                                if let Some(i) =
                                    d.pick_selected.filter(|&i| i + 1 < d.selected.len())
                                {
                                    d.selected.swap(i, i + 1);
                                    d.pick_selected = Some(i + 1);
                                }
                            }
                        }
                    }
                });
                ui.add_space(6.);
                ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                    ui.spacing_mut().item_spacing.x = crate::dialog_kit::ROW_GAP;
                    // Right to left: Default, Cancel, OK read OK, Cancel, Default.
                    for (key, button, value) in [
                        (
                            "detail_columns_default",
                            chrome::DialogButton::new("Default", None),
                            2,
                        ),
                        ("detail_columns_cancel", chrome::DialogButton::cancel(), 1),
                        ("detail_columns_ok", chrome::DialogButton::ok(), 0),
                    ] {
                        let r = chrome::dialog_button(ui, &button);
                        self.action(key, &r);
                        if r.clicked() {
                            answer = Some(value);
                        }
                    }
                });
            });
        if let Some(view) = chosen_view {
            if view != self.available_view {
                d.scroll_to_top = true;
            }
            self.set_available_view(view);
            // The picked row is a position in the list that was drawn.
            d.pick_available = None;
        }
        match answer {
            Some(0) => {
                self.apply_columns(d);
                available_options::close_menu(ctx);
                return;
            }
            Some(1) => {
                available_options::close_menu(ctx);
                return;
            }
            Some(_) => {
                d.selected = d.default.clone();
                d.pick_selected = None;
            }
            None if !menu_was_open && ctx.input(|i| i.key_pressed(Key::Escape)) => {
                available_options::close_menu(ctx);
                return;
            }
            None => {}
        }
        self.detail.column_dialog = Some(d);
    }
}

#[cfg(test)]
mod tests;
