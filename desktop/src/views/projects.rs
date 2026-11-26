//! Projects view: the workspace projects grouped under their EPS nodes, with
//! dates and activity counts, and the Project Details pane.
//!
//! XER files carry no EPS names (a project node only points at an EPS id),
//! so the EPS and each project's node are workspace data (Enterprise >
//! Project Folders, the EPS); a project not yet placed sits under the
//! first top-level node, "Workspace" until the EPS is edited. Cut and Paste
//! move a project to another node.
//! Summaries of closed projects are read from their stored XER text and
//! cached by save stamp; the open project is summarized from live data.
use super::grid::{self, Columns, Row};
use super::wbs::{project_name, project_node};
use super::*;
use super::{chart, sheet};
use crate::context_menu::{Entry, Spec};
use crate::schedule_dialogs::Canvas;
use openxer_xer::edit::notebooks::Owner;
use openxer_xer::edit::project_codes;
use openxer_xer::parse_absolute;

use super::project_rollups::{self as rollups, Group as Grouping, Sort, TITLES, WIDTHS};
use openxer_xer::workspace::rollup::{self, Rollup};
pub const TABS: [&str; 12] = [
    "General",
    "Notebook",
    "Budget Log",
    "Spending Plan",
    "Budget Summary",
    "Dates",
    "Funding",
    "Codes",
    "Defaults",
    "Resources",
    "Settings",
    "Calculations",
];
pub use openxer_xer::workspace::{EPS_ID, EPS_NAME};
/// Table width to the splitter, as measured.
const TABLE_WIDTH: f32 = 842.;

const DURATION_TYPES: [(&str, &str); 4] = [
    ("DT_FixedDUR2", "Fixed Duration & Units"),
    ("DT_FixedDrtn", "Fixed Duration & Units/Time"),
    ("DT_FixedQty", "Fixed Units"),
    ("DT_FixedRate", "Fixed Units/Time"),
];
const PERCENT_TYPES: [(&str, &str); 3] = [
    ("CP_Phys", "Physical"),
    ("CP_Drtn", "Duration"),
    ("CP_Units", "Units"),
];
const ACTIVITY_TYPES: [(&str, &str); 6] = [
    ("TT_Task", "Task Dependent"),
    ("TT_Rsrc", "Resource Dependent"),
    ("TT_LOE", "Level of Effort"),
    ("TT_Mile", "Start Milestone"),
    ("TT_FinMile", "Finish Milestone"),
    ("TT_WBS", "WBS Summary"),
];
pub const STATUSES: [(&str, &str); 5] = [
    ("WS_Planned", "Planned"),
    ("WS_Open", "Active"),
    ("WS_Inactive", "Inactive"),
    ("WS_Whatif", "What-If"),
    ("WS_Requested", "Requested"),
];
const RATE_TYPES: [(&str, &str); 5] = [
    ("COST_PER_QTY", "Price / Unit"),
    ("COST_PER_QTY2", "Price / Unit2"),
    ("COST_PER_QTY3", "Price / Unit3"),
    ("COST_PER_QTY4", "Price / Unit4"),
    ("COST_PER_QTY5", "Price / Unit5"),
];
const MONTHS: [(&str, &str); 12] = [
    ("1", "January"),
    ("2", "February"),
    ("3", "March"),
    ("4", "April"),
    ("5", "May"),
    ("6", "June"),
    ("7", "July"),
    ("8", "August"),
    ("9", "September"),
    ("10", "October"),
    ("11", "November"),
    ("12", "December"),
];

/// What the Projects right-click menu reads of the row under the pointer
/// and of the view, as plain data.
#[derive(Clone, Copy, Debug, Default)]
pub struct ProjectsMenu {
    /// The row is a project, not an EPS node.
    pub on_project: bool,
    /// The project row is currently open (Close Project applies).
    pub is_open: bool,
    /// Add and Paste have an EPS node to go under.
    pub can_add: bool,
    /// The project is kept in the workspace (Cut and Copy apply).
    pub stored: bool,
    /// The project is kept in the workspace, or the row is an EPS node.
    pub deletable: bool,
    /// A project was cut or copied.
    pub clipboard: bool,
    /// The details pane is shown.
    pub details: bool,
    /// Group and Sort By's choice.
    pub group: Grouping,
    /// Filter By: only the open projects, only the projects of one status
    /// (index into `STATUSES`), every status but What-If.
    pub open_only: bool,
    pub only_status: Option<usize>,
    pub non_whatif: bool,
    /// Expand All and Collapse All apply.
    pub any_collapsed: bool,
    pub any_open: bool,
    /// Fill Down applies (several rows selected, Project Name focused).
    pub fill: bool,
}

/// The Project Details check item that heads both Projects menus.
fn details_item(shown: bool) -> Entry {
    Entry::Item(
        Spec::new("details", "Project Details")
            .icon(chrome::Icon::Details)
            .checked(shown),
    )
}

/// The Projects chart menu (a bar, the empty chart area, the timescale).
pub fn projects_chart_menu(details: bool) -> Vec<Entry> {
    vec![details_item(details)]
}

/// The Projects table menu, for an EPS row, a project row, the empty area
/// and a column title, in the reference's order. Fill Down is enabled on
/// the Project Name column with several rows selected and greyed
/// otherwise, as in the reference. Edit Project Details is OpenXER's own.
pub fn projects_menu(m: &ProjectsMenu) -> Vec<Entry> {
    use chrome::Icon;
    let item = |key, label, shortcut, icon: Icon, enabled| {
        Entry::Item(
            Spec::new(key, label)
                .key(shortcut)
                .icon(icon)
                .enabled(enabled),
        )
    };
    vec![
        // No Ctrl+O text: here Ctrl+O is File > Open..., not this command.
        item(
            "open_project",
            "Open Project",
            "",
            Icon::ProjectOpen,
            m.on_project,
        ),
        item(
            "close_project",
            "Close Project",
            "",
            Icon::ProjectClosed,
            m.is_open,
        ),
        details_item(m.details),
        Entry::Item(Spec::new("edit_details", "Edit Project Details").enabled(m.on_project)),
        Entry::Separator,
        item("cut", "Cut", "Ctrl+X", Icon::Cut, m.stored),
        item("copy", "Copy", "Ctrl+C", Icon::Copy, m.stored),
        item(
            "paste",
            "Paste",
            "Ctrl+V",
            Icon::Paste,
            m.clipboard && m.can_add,
        ),
        item("fill_down", "Fill Down", "Ctrl+E", Icon::FillDown, m.fill),
        Entry::Separator,
        item("add", "Add", "Ins", Icon::Add, m.can_add),
        item("delete", "Delete", "Del", Icon::Delete, m.deletable),
        Entry::Separator,
        Entry::Submenu(
            Spec::new("columns", "Columns").icon(Icon::Columns),
            vec![Entry::Item(
                Spec::new("customize", "Customize...").icon(Icon::Columns),
            )],
        ),
        icon_item("table_font", "Table Font and Row...", Icon::TableFont),
        Entry::Separator,
        Entry::Submenu(
            Spec::new("filter_by", "Filter By").icon(Icon::Filter),
            filter_entries(m),
        ),
        Entry::Submenu(
            Spec::new("group_by", "Group and Sort By").icon(Icon::GroupSort),
            Grouping::MENU
                .iter()
                .map(|&(g, key, caption)| check(key, caption, m.group == g))
                .collect(),
        ),
        Entry::Separator,
        item(
            "expand_all",
            "Expand All",
            "",
            Icon::ExpandAll,
            m.any_collapsed,
        ),
        item(
            "collapse_all",
            "Collapse All",
            "",
            Icon::CollapseAll,
            m.any_open,
        ),
        icon_item("collapse_to", "Collapse To...", Icon::CollapseTo),
    ]
}

/// A check item of a submenu.
pub(crate) fn check(key: &'static str, caption: &'static str, on: bool) -> Entry {
    Entry::Item(Spec::new(key, caption).checked(on))
}

/// A menu item with its icon.
pub(crate) fn icon_item(key: &'static str, caption: &'static str, icon: chrome::Icon) -> Entry {
    Entry::Item(Spec::new(key, caption).icon(icon))
}

/// The status choices of Filter By > Status: (menu key, caption, status
/// code); Non What-if shows every status but What-If.
pub const STATUS_FILTERS: [(&str, &str, &str); 4] = [
    ("filter_active", "Active", "WS_Open"),
    ("filter_inactive", "Inactive", "WS_Inactive"),
    ("filter_planned", "Planned", "WS_Planned"),
    ("filter_whatif", "What-if", "WS_Whatif"),
];

/// Filter By: All, Currently Open and the Status choices, the one in use
/// checked.
fn filter_entries(m: &ProjectsMenu) -> Vec<Entry> {
    let all = !m.open_only && m.only_status.is_none() && !m.non_whatif;
    let mut status: Vec<Entry> = STATUS_FILTERS
        .iter()
        .map(|&(key, caption, code)| {
            let on = m
                .only_status
                .and_then(|k| STATUSES.get(k))
                .is_some_and(|s| s.0 == code);
            check(key, caption, on)
        })
        .collect();
    status.push(check("filter_non_whatif", "Non What-if", m.non_whatif));
    vec![
        check("filter_all", "All", all),
        check("filter_open", "Currently Open", m.open_only),
        Entry::Submenu(Spec::new("filter_status", "Status"), status),
    ]
}

/// One workspace project. Dates are absolute minutes from 1899-12-30.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProjectSummary {
    pub store_id: Option<i64>,
    pub source: String,
    pub key: i64,
    pub id: String,
    pub name: String,
    pub data_date: Option<i64>,
    pub planned_start: Option<i64>,
    pub start: Option<i64>,
    pub finish: Option<i64>,
    pub must_finish: Option<i64>,
    pub actual_start: Option<i64>,
    pub actual_finish: Option<i64>,
    pub activities: usize,
    pub project: BTreeMap<String, String>,
    pub node: BTreeMap<String, String>,
    pub current: bool,
    /// Dates, float, Schedule % Complete, units, costs and counts.
    pub rollup: Rollup,
    /// The project's stored user defined field values, for the User Defined
    /// columns (`udf_columns::raw_of`).
    pub udf: super::udf_columns::Raw,
    /// The responsible manager's OBS name.
    pub manager: String,
    /// The project node's anticipated start and finish.
    pub anticipated: (Option<i64>, Option<i64>),
    /// Original, current and proposed budget (`None`: an EPS row).
    pub budget: Option<[f64; 3]>,
    /// The spending plan's total spending and total benefit.
    pub plan: Option<[f64; 2]>,
    /// Project code values by code type (`project_rollups::code_values`).
    pub codes: BTreeMap<i64, String>,
}

/// The values of project `key` read from its node and its tables in `doc`:
/// manager, anticipated dates, budgets, spending plan and project codes.
fn fill_details(s: &mut ProjectSummary, doc: &Document, key: i64) {
    let obs = s.node.get("obs_id").cloned().unwrap_or_default();
    s.manager = doc.table("OBS").map_or(String::new(), |t| {
        t.rows
            .iter()
            .find(|r| t.value(r, "obs_id") == obs)
            .map_or(String::new(), |r| t.value(r, "obs_name").to_string())
    });
    s.anticipated = (
        date(&s.node, "anticip_start_date"),
        date(&s.node, "anticip_end_date"),
    );
    let node = s.node.get("wbs_id").map_or(0, |v| num(v));
    let original = super::budget::stored_amount(s.node.get("orig_cost").map_or("", String::as_str));
    let (orig, current, proposed) = super::budget::node_budget(doc, key, node, original);
    s.budget = Some([orig, current, proposed]);
    let (spend, benefit) = super::budget::plan_totals(doc, key, node);
    s.plan = Some([spend, benefit]);
    s.codes = rollups::code_values(doc, key);
}

fn row_map(table: &openxer_xer::Table, row: &[String]) -> BTreeMap<String, String> {
    table
        .fields
        .iter()
        .enumerate()
        .map(|(i, f)| {
            (
                f.to_ascii_lowercase(),
                row.get(i).cloned().unwrap_or_default(),
            )
        })
        .collect()
}

fn num(s: &str) -> i64 {
    s.trim().parse().unwrap_or(0)
}

fn project_maps(doc: &Document, key: i64) -> (BTreeMap<String, String>, BTreeMap<String, String>) {
    let project = doc.table("PROJECT").and_then(|t| {
        t.rows
            .iter()
            .find(|r| num(t.value(r, "proj_id")) == key)
            .map(|r| row_map(t, r))
    });
    let node = doc.table("PROJWBS").and_then(|t| {
        t.rows
            .iter()
            .find(|r| num(t.value(r, "proj_id")) == key && t.value(r, "proj_node_flag") == "Y")
            .map(|r| row_map(t, r))
    });
    (project.unwrap_or_default(), node.unwrap_or_default())
}

fn date(map: &BTreeMap<String, String>, key: &str) -> Option<i64> {
    map.get(key).and_then(|v| parse_absolute(v))
}

/// Summary of a stored project from its XER text alone.
pub fn summarize(doc: &Document, key: i64) -> Option<ProjectSummary> {
    let (project, node) = project_maps(doc, key);
    if project.is_empty() {
        return None;
    }
    let mut s = ProjectSummary {
        key,
        id: project.get("proj_short_name").cloned().unwrap_or_default(),
        data_date: date(&project, "last_recalc_date").or(date(&project, "plan_start_date")),
        planned_start: date(&project, "plan_start_date"),
        must_finish: date(&project, "plan_end_date"),
        ..Default::default()
    };
    s.name = node
        .get("wbs_name")
        .filter(|n| !n.is_empty())
        .cloned()
        .unwrap_or_else(|| s.id.clone());
    let mut all_complete = true;
    if let Some(t) = doc.table("TASK") {
        for r in t.rows.iter().filter(|r| num(t.value(r, "proj_id")) == key) {
            s.activities += 1;
            let actual_start = parse_absolute(t.value(r, "act_start_date"));
            let actual_finish = parse_absolute(t.value(r, "act_end_date"));
            let start = actual_start
                .or(parse_absolute(t.value(r, "early_start_date")))
                .or(parse_absolute(t.value(r, "target_start_date")));
            let finish = actual_finish
                .or(parse_absolute(t.value(r, "early_end_date")))
                .or(parse_absolute(t.value(r, "target_end_date")));
            s.start = min_opt(s.start, start);
            s.finish = s.finish.max(finish);
            s.actual_start = min_opt(s.actual_start, actual_start);
            s.actual_finish = s.actual_finish.max(actual_finish);
            all_complete &= t.value(r, "status_code") == "TK_Complete";
        }
    }
    if !all_complete || s.activities == 0 {
        s.actual_finish = None;
    }
    s.finish = s.finish.or(date(&project, "scd_end_date"));
    s.start = s.start.or(s.planned_start);
    s.project = project;
    s.node = node;
    s.rollup = rollup::of_document(doc, key).unwrap_or_default();
    s.udf = super::udf_columns::raw_of(doc, key);
    fill_details(&mut s, doc, key);
    Some(s)
}

/// The open project's activities for its rollup, from the live model.
fn live_facts(p: &ProjectData) -> Vec<rollup::ActivityFacts> {
    let m = &p.model;
    let base = m.origin_serial * DAY;
    let abs = |v: Option<i64>| v.map(|v| v + base);
    let leveled = m.options.do_level;
    m.activities
        .iter()
        .map(|a| rollup::ActivityFacts {
            completed: a.is_completed(),
            started: a.status == openxer_engine::model::StatusType::InProgress,
            start: abs(displayed_start(a, leveled)),
            finish: abs(displayed_finish(a, leveled)),
            late_start: abs(a.late_start),
            late_finish: abs(a.late_finish),
            actual_start: abs(a.act_start),
            actual_finish: abs(a.act_finish),
            remaining_start: abs(a.restart.or(a.early_start).or(a.target_start)),
            remaining_finish: abs(a.reend.or(a.early_finish).or(a.target_finish)),
            total_float: a.total_float,
        })
        .collect()
}

/// Summary of the open project from the live model, so unsaved edits show.
pub fn summarize_live(p: &ProjectData) -> ProjectSummary {
    let m = &p.model;
    let base = m.origin_serial * DAY;
    let abs = |v: Option<i64>| v.map(|v| v + base);
    let leveled = m.options.do_level;
    let (project, node) = project_maps(&p.document, m.project.proj_id);
    let all_complete = !m.activities.is_empty() && m.activities.iter().all(|a| a.is_completed());
    let mut s = ProjectSummary {
        store_id: None,
        source: p.source_name.clone(),
        key: m.project.proj_id,
        id: m.project.name.clone(),
        name: project_name(p),
        data_date: abs(m.project.data_date),
        planned_start: abs(m.project.plan_start),
        start: abs(m
            .activities
            .iter()
            .filter_map(|a| displayed_start(a, leveled))
            .min()
            .or(m.project.plan_start)),
        finish: abs(m
            .activities
            .iter()
            .filter_map(|a| displayed_finish(a, leveled))
            .max()
            .or(m.project.scd_end)),
        must_finish: abs(m.project.plan_end),
        actual_start: abs(m.activities.iter().filter_map(|a| a.act_start).min()),
        actual_finish: if all_complete {
            abs(m.activities.iter().filter_map(|a| a.act_finish).max())
        } else {
            None
        },
        activities: m.activities.len(),
        project,
        node,
        current: true,
        udf: super::udf_columns::raw_of(&p.document, m.project.proj_id),
        rollup: {
            let (units, costs) = rollup::amounts(&p.document, m.project.proj_id);
            Rollup::of_project(
                &live_facts(p),
                abs(m.project.data_date.or(m.project.plan_start)),
                abs(m.project.plan_end),
                abs(m.project.plan_start),
                abs(m.project.scd_end),
                rollup::project_calendar(&p.document, m.project.proj_id),
                units,
                costs,
            )
        },
        ..Default::default()
    };
    fill_details(&mut s, &p.document, m.project.proj_id);
    s
}

fn min_opt(a: Option<i64>, b: Option<i64>) -> Option<i64> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, None) => a,
        (None, b) => b,
    }
}

/// The EPS row's rollup over its projects.
pub fn eps_rollup(projects: &[ProjectSummary]) -> ProjectSummary {
    let refs: Vec<&ProjectSummary> = projects.iter().collect();
    rollups::folder_summary(EPS_ID.into(), EPS_NAME.into(), &refs)
}

#[cfg(test)]
fn label(options: &[(&str, &'static str)], code: &str) -> String {
    options
        .iter()
        .find(|(k, _)| *k == code)
        .map_or(code.to_string(), |(_, v)| v.to_string())
}

pub struct ProjectsState {
    pub columns: Columns,
    /// Selected project by (source name, project key); `None` selects an EPS row.
    pub selected: Option<(String, i64)>,
    /// The EPS row selected when no project is (the first one when `None`).
    pub selected_eps: Option<i64>,
    /// The row selection: Ctrl and Shift select several rows, as in the
    /// reference. Fill Down works on them; the other commands act on the
    /// current row.
    pub rows: super::grid::RowSelection<ProjectRow>,
    /// The project rows as last drawn, for the cell commands
    /// (`views::table_cells`): each row's key and whether it is the
    /// current project.
    pub cell_rows: Vec<(ProjectRow, bool)>,
    /// Collapsed EPS rows.
    pub collapsed: BTreeSet<i64>,
    /// The project waiting to be pasted under another EPS node.
    pub cut: Option<i64>,
    /// The project waiting to be pasted as a copy.
    pub copy: Option<i64>,
    pub copy_dialog: Option<super::project_ops::CopyDialog>,
    pub delete: Option<super::project_ops::DeleteConfirm>,
    /// A Delete of an EPS node waiting for its confirmation.
    pub folder_delete: Option<super::project_ops::FolderDeleteConfirm>,
    /// An Edit toolbar command for this view, run on its next draw.
    pub command: Option<&'static str>,
    /// Whether Add, Delete, Cut, Copy and Paste apply, as the view worked
    /// them out on its last draw (the open project cannot be deleted, Add
    /// and Paste need a folder); the Edit toolbar and menu follow them.
    pub edit_enabled: Option<[bool; 5]>,
    /// EPS ID and name of the selected EPS row, for the details pane.
    eps_label: Option<(String, String)>,
    pub tab: usize,
    pub show_details: bool,
    pub details_height: f32,
    pub table_width: f32,
    /// Selected notebook topic row on the Notebook tab.
    pub topic: usize,
    /// Selected row of the Codes tab.
    pub code_row: Option<usize>,
    /// Selected row of the Funding tab.
    pub funding_row: Option<usize>,
    cache: BTreeMap<i64, Cached>,
    /// The shared dictionary the cached rollups were made with, and its
    /// revision (schema 3).
    dictionary: Option<(i64, Document)>,
    /// The chart's first minute once scrolled (None: the week before the
    /// first bar), its zoom in pixels a week, where it was drawn and its
    /// default first minute.
    pub timeline_first: Option<f64>,
    pub zoom: f64,
    pub timeline_place: crate::timeline::Place,
    pub timeline_default: f64,
    /// Projects sorted on a column inside each group (`None`: by Project ID).
    pub sort: Option<Sort>,
    pub group: Grouping,
    /// Project status codes shown; empty shows every project.
    pub status_filter: BTreeSet<String>,
    /// Filter By > Currently Open: only the open projects.
    pub open_only: bool,
    /// Table Font and Row: the table's font, cell colour and row height.
    pub table: crate::grid::layout::TableDisplay,
    pub row_height: f32,
    /// The bands as last drawn (id, depth), for Collapse To.
    pub bands: Vec<(i64, usize)>,
    /// Start and finish of stored baselines by row id, with the stamp they
    /// were read at.
    baselines: BTreeMap<i64, (String, Option<i64>, Option<i64>)>,
}

struct Cached {
    stamp: String,
    source: String,
    key: i64,
    summary: Option<ProjectSummary>,
    /// The shared dictionary rows the rollup was made with (schema 3).
    uses: BTreeSet<(String, i64)>,
}

impl ProjectsState {
    /// Whether workspace project `id`'s rollup is cached.
    #[cfg(test)]
    pub fn is_cached(&self, id: i64) -> bool {
        self.cache.get(&id).is_some_and(|c| c.summary.is_some())
    }
}

impl Default for ProjectsState {
    fn default() -> Self {
        let mut columns = Columns::new(&WIDTHS);
        columns.order = rollups::default_order();
        Self {
            columns,
            selected: None,
            selected_eps: None,
            rows: Default::default(),
            cell_rows: vec![],
            collapsed: BTreeSet::new(),
            cut: None,
            copy: None,
            copy_dialog: None,
            delete: None,
            folder_delete: None,
            command: None,
            edit_enabled: None,
            eps_label: None,
            tab: 0,
            show_details: true,
            details_height: 560.,
            table_width: TABLE_WIDTH,
            topic: 0,
            code_row: None,
            funding_row: None,
            cache: BTreeMap::new(),
            dictionary: None,
            timeline_first: None,
            zoom: chart::WEEK_PITCH,
            timeline_place: Default::default(),
            timeline_default: 0.,
            sort: None,
            group: Grouping::Eps,
            status_filter: BTreeSet::new(),
            open_only: false,
            table: Default::default(),
            row_height: grid::ROW_HEIGHT,
            bands: vec![],
            baselines: BTreeMap::new(),
        }
    }
}

impl ProjectsState {
    pub fn invalidate(&mut self) {
        self.cache.clear();
        self.baselines.clear();
    }
    pub fn json(&self) -> serde_json::Value {
        serde_json::json!({"columns": self.columns.json(), "tab": self.tab,
            "show_details": self.show_details, "details_height": self.details_height,
            "table_width": self.table_width, "zoom": self.zoom,
            "sort": self.sort.map(|s| serde_json::json!({"field": s.field, "descending": s.descending})),
            "group": self.group.key(),
            "status_filter": self.status_filter, "open_only": self.open_only,
            "table": crate::grid::layout::table_to_json(&self.table),
            "row_height": self.row_height})
    }
    pub fn restore(&mut self, value: &serde_json::Value) {
        // A layout saved before the rollup columns has four columns and no
        // widths for the new ones; it keeps its own choice of columns.
        self.columns.restore(&value["columns"]);
        // A User Defined column counts after the table's own.
        let fields = self.columns.widths.len();
        self.sort = value["sort"]["field"]
            .as_u64()
            .map(|f| f as usize)
            .filter(|f| *f < fields)
            .map(|field| Sort {
                field,
                descending: value["sort"]["descending"].as_bool().unwrap_or(false),
            });
        if let Some(g) = value["group"].as_str() {
            self.group = Grouping::from_key(g).unwrap_or(Grouping::Eps);
        }
        self.open_only = value["open_only"].as_bool().unwrap_or(false);
        if let Ok(table) = crate::grid::layout::table_from_json(&value["table"]) {
            self.table = table;
        }
        if let Some(h) = value["row_height"]
            .as_f64()
            .filter(|h| (17.0..=48.).contains(h))
        {
            self.row_height = h as f32;
        }
        if let Some(list) = value["status_filter"].as_array() {
            self.status_filter = list
                .iter()
                .filter_map(|v| v.as_str())
                .filter(|c| STATUSES.iter().any(|(k, _)| k == c))
                .map(String::from)
                .collect();
        }
        if let Some(tab) = value["tab"].as_u64().filter(|t| (*t as usize) < TABS.len()) {
            self.tab = tab as usize;
        }
        if let Some(show) = value["show_details"].as_bool() {
            self.show_details = show;
        }
        restore_height(&value["details_height"], &mut self.details_height);
        restore_width(&value["table_width"], &mut self.table_width);
        chart::restore_zoom(&value["zoom"], &mut self.zoom);
    }
}

/// Text of a PROJECT or PROJWBS field in a summary map.
fn get<'a>(map: &'a BTreeMap<String, String>, key: &str) -> &'a str {
    map.get(key).map_or("", String::as_str)
}

fn flag(map: &BTreeMap<String, String>, key: &str) -> bool {
    get(map, key) == "Y"
}

fn yn(value: bool) -> String {
    if value { "Y" } else { "N" }.into()
}

/// A Project Details value as OpenXER uses it: the stored value, or what
/// OpenXER takes where the project holds none, so a tab never shows a blank
/// box for a value that is in use.
fn effective(s: &ProjectSummary, field: &str) -> String {
    let read = |c: &str| s.project.get(c).map(|v| v.trim().to_string());
    // Defaults tab: what a new activity takes.
    if let Some(v) = openxer_xer::edit::new_activity::effective_default(field, &read) {
        return v;
    }
    let stored = get(&s.project, field).trim();
    if !stored.is_empty() {
        return stored.to_string();
    }
    match field {
        // New assignments take the first rate type and drive the dates
        // (`xer::assignments`), and a resource may be assigned twice.
        "def_rate_type" => "COST_PER_QTY",
        "def_rollup_dates_flag" | "rsrc_multi_assign_flag" => "Y",
        // WBS code paths join with a full stop (`xer::edit::wbs::separator`).
        "name_sep_char" => ".",
        // The level a new project summarizes to.
        "wbs_max_sum_level" => "2",
        _ => "",
    }
    .into()
}

impl App {
    /// Workspace projects ordered by Project ID, with the open one live.
    pub(crate) fn workspace_projects(&mut self) -> Vec<ProjectSummary> {
        let stamps = self.store.stamps().unwrap_or_default();
        self.drop_stale_rollups();
        let shared = self.dictionary_shared();
        let has_open = !self.open_set.empty;
        let live = has_open.then(|| summarize_live(&self.project));
        let is_live = |c: &Cached| {
            live.as_ref()
                .is_some_and(|l| c.source == l.source && c.key == l.key)
        };
        let mut out = vec![];
        let mut found_current = false;
        let cache = &mut self.views.projects.cache;
        cache.retain(|id, _| stamps.iter().any(|(s, _)| s == id));
        for (id, stamp) in stamps {
            let fresh = cache
                .get(&id)
                .is_some_and(|c| c.stamp == stamp && (c.summary.is_some() || is_live(c)));
            if !fresh {
                let Ok((source, key, _)) = self.store.source(id) else {
                    continue;
                };
                let mut entry = Cached {
                    stamp,
                    source,
                    key,
                    summary: None,
                    uses: BTreeSet::new(),
                };
                // The open project is summarized live and parsed only once closed.
                if !is_live(&entry) {
                    // A schema 3 workspace keeps calendars and resources in
                    // its dictionary; the rollup needs them back.
                    let doc = self.store.source_document(id).ok().map(|(_, _, d)| d);
                    if let Some(doc) = doc.as_ref().filter(|_| shared) {
                        entry.uses = openxer_xer::workspace_scope::used_rows(doc, key);
                    }
                    entry.summary =
                        doc.as_ref()
                            .and_then(|doc| summarize(doc, key))
                            .map(|s| ProjectSummary {
                                source: entry.source.clone(),
                                ..s
                            });
                }
                cache.insert(id, entry);
            }
            let entry = &cache[&id];
            if is_live(entry) {
                found_current = true;
                if let Some(live) = live.as_ref() {
                    out.push(ProjectSummary {
                        store_id: Some(id),
                        ..live.clone()
                    });
                }
            } else if let Some(s) = &entry.summary {
                out.push(ProjectSummary {
                    store_id: Some(id),
                    ..s.clone()
                });
            }
        }
        if !found_current {
            if let Some(live) = live {
                out.push(live);
            }
        }
        out.sort_by(|a, b| a.id.cmp(&b.id).then(a.source.cmp(&b.source)));
        for s in &mut out {
            self.attach_baseline(s);
        }
        out
    }

    /// A schema 3 workspace keeps calendars and resources in its shared
    /// dictionary, so a change made in one project (say, a calendar's work
    /// hours) changes the rollups of closed projects that use them. Drop the
    /// cached rollups of those projects when the dictionary changed: every
    /// one when a calendar changed (rollups count work time on the
    /// workspace's default calendar), else those that use a changed row.
    pub(crate) fn drop_stale_rollups(&mut self) {
        if !self.dictionary_shared() {
            return;
        }
        let Ok(revision) = self.store.dictionary_revision() else {
            return;
        };
        if matches!(&self.views.projects.dictionary, Some((seen, _)) if *seen == revision) {
            return;
        }
        let Ok((dict, revision)) = self.store.dictionary() else {
            return;
        };
        let state = &mut self.views.projects;
        if let Some((_, old)) = &state.dictionary {
            {
                use openxer_xer::workspace_scope::delta::{diff, RowKey};
                let delta = diff(old, &dict);
                let calendars = delta.changes.iter().any(|c| c.table == "CALENDAR");
                let changed: BTreeSet<(String, i64)> = delta
                    .changes
                    .iter()
                    .map(|c| match c.key {
                        RowKey::Id(id) => (c.table.clone(), id),
                        // An unkeyed child row (a resource's role) changes its owner.
                        RowKey::Owner(id) => (
                            openxer_xer::workspace_scope::OWNED
                                .iter()
                                .find(|(t, _, _)| *t == c.table)
                                .map_or(c.table.clone(), |(_, _, o)| o.to_string()),
                            id,
                        ),
                    })
                    .collect();
                state
                    .cache
                    .retain(|_, c| !calendars && c.uses.is_disjoint(&changed));
            }
        }
        state.dictionary = Some((revision, dict));
    }

    /// Put the start and finish of the project's baseline (its project
    /// baseline, else its Primary baseline when that is a stored one) into
    /// its rollup, for Finish Variance and the chart's baseline bar.
    fn attach_baseline(&mut self, s: &mut ProjectSummary) {
        use crate::baselines::store::{self as bstore, Choice};
        let Some(id) = s.store_id else {
            return;
        };
        let c = self.store.connection();
        let stored = bstore::list(c, id).unwrap_or_default();
        if stored.is_empty() {
            return;
        }
        let pairs: Vec<(i64, i64)> = stored.iter().map(|b| (b.id, b.project_key)).collect();
        let primary = match bstore::user_baselines(c, id).0[0] {
            Choice::Baseline(b) => Some(b),
            _ => None,
        };
        let assigned = s
            .project
            .get("sum_base_proj_id")
            .and_then(|v| v.trim().parse::<i64>().ok())
            .filter(|k| *k != 0);
        let Some(bl) = rollup::pick_baseline(assigned, &pairs, primary) else {
            return;
        };
        let meta = stored.iter().find(|b| b.id == bl).unwrap();
        let stamp = format!("{}|{}|{}", meta.added, meta.updated, meta.project_key);
        let cache = &mut self.views.projects.baselines;
        let fresh = cache.get(&bl).filter(|(st, _, _)| *st == stamp).cloned();
        let (start, finish) = match fresh {
            Some((_, a, b)) => (a, b),
            None => {
                let dates = bstore::get(c, bl)
                    .ok()
                    .flatten()
                    .and_then(|row| Document::parse(&row.xer).ok())
                    .and_then(|doc| rollup::of_document(&doc, meta.project_key))
                    .map_or((None, None), |r| (r.start, r.finish));
                cache.insert(bl, (stamp, dates.0, dates.1));
                dates
            }
        };
        s.rollup.set_baseline(start, finish);
    }

    pub(super) fn open_workspace_project(&mut self, summary: &ProjectSummary) {
        if summary.current {
            self.show_view(MainView::Activities);
            return;
        }
        // Another open project becomes the current one.
        if let Some(id) = summary.store_id.filter(|&id| self.is_open(id)) {
            self.activate_id(id);
            self.show_view(MainView::Activities);
            return;
        }
        let Some(id) = summary.store_id else {
            return;
        };
        match self.store.load(id) {
            Ok(p) => {
                let was_empty = self.open_set.empty;
                let before = self.project.source_name.clone();
                let key = self.project.model.project.proj_id;
                self.load(p);
                if was_empty
                    || self.project.source_name != before
                    || self.project.model.project.proj_id != key
                {
                    self.saved_projects = self.store.list().unwrap_or_default();
                    self.show_view(MainView::Activities);
                }
            }
            Err(e) => self.error(e),
        }
    }

    pub(super) fn projects_view(&mut self, host: &mut egui::Ui) {
        let filter = self.views.projects.status_filter.clone();
        let open_only = self.views.projects.open_only;
        let open_now = self.open_ids();
        let mut projects: Vec<ProjectSummary> = self
            .workspace_projects()
            .into_iter()
            .filter(|s| filter.is_empty() || filter.contains(rollups::status_of(s)))
            .filter(|s| {
                !open_only || s.current || s.store_id.is_some_and(|id| open_now.contains(&id))
            })
            .collect();
        // The User Defined columns after the table's own.
        let udfs =
            super::udf_columns::Shown::of(&self.project.document, &self.views.projects.columns);
        // The project code columns among the extra ones (`pcat_<type>`).
        let extra = self.views.projects.columns.extra.clone();
        let code_names: BTreeMap<i64, String> = rollups::code_types(&self.project.document)
            .into_iter()
            .collect();
        let code_of = |field: usize| {
            field
                .checked_sub(udfs.own)
                .and_then(|k| extra.get(k))
                .and_then(|id| rollups::code_type_of(id))
        };
        let extra_titles: Vec<String> = extra
            .iter()
            .zip(&udfs.titles)
            .map(|(id, t)| match rollups::code_type_of(id) {
                Some(ty) => code_names.get(&ty).cloned().unwrap_or_default(),
                None => t.clone(),
            })
            .collect();
        if let Some(sort) = self.views.projects.sort {
            match udfs.field(sort.field) {
                _ if code_of(sort.field).is_some() => {
                    let ty = code_of(sort.field).unwrap_or_default();
                    projects.sort_by(|a, b| {
                        let (x, y) = (a.codes.get(&ty), b.codes.get(&ty));
                        let o = match (x, y) {
                            (Some(x), Some(y)) => x.to_lowercase().cmp(&y.to_lowercase()),
                            (Some(_), None) => std::cmp::Ordering::Less,
                            (None, Some(_)) => std::cmp::Ordering::Greater,
                            (None, None) => std::cmp::Ordering::Equal,
                        };
                        let o = if sort.descending && x.is_some() && y.is_some() {
                            o.reverse()
                        } else {
                            o
                        };
                        o.then_with(|| a.id.cmp(&b.id).then(a.source.cmp(&b.source)))
                    });
                }
                Some(ty) => {
                    // Blanks last, ties by Project ID.
                    use super::udf_columns::{compare, raw_sort};
                    let doc = &self.project.document;
                    projects.sort_by(|a, b| {
                        let x = raw_sort(doc, 0, ty, &a.udf);
                        let y = raw_sort(doc, 0, ty, &b.udf);
                        compare(x.as_ref(), y.as_ref(), sort.descending)
                            .then_with(|| a.id.cmp(&b.id).then(a.source.cmp(&b.source)))
                    });
                }
                None => projects.sort_by(|a, b| rollups::compare(a, b, sort)),
            }
        }
        let ws = self.workspace().clone();
        // Workspace IDs; the open project before its first save is -1.
        let keys: Vec<i64> = projects.iter().map(|p| p.store_id.unwrap_or(-1)).collect();
        // Grouped by anything but the EPS: the bands are not folders.
        let group = self.views.projects.group;
        let by_status = group != Grouping::Eps;
        let (groups, band_names) = match group {
            Grouping::Eps => (ws.grouped(&keys), vec![]),
            Grouping::Status => (rollups::status_groups(&projects, &keys), vec![]),
            Grouping::List => (vec![], vec![]),
            Grouping::Manager => rollups::value_groups(&projects, &keys, |s| s.manager.clone()),
            Grouping::Priority => rollups::value_groups(&projects, &keys, |s| {
                s.project.get("priority_num").cloned().unwrap_or_default()
            }),
        };
        self.views.projects.bands = groups.iter().map(|(id, depth, _)| (*id, *depth)).collect();
        let lines = if group == Grouping::List {
            keys.iter().map(|k| EpsLine::Project(*k, 0)).collect()
        } else {
            eps_lines(&groups, &self.views.projects.collapsed)
        };
        let members = |eps: i64| -> Vec<i64> {
            if eps < 0 {
                groups
                    .iter()
                    .find(|(g, _, _)| *g == eps)
                    .map_or(vec![], |(_, _, p)| p.clone())
            } else {
                ws.projects_under(eps, &keys)
            }
        };
        let subtree = |eps: i64| -> Vec<ProjectSummary> {
            members(eps)
                .iter()
                .filter_map(|id| keys.iter().position(|k| k == id))
                .map(|i| projects[i].clone())
                .collect()
        };
        let group_label = |id: i64| -> (String, String) {
            if id < 0 {
                let name = match group {
                    Grouping::Status => rollups::status_group_name(id).to_string(),
                    _ => band_names
                        .get((-id - 1) as usize)
                        .cloned()
                        .unwrap_or_default(),
                };
                (name.clone(), name)
            } else {
                (
                    ws.eps_value(id, "eps_short_name"),
                    ws.eps_value(id, "eps_name"),
                )
            }
        };
        let selected = self
            .views
            .projects
            .selected
            .clone()
            .and_then(|key| projects.iter().find(|p| (p.source.clone(), p.key) == key))
            .cloned();
        let selected_eps = self
            .views
            .projects
            .selected_eps
            .filter(|id| groups.iter().any(|(g, _, _)| g == id))
            .or_else(|| groups.first().map(|(id, _, _)| *id));
        self.views.projects.eps_label = selected_eps.map(group_label);
        let eps_projects = selected_eps.map_or(vec![], subtree);
        let mut open = None;
        let mut close = None;
        let mut cut = None;
        let mut paste = None;
        let mut copy = None;
        let mut add = None;
        let mut delete = None;
        let open_ids = self.open_ids();
        let is_open =
            |s: &ProjectSummary| s.current || s.store_id.is_some_and(|id| open_ids.contains(&id));
        // Paste goes under the selected EPS row, or the selected project's node.
        let target = match &selected {
            Some(s) => s.store_id.map(|id| ws.node_of(Some(id))),
            // A status group is not a folder.
            None => selected_eps.filter(|id| *id > 0),
        };
        // An EPS row selected: Delete takes the node with what it holds.
        let folder = if selected.is_none() { target } else { None };
        let mut delete_folder = None;
        let pending = self.views.projects.cut.filter(|id| keys.contains(id));
        let copied = self.views.projects.copy.filter(|id| keys.contains(id));
        let edit_bar = self.bar_visible("Edit");
        let has_store = selected.as_ref().is_some_and(|s| s.store_id.is_some());
        self.views.projects.edit_enabled = Some([
            target.is_some(),
            folder.is_some() || has_store,
            has_store,
            has_store,
            (pending.is_some() || copied.is_some()) && target.is_some(),
        ]);
        egui::SidePanel::right("projects_command_bar")
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
                let response = sheet::side_button(
                    ui,
                    chrome::Icon::ProjectOpen,
                    "Open Project",
                    selected.is_some(),
                );
                self.action("projects_cmd_open", &response);
                if response.clicked() {
                    open = selected.clone();
                }
                let closable = selected.as_ref().is_some_and(is_open);
                let response =
                    sheet::side_button(ui, chrome::Icon::ProjectClosed, "Close Project", closable);
                self.action("projects_cmd_close", &response);
                if response.clicked() {
                    close = selected.as_ref().and_then(|s| s.store_id);
                }
                // The right dock's Edit bar runs Add, Delete, Cut, Copy and
                // Paste here too (`view_edit`): the bar keeps them only
                // while that bar is hidden, so each shows once.
                if edit_bar {
                    return;
                }
                let response = sheet::side_button(
                    ui,
                    chrome::Icon::Add,
                    "Add (the New Project dialog, filing the project in the selected folder)",
                    target.is_some(),
                );
                self.action("projects_cmd_add", &response);
                if response.clicked() {
                    add = target;
                }
                let deletable = folder.is_some() || has_store;
                let response = sheet::side_button(
                    ui,
                    chrome::Icon::Delete,
                    "Delete (a project, or an EPS node with everything under it)",
                    deletable,
                );
                self.action("projects_cmd_delete", &response);
                if response.clicked() {
                    delete = selected.as_ref().and_then(|s| s.store_id);
                    delete_folder = folder;
                }
                let response = sheet::side_button(
                    ui,
                    chrome::Icon::Cut,
                    "Cut (then Paste on an EPS node to move the project)",
                    selected.as_ref().is_some_and(|s| s.store_id.is_some()),
                );
                self.action("projects_cmd_cut", &response);
                if response.clicked() {
                    cut = selected.as_ref().and_then(|s| s.store_id);
                }
                let response = sheet::side_button(
                    ui,
                    chrome::Icon::Copy,
                    "Copy (then Paste on an EPS node to add a copy)",
                    selected.as_ref().is_some_and(|s| s.store_id.is_some()),
                );
                self.action("projects_cmd_copy", &response);
                if response.clicked() {
                    copy = selected.as_ref().and_then(|s| s.store_id);
                }
                let response = sheet::side_button(
                    ui,
                    chrome::Icon::Paste,
                    "Paste",
                    (pending.is_some() || copied.is_some()) && target.is_some(),
                );
                self.action("projects_cmd_paste", &response);
                if response.clicked() {
                    paste = pending.or(copied).zip(target);
                }
            });
        if self.views.projects.show_details {
            let tab = self.views.projects.tab;
            let (height, clicked) = self.details_pane(
                host,
                "projects",
                self.views.projects.details_height,
                &TABS,
                tab,
                |app, c, size| app.project_details(c, size, tab, selected.as_ref(), &eps_projects),
            );
            self.views.projects.details_height = height;
            if let Some(tab) = clicked {
                self.views.projects.tab = tab;
            }
        }
        let label_options = self.label_options();
        let mut customize = false;
        let mut table_font = false;
        let mut collapse_to = false;
        let mut fill_down = false;
        let mut cell_edit = self.take_cell_edit("projects");
        let mut committed = None;
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(sheet::MARGIN))
            .show_inside(host, |ui| {
                ui.spacing_mut().item_spacing.y = 0.;
                let (inner, strip) = sheet::table_frame(ui, ui.max_rect());
                let bar = Rect::from_min_size(strip, Vec2::new(inner.width(), 17.));
                let header = sheet::header_button(ui, "projects", bar, "Layout: Projects");
                self.action("projects_layout", &header);
                egui::Popup::menu(&header).show(|ui| {
                    ui.checkbox(&mut self.views.projects.show_details, "Project Details");
                    ui.separator();
                    let s = &mut self.views.projects;
                    let r = ui.radio_value(&mut s.group, Grouping::Eps, "Group by EPS Folder");
                    let eps_rect = r.rect;
                    let r2 = ui.radio_value(&mut s.group, Grouping::Status, "Group by Status");
                    let status_rect = r2.rect;
                    ui.separator();
                    ui.label("Show Status");
                    let mut rects = vec![
                        ("projects_group_eps".to_string(), eps_rect),
                        ("projects_group_status".to_string(), status_rect),
                    ];
                    for (code, label) in STATUSES {
                        let mut on = s.status_filter.is_empty() || s.status_filter.contains(code);
                        let r = ui.checkbox(&mut on, label);
                        rects.push((format!("projects_show_{code}"), r.rect));
                        if r.changed() {
                            if s.status_filter.is_empty() {
                                s.status_filter =
                                    STATUSES.iter().map(|(k, _)| k.to_string()).collect();
                            }
                            if on {
                                s.status_filter.insert(code.to_string());
                            } else {
                                s.status_filter.remove(code);
                            }
                            if s.status_filter.len() == STATUSES.len() {
                                s.status_filter.clear();
                            }
                        }
                    }
                    ui.separator();
                    let r = ui.button("Columns...");
                    rects.push(("projects_layout_customize".into(), r.rect));
                    if r.clicked() {
                        customize = true;
                        ui.close();
                    }
                    for (name, r) in rects {
                        self.action_rects
                            .insert(name, [r.left(), r.top(), r.right(), r.bottom()]);
                    }
                });
                let body = Rect::from_min_max(Pos2::new(inner.left(), bar.bottom()), inner.max);
                let mut ui = ui.new_child(egui::UiBuilder::new().max_rect(body));
                let ui = &mut ui;
                let first_start = projects
                    .iter()
                    .filter_map(|p| min_opt(p.start, p.rollup.baseline_start))
                    .min();
                let mut gantt = chart::SummaryGantt::new(0, first_start, label_options, 0.);
                let bar_of = |s: &ProjectSummary, kind: chart::BarKind| {
                    let r = &s.rollup;
                    let actual_to = r.actual_start.map(|_| {
                        r.actual_finish
                            .or(r.data_date)
                            .unwrap_or(s.finish.unwrap_or(0))
                    });
                    s.start.zip(s.finish).map(|(a, b)| chart::Bar {
                        start: a,
                        finish: b,
                        kind,
                        start_label: None,
                        finish_label: format::date(0, s.finish),
                        actual: r
                            .actual_start
                            .zip(actual_to)
                            .map(|(x, y)| (x, y.clamp(x, b.max(x)))),
                        data_date: r.data_date,
                        baseline: r.baseline_start.zip(r.baseline_finish),
                    })
                };
                let mut rows = vec![];
                for line in &lines {
                    match *line {
                        EpsLine::Eps(id, depth) => {
                            let members = subtree(id);
                            let refs: Vec<&ProjectSummary> = members.iter().collect();
                            let (code, name) = group_label(id);
                            let eps = rollups::folder_summary(code, name, &refs);
                            rows.push(Row {
                                depth,
                                expander: Some(!self.views.projects.collapsed.contains(&id)),
                                icon: Some(chrome::Icon::Eps),
                                // A folder holds no values of its own.
                                cells: rollups::cells(&eps, true),
                                band: true,
                                strips: depth,
                                ..Default::default()
                            });
                            gantt.bars.push(bar_of(&eps, chart::BarKind::Eps));
                        }
                        EpsLine::Project(key, depth) => {
                            let Some(i) = keys.iter().position(|k| *k == key) else {
                                continue;
                            };
                            let s = &projects[i];
                            let mut cells = rollups::cells(s, false);
                            if !udfs.types.is_empty() {
                                // Dates are absolute here: the origin is 0.
                                use super::udf_columns::raw_text;
                                let doc = &self.project.document;
                                cells.resize(udfs.own, String::new());
                                for (ty, id) in udfs.types.iter().zip(&extra) {
                                    let text = match (ty, rollups::code_type_of(id)) {
                                        (_, Some(code)) => {
                                            s.codes.get(&code).cloned().unwrap_or_default()
                                        }
                                        (Some(ty), None) => raw_text(doc, 0, ty, &s.udf),
                                        (None, None) => String::new(),
                                    };
                                    cells.push(text);
                                }
                            }
                            rows.push(Row {
                                depth,
                                icon: Some(if is_open(s) {
                                    chrome::Icon::ProjectOpen
                                } else {
                                    chrome::Icon::ProjectClosed
                                }),
                                cells,
                                strips: depth,
                                ..Default::default()
                            });
                            gantt.bars.push(
                                (s.activities > 0)
                                    .then_some(())
                                    .and_then(|_| bar_of(s, chart::BarKind::Project)),
                            );
                        }
                    }
                }
                // The chart scrolls and zooms like the Gantt, except in a
                // printout, which shows the whole chart at its own scale.
                let painting = self.print.painting;
                let default = if painting {
                    gantt.room_for_start_labels(ui.painter());
                    0.
                } else {
                    gantt.follow(self.views.projects.timeline_first, self.views.projects.zoom)
                };
                let project_line = |line: &EpsLine| match line {
                    EpsLine::Project(key, _) => {
                        keys.iter().position(|k| k == key).map(|i| &projects[i])
                    }
                    EpsLine::Eps(..) => None,
                };
                let selected_row = match &self.views.projects.selected {
                    Some(key) => lines.iter().position(|l| {
                        project_line(l).is_some_and(|p| (p.source.clone(), p.key) == *key)
                    }),
                    None => lines.iter().position(
                        |l| matches!(l, EpsLine::Eps(id, _) if Some(*id) == selected_eps),
                    ),
                };
                // Built for the row under the pointer (an EPS node or a
                // project), so it reads true on the frame the click selects.
                let show_details = self.views.projects.show_details;
                // Filter By's choice in use, as the menu checks it.
                let status_filter = &self.views.projects.status_filter;
                let only_status = STATUSES
                    .iter()
                    .position(|(k, _)| status_filter.contains(*k))
                    .filter(|_| status_filter.len() == 1);
                let non_whatif = status_filter.len() == STATUSES.len() - 1
                    && !status_filter.contains("WS_Whatif");
                let collapsed = &self.views.projects.collapsed;
                let any_collapsed = groups.iter().any(|g| collapsed.contains(&g.0));
                let any_open = groups.iter().any(|g| !collapsed.contains(&g.0));
                let deletable_row = |s: &ProjectSummary| s.store_id.is_some();
                let fill_ready = self.view_fill_ready();
                let menu = |row: usize| {
                    let project = lines.get(row).and_then(project_line);
                    projects_menu(&ProjectsMenu {
                        on_project: project.is_some(),
                        is_open: project.is_some_and(is_open),
                        can_add: match lines.get(row) {
                            Some(EpsLine::Eps(id, _)) => *id > 0,
                            Some(_) => project.is_some_and(|p| p.store_id.is_some()),
                            None => !by_status,
                        },
                        stored: project.is_some_and(|p| p.store_id.is_some()),
                        deletable: project.is_some_and(deletable_row)
                            || matches!(lines.get(row), Some(EpsLine::Eps(id, _)) if *id > 0),
                        clipboard: pending.is_some() || copied.is_some(),
                        details: show_details,
                        group,
                        open_only,
                        only_status,
                        non_whatif,
                        any_collapsed,
                        any_open,
                        fill: fill_ready,
                    })
                };
                let chart_menu = || projects_chart_menu(show_details);
                let open_menu = self.ctx_menu.take();
                // The cell commands (Fill Down) read the drawn rows. Noted
                // before the cell editor borrows the project below.
                let mut cell_rows = vec![];
                let cell_keys: Vec<Option<super::table_cells::CellKey>> = lines
                    .iter()
                    .map(|line| {
                        let p = project_line(line)?;
                        cell_rows.push((ProjectRow::Project(p.source.clone(), p.key), p.current));
                        Some(super::table_cells::CellKey::Project(cell_rows.len() - 1))
                    })
                    .collect();
                let fields: Vec<String> = rollups::IDS
                    .iter()
                    .map(|f| f.to_string())
                    .chain(self.views.projects.columns.extra.iter().cloned())
                    .collect();
                self.views.projects.cell_rows = cell_rows;
                self.note_view_cells(
                    "projects",
                    fields,
                    TITLES
                        .iter()
                        .map(|t| t.to_string())
                        .chain(udfs.titles.iter().cloned())
                        .collect(),
                    cell_keys,
                    std::rc::Rc::new(((), rows.clone())),
                );
                let right: Vec<usize> = rollups::right_columns()
                    .into_iter()
                    .chain(udfs.right())
                    .collect();
                let titles: Vec<&str> = TITLES
                    .iter()
                    .copied()
                    .chain(extra_titles.iter().map(String::as_str))
                    .collect();
                let indicators = udfs.indicators();
                let table = grid::Table {
                    id: "projects",
                    titles: &titles,
                    right: &right,
                    tree_field: 0,
                    header_height: grid::HEADER_HEIGHT,
                    row_lines: false,
                    band_pitch: grid::BandPitch::Styled,
                    // EPS bands: ID and name in the caption font.
                    band_caption: &[1],
                    menu: Some(&menu),
                    open_menu,
                    chart_menu: Some(&chart_menu),
                };
                let mut width = self.views.projects.table_width;
                let row_key = |line: &EpsLine| match line {
                    EpsLine::Eps(id, _) => Some(ProjectRow::Eps(*id)),
                    EpsLine::Project(..) => {
                        project_line(line).map(|p| ProjectRow::Project(p.source.clone(), p.key))
                    }
                };
                let shown: Vec<Option<ProjectRow>> = lines.iter().map(row_key).collect();
                // Only the open project's own values are typed here.
                let current_row = |row: usize| {
                    lines
                        .get(row)
                        .and_then(project_line)
                        .is_some_and(|p| p.current)
                };
                let doc = &self.project.document;
                let obs = super::wbs::obs_names(&self.project);
                let kind = |row: usize, field: usize| {
                    if !current_row(row) {
                        return None;
                    }
                    match udfs.field(field) {
                        Some(ty) => super::udf_columns::editor(doc, ty),
                        None => project_cell_editor(field, &obs),
                    }
                };
                // Shown by construction: `selected_row` is a position in `lines`.
                let mut current = selected_row.and_then(|i| shown.get(i).cloned().flatten());
                let selection = self.views.projects.rows.shown(&mut current, &shown);
                let view = &self.views.projects;
                let look = grid::Look::of(&view.table, view.row_height);
                let response = grid::show_with(
                    ui,
                    &table,
                    &mut self.views.projects.columns,
                    &rows,
                    &if self.print.painting {
                        grid::Selection::default()
                    } else {
                        selection
                    },
                    None,
                    Some((&mut gantt, &mut width)),
                    &mut self.action_rects,
                    grid::Extras {
                        indicators: &indicators,
                        editing: Some(grid::Editing {
                            kind: &kind,
                            open: &mut cell_edit,
                        }),
                        look: Some(&look),
                        ..Default::default()
                    },
                );
                committed = response.committed.clone();
                self.views.projects.table_width = width;
                if !painting {
                    let s = &mut self.views.projects;
                    (s.timeline_first, s.zoom, s.timeline_place) =
                        gantt.kept(s.timeline_first, default);
                    s.timeline_default = default;
                }
                for (row, r) in &gantt.drawn {
                    self.action_rects.insert(
                        format!("projects_bar_{row}"),
                        [r.left(), r.top(), r.right(), r.bottom()],
                    );
                }
                for (row, r) in &gantt.baselines {
                    self.action_rects.insert(
                        format!("projects_baseline_{row}"),
                        [r.left(), r.top(), r.right(), r.bottom()],
                    );
                }
                if let Some(field) = response.header_clicked {
                    let s = &mut self.views.projects;
                    s.sort = rollups::next_sort(s.sort, field);
                }
                if let Some(EpsLine::Eps(id, _)) = response.toggled.and_then(|i| lines.get(i)) {
                    let collapsed = &mut self.views.projects.collapsed;
                    if !collapsed.remove(id) {
                        collapsed.insert(*id);
                    }
                }
                if let Some(key) = response
                    .clicked
                    .and_then(|i| shown.get(i).cloned().flatten())
                {
                    let s = &mut self.views.projects;
                    let next = s.rows.click(
                        current.as_ref(),
                        key,
                        response.modifiers,
                        response.secondary,
                    );
                    match next {
                        ProjectRow::Eps(id) => {
                            s.selected = None;
                            s.selected_eps = Some(id);
                        }
                        ProjectRow::Project(source, key) => s.selected = Some((source, key)),
                    }
                }
                let project_at = |i: usize| lines.get(i).and_then(project_line).cloned();
                if let Some(p) = response.double_clicked.and_then(project_at) {
                    open = Some(p);
                }
                if let Some((i, key)) = response.menu {
                    match key {
                        "open_project" => open = project_at(i),
                        "close_project" => close = project_at(i).and_then(|p| p.store_id),
                        "add" | "cut" | "copy" | "paste" | "delete" => {
                            self.views.projects.command = Some(key);
                        }
                        "details" => {
                            self.views.projects.show_details = !self.views.projects.show_details;
                        }
                        "edit_details" => {
                            self.views.projects.show_details = true;
                            self.views.projects.tab = 0;
                        }
                        "customize" => customize = true,
                        "fill_down" => fill_down = true,
                        "table_font" => table_font = true,
                        "collapse_to" => collapse_to = true,
                        "expand_all" => self.views.projects.collapsed.clear(),
                        "collapse_all" => {
                            self.views.projects.collapsed = groups.iter().map(|g| g.0).collect();
                        }
                        other => self.projects_filter_group(other),
                    }
                }
                if response.chart_menu == Some("details") {
                    self.views.projects.show_details = !self.views.projects.show_details;
                }
            });
        self.keep_cell_edit("projects", cell_edit);
        if let Some((_, field, text)) = committed {
            // The editor opens on the open project's row only.
            let time = host.ctx().input(|i| i.time);
            if let Some(type_id) = udfs.field(field).map(|t| t.type_id) {
                let pid = self.project.model.project.proj_id;
                self.commit_udf_cell(type_id, pid, &text, time);
            } else if let Some(id) = rollups::IDS.get(field) {
                self.commit_project_cell(id, &text, time);
            }
        }
        if table_font {
            self.open_table_font_for(crate::layout_opts::TableTarget::Projects);
        }
        if collapse_to {
            self.open_collapse_to_for(crate::layout_opts::CollapseTarget::Projects);
        }
        if customize {
            let titles: Vec<String> = TITLES
                .iter()
                .map(|t| t.to_string())
                .chain(extra_titles.iter().cloned())
                .collect();
            let shown = self
                .views
                .projects
                .columns
                .order
                .iter()
                .filter_map(|&c| titles.get(c).cloned())
                .collect();
            let default: Vec<&str> = rollups::default_order()
                .iter()
                .map(|&c| TITLES[c])
                .collect();
            // Project codes, then the User Defined fields, after the own.
            let codes: Vec<(String, String)> = code_names
                .iter()
                .map(|(ty, name)| (rollups::code_id(*ty), name.clone()))
                .collect();
            let udf_choices = super::udf_columns::choices(&self.project.document, "projects");
            let choices: Vec<(String, String)> = codes.iter().cloned().chain(udf_choices).collect();
            self.open_view_columns_with(
                crate::details::grid_columns::ColumnTarget::Projects,
                "Project",
                &TITLES,
                &choices,
                &default,
                shown,
            );
            if let Some(d) = self.detail.column_dialog.as_mut() {
                d.groups = rollups::GROUPS
                    .iter()
                    .map(|g| g.to_string())
                    .chain(codes.iter().map(|_| rollups::CODES.to_string()))
                    .collect();
            }
        }
        let time = host.ctx().input(|i| i.time);
        if fill_down {
            self.view_fill_down(time);
        }

        let deletable = |s: &ProjectSummary| s.store_id.is_some();
        match self.views.projects.command.take() {
            Some("add") => add = target,
            Some("delete") => {
                delete = selected
                    .as_ref()
                    .filter(|s| deletable(s))
                    .and_then(|s| s.store_id);
                delete_folder = folder;
            }
            Some("cut") => cut = selected.as_ref().and_then(|s| s.store_id),
            Some("copy") => copy = selected.as_ref().and_then(|s| s.store_id),
            Some("paste") => paste = pending.or(copied).zip(target),
            _ => {}
        }
        if let Some(id) = cut {
            self.views.projects.cut = Some(id);
            self.views.projects.copy = None;
            self.status = "Select an EPS node and choose Paste to move the project".into();
        }
        if let Some(id) = copy {
            self.views.projects.copy = Some(id);
            self.views.projects.cut = None;
            self.status = "Select an EPS node and choose Paste to add a copy of the project".into();
        }
        if let Some(eps) = add {
            // The New Project dialog, with this folder chosen.
            self.open_new_project_in(eps);
        }

        if let Some(id) = delete {
            self.request_project_delete(id);
        }
        if let Some(eps) = delete_folder {
            self.request_folder_delete(eps);
        }
        if let Some((project, eps)) = paste.filter(|_| copied.is_some() && pending.is_none()) {
            self.open_copy_options(project, eps);
            paste = None;
        }
        if let Some((project, eps)) = paste {
            match self.run_workspace(
                Box::new(move |ws| {
                    Ok(if ws.place(project, eps)? {
                        crate::enterprise::Done::Changed(None)
                    } else {
                        crate::enterprise::Done::Nothing
                    })
                }),
                time,
            ) {
                Ok(_) => {
                    self.views.projects.cut = None;
                    let name = self.workspace().eps_value(eps, "eps_name");
                    self.status = format!("Project moved to {name}");
                }
                Err(e) => self.error(e),
            }
        }
        if let Some(summary) = open {
            self.open_workspace_project(&summary);
        }
        if let Some(id) = close {
            self.close_project_id(id);
        }
        self.project_ops_dialogs(&host.ctx().clone());
    }

    /// Writes a PROJECT column of the open project through the shared edit
    /// layer (the command line's `set_project` record edit); the frame's
    /// history makes it one undo step.
    fn set_project_field(&mut self, field: &str, value: String) {
        use openxer_xer::edit::tables::{self, serde_like::Value};
        let pid = self.project.model.project.proj_id;
        let current = self.project.document.table("PROJECT").and_then(|t| {
            t.rows
                .iter()
                .find(|r| num(t.value(r, "proj_id")) == pid)
                .map(|r| t.value(r, field).to_string())
        });
        if current.is_none() || current.as_deref() == Some(value.as_str()) {
            return;
        }
        match tables::set_project_field(&mut self.project, field, &Value::Text(value)) {
            Ok(()) => self.touch_records(),
            Err(e) => self.status = e,
        }
    }

    /// Writes a PROJWBS field of the open project's project node.
    fn set_node_field(&mut self, field: &str, value: String) {
        if let Some(node) = project_node(&self.project) {
            let table = self.project.document.table_mut("PROJWBS");
            if let Some(i) = table
                .rows
                .iter()
                .position(|r| num(table.value(r, "wbs_id")) == node)
            {
                if table.value(&table.rows[i], field) != value {
                    table.set(i, field, value);
                    self.touch_records();
                }
            }
        }
    }

    /// Must Finish By: the project's finish constraint used by the next F9.
    pub(crate) fn set_must_finish(&mut self, value: Option<i64>) {
        if self.project.model.project.plan_end != value {
            self.project.model.project.plan_end = value;
            self.must_finish = openxer_xer::date_string(self.project.model.origin_serial, value);
            self.dirty = true;
        }
    }

    /// Project Details, Dates tab, Data Date: one undo step through the
    /// shared edit layer. Nothing is scheduled; the status line says to run
    /// the schedule.
    pub(crate) fn set_data_date(&mut self, value: Option<i64>) -> Result<(), String> {
        let before = self.project.clone();
        let revision = self.records_revision;
        if !openxer_xer::edit::project_dates::set_data_date(&mut self.project, value)? {
            return Ok(());
        }
        self.data_date = openxer_xer::date_string(self.project.model.origin_serial, value);
        self.edit_step(before, revision, 0.);
        self.status = "Data date changed. Schedule (F9) to move the activities to it.".into();
        Ok(())
    }

    pub(crate) fn set_planned_start(&mut self, value: Option<i64>) -> Result<(), String> {
        let Some(value) = value else {
            return Err("The project needs a planned start.".into());
        };
        if self.project.model.project.plan_start != Some(value) {
            self.project.model.project.plan_start = Some(value);
            self.dirty = true;
        }
        Ok(())
    }

    fn project_details(
        &mut self,
        c: &mut Canvas,
        size: Vec2,
        tab: usize,
        selected: Option<&ProjectSummary>,
        projects: &[ProjectSummary],
    ) {
        let eps;
        let (s, is_eps) = match selected {
            Some(s) => (s, false),
            None => {
                let mut rollup = eps_rollup(projects);
                if let Some((id, name)) = &self.views.projects.eps_label {
                    rollup.id = id.clone();
                    rollup.name = name.clone();
                }
                eps = rollup;
                (&eps, true)
            }
        };
        let editable = s.current && !is_eps;
        let w = size.x;
        let h = size.y;
        match TABS[tab.min(TABS.len() - 1)] {
            "General" => self.project_general(c, w, s, is_eps, editable),
            "Notebook" => self.project_notebook(c, w, h, s),
            "Budget Log" | "Spending Plan" | "Budget Summary" => {
                let scope =
                    project_node(&self.project)
                        .filter(|_| editable)
                        .map(|wbs| budget::Scope {
                            proj: self.project.model.project.proj_id,
                            wbs,
                        });
                match TABS[tab.min(TABS.len() - 1)] {
                    "Budget Log" => self.budget_log_tab(c, "project", w, h, scope, &s.node),
                    "Spending Plan" => self.spending_plan_tab(c, "project", w, h, scope, &s.node),
                    _ => self.budget_summary_tab(c, w, h, scope, &s.node),
                }
            }
            "Dates" => self.project_dates(c, w, h, s, editable),
            "Funding" => self.project_funding_tab(c, w, h, editable),
            "Codes" => self.project_codes_tab(c, w, h, editable),
            "Defaults" => self.project_defaults(c, w, h, s, editable),
            "Resources" => self.project_resources_tab(c, w, h, s, editable),
            "Settings" => self.project_settings(c, w, h, s, editable),
            _ => self.project_calculations(c, w, h, s, editable),
        }
    }

    fn project_general(
        &mut self,
        c: &mut Canvas,
        w: f32,
        s: &ProjectSummary,
        is_eps: bool,
        editable: bool,
    ) {
        sheet::section(
            c,
            6.,
            6.,
            w - 12.,
            505f32.min(c.ui.max_rect().height() - 6.),
            "",
        );
        let (id_label, name_label) = if is_eps {
            ("EPS ID", "EPS Name")
        } else {
            ("Project ID", "Project Name")
        };
        let node = project_node(&self.project);
        if let Some(id) = sheet::text_field(
            c,
            "project_id_edit",
            21.,
            34.,
            212.,
            id_label,
            &s.id,
            editable,
        ) {
            if !id.trim().is_empty() {
                self.project.model.project.name = id.trim().to_string();
                self.dirty = true;
            }
        }
        if let Some(name) = sheet::text_field(
            c,
            "project_name_edit",
            248.,
            34.,
            773.,
            name_label,
            &s.name,
            editable && node.is_some(),
        ) {
            if let Some(w) = node.and_then(|n| self.project.wbs.iter_mut().find(|w| w.id == n)) {
                w.name = name;
                super::wbs::sync_projwbs(&mut self.project);
                self.touch_records();
            }
        }
        sheet::above(c, 21., 80., "Status", true);
        if let Some(code) = sheet::combo(
            c,
            "project_status",
            21.,
            80.,
            212.,
            &STATUSES,
            get(&s.node, "status_code"),
            editable && node.is_some(),
        ) {
            self.set_node_field("status_code", code);
        }
        sheet::above(c, 248., 80., "Responsible Manager", true);
        let obs = super::wbs::obs_names(&self.project);
        let obs_options: Vec<(&str, &str)> =
            obs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        let obs_id = if is_eps {
            obs.first().map_or(String::new(), |(k, _)| k.clone())
        } else {
            get(&s.node, "obs_id").to_string()
        };
        if editable && node.is_some() && !obs_options.is_empty() {
            if let Some(code) = sheet::combo(
                c,
                "project_manager",
                248.,
                80.,
                384.,
                &obs_options,
                &obs_id,
                true,
            ) {
                self.set_node_field("obs_id", code);
            }
        } else {
            let name = obs
                .iter()
                .find(|(k, _)| *k == obs_id)
                .map_or("", |(_, n)| n.as_str());
            sheet::picker(
                c,
                "project_manager",
                248.,
                80.,
                384.,
                Some(chrome::Icon::Resources),
                name,
                false,
            );
        }
        sheet::above(c, 857., 80., "Project Leveling Priority", true);
        let mut priority = get(&s.project, "priority_num").to_string();
        let before = priority.clone();
        c.spin(
            "project_priority",
            857.,
            80.,
            164.,
            24.,
            &mut priority,
            1.,
            1.0..=100.,
            editable,
        );
        if editable && priority != before {
            if let Ok(v) = priority.trim().parse::<u32>() {
                self.set_project_field("priority_num", v.clamp(1, 100).to_string());
            }
        }
        // The check-out fields (a multi-user database lock, always empty in
        // a local workspace) are not shown; checkout_flag and the other
        // PROJECT columns still import, save and export unchanged.
        if let Some(url) = sheet::text_field(
            c,
            "project_field_proj_url",
            21.,
            127.,
            1000.,
            "Project Link (URL)",
            get(&s.project, "proj_url"),
            editable,
        ) {
            self.set_project_field("proj_url", url);
        }
        sheet::above(c, 21., 173., "Financial Period Calendar", true);
        let fin = self.project.document.table("FINTMPL").and_then(|t| {
            t.rows
                .iter()
                .find(|r| t.value(r, "fintmpl_id") == get(&s.project, "fintmpl_id"))
                .map(|r| t.value(r, "fintmpl_name").to_string())
        });
        sheet::picker(
            c,
            "project_fin_calendar",
            21.,
            173.,
            212.,
            Some(chrome::Icon::Options),
            fin.as_deref().unwrap_or(""),
            false,
        );
    }

    fn project_dates(
        &mut self,
        c: &mut Canvas,
        w: f32,
        h: f32,
        s: &ProjectSummary,
        editable: bool,
    ) {
        let right = w - 327.;
        let left_w = right - 13.;
        sheet::section(c, 5., 5., left_w, 132., "Schedule Dates");
        sheet::section(c, 5., 142., left_w, h - 142., "");
        sheet::section(c, right, 5., 322., h - 5., "Anticipated Dates");
        let origin = 0;
        let fallback = s.data_date.unwrap_or(0);
        let base = self.project.model.origin_serial * DAY;
        let rel = |v: Option<i64>| v.map(|v| v - base);
        let model_origin = self.project.model.origin_serial;
        sheet::above(c, 22., 52., "Project Planned Start", true);
        match sheet::date_field(
            c,
            "project_planned_start",
            22.,
            52.,
            model_origin,
            rel(s.planned_start),
            rel(Some(fallback)).unwrap_or(0),
            editable,
        ) {
            Ok(Some(v)) => {
                if let Err(e) = self.set_planned_start(v) {
                    self.status = e;
                }
            }
            Ok(None) => {}
            Err(e) => self.status = e,
        }
        sheet::above(c, 356., 52., "Must Finish By", true);
        match sheet::date_field(
            c,
            "project_must_finish",
            356.,
            52.,
            model_origin,
            rel(s.must_finish),
            rel(s.finish).unwrap_or(0),
            editable,
        ) {
            Ok(Some(v)) => self.set_must_finish(v),
            Ok(None) => {}
            Err(e) => self.status = e,
        }
        sheet::above(c, 22., 101., "Data Date", true);
        // Editable here as in the Schedule dialog; the change is one undo
        // step and schedules nothing until the next run.
        match sheet::date_field(
            c,
            "project_data_date",
            22.,
            101.,
            model_origin,
            rel(s.data_date),
            rel(Some(fallback)).unwrap_or(0),
            editable,
        ) {
            Ok(Some(v)) => {
                if let Err(e) = self.set_data_date(v) {
                    self.status = e;
                }
            }
            Ok(None) => {}
            Err(e) => self.status = e,
        }
        sheet::above(c, 356., 101., "Finish", true);
        sheet::value_box(
            c,
            "project_finish",
            356.,
            101.,
            217.,
            &format::date(origin, s.finish),
            false,
        );
        sheet::above(c, 22., 168., "Actual Start", true);
        sheet::value_box(
            c,
            "project_actual_start",
            22.,
            168.,
            217.,
            &format::date(origin, s.actual_start),
            false,
        );
        sheet::above(c, 356., 168., "Actual Finish", true);
        sheet::value_box(
            c,
            "project_actual_finish",
            356.,
            168.,
            217.,
            &format::date(origin, s.actual_finish),
            false,
        );
        for (y, caption, field) in [
            (53., "Anticipated Start", "anticip_start_date"),
            (101., "Anticipated Finish", "anticip_end_date"),
        ] {
            sheet::above(c, right + 17., y, caption, true);
            let value = rel(date(&s.node, field));
            let node = editable && project_node(&self.project).is_some();
            match sheet::date_field(
                c,
                &format!("project_{field}"),
                right + 17.,
                y,
                model_origin,
                value,
                fallback - base,
                node,
            ) {
                Ok(Some(v)) => {
                    self.set_node_field(field, openxer_xer::date_string(model_origin, v))
                }
                Ok(None) => {}
                Err(e) => self.status = e,
            }
        }
    }

    fn project_defaults(
        &mut self,
        c: &mut Canvas,
        w: f32,
        h: f32,
        s: &ProjectSummary,
        editable: bool,
    ) {
        sheet::section(c, 6., 11., w - 12., 115., "Defaults for New Activities");
        sheet::section(c, 6., 126., w - 12., h - 126., "Auto-numbering Defaults");
        for (y, caption, field, options) in [
            (
                43.,
                "Duration Type",
                "def_duration_type",
                &DURATION_TYPES[..],
            ),
            (
                70.,
                "Percent Complete Type",
                "def_complete_pct_type",
                &PERCENT_TYPES[..],
            ),
            (97., "Activity Type", "def_task_type", &ACTIVITY_TYPES[..]),
        ] {
            c.label(11., y, 22., caption, true);
            if let Some(code) = sheet::combo(
                c,
                &format!("project_{field}"),
                263.,
                y,
                225.,
                options,
                &effective(s, field),
                editable,
            ) {
                self.set_project_field(field, code);
            }
        }
        c.label(545., 40., 21., "Cost Account", true);
        let account = self.project.document.table("ACCOUNT").and_then(|t| {
            t.rows
                .iter()
                .find(|r| t.value(r, "acct_id") == get(&s.project, "acct_id"))
                .map(|r| t.value(r, "acct_name").to_string())
        });
        sheet::picker(
            c,
            "project_cost_account",
            713.,
            40.,
            307.,
            Some(chrome::Icon::Expenses),
            account.as_deref().unwrap_or(""),
            false,
        );
        c.label(545., 70., 21., "Calendar", true);
        let calendar = get(&s.project, "clndr_id").to_string();
        let name = if s.current {
            self.project
                .model
                .calendars
                .iter()
                .position(|cal| cal.id == calendar)
                .and_then(|i| self.project.calendar_names.get(i).cloned())
                .unwrap_or(calendar)
        } else {
            calendar
        };
        sheet::picker(
            c,
            "project_calendar",
            713.,
            70.,
            307.,
            Some(chrome::Icon::Options),
            &name,
            false,
        );
        let r = c.r(713., 70., 307., 21.);
        sheet::not_in_xer(
            c,
            "project_calendar_tip",
            r,
            "The project calendar drives scheduling; change it in Enterprise > Calendars.",
        );
        for (x, caption, field) in [
            (23., "Activity ID Prefix", "task_code_prefix"),
            (240., "Activity ID Suffix", "task_code_base"),
            (456., "Increment", "task_code_step"),
        ] {
            sheet::above(c, x, 172., caption, true);
            let key = format!("project_field_{field}");
            let shown = effective(s, field);
            let stored = get(&s.project, field).trim();
            let at = (x, 172., 170.);
            let typed = if field == "task_code_prefix" {
                sheet::commit_field_over(c, &key, at, (&shown, stored), editable, Align::Min)
            } else {
                sheet::number_field_over(c, &key, at, (&shown, stored), editable)
            };
            if let Some(text) = typed {
                let ok = field == "task_code_prefix" || text.trim().parse::<u64>().is_ok();
                if ok {
                    self.set_project_field(field, text.trim().to_string());
                } else {
                    self.status = format!("{caption} must be a whole number.");
                }
            }
        }
        let mut value = effective(s, "task_code_prefix_flag") == "Y";
        if c.checkbox(
            "project_increment_flag",
            23.,
            204.,
            788.,
            17.,
            "Increment Activity ID based on selected activity",
            &mut value,
            editable,
        )
        .clicked()
            && editable
        {
            self.set_project_field("task_code_prefix_flag", yn(value));
        }
    }

    fn project_resources_tab(
        &mut self,
        c: &mut Canvas,
        w: f32,
        h: f32,
        s: &ProjectSummary,
        editable: bool,
    ) {
        sheet::section(c, 14., 6., w - 20., h - 6., "Assignment Defaults");
        c.label(
            32.,
            44.,
            13.,
            "Specify the default Rate Type for new assignments",
            true,
        );
        if let Some(code) = sheet::combo(
            c,
            "project_rate_type",
            32.,
            72.,
            242.,
            &RATE_TYPES,
            &effective(s, "def_rate_type"),
            editable,
        ) {
            self.set_project_field("def_rate_type", code);
        }
        for (key, x, y, hh, caption, field) in [
            (
                "project_drive_dates",
                42.,
                107.,
                24.,
                "Drive activity dates by default",
                "def_rollup_dates_flag",
            ),
            (
                "project_multi_assign",
                30.,
                h - 38.,
                30.,
                "Resources can be assigned to the same activity more than once",
                "rsrc_multi_assign_flag",
            ),
        ] {
            if field == "rsrc_multi_assign_flag" {
                c.hline(15., h - 73., w - 22., SHADOW_LINE);
                c.bold(25., h - 72., 25., "Resource Assignments");
                c.hline(15., h - 48., w - 22., SHADOW_LINE);
            }
            let mut value = effective(s, field) == "Y";
            if c.checkbox(key, x, y, 400., hh, caption, &mut value, editable)
                .clicked()
                && editable
            {
                self.set_project_field(field, yn(value));
            }
        }
    }

    fn project_settings(
        &mut self,
        c: &mut Canvas,
        w: f32,
        h: f32,
        s: &ProjectSummary,
        editable: bool,
    ) {
        sheet::section(c, 5., 5., 262., h - 5., "Summarized Data");
        sheet::section(c, 268., 5., w - 273., 145., "Project Settings");
        sheet::section(
            c,
            268.,
            150.,
            w - 273.,
            h - 150.,
            "Define Critical Activities",
        );
        sheet::above(c, 12., 54., "Last Summarized On", true);
        sheet::value_box(
            c,
            "project_last_summarized",
            12.,
            54.,
            121.,
            &format::date(0, date(&s.project, "last_tasksum_date")),
            false,
        );
        sheet::above(c, 12., 105., "Summarize to WBS Level", true);
        let mut level = effective(s, "wbs_max_sum_level");
        let before = level.clone();
        c.spin(
            "project_sum_level",
            12.,
            105.,
            121.,
            24.,
            &mut level,
            1.,
            1.0..=99.,
            editable,
        );
        if editable && level != before {
            if let Ok(v) = level.trim().parse::<u32>() {
                self.set_project_field("wbs_max_sum_level", v.clamp(1, 99).to_string());
            }
        }
        c.label(
            280.,
            31.,
            21.,
            "Character for separating code fields for the WBS tree",
            true,
        );
        let sep = effective(s, "name_sep_char");
        let stored = get(&s.project, "name_sep_char");
        if let Some(sep) = sheet::commit_field_over(
            c,
            "project_field_name_sep_char",
            (w - 654., 31., 82.),
            (&sep, stored),
            editable,
            Align::Min,
        ) {
            if sep.chars().count() == 1 {
                self.set_project_field("name_sep_char", sep);
            } else {
                self.status = "The WBS code separator is one character.".into();
            }
        }
        c.label(280., 61., 22., "Fiscal year begins on the 1st day of", true);
        let month = get(&s.project, "fy_start_month_num");
        let month = if month.is_empty() { "1" } else { month };
        if let Some(code) = sheet::combo(
            c,
            "project_fiscal_month",
            w - 838.,
            61.,
            265.,
            &MONTHS,
            month,
            editable,
        ) {
            self.set_project_field("fy_start_month_num", code);
        }
        c.label(
            280.,
            86.,
            13.,
            "Baseline for earned value calculations",
            true,
        );
        let project_baseline = get(&s.project, "use_project_baseline_flag") != "N";
        for (key, x, value, caption) in [
            ("project_ev_project", 288., true, "Project baseline"),
            ("project_ev_user", 788., false, "User's primary baseline"),
        ] {
            if c.radio(
                key,
                x,
                115.,
                300.,
                14.,
                caption,
                project_baseline == value,
                editable,
            ) && editable
            {
                self.set_project_field("use_project_baseline_flag", yn(value));
            }
        }
        // The same PROJECT columns Schedule Options writes and
        // `xer::critical::critical_definition` reads for the table, bars,
        // filters and reports.
        let longest = get(&s.project, "critical_path_type") == "CT_DrivPath";
        for (key, y, value, caption) in [
            (
                "project_critical_float",
                177.,
                false,
                "Total Float less than or equal to",
            ),
            ("project_critical_longest", 200., true, "Longest Path"),
        ] {
            if c.radio(key, 288., y, 400., 23., caption, longest == value, editable)
                && editable
                && longest != value
            {
                let code = if value { "CT_DrivPath" } else { "CT_TotFloat" };
                self.set_project_field("critical_path_type", code.into());
            }
        }
        let hours = records::float(get(&s.project, "critical_drtn_hr_cnt"));
        let periods = if s.current {
            format::project_periods(&self.project)
        } else {
            format::Periods::DEFAULT
        };
        if let Some(text) = sheet::number_field(
            c,
            "project_critical_float_value",
            w - 717.,
            175.,
            137.,
            &format::duration((hours * 60.).round() as i64, periods),
            editable,
        ) {
            match format::parse_duration(&text, periods) {
                Ok(minutes) => {
                    let hours = crate::dialog_kit::format_number(minutes as f64 / 60.);
                    self.set_project_field("critical_drtn_hr_cnt", hours);
                }
                Err(_) => {
                    let example = format::duration(0, periods);
                    self.status = format!("Enter the critical float, for example {example}.");
                }
            }
        }
    }

    fn project_calculations(
        &mut self,
        c: &mut Canvas,
        w: f32,
        h: f32,
        s: &ProjectSummary,
        editable: bool,
    ) {
        sheet::section(c, 5., 5., 521., h - 5., "Activities");
        sheet::section(c, 534., 5., w - 539., h - 5., "Resource Assignments");
        c.label(
            10.,
            40.,
            13.,
            "Default Price / Unit for activities without",
            true,
        );
        c.label(10., 53., 13., "resource or role Price / Units", true);
        let price = format::cost(records::float(get(&s.project, "def_cost_per_qty")));
        if let Some(text) = sheet::commit_field(
            c,
            "project_default_price",
            228.,
            43.,
            84.,
            &format!("{price}/h"),
            editable,
            Align::Max,
        ) {
            let clean: String = text
                .chars()
                .filter(|ch| ch.is_ascii_digit() || *ch == '.')
                .collect();
            match clean.parse::<f64>() {
                Ok(v) if v >= 0. => self.set_project_field("def_cost_per_qty", format!("{v:.4}")),
                _ => self.status = "Enter a price per hour, for example 45.00.".into(),
            }
        }
        let mut steps = flag(&s.project, "step_complete_flag");
        if c.checkbox(
            "project_step_complete",
            10.,
            86.,
            500.,
            24.,
            "Activity percent complete based on activity steps",
            &mut steps,
            editable,
        )
        .clicked()
            && editable
        {
            self.set_project_field("step_complete_flag", yn(steps));
        }
        let mut linked = flag(&s.project, "rem_target_link_flag");
        c.checkbox(
            "project_link_budget",
            10.,
            107.,
            500.,
            41.,
            "Link Budget and At Completion for not started activities",
            &mut linked,
            false,
        );
        let r = c.r(10., 107., 500., 41.);
        sheet::not_in_xer(
            c,
            "project_link_budget_tip",
            r,
            "Applied when the XER file is read, so it is shown read-only.",
        );
        let reset = flag(&s.project, "reset_planned_flag");
        for (key, y, value, caption) in [
            (
                "project_reset_original",
                155.,
                true,
                "Reset Original Duration and Units to Remaining",
            ),
            (
                "project_reset_remaining",
                173.,
                false,
                "Reset Remaining Duration and Units to Original",
            ),
        ] {
            if c.radio(key, 23., y, 480., 18., caption, reset == value, editable) && editable {
                self.set_project_field("reset_planned_flag", yn(value));
            }
        }
        c.label(540., 40., 13., "When updating Actual Units or Cost", true);
        let add = flag(&s.project, "add_act_remain_flag");
        for (key, y, value, caption) in [
            ("project_add_actual", 68., true, "Add Actual to Remaining"),
            (
                "project_subtract_actual",
                86.,
                false,
                "Subtract Actual from At Completion",
            ),
        ] {
            if c.radio(key, 548., y, 300., 18., caption, add == value, editable) && editable {
                self.set_project_field("add_act_remain_flag", yn(value));
            }
        }
        for (key, y, caption, field) in [
            (
                "project_recalc_actual",
                108.,
                "Recalculate Actual Units and Cost when duration % complete changes",
                "act_pct_link_flag",
            ),
            (
                "project_update_units",
                142.,
                "Update units when costs change on resource assignments",
                "cost_qty_recalc_flag",
            ),
            (
                "project_link_actual",
                176.,
                "Link actual to date and actual this period units and costs",
                "act_this_per_link_flag",
            ),
        ] {
            let mut value = flag(&s.project, field);
            if c.checkbox(key, 537., y, 600., 34., caption, &mut value, editable)
                .clicked()
                && editable
            {
                self.set_project_field(field, yn(value));
            }
        }
    }

    /// The Notebook tab: the project's notebook topics (on its own WBS
    /// node) in the notebook pane every Notebook tab shares.
    fn project_notebook(&mut self, c: &mut Canvas, w: f32, h: f32, s: &ProjectSummary) {
        let node = num(get(&s.node, "wbs_id"));
        let owner = (s.current && node != 0).then_some(Owner::Project(node));
        let content = c.r(2., 2., w - 4., h - 4.);
        let topic = Some(self.views.projects.topic);
        let out = self.notebook_pane(
            c.ui,
            content,
            "project_notebook",
            "project_notebook",
            owner,
            topic,
            owner.is_some(),
        );
        if let Some(r) = out.select {
            self.views.projects.topic = r;
        }
        if out.cleared {
            self.views.projects.topic = 0;
        }
        if let (true, Some(owner)) = (out.add, owner) {
            self.open_topic_picker(owner);
        }
    }

    /// The Codes tab: the project's code values, Assign (Assign Project
    /// Codes) and Remove (asks first), each change one undo step.
    fn project_codes_tab(&mut self, c: &mut Canvas, w: f32, h: f32, editable: bool) {
        let pid = self.project.model.project.proj_id;
        let assigned = if editable {
            project_codes::assigned(&self.project.document, pid)
        } else {
            vec![]
        };
        let rows: Vec<Vec<String>> = assigned
            .iter()
            .map(|a| vec![a.code_type.clone(), a.value.clone(), a.description.clone()])
            .collect();
        let cols = [
            ("Project Code", 100., false),
            ("Code Value", 100., false),
            ("Code Description", 200., false),
        ];
        let selected = self.views.projects.code_row.filter(|&r| r < rows.len());
        let (x, y, tw, th) = (12., 12., w - 24., h - 44.);
        if let Some(r) = sheet::table(c, "project_codes", x, y, tw, th, &cols, &rows, selected) {
            self.views.projects.code_row = Some(r);
        }
        let bar = c.r(11., h - 27., (w - 22.).max(200.), 25.);
        let ui = &mut crate::details::area(c.ui, bar);
        let (assign, remove) = ui
            .horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(crate::details::form::GAP, 0.);
                let assign = crate::details::form::add_button(ui, "Assign", editable);
                let enabled = editable && selected.is_some();
                let remove = crate::details::form::command_button(ui, "Remove", None, enabled);
                (assign, remove)
            })
            .inner;
        self.action("project_codes_assign", &assign);
        self.action("project_codes_remove", &remove);
        if assign.clicked() && editable {
            self.open_project_code_picker();
        }
        if let (true, Some(r)) = (remove.clicked(), selected) {
            self.ask_remove_project_code(assigned[r].value_id);
        }
    }
}

const SHADOW_LINE: Color32 = Color32::from_rgb(160, 160, 160);

/// One budget group: left edge, width, caption and its (label, value) fields.
type Group<'a> = (f32, f32, &'a str, Vec<(&'a str, String)>);

/// The figures of the Budget Summary tab.
pub struct SummaryFigures {
    pub current: f64,
    pub spend: f64,
    pub benefit: f64,
}

/// Budget, Variance, Spending Plan and Benefit Plan groups side by side.
pub fn budget_summary(c: &mut Canvas, w: f32, h: f32, f: &SummaryFigures) {
    let current = format::cost(f.current);
    let zero = format::cost(0.);
    let groups: [Group; 4] = [
        (
            5.,
            293.,
            "Budget",
            vec![
                ("Current Budget", current.clone()),
                ("Unallocated Budget", zero.clone()),
                ("Distributed Current Budget", current),
            ],
        ),
        (
            306.,
            208.,
            "Variance",
            vec![("Current Variance", zero.clone())],
        ),
        (
            522.,
            302.,
            "Spending Plan",
            vec![
                ("Total Spending Plan", format::cost(f.spend)),
                (
                    "Undistributed Current Variance",
                    format::cost(f.current - f.spend),
                ),
                ("Total Spending Plan Tally", format::cost(f.spend)),
            ],
        ),
        (
            832.,
            (w - 837.).max(200.),
            "Benefit Plan",
            vec![
                ("Total Benefit Plan", format::cost(f.benefit)),
                ("Total Benefit Plan Tally", format::cost(f.benefit)),
            ],
        ),
    ];
    for (x, gw, caption, fields) in groups {
        sheet::section(c, x, 5., gw, h - 5., caption);
        for (i, (label, value)) in fields.iter().enumerate() {
            let y = 53. + 48. * i as f32;
            // The first group's fields sit 5 px in, the others 7 px.
            let fx = if x < 10. { x + 5. } else { x + 7. };
            sheet::above(c, fx, y, label, true);
            sheet::value_box(
                c,
                &format!("budget_{caption}_{i}"),
                fx,
                y,
                166.,
                value,
                true,
            );
        }
    }
}

impl App {
    /// The View menu's Table Font and Row, Expand All, Collapse All and
    /// Collapse To in the WBS and Projects windows (`table_font`,
    /// `expand_all`, `collapse_all`, `collapse_to`). False in other views.
    pub(crate) fn wbs_projects_view_command(&mut self, key: &str) -> bool {
        match self.views.main {
            super::MainView::Wbs => self.wbs_menu_command(key),
            super::MainView::Projects => {
                let s = &mut self.views.projects;
                match key {
                    "table_font" => {
                        self.open_table_font_for(crate::layout_opts::TableTarget::Projects);
                    }
                    "collapse_to" => {
                        self.open_collapse_to_for(crate::layout_opts::CollapseTarget::Projects);
                    }
                    "expand_all" => s.collapsed.clear(),
                    "collapse_all" => s.collapsed = s.bands.iter().map(|b| b.0).collect(),
                    _ => {}
                }
            }
            _ => return false,
        }
        true
    }

    /// A Filter By or Group and Sort By choice of the Projects menu.
    fn projects_filter_group(&mut self, key: &str) {
        let s = &mut self.views.projects;
        if let Some(g) = Grouping::of_menu(key) {
            s.group = g;
            s.collapsed.clear();
            return;
        }
        match key {
            "filter_all" => {
                s.open_only = false;
                s.status_filter.clear();
            }
            "filter_open" => {
                s.open_only = true;
                s.status_filter.clear();
            }
            "filter_non_whatif" => {
                s.open_only = false;
                s.status_filter = STATUSES
                    .iter()
                    .map(|(k, _)| k.to_string())
                    .filter(|k| k != "WS_Whatif")
                    .collect();
            }
            _ => {
                if let Some((_, _, code)) = STATUS_FILTERS.iter().find(|f| f.0 == key) {
                    s.open_only = false;
                    s.status_filter = BTreeSet::from([code.to_string()]);
                }
            }
        }
    }

    /// Collapse To's levels in the Projects window: one per band level as
    /// last drawn (EPS levels, or the one level of another grouping).
    pub(crate) fn projects_outline_levels(&self) -> Vec<String> {
        let depth = self.views.projects.bands.iter().map(|b| b.1 + 1).max();
        let name = if self.views.projects.group == Grouping::Eps {
            "EPS Level"
        } else {
            "Level"
        };
        (1..=depth.unwrap_or(0))
            .map(|n| format!("{name} {n}"))
            .collect()
    }

    /// Show band levels 1 to `level` of the Projects table with the bands
    /// of `level` and below collapsed.
    pub(crate) fn projects_collapse_to(&mut self, level: usize) {
        let s = &mut self.views.projects;
        s.collapsed = s
            .bands
            .iter()
            .filter(|b| b.1 + 1 >= level.max(1))
            .map(|b| b.0)
            .collect();
    }
}

/// The Projects table fields of the open project that are typed or picked
/// in the table: name, status, the two priorities, the responsible manager
/// and the anticipated dates.
pub const EDITED_FIELDS: [&str; 7] = [
    "project_name",
    "project_status",
    "strategic_priority",
    "leveling_priority",
    "responsible_manager",
    "anticipated_start",
    "anticipated_finish",
];

/// The editor of Projects table field `field` on the open project's row;
/// `obs` lists the OBS elements (id, name) the manager is picked from.
pub fn project_cell_editor(field: usize, obs: &[(String, String)]) -> Option<grid::EditKind> {
    let id = *rollups::IDS.get(field)?;
    match id {
        "project_status" => Some(grid::EditKind::Choice(
            STATUSES.iter().map(|(_, v)| v.to_string()).collect(),
        )),
        "responsible_manager" if obs.is_empty() => None,
        "responsible_manager" => Some(grid::EditKind::Choice(
            obs.iter().map(|(_, name)| name.clone()).collect(),
        )),
        _ if EDITED_FIELDS.contains(&id) => Some(grid::EditKind::Text),
        _ => None,
    }
}

impl App {
    /// Set the open project's Projects table field `id` (one of
    /// [`EDITED_FIELDS`]) from the text typed or picked in its cell,
    /// through the shared edit layer. Returns whether it changed; the
    /// caller makes the undo step.
    pub(crate) fn set_project_cell(&mut self, id: &str, text: &str) -> Result<bool, String> {
        use openxer_xer::edit::tables::{self, serde_like::Value};
        let text = text.trim();
        let p = &mut self.project;
        let node_id = project_node(p);
        let node = || node_id.ok_or("The project has no project node.");
        let changed = match id {
            "project_name" => openxer_xer::edit::wbs::set_project_name(p, text)?,
            "project_status" => {
                let code = STATUSES
                    .iter()
                    .find(|(k, v)| v.eq_ignore_ascii_case(text) || k.eq_ignore_ascii_case(text))
                    .map(|(k, _)| k.to_string())
                    .ok_or_else(|| format!("{text:?} is not a project status."))?;
                let node = node()?;
                openxer_xer::edit::wbs::set_column(p, node, "status_code", &code)?
            }
            "responsible_manager" => {
                let obs = if text.is_empty() {
                    String::new()
                } else {
                    super::wbs::obs_names(p)
                        .into_iter()
                        .find(|(_, name)| name == text)
                        .map(|(k, _)| k)
                        .ok_or_else(|| format!("{text:?} is not in the OBS."))?
                };
                let node = node()?;
                openxer_xer::edit::wbs::set_column(p, node, "obs_id", &obs)?
            }
            "anticipated_start" | "anticipated_finish" => {
                let column = if id == "anticipated_start" {
                    "anticip_start_date"
                } else {
                    "anticip_end_date"
                };
                let origin = p.model.origin_serial;
                let value = if text.is_empty() {
                    String::new()
                } else {
                    match crate::grid::columns::parse_input(
                        p,
                        None,
                        crate::grid::columns::Kind::Date,
                        text,
                    )? {
                        crate::grid::columns::Value::Date { minute, .. } => {
                            openxer_xer::date_string(origin, Some(minute))
                        }
                        _ => String::new(),
                    }
                };
                let node = node()?;
                openxer_xer::edit::wbs::set_column(p, node, column, &value)?
            }
            "strategic_priority" | "leveling_priority" => {
                let (field, range) = if id == "leveling_priority" {
                    ("priority_num", 1..=100)
                } else {
                    ("strgy_priority_num", 1..=10000)
                };
                let n: i64 = text
                    .parse()
                    .ok()
                    .filter(|n| range.contains(n))
                    .ok_or_else(|| {
                        format!(
                            "Enter a whole number from {} to {}.",
                            range.start(),
                            range.end()
                        )
                    })?;
                let pid = p.model.project.proj_id;
                let now = p.document.table("PROJECT").and_then(|t| {
                    t.rows
                        .iter()
                        .find(|r| num(t.value(r, "proj_id")) == pid)
                        .map(|r| t.value(r, field).to_string())
                });
                if now.as_deref() == Some(n.to_string().as_str()) {
                    false
                } else {
                    tables::set_project_field(p, field, &Value::Text(n.to_string()))?;
                    true
                }
            }
            _ => return Err("That column cannot be typed in here.".into()),
        };
        if changed {
            self.touch_records();
        }
        Ok(changed)
    }

    /// A Projects table cell typed or picked on the open project's row:
    /// one undo step when the value changed; a value the field refuses is
    /// reported and changes nothing.
    fn commit_project_cell(&mut self, id: &str, text: &str, time: f64) {
        if !EDITED_FIELDS.contains(&id) {
            return;
        }
        let before = self.project.clone();
        let before_revision = self.records_revision;
        match self.set_project_cell(id, text) {
            Ok(true) => self.edit_step(before, before_revision, time),
            Ok(false) => {}
            Err(e) => {
                self.project = before;
                self.error(e);
            }
        }
    }
}
/// A Projects row as the multi-row selection keys it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProjectRow {
    Eps(i64),
    /// Source name and project key.
    Project(String, i64),
}

/// A row of the Projects view: an EPS node or a project (workspace ID),
/// each with its indent level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EpsLine {
    Eps(i64, usize),
    Project(i64, usize),
}

/// The Projects view rows from the EPS groups (nodes in tree order with
/// their own projects): each node, then its child nodes, then its projects.
/// Rows under a collapsed node are left out.
pub fn eps_lines(groups: &[(i64, usize, Vec<i64>)], collapsed: &BTreeSet<i64>) -> Vec<EpsLine> {
    let mut lines = vec![];
    // Open nodes: depth, projects, whether their children are hidden.
    let mut stack: Vec<(usize, &[i64], bool)> = vec![];
    let flush = |lines: &mut Vec<EpsLine>, (depth, projects, hidden): (usize, &[i64], bool)| {
        if !hidden {
            lines.extend(projects.iter().map(|p| EpsLine::Project(*p, depth + 1)));
        }
    };
    for (id, depth, projects) in groups {
        while stack.last().is_some_and(|(d, _, _)| d >= depth) {
            flush(&mut lines, stack.pop().unwrap());
        }
        let hidden = stack.last().is_some_and(|(_, _, h)| *h);
        if !hidden {
            lines.push(EpsLine::Eps(*id, *depth));
        }
        stack.push((*depth, projects, hidden || collapsed.contains(id)));
    }
    while let Some(entry) = stack.pop() {
        flush(&mut lines, entry);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_and_live_summaries_agree_and_roll_up() {
        let mut p = openxer_xer::starter();
        scheduler::schedule_with_resources(&mut p.model, &p.resources).unwrap();
        let live = summarize_live(&p);
        let stored = summarize(&Document::parse(&p.updated_document().text()).unwrap(), 1).unwrap();
        assert_eq!(live.activities, 14);
        assert_eq!(stored.activities, 14);
        assert_eq!(live.id, "RIVERBEND");
        assert_eq!(
            (stored.id.as_str(), stored.name.as_str()),
            ("RIVERBEND", "RIVERBEND")
        );
        assert_eq!(live.start, stored.start);
        assert_eq!(live.finish, stored.finish);
        assert_eq!(live.data_date, stored.data_date);
        assert_eq!(openxer_xer::short_date(0, live.data_date), "02-Nov-26");
        assert!(live.finish > live.start);
        assert_eq!(live.actual_start, None);
        let mut other = stored.clone();
        other.activities = 6;
        other.start = live.start.map(|s| s - DAY);
        other.finish = live.finish.map(|f| f + DAY);
        let eps = eps_rollup(&[live.clone(), other.clone()]);
        assert_eq!(eps.activities, 20);
        assert_eq!(eps.start, other.start);
        assert_eq!(eps.finish, other.finish);
        assert!(summarize(&Document::parse(&p.updated_document().text()).unwrap(), 99).is_none());
    }

    #[test]
    fn project_node_supplies_the_project_name() {
        let p = super::super::wbs::tests::with_project_node();
        assert_eq!(summarize_live(&p).name, "Riverbend Plant");
        let stored = summarize(&p.updated_document(), 1).unwrap();
        assert_eq!(stored.name, "Riverbend Plant");
        assert_eq!(label(&STATUSES, "WS_Open"), "Active");
    }

    #[test]
    fn workspace_list_marks_the_open_project_and_reads_closed_ones() {
        let path = std::env::temp_dir().join(format!(
            "openxer-projects-view-{}-{}.db",
            std::process::id(),
            line!()
        ));
        {
            let mut store = Store::open(path.clone()).unwrap();
            let mut other = openxer_xer::starter();
            other.source_name = "Second".into();
            other.model.project.name = "SECOND".into();
            store.save(&other).unwrap();
            let project = openxer_xer::starter();
            store.save(&project).unwrap();
            let ctx = egui::Context::default();
            let mut app = App::new(&ctx, store, project);
            app.project.model.project.name = "LIVE-EDIT".into();
            app.dirty = true;
            let list = app.workspace_projects();
            assert_eq!(list.len(), 2);
            let open = list.iter().find(|s| s.current).unwrap();
            assert_eq!(open.id, "LIVE-EDIT", "the open project shows unsaved edits");
            let closed = list.iter().find(|s| !s.current).unwrap();
            assert_eq!((closed.id.as_str(), closed.activities), ("SECOND", 14));
            let closed = closed.clone();
            app.open_workspace_project(&closed);
            assert_eq!(app.project.model.project.name, "SECOND");
            assert_eq!(app.views.main, MainView::Activities);
            let list = app.workspace_projects();
            assert_eq!(list.iter().find(|s| s.current).unwrap().id, "SECOND");
            assert!(
                list.iter().any(|s| s.id == "LIVE-EDIT" && !s.current),
                "the previous project was saved"
            );
        }
        let _ = std::fs::remove_file(path);
    }
}
