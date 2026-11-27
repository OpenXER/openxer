//! Activity Details: the bottom pane with its reorderable tab set, shared grid and
//! form helpers, the tab customization dialog and the layout preference.
use super::*;
use crate::context_menu::{ContextMenu, Menu};
use crate::records::{self, clean, float, format_number, num};

pub(crate) mod assign_kit;
#[cfg(test)]
mod assignment_tests;
mod assignment_window;
pub(crate) mod assignments;
mod bulk_assign;
#[cfg(test)]
mod codes_assign_tests;
pub(crate) mod form;
pub(crate) mod grid_columns;
#[cfg(test)]
mod headless_tests;
pub(crate) mod notes_pane;
pub(crate) mod owner_pickers;
pub(crate) mod record_tabs;
pub(crate) mod removals;
#[cfg(test)]
mod review_renders;
pub(crate) mod schedule_tabs;
pub(crate) mod setters;
#[cfg(test)]
mod split_tests;
#[cfg(test)]
mod status_layout_tests;
mod units_progress;

/// Preference key for the visible tab order.
pub const LAYOUT_KEY: &str = "activity_details_tabs_v1";
/// Preference: where the user moved each Details tab's panel splitter, as
/// the left panel's share of the page width, by tab key.
pub const SPLITS_KEY: &str = "activity_details_splits_v1";
pub const ROW_H: f32 = 18.;
pub const HEADER_H: f32 = 18.;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum DetailTab {
    General,
    Status,
    Predecessors,
    Successors,
    Relationships,
    Resources,
    Steps,
    Notebook,
    Codes,
    Expenses,
    Documents,
    Summary,
    Feedback,
    Risks,
    UserDefinedFields,
}

impl DetailTab {
    /// Every tab in the reference's order: the default display set
    /// (General, Status, Resources, Predecessors, Successors), then the rest
    /// alphabetically by the reference's own captions (Notes to Resources
    /// sits where Feedback does, Documents where WPs & Docs does).
    pub const REFERENCE: [DetailTab; 15] = [
        DetailTab::General,
        DetailTab::Status,
        DetailTab::Resources,
        DetailTab::Predecessors,
        DetailTab::Successors,
        DetailTab::Codes,
        DetailTab::Expenses,
        DetailTab::Feedback,
        DetailTab::Notebook,
        DetailTab::Relationships,
        DetailTab::Risks,
        DetailTab::Steps,
        DetailTab::Summary,
        DetailTab::UserDefinedFields,
        DetailTab::Documents,
    ];
    /// Every tab in the order of earlier builds (the legacy defaults of
    /// tests, `crate::defaults`): their twelve-tab display set first.
    pub const ALL: [DetailTab; 15] = [
        DetailTab::General,
        DetailTab::Status,
        DetailTab::Resources,
        DetailTab::Codes,
        DetailTab::Relationships,
        DetailTab::Notebook,
        DetailTab::Steps,
        DetailTab::Feedback,
        DetailTab::Documents,
        DetailTab::Risks,
        DetailTab::Expenses,
        DetailTab::Summary,
        DetailTab::Predecessors,
        DetailTab::Successors,
        DetailTab::UserDefinedFields,
    ];
    pub fn caption(self) -> &'static str {
        match self {
            DetailTab::General => "General",
            DetailTab::Status => "Status",
            DetailTab::Predecessors => "Predecessors",
            DetailTab::Successors => "Successors",
            DetailTab::Relationships => "Relationships",
            DetailTab::Resources => "Resources",
            DetailTab::Steps => "Steps",
            DetailTab::Notebook => "Notebook",
            DetailTab::Codes => "Codes",
            DetailTab::Expenses => "Expenses",
            DetailTab::Documents => "Documents",
            DetailTab::Summary => "Summary",
            DetailTab::Feedback => "Notes to Resources",
            DetailTab::Risks => "Risks",
            DetailTab::UserDefinedFields => "User Defined Fields",
        }
    }
    /// Stable identifier used in the saved layout.
    pub fn key(self) -> &'static str {
        match self {
            DetailTab::General => "general",
            DetailTab::Status => "status",
            DetailTab::Predecessors => "predecessors",
            DetailTab::Successors => "successors",
            DetailTab::Relationships => "relationships",
            DetailTab::Resources => "resources",
            DetailTab::Steps => "steps",
            DetailTab::Notebook => "notebook",
            DetailTab::Codes => "codes",
            DetailTab::Expenses => "expenses",
            DetailTab::Documents => "documents",
            DetailTab::Summary => "summary",
            DetailTab::Feedback => "feedback",
            DetailTab::Risks => "risks",
            DetailTab::UserDefinedFields => "udf",
        }
    }
    fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.key() == key)
    }
    /// Every tab in the order Customize lists them and a shown tab is put
    /// in: the reference order (`REFERENCE`), or `ALL` under the legacy
    /// defaults of tests.
    pub fn ordered() -> [DetailTab; 15] {
        if crate::defaults::reference() {
            Self::REFERENCE
        } else {
            Self::ALL
        }
    }
    /// The default display set: the reference's General, Status, Resources,
    /// Predecessors and Successors (the legacy defaults of tests: the first
    /// twelve of `ALL`). The others stay available in Customize Activity
    /// Details.
    pub fn default_tabs() -> Vec<DetailTab> {
        if crate::defaults::reference() {
            Self::REFERENCE[..5].to_vec()
        } else {
            Self::ALL[..12].to_vec()
        }
    }
    fn order(self) -> usize {
        Self::ordered().iter().position(|t| *t == self).unwrap_or(0)
    }
}

/// Parse a saved tab layout. Unknown keys, duplicates, empty lists or bad JSON are rejected.
pub fn parse_layout(json: &str) -> Option<Vec<DetailTab>> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let mut tabs = vec![];
    for item in value.as_array()? {
        // Trace Logic was a tab before it became a bottom view; Discussion,
        // Tasks and Contract Management Docs were placeholder tabs (XER
        // carries none of them) removed in the 2026-11-07 cleanup.
        if matches!(
            item.as_str(),
            Some("trace_logic" | "discussion" | "tasks" | "contract_docs")
        ) {
            continue;
        }
        let tab = DetailTab::from_key(item.as_str()?)?;
        if tabs.contains(&tab) {
            return None;
        }
        tabs.push(tab);
    }
    (!tabs.is_empty()).then_some(tabs)
}
pub fn layout_json(tabs: &[DetailTab]) -> String {
    serde_json::Value::from(tabs.iter().map(|t| t.key()).collect::<Vec<_>>()).to_string()
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum UnitsMode {
    LaborUnits,
    NonlaborUnits,
    LaborCost,
    NonlaborCost,
    MaterialCost,
}
impl UnitsMode {
    pub const ALL: [UnitsMode; 5] = [
        UnitsMode::LaborUnits,
        UnitsMode::NonlaborUnits,
        UnitsMode::LaborCost,
        UnitsMode::NonlaborCost,
        UnitsMode::MaterialCost,
    ];
    pub fn caption(self) -> &'static str {
        match self {
            UnitsMode::LaborUnits => "Labor Units",
            UnitsMode::NonlaborUnits => "Nonlabor Units",
            UnitsMode::LaborCost => "Labor Cost",
            UnitsMode::NonlaborCost => "Nonlabor Cost",
            UnitsMode::MaterialCost => "Material Cost",
        }
    }
    /// The table measure this section shows.
    pub fn measure(self) -> crate::grid::columns::Measure {
        use crate::grid::columns::Measure;
        match self {
            UnitsMode::LaborUnits => Measure::LaborUnits,
            UnitsMode::NonlaborUnits => Measure::NonlaborUnits,
            UnitsMode::LaborCost => Measure::LaborCost,
            UnitsMode::NonlaborCost => Measure::NonlaborCost,
            UnitsMode::MaterialCost => Measure::MaterialCost,
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SummaryMode {
    Units,
    Cost,
    Dates,
}

#[derive(Clone)]
pub enum Picker {
    Relationship {
        predecessor: bool,
        query: String,
        selected: Option<usize>,
        kind: LinkType,
        lag: f64,
    },
    Code {
        type_id: Option<i64>,
        selected: Option<i64>,
    },
    Topic {
        selected: Option<i64>,
    },
    Document {
        selected: Option<i64>,
    },
    Risk {
        selected: Option<i64>,
    },
}

pub struct Customize {
    display: Vec<DetailTab>,
    available_pick: Option<DetailTab>,
    display_pick: Option<DetailTab>,
}

/// Step fields copied with the Steps context menu.
pub use openxer_xer::edit::steps::StepCopy;

/// Filter applied inside Assign Predecessors / Successors.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PickerFilter {
    #[default]
    All,
    CurrentProject,
    NotStarted,
    InProgress,
    Completed,
    Critical,
}

/// Grouping mode inside Assign Predecessors / Successors.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PickerGroup {
    /// Follow the activity table's WBS/band layout (default).
    #[default]
    Wbs,
    /// Flat activity list with no grouping bands.
    List,
    /// Grouped by Activity Status.
    Status,
}

/// Optional extra columns toggled from the Assign Predecessors / Successors
/// right-click Columns submenu.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PickerColumn {
    Status,
    OriginalDuration,
    Start,
    Finish,
    TotalFloat,
}

impl PickerColumn {
    pub const ALL: [Self; 5] = [
        Self::Status,
        Self::OriginalDuration,
        Self::Start,
        Self::Finish,
        Self::TotalFloat,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Self::Status => "col_status",
            Self::OriginalDuration => "col_orig_dur",
            Self::Start => "col_start",
            Self::Finish => "col_finish",
            Self::TotalFloat => "col_total_float",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Status => "Activity Status",
            Self::OriginalDuration => "Original Duration",
            Self::Start => "Start",
            Self::Finish => "Finish",
            Self::TotalFloat => "Total Float",
        }
    }

    pub fn width(self) -> f32 {
        match self {
            Self::Status => 85.,
            Self::OriginalDuration => 78.,
            Self::Start => 84.,
            Self::Finish => 84.,
            Self::TotalFloat => 72.,
        }
    }
}

/// What the relationship picker's lines depend on: the search text, the
/// project's undo snapshot, the table's Group and Sort, the picker's own
/// filter/grouping, and the activities that take the relationship.
#[derive(Clone, Debug, PartialEq)]
pub struct PickerLinesKey {
    pub query: String,
    pub serial: u64,
    pub group_sort: crate::grid::layout::GroupSort,
    pub filter: PickerFilter,
    pub group: PickerGroup,
    pub targets: Vec<usize>,
    pub collapsed: BTreeSet<String>,
}

pub struct DetailState {
    pub tab: DetailTab,
    pub tabs: Vec<DetailTab>,
    pub customize: Option<Customize>,
    /// Selected grid rows; the Relationships tab uses both slots.
    pub rows: [Option<usize>; 2],
    context: Option<(i64, DetailTab)>,
    pub picker: Option<Picker>,
    /// Assign Predecessors / Successors: the lines of the last build, all
    /// band keys (for Collapse All), and what they were built for.
    pub picker_lines: Option<(
        PickerLinesKey,
        std::rc::Rc<Vec<crate::grid::PickLine>>,
        Vec<String>,
    )>,
    /// Bands folded shut in Assign Predecessors / Successors (band keys).
    pub picker_collapsed: BTreeSet<String>,
    /// Filter By choice in Assign Predecessors / Successors.
    pub picker_filter: PickerFilter,
    /// Group and Sort By choice in Assign Predecessors / Successors.
    pub picker_group: PickerGroup,
    /// Optional extra columns shown in Assign Predecessors / Successors.
    pub picker_columns: BTreeSet<PickerColumn>,
    /// The Activity Codes dictionary was opened from Assign: when it closes
    /// with code values to assign, the code picker opens.
    pub assign_codes_after: bool,
    /// Resources tab picker (Add Resource, Add Role, Assign by Role).
    pub assign: Option<assignments::AssignPicker>,
    /// The activity the open picker assigns to when the Resource
    /// Assignments window opened it (Add), not the selected activity.
    pub assign_for: Option<usize>,
    /// Assign Resources, New: the Resource ID and Resource Name being typed.
    pub new_resource: Option<(String, String)>,
    /// Assign Resources, Replace: the TASKRSRC row whose resource the next
    /// Assign replaces.
    pub replace_from: Option<usize>,
    /// The choices selected in each Assign dialog's list (Ctrl and Shift
    /// select several), by dialog.
    pub assign_rows: BTreeMap<&'static str, crate::views::grid::RowSelection<i64>>,
    /// Assign Activity Codes, New: the Add Code Value dialog while open.
    pub new_code: Option<assign_kit::NewCode>,
    /// Project > Resource Assignments window.
    pub assignment_window: Option<assignment_window::AssignmentWindow>,
    /// User Preferences, Calculations: assignment options.
    pub calc: assignments::AssignmentCalc,
    /// A resource replacement waiting for the user's staffing choice.
    pub question: Option<assignments::Question>,
    /// A relationship, assignment or code removal waiting for its answer.
    pub removal: Option<removals::Removal>,
    /// Memo edit buffers keyed by record: (html the buffer was built from, text).
    pub memo: BTreeMap<String, (String, String)>,
    /// The notes panes: formatted memos, the one open for editing, and the
    /// WBS and project pickers (notebook topics, project codes).
    pub notes: notes_pane::NotesState,
    /// Date edit buffers keyed by field: (value the buffer shows, text).
    pub dates: BTreeMap<String, (Option<i64>, String)>,
    pub step_clipboard: Option<StepCopy>,
    pub units_mode: UnitsMode,
    pub summary_mode: SummaryMode,
    pub document_details: Option<i64>,
    /// Resources tab shows every assignment column, not just the defaults.
    pub all_resource_columns: bool,
    /// Relationship grids add Start, Finish, Driving and Critical.
    pub all_relationship_columns: bool,
    /// Grids (by name) that show every column instead of the defaults.
    pub all_columns: std::collections::BTreeSet<String>,
    /// Columns chosen through Customize ... Columns, by grid (titles in order).
    pub grid_columns: BTreeMap<String, Vec<String>>,
    /// The Customize ... Columns dialog while open.
    pub column_dialog: Option<grid_columns::ColumnDialog>,
    /// Moved panel splitters: the left panel's share of the width, by tab.
    pub splits: BTreeMap<String, f32>,
    strip_first: usize,
}
impl Default for DetailState {
    fn default() -> Self {
        Self {
            tab: DetailTab::General,
            tabs: DetailTab::default_tabs(),
            customize: None,
            rows: [None, None],
            context: None,
            picker: None,
            picker_lines: None,
            picker_collapsed: BTreeSet::new(),
            picker_filter: PickerFilter::All,
            picker_group: PickerGroup::Wbs,
            picker_columns: BTreeSet::new(),
            assign_codes_after: false,
            assign: None,
            assign_for: None,
            new_resource: None,
            replace_from: None,
            assign_rows: BTreeMap::new(),
            new_code: None,
            assignment_window: None,
            calc: Default::default(),
            question: None,
            removal: None,
            memo: BTreeMap::new(),
            notes: Default::default(),
            dates: BTreeMap::new(),
            step_clipboard: None,
            units_mode: UnitsMode::LaborUnits,
            summary_mode: SummaryMode::Units,
            document_details: None,
            all_resource_columns: false,
            all_relationship_columns: false,
            all_columns: Default::default(),
            grid_columns: BTreeMap::new(),
            column_dialog: None,
            splits: BTreeMap::new(),
            strip_first: 0,
        }
    }
}

pub enum Ev {
    Cell(usize, usize),
    Menu(Option<usize>),
}
#[derive(Default)]
pub struct GridOut {
    pub clicked: Option<usize>,
    pub double: Option<usize>,
    /// The Ctrl, Shift and Alt keys held for the click.
    pub modifiers: egui::Modifiers,
}

/// Grid body line color.
pub const LINE: Color32 = Color32::from_rgb(192, 192, 192);
/// Focus cell outline in the selected row.
pub const FOCUS: Color32 = crate::theme::color(crate::theme::AMBER);

/// Captions of numeric columns, whose caption and cells are right aligned.
pub fn numeric_caption(title: &str) -> bool {
    let t = title.to_ascii_lowercase();
    [
        "units",
        "cost",
        "price",
        "/ unit",
        "lag",
        "% complete",
        "units / time",
        "budgeted",
        "actual",
        "remaining",
        "at completion",
        "variance",
        "weight",
    ]
    .iter()
    .any(|w| t.ends_with(w))
}

/// The detail grids' look, as measured: a 1 px lowered frame, an 18 px
/// header (white top and left highlight, `#F0F0F0`, black dividers and
/// bottom line, captions 3 px in), 18 px rows (17 px cell plus a `#C0C0C0`
/// line under and right of each cell), full-row selection with the first
/// cell outlined in the theme amber. Columns keep their widths; space right of
/// the last column stays empty.
pub fn grid(
    ui: &mut egui::Ui,
    id: &str,
    cols: &[(&str, f32)],
    rows: usize,
    selected: Option<usize>,
    size: Vec2,
    f: &mut dyn FnMut(&mut egui::Ui, Ev, bool),
) -> GridOut {
    grid_marked(ui, id, cols, rows, selected, &BTreeSet::new(), size, f)
}

/// [`grid`] with more rows painted as selected (`marked`) besides the
/// current row `selected`, which alone carries the focus outline.
#[allow(clippy::too_many_arguments)]
pub fn grid_marked(
    ui: &mut egui::Ui,
    id: &str,
    cols: &[(&str, f32)],
    rows: usize,
    selected: Option<usize>,
    marked: &BTreeSet<usize>,
    size: Vec2,
    f: &mut dyn FnMut(&mut egui::Ui, Ev, bool),
) -> GridOut {
    let mut out = GridOut::default();
    let size = Vec2::new(size.x.max(80.), size.y.max(HEADER_H + ROW_H * 2. + 2.));
    let rect = Rect::from_min_size(ui.cursor().min, size);
    ui.allocate_rect(rect, Sense::hover());
    let frame = rect;
    let inner = Rect::from_min_max(rect.min + Vec2::splat(1.), rect.max - Vec2::splat(1.));
    {
        let p = ui.painter();
        form::fill(p, rect, Color32::WHITE);
        form::lowered(p, frame);
        // The header band runs across the whole width.
        let band = Rect::from_min_size(inner.min, Vec2::new(inner.width(), HEADER_H));
        form::fill(p, band, CHROME);
        chrome::hline(p, band.left(), band.right(), band.top(), Color32::WHITE);
        chrome::hline(p, band.left(), band.right(), band.bottom() - 1., INK);
    }
    // A new window's first pass only measures it. The table would remember
    // the collapsed column widths of that invisible pass, so it waits for the
    // real one; the grid's size is fixed, so nothing moves.
    if ui.is_sizing_pass() {
        return out;
    }
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(Layout::top_down(Align::LEFT)),
    );
    child.set_clip_rect(inner.intersect(ui.clip_rect()));
    let ui = &mut child;
    // No column resize lines down the empty body.
    ui.visuals_mut().widgets.noninteractive.bg_stroke = Stroke::NONE;
    let background = ui.interact(inner, ui.id().with((id, "bg")), Sense::click());
    ContextMenu::new("detail_grid_menu").show(&background, &mut (), |_, m| {
        f(m.ui(), Ev::Menu(None), false)
    });
    ui.spacing_mut().item_spacing = Vec2::ZERO;
    let mut builder = egui_extras::TableBuilder::new(ui)
        .id_salt(id)
        .striped(false)
        .sense(Sense::click())
        .resizable(true)
        .vscroll(true)
        .auto_shrink([false, false])
        .min_scrolled_height(0.)
        .max_scroll_height(inner.height() - HEADER_H)
        .cell_layout(Layout::left_to_right(Align::Center));
    for (_, width) in cols {
        builder = builder.column(
            egui_extras::Column::initial(*width)
                .at_least(18.)
                .clip(true),
        );
    }
    builder = builder.column(egui_extras::Column::remainder().resizable(false));
    builder
        .header(HEADER_H, |mut header| {
            for (title, _) in cols {
                header.col(|ui| {
                    let r = ui.max_rect();
                    let p = ui.painter();
                    form::fill(p, r, CHROME);
                    chrome::hline(p, r.left(), r.right(), r.top(), Color32::WHITE);
                    chrome::vline(p, r.left(), r.top(), r.bottom(), Color32::WHITE);
                    chrome::vline(p, r.right() - 1., r.top(), r.bottom(), INK);
                    chrome::hline(p, r.left(), r.right(), r.bottom() - 1., INK);
                    let clip = Rect::from_min_max(r.min, Pos2::new(r.right() - 1., r.bottom()));
                    let p = p.with_clip_rect(clip.intersect(p.clip_rect()));
                    let top = r.top() + 3.;
                    if numeric_caption(title) {
                        p.text(
                            Pos2::new(r.right() - 4., top),
                            Align2::RIGHT_TOP,
                            *title,
                            form::font(),
                            INK,
                        );
                    } else {
                        p.text(
                            Pos2::new(r.left() + 3., top),
                            Align2::LEFT_TOP,
                            *title,
                            form::font(),
                            INK,
                        );
                    }
                });
            }
            header.col(|ui| {
                let r = ui.max_rect();
                chrome::vline(
                    ui.painter(),
                    r.left(),
                    r.top(),
                    r.bottom() - 1.,
                    Color32::WHITE,
                );
            });
        })
        .body(|body| {
            body.rows(ROW_H, rows, |mut row| {
                let i = row.index();
                let current = selected == Some(i);
                let sel = current || marked.contains(&i);
                row.set_selected(sel);
                for c in 0..cols.len() {
                    row.col(|ui| {
                        let r = ui.max_rect();
                        let p = ui.painter();
                        chrome::vline(p, r.right() - 1., r.top(), r.bottom(), LINE);
                        chrome::hline(p, r.left(), r.right(), r.bottom() - 1., LINE);
                        if current && c == 0 {
                            let f = Rect::from_min_max(
                                r.min,
                                Pos2::new(r.right() - 1., r.bottom() - 1.),
                            );
                            p.rect_stroke(
                                f,
                                0.,
                                Stroke::new(2_f32, FOCUS),
                                egui::StrokeKind::Inside,
                            );
                        }
                        ui.add_space(2.);
                        f(ui, Ev::Cell(i, c), sel);
                    });
                }
                row.col(|_| {});
                let response = row.response();
                if response.clicked() || response.secondary_clicked() {
                    out.clicked = Some(i);
                    out.modifiers = response.ctx.input(|i| i.modifiers);
                }
                if response.double_clicked() {
                    out.double = Some(i);
                }
                ContextMenu::new("detail_grid_menu")
                    .show(&response, &mut (), |_, m| f(m.ui(), Ev::Menu(Some(i)), sel));
            });
        });
    out
}
/// `left` kept between the panels' minimum widths of a page `w` wide; a
/// page too narrow for both splits evenly.
pub fn split_clamp(w: f32, left: f32, min: [f32; 2]) -> f32 {
    if w < min[0] + min[1] {
        return (w * 0.5).round();
    }
    left.clamp(min[0], w - min[1])
}

/// A lowered content box split by a vertical splitter `left` pixels in.
/// Returns the areas left and right of it, inside their 5 px margins.
pub fn split(ui: &egui::Ui, content: Rect, left: f32) -> (Rect, Rect) {
    let p = ui.painter();
    form::lowered(p, content);
    let x = content.left() + left;
    form::vsplitter(p, x, content.top() + 1., content.bottom() - 1.);
    (
        Rect::from_min_max(
            content.min + Vec2::splat(6.),
            Pos2::new(x - 5., content.bottom() - 6.),
        ),
        Rect::from_min_max(
            Pos2::new(x + 11., content.top() + 6.),
            content.max - Vec2::splat(6.),
        ),
    )
}

/// A top-down child confined and clipped to `r`, 5 px item spacing.
pub fn area(ui: &mut egui::Ui, r: Rect) -> egui::Ui {
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(r)
            .layout(Layout::top_down(Align::LEFT)),
    );
    child.set_clip_rect(r.intersect(ui.clip_rect()));
    child.spacing_mut().item_spacing = Vec2::new(form::GAP, form::GAP);
    child
}

/// Marks a displayed column that has no data in XER files.
pub const BLANK: usize = usize::MAX;

/// [`grid`] over a column set given as (caption, width, data column): the
/// callback sees data column numbers, and [`BLANK`] columns stay empty.
#[allow(clippy::too_many_arguments)]
pub fn grid_columns(
    ui: &mut egui::Ui,
    id: &str,
    cols: &[(&str, f32, usize)],
    rows: usize,
    selected: Option<usize>,
    size: Vec2,
    f: &mut dyn FnMut(&mut egui::Ui, Ev, bool),
) -> GridOut {
    let display: Vec<(&str, f32)> = cols.iter().map(|(t, w, _)| (*t, *w)).collect();
    grid(
        ui,
        id,
        &display,
        rows,
        selected,
        size,
        &mut |ui, ev, sel| match ev {
            Ev::Cell(_, c) if cols[c].2 == BLANK => {}
            Ev::Cell(r, c) => f(ui, Ev::Cell(r, cols[c].2), sel),
            other => f(ui, other, sel),
        },
    )
}

/// In-place editor look for a cell of the selected row: white box, black text.
pub fn editor_cell(ui: &mut egui::Ui) {
    let r = ui.max_rect().shrink(1.);
    ui.painter().rect_filled(r, 0., Color32::WHITE);
    ui.visuals_mut().override_text_color = Some(INK);
    let w = &mut ui.visuals_mut().widgets;
    for state in [&mut w.inactive, &mut w.hovered, &mut w.active] {
        state.weak_bg_fill = Color32::WHITE;
        state.bg_fill = Color32::WHITE;
        state.fg_stroke.color = INK;
    }
}
/// Plain cell text, truncated to the column.
pub fn cell(ui: &mut egui::Ui, text: impl Into<egui::WidgetText>) {
    ui.add(egui::Label::new(text).truncate().selectable(false));
}
pub fn money(v: f64) -> String {
    format::cost(v)
}

impl App {
    /// Every edit of a detail table goes through here so undo sees a new revision.
    pub(crate) fn touch_records(&mut self) {
        self.records_serial += 1;
        self.records_revision = self.records_serial;
        self.grid.frame_verified_serial.set(None);
        self.dirty = true;
    }
    pub(crate) fn ids(&self, i: usize) -> (i64, i64) {
        (
            self.project.model.activities[i].task_id,
            self.project.model.project.proj_id,
        )
    }
    pub(crate) fn restore_detail_layout(&mut self) {
        if let Ok(Some(json)) = self.store.preference(assignments::CALC_KEY) {
            if let Ok(calc) = serde_json::from_str(&json) {
                self.detail.calc = calc;
            }
        }
        if let Ok(Some(json)) = self.store.preference(LAYOUT_KEY) {
            if let Some(tabs) = parse_layout(&json) {
                self.detail.tabs = tabs;
            }
        }
        if !self.detail.tabs.contains(&self.detail.tab) {
            self.detail.tab = self.detail.tabs[0];
        }
        if let Ok(Some(json)) = self.store.preference(SPLITS_KEY) {
            if let Ok(splits) = serde_json::from_str::<BTreeMap<String, f32>>(&json) {
                self.detail.splits = splits
                    .into_iter()
                    .filter(|(_, f)| f.is_finite() && (0.0..=1.0).contains(f))
                    .collect();
            }
        }
    }

    /// The left width of a two-panel Details tab whose splitter the user can
    /// drag (resize cursor on hover). `default_left` is used until it is
    /// moved; each panel keeps at least its `min` width; a double-click
    /// splits the page 50/50. The position is kept per tab, as a share of
    /// the width, in the user's preferences.
    pub(crate) fn detail_split(
        &mut self,
        ui: &egui::Ui,
        key: &str,
        content: Rect,
        default_left: f32,
        min: [f32; 2],
    ) -> f32 {
        let w = content.width();
        let clamp = |left: f32| split_clamp(w, left, min);
        let mut left = clamp(
            self.detail
                .splits
                .get(key)
                .map_or(default_left, |share| (share * w).round()),
        );
        let x = content.left() + left;
        let bar = Rect::from_min_max(
            Pos2::new(x - 1., content.top()),
            Pos2::new(x + 6., content.bottom()),
        );
        let response = ui
            .interact(
                bar,
                ui.id().with(("detail_split", key)),
                Sense::click_and_drag(),
            )
            .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
        self.action(&format!("detail_split_{key}"), &response);
        let mut save = false;
        if response.double_clicked() {
            left = clamp((w * 0.5).round());
            save = true;
        } else if response.dragged() {
            if let Some(pos) = response.interact_pointer_pos() {
                left = clamp((pos.x - content.left() - 2.).round());
            }
        }
        if (response.dragged() || save) && w > 0. {
            self.detail.splits.insert(key.to_string(), left / w);
        }
        if save || response.drag_stopped() {
            let json = serde_json::to_string(&self.detail.splits).unwrap_or_default();
            if let Err(error) = self.store.set_preference(SPLITS_KEY, &json) {
                self.error(format!("Cannot save Activity Details layout: {error}"));
            }
        }
        left
    }
    pub(crate) fn save_detail_layout(&mut self) {
        let json = layout_json(&self.detail.tabs);
        if let Err(error) = self.store.set_preference(LAYOUT_KEY, &json) {
            self.error(format!("Cannot save Activity Details layout: {error}"));
        }
    }
    /// Show the pane on `tab`, adding the tab to the visible set when it was hidden.
    pub(crate) fn show_detail_tab(&mut self, tab: DetailTab) {
        self.show_details = true;
        self.panes.bottom = crate::panes::BottomView::Details;
        if !self.detail.tabs.contains(&tab) {
            let at = self
                .detail
                .tabs
                .iter()
                .position(|t| t.order() > tab.order())
                .unwrap_or(self.detail.tabs.len());
            self.detail.tabs.insert(at, tab);
            self.save_detail_layout();
        }
        self.detail.tab = tab;
    }
    pub(crate) fn open_customize_details(&mut self) {
        self.detail.customize = Some(Customize {
            display: self.detail.tabs.clone(),
            available_pick: None,
            display_pick: None,
        });
    }
    /// The tab strip's context menu: two items, as in the reference.
    fn pane_menu(&mut self, ui: &mut egui::Ui) {
        ui.set_min_width(240.);
        let mut menu = Menu::on(ui, "detail_menu");
        if menu.item("hide", chrome::Item::new("Hide Detail Window")) {
            self.show_details = false;
        }
        if menu.item(
            "customize",
            chrome::Item::new("Customize Activity Details..."),
        ) {
            self.open_customize_details();
        }
    }

    pub(crate) fn activity_details(&mut self, host: &mut egui::Ui) {
        egui::TopBottomPanel::bottom(self.activity_pane_id())
            .default_height(260.)
            .min_height(150.)
            .max_height((host.available_rect_before_wrap().height() - 150.).max(150.))
            .resizable(true)
            .show_separator_line(false)
            .frame(egui::Frame::NONE.fill(form::MARGIN))
            .show_inside(host, |ui| {
                let area = ui.max_rect();
                let p = ui.painter();
                // The splitter under the top pane: shadow, white, face, face,
                // shadow, shadow.
                let s = area.top();
                form::fill(p, form::rect(area.left(), s, area.width(), 6.), form::FACE);
                for (dy, color) in [
                    (0., form::SHADOW),
                    (1., form::WHITE),
                    (4., form::SHADOW),
                    (5., form::SHADOW),
                ] {
                    chrome::hline(p, area.left(), area.right(), s + dy, color);
                }
                let strip_top = s + form::SPLITTER_H + form::MARGIN_W;
                let page = Rect::from_min_max(
                    Pos2::new(area.left() + form::MARGIN_W, strip_top + form::TAB_H),
                    Pos2::new(
                        area.right() - form::MARGIN_W,
                        area.bottom() - form::MARGIN_W,
                    ),
                );
                self.tab_strip(ui, page);
                let background = ui.interact(page, ui.id().with("detail_page"), Sense::click());
                egui::Popup::context_menu(&background)
                    .style(chrome::popup_menu_style)
                    .show(|ui| self.pane_menu(ui));
                // Tabs lay out in a child clipped to the page so content that
                // does not fit never grows the panel.
                let mut child = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(page)
                        .layout(Layout::top_down(Align::LEFT)),
                );
                child.set_clip_rect(page.intersect(ui.clip_rect()));
                self.detail_page(&mut child, page);
                ui.allocate_rect(area, Sense::hover());
            });
    }

    /// The shared flat tab strip (`crate::tab_strip`) over the page, with
    /// the pane's context menu on the strip and on every tab.
    fn tab_strip(&mut self, ui: &mut egui::Ui, page: Rect) {
        let tabs = self.detail.tabs.clone();
        let captions: Vec<&str> = tabs.iter().map(|t| t.caption()).collect();
        let selected = tabs.iter().position(|t| *t == self.detail.tab).unwrap_or(0);
        let strip = crate::tab_strip::show(
            ui,
            "detail",
            page,
            &captions,
            selected,
            &mut self.detail.strip_first,
        );
        for response in std::iter::once(&strip.strip).chain(strip.tabs.iter().map(|(_, r)| r)) {
            egui::Popup::context_menu(response)
                .style(chrome::popup_menu_style)
                .show(|ui| self.pane_menu(ui));
        }
        for (index, response) in &strip.tabs {
            self.action(&format!("detail_tab_{}", tabs[*index].key()), response);
        }
        if let Some(index) = strip.clicked {
            self.detail.tab = tabs[index];
        }
    }

    fn detail_page(&mut self, ui: &mut egui::Ui, page: Rect) {
        if !self.detail.tabs.contains(&self.detail.tab) {
            self.detail.tab = self.detail.tabs[0];
        }
        let content = Rect::from_min_max(
            page.min + Vec2::new(form::PAD, form::CONTENT_TOP),
            page.max - Vec2::splat(form::PAD),
        );
        let Some(i) = self
            .selected
            .filter(|&i| i < self.project.model.activities.len())
        else {
            ui.painter().text(
                content.min,
                Align2::LEFT_TOP,
                "Select an activity to view its details.",
                form::font(),
                INK,
            );
            return;
        };
        let key = (self.project.model.activities[i].task_id, self.detail.tab);
        if self.detail.context != Some(key) {
            self.detail.context = Some(key);
            self.detail.rows = [None, None];
            self.detail.document_details = None;
        }
        self.activity_header(ui, i, page);
        let area = content;
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(area)
                .layout(Layout::top_down(Align::LEFT)),
        );
        child.set_clip_rect(area.intersect(ui.clip_rect()));
        child.spacing_mut().item_spacing = Vec2::new(form::GAP, form::GAP);
        let ui = &mut child;
        match self.detail.tab {
            DetailTab::General => self.general_tab(ui, i),
            DetailTab::Status => self.status_tab(ui, i),
            DetailTab::Predecessors => self.relationship_tab(ui, i, Some(true)),
            DetailTab::Successors => self.relationship_tab(ui, i, Some(false)),
            DetailTab::Relationships => self.relationship_tab(ui, i, None),
            DetailTab::Resources => self.resources_tab(ui, i),
            DetailTab::Steps => self.steps_tab(ui, i),
            DetailTab::Notebook => self.notebook_tab(ui, i),
            DetailTab::Codes => self.codes_tab(ui, i),
            DetailTab::Expenses => self.expenses_tab(ui, i),
            DetailTab::Documents => self.documents_tab(ui, i),
            DetailTab::Summary => self.summary_tab(ui, i),
            DetailTab::Feedback => self.feedback_tab(ui, i),
            DetailTab::Risks => self.risks_tab(ui, i),
            DetailTab::UserDefinedFields => self.udf_tab(ui, i),
        }
    }

    /// The header row over every tab: the previous/next spinner, Activity ID
    /// and name (editable) and the project (read only), at the measured
    /// places. The name stretches with the pane; the project field keeps
    /// its width at the right.
    fn activity_header(&mut self, ui: &mut egui::Ui, i: usize, page: Rect) {
        let (l, t) = (page.left(), page.top());
        let (previous, next) = form::spinner(ui, Pos2::new(l + 21., t + 4.));
        self.action("detail_previous", &previous);
        self.action("detail_next", &next);
        if previous.clicked() || next.clicked() {
            let order: Vec<usize> = self
                .rows()
                .into_iter()
                .filter_map(|r| match r {
                    crate::grid::Row::Activity(a) => Some(a),
                    _ => None,
                })
                .collect();
            if let Some(at) = order.iter().position(|&a| a == i) {
                let to = if previous.clicked() {
                    at.checked_sub(1)
                } else {
                    Some(at + 1).filter(|&n| n < order.len())
                };
                if let Some(to) = to {
                    self.selected = Some(order[to]);
                    self.scroll_to_selected = true;
                }
            }
        }
        let width = page.width();
        let id_field = form::rect(l + 230., t + 6., 242., form::FIELD_H);
        let name_right = (l + width - 548.).max(id_field.right() + 134.);
        let name_field = Rect::from_min_max(
            Pos2::new(l + 485., t + 6.),
            Pos2::new(name_right, t + 6. + form::FIELD_H),
        );
        let project_field = Rect::from_min_max(
            Pos2::new((l + width - 375.).max(name_right + 174.), t + 6.),
            Pos2::new(
                (l + width - 43.).max(name_right + 300.),
                t + 6. + form::FIELD_H,
            ),
        );
        let p = ui.painter().clone();
        form::label(&p, id_field.left() - 44., id_field.top(), "Activity", true);
        // Both fields commit on Enter or focus loss through the same setters
        // as the table cells, so an empty or duplicate ID is refused and
        // nothing is written while the user types. The ids carry the
        // activity, so a draft never lands on the next activity shown.
        let task_id = self.project.model.activities[i].task_id;
        let shown_code = self.project.model.activities[i].task_code.clone();
        let (code, typed) = form::commit_edit(
            ui,
            ui.id().with(("detail_activity_id", task_id)),
            id_field,
            &shown_code,
            true,
            Align::Min,
            6.,
        );
        self.action("detail_activity_id", &code);
        if let Some(text) = typed {
            match openxer_xer::edit::activity::set_activity_id(&mut self.project, i, &text) {
                Ok(changed) => self.dirty |= changed,
                Err(e) => self.error(e),
            }
        }
        let shown_name = self.project.name(i).to_string();
        let (name, typed) = form::commit_edit(
            ui,
            ui.id().with(("detail_activity_name", task_id)),
            name_field,
            &shown_name,
            true,
            Align::Min,
            6.,
        );
        self.action("detail_activity_name", &name);
        if let Some(text) = typed {
            match openxer_xer::edit::activity::set_activity_name(&mut self.project, i, &text) {
                Ok(changed) => self.dirty |= changed,
                Err(e) => self.error(e),
            }
        }
        form::label(
            &p,
            project_field.left() - 44.,
            project_field.top(),
            "Project",
            true,
        );
        form::value(
            &p,
            project_field,
            &self.project.model.project.name,
            false,
            form::READONLY,
        );
    }

    /// A date field at `r`: text in the user's date format, committed on
    /// Enter or focus loss, and a `...` button opening a calendar. Returns
    /// Ok(true) when the value changed. `edge` is the activity and field: a
    /// date typed without a time into an empty field, or picked with none
    /// there, takes the start of the activity calendar's working day (its
    /// end for a finish-type field).
    pub(crate) fn detail_date_field(
        &mut self,
        ui: &mut egui::Ui,
        key: &str,
        r: Rect,
        value: &mut Option<i64>,
        enabled: bool,
        edge: (usize, crate::grid::columns::Field),
    ) -> Result<bool, String> {
        let origin = self.project.model.origin_serial;
        let (i, field) = edge;
        let finish = self
            .project
            .model
            .activities
            .get(i)
            .is_some_and(|a| crate::grid::edit::finish_type(field, a));
        let data_day = self
            .project
            .model
            .project
            .data_date
            .unwrap_or(0)
            .div_euclid(DAY);
        let fallback = data_day * DAY + self.day_edge(i, finish, data_day);
        let shown = format::date(origin, *value);
        let id = ui.id().with(("date_field", key));
        let (response, typed) = form::commit_edit(ui, id, r, &shown, enabled, Align::Min, 20.);
        self.action(&format!("detail_date_{key}"), &response);
        let mut result = Ok(false);
        if let Some(text) = typed {
            match self.parse_field_date(i, finish, &text, *value) {
                Ok(v) if v != *value => {
                    *value = v;
                    result = Ok(true);
                }
                Ok(_) => {}
                Err(e) => result = Err(e),
            }
        }
        let button = form::ellipsis_button(&ui.painter().clone(), r, enabled);
        if enabled {
            let current = value.unwrap_or(fallback);
            let text = date_string(origin, Some(current));
            if let Ok(stamp) = chrono::NaiveDateTime::parse_from_str(&text, "%Y-%m-%d %H:%M") {
                let mut day = stamp.date();
                let picker = form::calendar_drop_down(ui, button, key, &mut day);
                // The calendar drop-down keeps working under the classic face.
                form::ellipsis_button(ui.painter(), r, true);
                if picker.changed() && day != stamp.date() {
                    let picked = day
                        .and_time(stamp.time())
                        .format("%Y-%m-%d %H:%M")
                        .to_string();
                    if let Ok(v) = openxer_xer::parse_relative(origin, &picked) {
                        *value = v;
                        result = Ok(true);
                    }
                }
            }
        }
        result
    }

    /// A date box with a calendar drop-down. Returns Ok(true) when the value changed.
    pub(crate) fn date_edit(
        &mut self,
        ui: &mut egui::Ui,
        key: &str,
        value: &mut Option<i64>,
        enabled: bool,
    ) -> Result<bool, String> {
        let origin = self.project.model.origin_serial;
        let fallback = self
            .project
            .model
            .project
            .data_date
            .unwrap_or(0)
            .div_euclid(DAY)
            * DAY
            + 8 * 60;
        let text_id = ui.id().with(("date_text", key));
        let focused = ui.memory(|m| m.has_focus(text_id));
        let entry = self
            .detail
            .dates
            .entry(key.to_string())
            .or_insert_with(|| (*value, format::date(origin, *value)));
        if entry.0 != *value && !focused {
            *entry = (*value, format::date(origin, *value));
        }
        let mut changed = Ok(false);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.;
            let response = ui.add_enabled(
                enabled,
                egui::TextEdit::singleline(&mut entry.1)
                    .id(text_id)
                    .desired_width(104.)
                    .hint_text(if enabled {
                        format::current().date_example()
                    } else {
                        String::new()
                    }),
            );
            crate::dialog_keys::note_keeps_enter(&response);
            if response.lost_focus()
                || response.has_focus() && ui.input(|i| i.key_pressed(Key::Enter))
            {
                match format::parse_date(origin, &entry.1, *value) {
                    Ok(v) if v != *value => {
                        *value = v;
                        entry.0 = v;
                        changed = Ok(true);
                    }
                    Ok(_) => {}
                    Err(e) => {
                        entry.1 = format::date(origin, *value);
                        changed = Err(e);
                    }
                }
            }
            let current = value.unwrap_or(fallback);
            let text = date_string(origin, Some(current));
            if let Ok(stamp) = chrono::NaiveDateTime::parse_from_str(&text, "%Y-%m-%d %H:%M") {
                let mut day = stamp.date();
                let (btn_rect, _) = ui.allocate_exact_size(Vec2::new(19., 21.), Sense::hover());
                let picker = if enabled {
                    let r = form::calendar_drop_down(ui, btn_rect, key, &mut day);
                    form::ellipsis_button(ui.painter(), btn_rect.expand2(Vec2::new(2., 0.)), true);
                    Some(r)
                } else {
                    form::ellipsis_button(ui.painter(), btn_rect.expand2(Vec2::new(2., 0.)), false);
                    None
                };
                if picker.is_some_and(|p| p.changed()) && day != stamp.date() {
                    let picked = day
                        .and_time(stamp.time())
                        .format("%Y-%m-%d %H:%M")
                        .to_string();
                    if let Ok(v) = openxer_xer::parse_relative(origin, &picked) {
                        *value = v;
                        *entry = (v, format::date(origin, v));
                        changed = Ok(true);
                    }
                }
            }
        });
        changed
    }

    /// Notebook-style memo editor. Returns the new HTML only when the text changed.
    pub(crate) fn memo_editor(
        &mut self,
        ui: &mut egui::Ui,
        key: String,
        html: &str,
        size: Vec2,
        enabled: bool,
    ) -> Option<String> {
        let entry = self
            .detail
            .memo
            .entry(key.clone())
            .or_insert_with(|| (html.to_string(), records::memo_to_text(html)));
        if entry.0 != html {
            *entry = (html.to_string(), records::memo_to_text(html));
        }
        let response = egui::Frame::NONE
            .fill(Color32::WHITE)
            .show(ui, |ui| {
                ui.set_min_size(size);
                ui.set_max_size(size);
                egui::ScrollArea::vertical()
                    .id_salt(("memo", &key))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let response = ui.add_enabled(
                            enabled,
                            egui::TextEdit::multiline(&mut entry.1)
                                .id(egui::Id::new(("memo_text", &key)))
                                .frame(false)
                                .desired_width(size.x - 12.)
                                .desired_rows(((size.y - 4.) / 14.).max(1.) as usize),
                        );
                        crate::dialog_keys::note_keeps_enter(&response);
                        response
                    })
                    .inner
            })
            .inner;
        if response.changed() && entry.1 != records::memo_to_text(html) {
            let new = records::text_to_memo(&entry.1);
            entry.0 = new.clone();
            return Some(new);
        }
        None
    }

    pub(crate) fn detail_dialogs(&mut self, ctx: &egui::Context) {
        self.customize_dialog(ctx);
        self.picker_dialog(ctx);
        self.assignment_picker(ctx);
        // Add from Template, opened from the Steps tab or Assign > Steps.
        self.template_pick_window(ctx);
        self.assignment_window(ctx);
        self.assignment_question(ctx);
        self.document_details_dialog(ctx);
        self.owner_pickers(ctx);
        self.removal_dialog(ctx);
    }

    /// Customize Activity Details: Available Tabs and Display Tabs lists
    /// with move buttons between them and up/down buttons, and the
    /// OK, Cancel, Apply, Default, Help column, at the measured places.
    fn customize_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.detail.customize.take() else {
            return;
        };
        let mut open = true;
        let mut close = false;
        let mut apply = false;
        let mut help = false;
        let mut actions: Vec<(&str, Rect)> = vec![];
        // Every tab not on display; tabs XER files cannot fill are gone.
        let available: Vec<DetailTab> = DetailTab::ordered()
            .into_iter()
            .filter(|t| !draft.display.contains(t))
            .collect();
        chrome::dialog_window("Activity Details")
            .id(egui::Id::new("customize_activity_details"))
            .open(&mut open)
            .show(ctx, |ui| {
                let buttons = [
                    chrome::DialogButton::ok().enabled(!draft.display.is_empty()),
                    chrome::DialogButton::cancel(),
                    chrome::DialogButton::apply().enabled(!draft.display.is_empty()),
                    chrome::DialogButton::new("Default", None).grouped(),
                    chrome::DialogButton::help(),
                ];
                let (_, clicked, responses) = chrome::dialog_with_buttons(ui, &buttons, |ui| {
                    let (body, _) = ui.allocate_exact_size(Vec2::new(389., 283.), Sense::hover());
                    let o = body.min - Vec2::splat(8.);
                    let p = ui.painter().clone();
                    form::lowered(&p, form::rect(o.x + 5., o.y + 5., 395., 289.));
                    let list = |ui: &mut egui::Ui,
                                x: f32,
                                w: f32,
                                title: &str,
                                items: &[DetailTab],
                                pick: &mut Option<DetailTab>,
                                id: &str| {
                        let frame = form::rect(o.x + x, o.y + 11., w, 277.);
                        form::lowered(&p, frame);
                        let header = form::rect(frame.left() + 1., frame.top() + 1., w - 2., 21.);
                        form::raised(&p, header);
                        p.text(
                            Pos2::new(header.left() + 7., header.top() + 5.),
                            Align2::LEFT_TOP,
                            title,
                            form::font(),
                            INK,
                        );
                        let body = Rect::from_min_max(
                            Pos2::new(frame.left() + 1., header.bottom()),
                            frame.max - Vec2::splat(1.),
                        );
                        form::fill(&p, body, Color32::WHITE);
                        for (row, tab) in items.iter().enumerate() {
                            let r = form::rect(
                                body.left(),
                                body.top() + 1. + 14. * row as f32,
                                body.width(),
                                14.,
                            );
                            let response = ui.interact(r, ui.id().with((id, row)), Sense::click());
                            let selected = *pick == Some(*tab);
                            if selected {
                                form::fill(&p, r, SELECT);
                            }
                            p.text(
                                Pos2::new(r.left() + 2., r.top() + 1.),
                                Align2::LEFT_TOP,
                                tab.caption(),
                                form::font(),
                                if selected { Color32::WHITE } else { INK },
                            );
                            if response.clicked() {
                                *pick = Some(*tab);
                            }
                        }
                    };
                    list(
                        ui,
                        11.,
                        144.,
                        "Available Tabs",
                        &available,
                        &mut draft.available_pick,
                        "available_tabs",
                    );
                    list(
                        ui,
                        208.,
                        138.,
                        "Display Tabs",
                        &draft.display.clone(),
                        &mut draft.display_pick,
                        "display_tabs",
                    );
                    // 33 x 33 move buttons with drawn arrows.
                    let mut arrow = |ui: &mut egui::Ui,
                                     key: &'static str,
                                     x: f32,
                                     y: f32,
                                     glyph: &str,
                                     enabled: bool|
                     -> bool {
                        let r = form::rect(o.x + x, o.y + y, 33., 33.);
                        let response = ui.interact(
                            r,
                            ui.id().with(key),
                            if enabled {
                                Sense::click()
                            } else {
                                Sense::hover()
                            },
                        );
                        actions.push((key, r));
                        form::fill(&p, r, form::FACE);
                        form::raised(&p, r);
                        let color = if enabled { INK } else { form::SHADOW };
                        let c = r.center();
                        let triangle = |dx: f32, dir: char| {
                            let (a, b, tip) = match dir {
                                '>' => (
                                    Pos2::new(c.x + dx - 2., c.y - 4.),
                                    Pos2::new(c.x + dx - 2., c.y + 4.),
                                    Pos2::new(c.x + dx + 2., c.y),
                                ),
                                '<' => (
                                    Pos2::new(c.x + dx + 2., c.y - 4.),
                                    Pos2::new(c.x + dx + 2., c.y + 4.),
                                    Pos2::new(c.x + dx - 2., c.y),
                                ),
                                '^' => (
                                    Pos2::new(c.x - 4., c.y + 2.),
                                    Pos2::new(c.x + 4., c.y + 2.),
                                    Pos2::new(c.x, c.y - 2.),
                                ),
                                _ => (
                                    Pos2::new(c.x - 4., c.y - 2.),
                                    Pos2::new(c.x + 4., c.y - 2.),
                                    Pos2::new(c.x, c.y + 2.),
                                ),
                            };
                            p.add(egui::Shape::convex_polygon(
                                vec![a, b, tip],
                                color,
                                Stroke::NONE,
                            ));
                        };
                        match glyph {
                            ">>" => {
                                triangle(-3., '>');
                                triangle(3., '>');
                            }
                            "<<" => {
                                triangle(-3., '<');
                                triangle(3., '<');
                            }
                            g => triangle(0., g.chars().next().unwrap_or('>')),
                        }
                        enabled && response.clicked()
                    };
                    if arrow(
                        ui,
                        "customize_tabs_add",
                        165.,
                        33.,
                        ">",
                        draft.available_pick.is_some(),
                    ) {
                        if let Some(tab) = draft.available_pick.take() {
                            if !draft.display.contains(&tab) {
                                draft.display.push(tab);
                                draft.display_pick = Some(tab);
                            }
                        }
                    }
                    if arrow(
                        ui,
                        "customize_tabs_add_all",
                        165.,
                        68.,
                        ">>",
                        !available.is_empty(),
                    ) {
                        draft.display.extend(available.iter().copied());
                        draft.available_pick = None;
                    }
                    let can_remove = draft.display_pick.is_some() && draft.display.len() > 1;
                    if arrow(ui, "customize_tabs_remove", 165., 114., "<", can_remove) {
                        if let Some(tab) = draft.display_pick.take() {
                            draft.display.retain(|t| *t != tab);
                            draft.available_pick = Some(tab);
                        }
                    }
                    if arrow(
                        ui,
                        "customize_tabs_remove_all",
                        165.,
                        150.,
                        "<<",
                        draft.display.len() > 1,
                    ) {
                        draft.display.truncate(1);
                        draft.display_pick = None;
                    }
                    let position = draft
                        .display_pick
                        .and_then(|t| draft.display.iter().position(|d| *d == t));
                    if arrow(
                        ui,
                        "customize_tabs_up",
                        356.,
                        33.,
                        "^",
                        position.is_some_and(|p| p > 0),
                    ) {
                        let at = position.unwrap();
                        draft.display.swap(at, at - 1);
                    }
                    let can_down = position.is_some_and(|p| p + 1 < draft.display.len());
                    if arrow(ui, "customize_tabs_down", 356., 68., "v", can_down) {
                        let at = position.unwrap();
                        draft.display.swap(at, at + 1);
                    }
                });
                for (name, response) in [
                    "customize_tabs_ok",
                    "customize_tabs_cancel",
                    "customize_tabs_apply",
                    "customize_tabs_default",
                    "customize_tabs_help",
                ]
                .into_iter()
                .zip(&responses)
                {
                    actions.push((name, response.rect));
                }
                match clicked {
                    Some(0) => {
                        apply = true;
                        close = true;
                    }
                    Some(1) => close = true,
                    Some(2) => apply = true,
                    Some(3) => {
                        draft.display = DetailTab::default_tabs();
                        draft.display_pick = None;
                        draft.available_pick = None;
                    }
                    Some(4) => help = true,
                    _ => {}
                }
            });
        if help {
            self.open_help_for("Activity Details");
        }
        for (name, r) in actions {
            self.action_rects
                .insert(name.into(), [r.left(), r.top(), r.right(), r.bottom()]);
        }
        if apply && !draft.display.is_empty() {
            self.detail.tabs = draft.display.clone();
            if !self.detail.tabs.contains(&self.detail.tab) {
                self.detail.tab = self.detail.tabs[0];
            }
            self.save_detail_layout();
        }
        if open && !close && !ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.detail.customize = Some(draft);
        }
    }

    /// Test hook: pick a tab in the customize dialog list.
    #[cfg(test)]
    pub(crate) fn customize_pick(&mut self, tab: DetailTab) {
        if let Some(draft) = &mut self.detail.customize {
            if draft.display.contains(&tab) {
                draft.display_pick = Some(tab);
            } else {
                draft.available_pick = Some(tab);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn layout_round_trip_and_rejects_invalid_data() {
        let tabs = vec![DetailTab::Status, DetailTab::General, DetailTab::Steps];
        assert_eq!(parse_layout(&layout_json(&tabs)), Some(tabs));
        for bad in [
            "",
            "{}",
            "[]",
            "[\"general\",\"general\"]",
            "[\"general\",\"bogus\"]",
            "[1,2]",
            "not json",
        ] {
            assert_eq!(parse_layout(bad), None, "{bad}");
        }
        let defaults = DetailTab::default_tabs();
        let captions: Vec<&str> = defaults.iter().map(|t| t.caption()).collect();
        assert_eq!(
            captions,
            [
                "General",
                "Status",
                "Resources",
                "Codes",
                "Relationships",
                "Notebook",
                "Steps",
                "Notes to Resources",
                "Documents",
                "Risks",
                "Expenses",
                "Summary"
            ]
        );
        for hidden in [
            DetailTab::Predecessors,
            DetailTab::Successors,
            DetailTab::UserDefinedFields,
        ] {
            assert!(
                !defaults.contains(&hidden),
                "{} is hidden",
                hidden.caption()
            );
        }
    }
    #[test]
    fn money_formatting() {
        assert_eq!(money(1234567.5), "$1,234,567.50");
        // Negative costs follow the currency's negative pattern, `(#1.1)` by default.
        assert_eq!(money(-12.345), "($12.35)");
        assert_eq!(money(0.), "$0.00");
    }
}
