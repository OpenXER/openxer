//! Columns, Edit Column, Filters, Filter editor, Group and Sort, Sort, Open
//! Layout, Save Layout As, the save-changes prompt, the layout and filter
//! delete questions and Find/Replace.
//! Each dialog edits a draft; OK and Apply commit it, Cancel and Escape drop it.
use super::columns::{self, Align as ColAlign, Column, Editor, Kind};
use super::filters::{Criterion, FilterSelection, Op, Standard, UserFilter};
use super::layout::{ColumnSetting, GroupLevel, GroupSort, SortKey};
use super::*;
use crate::available_options;

#[path = "filter_editor.rs"]
mod filter_editor;

use crate::chrome::DialogButton;
use crate::dialog_kit::{self as kit, Entry, STRIP};

/// Group rows of the Available Options tree.
const TREE_GROUP: Color32 = Color32::from_rgb(0xD0, 0xFF, 0xD0);
/// Group rows of the Filters list.
const FILTER_GROUP: Color32 = Color32::from_rgb(0xFF, 0xFF, 0xD0);

/// Whose layouts the Open Layout, Save Layout As and save-changes windows
/// work on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LayoutTarget {
    #[default]
    Activities,
    /// The Resource Assignments view (`views::assignment_layouts`).
    Assignments,
}

#[derive(Default)]
pub struct Dialogs {
    pub columns: Option<ColumnsDraft>,
    pub edit_column: Option<EditColumnDraft>,
    pub filters: Option<FiltersDraft>,
    pub filter_editor: Option<FilterEditor>,
    pub group_sort: Option<GroupSortDraft>,
    pub sort: Option<SortDraft>,
    pub open_layout: Option<OpenLayoutDraft>,
    pub save_as: Option<String>,
    /// A layout to open once the save-changes question is answered.
    pub save_prompt: Option<(String, bool)>,
    /// A layout or filter delete waiting for its answer.
    pub remove_prompt: Option<GridRemoval>,
    /// Whose layouts the layout windows above list and save.
    pub layout_target: LayoutTarget,
    pub find: Option<FindState>,
    /// Find was just opened from the keyboard: its Find what edit takes the
    /// keyboard focus the first time it is drawn.
    pub find_focus: bool,
    /// The last search that had text, kept after the Find dialog closes so
    /// Find Next (F3) can go on with it.
    pub last_find: Option<FindState>,
    /// Rows cut or copied in the Filter editor.
    pub filter_clip: Vec<Criterion>,
    /// The Filter editor cell whose in-grid editor is open: (row, column).
    pub filter_editing: Option<(usize, u8)>,
    /// The dialog that Escape closes this frame (the topmost one).
    escape_for: &'static str,
}

/// What Delete in Open Layout or Filters asks about.
#[derive(Clone, Debug, PartialEq)]
pub enum GridRemoval {
    /// A saved layout, by name, of the windows `layout_target` names.
    Layout(String),
    /// A user filter in the open Filters dialog, by its key.
    Filter(String),
}

pub struct ColumnsDraft {
    pub columns: Vec<ColumnSetting>,
    pub selected: Option<usize>,
    pub available: Option<String>,
    pub open_groups: BTreeSet<String>,
    /// Edits the bottom pane's Activity Table instead of the layout's.
    pub bottom: bool,
}

pub struct EditColumnDraft {
    pub index: usize,
    pub title: String,
    /// The width as typed in its spin edit.
    pub width: String,
    pub align: ColAlign,
}

pub struct FiltersDraft {
    pub selection: FilterSelection,
    pub users: Vec<UserFilter>,
    pub current: Option<String>,
}

pub struct FilterEditor {
    /// Name of the filter being modified; `None` for a new filter.
    pub original: Option<String>,
    pub filter: UserFilter,
    pub selected: Option<usize>,
}

pub struct GroupSortDraft {
    pub gs: GroupSort,
    /// The selected Group By row (row n is band level n).
    pub selected: Option<usize>,
    /// The cell whose in-grid editor is open: (row, grid column).
    pub editing: Option<(usize, u8)>,
    /// The level whose Font & Color editor is open.
    pub band_edit: Option<usize>,
}

pub struct SortDraft {
    pub keys: Vec<SortKey>,
    pub selected: Option<usize>,
}

pub struct OpenLayoutDraft {
    pub selected: Option<String>,
    pub use_filter: bool,
}

#[derive(Default, Clone)]
pub struct FindState {
    pub what: String,
    pub with: String,
    /// A column id, or `None` for every displayed column.
    pub column: Option<String>,
    pub match_case: bool,
    pub replace: bool,
}

/// The file chooser of Open Layout's Import (`None`) and Export (the
/// layout's name). Tests never open it.
#[cfg(not(test))]
fn layout_file_dialog(export: Option<&str>) -> Option<std::path::PathBuf> {
    let dialog = rfd::FileDialog::new().add_filter("OpenXER layout", &["oxl", "json"]);
    match export {
        Some(name) => {
            let safe: String = name
                .chars()
                .map(|c| {
                    if c.is_alphanumeric() || c == ' ' {
                        c
                    } else {
                        '_'
                    }
                })
                .collect();
            dialog.set_file_name(format!("{safe}.oxl")).save_file()
        }
        None => dialog.pick_file(),
    }
}

#[cfg(test)]
fn layout_file_dialog(_export: Option<&str>) -> Option<std::path::PathBuf> {
    None
}

/// The `v` chevron of a header button, five pixels wide at client (x, y).
fn chevron(c: &crate::schedule_dialogs::Canvas, x: f32, y: f32) {
    let mut mesh = egui::Mesh::default();
    for i in 0..3 {
        for dx in [i as f32, 4. - i as f32] {
            mesh.add_colored_rect(
                Rect::from_min_size(c.origin + Vec2::new(x + dx, y + i as f32), Vec2::splat(1.)),
                INK,
            );
        }
    }
    c.ui.painter().add(egui::Shape::mesh(mesh));
}

/// Columns a group level can use: WBS, then every column but notebook
/// topics (dates and numbers take a Group Interval).
fn groupable(catalog: &[Column]) -> Vec<(String, String)> {
    let mut rest: Vec<(String, String)> = catalog
        .iter()
        .filter(|c| c.id != "wbs")
        .filter(|c| !matches!(c.source, columns::Source::Notebook(_)))
        .map(|c| (c.id.clone(), c.title.clone()))
        .collect();
    rest.sort_by_key(|c| c.1.to_lowercase());
    std::iter::once(("wbs".to_string(), "WBS".to_string()))
        .chain(rest)
        .collect()
}

/// A row of the Group By grid. Row n draws band level n.
#[derive(Clone, Copy, Debug, PartialEq)]
enum GroupRow {
    /// The first row of grouping level n.
    Level(usize),
    /// WBS level k + 1 below a WBS grouping level.
    Wbs(usize, usize),
    /// The blank row that adds a level.
    New,
}

/// Action key prefixes of the in-grid editors, by grid column.
const EDITOR_KEYS: [&str; 4] = ["group_by", "", "group_to", "group_interval"];

/// The Group By grid's rows: each level, a WBS level's deeper WBS levels
/// (up to its To Level, or the project's depth), then the blank row; at
/// most one row per band style.
fn group_rows(gs: &GroupSort, wbs_depth: usize) -> Vec<GroupRow> {
    let mut rows = vec![];
    for (n, l) in gs.levels.iter().enumerate() {
        rows.push(GroupRow::Level(n));
        if l.column == "wbs" {
            let depth = l.to_level.unwrap_or(wbs_depth).min(wbs_depth).max(1);
            rows.extend((1..depth).map(|k| GroupRow::Wbs(n, k)));
        }
    }
    rows.truncate(super::layout::MAX_BAND_LEVELS);
    rows.push(GroupRow::New);
    rows
}

/// The action name of the entry a button column reported clicked.
fn clicked_key(entries: &[Entry<'static>], clicked: Option<&str>) -> Option<&'static str> {
    entries.iter().find_map(|e| match e {
        Entry::Button(key, ..) if Some(*key) == clicked => Some(*key),
        _ => None,
    })
}

/// One row of a chooser's Available Options: a group heading (open or not)
/// or an item, by id and title.
pub(crate) enum ChoiceRow<'a> {
    Group(&'a str, bool),
    Item(&'a str, &'a str),
}

/// The arrow buttons of a chooser panel, as `ChooserOut::clicks` reports
/// them.
pub(crate) struct Chooser;

impl Chooser {
    pub const ADD: &'static str = "add";
    pub const ADD_ALL: &'static str = "add_all";
    pub const REMOVE: &'static str = "remove";
    pub const REMOVE_ALL: &'static str = "remove_all";
    pub const UP: &'static str = "up";
    pub const DOWN: &'static str = "down";
}

/// What happened in a chooser panel this frame.
#[derive(Default)]
pub(crate) struct ChooserOut {
    /// A row of the Available Options clicked, and double-clicked.
    pub tree_click: Option<usize>,
    pub tree_double: Option<usize>,
    /// A row of the Selected Options clicked.
    pub picked: Option<usize>,
    /// Arrow buttons clicked (`Chooser` names).
    pub clicks: Vec<&'static str>,
    /// The Available Options view picked from its header menu.
    pub view: Option<available_options::AvailableView>,
}

/// The chooser panel of the Columns dialog, shared by every dialog of that
/// shape: in a lowered 432 x 285 panel at (5, 5), the Available Options
/// tree (`tree`, or a flat list when `view` is List) with its header menu,
/// the four arrow buttons, the Selected Options list (`titles`) and its Up
/// and Down buttons. The rectangles are named after `prefix`:
/// `<prefix>_available_header`, `<prefix>_tree` or `<prefix>_list_all`,
/// `<prefix>_group_<group>`, `<prefix>_available_<id>`, `<prefix>_add`,
/// `<prefix>_add_all`, `<prefix>_remove`, `<prefix>_remove_all`,
/// `<prefix>_list`, `<prefix>_selected_<n>`, `<prefix>_up` and
/// `<prefix>_down`.
pub(crate) fn chooser_panel(
    c: &mut crate::schedule_dialogs::Canvas,
    prefix: &str,
    tree: &[ChoiceRow],
    view: available_options::AvailableView,
    available: Option<&str>,
    titles: &[String],
    selected: Option<usize>,
) -> ChooserOut {
    use crate::dialog_kit as kit;
    let list = view.is_list();
    let mut panel = ChooserOut::default();
    c.fill(5., 5., 432., 285., CHROME);
    c.lowered(5., 5., 432., 285.);
    // Available Options.
    c.lowered(11., 11., 163., 273.);
    c.fill(12., 12., 161., 271., Color32::WHITE);
    c.fill(12., 12., 161., 18., CHROME);
    c.raised(12., 12., 161., 18.);
    chevron(c, 24., 18.);
    kit::text(c, 36., 15., "Available Options", true);
    // The whole header opens the Group and Sort By menu.
    let header = c.r(12., 12., 161., 18.);
    let header = c.hit(&format!("{prefix}_available_header"), header, true);
    panel.view = available_options::header_menu(&header, view, &mut c.actions);
    let body = c.r(12., 30., 161., 253.);
    let out = kit::rows_in(
        c,
        &if list {
            format!("{prefix}_list_all")
        } else {
            format!("{prefix}_tree")
        },
        body,
        16.,
        tree.len(),
        None,
        &mut |c, r, rect, _| {
            let p = c.ui.painter().with_clip_rect(body);
            let gutter = 13.;
            let line = Rect::from_min_size(
                Pos2::new(rect.left() + gutter, rect.bottom() - 1.),
                Vec2::new(rect.width() - gutter, 1.),
            );
            match &tree[r] {
                ChoiceRow::Group(g, open) => {
                    p.rect_filled(
                        Rect::from_min_max(Pos2::new(rect.left() + gutter, rect.top()), rect.max),
                        0.,
                        TREE_GROUP,
                    );
                    p.rect_filled(line, 0., Color32::from_gray(0xC0));
                    let b = Rect::from_min_size(rect.min + Vec2::new(2., 3.), Vec2::splat(9.));
                    p.rect_filled(b, 0., Color32::WHITE);
                    p.rect_stroke(
                        b,
                        0.,
                        Stroke::new(1_f32, Color32::from_gray(0x80)),
                        egui::StrokeKind::Inside,
                    );
                    p.rect_filled(
                        Rect::from_min_size(b.min + Vec2::new(2., 4.), Vec2::new(5., 1.)),
                        0.,
                        INK,
                    );
                    if !open {
                        p.rect_filled(
                            Rect::from_min_size(b.min + Vec2::new(4., 2.), Vec2::new(1., 5.)),
                            0.,
                            INK,
                        );
                    }
                    kit::cell_text(c, rect, g, false, 29.);
                    c.act(&format!("{prefix}_group_{g}"), rect.intersect(body));
                }
                ChoiceRow::Item(id, title) => {
                    let sel = available == Some(*id);
                    if sel {
                        p.rect_filled(rect, 0., kit::SELECTED);
                    }
                    kit::cell_text(c, rect, title, sel, if list { 4. } else { 43. });
                    c.act(&format!("{prefix}_available_{id}"), rect.intersect(body));
                }
            }
        },
    );
    panel.tree_click = out.clicked;
    panel.tree_double = out.double;
    // Arrow buttons.
    for (click, y, dir, double) in [
        (Chooser::ADD, 33., kit::Move::Right, false),
        (Chooser::ADD_ALL, 68., kit::Move::Right, true),
        (Chooser::REMOVE, 114., kit::Move::Left, false),
        (Chooser::REMOVE_ALL, 150., kit::Move::Left, true),
    ] {
        let key = format!("{prefix}_{click}");
        if kit::move_button(c, &key, (184., y), dir, double, true) {
            panel.clicks.push(click);
        }
    }
    // Selected Options.
    c.lowered(227., 11., 156., 273.);
    c.fill(228., 12., 154., 271., Color32::WHITE);
    c.fill(228., 12., 154., 21., CHROME);
    c.raised(228., 12., 154., 21.);
    kit::text(c, 233., 16., "Selected Options", true);
    let body = c.r(228., 33., 154., 234.);
    let out = kit::rows_in(
        c,
        &format!("{prefix}_list"),
        body,
        15.,
        titles.len(),
        selected,
        &mut |c, r, rect, _| {
            let sel = selected == Some(r);
            if sel {
                c.ui.painter()
                    .with_clip_rect(body)
                    .rect_filled(rect, 0., kit::SELECTED);
            }
            kit::cell_text(c, rect, &titles[r], sel, 16.);
            c.act(&format!("{prefix}_selected_{r}"), rect.intersect(body));
        },
    );
    panel.picked = out.clicked;
    c.fill(228., 267., 154., 16., CHROME);
    let s = selected.filter(|s| *s < titles.len());
    let up = s.is_some_and(|s| s > 0);
    let key = format!("{prefix}_up");
    if kit::move_button(c, &key, (393., 33.), kit::Move::Up, false, up) {
        panel.clicks.push(Chooser::UP);
    }
    let down = s.is_some_and(|s| s + 1 < titles.len());
    let key = format!("{prefix}_down");
    if kit::move_button(c, &key, (393., 68.), kit::Move::Down, false, down) {
        panel.clicks.push(Chooser::DOWN);
    }
    panel
}

/// One row of the Open Layout list: a scope band or a layout (index into
/// the names).
#[derive(Clone, Debug, PartialEq)]
enum LayoutLine {
    Band(String),
    Layout(usize),
}

/// The Open Layout rows grouped by scope: the layouts OpenXER ships (the
/// built-in one and the starter samples) under "Global", then the ones the
/// user saved under "User - <user name>". Layouts are user-level, so there
/// is no Project band. A band with no layouts is left out.
fn layout_lines(names: &[String], library: &[String], shipped: &[String]) -> Vec<LayoutLine> {
    let (user, global): (Vec<usize>, Vec<usize>) = (0..names.len())
        .partition(|i| library.contains(&names[*i]) && !shipped.contains(&names[*i]));
    let mut lines = vec![];
    for (caption, rows) in [
        ("Global".to_string(), global),
        (format!("User - {}", crate::reports::doc::user_name()), user),
    ] {
        if !rows.is_empty() {
            lines.push(LayoutLine::Band(caption));
            lines.extend(rows.into_iter().map(LayoutLine::Layout));
        }
    }
    lines
}

fn title_of(catalog: &[Column], id: &str) -> String {
    if id == "wbs" {
        return "WBS".into();
    }
    columns::find(catalog, id).map_or_else(|| id.to_string(), |c| c.title.clone())
}

impl App {
    pub(crate) fn top_grid_dialog(&self) -> &'static str {
        let d = &self.grid.dialogs;
        if d.remove_prompt.is_some() {
            "remove_prompt"
        } else if d.save_prompt.is_some() {
            "prompt"
        } else if d.edit_column.is_some() {
            "edit_column"
        } else if d.filter_editor.is_some() {
            "filter_editor"
        } else if d.sort.is_some() {
            "sort"
        } else if d.save_as.is_some() {
            "save_as"
        } else if d.open_layout.is_some() {
            "open_layout"
        } else if d.columns.is_some() {
            "columns"
        } else if d.filters.is_some() {
            "filters"
        } else if d.group_sort.is_some() {
            "group_sort"
        } else if d.find.is_some() {
            "find"
        } else {
            ""
        }
    }

    pub(crate) fn grid_dialog_open(&self) -> bool {
        !self.top_grid_dialog().is_empty()
    }

    fn escape(&self, _ctx: &egui::Context, name: &str) -> bool {
        self.grid.dialogs.escape_for == name
    }

    pub(crate) fn open_columns_dialog(&mut self) {
        self.grid.dialogs.columns = Some(ColumnsDraft {
            columns: self.grid.layout.columns.clone(),
            selected: Some(0),
            available: None,
            open_groups: BTreeSet::new(),
            bottom: false,
        });
    }

    pub(crate) fn open_filters_dialog(&mut self) {
        self.grid.dialogs.filters = Some(FiltersDraft {
            selection: self.grid.layout.filter.clone(),
            users: self.grid.user_filters.clone(),
            current: None,
        });
    }

    pub(crate) fn open_group_sort_dialog(&mut self) {
        self.grid.dialogs.group_sort = Some(GroupSortDraft {
            gs: self.grid.layout.group_sort.clone(),
            selected: Some(0),
            editing: None,
            band_edit: None,
        });
    }

    pub(crate) fn open_layout_dialog(&mut self) {
        self.grid.dialogs.layout_target = LayoutTarget::Activities;
        self.grid.dialogs.open_layout = Some(OpenLayoutDraft {
            selected: Some(self.grid.layout.name.clone()),
            use_filter: true,
        });
    }

    pub(crate) fn open_save_as_dialog(&mut self) {
        self.grid.dialogs.layout_target = LayoutTarget::Activities;
        self.grid.dialogs.save_as = Some(self.grid.layout.name.clone());
    }

    pub(crate) fn open_find(&mut self, replace: bool) {
        let state = self
            .grid
            .dialogs
            .find
            .get_or_insert_with(FindState::default);
        state.replace = replace;
        self.grid.dialogs.find_focus = true;
    }

    /// Draw every open table dialog.
    pub(crate) fn grid_dialogs(&mut self, ctx: &egui::Context) {
        // Escape closes an open Available Options menu before the dialog.
        self.grid.dialogs.escape_for =
            if ctx.input(|i| i.key_pressed(Key::Escape)) && !available_options::menu_open(ctx) {
                self.top_grid_dialog()
            } else {
                ""
            };
        self.columns_dialog(ctx);
        self.filters_dialog(ctx);
        self.group_sort_dialog(ctx);
        self.find_dialog(ctx);
        self.open_layout_window(ctx);
        self.save_as_window(ctx);
        self.sort_dialog(ctx);
        self.filter_editor_dialog(ctx);
        self.edit_column_dialog(ctx);
        self.save_prompt_window(ctx);
        self.remove_prompt_window(ctx);
    }

    fn columns_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.grid.dialogs.columns.take() else {
            return;
        };
        let catalog = columns::catalog(&self.project);
        let mut groups: Vec<String> = vec![];
        for c in &catalog {
            if !groups.contains(&c.group) {
                groups.push(c.group.clone());
            }
        }
        groups.sort_by_key(|g| g.to_lowercase());
        let mut open = true;
        let mut close = false;
        let mut commit = false;
        let mut edit = None;
        // 554 x 295 px inside the frame: the shared chooser panel, then the
        // button column at 442.
        use crate::dialog_kit as kit;
        let view = self.available_view;
        let list = view.is_list();
        let mut tree: Vec<ChoiceRow> = vec![];
        if list {
            // One flat list of every column not yet selected.
            let mut items: Vec<&Column> = catalog
                .iter()
                .filter(|c| !d.columns.iter().any(|s| s.id == c.id))
                .collect();
            available_options::alphabetical(&mut items, |c| &c.title);
            tree.extend(items.into_iter().map(|c| ChoiceRow::Item(&c.id, &c.title)));
        } else {
            for g in &groups {
                let is_open = d.open_groups.contains(g);
                tree.push(ChoiceRow::Group(g, is_open));
                if is_open {
                    let mut items: Vec<&Column> = catalog
                        .iter()
                        .filter(|c| &c.group == g)
                        .filter(|c| !d.columns.iter().any(|s| s.id == c.id))
                        .collect();
                    available_options::alphabetical(&mut items, |c| &c.title);
                    tree.extend(items.into_iter().map(|c| ChoiceRow::Item(&c.id, &c.title)));
                }
            }
        }
        let titles: Vec<String> = d
            .columns
            .iter()
            .map(|s| s.title.clone().unwrap_or_else(|| title_of(&catalog, &s.id)))
            .collect();
        let mut clicks: Vec<&'static str> = vec![];
        let mut panel = ChooserOut::default();
        let ((), closed, actions) =
            kit::window(ctx, "grid_columns", "Columns", Vec2::new(554., 295.), |c| {
                c.fill(0., 0., 554., 295., STRIP);
                panel = chooser_panel(
                    c,
                    "columns",
                    &tree,
                    view,
                    d.available.as_deref(),
                    &titles,
                    d.selected,
                );
                let can_edit = d.selected.is_some_and(|s| s < d.columns.len());
                let entries = [
                    Entry::ok("columns_ok"),
                    Entry::cancel("columns_cancel"),
                    Entry::apply("columns_apply", true),
                    Entry::Button("columns_default", "Default", true, true),
                    Entry::Button("columns_edit", "Edit Column...", can_edit, false),
                    Entry::help("columns_help"),
                ];
                let out = kit::button_column(c, 554., &entries);
                clicks.extend(clicked_key(&entries, out.clicked.as_deref()));
            });
        self.record_dialog_actions(actions);
        open &= !closed;
        if let Some(view) = panel.view {
            self.set_available_view(view);
        }
        if let Some(r) = panel.tree_click {
            match &tree[r] {
                ChoiceRow::Group(g, _) => {
                    let g = g.to_string();
                    if !d.open_groups.insert(g.clone()) {
                        d.open_groups.remove(&g);
                    }
                }
                ChoiceRow::Item(id, _) => d.available = Some(id.to_string()),
            }
        }
        if let Some(r) = panel.tree_double {
            if let ChoiceRow::Item(id, _) = &tree[r] {
                let id = id.to_string();
                add_column(&mut d, &catalog, &id);
            }
        }
        if let Some(r) = panel.picked {
            d.selected = Some(r);
        }
        for key in panel.clicks.into_iter().chain(clicks) {
            match key {
                Chooser::ADD => {
                    if let Some(id) = d.available.take() {
                        add_column(&mut d, &catalog, &id);
                    }
                }
                Chooser::ADD_ALL => {
                    // Every column the tree shows: the open groups, or in
                    // the flat list all of them, in the order drawn.
                    let ids: Vec<String> = if list {
                        tree.iter()
                            .filter_map(|r| match r {
                                ChoiceRow::Item(id, _) => Some(id.to_string()),
                                ChoiceRow::Group(..) => None,
                            })
                            .collect()
                    } else {
                        catalog
                            .iter()
                            .filter(|c| d.open_groups.contains(&c.group))
                            .map(|c| c.id.clone())
                            .collect()
                    };
                    for id in ids {
                        add_column(&mut d, &catalog, &id);
                    }
                }
                Chooser::REMOVE => {
                    if let Some(s) = d.selected.filter(|s| *s < d.columns.len()) {
                        d.columns.remove(s);
                        d.selected = (!d.columns.is_empty()).then(|| s.min(d.columns.len() - 1));
                    }
                }
                Chooser::REMOVE_ALL => {
                    d.columns.clear();
                    d.selected = None;
                }
                Chooser::UP | Chooser::DOWN => {
                    if let Some(s) = d.selected.filter(|s| *s < d.columns.len()) {
                        let to = if key == Chooser::UP { s - 1 } else { s + 1 };
                        d.columns.swap(s, to);
                        d.selected = Some(to);
                    }
                }
                "columns_ok" => {
                    commit = true;
                    close = true;
                }
                "columns_cancel" => close = true,
                "columns_apply" => commit = true,
                "columns_default" => {
                    d.columns = layout::default_columns();
                    d.selected = Some(0);
                }
                "columns_edit" => edit = d.selected,
                "columns_help" => self.open_help_for("grid_columns"),
                _ => {}
            }
        }
        if let Some(index) = edit {
            let setting = &d.columns[index];
            let column = columns::find(&catalog, &setting.id);
            self.grid.dialogs.edit_column = Some(EditColumnDraft {
                index,
                title: setting
                    .title
                    .clone()
                    .unwrap_or_else(|| title_of(&catalog, &setting.id)),
                width: kit::format_number(f64::from(setting.width)),
                align: setting
                    .align
                    .or(column.map(|c| c.align))
                    .unwrap_or(ColAlign::Left),
            });
        }
        if commit {
            if d.columns.is_empty() {
                self.error("Select at least one column.".into());
                close = false;
            } else if d.bottom {
                self.panes.grid.columns = d.columns.clone();
            } else {
                self.grid.layout.columns = d.columns.clone();
                self.drop_frame_cache();
            }
        }
        if close || !open || self.escape(ctx, "columns") {
            open = false;
        }
        if open {
            self.grid.dialogs.columns = Some(d);
        } else {
            available_options::close_menu(ctx);
        }
    }

    fn edit_column_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut e) = self.grid.dialogs.edit_column.take() else {
            return;
        };
        let catalog = columns::catalog(&self.project);
        let Some(setting) = self
            .grid
            .dialogs
            .columns
            .as_ref()
            .and_then(|d| d.columns.get(e.index))
            .cloned()
        else {
            return;
        };
        let default = columns::find(&catalog, &setting.id).cloned();
        let default_title = default
            .as_ref()
            .map_or(setting.id.clone(), |c| c.title.clone());
        let mut done = None;
        // 442 x 130 px inside the frame: the form at the left, OK, Cancel
        // and Default in the button column.
        const W: f32 = 442.;
        const H: f32 = 130.;
        let ((), closed, actions) = kit::window(
            ctx,
            "grid_edit_column",
            "Edit Column",
            Vec2::new(W, H),
            |c| {
                kit::strip(c, W, H);
                kit::text(c, 10., 13., "Default Title:", true);
                kit::text(c, 100., 13., &default_title, true);
                kit::text(c, 10., 41., "New Title:", true);
                c.edit(
                    "edit_column_title",
                    100.,
                    37.,
                    217.,
                    21.,
                    &mut e.title,
                    true,
                    Align::Min,
                );
                kit::text(c, 10., 69., "Width:", true);
                c.spin(
                    "edit_column_width",
                    100.,
                    65.,
                    80.,
                    21.,
                    &mut e.width,
                    1.,
                    20.0..=1500.,
                    true,
                );
                kit::text(c, 10., 97., "Alignment", true);
                for (k, (a, caption)) in [
                    (ColAlign::Left, "Left"),
                    (ColAlign::Center, "Center"),
                    (ColAlign::Right, "Right"),
                ]
                .into_iter()
                .enumerate()
                {
                    let key = format!("edit_column_align_{}", a.key());
                    if c.radio(
                        &key,
                        100. + 72. * k as f32,
                        95.,
                        68.,
                        17.,
                        caption,
                        e.align == a,
                        true,
                    ) {
                        e.align = a;
                    }
                }
                let out = kit::button_column(
                    c,
                    W,
                    &[
                        Entry::ok("edit_column_ok"),
                        Entry::cancel("edit_column_cancel"),
                        Entry::Button("edit_column_default", "Default", true, true),
                    ],
                );
                match out.clicked.as_deref() {
                    Some("edit_column_ok") => done = Some(true),
                    Some("edit_column_cancel") => done = Some(false),
                    Some(_) => {
                        e.title = default_title.clone();
                        e.width = kit::format_number(f64::from(
                            default.as_ref().map_or(80., |c| c.width),
                        ));
                        e.align = default.as_ref().map_or(ColAlign::Left, |c| c.align);
                    }
                    None => {}
                }
            },
        );
        self.record_dialog_actions(actions);
        if closed {
            done = Some(false);
        }
        match done {
            Some(true) => {
                if let Some(d) = self.grid.dialogs.columns.as_mut() {
                    if let Some(s) = d.columns.get_mut(e.index) {
                        let title = e.title.trim();
                        s.title =
                            (!title.is_empty() && title != default_title).then(|| title.into());
                        if let Ok(width) = e.width.trim().parse::<f32>() {
                            s.width = width.clamp(20., 1500.).round();
                        }
                        s.align =
                            (Some(e.align) != default.as_ref().map(|c| c.align)).then_some(e.align);
                    }
                }
            }
            Some(false) => {}
            None if self.escape(ctx, "edit_column") => {}
            None => self.grid.dialogs.edit_column = Some(e),
        }
    }

    fn filters_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.grid.dialogs.filters.take() else {
            return;
        };
        let mut close = false;
        let mut commit = false;
        let mut new_filter = false;
        let mut modify = None;
        // 495 x 441 px inside the frame: match options, the filter list (18 px rows, a
        // yellow strip down the members of each group), the apply options,
        // and the button column at 383.
        use crate::dialog_kit as kit;
        let mut rows: Vec<(Option<&str>, String, String)> =
            vec![(Some("Default"), String::new(), String::new())];
        rows.extend(
            Standard::LISTED
                .iter()
                .map(|s| (None, s.key(), s.caption().to_string())),
        );
        rows.push((Some("Global"), String::new(), String::new()));
        rows.push((Some("User Defined"), String::new(), String::new()));
        rows.extend(d.users.iter().map(|u| (None, u.key(), u.name.clone())));
        let mut clicks: Vec<String> = vec![];
        let mut toggles: Vec<(String, bool)> = vec![];
        let ((), closed, actions) =
            kit::window(ctx, "grid_filters", "Filters", Vec2::new(495., 441.), |c| {
                let current_user = d
                    .current
                    .as_ref()
                    .and_then(|k| d.users.iter().position(|u| &u.key() == k));
                c.fill(0., 0., 495., 441., STRIP);
                c.fill(5., 5., 373., 339., CHROME);
                c.lowered(5., 5., 373., 339.);
                let mut all = d.selection.all_activities;
                c.checkbox(
                    "filters_all_activities",
                    12.,
                    10.,
                    161.,
                    18.,
                    "All Activities",
                    &mut all,
                    true,
                );
                d.selection.all_activities = all;
                kit::text(c, 182., 11., "Show activities that match", true);
                if c.radio(
                    "filters_match_all",
                    184.,
                    26.,
                    190.,
                    16.,
                    "All selected filters",
                    d.selection.match_all,
                    true,
                ) {
                    d.selection.match_all = true;
                }
                if c.radio(
                    "filters_match_any",
                    184.,
                    42.,
                    190.,
                    17.,
                    "Any selected filter",
                    !d.selection.match_all,
                    true,
                ) {
                    d.selection.match_all = false;
                }
                // List: header, then rows.
                let (x, w) = (6., 371.);
                c.fill(x, 61., w, 282., Color32::WHITE);
                c.fill(x, 61., w, 18., CHROME);
                c.hline(x, 61., w, Color32::WHITE);
                c.hline(x, 78., w, INK);
                c.vline(x + 292., 61., 17., INK);
                c.vline(x + 293., 61., 17., Color32::WHITE);
                kit::text(c, x + 3., 64., "Filter", true);
                kit::text(c, x + 309., 64., "Select", true);
                let body = c.r(x, 79., w, 264.);
                let current = d.current.clone();
                let selected = d.selection.selected.clone();
                let reveal = current
                    .as_ref()
                    .and_then(|k| rows.iter().position(|row| row.0.is_none() && &row.1 == k));
                let out = kit::rows_in(
                    c,
                    "filters_list",
                    body,
                    18.,
                    rows.len(),
                    reveal,
                    &mut |c, r, rect, _| {
                        let p = c.ui.painter().with_clip_rect(body);
                        let line = Color32::from_gray(0xC0);
                        let (group, key, caption) = &rows[r];
                        if let Some(g) = group {
                            p.rect_filled(rect, 0., FILTER_GROUP);
                            let b =
                                Rect::from_min_size(rect.min + Vec2::new(3., 4.), Vec2::splat(9.));
                            p.rect_filled(b, 0., Color32::WHITE);
                            p.rect_stroke(
                                b,
                                0.,
                                Stroke::new(1_f32, Color32::from_gray(0x80)),
                                egui::StrokeKind::Inside,
                            );
                            p.rect_filled(
                                Rect::from_min_size(b.min + Vec2::new(2., 4.), Vec2::new(5., 1.)),
                                0.,
                                INK,
                            );
                            kit::cell_text(c, rect, g, false, 32.);
                            return;
                        }
                        let sel = current.as_deref() == Some(key.as_str());
                        p.rect_filled(
                            Rect::from_min_size(rect.min, Vec2::new(22., rect.height())),
                            0.,
                            FILTER_GROUP,
                        );
                        let name = Rect::from_min_max(
                            Pos2::new(rect.left() + 22., rect.top()),
                            Pos2::new(rect.left() + 292., rect.bottom()),
                        );
                        if sel {
                            p.rect_filled(name, 0., kit::SELECTED);
                        }
                        p.rect_filled(
                            Rect::from_min_size(
                                Pos2::new(rect.left() + 22., rect.bottom() - 1.),
                                Vec2::new(rect.width() - 22., 1.),
                            ),
                            0.,
                            line,
                        );
                        p.rect_filled(
                            Rect::from_min_size(
                                Pos2::new(rect.left() + 292., rect.top()),
                                Vec2::new(1., rect.height()),
                            ),
                            0.,
                            line,
                        );
                        chrome::paint_icon_at(
                            &p,
                            chrome::Icon::Filter,
                            Pos2::new(rect.left() + 32., rect.top() + 1.),
                            true,
                        );
                        kit::cell_text(c, name, caption, sel, 29.);
                        c.act(&format!("filter_row_{key}"), name.intersect(body));
                        let on = selected.contains(key);
                        let bx = rect.left() + 316.;
                        let local = Pos2::new(bx, rect.top() + 2.) - c.origin.to_vec2();
                        kit::check_glyph(c, local.x, local.y, on, true);
                        let hit_rect =
                            Rect::from_min_size(Pos2::new(bx, rect.top() + 2.), Vec2::splat(13.))
                                .intersect(body);
                        let hit =
                            c.ui.interact(hit_rect, c.id("filter_select").with(r), Sense::click());
                        c.act(&format!("filter_select_{key}"), hit_rect);
                        if hit.clicked() {
                            toggles.push((key.clone(), !on));
                        }
                    },
                );
                if let Some(r) = out.clicked {
                    if rows[r].0.is_none() {
                        d.current = Some(rows[r].1.clone());
                    }
                }
                if let Some(r) = out.double {
                    let key = &rows[r].1;
                    if key.starts_with("user:") {
                        modify = d.users.iter().position(|u| &u.key() == key);
                    }
                }
                // Apply options.
                c.fill(5., 349., 373., 87., CHROME);
                c.lowered(5., 349., 373., 87.);
                kit::heading(c, 6., 350., 371., "");
                kit::text(c, 12., 353., "When applying the selected filter(s):", true);
                if c.radio(
                    "filters_replace",
                    13.,
                    381.,
                    360.,
                    17.,
                    "Replace activities shown in current layout",
                    !d.selection.highlight,
                    true,
                ) {
                    d.selection.highlight = false;
                }
                if c.radio(
                    "filters_highlight",
                    13.,
                    405.,
                    360.,
                    17.,
                    "Highlight activities in current layout which match criteria",
                    d.selection.highlight,
                    true,
                ) {
                    d.selection.highlight = true;
                }
                let user = current_user.is_some();
                let out = kit::button_column(
                    c,
                    495.,
                    &[
                        Entry::ok("filters_ok"),
                        Entry::cancel("filters_cancel"),
                        Entry::apply("filters_apply", true),
                        Entry::Button("filters_new", "New...", true, false),
                        Entry::Button("filters_delete", "Delete", user, false),
                        Entry::Button("filters_copy", "Copy", user, false),
                        Entry::Button("filters_paste", "Paste", false, false),
                        Entry::Button("filters_modify", "Modify...", user, true),
                        Entry::Button("filters_make_global", "Make Global", false, true),
                        Entry::Button("filters_copy_as_layout", "Copy As Layout", false, true),
                        Entry::help("filters_help"),
                    ],
                );
                clicks.extend(out.clicked);
            });
        self.record_dialog_actions(actions);
        close |= closed;
        for (key, on) in toggles {
            if on {
                d.selection.selected.push(key);
                d.selection.all_activities = false;
            } else {
                d.selection.selected.retain(|k| *k != key);
            }
        }
        let current_user = d
            .current
            .as_ref()
            .and_then(|k| d.users.iter().position(|u| &u.key() == k));
        for key in clicks {
            match key.as_str() {
                "filters_ok" => {
                    commit = true;
                    close = true;
                }
                "filters_cancel" => close = true,
                "filters_apply" => commit = true,
                "filters_new" => new_filter = true,
                // Delete asks first (`remove_prompt_window`).
                "filters_delete" => {
                    if let Some(n) = current_user {
                        self.grid.dialogs.remove_prompt =
                            Some(GridRemoval::Filter(d.users[n].key()));
                    }
                }
                "filters_copy" => {
                    if let Some(n) = current_user {
                        let mut copy = d.users[n].clone();
                        let mut name = format!("Copy of {}", copy.name);
                        while d.users.iter().any(|u| u.name == name) {
                            name.push_str(" (2)");
                        }
                        copy.name = name;
                        d.current = Some(copy.key());
                        d.users.push(copy);
                    }
                }
                "filters_modify" => modify = current_user,
                "filters_help" => self.open_help_for("grid_filters"),
                _ => {}
            }
        }
        if new_filter {
            let mut name = "(Untitled Filter)".to_string();
            let mut n = 2;
            while d.users.iter().any(|u| u.name == name) {
                name = format!("(Untitled Filter {n})");
                n += 1;
            }
            self.grid.dialogs.filter_editor = Some(FilterEditor {
                original: None,
                filter: UserFilter {
                    name,
                    match_all: true,
                    rows: vec![Criterion::default()],
                },
                selected: Some(0),
            });
        }
        if let Some(n) = modify {
            self.grid.dialogs.filter_editor = Some(FilterEditor {
                original: Some(d.users[n].name.clone()),
                filter: d.users[n].clone(),
                selected: None,
            });
        }
        if commit {
            self.grid.layout.filter = d.selection.clone();
            self.grid.user_filters = d.users.clone();
            self.save_user_filters();
        }
        if !(close || self.escape(ctx, "filters")) {
            self.grid.dialogs.filters = Some(d);
        }
    }

    fn group_sort_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.grid.dialogs.group_sort.take() else {
            return;
        };
        let catalog = columns::catalog(&self.project);
        let choices = groupable(&catalog);
        let wbs_depth = {
            let ix = columns::Index::default();
            let depth = ix
                .wbs(&self.project)
                .tree
                .preorder()
                .iter()
                .map(|w| w.1 + 1)
                .max();
            depth.unwrap_or(1)
        };
        let mut close = false;
        let mut commit = false;
        let mut open_sort = false;
        // 694 x 411 px inside the frame: Display Options, the Group By grid and Group By
        // Options sections, the button column at 582.
        use crate::dialog_kit as kit;
        let mut clicks: Vec<&'static str> = vec![];
        let ((), closed, actions) = kit::window(
            ctx,
            "grid_group_sort",
            "Group and Sort",
            Vec2::new(694., 411.),
            |c| {
                c.fill(0., 0., 694., 411., STRIP);
                c.fill(0., 0., 582., 411., CHROME);
                kit::section(c, 5., 5., 572., 92., "Display Options");
                c.checkbox(
                    "group_totals",
                    19.,
                    31.,
                    179.,
                    21.,
                    "Show Group Totals",
                    &mut d.gs.show_group_totals,
                    true,
                );
                c.checkbox(
                    "group_grand_totals",
                    38.,
                    51.,
                    411.,
                    20.,
                    "Show Grand Totals",
                    &mut d.gs.show_grand_totals,
                    true,
                );
                c.checkbox(
                    "group_summaries_only",
                    38.,
                    73.,
                    459.,
                    20.,
                    "Show Summaries Only",
                    &mut d.gs.show_summaries_only,
                    true,
                );
                // Group By grid.
                kit::section(c, 5., 102., 572., 139., "Group By");
                let (gx, gy) = (6., 124.);
                c.fill(gx, gy, 570., 116., Color32::WHITE);
                let cols: [(&str, f32); 6] = [
                    ("Group By", 140.),
                    ("Indent", 44.),
                    ("To Level", 64.),
                    ("Group Interval", 82.),
                    ("Page Break", 100.),
                    ("Font & Color", 123.),
                ];
                c.fill(gx, gy, 570., 18., CHROME);
                c.hline(gx, gy, 570., Color32::WHITE);
                c.hline(gx, gy + 17., 570., INK);
                let mut x = gx;
                for (title, w) in cols {
                    c.vline(x + w - 1., gy, 17., INK);
                    kit::text(c, x + 3., gy + 3., title, true);
                    x += w;
                }
                let body = c.r(gx, gy + 18., 570., 98.);
                // One grid row per band level: a WBS level lists its WBS
                // levels below it, grayed, then a blank row adds a level.
                let rows = group_rows(&d.gs, wbs_depth);
                let selected = d.selected;
                let editing = d.editing;
                let mut set_selected = None;
                let mut set_editing = None;
                let mut add = None;
                let mut edit_band = None;
                let mut toggle_break = None;
                let gs = &mut d.gs;
                let out = kit::rows_in(
                    c,
                    "group_levels",
                    body,
                    18.,
                    rows.len(),
                    selected,
                    &mut |c, n, rect, _| {
                        let p = c.ui.painter().with_clip_rect(body);
                        let line = Color32::from_gray(0xC0);
                        let mut x = rect.left();
                        let mut cells = [Rect::NOTHING; 6];
                        for (k, (_, w)) in cols.iter().enumerate() {
                            cells[k] =
                                Rect::from_min_size(Pos2::new(x, rect.top()), Vec2::new(*w, 18.));
                            p.rect_filled(
                                Rect::from_min_size(
                                    Pos2::new(x + w - 1., rect.top()),
                                    Vec2::new(1., 18.),
                                ),
                                0.,
                                line,
                            );
                            x += w;
                        }
                        p.rect_filled(
                            Rect::from_min_size(
                                Pos2::new(rect.left(), rect.bottom() - 1.),
                                Vec2::new(x - rect.left(), 1.),
                            ),
                            0.,
                            line,
                        );
                        let inner = |k: usize| {
                            Rect::from_min_max(cells[k].min, cells[k].max - Vec2::new(1., 1.))
                        };
                        let is_editing = |k: u8| editing == Some((n, k));
                        // A click on the selected row's cell starts its editor.
                        let mut clicked_cell = |k: u8, clicked: bool| {
                            if clicked {
                                if selected == Some(n) {
                                    set_editing = Some((n, k));
                                }
                                set_selected = Some(n);
                            }
                        };
                        let row = rows[n];
                        // Group By.
                        let caption = match row {
                            GroupRow::Level(l) if gs.levels[l].column == "wbs" => {
                                "WBS level 1".to_string()
                            }
                            GroupRow::Level(l) => title_of(&catalog, &gs.levels[l].column),
                            GroupRow::Wbs(_, k) => format!("WBS level {}", k + 1),
                            GroupRow::New => String::new(),
                        };
                        let titles: Vec<String> = choices.iter().map(|c| c.1.clone()).collect();
                        let r = super::cell_combo::cell_combo(
                            c,
                            &format!("group_by_{n}"),
                            inner(0),
                            body,
                            &caption,
                            &titles,
                            is_editing(0) && !matches!(row, GroupRow::Wbs(..)),
                            false,
                            !matches!(row, GroupRow::Wbs(..)),
                        );
                        clicked_cell(0, r.clicked);
                        if let Some(k) = r.picked {
                            let id = choices[k].0.clone();
                            match row {
                                GroupRow::Level(l) => gs.levels[l].column = id,
                                GroupRow::New => add = Some(id),
                                GroupRow::Wbs(..) => {}
                            }
                        }
                        if selected == Some(n) && editing != Some((n, 0)) {
                            p.rect_stroke(
                                cells[0].shrink(1.),
                                0.,
                                Stroke::new(2_f32, kit::FOCUS),
                                egui::StrokeKind::Middle,
                            );
                        }
                        // Page Break and Font & Color: every band level.
                        if row != GroupRow::New {
                            let on = gs.page_breaks.contains(&n);
                            let bx = (cells[4].center().x - 6.5).floor();
                            let local = Pos2::new(bx, rect.top() + 2.) - c.origin.to_vec2();
                            kit::check_glyph(c, local.x, local.y, on, true);
                            let hit = Rect::from_min_size(
                                Pos2::new(bx, rect.top() + 2.),
                                Vec2::splat(13.),
                            )
                            .intersect(body);
                            if c.ui
                                .interact(hit, c.id("group_page_break").with(n), Sense::click())
                                .clicked()
                            {
                                toggle_break = Some(n);
                            }
                            c.act(&format!("group_page_break_{n}"), hit);
                            let style = gs.band_style(n);
                            let sample = cells[5].shrink2(Vec2::new(1., 0.)).intersect(body);
                            let [r_, g_, b_] = style.background;
                            p.rect_filled(sample, 0., Color32::from_rgb(r_, g_, b_));
                            let [r_, g_, b_] = style.text;
                            let family = if style.bold {
                                chrome::bold_font().family
                            } else {
                                FontId::proportional(FONT_SIZE).family
                            };
                            p.text(
                                sample.left_center() + Vec2::new(4., 0.),
                                Align2::LEFT_CENTER,
                                format!("{} Arial", style.font_size),
                                FontId::new((style.font_size * 4. / 3.).min(17.), family),
                                Color32::from_rgb(r_, g_, b_),
                            );
                            if c.ui
                                .interact(sample, c.id("group_band").with(n), Sense::click())
                                .clicked()
                            {
                                edit_band = Some(n);
                            }
                            c.act(&format!("group_band_{n}"), sample);
                        }
                        let GroupRow::Level(l) = row else {
                            return;
                        };
                        let level = &mut gs.levels[l];
                        // Indent.
                        let bx = (cells[1].center().x - 6.5).floor();
                        let local = Pos2::new(bx, rect.top() + 2.) - c.origin.to_vec2();
                        kit::check_glyph(c, local.x, local.y, level.indent, true);
                        let hit =
                            Rect::from_min_size(Pos2::new(bx, rect.top() + 2.), Vec2::splat(13.))
                                .intersect(body);
                        if c.ui
                            .interact(hit, c.id("group_indent").with(n), Sense::click())
                            .clicked()
                        {
                            level.indent = !level.indent;
                        }
                        c.act(&format!("group_indent_{n}"), hit);
                        // To Level, for WBS.
                        if level.column == "wbs" {
                            let items: Vec<String> = std::iter::once("All".to_string())
                                .chain((1..=10).map(|l| l.to_string()))
                                .collect();
                            let text = level.to_level.map_or("All".into(), |l| l.to_string());
                            let r = super::cell_combo::cell_combo(
                                c,
                                &format!("group_to_{n}"),
                                inner(2),
                                body,
                                &text,
                                &items,
                                is_editing(2),
                                false,
                                true,
                            );
                            clicked_cell(2, r.clicked);
                            if let Some(k) = r.picked {
                                level.to_level = (k > 0).then_some(k);
                            }
                            return;
                        }
                        // Group Interval: periods for dates, a step for numbers.
                        let kind = columns::find(&catalog, &level.column).map(|c| c.kind);
                        match kind {
                            Some(Kind::Date) => {
                                let items: Vec<String> = std::iter::once(String::new())
                                    .chain(
                                        super::interval::DATE_INTERVALS
                                            .iter()
                                            .map(|(_, t)| t.to_string()),
                                    )
                                    .collect();
                                let text = super::interval::DATE_INTERVALS
                                    .iter()
                                    .find(|(k, _)| *k == level.interval)
                                    .map_or("", |(_, t)| t);
                                let r = super::cell_combo::cell_combo(
                                    c,
                                    &format!("group_interval_{n}"),
                                    inner(3),
                                    body,
                                    text,
                                    &items,
                                    is_editing(3),
                                    false,
                                    true,
                                );
                                clicked_cell(3, r.clicked);
                                if let Some(k) = r.picked {
                                    level.interval = if k == 0 {
                                        String::new()
                                    } else {
                                        super::interval::DATE_INTERVALS[k - 1].0.to_string()
                                    };
                                }
                            }
                            Some(Kind::Text | Kind::Flag) | None => {}
                            Some(_) => {
                                let cell = inner(3).shrink2(Vec2::new(3., 1.)).intersect(body);
                                let edit = egui::TextEdit::singleline(&mut level.interval)
                                    .id(c.id("group_interval").with(n))
                                    .frame(false)
                                    .font(crate::schedule_dialogs::dialog_font())
                                    .text_color(INK)
                                    .margin(egui::Margin::ZERO)
                                    .desired_width(cell.width());
                                let r = c.ui.put(cell, edit);
                                c.act(&format!("group_interval_{n}"), cell);
                                clicked_cell(3, r.clicked());
                            }
                        }
                    },
                );
                if let Some(n) = out.clicked.or(set_selected) {
                    if d.selected != Some(n) {
                        d.editing = None;
                    }
                    d.selected = Some(n);
                }
                if let Some(cell) = set_editing {
                    d.editing = Some(cell);
                    super::cell_combo::set_open(
                        c,
                        &format!("{}_{}", EDITOR_KEYS[cell.1 as usize], cell.0),
                        true,
                    );
                }
                if let Some(column) = add {
                    if !d.gs.levels.iter().any(|l| l.column == column) {
                        d.gs.levels.push(GroupLevel {
                            column,
                            to_level: None,
                            indent: true,
                            interval: String::new(),
                        });
                    }
                }
                if let Some(n) = toggle_break {
                    if let Some(at) = d.gs.page_breaks.iter().position(|b| *b == n) {
                        d.gs.page_breaks.remove(at);
                    } else {
                        d.gs.page_breaks.push(n);
                        d.gs.page_breaks.sort_unstable();
                    }
                }
                if let Some(n) = edit_band {
                    d.band_edit = if d.band_edit == Some(n) {
                        None
                    } else {
                        Some(n)
                    };
                }
                // Group By Options.
                kit::section(c, 5., 246., 572., 160., "Group By Options");
                c.checkbox(
                    "group_hide_empty",
                    19.,
                    272.,
                    298.,
                    20.,
                    "Hide if empty",
                    &mut d.gs.hide_if_empty,
                    true,
                );
                c.checkbox(
                    "group_alpha",
                    19.,
                    294.,
                    298.,
                    20.,
                    "Sort bands alphabetically",
                    &mut d.gs.sort_bands_alphabetically,
                    true,
                );
                kit::text(c, 18., 318., "Show :", true);
                c.checkbox(
                    "group_show_title",
                    19.,
                    336.,
                    298.,
                    21.,
                    "Title",
                    &mut d.gs.show_title,
                    true,
                );
                c.checkbox(
                    "group_show_code",
                    19.,
                    358.,
                    298.,
                    21.,
                    "ID / Code",
                    &mut d.gs.show_code,
                    true,
                );
                c.checkbox(
                    "group_show_name",
                    19.,
                    380.,
                    298.,
                    21.,
                    "Name / Description",
                    &mut d.gs.show_name,
                    true,
                );
                let can = d.selected.is_some_and(|s| {
                    !matches!(
                        group_rows(&d.gs, wbs_depth).get(s),
                        Some(GroupRow::New) | None
                    )
                });
                let entries = [
                    Entry::ok("group_ok"),
                    Entry::cancel("group_cancel"),
                    Entry::apply("group_apply", true),
                    Entry::Button("group_delete", "Delete", can, false),
                    Entry::Button("group_sort", "&Sort...", true, true),
                    Entry::Button("group_default", "Default", true, false),
                    Entry::help("group_help"),
                ];
                let out = kit::button_column(c, 694., &entries);
                clicks.extend(clicked_key(&entries, out.clicked.as_deref()));
            },
        );
        self.record_dialog_actions(actions);
        close |= closed;
        // The Font & Color editor of one level, beside the grid: every level
        // the grid lists, its style filled in when the layout holds fewer.
        if let Some(n) = d.band_edit.filter(|n| *n < super::layout::MAX_BAND_LEVELS) {
            while d.gs.bands.len() <= n {
                let next = d.gs.band_style(d.gs.bands.len());
                d.gs.bands.push(next);
            }
            let anchor = ctx
                .data(|m| {
                    m.get_temp::<Rect>(egui::Id::new((
                        "dialog_client",
                        "grid_group_sort".to_owned(),
                    )))
                })
                .map_or(Pos2::new(200., 200.), |r| r.min + Vec2::new(330., 280.));
            // A small window beside the grid: the level's colors, font size
            // and weight, and Close.
            const W: f32 = 330.;
            const H: f32 = 70.;
            let style = &mut d.gs.bands[n];
            let title = format!("Level {} Font & Color", n + 1);
            let size_id = egui::Id::new(("group_band_size", n));
            let mut size = ctx
                .data(|m| m.get_temp::<String>(size_id))
                .unwrap_or_else(|| kit::format_number(f64::from(style.font_size)));
            let out = kit::shell(
                ctx,
                kit::Shell {
                    name: "group_band_editor",
                    id: egui::Id::new("group_band_editor"),
                    title: &title,
                    client: Vec2::new(W, H),
                    layering: kit::Layering::Window,
                    at: Some(anchor),
                    extras: kit::Extras::default(),
                },
                |ui, origin| {
                    let mut c = kit::Canvas {
                        ui,
                        origin,
                        salt: "group_band_editor",
                        actions: vec![],
                    };
                    kit::strip(&c, W, H);
                    for (k, (label, color)) in [
                        ("Background", &mut style.background),
                        ("Text", &mut style.text),
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        let x = 8. + 110. * k as f32;
                        kit::text(&c, x, 12., label, true);
                        let mut rgb = crate::gantt::settings::Rgb(color[0], color[1], color[2]);
                        kit::swatch(
                            &mut c,
                            &format!("group_band_{}", label.to_lowercase()),
                            (x + 62., 8., 36., 21.),
                            &mut rgb,
                            true,
                        );
                        *color = [rgb.0, rgb.1, rgb.2];
                    }
                    kit::text(&c, 8., 43., "Size", true);
                    if c.spin(
                        "group_band_font_size",
                        70.,
                        39.,
                        60.,
                        21.,
                        &mut size,
                        1.,
                        6.0..=24.,
                        true,
                    ) {
                        if let Ok(v) = size.trim().parse::<f32>() {
                            style.font_size = v.clamp(6., 24.);
                            // The band row follows its font, as in Font & Color.
                            style.row_height = super::layout::band_row_height(style.font_size);
                        }
                        size = kit::format_number(f64::from(style.font_size));
                    }
                    c.checkbox(
                        "group_band_bold",
                        150.,
                        41.,
                        60.,
                        17.,
                        "Bold",
                        &mut style.bold,
                        true,
                    );
                    let close = kit::button_column(&mut c, W, &[Entry::close("group_band_close")])
                        .clicked
                        .is_some();
                    (close, c.actions)
                },
            );
            let (close, band_actions) = out.inner;
            self.record_dialog_actions(band_actions);
            ctx.data_mut(|m| m.insert_temp(size_id, size));
            if close || out.closed {
                ctx.data_mut(|m| m.remove::<String>(size_id));
                d.band_edit = None;
            }
        }
        for key in clicks {
            match key {
                "group_ok" => {
                    commit = true;
                    close = true;
                }
                "group_cancel" => close = true,
                "group_apply" => commit = true,
                "group_delete" => {
                    let rows = group_rows(&d.gs, wbs_depth);
                    let level = match d.selected.and_then(|s| rows.get(s)) {
                        Some(GroupRow::Level(n) | GroupRow::Wbs(n, _)) => Some(*n),
                        _ => None,
                    };
                    if let Some(n) = level {
                        d.gs.levels.remove(n);
                        d.selected = None;
                        d.editing = None;
                    }
                }
                "group_sort" => open_sort = true,
                "group_default" => d.gs = GroupSort::default(),
                "group_help" => self.open_help_for("grid_group_sort"),
                _ => {}
            }
        }
        if open_sort {
            self.grid.dialogs.sort = Some(SortDraft {
                keys: d.gs.sort.clone(),
                selected: Some(0),
            });
        }
        if commit {
            d.gs.levels.retain(|l| !l.column.is_empty());
            let bad = d.gs.levels.iter().find_map(|l| {
                let c = columns::find(&catalog, &l.column)?;
                super::interval::check(&self.project, c.kind, &l.interval)
                    .err()
                    .map(|e| format!("{} Group Interval: {e}", c.title))
            });
            if let Some(e) = bad {
                self.error(e);
                close = false;
            } else {
                self.grid.layout.group_sort = d.gs.clone();
                self.collapsed.clear();
            }
        }
        if !(close || self.escape(ctx, "group_sort")) {
            self.grid.dialogs.group_sort = Some(d);
        }
    }

    /// Sort (from Group and Sort): the shared sort-key table in a lowered
    /// panel and the standard button column with Help last.
    fn sort_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.grid.dialogs.sort.take() else {
            return;
        };
        let catalog = columns::catalog(&self.project);
        let choices: Vec<(String, String)> = catalog
            .iter()
            .map(|c| (c.id.clone(), c.title.clone()))
            .collect();
        let mut done = None;
        let mut help = false;
        let (w, h) = (452., 330.);
        let ((), closed, actions) = kit::window(ctx, "grid_sort", "Sort", Vec2::new(w, h), |c| {
            c.fill(0., 0., w, h, STRIP);
            c.fill(0., 0., w - kit::STRIP_WIDTH, h, CHROME);
            super::sort_keys::table(
                c,
                "sort",
                (5., 5., w - kit::STRIP_WIDTH - 10., h - 10.),
                [220., 108.],
                &choices,
                &mut d.keys,
                &mut d.selected,
            );
            let s = d.selected.filter(|s| *s < d.keys.len());
            let entries = [
                Entry::ok("sort_ok"),
                Entry::cancel("sort_cancel"),
                Entry::Button("sort_add", "Add", true, true),
                Entry::Button("sort_delete", "Delete", s.is_some(), false),
                Entry::Button("sort_up", "Move Up", s.is_some_and(|s| s > 0), false),
                Entry::Button(
                    "sort_down",
                    "Move Down",
                    s.is_some_and(|s| s + 1 < d.keys.len()),
                    false,
                ),
                Entry::help("sort_help"),
            ];
            let clicked = kit::button_column(c, w, &entries).clicked;
            match clicked.as_deref() {
                Some("sort_ok") => done = Some(true),
                Some("sort_cancel") => done = Some(false),
                Some("sort_add") => {
                    d.keys.push(SortKey {
                        column: "activity_id".into(),
                        descending: false,
                    });
                    d.selected = Some(d.keys.len() - 1);
                }
                Some("sort_delete") => {
                    if let Some(s) = s {
                        d.keys.remove(s);
                        d.selected = None;
                    }
                }
                Some("sort_up") => {
                    if let Some(s) = s.filter(|s| *s > 0) {
                        d.keys.swap(s, s - 1);
                        d.selected = Some(s - 1);
                    }
                }
                Some("sort_down") => {
                    if let Some(s) = s.filter(|s| s + 1 < d.keys.len()) {
                        d.keys.swap(s, s + 1);
                        d.selected = Some(s + 1);
                    }
                }
                Some("sort_help") => help = true,
                _ => {}
            }
        });
        self.record_dialog_actions(actions);
        if help {
            self.open_help_for("grid_sort");
        }
        if closed && done.is_none() {
            done = Some(false);
        }
        self.sort_finish(ctx, d, done);
    }

    /// OK, Cancel and Escape of Sort.
    fn sort_finish(&mut self, ctx: &egui::Context, d: SortDraft, done: Option<bool>) {
        match done {
            Some(true) => match self.grid.dialogs.group_sort.as_mut() {
                Some(g) => g.gs.sort = d.keys,
                None => self.grid.layout.group_sort.sort = d.keys,
            },
            Some(false) => {}
            None if self.escape(ctx, "sort") => {}
            None => self.grid.dialogs.sort = Some(d),
        }
    }

    fn open_layout_window(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.grid.dialogs.open_layout.take() else {
            return;
        };
        let assignments = self.grid.dialogs.layout_target == LayoutTarget::Assignments;
        let names = if assignments {
            self.assignment_layout_names()
        } else {
            self.layout_names()
        };
        let library: Vec<String> = if assignments {
            self.assignment_layouts()
                .into_iter()
                .map(|l| l.name)
                .collect()
        } else {
            self.grid.library.iter().map(|l| l.name.clone()).collect()
        };
        let mut close = false;
        let mut open_name = None;
        // Apply shows the layout and keeps the window open.
        let mut apply = false;
        let mut delete = None;
        let (mut import, mut export) = (false, false);
        // Same anatomy as the other list dialogs: the list in a lowered
        // panel and the 112 px button column.
        use crate::dialog_kit::{self as kit, Col};
        let picked = d.selected.clone().filter(|n| names.contains(n));
        let saved = picked.as_ref().is_some_and(|n| library.contains(n));
        let shipped: Vec<String> = if assignments {
            vec![]
        } else {
            super::starters::layouts()
                .into_iter()
                .map(|(_, l, _)| l.name)
                .collect()
        };
        let lines = layout_lines(&names, &library, &shipped);
        let selected = picked.as_ref().and_then(|p| {
            lines
                .iter()
                .position(|l| matches!(l, LayoutLine::Layout(i) if names[*i] == *p))
        });
        let mut help = false;
        let ((), closed, actions) = kit::window(
            ctx,
            "grid_open_layout",
            "Open Layout",
            Vec2::new(424., 312.),
            |c| {
                c.fill(0., 0., 424., 312., STRIP);
                c.fill(5., 5., 302., 302., CHROME);
                c.lowered(5., 5., 302., 302.);
                let heading = if assignments {
                    "Resource Assignment Layouts"
                } else {
                    "Activity Layouts"
                };
                kit::heading(c, 6., 6., 300., heading);
                let band = crate::views::grid::band_style(
                    0,
                    crate::views::grid::BandPitch::Fixed(kit::ROW),
                );
                let out = kit::list(
                    c,
                    "open_layout",
                    (10., 33., 292., 245.),
                    None,
                    &[Col {
                        title: "Layout",
                        width: 272.,
                    }],
                    lines.len(),
                    selected,
                    Some(0),
                    &mut |c, r, _, rect, sel| match &lines[r] {
                        // A scope band: whom the layouts under it belong to.
                        LayoutLine::Band(caption) => {
                            let fill = crate::grid::band::fill(&band, false);
                            c.ui.painter().rect_filled(rect, 0., fill);
                            crate::grid::band::caption(
                                c.ui.painter(),
                                rect,
                                rect.left() + 4.,
                                rect.top(),
                                &band,
                                caption,
                                false,
                            );
                        }
                        LayoutLine::Layout(i) => {
                            kit::cell_text(c, rect, &names[*i], sel, 16.);
                            c.act(&format!("open_layout_item_{i}"), rect);
                        }
                    },
                );
                let layout_at = |r: usize| match lines.get(r) {
                    Some(LayoutLine::Layout(i)) => Some(names[*i].clone()),
                    _ => None,
                };
                if let Some(name) = out.clicked.and_then(layout_at) {
                    d.selected = Some(name);
                }
                if let Some(name) = out.double.and_then(layout_at) {
                    open_name = Some(name);
                }
                // Assignment layouts carry no filter.
                if !assignments {
                    c.checkbox(
                        "open_layout_use_filter",
                        10.,
                        283.,
                        290.,
                        18.,
                        "Use filter selection saved with layout",
                        &mut d.use_filter,
                        true,
                    );
                }
                let mut entries = vec![
                    Entry::Button("open_layout_open", "&Open", picked.is_some(), false),
                    Entry::cancel("open_layout_cancel"),
                    Entry::apply("open_layout_apply", picked.is_some()),
                    Entry::Button("open_layout_delete", "&Delete", saved, false),
                ];
                // Layout files hold Activities layouts only.
                if !assignments {
                    entries.extend([
                        Entry::Button("open_layout_import", "&Import...", true, true),
                        Entry::Button("open_layout_export", "E&xport...", picked.is_some(), false),
                    ]);
                }
                entries.push(Entry::help("open_layout_help"));
                match kit::button_column(c, 424., &entries).clicked.as_deref() {
                    Some("open_layout_open") => open_name = picked.clone(),
                    Some("open_layout_cancel") => close = true,
                    Some("open_layout_apply") => {
                        apply = true;
                        open_name = picked.clone();
                    }
                    Some("open_layout_delete") => delete = picked.clone(),
                    Some("open_layout_import") => import = true,
                    Some("open_layout_export") => export = true,
                    Some(_) => help = true,
                    None => {}
                }
            },
        );
        self.record_dialog_actions(actions);
        close |= closed;
        if help {
            self.open_help_for("grid_open_layout");
        }
        // Delete asks first (`remove_prompt_window`).
        if let Some(name) = delete {
            self.grid.dialogs.remove_prompt = Some(GridRemoval::Layout(name));
        }
        if import {
            if let Some(path) = layout_file_dialog(None) {
                match self.import_layout_file(&path) {
                    Ok(name) => d.selected = Some(name),
                    Err(e) => self.error(e),
                }
            }
        }
        if let Some(name) = picked.filter(|_| export) {
            if let Some(path) = layout_file_dialog(Some(&name)) {
                if let Err(e) = self.export_layout_file(&name, &path) {
                    self.error(e);
                }
            }
        }
        if let Some(name) = open_name {
            let changed = if assignments {
                self.assignment_layout_changed()
            } else {
                self.layout_changed()
            };
            if changed {
                self.grid.dialogs.save_prompt = Some((name, d.use_filter));
            } else if let Err(e) = self.open_layout_of_target(&name, d.use_filter) {
                self.error(e);
            }
            // Open closes the window; Apply keeps it for another try.
            close |= !apply;
        }
        if !(close || self.escape(ctx, "open_layout")) {
            self.grid.dialogs.open_layout = Some(d);
        }
    }

    fn save_prompt_window(&mut self, ctx: &egui::Context) {
        let Some((name, use_filter)) = self.grid.dialogs.save_prompt.clone() else {
            return;
        };
        let mut rects = vec![];
        let mut answer = kit::message_box(
            ctx,
            "prompt",
            "Layout",
            kit::MessageIcon::Question,
            "Would you like to save the changes to this layout?",
            &["Yes", "No", "Cancel"],
            kit::Question::SaveLayoutChanges.default_in(&["Yes", "No", "Cancel"]),
            &mut rects,
        )
        .map(|k| ["prompt_yes", "prompt_no", "prompt_cancel"][k]);
        self.record_dialog_actions(rects);
        if self.escape(ctx, "prompt") {
            answer = Some("prompt_cancel");
        }
        let Some(answer) = answer else {
            return;
        };
        self.grid.dialogs.save_prompt = None;
        if answer == "prompt_cancel" {
            return;
        }
        if answer == "prompt_yes" {
            let result = match self.grid.dialogs.layout_target {
                LayoutTarget::Activities => {
                    let current = self.grid.layout.name.clone();
                    self.save_layout_as(&current)
                }
                LayoutTarget::Assignments => {
                    let current = self.views.assignments.layout_name.clone();
                    self.save_assignment_layout_as(&current)
                }
            };
            if let Err(e) = result {
                self.error(e);
                return;
            }
        }
        if let Err(e) = self.open_layout_of_target(&name, use_filter) {
            self.error(e);
        }
    }

    /// Delete in Open Layout or Filters: Yes deletes the layout (or takes
    /// the filter out of the dialog's list, kept on OK), No and Escape keep
    /// it. Defaults from `kit_window::Question`.
    fn remove_prompt_window(&mut self, ctx: &egui::Context) {
        let Some(removal) = self.grid.dialogs.remove_prompt.clone() else {
            return;
        };
        let (id, text, question) = match &removal {
            GridRemoval::Layout(name) => (
                "remove_layout",
                format!("Delete the layout \"{name}\"?"),
                kit::Question::DeleteLayout,
            ),
            GridRemoval::Filter(key) => {
                let name = self
                    .grid
                    .dialogs
                    .filters
                    .as_ref()
                    .and_then(|d| d.users.iter().find(|u| &u.key() == key))
                    .map(|u| u.name.clone())
                    .unwrap_or_default();
                (
                    "remove_filter",
                    format!("Delete the filter \"{name}\"?"),
                    kit::Question::DeleteFilter,
                )
            }
        };
        let buttons = ["Yes", "No"];
        let mut rects = vec![];
        let mut answer = kit::message_box(
            ctx,
            id,
            "Confirmation",
            kit::MessageIcon::Question,
            &text,
            &buttons,
            question.default_in(&buttons),
            &mut rects,
        )
        .map(|k| k == 0);
        self.record_dialog_actions(rects);
        if self.escape(ctx, "remove_prompt") {
            answer = Some(false);
        }
        let Some(yes) = answer else {
            return;
        };
        self.grid.dialogs.remove_prompt = None;
        if !yes {
            return;
        }
        match removal {
            GridRemoval::Layout(name) => {
                let result = match self.grid.dialogs.layout_target {
                    LayoutTarget::Assignments => self.delete_assignment_layout(&name),
                    LayoutTarget::Activities => self.delete_layout(&name),
                };
                if let Err(e) = result {
                    self.error(e);
                }
                if let Some(d) = self.grid.dialogs.open_layout.as_mut() {
                    d.selected = None;
                }
            }
            GridRemoval::Filter(key) => {
                if let Some(d) = self.grid.dialogs.filters.as_mut() {
                    d.users.retain(|u| u.key() != key);
                    d.selection.selected.retain(|k| *k != key);
                    d.current = None;
                }
            }
        }
    }

    /// Open `name` among the layouts the windows work on now.
    fn open_layout_of_target(&mut self, name: &str, use_filter: bool) -> Result<(), String> {
        match self.grid.dialogs.layout_target {
            LayoutTarget::Activities => self.open_layout(name, use_filter),
            LayoutTarget::Assignments => self.open_assignment_layout(name),
        }
    }

    fn save_as_window(&mut self, ctx: &egui::Context) {
        let Some(mut name) = self.grid.dialogs.save_as.take() else {
            return;
        };
        let mut done = None;
        let mut help = false;
        use crate::dialog_kit as kit;
        // The name over a bottom row Help, Save, Cancel, as the reference
        // lays it out. Layouts are user-level, so there is no Available to
        // choice.
        let (w, h) = (360., 100.);
        let ((), closed, actions) = kit::window(
            ctx,
            "grid_save_as",
            "Save Layout As",
            Vec2::new(w, h),
            |c| {
                c.fill(0., 0., w, h, CHROME);
                kit::text(c, 12., 14., "Layout Name", true);
                c.edit(
                    "save_as_name",
                    12.,
                    32.,
                    w - 24.,
                    21.,
                    &mut name,
                    true,
                    Align::Min,
                );
                let out = kit::button_row(
                    c,
                    w - 12.,
                    h - 35.,
                    &[
                        Entry::Button("save_as_ok", "&Save", true, false),
                        Entry::cancel("save_as_cancel"),
                        Entry::help("save_as_help"),
                    ],
                );
                kit::mark_default(c, "save_as_ok");
                match out.clicked.as_deref() {
                    Some("save_as_ok") => done = Some(true),
                    Some("save_as_cancel") => done = Some(false),
                    Some(_) => help = true,
                    None => {}
                }
            },
        );
        if help {
            self.open_help_for("grid_save_as");
        }
        self.record_dialog_actions(actions);
        if closed {
            done = Some(false);
        }
        match done {
            Some(true) => {
                let result = match self.grid.dialogs.layout_target {
                    LayoutTarget::Activities => self.save_layout_as(&name),
                    LayoutTarget::Assignments => self.save_assignment_layout_as(&name),
                };
                if let Err(e) = result {
                    self.error(e);
                    self.grid.dialogs.save_as = Some(name);
                }
            }
            Some(false) => {}
            None if self.escape(ctx, "save_as") => {}
            None => self.grid.dialogs.save_as = Some(name),
        }
    }

    fn find_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.grid.dialogs.find.take() else {
            return;
        };
        // Look in lists the shown table's columns: the activity table's, or
        // a view table's that has the cell commands.
        let shown: Vec<(String, String)> = if self.views.main.is_activities() {
            let catalog = columns::catalog(&self.project);
            self.visible_columns(&catalog)
                .into_iter()
                .map(|c| (c.id.clone(), c.title.clone()))
                .collect()
        } else {
            self.view_find_columns()
        };
        let unknown = f
            .column
            .as_ref()
            .is_some_and(|id| !shown.iter().any(|(c, _)| c == id));
        if !self.views.main.is_activities() && unknown {
            f.column = None;
        }
        let mut close = false;
        let mut command = None;
        let replace = f.replace;
        let mut keys = vec!["find_next"];
        if replace {
            keys.extend(["replace_one", "replace_all"]);
        }
        keys.push("find_close");
        // The buttons follow the text as the form leaves it this frame.
        let buttons = |any: &bool| {
            let any = *any;
            let mut buttons = vec![DialogButton::new("Find Next", None).enabled(any)];
            if replace {
                buttons.extend([
                    DialogButton::new("Replace", None).enabled(any),
                    DialogButton::new("Replace All", None).enabled(any),
                ]);
            }
            buttons.push(DialogButton::new("Close", None).grouped());
            buttons
        };
        chrome::dialog_window(if f.replace { "Replace" } else { "Find" })
            .id(egui::Id::new("grid_find"))
            .resizable(false)
            .show(ctx, |ui| {
                let body = |ui: &mut egui::Ui| {
                    ui.set_min_size(Vec2::new(314., 134.));
                    self.find_form(ui, &mut f, &shown, &mut command);
                    !f.what.is_empty()
                };
                let (_, button, responses) = chrome::dialog_with_buttons_from(ui, body, buttons);
                for (key, r) in keys.iter().zip(&responses) {
                    self.action(key, r);
                }
                match button.map(|k| keys[k]) {
                    Some("find_next") => command = Some("next"),
                    Some("replace_one") => command = Some("replace"),
                    Some("replace_all") => command = Some("all"),
                    Some(_) => close = true,
                    None => {}
                }
            });
        self.find_finish(ctx, f, command, close);
    }

    /// Find what, Replace with, Look in and Match case.
    fn find_form(
        &mut self,
        ui: &mut egui::Ui,
        f: &mut FindState,
        shown: &[(String, String)],
        command: &mut Option<&'static str>,
    ) {
        egui::Grid::new("find_form")
            .num_columns(2)
            .spacing([8., 6.])
            .show(ui, |ui| {
                ui.label("Find what:");
                let r = ui.add(
                    egui::TextEdit::singleline(&mut f.what)
                        .id(egui::Id::new("find_what"))
                        .desired_width(220.),
                );
                self.action("find_what", &r);
                if std::mem::take(&mut self.grid.dialogs.find_focus) {
                    r.request_focus();
                }
                // With text in it, the edit answers Enter itself (one find
                // per press). The buttons were drawn with the text of the
                // pass before, so a search typed in the frame before Enter
                // would otherwise leave Find Next disabled for the dialog
                // keys and Enter would fall to Close.
                if !f.what.is_empty() {
                    crate::dialog_keys::note_keeps_enter(&r);
                }
                if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    *command = Some("next");
                }
                ui.end_row();
                if f.replace {
                    ui.label("Replace with:");
                    let r = ui.add(egui::TextEdit::singleline(&mut f.with).desired_width(220.));
                    self.action("replace_with", &r);
                    ui.end_row();
                }
                ui.label("Look in:");
                egui::ComboBox::from_id_salt("find_column")
                    .selected_text(
                        f.column
                            .as_ref()
                            .and_then(|c| shown.iter().find(|(id, _)| id == c))
                            .map_or("All displayed columns".to_string(), |(_, t)| t.clone()),
                    )
                    .width(220.)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut f.column, None, "All displayed columns");
                        for (id, title) in shown {
                            ui.selectable_value(&mut f.column, Some(id.clone()), title);
                        }
                    });
                ui.end_row();
                ui.label("");
                let r = ui.checkbox(&mut f.match_case, "Match case");
                self.action("find_match_case", &r);
                ui.end_row();
            });
    }

    /// Runs the button pressed in Find and keeps the search for Find Next.
    fn find_finish(
        &mut self,
        ctx: &egui::Context,
        f: FindState,
        command: Option<&'static str>,
        close: bool,
    ) {
        let time = ctx.input(|i| i.time);
        // A view table other than the activity table has its own.
        let view = !self.views.main.is_activities();
        match command {
            Some("next") if view => {
                self.view_find_next(&f);
            }
            Some("replace") if view => self.view_replace_current(&f, time),
            Some("all") if view => {
                self.view_replace_all(&f, time);
            }
            Some("next") => {
                self.find_next(&f);
            }
            Some("replace") => {
                self.replace_current(&f);
            }
            Some("all") => {
                self.replace_all(&f);
            }
            _ => {}
        }
        if !f.what.is_empty() {
            self.grid.dialogs.last_find = Some(f.clone());
        }
        if !(close || self.escape(ctx, "find")) {
            self.grid.dialogs.find = Some(f);
        }
    }

    /// Columns a find searches: the chosen one or every displayed column.
    fn find_columns(&self, f: &FindState) -> Vec<Column> {
        let catalog = columns::catalog(&self.project);
        match &f.column {
            Some(id) => columns::find(&catalog, id).cloned().into_iter().collect(),
            None => self.visible_columns(&catalog),
        }
    }

    fn contains(f: &FindState, text: &str) -> bool {
        find_span(text, &f.what, f.match_case).is_some()
    }

    /// Whether Find Next has a search to go on with: the open Find dialog's
    /// text or the last search made.
    pub(crate) fn find_ready(&self) -> bool {
        let dialogs = &self.grid.dialogs;
        dialogs
            .find
            .as_ref()
            .or(dialogs.last_find.as_ref())
            .is_some_and(|f| !f.what.is_empty())
    }

    /// Find Next (F3, the menu item and the Find toolbar button): the open
    /// dialog's search, else the last one. Without one nothing changes.
    pub(crate) fn run_find_next(&mut self) -> bool {
        let search = match &self.grid.dialogs.find {
            Some(f) if !f.what.is_empty() => Some(f.clone()),
            _ => self.grid.dialogs.last_find.clone(),
        };
        match search {
            Some(f) if !f.what.is_empty() && !self.views.main.is_activities() => {
                self.view_find_next(&f)
            }
            Some(f) if !f.what.is_empty() => self.find_next(&f),
            _ => false,
        }
    }

    /// Select the next visible activity with a matching cell, wrapping at the end.
    pub(crate) fn find_next(&mut self, f: &FindState) -> bool {
        if f.what.is_empty() {
            return false;
        }
        let cols = self.find_columns(f);
        let ix = self.grid_index();
        let order: Vec<usize> = self
            .rows()
            .into_iter()
            .filter_map(|r| match r {
                Row::Activity(i) => Some(i),
                Row::Group(_) => None,
            })
            .collect();
        let start = self
            .selected
            .and_then(|s| order.iter().position(|&i| i == s))
            .map_or(0, |p| p + 1);
        for step in 0..order.len() {
            let i = order[(start + step) % order.len()];
            for c in &cols {
                let text = columns::display(
                    &self.project,
                    Some(i),
                    &columns::value(&self.project, &ix, c, i),
                );
                if Self::contains(f, &text) {
                    self.selected = Some(i);
                    self.grid.focus_column = Some(c.id.clone());
                    self.show_focus_column();
                    self.scroll_to_selected = true;
                    self.date_edits.clear();
                    self.status = format!("Found {} in {}", f.what, c.title);
                    return true;
                }
            }
        }
        self.status = format!("{} not found.", f.what);
        false
    }

    fn replaced(f: &FindState, text: &str) -> String {
        replace_matches(text, &f.what, &f.with, f.match_case)
    }

    /// Replace in the focused cell of the selected activity when it matches,
    /// then move to the next match.
    pub(crate) fn replace_current(&mut self, f: &FindState) {
        if let (Some(i), Some(id)) = (self.selected, self.grid.focus_column.clone()) {
            let catalog = columns::catalog(&self.project);
            if let Some(c) = columns::find(&catalog, &id).filter(|c| c.editor == Editor::Text) {
                let ix = self.grid_index();
                let text = columns::display(
                    &self.project,
                    Some(i),
                    &columns::value(&self.project, &ix, c, i),
                );
                if Self::contains(f, &text) {
                    self.apply_cell(i, &id, &Self::replaced(f, &text));
                }
            }
        }
        self.find_next(f);
    }

    /// Replace every match in editable text columns of the visible
    /// activities. Returns (replaced, rejected).
    pub(crate) fn replace_all(&mut self, f: &FindState) -> (usize, usize) {
        if f.what.is_empty() {
            return (0, 0);
        }
        let cols: Vec<Column> = self
            .find_columns(f)
            .into_iter()
            .filter(|c| c.editor == Editor::Text)
            .collect();
        let order: Vec<usize> = self
            .rows()
            .into_iter()
            .filter_map(|r| match r {
                Row::Activity(i) => Some(i),
                Row::Group(_) => None,
            })
            .collect();
        let (mut done, mut failed) = (0, 0);
        for i in order {
            for c in &cols {
                let ix = self.grid_index();
                let text = columns::display(
                    &self.project,
                    Some(i),
                    &columns::value(&self.project, &ix, c, i),
                );
                if Self::contains(f, &text) {
                    if self.apply_cell(i, &c.id, &Self::replaced(f, &text)) {
                        done += 1;
                    } else {
                        failed += 1;
                    }
                }
            }
        }
        self.status = if failed == 0 {
            format!("Replaced {done}")
        } else {
            format!("Replaced {done}; {failed} rejected")
        };
        (done, failed)
    }
}

/// Do two characters match? Without case, each character's lowercase form is
/// compared as a whole, so characters whose lowercase has another length
/// still compare one character to one character.
fn same_char(a: char, b: char, match_case: bool) -> bool {
    a == b || !match_case && a.to_lowercase().eq(b.to_lowercase())
}

/// Byte range of the first match of `needle` in `text`, found by walking
/// `text` itself character by character. Offsets always fall on `text`'s
/// own character boundaries.
pub(crate) fn find_span(text: &str, needle: &str, match_case: bool) -> Option<(usize, usize)> {
    find_span_from(text, needle, match_case, 0)
}

fn find_span_from(
    text: &str,
    needle: &str,
    match_case: bool,
    from: usize,
) -> Option<(usize, usize)> {
    if needle.is_empty() {
        return None;
    }
    for (start, _) in text[from..].char_indices().map(|(i, c)| (i + from, c)) {
        let mut hay = text[start..].char_indices();
        let mut end = start;
        let mut matched = true;
        for n in needle.chars() {
            match hay.next() {
                Some((i, c)) if same_char(c, n, match_case) => end = start + i + c.len_utf8(),
                _ => {
                    matched = false;
                    break;
                }
            }
        }
        if matched {
            return Some((start, end));
        }
    }
    None
}

/// Replace every match of `needle` (see `find_span`) with `with`.
pub(crate) fn replace_matches(text: &str, needle: &str, with: &str, match_case: bool) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while let Some((start, end)) = find_span_from(text, needle, match_case, at) {
        out.push_str(&text[at..start]);
        out.push_str(with);
        at = end;
    }
    out.push_str(&text[at..]);
    out
}

fn add_column(d: &mut ColumnsDraft, catalog: &[Column], id: &str) {
    if d.columns.iter().any(|c| c.id == id) {
        return;
    }
    let Some(c) = columns::find(catalog, id) else {
        return;
    };
    let at = d
        .selected
        .map_or(d.columns.len(), |s| (s + 1).min(d.columns.len()));
    d.columns.insert(at, ColumnSetting::new(id, c.width));
    d.selected = Some(at);
}
