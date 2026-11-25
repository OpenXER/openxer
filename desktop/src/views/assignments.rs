//! Resource Assignments window: the project's assignments on the shared
//! view table (`grid`), with the shared assignment column catalog
//! (`assignment_columns`, also the Resources tab's), cells edited in place
//! through the same edit layer calls as the Resources tab, Group and Sort
//! By (resource, role, activity, project, cost account), and the Resource
//! Usage Spreadsheet's lines and cell commands as its time-phased
//! spreadsheet (`panes::sheet_pane`), on a week over day timescale. The
//! Assignment Details tabs sit below. Its layouts are kept by
//! `assignment_layouts`.
use super::assignment_columns::{self as ac, Source, FIELDS};
use super::grid::{self, Columns, Row};
use super::resources;
use super::sheet;
use super::*;
use crate::context_menu::{Entry, Menu, Spec};
use crate::grid::columns as cols;
use crate::panes::sheet_pane::{self, SheetPane};
use crate::panes::time_fields::{self, Sheet, TimeField};
use crate::panes::usage::ResRow;
use crate::panes::usage_cells::SheetHost;
use crate::schedule_dialogs::Canvas;
use crate::views::table_cells::CellKey;
use openxer_xer::assignments::{AssignmentEdit, AssignmentView, RATE_TYPES};
use openxer_xer::edit::curves;
use openxer_xer::spread::{self, Plan, Shaping};
use std::collections::{BTreeSet, HashMap};
use std::rc::Rc;
use std::sync::{Arc, OnceLock};

/// Zoom steps of the spreadsheet, in pixels a week.
pub const ZOOM_STEPS: [f64; 7] = [49., 70., 100., 140., 200., 280., 400.];
/// The spreadsheet's zoom range (its first and last step), for Ctrl+wheel
/// and header drags.
pub const ZOOM_LIMITS: (f64, f64) = (ZOOM_STEPS[0], ZOOM_STEPS[6]);
/// Default day pitch of the spreadsheet (100 px a week).
pub const SHEET_WEEK: f64 = 100.;
pub const TABS: [&str; 4] = ["General", "Planning", "User Defined Fields", "Codes"];
/// Table width to the splitter, as measured.
const TABLE_WIDTH: f32 = 255.;
/// The table's id: its rectangles are `assignments_*`.
pub const TABLE: &str = "assignments";

/// A list window opened from a General tab picker: what it sets on which
/// TASKRSRC row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssignPick {
    Resource(usize),
    Role(usize),
    Account(usize),
}

/// One TASKRSRC row of the open project.
#[derive(Clone, Debug, PartialEq)]
pub struct Assignment {
    /// TASKRSRC row index.
    pub row: usize,
    pub id: i64,
    pub task: usize,
    pub rsrc: i64,
    pub role: String,
    pub budget: f64,
    pub actual: f64,
    pub remaining: f64,
    pub budget_per_hour: f64,
    pub remaining_per_hour: f64,
    pub cost_per_qty: f64,
    pub rate_type: String,
    pub account: String,
    /// Budgeted, actual (regular plus overtime) and remaining cost.
    pub budget_cost: f64,
    pub actual_cost: f64,
    pub remaining_cost: f64,
    /// The activity's original duration in minutes (the planned span a
    /// curved budget spreads over).
    pub original: i64,
    /// What the spreads need of the assignment's row: its own lag,
    /// durations, calendar and actual dates, and the resource curve that
    /// shapes it (none: even). See `openxer_xer::spread::Shaping`.
    pub shaping: Option<Shaping>,
    /// Its spread windows and curve position, worked out on first use.
    pub plan: PlanCell,
}

/// An assignment's spread plan (`openxer_xer::spread::Plan`), worked out the
/// first time one of its spreads is asked for and shared by its clones (the
/// cost copy of `as_costs`, the per-activity lists), so a kept assignment
/// list walks the calendar once per assignment, not once per spread or per
/// change of periods. A plan follows from the assignment and the model it
/// was built from, so it never makes two assignments unequal.
#[derive(Clone, Debug, Default)]
pub struct PlanCell(Arc<OnceLock<Plan>>);

impl PartialEq for PlanCell {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl Assignment {
    /// A resource curve shapes this assignment's spread.
    #[cfg(test)]
    pub fn curved(&self) -> bool {
        self.shaping.as_ref().is_some_and(|s| s.shape.is_some())
    }

    /// Its spread plan in `p`, the project it was built from.
    pub fn plan(&self, p: &ProjectData) -> &Plan {
        self.plan
            .0
            .get_or_init(|| Plan::of(&p.model, &item(p, self)))
    }
    /// The same assignment with its costs in the unit fields, so the unit
    /// spreads (`remaining_spread`, `actual_spread`, `budget_spread`) spread costs.
    pub fn as_costs(&self) -> Assignment {
        Assignment {
            budget: self.budget_cost,
            actual: self.actual_cost,
            remaining: self.remaining_cost,
            ..self.clone()
        }
    }
}

fn num(s: &str) -> i64 {
    s.trim().parse().unwrap_or(0)
}

fn float(s: &str) -> f64 {
    s.trim().parse().unwrap_or(0.)
}

/// The open project's assignments with an activity in the model: the shared
/// rows (`openxer_xer::spread::assignment_rows`, what the command line and
/// the S-curve export spread) with the fields this view also shows.
pub fn assignments(p: &ProjectData) -> Vec<Assignment> {
    let Some(t) = p.document.table("TASKRSRC") else {
        return vec![];
    };
    spread::assignment_rows(p)
        .into_iter()
        .map(|x| {
            let r = &t.rows[x.row];
            Assignment {
                row: x.row,
                id: num(t.value(r, "taskrsrc_id")),
                task: x.task,
                rsrc: x.rsrc,
                role: t.value(r, "role_id").to_string(),
                budget: x.units[0],
                actual: x.units[1],
                remaining: x.units[2],
                budget_per_hour: float(t.value(r, "target_qty_per_hr")),
                remaining_per_hour: float(t.value(r, "remain_qty_per_hr")),
                cost_per_qty: float(t.value(r, "cost_per_qty")),
                rate_type: t.value(r, "rate_type").to_string(),
                account: t.value(r, "acct_id").to_string(),
                budget_cost: x.costs[0],
                actual_cost: x.costs[1],
                remaining_cost: x.costs[2],
                original: x.original,
                shaping: x.shaping,
                plan: PlanCell::default(),
            }
        })
        .collect()
}

/// The assignment as the shared spreads see it.
pub fn item<'a>(p: &'a ProjectData, a: &'a Assignment) -> spread::Item<'a> {
    spread::Item {
        activity: &p.model.activities[a.task],
        original: a.original,
        shaping: a.shaping.as_ref(),
    }
}

/// Remaining units of one assignment added into `out` per period: the
/// remaining quantity spread over its remaining dates on its calendar, along
/// its resource curve when one applies.
pub fn remaining_into(p: &ProjectData, a: &Assignment, periods: &[(i64, i64)], out: &mut [f64]) {
    spread::remaining(&p.model, &item(p, a), a.plan(p), a.remaining, periods, out);
}

/// Remaining units of one assignment over the remaining late dates, added
/// into `out`.
pub fn remaining_late_into(
    p: &ProjectData,
    a: &Assignment,
    periods: &[(i64, i64)],
    out: &mut [f64],
) {
    spread::remaining_late(&p.model, &item(p, a), a.plan(p), a.remaining, periods, out);
}

/// `quantity` (the actual units, or a part of them such as overtime) of one
/// assignment from its actual start to the actual finish or the data date,
/// added into `out`.
pub fn actual_of_into(
    p: &ProjectData,
    a: &Assignment,
    quantity: f64,
    periods: &[(i64, i64)],
    out: &mut [f64],
) {
    spread::actual(&p.model, &item(p, a), a.plan(p), quantity, periods, out);
}

/// Actual units of one assignment added into `out`.
pub fn actual_into(p: &ProjectData, a: &Assignment, periods: &[(i64, i64)], out: &mut [f64]) {
    actual_of_into(p, a, a.actual, periods, out);
}

/// Budgeted units of one assignment over the planned dates, added into
/// `out`.
pub fn budget_into(p: &ProjectData, a: &Assignment, periods: &[(i64, i64)], out: &mut [f64]) {
    spread::budget(&p.model, &item(p, a), a.plan(p), a.budget, periods, out);
}

/// Remaining units of one assignment per period (see `remaining_into`).
pub fn remaining_spread(p: &ProjectData, a: &Assignment, periods: &[(i64, i64)]) -> Vec<f64> {
    let mut out = vec![0.; periods.len()];
    remaining_into(p, a, periods, &mut out);
    out
}

/// Actual units of one assignment per period.
pub fn actual_spread(p: &ProjectData, a: &Assignment, periods: &[(i64, i64)]) -> Vec<f64> {
    let mut out = vec![0.; periods.len()];
    actual_into(p, a, periods, &mut out);
    out
}

/// Budgeted units of one assignment over the planned dates.
pub fn budget_spread(p: &ProjectData, a: &Assignment, periods: &[(i64, i64)]) -> Vec<f64> {
    let mut out = vec![0.; periods.len()];
    budget_into(p, a, periods, &mut out);
    out
}

/// Model minutes far beyond any schedule date: the usage views add a period
/// from `-EDGE` up to the first period on show and one from the last to
/// `EDGE`, so what falls outside the view is still counted (see
/// `panes::usage::edged`).
pub const EDGE: i64 = i64::MAX / 8;

/// Whether `period` is one of those edge periods (shifted a little for
/// another project's origin, it still is). Nothing that walks the calendar
/// period by period, such as a limit, may run over one.
pub fn is_edge(&(a, b): &(i64, i64)) -> bool {
    a <= -EDGE / 2 || b >= EDGE / 2
}

/// Positions in `list` of each resource's assignments, in list order: built
/// once for a project's list so per-resource views do not rebuild or scan
/// the whole list for every resource.
pub fn by_resource(list: &[Assignment]) -> HashMap<i64, Vec<usize>> {
    let mut out: HashMap<i64, Vec<usize>> = HashMap::new();
    for (k, a) in list.iter().enumerate() {
        out.entry(a.rsrc).or_default().push(k);
    }
    out
}

/// Group and Sort By: what the bands of the table stand for. The
/// window opens grouped by resource.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Group {
    #[default]
    Resource,
    Role,
    Activity,
    Project,
    CostAccount,
}

impl Group {
    /// In the order of the Group and Sort By submenu.
    pub const ALL: [Group; 5] = [
        Group::Resource,
        Group::Role,
        Group::Activity,
        Group::Project,
        Group::CostAccount,
    ];
    pub fn key(self) -> &'static str {
        match self {
            Group::Resource => "resource",
            Group::Role => "role",
            Group::Activity => "activity",
            Group::Project => "project",
            Group::CostAccount => "cost_account",
        }
    }
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|g| g.key() == key)
    }
    pub fn caption(self) -> &'static str {
        match self {
            Group::Resource => "Resource",
            Group::Role => "Role",
            Group::Activity => "Activity",
            Group::Project => "Project",
            Group::CostAccount => "Cost Account",
        }
    }
    /// The submenu item's key.
    fn menu_key(self) -> &'static str {
        match self {
            Group::Resource => "group_resource",
            Group::Role => "group_role",
            Group::Activity => "group_activity",
            Group::Project => "group_project",
            Group::CostAccount => "group_cost_account",
        }
    }
    fn icon(self) -> chrome::Icon {
        match self {
            Group::Resource => chrome::Icon::Labor,
            Group::Role => chrome::Icon::Resources,
            Group::Activity => chrome::Icon::Activities,
            Group::Project => chrome::Icon::Projects,
            Group::CostAccount => chrome::Icon::Expenses,
        }
    }
}

/// A band of the table: the resource, role, activity (by index), the
/// project or the cost account its assignments share (0: none).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Band {
    Resource(i64),
    Role(i64),
    Activity(usize),
    Project,
    Account(i64),
}

/// What a table row stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Key {
    Band(Band),
    Assignment(i64),
}

impl Key {
    /// The assignment (TASKRSRC id) of an assignment row.
    pub fn assignment(&self) -> Option<i64> {
        match self {
            Key::Assignment(id) => Some(*id),
            Key::Band(_) => None,
        }
    }
}

pub struct AssignmentsState {
    /// The layout opened or saved last (Layout > Open, Save, Save As).
    pub layout_name: String,
    /// Every field of the shared catalog (`assignment_columns::FIELDS`);
    /// `order` holds the ones shown.
    pub columns: Columns,
    pub selected: Option<Key>,
    /// Rows selected along with the current one.
    pub rows: grid::RowSelection<Key>,
    pub collapsed: BTreeSet<Band>,
    pub group: Group,
    pub tab: usize,
    pub show_details: bool,
    /// Resource Usage Spreadsheet: the spreadsheet beside the table.
    pub show_sheet: bool,
    /// The spreadsheet's time-phased fields (`time_fields` ids).
    pub time_field_ids: Vec<String>,
    pub details_height: f32,
    pub table_width: f32,
    /// First minute shown by the spreadsheet; `None` starts at the data date.
    pub first_minute: Option<f64>,
    pub pick: Option<AssignPick>,
    /// Spreadsheet zoom in pixels a week.
    pub zoom: f64,
    /// Show All Projects: the resource bands total the assignments of the
    /// other open projects too.
    pub show_all_projects: bool,
    /// Where the spreadsheet was drawn and its default first minute, for
    /// the zoom commands.
    pub timeline_place: crate::timeline::Place,
    pub timeline_default: f64,
    /// Delete (the key or Edit > Delete) on these assignments (TASKRSRC
    /// ids): the question is up.
    pub confirm_delete: Option<Vec<i64>>,
    /// The spreadsheet's lines as drawn last, one per table row, for its
    /// cell commands (`usage_cells`). Not saved.
    pub(crate) sheet_lines: Option<Rc<Vec<ResRow>>>,
}

impl Default for AssignmentsState {
    fn default() -> Self {
        let widths: Vec<f32> = FIELDS.iter().map(|f| f.width).collect();
        let mut columns = Columns::new(&widths);
        columns.order = ac::default_order();
        Self {
            layout_name: super::assignment_layouts::STANDARD.into(),
            columns,
            selected: None,
            rows: Default::default(),
            collapsed: BTreeSet::new(),
            group: Group::Resource,
            tab: 0,
            show_details: true,
            show_sheet: true,
            time_field_ids: time_fields::default_ids(Sheet::Resource),
            details_height: 255.,
            table_width: TABLE_WIDTH,
            first_minute: None,
            pick: None,
            zoom: SHEET_WEEK,
            show_all_projects: false,
            timeline_place: Default::default(),
            timeline_default: 0.,
            confirm_delete: None,
            sheet_lines: None,
        }
    }
}

impl AssignmentsState {
    pub fn reset(&mut self) {
        self.selected = None;
        self.rows = Default::default();
        self.collapsed.clear();
        self.first_minute = None;
        self.pick = None;
        self.sheet_lines = None;
    }
    pub fn json(&self) -> serde_json::Value {
        serde_json::json!({"columns": self.columns.json(), "tab": self.tab,
            "show_details": self.show_details, "details_height": self.details_height,
            "table_width": self.table_width, "zoom": self.zoom,
            "show_all_projects": self.show_all_projects, "layout_name": self.layout_name,
            "group": self.group.key(), "show_sheet": self.show_sheet,
            "time_fields": self.time_field_ids})
    }
    pub fn restore(&mut self, value: &serde_json::Value) {
        if let Some(name) = value["layout_name"]
            .as_str()
            .and_then(|n| crate::grid::layout::check_name(n).ok())
        {
            self.layout_name = name;
        }
        self.columns.restore(&value["columns"]);
        if let Some(tab) = value["tab"].as_u64().filter(|t| (*t as usize) < TABS.len()) {
            self.tab = tab as usize;
        }
        if let Some(show) = value["show_details"].as_bool() {
            self.show_details = show;
        }
        if let Some(show) = value["show_sheet"].as_bool() {
            self.show_sheet = show;
        }
        if let Some(group) = value["group"].as_str().and_then(Group::from_key) {
            self.group = group;
        }
        if let Some(ids) = time_fields::restore_ids(&value["time_fields"], Sheet::Resource) {
            self.time_field_ids = ids;
        }
        restore_height(&value["details_height"], &mut self.details_height);
        restore_width(&value["table_width"], &mut self.table_width);
        if let Some(zoom) = value["zoom"]
            .as_f64()
            .filter(|z| z.is_finite() && (ZOOM_LIMITS.0..=ZOOM_LIMITS.1).contains(z))
        {
            self.zoom = zoom;
        }
        if let Some(all) = value["show_all_projects"].as_bool() {
            self.show_all_projects = all;
        }
    }
    /// Zoom In or Out one step; the same zoom at the end of the range.
    pub fn zoom_step(&mut self, zoom_in: bool) {
        let at = ZOOM_STEPS
            .iter()
            .position(|z| *z >= self.zoom - 1e-9)
            .unwrap_or(2);
        let to = if zoom_in {
            (at + 1).min(ZOOM_STEPS.len() - 1)
        } else {
            at.saturating_sub(1)
        };
        self.zoom = ZOOM_STEPS[to];
    }
    /// The spreadsheet's time-phased fields.
    pub fn time_fields(&self) -> Vec<TimeField> {
        time_fields::fields_of(Sheet::Resource, &self.time_field_ids)
    }
    /// The first shown field: it carries the tree (bands, icons).
    pub fn tree_field(&self) -> usize {
        self.columns.order.first().copied().unwrap_or(0)
    }
    /// Columns > Default: the window's own columns, in order.
    pub fn default_columns(&mut self) {
        let shown: Vec<Result<usize, String>> = ac::default_order().into_iter().map(Ok).collect();
        self.columns.show_fields(&shown, super::udf_columns::WIDTH);
    }
}

/// One row of the table as grouped: a band, or an assignment (its place
/// in the assignment list).
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Band {
        band: Band,
        caption: String,
        open: bool,
    },
    Assignment(usize),
}

/// (code, name) of each row of a dictionary table, by id.
fn names_of(
    p: &ProjectData,
    table: &str,
    key: &str,
    code: &str,
    name: &str,
) -> HashMap<i64, (String, String)> {
    p.document.table(table).map_or_else(HashMap::new, |t| {
        t.rows
            .iter()
            .map(|r| {
                (
                    num(t.value(r, key)),
                    (t.value(r, code).to_string(), t.value(r, name).to_string()),
                )
            })
            .collect()
    })
}

/// A band's caption: the name, else the code; `none` when the
/// assignments have no such value.
fn caption_of(found: Option<&(String, String)>, none: &str) -> String {
    match found {
        Some((code, name)) if name.is_empty() => code.clone(),
        Some((_, name)) => name.clone(),
        None => none.to_string(),
    }
}

/// The table's rows for Group and Sort By `group`: one band per resource
/// (role, activity, cost account; the project), in code order with the
/// empty band last, and each band's assignments under it by Activity ID
/// (by Resource ID under an activity). A collapsed band hides them.
pub fn layout(
    p: &ProjectData,
    list: &[Assignment],
    group: Group,
    collapsed: &BTreeSet<Band>,
) -> Vec<Item> {
    let resources = names_of(p, "RSRC", "rsrc_id", "rsrc_short_name", "rsrc_name");
    let roles = names_of(p, "ROLES", "role_id", "role_short_name", "role_name");
    let accounts = names_of(p, "ACCOUNT", "acct_id", "acct_short_name", "acct_name");
    let code_of = |m: &HashMap<i64, (String, String)>, id: i64| m.get(&id).map(|n| n.0.clone());
    // (empty band last, sort text, band) -> the assignments' places.
    let mut bands: BTreeMap<(bool, String, Band), Vec<usize>> = BTreeMap::new();
    for (k, a) in list.iter().enumerate() {
        let (band, code) = match group {
            Group::Resource => (Band::Resource(a.rsrc), code_of(&resources, a.rsrc)),
            Group::Role => {
                let role = num(&a.role);
                (Band::Role(role), code_of(&roles, role))
            }
            Group::Activity => (
                Band::Activity(a.task),
                Some(p.model.activities[a.task].task_code.clone()),
            ),
            Group::Project => (Band::Project, Some(String::new())),
            Group::CostAccount => {
                let account = num(&a.account);
                (Band::Account(account), code_of(&accounts, account))
            }
        };
        let empty = code.is_none();
        bands
            .entry((empty, code.unwrap_or_default(), band))
            .or_default()
            .push(k);
    }
    let mut out = vec![];
    for ((_, _, band), mut members) in bands {
        let caption = match band {
            Band::Resource(id) => caption_of(resources.get(&id), ""),
            Band::Role(id) => caption_of(roles.get(&id), "No Role"),
            Band::Activity(task) => p.name(task).to_string(),
            Band::Project => p.model.project.name.clone(),
            Band::Account(id) => caption_of(accounts.get(&id), "No Cost Account"),
        };
        let open = !collapsed.contains(&band);
        out.push(Item::Band {
            band,
            caption,
            open,
        });
        if !open {
            continue;
        }
        members.sort_by(|&x, &y| {
            let (a, b) = (&list[x], &list[y]);
            let order = if group == Group::Activity {
                let code = |r: i64| code_of(&resources, r).unwrap_or_default();
                code(a.rsrc).cmp(&code(b.rsrc))
            } else {
                let (s, t) = (&p.model.activities[a.task], &p.model.activities[b.task]);
                s.task_code.cmp(&t.task_code)
            };
            order.then(a.id.cmp(&b.id))
        });
        out.extend(members.into_iter().map(Item::Assignment));
    }
    out
}

/// The table as drawn: its rows with their keys, the row each assignment
/// line belongs to, and the spreadsheet line of each row.
pub struct Built {
    pub rows: Vec<Row>,
    /// The selectable rows' keys (`None`: a field line after an
    /// assignment's first, which follows its assignment).
    pub keys: Vec<Option<Key>>,
    /// Each row's assignment, by its place in the list.
    pub assignment_of: Vec<Option<usize>>,
    /// Each row's band, for the band rows.
    pub band_of: Vec<Option<Band>>,
    /// Each row's time-phased field (its place in the chosen list).
    pub field_of: Vec<usize>,
}

impl crate::views::table_cells::RowCells for Built {
    fn rows(&self) -> &[Row] {
        &self.rows
    }
}

/// A check item of a menu.
fn check(key: &'static str, label: &'static str, on: bool) -> Entry {
    Spec::new(key, label).checked(on).into()
}

/// The Layout submenu: Open..., Save, Save As....
fn layout_entries() -> Entry {
    use chrome::Icon;
    Entry::Submenu(
        Spec::new("layout", "Layout"),
        vec![
            Spec::new("layout_open", "Open...").icon(Icon::Open).into(),
            Spec::new("layout_save", "Save").icon(Icon::Save).into(),
            Spec::new("layout_save_as", "Save As...")
                .icon(Icon::Save)
                .into(),
        ],
    )
}

/// The Columns submenu: Default, then Customize....
fn columns_entries() -> Entry {
    Entry::Submenu(
        Spec::new("columns", "Columns").icon(chrome::Icon::Columns),
        vec![
            Spec::new("columns_default", "Default").into(),
            Spec::new("columns_customize", "Customize...").into(),
        ],
    )
}

/// The Group and Sort By submenu: the five groupings, the one in effect
/// checked.
fn group_entries(group: Group) -> Entry {
    let spec = Spec::new("group_by", "Group and Sort By");
    Entry::Submenu(
        spec.icon(chrome::Icon::GroupSort),
        Group::ALL
            .into_iter()
            .map(|g| {
                Spec::new(g.menu_key(), g.caption())
                    .icon(g.icon())
                    .checked(g == group)
                    .into()
            })
            .collect(),
    )
}

/// Expand All and Collapse All.
fn expand_entries(state: &AssignmentsState) -> [Entry; 2] {
    let any = !state.collapsed.is_empty();
    [
        Spec::new("expand_all", "Expand All")
            .key("Ctrl+Num +")
            .icon(chrome::Icon::ExpandAll)
            .enabled(any)
            .into(),
        Spec::new("collapse_all", "Collapse All")
            .key("Ctrl+Num -")
            .icon(chrome::Icon::CollapseAll)
            .into(),
    ]
}

/// The layout options bar's menu (`Layout: ...`), in the reference's
/// order: Layout, Assignment Details, Resource Usage Spreadsheet; Columns;
/// Group and Sort By; Expand All, Collapse All.
fn layout_bar_entries(state: &AssignmentsState) -> Vec<Entry> {
    let mut out = vec![
        layout_entries(),
        check("details", "Assignment Details", state.show_details),
        check(
            "resource_usage",
            "Resource Usage Spreadsheet",
            state.show_sheet,
        ),
        Entry::Separator,
        columns_entries(),
        Entry::Separator,
        group_entries(state.group),
        Entry::Separator,
    ];
    out.extend(expand_entries(state));
    out
}

/// The table's right-click menu: the window's views and spreadsheet
/// commands (Layout, Resource Usage Spreadsheet, Assignment Details,
/// Timescale, Spreadsheet Options, Zoom), then Add, Delete and Fill Down,
/// Columns, Group and Sort By, Expand All and Collapse All.
fn menu_entries(state: &AssignmentsState, delete: bool, fill: bool) -> Vec<Entry> {
    use chrome::Icon;
    let mut out = vec![
        layout_entries(),
        check(
            "resource_usage",
            "Resource Usage Spreadsheet",
            state.show_sheet,
        ),
        check("details", "Assignment Details", state.show_details),
        Entry::Separator,
        Spec::new("timescale", "Timescale...")
            .icon(Icon::Timescale)
            .into(),
        Entry::Separator,
        Spec::new("options", "Spreadsheet Options...")
            .icon(Icon::Options)
            .into(),
        check(
            "show_all_projects",
            "Show All Projects",
            state.show_all_projects,
        ),
        Entry::Separator,
        Spec::new("zoom_in", "Zoom In")
            .icon(Icon::ZoomIn)
            .enabled(state.zoom < ZOOM_STEPS[ZOOM_STEPS.len() - 1])
            .into(),
        Spec::new("zoom_out", "Zoom Out")
            .icon(Icon::ZoomOut)
            .enabled(state.zoom > ZOOM_STEPS[0])
            .into(),
        Entry::Separator,
        Spec::new("user_preferences", "User Preferences...")
            .icon(Icon::Preferences)
            .into(),
        Entry::Separator,
        Spec::new("add", "Add").key("Ins").icon(Icon::Add).into(),
        Spec::new("delete", "Delete")
            .key("Del")
            .icon(Icon::Delete)
            .enabled(delete)
            .into(),
        Spec::new("fill_down", "Fill Down")
            .key("Ctrl+E")
            .icon(Icon::FillDown)
            .enabled(fill)
            .into(),
        Entry::Separator,
        columns_entries(),
        group_entries(state.group),
        Entry::Separator,
    ];
    out.extend(expand_entries(state));
    out
}

/// The assignment's Cells for the shared catalog.
fn cells_of<'a>(p: &ProjectData, a: &Assignment, view: &'a AssignmentView) -> ac::Cells<'a> {
    let act = &p.model.activities[a.task];
    ac::Cells {
        view,
        activity: ac::Activity {
            started: act.status != StatusType::NotStarted,
            completed: act.is_completed(),
            periods: format::activity_periods(p, a.task),
            origin: p.model.origin_serial,
        },
        curve: curves::curve_label(&p.document, a.row).unwrap_or_default(),
    }
}

impl App {
    /// The table rows for the window's grouping, fields and time-phased
    /// fields; kept while they and the project are unchanged.
    fn assignment_table(&self, list: &Rc<Vec<Assignment>>) -> Rc<Built> {
        let s = &self.views.assignments;
        let serial = self.project_serial();
        let collapsed: Vec<Band> = s.collapsed.iter().copied().collect();
        let lines = s.time_fields().len().max(1);
        let tree = s.tree_field();
        let udfs = super::udf_columns::Shown::of(&self.project.document, &s.columns);
        let extra = s.columns.extra.clone();
        let key = (
            collapsed,
            s.group,
            s.columns.order.clone(),
            extra,
            lines,
            format!("{:?}", format::current()),
        );
        self.memo(serial, "assignment_rows", key, || {
            let p = &self.project;
            let items = layout(p, list, s.group, &s.collapsed);
            let look = ac::Lookups::of(&p.document);
            let catalog = cols::catalog(p);
            let ix = self.shared_index();
            let shown = self.shown();
            let order = s.columns.order.clone();
            let mut b = Built {
                rows: vec![],
                keys: vec![],
                assignment_of: vec![],
                band_of: vec![],
                field_of: vec![],
            };
            let mut band_captions: Vec<(usize, String)> = vec![];
            for item in items {
                match item {
                    Item::Band {
                        band,
                        caption,
                        open,
                    } => {
                        let row_idx = b.rows.len();
                        b.rows.push(Row {
                            expander: Some(open),
                            icon: Some(s.group.icon()),
                            cells: vec![String::new(); FIELDS.len()],
                            band: true,
                            span: true,
                            ..Default::default()
                        });
                        band_captions.push((row_idx, caption));
                        b.keys.push(Some(Key::Band(band)));
                        b.assignment_of.push(None);
                        b.band_of.push(Some(band));
                        b.field_of.push(0);
                    }
                    Item::Assignment(k) => {
                        let a = &list[k];
                        let view = p.assignment_view(a.row).unwrap_or_default();
                        let c = cells_of(p, a, &view);
                        let task = a.task;
                        let activity_text = |id: &str| {
                            let Some(column) = cols::find(&catalog, id) else {
                                return String::new();
                            };
                            let value = cols::shown_value(shown, &ix, column, task);
                            cols::display(shown, Some(task), &value)
                        };
                        let mut cells = vec![String::new(); FIELDS.len()];
                        for &f in &order {
                            if let Some(spec) = FIELDS.get(f) {
                                cells[f] = match spec.source {
                                    Source::Data(d) => ac::text(&look, &c, d),
                                    Source::Activity(id) => activity_text(id),
                                };
                            }
                        }
                        for line in 0..lines {
                            b.rows.push(Row {
                                depth: 1,
                                icon: (line == 0).then_some(chrome::Icon::Activities),
                                cells: if line == 0 {
                                    cells.clone()
                                } else {
                                    vec![String::new(); FIELDS.len()]
                                },
                                strips: 1,
                                ..Default::default()
                            });
                            b.keys.push((line == 0).then_some(Key::Assignment(a.id)));
                            b.assignment_of.push(Some(k));
                            b.band_of.push(None);
                            b.field_of.push(line);
                        }
                    }
                }
            }
            let id = |k: &Option<Key>| k.and_then(|k| k.assignment());
            let records: Vec<Option<i64>> = b.keys.iter().map(id).collect();
            udfs.fill(&p.document, p.model.origin_serial, &mut b.rows, &records);
            for (row_idx, caption) in band_captions {
                if let Some(cell) = b.rows[row_idx].cells.get_mut(tree) {
                    *cell = caption;
                }
            }
            b
        })
    }

    /// The spreadsheet's lines over `periods`, one per table row: an
    /// assignment's line per time-phased field, as the Resource Usage
    /// Spreadsheet spreads it; a band line empty, or under Show All
    /// Projects a resource band with the resource's remaining units across
    /// the open projects.
    fn assignment_sheet_lines(
        &self,
        list: &Rc<Vec<Assignment>>,
        built: &Rc<Built>,
        periods: &[(i64, i64)],
        all_projects: bool,
    ) -> Rc<Vec<ResRow>> {
        let s = &self.views.assignments;
        let serial = self.project_serial();
        let fields = s.time_fields();
        let key = (
            s.time_field_ids.clone(),
            s.collapsed.iter().copied().collect::<Vec<Band>>(),
            s.group,
            s.columns.order.clone(),
            periods.to_vec(),
            all_projects,
            self.open_set.version,
        );
        self.memo(serial, "assignment_sheet", key, || {
            let p = &self.project;
            let list_r = resources::resources(p);
            let edges = crate::panes::usage::edged(periods);
            let mut out = Vec::with_capacity(built.rows.len());
            for r in 0..built.rows.len() {
                let (Some(k), field) = (built.assignment_of[r], built.field_of[r]) else {
                    let total = match built.band_of[r] {
                        Some(Band::Resource(rsrc)) if all_projects => {
                            let own: f64 = list
                                .iter()
                                .filter(|a| a.rsrc == rsrc)
                                .map(|a| a.remaining)
                                .sum();
                            let code = list_r
                                .iter()
                                .find(|x| x.id == rsrc)
                                .map(|x| x.code.clone())
                                .unwrap_or_default();
                            // Stored units, as `own` is: not only the days
                            // on show.
                            own + self.other_resource_total(&code, false)
                        }
                        _ => 0.,
                    };
                    let mut band = ResRow::field_line(vec![], (0, fields[0]), &[], total, None);
                    band.band = true;
                    out.push(band);
                    continue;
                };
                let a = &list[k];
                let f = fields[field.min(fields.len() - 1)];
                let one = BTreeSet::from([a.rsrc]);
                let unit = crate::panes::usage_list::material_unit(&list_r, &one);
                let mut values = vec![0.; edges.len()];
                let total = time_fields::assignment_into(p, a, f, &edges, &mut values);
                let mut line =
                    ResRow::field_line(vec![], (field, f), &values, total, unit.as_deref());
                line.assignment = Some(a.row);
                out.push(line);
            }
            out
        })
    }

    pub(super) fn assignments_view(&mut self, host: &mut egui::Ui) {
        let serial = self.project_serial();
        let list = self.memo(serial, "assignments", (), || assignments(&self.project));
        let built = self.assignment_table(&list);
        let shown_keys: Vec<Option<Key>> = built.keys.clone();
        let s = &mut self.views.assignments;
        if s.selected.is_none() {
            s.selected = shown_keys.iter().flatten().next().copied();
        }
        let selection = s.rows.shown(&mut s.selected, &shown_keys);
        let selected = self.views.assignments.selected;
        if self.views.assignments.show_details {
            let tab = self.views.assignments.tab;
            let chosen = match selected {
                Some(Key::Assignment(id)) => list.iter().find(|a| a.id == id).cloned(),
                _ => None,
            };
            let (height, clicked) = self.details_pane(
                host,
                "assignments",
                self.views.assignments.details_height,
                &TABS,
                tab,
                |app, c, size| app.assignment_details(c, size, tab, chosen.as_ref()),
            );
            self.views.assignments.details_height = height;
            if let Some(tab) = clicked {
                self.views.assignments.tab = tab;
            }
        }
        let options = self.label_options();
        let origin = self.project.model.origin_serial;
        let data_date = self.project.model.project.data_date.unwrap_or(0);
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(sheet::MARGIN))
            .show_inside(host, |ui| {
                ui.spacing_mut().item_spacing.y = 0.;
                let (inner, strip) = sheet::table_frame(ui, ui.max_rect());
                let table_w = self.views.assignments.table_width;
                let bar = Rect::from_min_size(strip, Vec2::new(table_w, 17.));
                let caption = format!("Layout: {}", self.views.assignments.layout_name);
                let header = sheet::header_button(ui, "assignments", bar, &caption);
                self.action("assignments_layout", &header);
                let bar_entries = layout_bar_entries(&self.views.assignments);
                let mut bar_key = None;
                egui::Popup::menu(&header)
                    .style(chrome::popup_menu_style)
                    .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                    .show(|ui| {
                        let mut menu = Menu::on(ui, "assignments_layout_menu");
                        bar_key = menu.entries(&bar_entries);
                    });
                if let Some(key) = bar_key {
                    let time = ui.input(|i| i.time);
                    self.assignments_command(key, time);
                }
                let display = Rect::from_min_max(
                    Pos2::new(bar.right() + grid::SPLITTER, bar.top()),
                    Pos2::new(inner.right(), bar.bottom()),
                );
                let response = sheet::header_button(ui, "assignments_display", display, "Display");
                self.action("assignments_display", &response);
                grid::splitter(
                    ui.painter(),
                    Rect::from_min_max(
                        Pos2::new(bar.right(), bar.top()),
                        Pos2::new(display.left(), bar.bottom()),
                    ),
                );
                let body = Rect::from_min_max(Pos2::new(inner.left(), bar.bottom()), inner.max);
                let mut ui = ui.new_child(egui::UiBuilder::new().max_rect(body));
                let ui = &mut ui;
                self.assignments_table(ui, &list, &built, selection, (origin, data_date, options));
            });
        self.assignment_delete_question(&host.ctx().clone());
    }

    /// The table and its spreadsheet.
    fn assignments_table(
        &mut self,
        ui: &mut egui::Ui,
        list: &Rc<Vec<Assignment>>,
        built: &Rc<Built>,
        selection: grid::Selection,
        (origin, data_date, options): (i64, i64, crate::timescale::LabelOptions),
    ) {
        let body = ui.max_rect();
        let time = ui.input(|i| i.time);
        let show_sheet = self.views.assignments.show_sheet;
        let default = ((data_date.div_euclid(DAY) - 1) * DAY) as f64;
        let first = self.views.assignments.first_minute.unwrap_or(default);
        let zoom = self.views.assignments.zoom;
        // A week to spare each side, so a scroll or zoom this frame still
        // finds its days.
        let days = ((body.width() / (zoom as f32 / 7.)).ceil() as i64 + 16).max(21);
        let start = first.floor() as i64 - 7 * DAY;
        let periods = sheet_pane::day_periods(start, start + days * DAY);
        let all_projects =
            self.views.assignments.show_all_projects && !self.open_set.parked.is_empty();
        let mut pane = None;
        if show_sheet {
            self.claim_sheet(SheetHost::Assignments);
            self.panes.sheet_periods = periods.clone();
            let lines = self.assignment_sheet_lines(list, built, &periods, all_projects);
            self.views.assignments.sheet_lines = Some(lines.clone());
            let fields = self.views.assignments.time_fields();
            let value_title = match fields.as_slice() {
                [one] => format!("{} Units", one.label()),
                _ => "Units".to_string(),
            };
            let mut p = SheetPane {
                axis: crate::timescale::TimeScale::new(first, zoom, 0.),
                origin,
                options,
                periods: periods.clone(),
                lines,
                value_title,
                labelled: fields.len() > 1,
                selected_cells: self.panes.sheet_cells.range(&periods),
                timeline: None,
                place: Default::default(),
                drawn: vec![],
            };
            if !self.print.painting {
                p.timeline = Some(crate::timeline::Spec {
                    extent: self.gantt_data_extent(),
                    limits: ZOOM_LIMITS,
                    origin_serial: origin,
                    unit: crate::timescale::Unit::Day,
                    options,
                });
            }
            pane = Some(p);
        } else {
            self.views.assignments.sheet_lines = None;
        }
        let open_menu = self.ctx_menu.take();
        let keys = &built.keys;
        let state = &self.views.assignments;
        let delete = !self.assignments_picked().is_empty();
        let fill = self.view_fill_ready();
        let entries = menu_entries(state, delete, fill);
        let menu = |_: usize| entries.clone();
        let chart_entries = self.resource_sheet_entries();
        let chart_menu = || chart_entries.clone();
        let chart_fn: &dyn Fn() -> Vec<Entry> = &chart_menu;
        let tree = state.tree_field();
        let udfs =
            super::udf_columns::Shown::of(&self.project.document, &self.views.assignments.columns);
        let titles: Vec<&str> = ac::TITLES
            .iter()
            .copied()
            .chain(udfs.titles.iter().map(String::as_str))
            .collect();
        let right: Vec<usize> = ac::right_fields().into_iter().chain(udfs.right()).collect();
        let indicators = udfs.indicators();
        let mut cell_edit = self.take_cell_edit(TABLE);
        // The cells that take a value, by the Resources tab's rules (and
        // User Defined columns through the shared UDF editor).
        let p = &self.project;
        let doc = &p.document;
        let kind = |row: usize, field: usize| {
            let k = built.assignment_of.get(row).copied().flatten()?;
            keys.get(row).copied().flatten()?;
            if let Some(ty) = udfs.field(field) {
                return super::udf_columns::editor(doc, ty);
            }
            let Source::Data(d) = FIELDS.get(field)?.source else {
                return None;
            };
            let a = &list[k];
            let view = p.assignment_view(a.row)?;
            let look = ac::Lookups::of(doc);
            ac::edit_kind(&look, &cells_of(p, a, &view), d)
        };
        let table = grid::Table {
            id: TABLE,
            titles: &titles,
            right: &right,
            tree_field: tree,
            header_height: grid::HEADER_HEIGHT,
            row_lines: true,
            band_pitch: grid::BandPitch::Styled,
            band_caption: &[],
            menu: Some(&menu),
            open_menu,
            chart_menu: pane.is_some().then_some(chart_fn),
        };
        let mut width = self.views.assignments.table_width;
        let width_ref = &mut width;
        let pane_dyn = pane.as_mut().map(|p| p as &mut dyn grid::Pane);
        let painting = self.print.painting;
        let response = grid::show_with(
            ui,
            &table,
            &mut self.views.assignments.columns,
            &built.rows,
            &if painting {
                grid::Selection::default()
            } else {
                selection
            },
            None,
            pane_dyn.map(|p| (p, width_ref)),
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
        self.keep_cell_edit(TABLE, cell_edit);
        self.views.assignments.table_width = width;
        // The cell commands (Fill Down, Find and Replace) read the rows.
        let cell_keys: Vec<Option<CellKey>> = keys
            .iter()
            .map(|k| k.and_then(|k| k.assignment()).map(CellKey::Assignment))
            .collect();
        self.note_view_cells(
            TABLE,
            ac::IDS
                .iter()
                .map(|s| s.to_string())
                .chain(self.views.assignments.columns.extra.iter().cloned())
                .collect(),
            titles.iter().map(|s| s.to_string()).collect(),
            cell_keys,
            built.clone(),
        );
        if let Some((row, field, text)) = response.committed.clone() {
            let key = keys.get(row).copied().flatten();
            if let Some(id) = key.and_then(|k| k.assignment()) {
                if let Some(ty) = udfs.field(field) {
                    self.commit_udf_cell(ty.type_id, id, &text, time);
                } else {
                    self.commit_assignment_cell(id, field, &text, time);
                }
            }
        }
        if let Some(pane) = pane.as_mut() {
            self.assignment_sheet_events(ui, pane, &response, time);
            // The timescale scrolls and zooms like the Gantt's.
            if pane.timeline.is_some() {
                let s = &mut self.views.assignments;
                if (pane.axis.first_minute - first).abs() > 1e-6 {
                    s.first_minute = Some(pane.axis.first_minute);
                }
                s.zoom = pane.axis.pixels_per_week;
                s.timeline_place = pane.place;
                s.timeline_default = default;
                if let Some(r) = self.action_rects.get("assignments_timescale_pan").copied() {
                    self.action_rects.insert("assignments_timescale".into(), r);
                }
            }
        }
        if let Some(i) = response.toggled {
            if let Some(Some(band)) = built.band_of.get(i) {
                let s = &mut self.views.assignments;
                if !s.collapsed.insert(*band) {
                    s.collapsed.remove(band);
                }
            }
        }
        if let Some(i) = response.clicked {
            if let Some(key) = keys.get(i).copied().flatten() {
                let s = &mut self.views.assignments;
                let current = s.selected;
                s.selected = Some(s.rows.click(
                    current.as_ref(),
                    key,
                    response.modifiers,
                    response.secondary,
                ));
            }
        }
        if let Some((_, key)) = response.menu {
            self.assignments_command(key, time);
        }
        if let Some(key) = response.chart_menu {
            self.resource_sheet_command(key, &ui.ctx().clone());
        }
        if built.rows.is_empty() {
            grid::empty_note(
                ui,
                &response,
                TABLE,
                "This project has no resource assignments.",
                &mut self.action_rects,
            );
        }
    }

    /// Clicks on the spreadsheet's period cells: a click selects a cell
    /// (Shift: up to it), a double-click opens its editor, a right-click
    /// outside the selection selects the cell under it first; a click on
    /// the table takes the keyboard from the cells. Then the selected
    /// cell's frame or editor.
    fn assignment_sheet_events(
        &mut self,
        ui: &mut egui::Ui,
        pane: &SheetPane,
        response: &grid::Response,
        time: f64,
    ) {
        let range = self.panes.sheet_cells.range(&pane.periods);
        let mut click = None;
        for (line, r) in &response.rows {
            let pressed = r.clicked() || r.secondary_clicked();
            let Some(pos) = r.interact_pointer_pos().filter(|_| pressed) else {
                continue;
            };
            let Some(n) = pane.period_at(pos.x) else {
                self.panes.sheet_cells.focused = false;
                continue;
            };
            let inside = range
                .as_ref()
                .is_some_and(|(lines, ks)| lines.contains(line) && ks.contains(&n));
            if r.clicked() || !inside {
                let shift = r.clicked() && ui.input(|i| i.modifiers.shift);
                click = Some((*line, pane.periods[n], shift, r.double_clicked()));
            }
        }
        // Each assignment line's cells, named for the scenarios.
        for (line, rect) in &pane.drawn {
            let drawn = pane.lines.get(*line);
            if !drawn.is_some_and(|l| l.assignment.is_some()) {
                continue;
            }
            for (n, q) in pane.periods.iter().enumerate() {
                if let Some(c) = pane.cell(*rect, *q) {
                    self.action_rects.insert(
                        format!("assignments_sheet_cell_{line}_{n}"),
                        [c.left(), c.top(), c.right(), c.bottom()],
                    );
                }
            }
        }
        if let Some((line, period, shift, double)) = click {
            self.sheet_cell_click(line, period, shift, double, time);
        }
        let cursor = self.panes.sheet_cells.cursor;
        let cell = cursor.and_then(|(line, start)| {
            let rect = pane.drawn.iter().find(|(l, _)| *l == line)?.1;
            let period = pane.periods.iter().find(|q| q.0 == start)?;
            pane.cell(rect, *period)
        });
        self.sheet_cell_editor(ui, cell, &chrome::table_font(), time);
    }

    /// Delete on the selected assignments asks first, as observed in the
    /// reference: Yes deletes them as one undo step, No (the default) and
    /// Escape keep them.
    fn assignment_delete_question(&mut self, ctx: &egui::Context) {
        let Some(ids) = self.views.assignments.confirm_delete.clone() else {
            return;
        };
        let buttons = ["Yes", "No"];
        let mut rects = vec![];
        let answer = crate::kit_window::message_box(
            ctx,
            "delete_resource_assignment",
            "Confirmation",
            crate::kit_window::MessageIcon::Question,
            "Delete the selected resource assignments?",
            &buttons,
            crate::kit_window::Question::DeleteResourceAssignment.default_in(&buttons),
            &mut rects,
        );
        for (name, r) in rects {
            self.action_rects
                .insert(name, [r.left(), r.top(), r.right(), r.bottom()]);
        }
        match answer {
            Some(0) => {
                self.views.assignments.confirm_delete = None;
                let time = ctx.input(|i| i.time);
                self.delete_assignments(&ids, time);
            }
            Some(_) => self.views.assignments.confirm_delete = None,
            None if ctx.input(|i| i.key_pressed(egui::Key::Escape)) => {
                self.views.assignments.confirm_delete = None;
            }
            None => {}
        }
    }

    /// Delete the assignments `ids` (TASKRSRC ids) as one undo step.
    fn delete_assignments(&mut self, ids: &[i64], time: f64) {
        let mut rows: Vec<usize> = assignments(&self.project)
            .into_iter()
            .filter(|a| ids.contains(&a.id))
            .map(|a| a.row)
            .collect();
        if rows.is_empty() {
            return;
        }
        // From the last row up, so the rows still to go keep their places.
        rows.sort_unstable_by(|a, b| b.cmp(a));
        let before = self.project.clone();
        let revision = self.records_revision;
        let recalculate = self.detail.calc.recalculate;
        let count = rows.len();
        for row in rows {
            if let Err(e) = self.project.remove_assignment_with(row, recalculate) {
                self.project = before;
                self.status = e;
                return;
            }
        }
        self.edit_step(before, revision, time);
        self.views.assignments.selected = None;
        self.status = if count == 1 {
            "Resource assignment deleted".into()
        } else {
            format!("{count} resource assignments deleted")
        };
    }

    /// The selected assignments (TASKRSRC ids), in display order.
    pub(crate) fn assignments_picked(&self) -> Vec<i64> {
        let s = &self.views.assignments;
        s.rows
            .picked(s.selected.as_ref())
            .into_iter()
            .filter_map(|k| k.assignment())
            .collect()
    }

    /// Delete on the selected assignments: the question first.
    pub(crate) fn assignments_delete(&mut self) {
        let ids = self.assignments_picked();
        if !ids.is_empty() {
            self.views.assignments.confirm_delete = Some(ids);
        }
    }

    /// The activity Add assigns to: the selected assignment's, or the
    /// selected activity band's.
    pub(crate) fn assignments_add_target(&self) -> Option<usize> {
        let list = assignments(&self.project);
        let task = match self.views.assignments.selected {
            Some(Key::Assignment(id)) => list.iter().find(|a| a.id == id).map(|a| a.task),
            Some(Key::Band(Band::Activity(task))) => Some(task),
            _ => None,
        };
        task.filter(|t| *t < self.project.model.activities.len())
    }

    /// Add: the shared Assign Resources window for the selected
    /// assignment's activity (or the selected activity band's).
    pub(crate) fn assignments_add(&mut self) {
        let Some(task) = self.assignments_add_target() else {
            self.status = "Select an assignment or an activity band to add to.".into();
            return;
        };
        let activity = &self.project.model.activities[task];
        if openxer_xer::assignments::is_milestone(activity.task_type) {
            self.status = "Milestones have no resource assignments.".into();
            return;
        }
        self.detail.assign_for = Some(task);
        self.detail.new_resource = None;
        self.detail.replace_from = None;
        use crate::details::assignments::AssignPicker;
        self.detail.assign = Some(AssignPicker::Resource {
            query: String::new(),
            selected: None,
        });
    }

    /// Columns > Customize...: the window's fields from the shared
    /// catalog, plus the project's assignment User Defined columns.
    pub(crate) fn open_assignment_columns(&mut self) {
        let doc = &self.project.document;
        let udfs = super::udf_columns::Shown::of(doc, &self.views.assignments.columns);
        let titles: Vec<String> = ac::TITLES
            .iter()
            .map(|t| t.to_string())
            .chain(udfs.titles)
            .collect();
        let shown: Vec<String> = self
            .views
            .assignments
            .columns
            .order
            .iter()
            .filter_map(|&f| titles.get(f).cloned())
            .collect();
        let defaults: Vec<&str> = ac::default_order()
            .into_iter()
            .map(|f| ac::TITLES[f])
            .collect();
        let choices = super::udf_columns::choices(doc, "assignments");
        self.open_view_columns_with(
            crate::details::grid_columns::ColumnTarget::Assignments,
            "Resource Assignment",
            &ac::TITLES,
            &choices,
            &defaults,
            shown,
        );
    }

    /// Every band of the window's grouping, for Collapse All.
    fn assignment_bands(&self) -> BTreeSet<Band> {
        let list = assignments(&self.project);
        let group = self.views.assignments.group;
        layout(&self.project, &list, group, &BTreeSet::new())
            .into_iter()
            .filter_map(|item| match item {
                Item::Band { band, .. } => Some(band),
                Item::Assignment(_) => None,
            })
            .collect()
    }

    /// A command chosen on the window's menus (the layout options bar and
    /// the table's right-click menu).
    fn assignments_command(&mut self, key: &str, time: f64) {
        let s = &mut self.views.assignments;
        match key {
            "layout_open" => self.open_assignment_layout_dialog(),
            "layout_save" => self.save_assignment_layout(),
            "layout_save_as" => self.open_assignment_save_as(),
            "resource_usage" => s.show_sheet = !s.show_sheet,
            "details" => s.show_details = !s.show_details,
            "timescale" => self.open_timescale_dialog(),
            "options" => {
                self.open_spreadsheet_fields(crate::panes::usage_fields::FieldsView::Assignments)
            }
            "show_all_projects" => s.show_all_projects = !s.show_all_projects,
            "zoom_in" => s.zoom_step(true),
            "zoom_out" => s.zoom_step(false),
            "user_preferences" => self.open_user_preferences(),
            "add" => self.assignments_add(),
            "delete" => self.assignments_delete(),
            "fill_down" => self.view_fill_down(time),
            "columns_default" => s.default_columns(),
            "columns_customize" => self.open_assignment_columns(),
            "expand_all" => s.collapsed.clear(),
            "collapse_all" => {
                let bands = self.assignment_bands();
                self.views.assignments.collapsed = bands;
            }
            _ => {
                if let Some(group) = Group::ALL.into_iter().find(|g| g.menu_key() == key) {
                    s.group = group;
                    s.collapsed.clear();
                }
            }
        }
    }

    /// The change typed or picked into cell `field` of assignment `id`,
    /// through the Resources tab's edit layer calls; one undo step. A
    /// resource, a Manual curve taken off and Cost Units Linked ticked
    /// again go the Resources tab's way (its questions).
    fn commit_assignment_cell(&mut self, id: i64, field: usize, text: &str, time: f64) {
        // A field after the catalog's own (`Columns::extra`) is not an
        // assignment field.
        if self.views.assignments.columns.extra_of(field).is_some() {
            return;
        }
        let Some(Source::Data(d)) = FIELDS.get(field).map(|f| f.source) else {
            return;
        };
        let found = assignments(&self.project).into_iter().find(|a| a.id == id);
        let Some(a) = found else {
            return;
        };
        let Some(view) = self.project.assignment_view(a.row) else {
            return;
        };
        let look = ac::Lookups::of(&self.project.document);
        let change = ac::parse(&look, &cells_of(&self.project, &a, &view), d, text);
        let task_id = self.project.model.activities[a.task].task_id;
        let manual = openxer_xer::manual_units::is_manual(&self.project.document, a.row);
        use crate::details::removals::Removal;
        match change {
            Ok(None) => {}
            Err(e) => self.status = e,
            Ok(Some(ac::Change::Resource(rsrc))) => self.replace_assignment_resource(a.row, rsrc),
            Ok(Some(ac::Change::Relink)) => {
                self.detail.removal = Some(Removal::RelinkCosts { row: a.row });
            }
            Ok(Some(ac::Change::Curve(curve))) if manual => {
                self.detail.removal = Some(Removal::ManualCurve {
                    row: a.row,
                    task_id,
                    curve,
                });
            }
            Ok(Some(change)) => {
                let before = self.project.clone();
                let revision = self.records_revision;
                match self.apply_assignment_change(a.row, change) {
                    Ok(true) => self.edit_step(before, revision, time),
                    Ok(false) => {}
                    Err(e) => {
                        self.project = before;
                        self.status = e;
                    }
                }
            }
        }
    }

    /// One change through the edit layer; whether it changed anything.
    /// The caller makes the undo step.
    fn apply_assignment_change(&mut self, row: usize, change: ac::Change) -> Result<bool, String> {
        match change {
            ac::Change::Edit(edit) => self.project.edit_assignment(row, edit).map(|()| true),
            ac::Change::Primary(on) => self.project.set_primary_resource(row, on).map(|()| true),
            ac::Change::Curve(curve) => openxer_xer::edit::curves::set_assignment_curve(
                &mut self.project.document,
                row,
                curve.as_deref(),
            ),
            ac::Change::Resource(_) | ac::Change::Relink => {
                Err("Pick that value in the cell itself.".into())
            }
        }
    }

    /// Fill Down and Replace in the window (`table_cells`): set field
    /// `field_id` of assignment `id` to `text` when its cell takes a value.
    /// Returns whether it changed; the caller makes the undo step.
    pub(crate) fn set_assignment_cell(
        &mut self,
        id: i64,
        field_id: &str,
        text: &str,
    ) -> Result<bool, String> {
        let refused = || "That column cannot be typed in here.".to_string();
        let field = ac::position(field_id).ok_or_else(refused)?;
        let Source::Data(d) = FIELDS[field].source else {
            return Err(refused());
        };
        let a = assignments(&self.project)
            .into_iter()
            .find(|a| a.id == id)
            .ok_or_else(refused)?;
        let view = self.project.assignment_view(a.row).ok_or_else(refused)?;
        let look = ac::Lookups::of(&self.project.document);
        let cells = cells_of(&self.project, &a, &view);
        if ac::edit_kind(&look, &cells, d).is_none() {
            return Err("That cell does not take a value for this activity.".into());
        }
        let manual = openxer_xer::manual_units::is_manual(&self.project.document, a.row);
        match ac::parse(&look, &cells, d, text)? {
            None => Ok(false),
            Some(ac::Change::Curve(_)) if manual => {
                Err("A Manual curve is taken off in the cell itself.".into())
            }
            Some(change) => self.apply_assignment_change(a.row, change),
        }
    }

    /// One assignment edit from the details tabs, through the same setters
    /// as the Resources tab of Activity Details; one undo step.
    fn assignment_edit(&mut self, row: usize, edit: AssignmentEdit) {
        match self.project.edit_assignment(row, edit) {
            Ok(()) => self.touch_records(),
            Err(e) => self.status = e,
        }
    }

    /// The list window behind the General tab's pickers.
    pub(crate) fn assignment_pick_window(&mut self, ctx: &egui::Context) {
        let Some(pick) = self.views.assignments.pick else {
            return;
        };
        let doc = &self.project.document;
        let list = |table: &str, key: &str, a: &str, b: &str| -> Vec<(i64, String)> {
            doc.table(table).map_or(vec![], |t| {
                t.rows
                    .iter()
                    .map(|r| {
                        (
                            num(t.value(r, key)),
                            format!("{}  {}", t.value(r, a), t.value(r, b)),
                        )
                    })
                    .collect()
            })
        };
        let (title, mut options, clearable) = match pick {
            AssignPick::Resource(_) => (
                "Assign Resource",
                list("RSRC", "rsrc_id", "rsrc_short_name", "rsrc_name"),
                false,
            ),
            AssignPick::Role(_) => (
                "Assign Role",
                list("ROLES", "role_id", "role_short_name", "role_name"),
                true,
            ),
            AssignPick::Account(_) => (
                "Assign Cost Account",
                list("ACCOUNT", "acct_id", "acct_short_name", "acct_name"),
                true,
            ),
        };
        if clearable {
            options.insert(0, (0, "(none)".into()));
        }
        let mut open = true;
        let mut chosen = None;
        let mut close = false;
        crate::chrome::plain_window(title)
            .id(egui::Id::new("assignment_pick"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_size([320., 300.])
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(260.)
                    .show(ui, |ui| {
                        for (i, (id, label)) in options.iter().enumerate() {
                            let response = ui.selectable_label(false, label);
                            self.action(&format!("assignment_pick_item_{i}"), &response);
                            if response.clicked() {
                                chosen = Some(*id);
                            }
                        }
                    });
                ui.add_space(6.);
                let cancel = ui
                    .with_layout(Layout::right_to_left(Align::Min), |ui| {
                        crate::chrome::dialog_button(ui, &crate::chrome::DialogButton::cancel())
                    })
                    .inner;
                self.action("assignment_pick_cancel", &cancel);
                if cancel.clicked() {
                    close = true;
                }
            });
        if let Some(id) = chosen {
            close = true;
            let some = (id != 0).then_some(id);
            match pick {
                AssignPick::Resource(row) => self.replace_assignment_resource(row, id),
                AssignPick::Role(row) => self.assignment_edit(row, AssignmentEdit::Role(some)),
                AssignPick::Account(row) => {
                    self.assignment_edit(row, AssignmentEdit::CostAccount(some))
                }
            }
        }
        if !open || close || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.views.assignments.pick = None;
        }
    }

    fn assignment_details(
        &mut self,
        c: &mut Canvas,
        size: Vec2,
        tab: usize,
        a: Option<&Assignment>,
    ) {
        let (w, h) = (size.x, size.y);
        let p = &self.project;
        match TABS[tab.min(TABS.len() - 1)] {
            "General" => {
                sheet::section(c, 5., 5., w - 10., h - 5., "");
                let wide = (w - 105.).max(200.);
                let half = ((w - 189.) / 2.).max(150.);
                let view = a.and_then(|a| p.assignment_view(a.row));
                let activity = a.map_or(String::new(), |a| {
                    format!(
                        "{} - {}",
                        p.model.activities[a.task].task_code,
                        p.name(a.task)
                    )
                });
                sheet::above(c, 34., 29., "Activity Name", true);
                sheet::picker(
                    c,
                    "assignment_activity",
                    34.,
                    29.,
                    wide,
                    Some(chrome::Icon::Activities),
                    &activity,
                    false,
                );
                let assigned =
                    a.and_then(|a| resources::resources(p).into_iter().find(|r| r.id == a.rsrc));
                let resource = assigned
                    .as_ref()
                    .map_or(String::new(), |r| format!("{} - {}", r.code, r.name));
                // A price is per hour of labor or nonlabor, per unit of
                // measure of a material.
                let per = match &assigned {
                    Some(r) if r.kind == resources::ResourceType::Material => match r.unit.trim() {
                        "" => "unit".to_string(),
                        u => u.to_string(),
                    },
                    _ => "h".to_string(),
                };
                let role = a.map_or(String::new(), |a| {
                    resources::role_names(p)
                        .get(&a.role)
                        .cloned()
                        .unwrap_or_default()
                });
                let account = a.map_or(String::new(), |a| {
                    p.document
                        .table("ACCOUNT")
                        .and_then(|t| {
                            t.rows
                                .iter()
                                .find(|r| t.value(r, "acct_id") == a.account)
                                .map(|r| t.value(r, "acct_name").to_string())
                        })
                        .unwrap_or_default()
                });
                let has = |table: &str| p.document.table(table).is_some_and(|t| !t.rows.is_empty());
                let (has_rsrc, has_roles, has_accounts) =
                    (has("RSRC"), has("ROLES"), has("ACCOUNT"));
                let row = a.map(|a| a.row);
                sheet::above(c, 34., 77., "Resource", true);
                if sheet::picker_button(
                    c,
                    "assignment_resource",
                    34.,
                    77.,
                    half,
                    Some(chrome::Icon::Labor),
                    &resource,
                    row.is_some() && has_rsrc,
                ) {
                    self.views.assignments.pick = row.map(AssignPick::Resource);
                }
                sheet::above(c, half + 118., 77., "Role", true);
                if sheet::picker_button(
                    c,
                    "assignment_role",
                    half + 118.,
                    77.,
                    half,
                    Some(chrome::Icon::Resources),
                    &role,
                    row.is_some() && has_roles,
                ) {
                    self.views.assignments.pick = row.map(AssignPick::Role);
                }
                sheet::above(c, 34., 125., "Cost Account", true);
                if sheet::picker_button(
                    c,
                    "assignment_account",
                    34.,
                    125.,
                    half,
                    Some(chrome::Icon::Expenses),
                    &account,
                    row.is_some() && has_accounts,
                ) {
                    self.views.assignments.pick = row.map(AssignPick::Account);
                }
                // Price, Rate Type and Primary Resource share the row in
                // three equal columns sized to the tab, so none overlaps.
                let third = ((w - 34. - 71. - 2. * 24.) / 3.).max(120.);
                let (rate_x, primary_x) = (34. + third + 24., 34. + 2. * (third + 24.));
                sheet::above(c, 34., 175., "Price / Unit", true);
                let price = view.as_ref().map_or(String::new(), |v| {
                    format!("{}/{per}", format::cost(v.price))
                });
                if let Some(text) = sheet::commit_field(
                    c,
                    "assignment_price",
                    34.,
                    175.,
                    third,
                    &price,
                    view.is_some(),
                    Align::Max,
                ) {
                    match (row, resources::parse_price(&text)) {
                        (Some(row), Some(v)) => {
                            self.assignment_edit(row, AssignmentEdit::PricePerUnit(v))
                        }
                        _ => self.status = "Enter a price of zero or more.".into(),
                    }
                }
                sheet::above(c, rate_x, 175., "Rate Type", true);
                let rate = view.as_ref().map_or(String::new(), |v| v.rate_type.clone());
                let rates: Vec<(&str, &str)> = RATE_TYPES
                    .iter()
                    .map(|(code, label, _)| (*code, *label))
                    .collect();
                if let Some(code) = sheet::combo(
                    c,
                    "assignment_rate_type",
                    rate_x,
                    174.,
                    third,
                    &rates,
                    &rate,
                    view.is_some(),
                ) {
                    if let Some(row) = row {
                        self.assignment_edit(row, AssignmentEdit::RateType(code));
                    }
                }
                let mut primary = view.as_ref().is_some_and(|v| v.primary);
                if c.checkbox(
                    "assignment_primary",
                    primary_x,
                    177.,
                    third,
                    17.,
                    "Primary Resource",
                    &mut primary,
                    view.as_ref().is_some_and(|v| v.rsrc_id.is_some()),
                )
                .clicked()
                {
                    if let Some(row) = row {
                        match self.project.set_primary_resource(row, primary) {
                            Ok(()) => self.touch_records(),
                            Err(e) => self.status = e,
                        }
                    }
                }
            }
            "Planning" => {
                sheet::section(c, 5., 5., w - 10., h - 5., "");
                let pitch = ((w - 65.) / 3.).max(150.);
                let fw = (pitch - 81.).max(100.);
                let view = a.and_then(|a| p.assignment_view(a.row));
                let periods = a.map_or_else(
                    || format::project_periods(p),
                    |a| format::activity_periods(p, a.task),
                );
                let (started, completed) = a.map_or((false, false), |a| {
                    let act = &p.model.activities[a.task];
                    (act.status != StatusType::NotStarted, act.is_completed())
                });
                let text =
                    |f: &dyn Fn(&AssignmentView) -> String| view.as_ref().map_or(String::new(), f);
                // (x, y, caption, value, editable, which)
                let fields: Vec<(f32, f32, &str, String, bool, usize)> = vec![
                    (
                        42.,
                        36.,
                        "Budgeted Units",
                        text(&|v| format::units(v.budgeted.units)),
                        true,
                        0,
                    ),
                    (
                        42. + pitch,
                        36.,
                        "Budgeted Units / Time",
                        text(&|v| format::units_per_time(v.budgeted.per_hour)),
                        true,
                        1,
                    ),
                    (
                        42. + 2. * pitch,
                        36.,
                        "Original Duration",
                        text(&|v| format::duration(v.original_duration, periods)),
                        true,
                        2,
                    ),
                    (
                        42.,
                        84.,
                        "Actual Units",
                        text(&|v| format::units(v.actual_units)),
                        started,
                        3,
                    ),
                    (
                        42.,
                        135.,
                        "Remaining Early Units",
                        text(&|v| format::units(v.remaining.units)),
                        !completed,
                        4,
                    ),
                    (
                        42. + pitch,
                        135.,
                        "Remaining Units / Time",
                        text(&|v| format::units_per_time(v.remaining.per_hour)),
                        !completed,
                        5,
                    ),
                    (
                        42. + 2. * pitch,
                        135.,
                        "Remaining Duration",
                        text(&|v| format::duration(v.remaining_duration, periods)),
                        !completed,
                        6,
                    ),
                ];
                let row = a.map(|a| a.row);
                let mut committed: Option<(usize, String)> = None;
                for (i, (x, y, caption, value, editable, which)) in fields.iter().enumerate() {
                    sheet::above(c, *x, *y, caption, true);
                    if let Some(t) = sheet::commit_field(
                        c,
                        &format!("assignment_planning_{i}"),
                        *x,
                        *y,
                        fw,
                        value,
                        *editable && view.is_some(),
                        Align::Max,
                    ) {
                        committed = Some((*which, t));
                    }
                }
                // Dates: actual start and finish beside Actual Units (started
                // activities), remaining start (not started) and finish below.
                let origin = p.model.origin_serial;
                let data_date = p.model.project.data_date.unwrap_or(0);
                let dates = view.as_ref().map(|v| v.dates).unwrap_or_default();
                let date_fields = [
                    (
                        42. + pitch,
                        84.,
                        "Actual Start",
                        dates.act_start,
                        started,
                        0,
                    ),
                    (
                        42. + 2. * pitch,
                        84.,
                        "Actual Finish",
                        dates.act_finish,
                        started,
                        1,
                    ),
                    (
                        42.,
                        186.,
                        "Remaining Start",
                        dates.rem_start,
                        !started && !completed,
                        2,
                    ),
                    (
                        42. + pitch,
                        186.,
                        "Remaining Finish",
                        dates.rem_finish,
                        !completed,
                        3,
                    ),
                ];
                let mut date_commit: Option<(usize, i64)> = None;
                let mut date_error: Option<String> = None;
                for (n, (x, y, caption, value, enabled, which)) in date_fields.iter().enumerate() {
                    sheet::above(c, *x, *y, caption, true);
                    match sheet::date_field(
                        c,
                        &format!("assignment_planning_date_{n}"),
                        *x,
                        *y,
                        origin,
                        *value,
                        data_date,
                        *enabled && view.is_some(),
                    ) {
                        Ok(Some(Some(t))) => date_commit = Some((*which, t)),
                        Ok(Some(None)) => date_error = Some("Enter a date.".into()),
                        Ok(None) => {}
                        Err(e) => date_error = Some(e),
                    }
                }
                if let (Some((which, t)), Some(row)) = (date_commit, row) {
                    self.assignment_edit(
                        row,
                        match which {
                            0 => AssignmentEdit::ActualStart(t),
                            1 => AssignmentEdit::ActualFinish(t),
                            2 => AssignmentEdit::RemainingStart(t),
                            _ => AssignmentEdit::RemainingFinish(t),
                        },
                    );
                }
                if let Some(e) = date_error {
                    self.status = e;
                }
                if let (Some((which, t)), Some(row), Some(v)) = (committed, row, view) {
                    use crate::details::assignments::{parse_units, parse_units_per_time};
                    let edit = match which {
                        0 => parse_units(&t).map(AssignmentEdit::BudgetedUnits),
                        1 => parse_units_per_time(&t).map(AssignmentEdit::BudgetedUnitsPerTime),
                        2 => format::parse_duration(&t, periods)
                            .ok()
                            .map(AssignmentEdit::OriginalDuration),
                        3 => parse_units(&t).map(|u| {
                            AssignmentEdit::ActualRegularUnits(
                                (u - v.actual_overtime_units).max(0.),
                            )
                        }),
                        4 => parse_units(&t).map(AssignmentEdit::RemainingUnits),
                        5 => parse_units_per_time(&t).map(AssignmentEdit::RemainingUnitsPerTime),
                        _ => format::parse_duration(&t, periods)
                            .ok()
                            .map(AssignmentEdit::RemainingDuration),
                    };
                    match edit {
                        Some(edit) => self.assignment_edit(row, edit),
                        None => self.status = format!("Cannot read {t:?}."),
                    }
                }
            }
            "User Defined Fields" => {
                let cols = [
                    ("User Defined Field", 200., false),
                    ("Data Type", 200., false),
                    ("Value", 200., false),
                ];
                sheet::table(
                    c,
                    "assignment_udf",
                    1.,
                    1.,
                    w - 2.,
                    h - 34.,
                    &cols,
                    &[],
                    None,
                );
                c.button(
                    "assignment_udf_customize",
                    20.,
                    h - 27.,
                    200.,
                    21.,
                    "Customize User Defined Fields...",
                    false,
                    false,
                );
                let mut hide = false;
                c.checkbox(
                    "assignment_udf_hide",
                    300.,
                    h - 27.,
                    300.,
                    21.,
                    "Hide empty rows",
                    &mut hide,
                    false,
                );
            }
            _ => self.assignment_codes_tab(c, w, h, a),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assignments_group_under_bands_and_spread_remaining_units() {
        let p = resources::tests::with_resource();
        let list = assignments(&p);
        assert_eq!(list.len(), 1);
        let items = layout(&p, &list, Group::Resource, &BTreeSet::new());
        assert_eq!(
            items,
            vec![
                Item::Band {
                    band: Band::Resource(7),
                    caption: "Pipefitter".into(),
                    open: true,
                },
                Item::Assignment(0),
            ]
        );
        // Collapsing the band hides its assignments.
        let collapsed = BTreeSet::from([Band::Resource(7)]);
        assert_eq!(layout(&p, &list, Group::Resource, &collapsed).len(), 1);
        // Grouped by role: the assignment has none.
        match &layout(&p, &list, Group::Role, &BTreeSet::new())[0] {
            Item::Band { caption, .. } => assert_eq!(caption, "No Role"),
            other => panic!("{other:?}"),
        }
        // Grouped by activity: the band is the activity.
        let task = list[0].task;
        match &layout(&p, &list, Group::Activity, &BTreeSet::new())[0] {
            Item::Band { band, .. } => assert_eq!(*band, Band::Activity(task)),
            other => panic!("{other:?}"),
        }
        // 24 remaining hours spread over the activity's work days.
        let a = &list[0];
        let act = &p.model.activities[a.task];
        let start = act.early_start.unwrap();
        let periods = sheet_pane::day_periods(start - DAY, act.early_finish.unwrap() + DAY);
        let values = remaining_spread(&p, a, &periods);
        let total: f64 = values.iter().sum();
        assert!((total - 24.).abs() < 1e-6, "{values:?}");
        assert!(values.iter().filter(|v| **v > 0.).count() >= 3);
    }

    #[test]
    fn layouts_keep_the_grouping_fields_and_columns() {
        let mut s = AssignmentsState {
            group: Group::CostAccount,
            show_sheet: false,
            time_field_ids: vec!["actual".into(), "remaining".into()],
            ..Default::default()
        };
        s.columns.order = vec![0, 2, 9];
        s.columns.widths[9] = 130.;
        let mut again = AssignmentsState::default();
        again.restore(&s.json());
        assert_eq!(again.group, Group::CostAccount);
        assert!(!again.show_sheet);
        assert_eq!(again.time_field_ids, ["actual", "remaining"]);
        assert_eq!(again.columns.order, [0, 2, 9]);
        assert_eq!(again.columns.widths[9], 130.);
        // A layout saved before the catalog (two fields) still opens.
        let mut old = AssignmentsState::default();
        let value = serde_json::json!({"columns": {"order": [1, 0], "widths": [90., 160.]}});
        old.restore(&value);
        assert_eq!(old.columns.order, [1, 0]);
        assert_eq!(old.columns.widths[..2], [90., 160.]);
        assert_eq!(old.group, Group::Resource);
    }
}
