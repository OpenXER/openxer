//! Shared frame of the project record views (Expenses, WPs & Docs, Risks,
//! Issues and Thresholds): a full-width view table under a flat header
//! button, the icon command bar at the right and the details pane with flat
//! tabs, all on the shared view parts. Also the record helpers the views
//! share (lookups by key, memo and date fields, user defined fields) and the
//! activity / WBS picker window.
use super::grid::{self, Columns, Row};
use super::sheet;
use super::*;
use crate::context_menu::{Entry, Spec};
use crate::records::{self, num};
use crate::schedule_dialogs::Canvas;
pub use openxer_xer::edit::project_records::{
    activity_label, dictionary, label_of, name_of, project_rows, set_value, value, PRIORITY,
};
use openxer_xer::Document;

/// Selection, column layout and details pane of one record view.
#[derive(Clone, Debug)]
pub struct ListState {
    pub columns: Columns,
    pub selected: Option<i64>,
    /// Records selected along with the current one: Delete acts on all of
    /// them.
    pub rows: grid::RowSelection<i64>,
    pub tab: usize,
    pub details_height: f32,
    pub show_details: bool,
    /// Collapsed band or parent rows.
    pub collapsed: BTreeSet<i64>,
    /// Selected row of a grid on a details tab (assignments, issues).
    pub sub_row: Option<usize>,
}

impl ListState {
    /// View `id`'s state, its columns as `defaults::record_columns` starts
    /// them.
    pub fn new(id: &str, titles: &[&str], widths: &[f32]) -> Self {
        let mut columns = Columns::new(widths);
        columns.order = crate::defaults::record_columns(id, titles);
        Self {
            columns,
            selected: None,
            rows: Default::default(),
            tab: 0,
            details_height: 300.,
            show_details: true,
            collapsed: BTreeSet::new(),
            sub_row: None,
        }
    }
    pub fn reset(&mut self) {
        self.selected = None;
        self.rows = Default::default();
        self.collapsed.clear();
        self.sub_row = None;
    }
    pub fn json(&self) -> serde_json::Value {
        serde_json::json!({"columns": self.columns.json(), "tab": self.tab,
            "details_height": self.details_height, "show_details": self.show_details})
    }
    pub fn restore(&mut self, value: &serde_json::Value, tabs: usize) {
        self.columns.restore(&value["columns"]);
        if let Some(tab) = value["tab"].as_u64().filter(|t| (*t as usize) < tabs) {
            self.tab = tab as usize;
        }
        if let Some(show) = value["show_details"].as_bool() {
            self.show_details = show;
        }
        restore_height(&value["details_height"], &mut self.details_height);
    }
}

/// What a row of a record view stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowKey {
    /// A grouping band (WBS element, activity); collapsible.
    Band(i64),
    /// A record, by its id.
    Record(i64),
}

/// Static description of one record view.
pub struct ListSpec<'a> {
    pub id: &'static str,
    /// What the list shows, after "Display: " on the options bar (one
    /// caption rule for every record view).
    pub display: String,
    pub titles: &'a [&'a str],
    pub right: &'a [usize],
    pub tabs: &'a [&'a str],
    /// Command bar buttons: caption, icon and whether enabled.
    pub commands: Vec<(&'static str, chrome::Icon, bool)>,
    pub empty: &'a str,
    /// Caption of the right-click menu's details item, such as `Risk Details`.
    pub details: &'static str,
    /// The Customize Columns window names the list: `Customize <this> Columns`.
    pub customize: &'static str,
    /// Items a view adds to its right-click menu after Delete. Their keys
    /// come back from [`App::record_list_view`] as the command.
    pub extras: Vec<Entry>,
}

/// A record view's right-click menu: the command bar's Add and Delete, the
/// view's own items, the details pane, then Customize Columns.
/// A grouping band row has no record under it, so everything that acts on
/// a record is disabled there.
pub fn record_menu(spec: &ListSpec, shown: bool, on_record: bool) -> Vec<Entry> {
    use chrome::Icon;
    let enabled = |name: &str| spec.commands.iter().any(|(n, _, e)| *n == name && *e);
    let mut entries = vec![
        Entry::Item(
            Spec::new("add", "Add")
                .icon(Icon::Add)
                .enabled(enabled("Add")),
        ),
        Entry::Item(
            Spec::new("delete", "Delete")
                .icon(Icon::Delete)
                .enabled(enabled("Delete") && on_record),
        ),
    ];
    if !spec.extras.is_empty() {
        entries.push(Entry::Separator);
        entries.extend(spec.extras.iter().cloned().map(
            |e| {
                if on_record {
                    e
                } else {
                    disabled(e)
                }
            },
        ));
    }
    entries.push(Entry::Separator);
    entries.push(Entry::Item(
        Spec::new("details", spec.details)
            .icon(Icon::Details)
            .enabled(on_record),
    ));
    entries.push(Entry::Item(
        Spec::new("hide_details", "Hide Detail Window").enabled(shown),
    ));
    entries.push(Entry::Separator);
    entries.push(Entry::Item(Spec::new("customize", "Columns...")));
    entries
}

/// The list state of the record view `id` (`expenses`, `documents`,
/// `risks`, `issues` or `thresholds`).
pub fn list_state<'a>(views: &'a mut Views, id: &str) -> Option<&'a mut ListState> {
    let r = &mut views.records;
    Some(match id {
        "expenses" => &mut r.expenses.list,
        "documents" => &mut r.documents,
        "risks" => &mut r.risks,
        "issues" => &mut r.issues.list,
        "thresholds" => &mut r.thresholds.list,
        _ => return None,
    })
}

/// `entry` with every item switched off.
fn disabled(entry: Entry) -> Entry {
    match entry {
        Entry::Item(spec) => Entry::Item(spec.enabled(false)),
        Entry::Submenu(spec, children) => Entry::Submenu(
            spec.enabled(false),
            children.into_iter().map(disabled).collect(),
        ),
        Entry::Separator => Entry::Separator,
    }
}

/// Accessor of one view's state inside [`Views`].
pub type StateFn = fn(&mut Views) -> &mut ListState;

/// Details tab content: app, sheet canvas, sheet size and tab index.
pub type DetailsFn<'a> = Box<dyn FnOnce(&mut App, &mut Canvas, Vec2, usize) + 'a>;

/// Picker targets: which record an activity or WBS element is chosen for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickTarget {
    ExpenseActivity(i64),
    DocumentActivity(i64),
    DocumentWbs(i64),
    RiskActivity(i64),
    IssueActivity(i64),
    IssueWbs(i64),
    ThresholdWbs(i64),
    /// A work product or document for a WBS element (Assign on the WBS
    /// view's WPs & Docs tab).
    WbsDocument(i64),
    /// The WBS element of an activity, by task ID (the `...` of the WBS
    /// field on the Activity Details General tab).
    ActivityWbs(i64),
}

impl PickTarget {
    fn wbs(self) -> bool {
        matches!(
            self,
            Self::DocumentWbs(_) | Self::IssueWbs(_) | Self::ThresholdWbs(_) | Self::ActivityWbs(_)
        )
    }
}

/// The open activity / WBS picker window.
#[derive(Clone, Debug)]
pub struct Picker {
    pub target: PickTarget,
    pub selected: Option<usize>,
}

pub use openxer_xer::edit::project_records::today;

/// An XER date column shown in the user's date format.
pub fn show_date(origin: i64, text: &str) -> String {
    match openxer_xer::parse_relative(origin, text) {
        Ok(v) => format::date(origin, v),
        Err(_) => text.to_string(),
    }
}

/// A number column shown without trailing zeros.
pub fn show_number(text: &str) -> String {
    if text.trim().is_empty() {
        String::new()
    } else {
        records::format_number(records::float(text))
    }
}

/// `(task_id, Activity ID, Activity Name)` of the open project, by ID.
pub fn activity_choices(p: &ProjectData) -> Vec<(i64, String, String)> {
    let mut list: Vec<_> = p
        .model
        .activities
        .iter()
        .map(|a| {
            let name = p
                .details
                .get(&a.task_id)
                .map_or(String::new(), |d| d.name.clone());
            (a.task_id, a.task_code.clone(), name)
        })
        .collect();
    list.sort_by(|a, b| a.1.cmp(&b.1));
    list
}

/// The full WBS code of an element, as the WBS view's WBS Code column
/// shows it; empty for no element.
pub fn wbs_path(p: &ProjectData, id: i64) -> String {
    if id == 0 {
        return String::new();
    }
    let tree = wbs::Tree::build(p);
    wbs::full_codes(p, &tree).remove(&id).unwrap_or_default()
}

/// `(wbs_id, full code, name)` of the open project's WBS in outline order,
/// without the project node.
pub fn wbs_choices(p: &ProjectData) -> Vec<(i64, String, String)> {
    let tree = wbs::Tree::build(p);
    let preorder = tree.preorder();
    let codes = wbs::full_codes_in(p, &tree, &preorder);
    let project = wbs::project_node(p);
    preorder
        .into_iter()
        .filter(|(id, _)| Some(*id) != project)
        .map(|(id, _)| {
            let name = p
                .wbs
                .iter()
                .find(|w| w.id == id)
                .map_or(String::new(), |w| w.name.clone());
            (id, codes.get(&id).cloned().unwrap_or_default(), name)
        })
        .collect()
}

/// User defined field rows `(label, data type, value)` of record `fk_id` in
/// `table` (the UDFTYPE `table_name`).
pub fn udf_rows(doc: &Document, table: &str, fk_id: i64) -> Vec<Vec<String>> {
    let Some(types) = doc.table("UDFTYPE") else {
        return vec![];
    };
    types
        .rows
        .iter()
        .filter(|r| types.value(r, "table_name").eq_ignore_ascii_case(table))
        .map(|r| {
            let type_id = num(types.value(r, "udf_type_id"));
            let kind = types.value(r, "logical_data_type").to_string();
            let value = doc
                .table("UDFVALUE")
                .and_then(|v| {
                    v.rows
                        .iter()
                        .find(|row| {
                            num(v.value(row, "udf_type_id")) == type_id
                                && num(v.value(row, "fk_id")) == fk_id
                        })
                        .map(|row| {
                            [
                                v.value(row, "udf_text"),
                                v.value(row, "udf_number"),
                                v.value(row, "udf_date"),
                            ]
                            .into_iter()
                            .find(|s| !s.is_empty())
                            .unwrap_or("")
                            .to_string()
                        })
                })
                .unwrap_or_default();
            vec![
                types.value(r, "udf_type_label").to_string(),
                kind.trim_start_matches("FT_").replace('_', " "),
                value,
            ]
        })
        .collect()
}

/// The User Defined Fields tab: a read-only grid.
pub fn udf_tab(c: &mut Canvas, key: &str, w: f32, h: f32, rows: &[Vec<String>]) {
    let cols = [
        ("User Defined Field", 200., false),
        ("Data Type", 150., false),
        ("Value", 250., false),
    ];
    sheet::table(c, key, 1., 1., w - 2., h - 2., &cols, rows, None);
}

/// A memo filling a sheet area: white sunken box with a multi-line edit.
/// `html` memos are shown as text and written back as simple HTML; plain
/// memos are stored as typed. Returns the new stored value when edited.
#[allow(clippy::too_many_arguments)]
pub fn memo(
    c: &mut Canvas,
    key: &str,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    stored: &str,
    html: bool,
) -> Option<String> {
    let rect = c.r(x, y, w, h);
    c.sunken(rect, Color32::WHITE);
    let inner = rect.shrink2(Vec2::new(4., 3.));
    let mut text = if html {
        records::memo_to_text(stored)
    } else {
        stored.to_string()
    };
    let before = text.clone();
    let response = c.ui.put(
        inner,
        egui::TextEdit::multiline(&mut text)
            .id(c.id(key))
            .frame(false)
            .font(crate::schedule_dialogs::dialog_font())
            .text_color(INK)
            .desired_width(inner.width()),
    );
    crate::dialog_keys::note_keeps_enter(&response);
    c.act(key, rect);
    (response.changed() && text != before).then(|| {
        if html {
            records::text_to_memo(&text)
        } else {
            text
        }
    })
}

/// A labeled date field over an XER date column; returns the new XER text
/// (empty to clear) when a valid date was committed.
#[allow(clippy::too_many_arguments)]
pub fn date_text(
    c: &mut Canvas,
    key: &str,
    x: f32,
    y: f32,
    w: f32,
    caption: &str,
    origin: i64,
    stored: &str,
    enabled: bool,
) -> Option<String> {
    sheet::above(c, x, y, caption, enabled);
    let shown = show_date(origin, stored);
    let text = sheet::commit_field(c, key, x, y, w, &shown, enabled, Align::Min)?;
    if text.trim().is_empty() {
        return Some(String::new());
    }
    let keep = openxer_xer::parse_relative(origin, stored).ok().flatten();
    match format::parse_date(origin, &text, keep) {
        Ok(v) => Some(openxer_xer::date_string(origin, v)),
        Err(_) => None,
    }
}

/// A labeled number field; returns the new text when a number (or blank,
/// when `blank` allows it) was committed.
#[allow(clippy::too_many_arguments)]
pub fn number_text(
    c: &mut Canvas,
    key: &str,
    x: f32,
    y: f32,
    w: f32,
    caption: &str,
    stored: &str,
    blank: bool,
    enabled: bool,
) -> Option<String> {
    sheet::above(c, x, y, caption, enabled);
    let shown = show_number(stored);
    let text = sheet::commit_field(c, key, x, y, w, &shown, enabled, Align::Max)?;
    let trimmed = text.trim().replace(',', "");
    if trimmed.is_empty() {
        return blank.then(String::new);
    }
    trimmed
        .trim_start_matches(['$', '€', '£'])
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .map(records::format_number)
}

/// A labeled combo over `(code, caption)` options with an empty first
/// choice; returns the new code.
#[allow(clippy::too_many_arguments)]
pub fn combo_field(
    c: &mut Canvas,
    key: &str,
    x: f32,
    y: f32,
    w: f32,
    caption: &str,
    options: &[(&str, &str)],
    code: &str,
    enabled: bool,
) -> Option<String> {
    sheet::above(c, x, y, caption, enabled);
    sheet::combo(c, key, x, y, w, options, code, enabled)
}

/// A combo over a dictionary of `(id, name)` pairs, the first entry empty.
#[allow(clippy::too_many_arguments)]
pub fn dictionary_field(
    c: &mut Canvas,
    key: &str,
    x: f32,
    y: f32,
    w: f32,
    caption: &str,
    list: &[(String, String)],
    id: &str,
    enabled: bool,
) -> Option<String> {
    let mut options: Vec<(&str, &str)> = vec![("", "")];
    options.extend(list.iter().map(|(k, n)| (k.as_str(), n.as_str())));
    let code = list
        .iter()
        .find(|(k, _)| !id.is_empty() && num(k) == num(id))
        .map_or("", |(k, _)| k.as_str());
    combo_field(c, key, x, y, w, caption, &options, code, enabled)
}

/// Keep the selection on a record that is still listed, else the first one.
/// Views call it before reading the selection, so commands that need one are
/// enabled in the same frame.
pub fn keep_selection(s: &mut ListState, keys: &[RowKey]) {
    let listed = |id: i64| keys.contains(&RowKey::Record(id));
    if !s.selected.is_some_and(listed) {
        s.selected = keys.iter().find_map(|k| match k {
            RowKey::Record(id) => Some(*id),
            RowKey::Band(_) => None,
        });
    }
}

impl App {
    /// Draws one record view: command bar, details pane and the table.
    /// Returns the command clicked (the Insert and Delete keys go through
    /// `table_keys`).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn record_list_view(
        &mut self,
        host: &mut egui::Ui,
        spec: &ListSpec,
        state: StateFn,
        rows: Vec<Row>,
        keys: Vec<RowKey>,
        details: DetailsFn,
        menu: &mut dyn FnMut(&mut App, &mut egui::Ui),
    ) -> Option<&'static str> {
        let mut command = None;
        let mut customize: Option<Vec<String>> = None;
        keep_selection(state(&mut self.views), &keys);
        // The User Defined columns of the view's subject area, after its own.
        let udfs = {
            let columns = &state(&mut self.views).columns;
            super::udf_columns::Shown::of(&self.project.document, columns)
        };
        let mut rows = rows;
        let records: Vec<Option<i64>> = keys
            .iter()
            .map(|k| match k {
                RowKey::Record(id) => Some(*id),
                RowKey::Band(_) => None,
            })
            .collect();
        let origin = self.project.model.origin_serial;
        udfs.fill(&self.project.document, origin, &mut rows, &records);
        let titles: Vec<&str> = spec
            .titles
            .iter()
            .copied()
            .chain(udfs.titles.iter().map(String::as_str))
            .collect();
        let right: Vec<usize> = spec.right.iter().copied().chain(udfs.right()).collect();
        let indicators = udfs.indicators();
        let editors: Vec<(usize, grid::EditKind)> = (udfs.own..udfs.own + udfs.types.len())
            .filter_map(|f| {
                let ty = udfs.field(f)?;
                Some((f, super::udf_columns::editor(&self.project.document, ty)?))
            })
            .collect();
        let mut cell_edit = self.take_cell_edit(spec.id);
        let mut committed = None;
        // The right dock's Edit bar runs Add and Delete here too
        // (`view_edit`): the view's own bar keeps them only while that bar
        // is hidden, so each shows once; a bar left empty is not drawn.
        let edit_bar = self.bar_visible("Edit");
        let commands: Vec<&(&'static str, chrome::Icon, bool)> = spec
            .commands
            .iter()
            .filter(|(name, _, _)| !(edit_bar && matches!(*name, "Add" | "Delete")))
            .collect();
        if !commands.is_empty() {
            egui::SidePanel::right(egui::Id::new((spec.id, "command_bar")))
                .exact_width(sheet::SIDE_BAR)
                .resizable(false)
                .show_separator_line(false)
                .frame(
                    egui::Frame::NONE
                        .fill(Color32::WHITE)
                        .inner_margin(egui::Margin::symmetric(2, 6)),
                )
                .show_inside(host, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.;
                    for (name, icon, enabled) in commands {
                        let response = sheet::side_button(ui, *icon, name, *enabled);
                        self.action(
                            &format!("{}_cmd_{}", spec.id, name.to_lowercase()),
                            &response,
                        );
                        if response.clicked() {
                            command = Some(*name);
                        }
                    }
                });
        }
        if state(&mut self.views).show_details {
            let s = state(&mut self.views);
            let (tab, height) = (s.tab.min(spec.tabs.len() - 1), s.details_height);
            let (height, clicked) =
                self.details_pane(host, spec.id, height, spec.tabs, tab, |app, c, size| {
                    details(app, c, size, tab)
                });
            let s = state(&mut self.views);
            s.details_height = height;
            if let Some(tab) = clicked {
                s.tab = tab;
                s.sub_row = None;
            }
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(sheet::MARGIN))
            .show_inside(host, |ui| {
                ui.spacing_mut().item_spacing.y = 0.;
                let (inner, strip) = sheet::table_frame(ui, ui.max_rect());
                let bar = Rect::from_min_size(strip, Vec2::new(inner.width(), 17.));
                let caption = format!("Display: {}", spec.display);
                let header = sheet::header_button(ui, spec.id, bar, &caption);
                self.action(&format!("{}_layout", spec.id), &header);
                egui::Popup::menu(&header)
                    .style(chrome::popup_menu_style)
                    .show(|ui| {
                        menu(self, ui);
                        chrome::menu_separator(ui);
                        let s = state(&mut self.views);
                        if chrome::menu_item(ui, chrome::Item::new("Expand All")).clicked() {
                            s.collapsed.clear();
                            ui.close();
                        }
                        if chrome::menu_item(ui, chrome::Item::new("Collapse All")).clicked() {
                            s.collapsed = keys
                                .iter()
                                .filter_map(|k| match k {
                                    RowKey::Band(id) => Some(*id),
                                    RowKey::Record(_) => None,
                                })
                                .collect();
                            ui.close();
                        }
                        chrome::menu_separator(ui);
                        if chrome::menu_item(
                            ui,
                            chrome::Item::new("Details").checked(s.show_details),
                        )
                        .clicked()
                        {
                            s.show_details = !s.show_details;
                        }
                    });
                let body = Rect::from_min_max(Pos2::new(inner.left(), bar.bottom()), inner.max);
                let mut ui = ui.new_child(egui::UiBuilder::new().max_rect(body));
                let ui = &mut ui;
                let shown: Vec<Option<i64>> = keys
                    .iter()
                    .map(|k| match k {
                        RowKey::Record(id) => Some(*id),
                        RowKey::Band(_) => None,
                    })
                    .collect();
                let s = state(&mut self.views);
                let selection = s.rows.shown(&mut s.selected, &shown);
                let shown = state(&mut self.views).show_details;
                let menu = |row: usize| {
                    record_menu(
                        spec,
                        shown,
                        matches!(keys.get(row), Some(RowKey::Record(_))),
                    )
                };
                let open_menu = self.ctx_menu.take();
                let table = grid::Table {
                    id: spec.id,
                    titles: &titles,
                    right: &right,
                    tree_field: 0,
                    header_height: grid::HEADER_HEIGHT,
                    row_lines: false,
                    band_pitch: grid::BandPitch::Styled,
                    band_caption: &[],
                    menu: Some(&menu),
                    open_menu,
                    chart_menu: None,
                };
                let s = state(&mut self.views);
                let kind = |row: usize, field: usize| {
                    records.get(row).copied().flatten()?;
                    editors.iter().find(|e| e.0 == field).map(|e| e.1.clone())
                };
                let response = grid::show_with(
                    ui,
                    &table,
                    &mut s.columns,
                    &rows,
                    &selection,
                    None,
                    None,
                    &mut self.action_rects,
                    grid::Extras {
                        indicators: &indicators,
                        editing: Some(grid::Editing {
                            kind: &kind,
                            open: &mut cell_edit,
                        }),
                        ..Default::default()
                    },
                );
                committed = response.committed.clone();
                let s = state(&mut self.views);
                if let Some(i) = response.toggled {
                    let id = match keys[i] {
                        RowKey::Band(id) | RowKey::Record(id) => id,
                    };
                    if !s.collapsed.insert(id) {
                        s.collapsed.remove(&id);
                    }
                }
                if let Some(RowKey::Record(id)) = response.clicked.map(|i| keys[i]) {
                    let current = s.selected;
                    let next =
                        s.rows
                            .click(current.as_ref(), id, response.modifiers, response.secondary);
                    if current != Some(next) {
                        s.sub_row = None;
                    }
                    s.selected = Some(next);
                }
                if let Some(RowKey::Record(_)) = response.double_clicked.map(|i| keys[i]) {
                    s.show_details = true;
                }
                if let Some((_, key)) = response.menu {
                    match key {
                        "add" => command = Some("Add"),
                        "delete" => command = Some("Delete"),
                        "details" => s.show_details = true,
                        "hide_details" => s.show_details = false,
                        "customize" => {
                            customize = Some(
                                s.columns
                                    .order
                                    .iter()
                                    .filter_map(|&c| titles.get(c))
                                    .map(|t| t.to_string())
                                    .collect::<Vec<String>>(),
                            )
                        }
                        other => command = Some(other),
                    }
                }
                if rows.is_empty() {
                    // Under the header: the table takes the rest of the
                    // space, so a label after it would not show.
                    grid::empty_note(ui, &response, spec.id, spec.empty, &mut self.action_rects);
                }
            });
        self.keep_cell_edit(spec.id, cell_edit);
        if let Some((row, field, text)) = committed {
            let ty = udfs.field(field).map(|t| t.type_id);
            if let (Some(type_id), Some(Some(fk))) = (ty, records.get(row)) {
                let time = host.ctx().input(|i| i.time);
                self.commit_udf_cell(type_id, *fk, &text, time);
            }
        }
        if let Some(shown) = customize {
            // Default puts back the columns the view starts with.
            let default: Vec<&str> = crate::defaults::record_columns(spec.id, spec.titles)
                .into_iter()
                .filter_map(|c| spec.titles.get(c).copied())
                .collect();
            let choices = super::udf_columns::choices(&self.project.document, spec.id);
            self.open_view_columns_with(
                crate::details::grid_columns::ColumnTarget::Record(spec.id),
                spec.customize,
                spec.titles,
                &choices,
                &default,
                shown,
            );
        }
        command
    }

    /// Whether the right dock's Add and Delete act on a record view now.
    pub(super) fn record_edit_commands(&self) -> (bool, bool) {
        let r = &self.views.records;
        match self.views.main {
            MainView::Expenses => (
                !self.project.model.activities.is_empty(),
                r.expenses.list.selected.is_some(),
            ),
            MainView::Documents => (true, r.documents.selected.is_some()),
            MainView::Risks => (true, r.risks.selected.is_some()),
            MainView::Issues => (true, r.issues.list.selected.is_some()),
            MainView::Thresholds => (true, r.thresholds.list.selected.is_some()),
            _ => (false, false),
        }
    }

    /// The right dock's Add or Delete in a record view.
    pub(super) fn record_edit_command(&mut self, add: bool) {
        match (self.views.main, add) {
            (MainView::Expenses, true) => self.expense_command_add(),
            (MainView::Expenses, false) => self.expense_command_delete(),
            (MainView::Documents, true) => self.document_command_add(),
            (MainView::Documents, false) => self.document_command_delete(),
            (MainView::Risks, true) => self.risk_command_add(),
            (MainView::Risks, false) => self.risk_command_delete(),
            (MainView::Issues, true) => self.issue_command_add(),
            (MainView::Issues, false) => self.issue_command_delete(),
            (MainView::Thresholds, true) => self.threshold_command_add(),
            (MainView::Thresholds, false) => self.threshold_command_delete(),
            _ => {}
        }
    }

    /// The record view `view`'s list state, its record name (one, many)
    /// and how many records deleting `id` removes.
    fn record_kind(view: MainView) -> Option<(&'static str, &'static str, &'static str)> {
        Some(match view {
            MainView::Expenses => ("expenses", "expense item", "expense items"),
            MainView::Documents => (
                "documents",
                "work product or document",
                "work products and documents",
            ),
            MainView::Risks => ("risks", "risk", "risks"),
            MainView::Issues => ("issues", "issue", "issues"),
            MainView::Thresholds => ("thresholds", "threshold", "thresholds"),
            _ => return None,
        })
    }

    /// The records a delete in `view` removes now: the selected ones in
    /// table order, and for documents every document under them.
    fn record_delete_targets(&mut self, view: MainView) -> (Vec<i64>, Vec<i64>) {
        let Some((id, _, _)) = Self::record_kind(view) else {
            return (vec![], vec![]);
        };
        let Some(s) = list_state(&mut self.views, id) else {
            return (vec![], vec![]);
        };
        let picked = s.rows.picked(s.selected.as_ref());
        let mut gone = picked.clone();
        if view == MainView::Documents {
            for &doc in &picked {
                for under in openxer_xer::edit::documents::subtree(&self.project, doc) {
                    if !gone.contains(&under) {
                        gone.push(under);
                    }
                }
            }
        }
        (picked, gone)
    }

    /// Delete in a record view (command bar, right-click menu, dock, Edit
    /// menu or the Delete key): every selected record, as one undo step,
    /// after a confirmation when more than one record goes
    /// (`needs_confirm`). The record after the deleted ones becomes
    /// current (else the one before).
    pub(super) fn record_delete(&mut self, view: MainView, confirmed: bool) {
        let Some((id, one, many)) = Self::record_kind(view) else {
            return;
        };
        let (picked, gone) = self.record_delete_targets(view);
        if picked.is_empty() {
            return;
        }
        if needs_confirm(gone.len()) && !confirmed {
            let names: Vec<String> = gone
                .iter()
                .map(|g| self.record_name(view, *g))
                .filter(|n| !n.is_empty())
                .collect();
            self.views.records.confirm_delete = Some(ViewDelete {
                view,
                text: delete_question(gone.len(), one, many, &names),
            });
            return;
        }
        let next = list_state(&mut self.views, id).and_then(|s| s.rows.neighbour(&gone));
        let mut count = 0;
        for &record in &picked {
            count += match view {
                MainView::Expenses => {
                    usize::from(expenses::delete_expense(&mut self.project, record))
                }
                MainView::Documents => documents::delete_document(&mut self.project, record),
                MainView::Risks => usize::from(risks::delete_risk(&mut self.project, record)),
                MainView::Issues => usize::from(issues::delete_issue(&mut self.project, record)),
                MainView::Thresholds => {
                    usize::from(thresholds::delete_threshold(&mut self.project, record))
                }
                _ => 0,
            };
        }
        if count == 0 {
            return;
        }
        if let Some(s) = list_state(&mut self.views, id) {
            s.selected = next;
            s.sub_row = None;
            if let Some(next) = next {
                s.rows.select(&[next]);
            }
        }
        self.touch_records();
        let mut noun = if count == 1 { one } else { many }.to_string();
        if let Some(first) = noun.get_mut(..1) {
            first.make_ascii_uppercase();
        }
        self.status = if count == 1 {
            format!("{noun} deleted")
        } else {
            format!("{count} {} deleted", many)
        };
    }

    /// How a record reads in the delete question.
    fn record_name(&self, view: MainView, id: i64) -> String {
        let doc = &self.project.document;
        let (table, key, column) = match view {
            MainView::Expenses => ("PROJCOST", "cost_item_id", "cost_name"),
            MainView::Documents => ("DOCUMENT", "doc_id", "doc_name"),
            MainView::Risks => (records::risk_table(doc), "risk_id", "risk_name"),
            MainView::Issues => ("PROJISSU", "issue_id", "issue_name"),
            // A threshold has no name of its own.
            _ => return String::new(),
        };
        value(doc, table, key, id, column)
    }

    /// The Yes / No question before a delete of more than one record or
    /// resource. Escape and No change nothing.
    fn view_delete_dialog(&mut self, ctx: &egui::Context) {
        let Some(pending) = self.views.records.confirm_delete.clone() else {
            return;
        };
        if self.views.main != pending.view {
            self.views.records.confirm_delete = None;
            return;
        }
        let mut rects = vec![];
        let clicked = crate::kit_window::message_box(
            ctx,
            "view_delete",
            "Confirmation",
            crate::kit_window::MessageIcon::Question,
            &pending.text,
            &["Yes", "No"],
            crate::kit_window::Question::DeleteRecords.default_in(&["Yes", "No"]),
            &mut rects,
        );
        self.record_dialog_actions(rects);
        let answer = match clicked {
            Some(0) => Some(true),
            Some(_) => Some(false),
            None if ctx.input(|i| i.key_pressed(Key::Escape)) => Some(false),
            None => None,
        };
        match answer {
            Some(true) => {
                self.views.records.confirm_delete = None;
                let time = ctx.input(|i| i.time);
                if pending.view == MainView::Resources {
                    self.resource_delete_confirmed(time);
                } else {
                    self.record_delete(pending.view, true);
                }
            }
            Some(false) => self.views.records.confirm_delete = None,
            None => {}
        }
    }

    /// Any window of the record views is open (keys go to it).
    pub(super) fn record_dialog_open(&self) -> bool {
        let r = &self.views.records;
        r.picker.is_some()
            || r.confirm_delete.is_some()
            || r.monitor.is_some()
            || r.navigator.is_some()
            || r.launch.is_some()
            || self.views.resources.pick.is_some()
            || self.views.resources.merge.is_some()
            || self.views.assignments.pick.is_some()
    }

    /// Windows of the record views, drawn over any view.
    pub(crate) fn record_view_dialogs(&mut self, ctx: &egui::Context) {
        self.view_delete_dialog(ctx);
        self.picker_window(ctx);
        self.monitor_dialog(ctx);
        self.issue_navigator(ctx);
        self.launch_dialog(ctx);
        self.resource_pick_window(ctx);
        self.resource_merge_dialog(ctx);
        self.assignment_pick_window(ctx);
    }

    /// Activity or WBS rows offered by the picker for `target`.
    fn picker_rows(&self, target: PickTarget) -> Vec<(i64, String, String)> {
        let doc = &self.project.document;
        let pid = self.project.model.project.proj_id;
        if let PickTarget::WbsDocument(wbs_id) = target {
            return documents::unassigned_for_wbs(doc, pid, wbs_id);
        }
        if let PickTarget::ActivityWbs(_) = target {
            // An activity may sit on the project node itself.
            let mut rows: Vec<(i64, String, String)> = wbs::project_node(&self.project)
                .and_then(|id| self.project.wbs.iter().find(|w| w.id == id))
                .map(|w| (w.id, w.code.clone(), w.name.clone()))
                .into_iter()
                .collect();
            rows.extend(wbs_choices(&self.project));
            return rows;
        }
        if target.wbs() {
            let taken: Vec<i64> = match target {
                PickTarget::DocumentWbs(doc_id) => documents::assignments(doc, pid, doc_id)
                    .iter()
                    .filter(|a| a.task_id == 0)
                    .map(|a| a.wbs_id)
                    .collect(),
                _ => vec![],
            };
            return wbs_choices(&self.project)
                .into_iter()
                .filter(|(id, _, _)| !taken.contains(id))
                .collect();
        }
        let taken: Vec<i64> = match target {
            PickTarget::DocumentActivity(doc_id) => documents::assignments(doc, pid, doc_id)
                .iter()
                .map(|a| a.task_id)
                .collect(),
            PickTarget::RiskActivity(risk_id) => risks::risk_activities(doc, pid, risk_id),
            _ => vec![],
        };
        activity_choices(&self.project)
            .into_iter()
            .filter(|(id, _, _)| !taken.contains(id))
            .collect()
    }

    pub(crate) fn open_picker(&mut self, target: PickTarget) {
        // Select WBS for an activity opens on the activity's own element.
        let selected = match target {
            PickTarget::ActivityWbs(task_id) => {
                let current = self.project.details.get(&task_id).map(|d| d.wbs);
                self.picker_rows(target)
                    .iter()
                    .position(|(id, _, _)| Some(*id) == current)
            }
            _ => None,
        };
        self.views.records.picker = Some(Picker { target, selected });
    }

    /// Applies a picked activity or WBS element to its target.
    pub(super) fn apply_pick(&mut self, target: PickTarget, id: i64) {
        if let PickTarget::ActivityWbs(task_id) = target {
            // The same change as the General tab's WBS field.
            let activities = &self.project.model.activities;
            if let Some(i) = activities.iter().position(|a| a.task_id == task_id) {
                if let Err(e) = self.edit_activity(i, crate::details::setters::Edit::Wbs(id)) {
                    self.error(e);
                }
            }
            return;
        }
        let pid = self.project.model.project.proj_id;
        let changed = {
            let p = &mut self.project;
            match target {
                PickTarget::ExpenseActivity(cost) => expenses::move_expense(p, cost, id),
                PickTarget::DocumentActivity(doc) => documents::assign(p, doc, Some(id), None),
                PickTarget::DocumentWbs(doc) => documents::assign(p, doc, None, Some(id)),
                PickTarget::WbsDocument(wbs) => documents::assign(p, id, None, Some(wbs)),
                PickTarget::RiskActivity(risk) => risks::assign(&mut p.document, pid, risk, id),
                PickTarget::IssueActivity(issue) => {
                    openxer_xer::edit::project_records::set_issue_activity(p, issue, id)
                }
                PickTarget::IssueWbs(issue) => {
                    openxer_xer::edit::project_records::set_issue_wbs(p, issue, id)
                }
                PickTarget::ActivityWbs(_) => false,
                PickTarget::ThresholdWbs(thresh) => set_value(
                    &mut p.document,
                    "PROJTHRS",
                    "thresh_id",
                    thresh,
                    "wbs_id",
                    id.to_string(),
                ),
            }
        };
        if changed {
            self.touch_records();
        }
    }

    /// The Select Activity / Select WBS window: a list with Assign and
    /// Cancel; a double click assigns.
    fn picker_window(&mut self, ctx: &egui::Context) {
        let Some(picker) = self.views.records.picker.clone() else {
            return;
        };
        let items = self.picker_rows(picker.target);
        let wbs = picker.target.wbs();
        let document = matches!(picker.target, PickTarget::WbsDocument(_));
        let (title, first, second, first_width) = if document {
            (
                "Assign Work Products and Documents",
                "Title",
                "Reference No.",
                270.,
            )
        } else if wbs {
            ("Select WBS", "WBS Code", "WBS Name", 140.)
        } else {
            ("Select Activity", "Activity ID", "Activity Name", 140.)
        };
        let mut selected = picker.selected.filter(|&i| i < items.len());
        let ((assign, cancel), closed, actions) =
            crate::dialog_kit::window(ctx, "record_picker", title, Vec2::new(460., 420.), |c| {
                let cols = [
                    crate::dialog_kit::Col {
                        title: first,
                        width: first_width,
                    },
                    crate::dialog_kit::Col {
                        title: second,
                        width: 410. - first_width,
                    },
                ];
                let out = crate::dialog_kit::list(
                    c,
                    "pick",
                    (10., 10., 440., 360.),
                    None,
                    &cols,
                    items.len(),
                    selected,
                    Some(0),
                    &mut |c, r, k, rect, sel| {
                        let text = if k == 0 { &items[r].1 } else { &items[r].2 };
                        crate::dialog_kit::cell_text(c, rect, text, sel, 3.);
                    },
                );
                if let Some(r) = out.clicked {
                    selected = Some(r);
                }
                let double = out.double;
                let row = crate::dialog_kit::button_row(
                    c,
                    450.,
                    386.,
                    &[
                        crate::dialog_kit::Entry::Button(
                            "pick_assign",
                            "Assign",
                            selected.is_some(),
                            false,
                        ),
                        crate::dialog_kit::Entry::cancel("pick_cancel"),
                    ],
                );
                let assign = row.clicked.as_deref() == Some("pick_assign") || double.is_some();
                if double.is_some() {
                    selected = double;
                }
                (assign, row.clicked.as_deref() == Some("pick_cancel"))
            });
        self.record_dialog_actions(actions);
        if let Some(p) = &mut self.views.records.picker {
            p.selected = selected;
        }
        if assign {
            if let Some(&(id, _, _)) = selected.and_then(|i| items.get(i)) {
                self.apply_pick(picker.target, id);
            }
            self.views.records.picker = None;
        } else if cancel || closed || ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.views.records.picker = None;
        }
    }
}

/// State of the record views and their windows, saved with the views.
pub struct RecordViews {
    pub expenses: expenses::ExpensesState,
    pub documents: ListState,
    pub risks: ListState,
    pub issues: issues::IssuesState,
    pub thresholds: thresholds::ThresholdsState,
    pub picker: Option<Picker>,
    pub monitor: Option<thresholds::MonitorDialog>,
    pub navigator: Option<issues::Navigator>,
    /// A document file waiting for the Open / Cancel confirmation: the
    /// canonical path that passed the checks.
    pub launch: Option<String>,
    /// A delete of more than one record (or resource) waiting for Yes / No.
    pub confirm_delete: Option<ViewDelete>,
}

/// A delete in a view table waiting for its confirmation: the view and the
/// question. Yes deletes the view's selection as it is then.
#[derive(Clone, Debug, PartialEq)]
pub struct ViewDelete {
    pub view: MainView,
    pub text: String,
}

/// The one rule for Delete in the view tables (record views and
/// Resources): a delete that removes one record with nothing under it
/// happens at once (one Ctrl+Z brings it back); a delete that removes more
/// than one (a multi-selection, or a document with documents under it)
/// asks first.
pub fn needs_confirm(count: usize) -> bool {
    count > 1
}

/// The question asked before deleting `count` records named `one` /
/// `many`, listing up to five of `names`.
pub fn delete_question(count: usize, one: &str, many: &str, names: &[String]) -> String {
    let mut text = if count == 1 {
        format!("Delete the selected {one}?")
    } else {
        format!("Delete {count} {many}?")
    };
    for name in names.iter().take(5) {
        text.push('\n');
        text.push_str(name);
    }
    if names.len() > 5 {
        text.push_str(&format!("\nand {} more", names.len() - 5));
    }
    text
}

impl Default for RecordViews {
    fn default() -> Self {
        Self {
            expenses: Default::default(),
            documents: ListState::new("documents", &documents::TITLES, &documents::WIDTHS),
            risks: ListState::new("risks", &risks::TITLES, &risks::WIDTHS),
            issues: Default::default(),
            thresholds: Default::default(),
            picker: None,
            monitor: None,
            navigator: None,
            launch: None,
            confirm_delete: None,
        }
    }
}

impl RecordViews {
    pub fn reset(&mut self) {
        self.expenses.list.reset();
        self.documents.reset();
        self.risks.reset();
        self.issues.list.reset();
        self.thresholds.list.reset();
        self.picker = None;
        self.monitor = None;
        self.navigator = None;
        self.launch = None;
        self.confirm_delete = None;
    }
    pub fn json(&self) -> serde_json::Value {
        serde_json::json!({
            "expenses": self.expenses.json(),
            "documents": self.documents.json(),
            "risks": self.risks.json(),
            "issues": self.issues.json(),
            "thresholds": self.thresholds.json(),
        })
    }
    pub fn restore(&mut self, value: &serde_json::Value) {
        self.expenses.restore(&value["expenses"]);
        self.documents
            .restore(&value["documents"], documents::TABS.len());
        self.risks.restore(&value["risks"], risks::TABS.len());
        self.issues.restore(&value["issues"]);
        self.thresholds.restore(&value["thresholds"]);
    }
}
